//! Explicit desktop effects for Saytwig. No registration, clipboard writes,
//! keyboard injection or tray creation happens merely by loading this crate.
#[cfg(all(windows, not(target_arch = "x86_64")))]
compile_error!("Saytwig supports Windows x64 only; select an x86_64 Windows target.");

pub mod capabilities;
pub mod delivery;
pub mod hotkey;
#[cfg(feature = "native")]
pub mod native;
#[cfg(all(feature = "tray", any(windows, target_os = "macos")))]
pub mod tray;
pub use capabilities::{Capabilities, Capability, PermissionState, Platform};
pub use delivery::{
    ClipboardPolicy, DeliveryGate, DeliveryMode, DeliveryReceipt, DesktopBackend, DesktopError,
    RestoreTicket, Target,
};
