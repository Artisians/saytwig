//! Explicit, bounded capture and anti-aliased normalization for local dictation.
//! Constructing a type never acquires microphone access. `Microphone::start` does.
#[cfg(all(windows, not(target_arch = "x86_64")))]
compile_error!("Saytwig supports Windows x64 only; select an x86_64 Windows target.");

mod capture;
#[cfg(feature = "microphone")]
mod microphone;
mod pcm;
pub use capture::{CaptureBuffer, CaptureStatus};
#[cfg(feature = "microphone")]
pub use microphone::{Capture, CaptureCancellation, DeviceSelection, Microphone};
pub use pcm::{AudioError, Pcm16k, MAX_SECONDS, SAMPLE_RATE};
