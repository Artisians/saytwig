use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Every request owns a fresh token. Tokens must accompany all async results.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Generation(u64);
impl Generation {
    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Idle,
    Preparing,
    Recording,
    Transcribing,
    Cleaning,
    Error,
}

/// Sanitized categories only; provider bodies and transcript content never enter errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Failure {
    ModelUnavailable,
    MicrophoneUnavailable,
    DeviceLost,
    InvalidAudio,
    TranscriptionFailed,
    CleanupFailed,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SessionError {
    #[error("A dictation is already active")]
    Busy,
    #[error("Completion belongs to an earlier dictation")]
    Stale,
    #[error("Completion is not valid in the current phase")]
    WrongPhase,
    #[error("The request sequence is exhausted; restart the application")]
    SequenceExhausted,
}

/// Deliberately not Debug: transcripts may contain private user information.
#[derive(Clone, PartialEq, Eq)]
pub struct Transcript {
    pub original: String,
    pub cleaned: Option<String>,
}
impl Transcript {
    pub fn output(&self) -> &str {
        self.cleaned.as_deref().unwrap_or(&self.original)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completion {
    NeedsCleanup,
    Ready,
}

pub struct Session {
    generation: u64,
    active: Option<Generation>,
    phase: Phase,
    cleanup_enabled: bool,
    cleanup_for_request: bool,
    result: Option<Transcript>,
    failure: Option<Failure>,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            generation: 0,
            active: None,
            phase: Phase::Idle,
            cleanup_enabled: false,
            cleanup_for_request: false,
            result: None,
            failure: None,
        }
    }
}
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("phase", &self.phase)
            .field("active", &self.active)
            .field("cleanup_enabled", &self.cleanup_enabled)
            .finish_non_exhaustive()
    }
}
impl Session {
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn generation(&self) -> Option<Generation> {
        self.active
    }
    pub fn failure(&self) -> Option<Failure> {
        self.failure
    }
    pub fn transcript(&self) -> Option<&Transcript> {
        self.result.as_ref()
    }
    pub fn cleanup_enabled(&self) -> bool {
        self.cleanup_enabled
    }
    /// Revoking consent during cleanup immediately preserves the original. The
    /// caller must also cancel its network request; late results are rejected.
    pub fn set_cleanup_enabled(&mut self, enabled: bool) {
        self.cleanup_enabled = enabled;
        if !enabled {
            self.cleanup_for_request = false;
            if self.phase == Phase::Cleaning {
                self.finish();
            }
        }
    }
    pub fn start(&mut self) -> Result<Generation, SessionError> {
        if !matches!(self.phase, Phase::Idle | Phase::Error) {
            return Err(SessionError::Busy);
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(SessionError::SequenceExhausted)?;
        let token = Generation(self.generation);
        self.active = Some(token);
        self.phase = Phase::Preparing;
        self.cleanup_for_request = self.cleanup_enabled;
        self.result = None;
        self.failure = None;
        Ok(token)
    }
    pub fn prepared(&mut self, token: Generation) -> Result<(), SessionError> {
        self.require(token, Phase::Preparing)?;
        self.phase = Phase::Recording;
        Ok(())
    }
    pub fn recording_stopped(&mut self, token: Generation) -> Result<(), SessionError> {
        self.require(token, Phase::Recording)?;
        self.phase = Phase::Transcribing;
        Ok(())
    }
    pub fn transcribed(
        &mut self,
        token: Generation,
        original: String,
    ) -> Result<Completion, SessionError> {
        self.require(token, Phase::Transcribing)?;
        let nonblank = !original.trim().is_empty();
        self.result = Some(Transcript {
            original,
            cleaned: None,
        });
        if self.cleanup_for_request && self.cleanup_enabled && nonblank {
            self.phase = Phase::Cleaning;
            Ok(Completion::NeedsCleanup)
        } else {
            self.finish();
            Ok(Completion::Ready)
        }
    }
    /// None represents transport, policy or decoding failure. Empty/malformed
    /// output also falls back to the original; cleanup can never erase dictation.
    pub fn cleanup_completed(
        &mut self,
        token: Generation,
        cleaned: Option<String>,
    ) -> Result<(), SessionError> {
        self.require(token, Phase::Cleaning)?;
        if let Some(result) = self.result.as_mut() {
            result.cleaned = cleaned.filter(|text| valid_cleaned_text(text));
        }
        self.finish();
        Ok(())
    }
    pub fn fail(&mut self, token: Generation, failure: Failure) -> Result<(), SessionError> {
        if self.active != Some(token) {
            return Err(SessionError::Stale);
        }
        self.failure = Some(failure);
        if self.phase == Phase::Cleaning {
            self.finish();
            return Ok(());
        }
        self.phase = Phase::Error;
        self.active = None;
        Ok(())
    }
    /// Invalidates all outstanding completions, including model preparation.
    /// The host remains responsible for stopping its device and worker handles.
    pub fn cancel(&mut self) {
        self.active = None;
        self.phase = Phase::Idle;
        self.result = None;
        self.failure = None;
        self.cleanup_for_request = false;
    }
    pub fn take_transcript(&mut self) -> Option<Transcript> {
        if self.phase == Phase::Idle {
            self.result.take()
        } else {
            None
        }
    }
    fn finish(&mut self) {
        self.phase = Phase::Idle;
        self.active = None;
    }
    fn require(&self, token: Generation, phase: Phase) -> Result<(), SessionError> {
        if self.active != Some(token) {
            Err(SessionError::Stale)
        } else if self.phase != phase {
            Err(SessionError::WrongPhase)
        } else {
            Ok(())
        }
    }
}
fn valid_cleaned_text(text: &str) -> bool {
    !text.trim().is_empty()
        && !text.contains('\0')
        && text.len() <= 256_000
        && !text.trim_start().starts_with("<think>")
        && text
            .chars()
            .any(|ch| !ch.is_whitespace() && !matches!(ch, '\u{200B}' | '\u{FEFF}'))
}
