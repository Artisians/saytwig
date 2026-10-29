use dictation_domain::Generation;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub window: u64,
    pub process: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryMode {
    Copy,
    Paste,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardPolicy {
    KeepNewText,
    PreserveText,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum DesktopError {
    #[error("This desktop capability is not supported")]
    Unsupported,
    #[error("The result was cancelled, superseded, or already delivered")]
    Stale,
    #[error("The original target is no longer focused; nothing was pasted")]
    FocusChanged,
    #[error("Release keyboard modifiers before pasting")]
    ModifiersHeld,
    #[error("The clipboard could not be accessed")]
    ClipboardUnavailable,
    #[error("The clipboard changed; it was not overwritten")]
    ClipboardChanged,
    #[error("The clipboard contains formats that cannot safely be restored")]
    PreservationUnavailable,
    #[error("Keyboard input was not accepted")]
    InputRejected,
    #[error("Keyboard input was only partially dispatched; check the target before retrying")]
    InputPartiallyDispatched,
    #[error("The shortcut is invalid or could not be registered")]
    HotkeyUnavailable,
    #[error("The tray icon could not be created")]
    TrayUnavailable,
    #[error("Text is empty, invalid or too large")]
    InvalidText,
}
/// Private clipboard contents deliberately have no Debug representation.
pub struct ClipboardSnapshot {
    pub text: Option<String>,
    pub revision: u64,
}
pub struct RestoreTicket {
    snapshot: ClipboardSnapshot,
    written_revision: u64,
}
pub struct DeliveryReceipt {
    pub paste_sent: bool,
    pub restore: Option<RestoreTicket>,
}

pub trait DesktopBackend {
    fn foreground(&mut self) -> Result<Target, DesktopError>;
    fn snapshot_text(&mut self) -> Result<ClipboardSnapshot, DesktopError>;
    /// expected_revision must be checked while the native clipboard is locked.
    /// A backend supporting snapshots MUST return a revision after writes; if
    /// conditional writes cannot be honored, reject before changing contents.
    fn write_text(
        &mut self,
        text: &str,
        expected_revision: Option<u64>,
    ) -> Result<Option<u64>, DesktopError>;
    /// Implementations must never activate a window. Check original target and
    /// held modifiers immediately before submitting a single paste input batch.
    /// Check written_revision while holding the clipboard lock, submit without
    /// waiting for the recipient, then immediately release that lock.
    fn paste_to(&mut self, target: Target, written_revision: u64) -> Result<(), DesktopError>;
    fn restore_if_unchanged(
        &mut self,
        snapshot: ClipboardSnapshot,
        written_revision: u64,
    ) -> Result<bool, DesktopError>;
}
#[derive(Clone, Copy)]
enum DeliveryState {
    Ready,
    Delivering,
    Used,
}
struct GateState {
    last: u64,
    active: Option<(Generation, Option<Target>, DeliveryState)>,
}
pub struct DeliveryGate {
    state: Mutex<GateState>,
    revoked: AtomicBool,
}
impl Default for DeliveryGate {
    fn default() -> Self {
        Self {
            revoked: AtomicBool::new(false),
            state: Mutex::new(GateState {
                last: 0,
                active: None,
            }),
        }
    }
}
impl DeliveryGate {
    /// Arm once per new request, capturing its target before showing any UI.
    pub fn arm(&self, generation: Generation, target: Option<Target>) -> Result<(), DesktopError> {
        let mut state = self.state.lock().map_err(|_| DesktopError::Stale)?;
        if self.revoked.load(Ordering::Acquire) || generation.value() <= state.last {
            return Err(DesktopError::Stale);
        }
        state.last = generation.value();
        state.active = Some((generation, target, DeliveryState::Ready));
        Ok(())
    }
    /// Permanently revokes this gate without waiting for native operations.
    /// Use a fresh per-request gate when cancellation is initiated from a UI.
    /// Queued work and later commits are rejected; a commit that already passed
    /// its final check may finish. Revocation cannot recall dispatched OS input.
    /// Use `cancel` off the UI thread if a completed-operation fence is needed.
    pub fn revoke(&self) {
        self.revoked.store(true, Ordering::Release);
    }
    /// Serializes with the small OS commit operation. After this returns, no
    /// output for the cancelled generation can be submitted. Already submitted
    /// keystrokes cannot be recalled; never present cancellation as undo.
    pub fn cancel(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.active = None;
        }
    }
    pub fn deliver(
        &self,
        backend: &mut impl DesktopBackend,
        generation: Generation,
        text: &str,
        mode: DeliveryMode,
        policy: ClipboardPolicy,
    ) -> Result<DeliveryReceipt, DesktopError> {
        if self.revoked.load(Ordering::Acquire) {
            return Err(DesktopError::Stale);
        }
        if text.trim().is_empty() || text.contains('\0') || text.len() > 256_000 {
            return Err(DesktopError::InvalidText);
        }
        let target = {
            let mut state = self.state.lock().map_err(|_| DesktopError::Stale)?;
            match state.active {
                Some((id, target, DeliveryState::Ready)) if id == generation => {
                    state.active = Some((id, target, DeliveryState::Delivering));
                    target
                }
                _ => return Err(DesktopError::Stale),
            }
        };
        let outcome = (|| {
            if mode == DeliveryMode::Paste {
                let target = target.ok_or(DesktopError::Unsupported)?;
                if backend.foreground()? != target {
                    return Err(DesktopError::FocusChanged);
                }
            }
            let snapshot = if policy == ClipboardPolicy::PreserveText {
                Some(backend.snapshot_text()?)
            } else {
                None
            };
            let revision = self.commit(generation, || {
                backend.write_text(text, snapshot.as_ref().map(|saved| saved.revision))
            })?;
            let ticket = match (snapshot, revision) {
                (Some(snapshot), Some(written_revision)) => Some(RestoreTicket {
                    snapshot,
                    written_revision,
                }),
                (Some(_), None) => return Err(DesktopError::PreservationUnavailable),
                _ => None,
            };
            if mode == DeliveryMode::Paste {
                let result = self.commit(generation, || {
                    backend.paste_to(
                        target.ok_or(DesktopError::Unsupported)?,
                        revision.ok_or(DesktopError::Unsupported)?,
                    )
                });
                if let Err(error) = result {
                    // Partial input submission is ambiguous: do not restore before
                    // an application may have consumed the clipboard.
                    if error != DesktopError::InputPartiallyDispatched {
                        if let Some(ticket) = ticket {
                            let _ = backend
                                .restore_if_unchanged(ticket.snapshot, ticket.written_revision);
                        }
                    }
                    return Err(error);
                }
            }
            Ok(DeliveryReceipt {
                paste_sent: mode == DeliveryMode::Paste,
                restore: ticket,
            })
        })();
        if let Ok(mut state) = self.state.lock() {
            if let Some((id, target, _)) = state.active {
                if id == generation {
                    state.active = Some((id, target, DeliveryState::Used));
                }
            }
        }
        outcome
    }
    fn commit<T>(
        &self,
        generation: Generation,
        effect: impl FnOnce() -> Result<T, DesktopError>,
    ) -> Result<T, DesktopError> {
        let state = self.state.lock().map_err(|_| DesktopError::Stale)?;
        if self.revoked.load(Ordering::Acquire)
            || !matches!(state.active,Some((id,_,DeliveryState::Delivering))if id==generation)
        {
            return Err(DesktopError::Stale);
        }
        effect()
    }
}
impl RestoreTicket {
    /// Call only after an explicit consumer acknowledgment / user action, never
    /// an arbitrary timer. Paste dispatch does not prove the app read clipboard.
    pub fn restore(self, backend: &mut impl DesktopBackend) -> Result<bool, DesktopError> {
        backend.restore_if_unchanged(self.snapshot, self.written_revision)
    }
}
