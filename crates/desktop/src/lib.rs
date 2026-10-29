//! Explicit desktop effects for Saytwig. No registration, clipboard writes,
//! keyboard injection or tray creation happens merely by loading this crate.
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
