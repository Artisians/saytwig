//! Newly authored, shell-independent integration with the official Fermion runtime.
//! Call blocking operations off the UI thread. No model download occurs on spawn,
//! warm-up, or recognition: explicit installation is a separate operation.
#[cfg(all(windows, not(target_arch = "x86_64")))]
compile_error!("Saytwig supports Windows x64 only; Windows ARM and x86 builds are not supported.");
mod install;
mod integrity;
mod runtime_archive;
mod runtime_install;
mod runtime_targets;
mod service;
mod wire;
pub use install::install_requested;
pub use integrity::{
    verify_model, RuntimeBundle, ARCHIVE_BYTES, ARCHIVE_HASH, MODEL_BYTES, MODEL_HASH, MODEL_URL,
};
pub use runtime_install::{
    install_runtime_requested, managed_runtime, setup_capability, SetupCapability, SetupFailure,
    SetupMetadata, SetupProgress, MANAGED_RUNTIME_SHA256,
};
pub use service::{Audio16k, Cancellation, Phase, PhononService, Snapshot, WarmPolicy};
pub const MODEL_ID: &str = "FermionResearch/Phonon-2.5-English";
pub const RUNTIME_VERSION: &str = "0.2.15";
