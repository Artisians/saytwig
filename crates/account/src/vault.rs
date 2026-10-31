//! The only credential persistence boundary. No environment/file token discovery.
use crate::AccountError;
use std::{fs::File, path::Path, sync::Arc, time::Duration};
use zeroize::Zeroizing;

pub(crate) trait Vault: Send + Sync {
    fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, AccountError>;
    fn save(&self, bytes: &[u8]) -> Result<(), AccountError>;
}

pub(crate) async fn read(
    vault: Arc<dyn Vault>,
    lock: Arc<File>,
) -> Result<Option<Zeroizing<Vec<u8>>>, AccountError> {
    tokio::time::timeout(
        Duration::from_secs(10),
        tokio::task::spawn_blocking(move || {
            // Hold the process lock even if an OS vault call outlives our timeout.
            let _lock = lock;
            vault.load()
        }),
    )
    .await
    .map_err(|_| AccountError::Vault)?
    .map_err(|_| AccountError::Vault)?
}

pub(crate) async fn write(
    vault: Arc<dyn Vault>,
    lock: Arc<File>,
    writes: Arc<tokio::sync::Mutex<()>>,
    bytes: Zeroizing<Vec<u8>>,
) -> Result<(), AccountError> {
    // An owned ordering guard survives caller cancellation and remains held by
    // a timed-out OS operation. Later writes can never overtake that operation.
    let ordering = tokio::time::timeout(Duration::from_secs(10), writes.lock_owned())
        .await
        .map_err(|_| AccountError::Vault)?;
    tokio::time::timeout(
        Duration::from_secs(10),
        tokio::task::spawn_blocking(move || {
            let _ordering = ordering;
            let _lock = lock;
            vault.save(&bytes)
        }),
    )
    .await
    .map_err(|_| AccountError::Vault)?
    .map_err(|_| AccountError::Vault)?
}

pub(crate) fn platform(path: &Path) -> Result<Arc<dyn Vault>, AccountError> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let _ = path;
        // Explicit native builder: never keyring's mock/default fallback.
        #[cfg(target_os = "linux")]
        let builder = keyring::secret_service::default_credential_builder();
        #[cfg(target_os = "macos")]
        let builder = keyring::macos::default_credential_builder();
        let credential = builder
            .build(None, "com.saytwig.account", "host-v1")
            .map_err(|_| AccountError::Vault)?;
        Ok(Arc::new(NativeVault(keyring::Entry::new_with_credential(
            credential,
        ))))
    }
    #[cfg(target_os = "windows")]
    {
        Ok(Arc::new(DpapiVault(path.join("account-v1.dpapi"))))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = path;
        Err(AccountError::Vault)
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct NativeVault(keyring::Entry);
#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Vault for NativeVault {
    fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, AccountError> {
        match self.0.get_secret() {
            Ok(bytes) if bytes.len() <= crate::MAX_VAULT => Ok(Some(Zeroizing::new(bytes))),
            Err(keyring::Error::NoEntry) => Ok(None),
            _ => Err(AccountError::Vault),
        }
    }
    fn save(&self, bytes: &[u8]) -> Result<(), AccountError> {
        self.0.set_secret(bytes).map_err(|_| AccountError::Vault)
    }
}

#[cfg(target_os = "windows")]
struct DpapiVault(std::path::PathBuf);
#[cfg(target_os = "windows")]
impl Vault for DpapiVault {
    fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, AccountError> {
        use std::io::Read;
        let file = match File::open(&self.0) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(AccountError::Vault),
        };
        let mut encrypted = Vec::new();
        file.take((crate::MAX_VAULT + 65537) as u64)
            .read_to_end(&mut encrypted)
            .map_err(|_| AccountError::Vault)?;
        if encrypted.len() > crate::MAX_VAULT + 65536 {
            return Err(AccountError::Vault);
        }
        let decrypted = dpapi(&encrypted, false)?;
        if decrypted.len() > crate::MAX_VAULT {
            return Err(AccountError::Vault);
        }
        Ok(Some(decrypted))
    }
    fn save(&self, bytes: &[u8]) -> Result<(), AccountError> {
        use std::io::Write;
        let encrypted = dpapi(bytes, true)?;
        let mut temp = tempfile::NamedTempFile::new_in(self.0.parent().ok_or(AccountError::Vault)?)
            .map_err(|_| AccountError::Vault)?;
        temp.write_all(&encrypted)
            .map_err(|_| AccountError::Vault)?;
        temp.as_file().sync_all().map_err(|_| AccountError::Vault)?;
        durable_replace(temp.into_temp_path(), &self.0)?;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn dpapi(bytes: &[u8], encrypt: bool) -> Result<Zeroizing<Vec<u8>>, AccountError> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    use zeroize::Zeroize;
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into().map_err(|_| AccountError::Vault)?,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: The input blob borrows a live immutable slice for this synchronous
    // call; DPAPI does not mutate it. Optional/reserved arguments are null as
    // required. On success Windows allocates output for LocalFree. We check its
    // pointer/length before constructing a slice, copy it, wipe it, and release
    // it exactly once. Current-user DPAPI only, never LOCAL_MACHINE or UI.
    unsafe {
        let ok = if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(AccountError::Vault);
        }
        if output.pbData.is_null()
            || output.cbData == 0
            || output.cbData as usize > crate::MAX_VAULT + 65536
        {
            if !output.pbData.is_null() {
                LocalFree(output.pbData.cast());
            }
            return Err(AccountError::Vault);
        }
        let slice = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let result = Zeroizing::new(slice.to_vec());
        slice.zeroize();
        LocalFree(output.pbData.cast());
        Ok(result)
    }
}

#[cfg(target_os = "windows")]
fn durable_replace(temp: tempfile::TempPath, target: &Path) -> Result<(), AccountError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, SetFileAttributesW, FILE_ATTRIBUTE_NORMAL, MOVEFILE_REPLACE_EXISTING,
        MOVEFILE_WRITE_THROUGH,
    };
    let source: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    if source[..source.len() - 1].contains(&0) || target[..target.len() - 1].contains(&0) {
        return Err(AccountError::Vault);
    }
    // SAFETY: Both paths are valid NUL-terminated UTF-16 buffers that remain live
    // during the synchronous call. The private temporary ciphertext was synced
    // and closed above. Same-directory replacement has no copy/delete fallback;
    // WRITE_THROUGH requests completion on disk before token rotation proceeds.
    let replaced = unsafe {
        // NamedTempFile marks its file TEMPORARY. Clear that cache hint before
        // preserving it as the durable credential record, matching persist's
        // attribute handling while adding our explicit write-through move.
        if SetFileAttributesW(source.as_ptr(), FILE_ATTRIBUTE_NORMAL) == 0 {
            return Err(AccountError::Vault);
        }
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    // TempPath removes only the temporary name on failure; after successful
    // replacement that name no longer exists. It never removes the destination.
    if replaced == 0 {
        return Err(AccountError::Vault);
    }
    Ok(())
}
