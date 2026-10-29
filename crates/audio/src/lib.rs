//! Explicit, bounded capture and anti-aliased normalization for local dictation.
//! Constructing a type never acquires microphone access. `Microphone::start` does.
mod capture;
#[cfg(feature = "microphone")]
mod microphone;
mod pcm;
pub use capture::{CaptureBuffer, CaptureStatus};
#[cfg(feature = "microphone")]
pub use microphone::{Capture, CaptureCancellation, DeviceSelection, Microphone};
pub use pcm::{AudioError, Pcm16k, MAX_SECONDS, SAMPLE_RATE};
