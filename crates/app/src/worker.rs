//! Owned background effects. Audio, hashing, disk and inference never run on the UI thread.
use dictation_audio::{Capture, CaptureCancellation, CaptureStatus, DeviceSelection, Microphone};
use dictation_domain::{Failure, Generation, History, HistoryEntry, JsonStore, Revision, Settings};
use phonon_service::{Audio16k, Cancellation, PhononService, RuntimeBundle, WarmPolicy};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EngineState {
    Checking,
    MissingRuntime,
    MissingModel,
    Installed,
    Ready,
    Failed,
}

pub(crate) enum Event {
    Engine(EngineState),
    Loaded(Settings, History),
    History(History),
    Devices(Vec<String>),
    Recording(Generation, CaptureCancellation),
    Transcribing(Generation),
    Completed(Generation, String),
    Failed(Generation, Failure),
    Notice(&'static str),
    Installed,
    Setup(phonon_service::SetupProgress),
}
pub(crate) enum Command {
    Start {
        generation: Generation,
        cancellation: Cancellation,
        settings: Settings,
    },
    Stop,
    Cancel,
    Install(Cancellation),
    SaveSettings(Settings),
    SaveTranscript {
        original: String,
        cleaned: Option<String>,
    },
    Reload,
    ListDevices,
    Shutdown,
}
// One reserved final transcript and one pending start/install are independent:
// settings/device churn cannot displace a final result or a cancellation.
struct MailboxState {
    closed: AtomicBool,
    control: AtomicU8,
    reload: AtomicBool,
    devices: AtomicBool,
    settings: Mutex<Option<Settings>>,
    wake: mpsc::SyncSender<()>,
}
pub(crate) struct CommandSender {
    state: Arc<MailboxState>,
    work: mpsc::SyncSender<Command>,
    history: mpsc::SyncSender<Command>,
}
impl CommandSender {
    /// Never blocks the UI. Rejected payloads are returned to their owner;
    /// callers must retain final text and visibly report failed saves.
    pub(crate) fn send(&self, command: Command) -> Result<(), mpsc::TrySendError<Command>> {
        if self.state.closed.load(Ordering::Acquire) {
            return Err(mpsc::TrySendError::Disconnected(command));
        }
        match command {
            command @ (Command::Start { .. } | Command::Install(_)) => {
                self.work.try_send(command)?
            }
            command @ Command::SaveTranscript { .. } => self.history.try_send(command)?,
            Command::SaveSettings(settings) => {
                let Ok(mut slot) = self.state.settings.try_lock() else {
                    return Err(mpsc::TrySendError::Full(Command::SaveSettings(settings)));
                };
                *slot = Some(settings);
            }
            Command::Stop => {
                self.state.control.fetch_max(1, Ordering::AcqRel);
            }
            Command::Cancel => {
                self.state.control.store(2, Ordering::Release);
            }
            Command::Reload => self.state.reload.store(true, Ordering::Release),
            Command::ListDevices => self.state.devices.store(true, Ordering::Release),
            Command::Shutdown => self.state.closed.store(true, Ordering::Release),
        }
        let _ = self.state.wake.try_send(());
        Ok(())
    }
}
pub(crate) struct EventInbox {
    receiver: mpsc::Receiver<Event>,
    consumed: mpsc::SyncSender<()>,
}
impl EventInbox {
    pub(crate) fn try_iter(&self) -> impl Iterator<Item = Event> + '_ {
        std::iter::from_fn(|| {
            let event = self.receiver.try_recv().ok()?;
            let _ = self.consumed.try_send(());
            Some(event)
        })
    }
}
struct EventSender {
    sender: mpsc::SyncSender<Event>,
    consumed: mpsc::Receiver<()>,
    state: Arc<MailboxState>,
}
impl EventSender {
    fn send(&self, mut event: Event) {
        // Only the background actor waits. Normal operation never drops a final
        // result; explicit app shutdown interrupts backpressure, not persistence.
        while !self.state.closed.load(Ordering::Acquire) {
            match self.sender.try_send(event) {
                Ok(()) | Err(mpsc::TrySendError::Disconnected(_)) => return,
                Err(mpsc::TrySendError::Full(pending)) => event = pending,
            }
            let _ = self.consumed.recv_timeout(Duration::from_millis(50));
        }
    }
}
struct Mailbox {
    state: Arc<MailboxState>,
    work: mpsc::Receiver<Command>,
    history: mpsc::Receiver<Command>,
    wake: mpsc::Receiver<()>,
}
impl Mailbox {
    fn flush_history(&self, actor: &mut Actor) {
        if let Ok(command) = self.history.try_recv() {
            actor.command(command);
        }
    }
    fn flush_settings(&self, actor: &mut Actor) {
        // No I/O under this lock; the foreground uses try_lock, never waits.
        let settings = self
            .state
            .settings
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());
        if let Some(settings) = settings {
            actor.save_settings(settings);
        }
    }
    fn run(&self, actor: &mut Actor) {
        loop {
            self.flush_history(actor);
            if self.state.closed.load(Ordering::Acquire) {
                break;
            }
            match self.state.control.swap(0, Ordering::AcqRel) {
                2 => actor.cancel(),
                1 => actor.finish(),
                _ => {}
            }
            self.flush_settings(actor);
            if self.state.closed.load(Ordering::Acquire) {
                break;
            }
            if self.state.reload.swap(false, Ordering::AcqRel) {
                actor.command(Command::Reload);
            }
            if self.state.devices.swap(false, Ordering::AcqRel) {
                actor.command(Command::ListDevices);
            }
            if let Ok(command) = self.work.try_recv() {
                // UI submits SaveTranscript before its next Start. Separate
                // reserved channels must not reorder those accepted operations.
                self.flush_history(actor);
                if !self.state.closed.load(Ordering::Acquire) {
                    actor.command(command);
                }
            }
            actor.poll_capture();
            let _ = self.wake.recv_timeout(Duration::from_millis(50));
        }
        // Accepted writes survive shutdown. Never prune history to fit a cap.
        self.flush_history(actor);
        self.flush_settings(actor);
    }
}
impl Drop for Mailbox {
    fn drop(&mut self) {
        self.state.closed.store(true, Ordering::Release);
    }
}
/// The foreground owns only bounded mailboxes and cancellation handles.
pub(crate) struct Worker {
    pub(crate) commands: CommandSender,
    pub(crate) events: EventInbox,
    thread: Option<thread::JoinHandle<()>>,
}
impl Worker {
    pub(crate) fn start() -> std::io::Result<Self> {
        let (work, input) = mpsc::sync_channel(1);
        let (history, saves) = mpsc::sync_channel(1);
        let (wake, awakened) = mpsc::sync_channel(1);
        let state = Arc::new(MailboxState {
            closed: AtomicBool::new(false),
            control: AtomicU8::new(0),
            reload: AtomicBool::new(false),
            devices: AtomicBool::new(false),
            settings: Mutex::new(None),
            wake,
        });
        // Four queued events plus the producer's current event form a fixed
        // backpressure window, rather than unlimited full History snapshots.
        let (output, events) = mpsc::sync_channel(4);
        let (consumed, acknowledged) = mpsc::sync_channel(1);
        let output = EventSender {
            sender: output,
            consumed: acknowledged,
            state: state.clone(),
        };
        let mailbox = Mailbox {
            state: state.clone(),
            work: input,
            history: saves,
            wake: awakened,
        };
        let thread = thread::Builder::new()
            .name("saytwig-effects".into())
            .spawn(move || {
                let mut actor = Actor::new(output);
                actor.load();
                actor.configure();
                mailbox.run(&mut actor);
                actor.cancel();
                if let Some(service) = actor.service.take() {
                    service.shutdown();
                }
            })?;
        Ok(Self {
            commands: CommandSender {
                state,
                work,
                history,
            },
            events: EventInbox {
                receiver: events,
                consumed,
            },
            thread: Some(thread),
        })
    }
    pub(crate) fn shutdown(&mut self) -> Option<thread::JoinHandle<()>> {
        let _ = self.commands.send(Command::Shutdown);
        self.thread.take()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
struct ActiveCapture {
    generation: Generation,
    cancellation: Cancellation,
    capture: Capture,
    vocabulary: Vec<String>,
}
struct Actor {
    output: EventSender,
    bundle: Option<RuntimeBundle>,
    service: Option<PhononService>,
    capture: Option<ActiveCapture>,
    model_root: PathBuf,
    data_root: Option<PathBuf>,
    writable: bool,
    settings_revision: Option<Revision>,
    last_runtime_phase: Option<phonon_service::Phase>,
}
impl Actor {
    fn new(output: EventSender) -> Self {
        let data_root = data_directory();
        let model_root = std::env::var_os("SAYTWIG_MODEL_STORE")
            .map(PathBuf::from)
            .or_else(|| data_root.as_ref().map(|root| root.join("models")))
            .unwrap_or_else(|| PathBuf::from(".saytwig-models"));
        Self {
            output,
            bundle: None,
            service: None,
            capture: None,
            model_root,
            data_root,
            writable: true,
            settings_revision: None,
            last_runtime_phase: None,
        }
    }
    fn emit(&self, event: Event) {
        self.output.send(event);
    }
    fn load(&mut self) {
        let Some(root) = &self.data_root else {
            self.writable = false;
            self.emit(Event::Notice("storage_unavailable"));
            return;
        };
        let settings = JsonStore::<Settings>::new(root.join("settings.json")).load();
        let history = JsonStore::<History>::new(root.join("history.json")).load();
        match (settings, history) {
            (Ok(settings), Ok(history)) => {
                let settings = match settings {
                    Some(snapshot) => {
                        self.settings_revision = Some(snapshot.revision);
                        snapshot.value
                    }
                    None => Settings::default(),
                };
                self.emit(Event::Loaded(
                    settings,
                    history.map(|h| h.value).unwrap_or_default(),
                ));
            }
            _ => {
                self.writable = false;
                self.emit(Event::Notice("storage_unreadable"));
            }
        }
    }
    fn configure(&mut self) {
        self.emit(Event::Engine(EngineState::Checking));
        let explicit_root = std::env::var_os("SAYTWIG_RUNTIME_DIR");
        let explicit_hash = std::env::var("SAYTWIG_RUNTIME_SHA256").ok();
        let resolved = match (explicit_root, explicit_hash) {
            (Some(root), Some(hash)) => RuntimeBundle::open(PathBuf::from(root), &hash).map(Some),
            (None, None) => match &self.data_root {
                Some(root) => phonon_service::managed_runtime(&root.join("runtimes")),
                None => Ok(None),
            },
            _ => Err(anyhow::anyhow!("Incomplete development runtime override")),
        };
        let bundle = match resolved {
            Ok(Some(bundle)) => bundle,
            Ok(None) => {
                self.emit(Event::Engine(EngineState::MissingRuntime));
                return;
            }
            Err(_) => {
                self.emit(Event::Engine(EngineState::Failed));
                self.emit(Event::Notice("runtime_invalid"));
                return;
            }
        };
        self.configure_bundle(bundle);
    }
    fn configure_bundle(&mut self, bundle: RuntimeBundle) {
        self.bundle = Some(bundle.clone());
        let model = self.model_root.join("phonon-2.5-en");
        if phonon_service::verify_model(&model).is_err() {
            self.emit(Event::Engine(EngineState::MissingModel));
            return;
        }
        match PhononService::spawn(
            bundle,
            model,
            WarmPolicy::IdleTimeout(Duration::from_secs(900)),
        ) {
            Ok(service) => {
                self.service = Some(service);
                self.emit(Event::Engine(EngineState::Installed));
            }
            Err(_) => self.emit(Event::Engine(EngineState::Failed)),
        }
    }
    fn command(&mut self, command: Command) {
        match command {
            Command::Start {
                generation,
                cancellation,
                settings,
            } => self.start(generation, cancellation, settings),
            Command::Stop => self.finish(),
            Command::Cancel => self.cancel(),
            Command::Install(cancellation) => self.install(cancellation),
            Command::SaveSettings(settings) => self.save_settings(settings),
            Command::SaveTranscript { original, cleaned } => {
                self.save_transcript(original, cleaned)
            }
            Command::ListDevices => match Microphone::list_devices() {
                Ok(devices) => self.emit(Event::Devices(devices)),
                Err(_) => self.emit(Event::Notice("microphone_unavailable")),
            },
            Command::Reload => {
                if self.capture.is_none() {
                    if let Some(service) = self.service.take() {
                        service.shutdown();
                    }
                    self.bundle = None;
                    self.configure();
                }
            }
            Command::Shutdown => {}
        }
    }
    fn install(&mut self, cancellation: Cancellation) {
        let result = (|| -> anyhow::Result<()> {
            anyhow::ensure!(self.capture.is_none(), "Cannot install while recording");
            let bundle = match self.bundle.clone() {
                Some(bundle) => bundle,
                None => {
                    let root = self
                        .data_root
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No managed application data directory"))?
                        .join("runtimes");
                    let bundle = phonon_service::install_runtime_requested(
                        &root,
                        &cancellation,
                        |progress| self.emit(Event::Setup(progress)),
                    )?;
                    self.bundle = Some(bundle.clone());
                    self.emit(Event::Engine(EngineState::MissingModel));
                    bundle
                }
            };
            self.emit(Event::Setup(phonon_service::SetupProgress::Ready));
            phonon_service::install_requested(&bundle, &self.model_root, &cancellation)?;
            if let Some(service) = self.service.take() {
                service.shutdown();
            }
            self.configure_bundle(bundle);
            Ok(())
        })();
        match result {
            Ok(()) => self.emit(Event::Installed),
            Err(_) if cancellation.is_cancelled() => {
                self.emit(Event::Notice("installation_cancelled"))
            }
            Err(error) => {
                let notice = match error.downcast_ref::<phonon_service::SetupFailure>() {
                    Some(phonon_service::SetupFailure::UnsupportedHost) => "setup_unsupported",
                    Some(phonon_service::SetupFailure::InsufficientSpace) => "setup_no_space",
                    Some(phonon_service::SetupFailure::RecoveryRequired) => {
                        "setup_recovery_required"
                    }
                    Some(phonon_service::SetupFailure::BootstrapUnavailable) => {
                        "setup_bootstrap_unavailable"
                    }
                    None => "installation_failed",
                };
                self.emit(Event::Notice(notice));
            }
        }
    }
    fn start(&mut self, generation: Generation, cancellation: Cancellation, settings: Settings) {
        self.last_runtime_phase = None;
        if self.capture.is_some() || cancellation.is_cancelled() {
            return;
        }
        let Some(service) = &self.service else {
            self.emit(Event::Failed(generation, Failure::ModelUnavailable));
            return;
        };
        if service.warm(&cancellation).is_err() {
            if !cancellation.is_cancelled() {
                self.emit(Event::Failed(generation, Failure::ModelUnavailable));
            }
            return;
        }
        if cancellation.is_cancelled() {
            return;
        }
        let selection = settings
            .input_device
            .map(DeviceSelection::Named)
            .unwrap_or_default();
        let capture_request = cancellation.clone();
        match Microphone::start_cancellable(selection, move || capture_request.is_cancelled()) {
            Ok(capture) if !cancellation.is_cancelled() => {
                let capture_cancel = capture.cancellation();
                self.capture = Some(ActiveCapture {
                    generation,
                    cancellation,
                    capture,
                    vocabulary: settings.vocabulary,
                });
                self.emit(Event::Engine(EngineState::Ready));
                self.emit(Event::Recording(generation, capture_cancel));
            }
            Ok(capture) => {
                let _ = capture.cancel();
            }
            Err(_) => self.emit(Event::Failed(generation, Failure::MicrophoneUnavailable)),
        }
    }
    fn poll_capture(&mut self) {
        if self
            .capture
            .as_ref()
            .is_some_and(|active| active.cancellation.is_cancelled())
        {
            self.cancel();
            return;
        }
        if self
            .capture
            .as_ref()
            .is_some_and(|active| active.capture.status() != CaptureStatus::Recording)
        {
            self.finish();
        }
        if let Some(service) = &self.service {
            let phase = service.snapshot().phase;
            if self.last_runtime_phase != Some(phase) {
                self.last_runtime_phase = Some(phase);
                let engine = match phase {
                    phonon_service::Phase::Stopped => Some(EngineState::Installed),
                    phonon_service::Phase::Failed | phonon_service::Phase::Closed => {
                        Some(EngineState::Failed)
                    }
                    phonon_service::Phase::Ready => Some(EngineState::Ready),
                    _ => None,
                };
                if let Some(engine) = engine {
                    self.emit(Event::Engine(engine));
                }
            }
        }
    }

    fn finish(&mut self) {
        let Some(active) = self.capture.take() else {
            return;
        };
        let generation = active.generation;
        if active.cancellation.is_cancelled() {
            let _ = active.capture.cancel();
            return;
        }
        self.emit(Event::Transcribing(generation));
        let audio = active
            .capture
            .stop()
            .ok()
            .and_then(|pcm| Audio16k::new(pcm.into_samples()).ok());
        let Some(audio) = audio else {
            self.emit(Event::Failed(generation, Failure::InvalidAudio));
            return;
        };
        if let Some(service) = &self.service {
            match service.transcribe(audio, &active.vocabulary, &active.cancellation) {
                Ok(text) if !active.cancellation.is_cancelled() => {
                    self.emit(Event::Completed(generation, text))
                }
                Err(_) if !active.cancellation.is_cancelled() => {
                    self.emit(Event::Failed(generation, Failure::TranscriptionFailed))
                }
                _ => {}
            }
        }
    }
    fn cancel(&mut self) {
        if let Some(active) = self.capture.take() {
            active.cancellation.cancel();
            let _ = active.capture.cancel();
        }
    }
    fn save_settings(&mut self, settings: Settings) {
        let Some(root) = self.data_root.as_ref().filter(|_| self.writable) else {
            self.emit(Event::Notice("settings_not_saved"));
            return;
        };
        let store = JsonStore::<Settings>::new(root.join("settings.json"));
        match store.save(&settings, self.settings_revision.as_ref()) {
            Ok(revision) => {
                self.settings_revision = Some(revision);
                self.emit(Event::Notice("settings_saved"));
            }
            Err(_) => self.emit(Event::Notice("settings_not_saved")),
        }
    }

    fn save_transcript(&self, original: String, cleaned: Option<String>) {
        let Some(root) = self.data_root.as_ref().filter(|_| self.writable) else {
            self.emit(Event::Notice("history_not_saved"));
            return;
        };
        let store = JsonStore::<History>::new(root.join("history.json"));
        let result = (|| {
            let snapshot = store.load()?;
            let (mut history, revision) = snapshot
                .map(|s| (s.value, Some(s.revision)))
                .unwrap_or_default();
            history.append(HistoryEntry {
                id: uuid::Uuid::new_v4().to_string(),
                created_unix_ms: now_ms(),
                original,
                cleaned,
                extra: Default::default(),
            })?;
            store.save(&history, revision.as_ref())?;
            Ok::<_, dictation_domain::StoreError>(history)
        })();
        match result {
            Ok(history) => self.emit(Event::History(history)),
            Err(_) => self.emit(Event::Notice("history_not_saved")),
        }
    }
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}
fn data_directory() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("SAYTWIG_DATA_DIR") {
        return Some(root.into());
    }
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|root| PathBuf::from(root).join("Saytwig"))
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|root| PathBuf::from(root).join("Library/Application Support/Saytwig"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|root| PathBuf::from(root).join(".local/share"))
            })
            .map(|root| root.join("saytwig"))
    }
}
