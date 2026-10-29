use crate::worker::{EngineState, Event};
use dictation_domain::{Failure, Generation, History, Phase, Session, Settings, Transcript};

pub(crate) struct Model {
    pub(crate) session: Session,
    pub(crate) engine: EngineState,
    pub(crate) settings: Settings,
    pub(crate) settings_loaded: bool,
    pub(crate) history: History,
    pub(crate) last: Option<Transcript>,
    pub(crate) notice: Option<&'static str>,
    pub(crate) installing: bool,
    pub(crate) devices: Vec<String>,
}
impl Default for Model {
    fn default() -> Self {
        Self {
            session: Session::default(),
            engine: EngineState::Checking,
            settings: Settings::default(),
            settings_loaded: false,
            history: History::default(),
            last: None,
            notice: None,
            installing: false,
            devices: Vec::new(),
        }
    }
}
impl Model {
    pub(crate) fn active(&self) -> bool {
        !matches!(self.session.phase(), Phase::Idle | Phase::Error)
    }
    pub(crate) fn can_start(&self) -> bool {
        self.settings_loaded
            && !self.active()
            && !self.installing
            && matches!(self.engine, EngineState::Installed | EngineState::Ready)
    }
    pub(crate) fn apply(&mut self, event: Event) -> Option<(Generation, Transcript)> {
        match event {
            Event::Engine(engine) => self.engine = engine,
            Event::Loaded(settings, history) => {
                self.settings_loaded = true;
                self.settings = settings;
                self.history = history;
                self.session.set_cleanup_enabled(false);
            }
            Event::History(history) => self.history = history,
            Event::Devices(devices) => self.devices = devices,
            Event::Recording(generation, _) => {
                let _ = self.session.prepared(generation);
            }
            Event::Transcribing(generation) => {
                let _ = self.session.recording_stopped(generation);
            }
            Event::Completed(generation, text) => {
                if self.session.transcribed(generation, text).is_ok() {
                    if let Some(result) = self.session.take_transcript() {
                        self.last = Some(result.clone());
                        if result.original.trim().is_empty() {
                            self.notice = Some("empty_result");
                        }
                        return Some((generation, result));
                    }
                }
            }
            Event::Failed(generation, failure) => {
                if self.session.fail(generation, failure).is_ok() {
                    self.notice = Some(match failure {
                        Failure::ModelUnavailable => "model_unavailable",
                        Failure::MicrophoneUnavailable | Failure::DeviceLost => {
                            "microphone_unavailable"
                        }
                        Failure::InvalidAudio => "invalid_audio",
                        _ => "transcription_failed",
                    });
                }
            }
            Event::Notice(notice) => {
                self.notice = Some(notice);
                if matches!(notice, "storage_unavailable" | "storage_unreadable") {
                    self.settings_loaded = true;
                }
                if matches!(notice, "installation_failed" | "installation_cancelled") {
                    self.installing = false;
                }
            }
            Event::Installed => self.installing = false,
        }
        None
    }
}
