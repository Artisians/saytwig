//! Explicit, cancellable model provisioning. Service warm-up never calls this.
use crate::{
    integrity::{
        hash_file_checked, verify_model_checked, RuntimeBundle, ARCHIVE_BYTES, ARCHIVE_HASH,
        MODEL_URL,
    },
    Cancellation,
};
use anyhow::{ensure, Context, Result};
use fs2::FileExt;
use std::{
    fs::{self, OpenOptions},
    future::Future,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const OWNER: &[u8] = b"{\"schema\":1,\"purpose\":\"managed-phonon-model-store\"}\n";
fn check(cancel: &Cancellation) -> Result<()> {
    ensure!(!cancel.is_cancelled(), "Model installation cancelled");
    Ok(())
}

fn existing_regular_file(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file(),
                "Managed model file is not a regular file"
            );
            Ok(Some(metadata))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn open_store(root: &Path) -> Result<fs::File> {
    if !root.exists() {
        fs::create_dir_all(root)?;
    }
    ensure!(
        fs::symlink_metadata(root)?.is_dir(),
        "Model store is not a regular directory"
    );
    let owner = root.join("store.json");
    if !owner.exists() {
        ensure!(
            fs::read_dir(root)?.next().is_none(),
            "Refusing to claim a nonempty, unrecognized model directory"
        );
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&owner)?;
        file.write_all(OWNER)?;
        file.sync_all()?;
    }
    ensure!(
        fs::symlink_metadata(&owner)?.is_file() && fs::read(&owner)? == OWNER,
        "Unrecognized model-store ownership record"
    );
    let path = root.join("installation.lock");
    existing_regular_file(&path)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    lock.try_lock_exclusive()
        .context("Another model installation is in progress")?;
    Ok(lock)
}

async fn interruptible<F: Future>(future: F, cancel: &Cancellation) -> Result<F::Output> {
    tokio::pin!(future);
    loop {
        check(cancel)?;
        tokio::select! { output = &mut future => return Ok(output), _ = tokio::time::sleep(Duration::from_millis(50)) => {} }
    }
}

async fn download(archive: &Path, cancel: &Cancellation) -> Result<()> {
    let mut offset = 0;
    if let Some(metadata) = existing_regular_file(archive)? {
        offset = metadata.len();
        if offset == ARCHIVE_BYTES && hash_file_checked(archive, &|| check(cancel))? == ARCHIVE_HASH
        {
            return Ok(());
        }
        if offset >= ARCHIVE_BYTES {
            // This exact reserved file is owned by the recognized model store.
            fs::remove_file(archive)?;
            offset = 0;
        }
    }
    check(cancel)?;
    let client = reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(1800))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;
    let mut request = client
        .get(MODEL_URL)
        .header(reqwest::header::ACCEPT_ENCODING, "identity");
    if offset != 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
    }
    let mut response = interruptible(request.send(), cancel)
        .await?
        .context("Cannot download the pinned speech model")?;
    if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        let range = response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .context("Missing resumed-download range")?;
        ensure!(
            range == format!("bytes {offset}-{}/{}", ARCHIVE_BYTES - 1, ARCHIVE_BYTES),
            "Unexpected resumed-download range"
        );
    } else {
        ensure!(
            response.status() == reqwest::StatusCode::OK,
            "Model download returned HTTP {}",
            response.status()
        );
        offset = 0;
    }
    if let Some(length) = response.content_length() {
        ensure!(
            length == ARCHIVE_BYTES - offset,
            "Unexpected model download length"
        );
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(offset == 0)
        .append(offset != 0)
        .open(archive)?;
    let mut total = offset;
    while let Some(block) = interruptible(response.chunk(), cancel)
        .await?
        .context("Model download interrupted; retry will resume verified staging")?
    {
        check(cancel)?;
        total += block.len() as u64;
        ensure!(
            total <= ARCHIVE_BYTES,
            "Model download exceeded its pinned length"
        );
        output.write_all(&block)?;
    }
    output.sync_all()?;
    ensure!(
        total == ARCHIVE_BYTES && hash_file_checked(archive, &|| check(cancel))? == ARCHIVE_HASH,
        "Downloaded model integrity check failed; retry the download"
    );
    check(cancel)
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn prepare(
    bundle: &RuntimeBundle,
    archive: &Path,
    destination: &Path,
    cancel: &Cancellation,
) -> Result<()> {
    check(cancel)?;
    let mut command = Command::new(bundle.interpreter());
    command
        .args(["-I", "-B"])
        .arg(bundle.root().join("install_model.py"))
        .arg(archive)
        .arg(destination)
        .arg("--owner")
        .arg(std::process::id().to_string())
        .current_dir(bundle.root())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.env_clear();
    for key in [
        "PATH",
        "SYSTEMROOT",
        "WINDIR",
        "SYSTEMDRIVE",
        "COMSPEC",
        "HOME",
        "USERPROFILE",
        "TEMP",
        "TMP",
        "TMPDIR",
        "APPDATA",
        "LOCALAPPDATA",
        "PROGRAMDATA",
        "PROGRAMFILES",
        "PROGRAMFILES(X86)",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = ChildGuard(
        command
            .spawn()
            .context("Cannot start the verified model installer")?,
    );
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        check(cancel)?;
        if let Some(status) = child.0.try_wait()? {
            ensure!(
                status.success(),
                "Verified model reconstruction failed; retry installation"
            );
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "Model reconstruction exceeded its deadline"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub(crate) fn publish(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            source,
            rustix::fs::CWD,
            destination,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .context("Model destination appeared during installation; nothing was overwritten")?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
        let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        ensure!(
            !source[..source.len() - 1].contains(&0)
                && !destination[..destination.len() - 1].contains(&0),
            "Invalid model publication path"
        );
        // No REPLACE_EXISTING and no std::fs::rename fallback: modern Windows
        // rename can replace an empty directory that appeared after a precheck.
        // SAFETY: both pointers refer to live, NUL-terminated UTF-16 buffers.
        let moved = unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        };
        if moved == 0 {
            return Err(std::io::Error::last_os_error())
                .context("Model destination could not be published without replacement");
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (source, destination);
        anyhow::bail!("Atomic model publication is currently supported on Windows and Linux");
    }
    #[cfg(any(target_os = "linux", windows))]
    Ok(())
}

/// The caller must explicitly request a download. Blocks; use a background task.
/// Unknown directories are never adopted or replaced. A cancelled network fetch
/// retains only the app-owned partial archive for a later explicit retry.
pub fn install_requested(
    bundle: &RuntimeBundle,
    root: &Path,
    cancel: &Cancellation,
) -> Result<PathBuf> {
    check(cancel)?;
    bundle.verify_checked(&|| check(cancel))?;
    let _lease = open_store(root)?;
    let root = root.canonicalize().context("Cannot resolve model store")?;
    let target = root.join("phonon-2.5-en");
    if target.exists() {
        verify_model_checked(&target, &|| check(cancel)).context(
            "Existing model directory is not the pinned installation; it was not changed",
        )?;
        return Ok(target);
    }
    let archive = root.join("phonon-2.5-en.fsq.partial");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(download(&archive, cancel))?;
    let staging = root.join(format!(".preparing-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        prepare(bundle, &archive, &staging, cancel)?;
        check(cancel)?;
        verify_model_checked(&staging, &|| check(cancel))?;
        check(cancel)?;
        publish(&staging, &target)?;
        // The verified installed container is sufficient for offline use. Do
        // not retain a second 158 MB copy, but never remove a changed/unknown
        // cache entry. Cleanup is best effort after the atomic commit point.
        if existing_regular_file(&archive).ok().flatten().is_some()
            && hash_file_checked(&archive, &|| check(cancel))
                .ok()
                .as_deref()
                == Some(ARCHIVE_HASH)
        {
            let _ = fs::remove_file(&archive);
        }
        Ok(target)
    })();
    // Only this attempt's unpredictable staging name is removed, never an old
    // installation, unknown directory, or another process's staging state.
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}
