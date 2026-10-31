//! Win32 operations stay on the constructing thread. No target activation occurs.
use crate::delivery::{ClipboardSnapshot, DesktopBackend, DesktopError, Target};
use std::{marker::PhantomData, ptr, rc::Rc};
use windows_sys::Win32::{
    Foundation::{GetLastError, GlobalFree, SetLastError, HGLOBAL, HWND},
    System::{
        DataExchange::*,
        Memory::*,
        Ole::{CF_LOCALE, CF_OEMTEXT, CF_TEXT, CF_UNICODETEXT},
    },
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub struct NativeDesktop {
    owner: HWND,
    _thread: PhantomData<Rc<()>>,
}
impl NativeDesktop {
    /// Creates an invisible message-only clipboard owner, never a focusable UI.
    pub fn new() -> Result<Self, DesktopError> {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // SAFETY: class is terminated; predefined STATIC class requires no
        // registered class or instance. HWND_MESSAGE creates no visible window.
        let owner = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if owner.is_null() {
            return Err(DesktopError::ClipboardUnavailable);
        }
        Ok(Self {
            owner,
            _thread: PhantomData,
        })
    }
    fn open(&self) -> Result<ClipboardLock, DesktopError> {
        // SAFETY: owner is live on this thread for the duration of self.
        if unsafe { OpenClipboard(self.owner) } == 0 {
            Err(DesktopError::ClipboardUnavailable)
        } else {
            Ok(ClipboardLock)
        }
    }
}
impl Drop for NativeDesktop {
    fn drop(&mut self) {
        // SAFETY: this instance uniquely owns the live HWND on its creating thread.
        unsafe {
            DestroyWindow(self.owner);
        }
    }
}
struct ClipboardLock;
impl Drop for ClipboardLock {
    fn drop(&mut self) {
        // SAFETY: a guard is constructed only after successful OpenClipboard.
        unsafe {
            CloseClipboard();
        }
    }
}
struct Allocation(HGLOBAL);
impl Drop for Allocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: non-null means an owned allocation not transferred to Windows.
            unsafe {
                GlobalFree(self.0);
            }
        }
    }
}
fn sequence() -> Result<u64, DesktopError> {
    // SAFETY: this query takes no pointers and has no caller preconditions.
    let revision = unsafe { GetClipboardSequenceNumber() };
    if revision == 0 {
        Err(DesktopError::ClipboardUnavailable)
    } else {
        Ok(revision as u64)
    }
}
/// Called only while OpenClipboard is held. Reject rich formats rather than
/// claiming to preserve images, HTML, file lists or arbitrary delayed renderers.
fn snapshot_locked() -> Result<ClipboardSnapshot, DesktopError> {
    // SAFETY: SetLastError writes only this thread's error slot.
    unsafe {
        SetLastError(0);
    }
    // SAFETY: caller holds the clipboard lock; no raw pointers are passed.
    let count = unsafe { CountClipboardFormats() };
    // SAFETY: query of this thread's error slot has no preconditions.
    if count == 0 && unsafe { GetLastError() } != 0 {
        return Err(DesktopError::ClipboardUnavailable);
    }
    let revision = sequence()?;
    if count == 0 {
        return Ok(ClipboardSnapshot {
            text: None,
            revision,
        });
    }
    let mut format = 0;
    let mut unicode = false;
    loop {
        // SAFETY: SetLastError writes only this thread's error slot.
        unsafe {
            SetLastError(0);
        }
        // SAFETY: lock is held, and format is zero or the previous enumerated ID.
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            // SAFETY: query of this thread's error slot has no preconditions.
            if unsafe { GetLastError() } != 0 {
                return Err(DesktopError::ClipboardUnavailable);
            }
            break;
        }
        if format == CF_UNICODETEXT as u32 {
            unicode = true;
        }
        if ![CF_UNICODETEXT, CF_TEXT, CF_OEMTEXT, CF_LOCALE].contains(&(format as u16))
            || format > u16::MAX as u32
        {
            return Err(DesktopError::PreservationUnavailable);
        }
    }
    if !unicode {
        return Err(DesktopError::PreservationUnavailable);
    }
    // SAFETY: clipboard lock is held and the requested format was enumerated.
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT as u32) };
    if handle.is_null() {
        return Err(DesktopError::ClipboardUnavailable);
    }
    // SAFETY: non-null CF_UNICODETEXT data is a Windows global-memory handle.
    let size = unsafe { GlobalSize(handle) };
    if size == 0 || size > 2_000_000 || size % 2 != 0 {
        return Err(DesktopError::PreservationUnavailable);
    }
    // SAFETY: the clipboard-owned handle remains valid while the lock is held.
    let data = unsafe { GlobalLock(handle) } as *const u16;
    if data.is_null() {
        return Err(DesktopError::ClipboardUnavailable);
    }
    // SAFETY: GlobalLock returns stable storage of GlobalSize bytes until unlock.
    let units = unsafe { std::slice::from_raw_parts(data, size / 2) };
    let text = units
        .iter()
        .position(|unit| *unit == 0)
        .ok_or(DesktopError::PreservationUnavailable)
        .and_then(|end| {
            String::from_utf16(&units[..end]).map_err(|_| DesktopError::PreservationUnavailable)
        });
    // SAFETY: pairs the successful GlobalLock after the last borrowed data use.
    unsafe {
        GlobalUnlock(handle);
    }
    // Reading a delayed-rendered format can advance the sequence; use the final
    // value while still holding the clipboard lock.
    Ok(ClipboardSnapshot {
        text: Some(text?),
        revision: sequence()?,
    })
}
fn write_locked(text: Option<&str>) -> Result<u64, DesktopError> {
    // Allocate before clearing so allocation errors leave clipboard untouched.
    let mut memory = if let Some(text) = text {
        let units: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        // SAFETY: allocation size is exactly the live UTF-16 buffer size.
        let memory = Allocation(unsafe { GlobalAlloc(GMEM_MOVEABLE, units.len() * 2) });
        if memory.0.is_null() {
            return Err(DesktopError::ClipboardUnavailable);
        }
        // SAFETY: memory holds the non-null allocation owned by this scope.
        let data = unsafe { GlobalLock(memory.0) } as *mut u16;
        if data.is_null() {
            return Err(DesktopError::ClipboardUnavailable);
        }
        // SAFETY: separate allocations, aligned UTF-16 pointers, and destination
        // has units.len() elements; unlock follows the last data access.
        unsafe {
            ptr::copy_nonoverlapping(units.as_ptr(), data, units.len());
            GlobalUnlock(memory.0);
        }
        Some(memory)
    } else {
        None
    };
    // SAFETY: caller holds OpenClipboard with a valid owner HWND.
    if unsafe { EmptyClipboard() } == 0 {
        return Err(DesktopError::ClipboardUnavailable);
    }
    if let Some(memory) = memory.as_mut() {
        // SAFETY: memory is unlocked moveable global storage containing terminated
        // UTF-16; ownership transfers only on success while clipboard is open.
        if unsafe { SetClipboardData(CF_UNICODETEXT as u32, memory.0) }.is_null() {
            return Err(DesktopError::ClipboardUnavailable);
        }
        // Successful SetClipboardData transfers ownership to Windows.
        memory.0 = ptr::null_mut();
    }
    sequence()
}
pub(super) fn foreground_target() -> Result<Target, DesktopError> {
    // SAFETY: no pointers or caller preconditions for this window query.
    let window = unsafe { GetForegroundWindow() };
    let mut process = 0;
    if window.is_null() {
        return Err(DesktopError::FocusChanged);
    }
    // SAFETY: process is a live writable u32; stale/invalid HWND returns zero.
    let thread = unsafe { GetWindowThreadProcessId(window, &mut process) };
    if thread == 0 || process == 0 || process == std::process::id() {
        return Err(DesktopError::FocusChanged);
    }
    Ok(Target {
        window: window as usize as u64,
        process,
    })
}
fn key(code: u16, release: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: code,
                wScan: 0,
                dwFlags: if release { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
impl DesktopBackend for NativeDesktop {
    fn foreground(&mut self) -> Result<Target, DesktopError> {
        foreground_target()
    }
    fn snapshot_text(&mut self) -> Result<ClipboardSnapshot, DesktopError> {
        let _lock = self.open()?;
        snapshot_locked()
    }
    fn write_text(
        &mut self,
        text: &str,
        expected_revision: Option<u64>,
    ) -> Result<Option<u64>, DesktopError> {
        if text.contains('\0') || text.len() > 256_000 {
            return Err(DesktopError::InvalidText);
        }
        let _lock = self.open()?;
        if expected_revision.is_some_and(|expected| sequence().ok() != Some(expected)) {
            return Err(DesktopError::ClipboardChanged);
        }
        // Preserve mode can roll back a rare native write failure under the same
        // lock; no concurrent writer can be overwritten during this rollback.
        let backup = if expected_revision.is_some() {
            Some(snapshot_locked()?)
        } else {
            None
        };
        match write_locked(Some(text)) {
            Ok(revision) => Ok(Some(revision)),
            Err(error) => {
                if let Some(backup) = backup {
                    let _ = write_locked(backup.text.as_deref());
                }
                Err(error)
            }
        }
    }
    fn paste_to(&mut self, target: Target, written_revision: u64) -> Result<(), DesktopError> {
        // Hold only through enqueueing the input batch, never until the recipient
        // reads it. Dropping this guard before returning permits target access.
        let _lock = self.open()?;
        if sequence()? != written_revision {
            return Err(DesktopError::ClipboardChanged);
        }
        if foreground_target()? != target {
            return Err(DesktopError::FocusChanged);
        }
        for code in [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN, 0x56] {
            // SAFETY: all listed codes are valid Win32 virtual-key identifiers.
            if unsafe { GetAsyncKeyState(code as i32) } < 0 {
                return Err(DesktopError::ModifiersHeld);
            }
        }
        // Recheck immediately before submitting. Windows cannot atomically bind
        // SendInput to an HWND: a user focus change can still race this fence.
        if foreground_target()? != target {
            return Err(DesktopError::FocusChanged);
        }
        let inputs = [
            key(VK_CONTROL, false),
            key(0x56, false),
            key(0x56, true),
            key(VK_CONTROL, true),
        ];
        // SAFETY: inputs is a live array of fully initialized INPUT structures;
        // count and structure size exactly describe the readable array.
        let count = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                size_of::<INPUT>() as i32,
            )
        };
        if count != inputs.len() as u32 {
            if count > 0 {
                let releases = [key(0x56, true), key(VK_CONTROL, true)];
                // SAFETY: releases contains exactly two initialized INPUT values.
                unsafe {
                    SendInput(2, releases.as_ptr(), size_of::<INPUT>() as i32);
                }
            }
            return Err(if count == 0 {
                DesktopError::InputRejected
            } else {
                DesktopError::InputPartiallyDispatched
            });
        }
        Ok(())
    }
    fn restore_if_unchanged(
        &mut self,
        snapshot: ClipboardSnapshot,
        written_revision: u64,
    ) -> Result<bool, DesktopError> {
        let _lock = self.open()?;
        if sequence()? != written_revision {
            return Ok(false);
        }
        write_locked(snapshot.text.as_deref())?;
        Ok(true)
    }
}
