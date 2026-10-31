#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOs,
    LinuxX11,
    LinuxWayland,
    Headless,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    Supported,
    Unsupported,
    HostEventLoopRequired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionState {
    NotRequired,
    Unknown,
    CheckAtUse,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub platform: Platform,
    pub hotkeys: Capability,
    pub clipboard_copy: Capability,
    pub paste: Capability,
    pub clipboard_restore: Capability,
    pub tray: Capability,
    pub microphone_permission: PermissionState,
    pub input_permission: PermissionState,
}
impl Capabilities {
    /// Platform potential; use `detect` for this build's enabled capabilities.
    pub fn for_platform(platform: Platform) -> Self {
        use Capability::*;
        let (hotkeys, clipboard_copy, paste, clipboard_restore, tray) = match platform {
            Platform::Windows => (
                HostEventLoopRequired,
                Supported,
                Supported,
                Supported,
                HostEventLoopRequired,
            ),
            Platform::MacOs => (
                HostEventLoopRequired,
                Supported,
                Unsupported,
                Unsupported,
                HostEventLoopRequired,
            ),
            Platform::LinuxX11 => (Supported, Supported, Unsupported, Unsupported, Unsupported),
            _ => (
                Unsupported,
                Unsupported,
                Unsupported,
                Unsupported,
                Unsupported,
            ),
        };
        Self {
            platform,
            hotkeys,
            clipboard_copy,
            paste,
            clipboard_restore,
            tray,
            microphone_permission: PermissionState::CheckAtUse,
            input_permission: if platform == Platform::Windows {
                PermissionState::CheckAtUse
            } else {
                PermissionState::Unknown
            },
        }
    }
    pub fn detect() -> Self {
        let platform = if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(target_os = "linux") {
            if std::env::var_os("WAYLAND_DISPLAY").is_some()
                || std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind == "wayland")
            {
                Platform::LinuxWayland
            } else if std::env::var_os("DISPLAY").is_some() {
                Platform::LinuxX11
            } else {
                Platform::Headless
            }
        } else {
            Platform::Other
        };
        let mut capabilities = Self::for_platform(platform);
        if !cfg!(feature = "native") {
            capabilities.hotkeys = Capability::Unsupported;
            capabilities.clipboard_copy = Capability::Unsupported;
            capabilities.paste = Capability::Unsupported;
            capabilities.clipboard_restore = Capability::Unsupported;
        }
        if !cfg!(feature = "tray") {
            capabilities.tray = Capability::Unsupported;
        }
        capabilities
    }
}
