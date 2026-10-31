use crate::hotkey::{HotkeyBackend, ShortcutSignal};
use crate::{Capabilities, Capability, DesktopError};
use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::rc::Rc;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::NativeDesktop;
#[cfg(not(windows))]
mod portable;
#[cfg(not(windows))]
pub use portable::NativeDesktop;

/// Create and keep on the host's event-loop thread (macOS: main thread).
/// The host must pump that loop; this type never installs another event handler.
pub struct NativeHotkeys {
    manager: GlobalHotKeyManager,
    registered: HashMap<u32, HotKey>,
    _thread: PhantomData<Rc<()>>,
}
impl NativeHotkeys {
    pub fn new() -> Result<Self, DesktopError> {
        if Capabilities::detect().hotkeys == Capability::Unsupported {
            return Err(DesktopError::Unsupported);
        }
        Ok(Self {
            manager: GlobalHotKeyManager::new().map_err(|_| DesktopError::HotkeyUnavailable)?,
            registered: HashMap::new(),
            _thread: PhantomData,
        })
    }
}
impl HotkeyBackend for NativeHotkeys {
    fn register(&mut self, shortcut: &str) -> Result<u32, DesktopError> {
        let hotkey: HotKey = shortcut
            .parse()
            .map_err(|_| DesktopError::HotkeyUnavailable)?;
        if self.registered.contains_key(&hotkey.id()) {
            return Err(DesktopError::HotkeyUnavailable);
        }
        self.manager
            .register(hotkey)
            .map_err(|_| DesktopError::HotkeyUnavailable)?;
        self.registered.insert(hotkey.id(), hotkey);
        Ok(hotkey.id())
    }
    fn unregister(&mut self, id: u32) -> Result<(), DesktopError> {
        let hotkey = self
            .registered
            .get(&id)
            .ok_or(DesktopError::HotkeyUnavailable)?;
        self.manager
            .unregister(*hotkey)
            .map_err(|_| DesktopError::HotkeyUnavailable)?;
        self.registered.remove(&id);
        Ok(())
    }
    fn next_event(&mut self) -> Option<(u32, ShortcutSignal)> {
        GlobalHotKeyEvent::receiver().try_recv().ok().map(|event| {
            (
                event.id,
                match event.state {
                    HotKeyState::Pressed => ShortcutSignal::Pressed,
                    HotKeyState::Released => ShortcutSignal::Released,
                },
            )
        })
    }
}
impl Drop for NativeHotkeys {
    fn drop(&mut self) {
        for hotkey in self.registered.values() {
            let _ = self.manager.unregister(*hotkey);
        }
    }
}

/// Reads the current external foreground target without creating a window,
/// accessing clipboard contents, or changing focus. Call at the shortcut edge,
/// before showing any app UI. Saytwig's own process is never an insertion target.
/// Non-Windows platforms fail closed; this is not a permission-grant claim.
pub fn foreground_target() -> Result<crate::Target, DesktopError> {
    #[cfg(windows)]
    {
        windows::foreground_target()
    }
    #[cfg(not(windows))]
    {
        Err(DesktopError::Unsupported)
    }
}
