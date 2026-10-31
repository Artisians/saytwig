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
    #[error("Another process is using the data file; retry after it finishes")]
    Busy,
    #[error("The data path is not a regular file")]
    NotRegular,
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
#[derive(Deserialize)]
struct VersionHeader {
    #[serde(default)]
    schema_version: Value,
}
#[derive(Serialize)]
struct BorrowedEnvelope<'a, T> {
    schema_version: u32,
    data: &'a T,
    #[serde(flatten)]
    extra: &'a BTreeMap<String, Value>,
}

/// Stops serialization before private data grows beyond the on-disk limit.
#[derive(Default)]
struct LimitedBytes {
    bytes: Vec<u8>,
    exceeded: bool,
}
impl Write for LimitedBytes {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let length = self.bytes.len().saturating_add(buffer.len());
        if length as u64 > MAX_FILE_BYTES {
            self.exceeded = true;
            return Err(std::io::Error::other("Data size limit exceeded"));
        }
        // Do not let geometric Vec growth reserve beyond the file-size cap.
        if length > self.bytes.capacity() {
            let capacity = self
                .bytes
                .capacity()
                .saturating_mul(2)
                .max(length)
                .min(MAX_FILE_BYTES as usize);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(std::io::Error::other)?;
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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
                // Ignore data while inspecting the version; do not retain a
                // second complete JSON tree alongside the typed history. The
                // typed parse still rejects duplicate fields as before.
                if serde_json::from_slice::<VersionHeader>(&bytes)
                    .map_err(|_| StoreError::Malformed)?
                    .schema_version
                    .as_u64()
                    != Some(VERSION as u64)
                {
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
        let mut output = LimitedBytes::default();
        if serde_json::to_writer_pretty(
            &mut output,
            &BorrowedEnvelope {
                schema_version: VERSION,
                data: value,
                extra: &extra,
            },
        )
        .is_err()
        {
            return Err(if output.exceeded {
                StoreError::TooLarge
            } else {
                StoreError::Malformed
            });
        }
        let bytes = output.bytes;
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
        lock.try_lock_exclusive().map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock
                || error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
            {
                StoreError::Busy
            } else {
                StoreError::Io(error)
            }
        })?;
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
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(StoreError::NotRegular),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
