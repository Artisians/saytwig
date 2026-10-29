use crate::DesktopError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutSignal {
    Pressed,
    Released,
}
pub trait HotkeyBackend {
    fn register(&mut self, shortcut: &str) -> Result<u32, DesktopError>;
    fn unregister(&mut self, id: u32) -> Result<(), DesktopError>;
    fn next_event(&mut self) -> Option<(u32, ShortcutSignal)>;
}
/// One owned shortcut. Rebinding registers the replacement first, so an invalid
/// or occupied shortcut never silently removes the working binding.
pub struct HotkeyLease<B: HotkeyBackend> {
    backend: B,
    binding: Option<(u32, String)>,
    pressed: bool,
}
impl<B: HotkeyBackend> HotkeyLease<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            binding: None,
            pressed: false,
        }
    }
    pub fn bind(&mut self, shortcut: &str) -> Result<(), DesktopError> {
        if self
            .binding
            .as_ref()
            .is_some_and(|(_, existing)| existing == shortcut)
        {
            return Ok(());
        }
        let new = self.backend.register(shortcut)?;
        if let Some((old, _)) = &self.binding {
            if let Err(error) = self.backend.unregister(*old) {
                let _ = self.backend.unregister(new);
                return Err(error);
            }
        }
        self.binding = Some((new, shortcut.to_owned()));
        self.pressed = false;
        Ok(())
    }
    pub fn unbind(&mut self) -> Result<(), DesktopError> {
        if let Some((id, _)) = &self.binding {
            self.backend.unregister(*id)?;
        }
        self.binding = None;
        self.pressed = false;
        Ok(())
    }
    pub fn is_bound(&self) -> bool {
        self.binding.is_some()
    }
    pub fn poll(&mut self) -> Option<ShortcutSignal> {
        for _ in 0..32 {
            let (id, event) = self.backend.next_event()?;
            if self
                .binding
                .as_ref()
                .is_none_or(|(active, _)| *active != id)
            {
                continue;
            }
            let pressed = event == ShortcutSignal::Pressed;
            if pressed == self.pressed {
                continue;
            }
            self.pressed = pressed;
            return Some(event);
        }
        None
    }
}
impl<B: HotkeyBackend> Drop for HotkeyLease<B> {
    fn drop(&mut self) {
        let _ = self.unbind();
    }
}
