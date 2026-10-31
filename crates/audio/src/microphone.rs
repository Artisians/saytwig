use crate::{AudioError, CaptureBuffer, CaptureStatus, Pcm16k, MAX_SECONDS};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
pub enum DeviceSelection {
    #[default]
    Default,
    Named(String),
}
// A timed-out native call can outlive its caller. Keep the lease in the worker,
// through stream teardown, so retries cannot accumulate detached native workers.
static CAPTURE_WORKER_ACTIVE: AtomicBool = AtomicBool::new(false);
struct CaptureLease;
impl CaptureLease {
    fn acquire() -> Result<Self, AudioError> {
        CAPTURE_WORKER_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| AudioError::Busy)
    }
}
impl Drop for CaptureLease {
    fn drop(&mut self) {
        CAPTURE_WORKER_ACTIVE.store(false, Ordering::Release);
    }
}
pub struct Microphone;
pub struct Capture {
    commands: mpsc::SyncSender<Command>,
    status: Arc<AtomicU8>,
    worker: Option<JoinHandle<Result<Pcm16k, AudioError>>>,
}
/// Nonblocking cancellation capability for the owner UI. It stops capture even
/// when the application actor is busy with disk, clipboard or model work.
/// Retaining this handle does not keep an already dropped Capture recording.
#[derive(Clone)]
pub struct CaptureCancellation {
    commands: mpsc::SyncSender<Command>,
    status: Arc<AtomicU8>,
}
impl CaptureCancellation {
    /// Signals immediate cancellation; never waits for driver teardown. The
    /// owned capture worker observes the flag within its next polling interval.
    pub fn cancel(&self) {
        self.status.store(3, Ordering::Release);
        let _ = self.commands.try_send(Command::Cancel);
    }
}
enum Command {
    Stop,
    Cancel,
}

impl Microphone {
    /// Enumerate input names only; never opens or starts a stream. Call off the UI
    /// thread. Names are backend identifiers, not guaranteed unique across devices.
    pub fn list_devices() -> Result<Vec<String>, AudioError> {
        let devices = cpal::default_host()
            .input_devices()
            .map_err(|_| AudioError::DeviceUnavailable)?;
        let mut names: Vec<String> = devices.filter_map(|device| device.name().ok()).collect();
        names.sort();
        names.dedup();
        Ok(names)
    }

    /// Explicit user-action boundary. Stream creation happens only here on an
    /// owned worker; the caller should invoke off its UI thread.
    pub fn start(selection: DeviceSelection) -> Result<Capture, AudioError> {
        Self::start_cancellable(selection, || false)
    }
    /// Starts only while the caller's request is still current. The owned,
    /// nonblocking predicate is checked before device setup, around stream play,
    /// and throughout recording. It must not acquire a UI lock or perform I/O.
    /// Native device setup cannot itself be interrupted; if it finishes after
    /// cancellation, the prepared stream is dropped without calling play.
    pub fn start_cancellable(
        selection: DeviceSelection,
        cancelled: impl Fn() -> bool + Send + 'static,
    ) -> Result<Capture, AudioError> {
        if cancelled() {
            return Err(AudioError::Cancelled);
        }
        let lease = CaptureLease::acquire()?;
        let (commands, receiver) = mpsc::sync_channel(1);
        let (ready, started) = mpsc::sync_channel(1);
        let status = Arc::new(AtomicU8::new(0));
        let worker_status = status.clone();
        let worker = thread::Builder::new()
            .name("saytwig-microphone".into())
            .spawn(move || {
                let _lease = lease;
                run(selection, receiver, ready, worker_status, cancelled)
            })
            .map_err(|_| AudioError::StartFailed)?;
        match started.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(())) => Ok(Capture {
                commands,
                status,
                worker: Some(worker),
            }),
            // Native stream destruction may still be running after this error.
            // Do not turn a reported startup failure into an unbounded join;
            // the worker retains its lease until teardown really finishes.
            Ok(Err(error)) => Err(error),
            Err(_) => {
                // A slow device must not start playing after the caller has
                // already received a startup timeout.
                status.store(3, Ordering::Release);
                let _ = commands.try_send(Command::Cancel);
                Err(AudioError::StartFailed)
            }
        }
    }
}
impl Capture {
    /// Clone this onto the UI's generation-tagged request state. Call cancel on
    /// interruption, window close, or a late Recording event for an old request.
    pub fn cancellation(&self) -> CaptureCancellation {
        CaptureCancellation {
            commands: self.commands.clone(),
            status: self.status.clone(),
        }
    }
    pub fn status(&self) -> CaptureStatus {
        decode_status(self.status.load(Ordering::Acquire))
    }
    pub fn stop(mut self) -> Result<Pcm16k, AudioError> {
        let _ = self.commands.try_send(Command::Stop);
        self.join()
    }
    pub fn cancel(mut self) -> Result<(), AudioError> {
        self.cancellation().cancel();
        match self.join() {
            Err(AudioError::Cancelled) | Ok(_) => Ok(()),
            Err(error) => Err(error),
        }
    }
    fn join(&mut self) -> Result<Pcm16k, AudioError> {
        self.worker
            .take()
            .ok_or(AudioError::WorkerStopped)?
            .join()
            .map_err(|_| AudioError::WorkerStopped)?
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            self.cancellation().cancel();
            let _ = worker.join();
        }
    }
}
fn decode_status(value: u8) -> CaptureStatus {
    match value {
        0 => CaptureStatus::Recording,
        1 => CaptureStatus::LimitReached,
        2 => CaptureStatus::Stopped,
        3 => CaptureStatus::Cancelled,
        4 => CaptureStatus::Failed(AudioError::DeviceLost),
        5 => CaptureStatus::Failed(AudioError::InvalidSamples),
        _ => CaptureStatus::Failed(AudioError::Overrun),
    }
}
fn run(
    selection: DeviceSelection,
    commands: mpsc::Receiver<Command>,
    ready: mpsc::SyncSender<Result<(), AudioError>>,
    status: Arc<AtomicU8>,
    cancelled: impl Fn() -> bool,
) -> Result<Pcm16k, AudioError> {
    if cancelled() {
        let _ = ready.send(Err(AudioError::Cancelled));
        return Err(AudioError::Cancelled);
    }
    let setup = prepare(selection, status.clone());
    let (stream, buffer) = match setup {
        Ok(value) => value,
        Err(error) => {
            let _ = ready.send(Err(error));
            return Err(error);
        }
    };
    if status.load(Ordering::Acquire) == 3 || cancelled() {
        let _ = ready.send(Err(AudioError::Cancelled));
        return Err(AudioError::Cancelled);
    }
    if stream.play().is_err() {
        let _ = ready.send(Err(AudioError::StartFailed));
        return Err(AudioError::StartFailed);
    }
    if cancelled() {
        let _ = ready.send(Err(AudioError::Cancelled));
        return Err(AudioError::Cancelled);
    }
    if ready.send(Ok(())).is_err() {
        return Err(AudioError::Cancelled);
    }
    finish_capture(
        stream,
        buffer,
        commands,
        status,
        Duration::from_secs(MAX_SECONDS as u64),
        &cancelled,
    )
}

fn finish_capture(
    stream: cpal::Stream,
    buffer: Arc<Mutex<CaptureBuffer>>,
    commands: mpsc::Receiver<Command>,
    status: Arc<AtomicU8>,
    duration: Duration,
    cancelled: &impl Fn() -> bool,
) -> Result<Pcm16k, AudioError> {
    let deadline = Instant::now() + duration;
    loop {
        if cancelled() {
            status.store(3, Ordering::Release);
        }
        if status.load(Ordering::Acquire) != 0 {
            break;
        }
        if Instant::now() >= deadline {
            let _ = status.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
            break;
        }
        match commands.recv_timeout(Duration::from_millis(10)) {
            Ok(Command::Stop) => {
                let _ = status.compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire);
                break;
            }
            Ok(Command::Cancel) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                status.store(3, Ordering::Release);
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    // Stop the actual microphone at the duration cap, cancellation or error;
    // retaining only bounded samples is not enough for a privacy boundary.
    drop(stream);
    let mut buffer = buffer.lock().map_err(|_| AudioError::WorkerStopped)?;
    match decode_status(status.load(Ordering::Acquire)) {
        CaptureStatus::Cancelled => buffer.cancel(),
        CaptureStatus::Failed(error) => buffer.fail(error),
        CaptureStatus::LimitReached => buffer.mark_limit_reached(),
        _ => {}
    }
    buffer.finish()
}
fn prepare(
    selection: DeviceSelection,
    status: Arc<AtomicU8>,
) -> Result<(cpal::Stream, Arc<Mutex<CaptureBuffer>>), AudioError> {
    let host = cpal::default_host();
    let device = match selection {
        DeviceSelection::Default => host.default_input_device(),
        DeviceSelection::Named(name) => host
            .input_devices()
            .map_err(|_| AudioError::DeviceUnavailable)?
            .find(|device| device.name().is_ok_and(|candidate| candidate == name)),
    }
    .ok_or(AudioError::DeviceUnavailable)?;
    let supported = device
        .default_input_config()
        .map_err(|_| AudioError::UnsupportedFormat)?;
    let buffer = Arc::new(Mutex::new(CaptureBuffer::new(
        supported.sample_rate().0,
        supported.channels(),
    )?));
    let config = supported.config();
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config, buffer.clone(), status),
        cpal::SampleFormat::F64 => build::<f64>(&device, &config, buffer.clone(), status),
        cpal::SampleFormat::I8 => build::<i8>(&device, &config, buffer.clone(), status),
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, buffer.clone(), status),
        cpal::SampleFormat::I32 => build::<i32>(&device, &config, buffer.clone(), status),
        cpal::SampleFormat::I64 => build::<i64>(&device, &config, buffer.clone(), status),
        cpal::SampleFormat::U8 => build::<u8>(&device, &config, buffer.clone(), status),
        cpal::SampleFormat::U16 => build::<u16>(&device, &config, buffer.clone(), status),
        cpal::SampleFormat::U32 => build::<u32>(&device, &config, buffer.clone(), status),
        cpal::SampleFormat::U64 => build::<u64>(&device, &config, buffer.clone(), status),
        _ => return Err(AudioError::UnsupportedFormat),
    }?;
    Ok((stream, buffer))
}
fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buffer: Arc<Mutex<CaptureBuffer>>,
    status: Arc<AtomicU8>,
) -> Result<cpal::Stream, AudioError>
where
    T: cpal::SizedSample + Copy,
    f32: cpal::FromSample<T>,
{
    let error_status = status.clone();
    device
        .build_input_stream(
            config,
            move |samples: &[T], _| {
                if status.load(Ordering::Acquire) != 0 {
                    return;
                }
                let Ok(mut buffer) = buffer.try_lock() else {
                    let _ = status.compare_exchange(0, 6, Ordering::AcqRel, Ordering::Acquire);
                    return;
                };
                match buffer.push_converted(samples, |value| {
                    <f32 as cpal::FromSample<T>>::from_sample_(value)
                }) {
                    Ok(CaptureStatus::LimitReached) => {
                        let _ = status.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
                    }
                    Err(_) => {
                        let _ = status.compare_exchange(0, 5, Ordering::AcqRel, Ordering::Acquire);
                    }
                    _ => {}
                }
            },
            move |_| {
                let _ = error_status.compare_exchange(0, 4, Ordering::AcqRel, Ordering::Acquire);
            },
            None,
        )
        .map_err(|_| AudioError::StartFailed)
}
