//! A single worker serializes inference and owns the child's entire lifetime.
use crate::{
    integrity::{verify_model_checked, RuntimeBundle},
    wire,
};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Validated 16 kHz mono PCM. This service never captures microphone input.
#[derive(Clone)]
pub struct Audio16k(Vec<f32>);
impl Audio16k {
    pub fn new(samples: Vec<f32>) -> Result<Self> {
        ensure!(
            !samples.is_empty() && samples.len() <= 16_000 * 120,
            "Record between one sample and 120 seconds"
        );
        ensure!(
            samples.iter().all(|sample| sample.is_finite()),
            "Recording contains non-finite samples"
        );
        Ok(Self(samples))
    }
    fn wav(&self) -> Result<Vec<u8>> {
        let mut buffer = std::io::Cursor::new(Vec::with_capacity(self.0.len() * 2 + 44));
        let specification = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        {
            let mut output = hound::WavWriter::new(&mut buffer, specification)?;
            for sample in &self.0 {
                output.write_sample((sample.clamp(-1.0, 1.0) * 32767.0).round() as i16)?;
            }
            output.finalize()?;
        }
        Ok(buffer.into_inner())
    }
}

#[derive(Clone, Copy)]
pub enum WarmPolicy {
    KeepLoaded,
    IdleTimeout(Duration),
    ReleaseAfterRequest,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Stopped,
    Preparing,
    Ready,
    Transcribing,
    Failed,
    Closed,
}
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub phase: Phase,
    pub error: Option<String>,
}

struct Control {
    epoch: AtomicU64,
    closed: AtomicBool,
    busy: AtomicBool,
    state: Mutex<Snapshot>,
}
impl Control {
    fn state(&self, phase: Phase, error: Option<String>) {
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = Snapshot { phase, error };
    }
    fn check(&self, epoch: u64, cancellation: &Cancellation) -> Result<()> {
        ensure!(
            !self.closed.load(Ordering::Acquire)
                && self.epoch.load(Ordering::Acquire) == epoch
                && !cancellation.is_cancelled(),
            "Speech operation cancelled"
        );
        Ok(())
    }
}

enum Operation {
    Warm,
    Recognize(Audio16k, Vec<String>),
}
enum Message {
    Run {
        operation: Operation,
        cancellation: Cancellation,
        epoch: u64,
        reply: mpsc::Sender<Result<Option<String>>>,
    },
    Stop(mpsc::Sender<()>),
    Shutdown,
}
struct Inner {
    tx: mpsc::Sender<Message>,
    control: Arc<Control>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}
impl Inner {
    fn close(&self) {
        self.control.closed.store(true, Ordering::Release);
        self.control.epoch.fetch_add(1, Ordering::AcqRel);
        let _ = self.tx.send(Message::Shutdown);
        if let Some(worker) = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = worker.join();
        }
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        self.close();
    }
}

/// Cloneable handle. Blocking methods belong on the host UI's background worker.
#[derive(Clone)]
pub struct PhononService(Arc<Inner>);
impl PhononService {
    pub fn spawn(
        bundle: RuntimeBundle,
        model_directory: PathBuf,
        policy: WarmPolicy,
    ) -> Result<Self> {
        if let WarmPolicy::IdleTimeout(duration) = policy {
            ensure!(!duration.is_zero(), "Idle timeout must be positive");
        }
        ensure!(
            std::fs::symlink_metadata(&model_directory)?.is_dir(),
            "Model root must be a regular directory"
        );
        let model_directory = model_directory
            .canonicalize()
            .context("Cannot resolve model directory")?;
        let (tx, rx) = mpsc::channel();
        let control = Arc::new(Control {
            epoch: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            state: Mutex::new(Snapshot {
                phase: Phase::Stopped,
                error: None,
            }),
        });
        let shared = control.clone();
        let worker = thread::Builder::new()
            .name("local-speech".into())
            .spawn(move || run_worker(rx, shared, bundle, model_directory, policy))?;
        Ok(Self(Arc::new(Inner {
            tx,
            control,
            worker: Mutex::new(Some(worker)),
        })))
    }
    pub fn snapshot(&self) -> Snapshot {
        self.0
            .control
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    fn submit(&self, operation: Operation, cancellation: &Cancellation) -> Result<Option<String>> {
        let epoch = self.0.control.epoch.load(Ordering::Acquire);
        self.0.control.check(epoch, cancellation)?;
        // Do not accumulate unbounded recordings while an inference is running.
        ensure!(
            self.0
                .control
                .busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok(),
            "Speech service is busy"
        );
        struct BusyGuard<'a>(&'a AtomicBool);
        impl Drop for BusyGuard<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let _busy = BusyGuard(&self.0.control.busy);
        let (reply, result) = mpsc::channel();
        self.0
            .tx
            .send(Message::Run {
                operation,
                cancellation: cancellation.clone(),
                epoch,
                reply,
            })
            .context("Speech service is closed")?;
        result.recv().context("Speech worker stopped")?
    }
    pub fn warm(&self, cancellation: &Cancellation) -> Result<()> {
        self.submit(Operation::Warm, cancellation).map(|_| ())
    }
    pub fn transcribe(
        &self,
        audio: Audio16k,
        vocabulary: &[String],
        cancellation: &Cancellation,
    ) -> Result<String> {
        self.submit(
            Operation::Recognize(audio, vocabulary_terms(vocabulary)),
            cancellation,
        )?
        .context("Speech worker returned no result")
    }
    /// Invalidates queued/in-flight work, then waits for the owned child to exit.
    pub fn stop(&self) -> Result<()> {
        self.0.control.epoch.fetch_add(1, Ordering::AcqRel);
        let (reply, result) = mpsc::channel();
        self.0
            .tx
            .send(Message::Stop(reply))
            .context("Speech service is closed")?;
        result.recv().context("Speech worker stopped")?;
        Ok(())
    }
    pub fn shutdown(&self) {
        self.0.close();
    }
}

fn vocabulary_terms(input: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    let mut unique = std::collections::HashSet::new();
    for value in input {
        let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
        if !value.is_empty()
            && value.chars().count() <= 128
            && !value.chars().any(char::is_control)
            && unique.insert(value.to_lowercase())
        {
            result.push(value);
            if result.len() == 25 {
                break;
            }
        }
    }
    result
}

struct OwnedProcess {
    child: Child,
    port: u16,
    secret: String,
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Deserialize)]
struct Announcement {
    port: u16,
    protocol: u32,
}

fn launch(
    bundle: &RuntimeBundle,
    model_directory: &std::path::Path,
    check: &impl Fn() -> Result<()>,
) -> Result<OwnedProcess> {
    check()?;
    bundle.verify_checked(check)?;
    check()?;
    verify_model_checked(model_directory, check)?;
    check()?;
    let secret = uuid::Uuid::new_v4().to_string();
    let mut command = Command::new(bundle.interpreter());
    command
        .args(["-I", "-B"])
        .arg(bundle.bridge())
        .current_dir(bundle.root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // Do not even pass provider secrets/proxy/native-library overrides into Python.
    command.env_clear();
    for name in [
        "PATH",
        "SYSTEMROOT",
        "WINDIR",
        "SYSTEMDRIVE",
        "COMSPEC",
        "HOME",
        "USERPROFILE",
        "TEMP",
        "TMP",
        "TMPDIR",
        "APPDATA",
        "LOCALAPPDATA",
        "PROGRAMDATA",
        "PROGRAMFILES",
        "PROGRAMFILES(X86)",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut process = OwnedProcess {
        child: command
            .spawn()
            .context("Cannot launch the verified speech bundle")?,
        port: 0,
        secret,
    };
    let configuration = serde_json::to_vec(
        &serde_json::json!({"secret":process.secret,"owner":std::process::id(),"model_directory":model_directory}),
    )?;
    ensure!(
        configuration.len() < 8192,
        "Speech launch configuration is too large"
    );
    if let Some(mut input) = process.child.stdin.take() {
        input.write_all(&configuration)?;
        input.write_all(b"\n")?;
    }
    let mut output = process
        .child
        .stdout
        .take()
        .context("Missing owned announcement pipe")?;
    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || {
        let result = (|| -> Result<Announcement> {
            let mut line = Vec::new();
            for _ in 0..512 {
                let mut byte = [0];
                ensure!(
                    output.read(&mut byte)? == 1,
                    "Speech child exited before readiness"
                );
                if byte[0] == b'\n' {
                    return serde_json::from_slice(&line)
                        .context("Invalid speech port announcement");
                }
                line.push(byte[0]);
            }
            bail!("Speech port announcement exceeded its limit")
        })();
        let _ = sender.send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(180);
    let outcome = (|| -> Result<()> {
        loop {
            check()?;
            ensure!(
                Instant::now() < deadline,
                "Speech model did not load within three minutes"
            );
            ensure!(
                process.child.try_wait()?.is_none(),
                "Speech process exited during startup"
            );
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(announcement) => {
                    let announcement = announcement?;
                    ensure!(
                        announcement.protocol == 1 && announcement.port != 0,
                        "Unsupported speech announcement"
                    );
                    process.port = announcement.port;
                    wire::ready(process.port, &process.secret, check)?;
                    check()?;
                    return Ok(());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => bail!("Speech process announcement was lost"),
            }
        }
    })();
    if outcome.is_err() {
        let _ = process.child.kill();
        let _ = process.child.wait();
    }
    let _ = reader.join();
    outcome?;
    Ok(process)
}

fn run_worker(
    receiver: mpsc::Receiver<Message>,
    control: Arc<Control>,
    bundle: RuntimeBundle,
    model_directory: PathBuf,
    policy: WarmPolicy,
) {
    let mut process: Option<OwnedProcess> = None;
    let mut last_request = Instant::now();
    loop {
        if let Some(child) = process.as_mut() {
            if !matches!(child.child.try_wait(), Ok(None)) {
                process = None;
                control.state(
                    Phase::Failed,
                    Some("Speech process exited unexpectedly".into()),
                );
            }
        }
        let message = if process.is_some() {
            let interval = match policy {
                WarmPolicy::IdleTimeout(timeout) => timeout
                    .saturating_sub(last_request.elapsed())
                    .min(Duration::from_secs(1)),
                _ => Duration::from_secs(1),
            };
            match receiver.recv_timeout(interval) {
                Ok(message) => message,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if matches!(policy, WarmPolicy::IdleTimeout(timeout) if last_request.elapsed() >= timeout)
                    {
                        process = None;
                        control.state(Phase::Stopped, None);
                    }
                    continue;
                }
                Err(_) => break,
            }
        } else {
            match receiver.recv() {
                Ok(message) => message,
                Err(_) => break,
            }
        };
        match message {
            Message::Shutdown => break,
            Message::Stop(reply) => {
                process = None;
                control.state(Phase::Stopped, None);
                let _ = reply.send(());
            }
            Message::Run {
                operation,
                cancellation,
                epoch,
                reply,
            } => {
                let check = || control.check(epoch, &cancellation);
                let result = (|| -> Result<Option<String>> {
                    check()?;
                    if let Some(child) = process.as_mut() {
                        if child.child.try_wait()?.is_some() {
                            process = None;
                        }
                    }
                    if process.is_none() {
                        control.state(Phase::Preparing, None);
                        process = Some(launch(&bundle, &model_directory, &check)?);
                    }
                    let child = process.as_ref().context("Speech process is unavailable")?;
                    match operation {
                        Operation::Warm => {
                            wire::ready(child.port, &child.secret, &check)?;
                            check()?;
                            Ok(None)
                        }
                        Operation::Recognize(audio, terms) => {
                            control.state(Phase::Transcribing, None);
                            let text = wire::transcribe(
                                child.port,
                                &child.secret,
                                &audio.wav()?,
                                &terms,
                                &check,
                            )?;
                            check()?;
                            Ok(Some(text))
                        }
                    }
                })();
                match &result {
                    Ok(_) => {
                        last_request = Instant::now();
                        if matches!(policy, WarmPolicy::ReleaseAfterRequest) {
                            process = None;
                            control.state(Phase::Stopped, None);
                        } else {
                            control.state(Phase::Ready, None);
                        }
                    }
                    Err(error) => {
                        process = None;
                        if check().is_err() {
                            control.state(Phase::Stopped, None);
                        } else {
                            control.state(Phase::Failed, Some(error.to_string()));
                        }
                    }
                }
                let _ = reply.send(result);
            }
        }
    }
    drop(process);
    control.state(Phase::Closed, None);
}
