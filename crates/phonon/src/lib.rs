//! Newly authored, shell-independent integration with the official Fermion runtime.
//! Call blocking operations off the UI thread. No model download occurs on spawn,
//! warm-up, or recognition: explicit installation is a separate operation.
mod install;
mod integrity;
mod service;
mod wire;
pub use install::install_requested;
pub use integrity::{
    verify_model, RuntimeBundle, ARCHIVE_BYTES, ARCHIVE_HASH, MODEL_BYTES, MODEL_HASH, MODEL_URL,
};
pub use service::{Audio16k, Cancellation, Phase, PhononService, Snapshot, WarmPolicy};
pub const MODEL_ID: &str = "FermionResearch/Phonon-2.5-English";
pub const RUNTIME_VERSION: &str = "0.2.15";
