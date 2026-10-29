//! Versioned, bounded JSON persistence. All locations are explicitly provided by
//! the caller; no old application files, credentials or history are discovered.
use fs2::FileExt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use thiserror::Error;

const VERSION: u32 = 1;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    pub cleanup_enabled: bool,
    pub input_device: Option<String>,
    pub vocabulary: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct History {
    pub entries: Vec<HistoryEntry>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub created_unix_ms: u64,
    pub original: String,
    pub cleaned: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl History {
    pub fn append(&mut self, entry: HistoryEntry) -> Result<(), StoreError> {
        if self.entries.iter().any(|item| item.id == entry.id) {
            return Err(StoreError::DuplicateEntry);
        }
        self.entries.push(entry);
        Ok(())
    }
}
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("The data file could not be accessed")]
    Io(#[source] std::io::Error),
    #[error("The data file is malformed; it was not modified")]
    Malformed,
    #[error("The data file uses an unsupported schema version; it was not modified")]
    UnsupportedVersion,
    #[error("The data file exceeds the size limit; it was not modified")]
    TooLarge,
    #[error("The data changed since it was read; reload before saving")]
    Conflict,
    #[error("A symbolic-link data file is not accepted")]
    Symlink,
    #[error("A history entry with this identity already exists")]
    DuplicateEntry,
}
impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    schema_version: u32,
    data: T,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}
/// An opaque revision, intentionally not Debug because it retains private bytes.
pub struct Revision {
    bytes: Vec<u8>,
    extra: BTreeMap<String, Value>,
}
pub struct Snapshot<T> {
    pub value: T,
    pub revision: Revision,
}
pub struct JsonStore<T> {
    path: PathBuf,
    marker: PhantomData<T>,
}
impl<T: Serialize + DeserializeOwned> JsonStore<T> {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            marker: PhantomData,
        }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn load(&self) -> Result<Option<Snapshot<T>>, StoreError> {
        let _lock = self.lock()?;
        self.read_unlocked()?
            .map(|bytes| {
                let header: Value =
                    serde_json::from_slice(&bytes).map_err(|_| StoreError::Malformed)?;
                if header.get("schema_version").and_then(Value::as_u64) != Some(VERSION as u64) {
                    return Err(StoreError::UnsupportedVersion);
                }
                let envelope: Envelope<T> =
                    serde_json::from_slice(&bytes).map_err(|_| StoreError::Malformed)?;
                Ok(Snapshot {
                    value: envelope.data,
                    revision: Revision {
                        bytes,
                        extra: envelope.extra,
                    },
                })
            })
            .transpose()
    }
    /// None creates a new file only. Replacing an existing file requires the
    /// matching revision returned by load/save. Cooperative writers use a lock.
    pub fn save(&self, value: &T, expected: Option<&Revision>) -> Result<Revision, StoreError> {
        let _lock = self.lock()?;
        let previous = self.read_unlocked()?;
        if previous.as_deref() != expected.map(|revision| revision.bytes.as_slice()) {
            return Err(StoreError::Conflict);
        }
        let extra = expected
            .map(|revision| revision.extra.clone())
            .unwrap_or_default();
        let bytes = serde_json::to_vec_pretty(&Envelope {
            schema_version: VERSION,
            data: value,
            extra: extra.clone(),
        })
        .map_err(|_| StoreError::Malformed)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(StoreError::TooLarge);
        }
        let parent = self.parent();
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&self.path)
            .map_err(|error| StoreError::Io(error.error))?;
        // Directory durability is supported on Unix. Atomic replacement itself
        // is provided by tempfile on all supported operating systems.
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(Revision { bytes, extra })
    }
    fn parent(&self) -> &Path {
        self.path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
    }
    fn lock(&self) -> Result<File, StoreError> {
        fs::create_dir_all(self.parent())?;
        let name = self
            .path
            .file_name()
            .ok_or(StoreError::Malformed)?
            .to_string_lossy();
        let lock_path = self.parent().join(format!(".{name}.lock"));
        reject_symlink(&lock_path)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        lock.lock_exclusive()?;
        Ok(lock)
    }
    fn read_unlocked(&self) -> Result<Option<Vec<u8>>, StoreError> {
        reject_symlink(&self.path)?;
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if file.metadata()?.len() > MAX_FILE_BYTES {
            return Err(StoreError::TooLarge);
        }
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(StoreError::TooLarge);
        }
        Ok(Some(bytes))
    }
}
fn reject_symlink(path: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(StoreError::Symlink),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
