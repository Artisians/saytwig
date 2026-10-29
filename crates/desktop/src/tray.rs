//! Optional native tray ownership. Creation requires a running host event loop;
//! macOS additionally requires the main thread. No menu actions are implicit.
use crate::DesktopError;
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
pub struct TrayHandle {
    icon: TrayIcon,
}
impl TrayHandle {
    pub fn create(
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        tooltip: &str,
    ) -> Result<Self, DesktopError> {
        let icon =
            Icon::from_rgba(rgba, width, height).map_err(|_| DesktopError::TrayUnavailable)?;
        let icon = TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip(tooltip)
            .with_menu_on_left_click(false)
            .build()
            .map_err(|_| DesktopError::TrayUnavailable)?;
        Ok(Self { icon })
    }
    pub fn set_tooltip(&self, tooltip: &str) -> Result<(), DesktopError> {
        self.icon
            .set_tooltip(Some(tooltip))
            .map_err(|_| DesktopError::TrayUnavailable)
    }
    pub fn set_visible(&self, visible: bool) -> Result<(), DesktopError> {
        self.icon
            .set_visible(visible)
            .map_err(|_| DesktopError::TrayUnavailable)
    }
    // Dropping TrayIcon removes only this owned icon.
}
