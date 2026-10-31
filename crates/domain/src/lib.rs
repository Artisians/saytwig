//! Fresh, UI-independent dictation state and explicitly located user storage.
//! No microphone, network, credentials, automatic imports, or content logging.
pub mod session;
pub mod storage;
pub use session::{Completion, Failure, Generation, Phase, Session, SessionError, Transcript};
pub use storage::{History, HistoryEntry, JsonStore, Revision, Settings, Snapshot, StoreError};
