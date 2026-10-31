use crate::delivery::ClipboardSnapshot;
use crate::{Capabilities, Capability, DesktopBackend, DesktopError, Target};
/// Keep alive while copied text is offered, especially under X11 ownership.
pub struct NativeDesktop {
    clipboard: arboard::Clipboard,
}
impl NativeDesktop {
    pub fn new() -> Result<Self, DesktopError> {
        if Capabilities::detect().clipboard_copy == Capability::Unsupported {
            return Err(DesktopError::Unsupported);
        }
        Ok(Self {
            clipboard: arboard::Clipboard::new().map_err(|_| DesktopError::ClipboardUnavailable)?,
        })
    }
}
impl DesktopBackend for NativeDesktop {
    fn foreground(&mut self) -> Result<Target, DesktopError> {
        Err(DesktopError::Unsupported)
    }
    fn snapshot_text(&mut self) -> Result<ClipboardSnapshot, DesktopError> {
        Err(DesktopError::PreservationUnavailable)
    }
    fn write_text(
        &mut self,
        text: &str,
        expected_revision: Option<u64>,
    ) -> Result<Option<u64>, DesktopError> {
        if expected_revision.is_some() {
            return Err(DesktopError::PreservationUnavailable);
        }
        self.clipboard
            .set_text(text)
            .map_err(|_| DesktopError::ClipboardUnavailable)?;
        Ok(None)
    }
    fn paste_to(&mut self, _target: Target, _written_revision: u64) -> Result<(), DesktopError> {
        Err(DesktopError::Unsupported)
    }
    fn restore_if_unchanged(
        &mut self,
        _snapshot: ClipboardSnapshot,
        _written_revision: u64,
    ) -> Result<bool, DesktopError> {
        Err(DesktopError::PreservationUnavailable)
    }
}
