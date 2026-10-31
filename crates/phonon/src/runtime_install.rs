//! Explicit Windows x64 first-run setup from immutable upstream inputs.
//! This module is not an updater and never replaces an existing installation.
use crate::{
    install::publish,
    integrity::{hash_file_checked, require_supported_host},
    Cancellation, RuntimeBundle,
};
use anyhow::{ensure, Context, Result};
use fs2::FileExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    future::Future,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

// Reviewed from the pinned upstream inputs and retained full-text notices.
// An adjacent mutable manifest/digest is never promoted to this trust anchor.
pub const MANAGED_RUNTIME_SHA256: &str =
    "291c56eed6455f85b2f4a57e00d74925c058e2bff593f79fdd69969f59fea83c";
const VERSION_DIRECTORY: &str = "windows-x64-291c56eed6455f85";
const OWNER: &[u8] = b"{\"schema\":1,\"purpose\":\"saytwig-managed-runtime\",\"inventory\":\"291c56eed6455f85b2f4a57e00d74925c058e2bff593f79fdd69969f59fea83c\"}\n";
// Canonical source-input fingerprint, reviewed separately from mutable staging.
const SOURCE_SHA256: &str = "35ce1e4538735f06f2587f785e92eefd4790add8bc03061c6d986dc7365ed5f8";
const SOURCES: &[(&str, &[u8])] = &[
    (
        "scripts/phonon/assemble.py",
        include_bytes!("../../../scripts/phonon/assemble.py"),
    ),
    (
        "scripts/phonon/bridge.py",
        include_bytes!("../../../scripts/phonon/bridge.py"),
    ),
    (
        "scripts/phonon/install_model.py",
        include_bytes!("../../../scripts/phonon/install_model.py"),
    ),
    (
        "scripts/phonon/assets.json",
        include_bytes!("../../../scripts/phonon/assets.json"),
    ),
    (
        "scripts/phonon/NOTICES.md",
        include_bytes!("../../../scripts/phonon/NOTICES.md"),
    ),
    ("LICENSE", include_bytes!("../../../LICENSE")),
];
const ASSETS: &[u8] = include_bytes!("../../../scripts/phonon/assets.json");
// Exact central-directory sums from the pinned Python archive + 42 wheels.
const UNPACKED_UPPER_BYTES: u64 = 1_524_568_002;
const ARCHIVE_ENTRY_COUNT: u64 = 21_208;
const BOOTSTRAP_BYTES: u64 = 22_640_669;
const BOOTSTRAP_ENTRIES: usize = 34;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupProgress {
    Downloading {
        completed_assets: usize,
        total_assets: usize,
    },
    Preparing,
    Verifying,
    Ready,
}
/// Sanitized actionable setup failures; no paths, URLs or private payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupFailure {
    UnsupportedHost,
    InsufficientSpace,
    RecoveryRequired,
    BootstrapUnavailable,
}
impl std::fmt::Display for SetupFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedHost => "Managed setup requires a verified native Windows x64 host",
            Self::InsufficientSpace => "Not enough free space to prepare local speech safely",
            Self::RecoveryRequired => "Retained runtime setup files require explicit review; nothing was replaced",
            Self::BootstrapUnavailable => "Embedded Python could not start; check file access and the official Microsoft Visual C++ x64 prerequisite",
        })
    }
}
impl std::error::Error for SetupFailure {}
#[derive(Deserialize)]
struct Asset {
    filename: String,
    url: String,
    sha256: String,
    bytes: u64,
}
#[derive(Deserialize)]
struct AssetLock {
    schema: u32,
    target: String,
    python: Asset,
    wheels: Vec<Asset>,
    notices: Vec<Asset>,
}
impl AssetLock {
    fn all(&self) -> impl Iterator<Item = &Asset> {
        std::iter::once(&self.python)
            .chain(&self.wheels)
            .chain(&self.notices)
    }
    fn read() -> Result<Self> {
        let lock: Self = serde_json::from_slice(ASSETS)?;
        ensure!(
            lock.schema == 1
                && lock.target == "windows-x86_64"
                && lock.python.filename.ends_with("-embed-amd64.zip"),
            "Unsupported runtime asset lock"
        );
        let mut names = BTreeSet::new();
        for asset in lock.all() {
            ensure!(
                Path::new(&asset.filename).components().count() == 1
                    && matches!(
                        Path::new(&asset.filename).components().next(),
                        Some(Component::Normal(_))
                    )
                    && !asset.filename.contains(['\\', ':', '\0'])
                    && names.insert(&asset.filename)
                    && asset.bytes > 0
                    && asset.sha256.len() == 64
                    && asset.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "Invalid embedded runtime asset metadata"
            );
            let url = reqwest::Url::parse(&asset.url)?;
            ensure!(
                url.scheme() == "https" && url.username().is_empty() && url.password().is_none(),
                "Runtime assets require HTTPS"
            );
        }
        Ok(lock)
    }
}
struct Setup<'a> {
    cancellation: &'a Cancellation,
    deadline: Instant,
}
fn check(setup: &Setup<'_>) -> Result<()> {
    ensure!(
        !setup.cancellation.is_cancelled(),
        "Local speech setup cancelled"
    );
    ensure!(Instant::now() < setup.deadline, "Local speech setup exceeded its 30-minute deadline; verified downloads are retained for retry");
    Ok(())
}
fn canonical_source(bytes: &[u8]) -> Vec<u8> {
    // Git may use CRLF on Windows. Only newly authored integration text is
    // canonicalized; upstream archive/license bytes remain untouched.
    bytes
        .iter()
        .enumerate()
        .filter_map(|(index, byte)| {
            if *byte == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
                None
            } else {
                Some(*byte)
            }
        })
        .collect()
}
fn verify_sources() -> Result<()> {
    let mut hash = Sha256::new();
    for (name, bytes) in SOURCES {
        hash.update(name.as_bytes());
        hash.update([0]);
        hash.update(canonical_source(bytes));
        hash.update([0]);
    }
    ensure!(
        hex::encode(hash.finalize()) == SOURCE_SHA256,
        "Runtime integration sources changed without reviewed installation metadata"
    );
    Ok(())
}
fn regular(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() && !is_reparse(&metadata),
                "A managed runtime path is not a regular file"
            );
            Ok(Some(metadata))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
fn is_reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
fn directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && !is_reparse(&metadata),
        "A managed runtime path is not a regular directory"
    );
    Ok(())
}
fn reject_unsafe_ancestors(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute()
            && !path
                .components()
                .any(|part| matches!(part, Component::ParentDir | Component::CurDir)),
        "Managed runtime setup requires an absolute normalized path"
    );
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => ensure!(
                metadata.is_dir() && !is_reparse(&metadata),
                "Managed runtime setup does not follow linked or non-directory ancestors"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
fn version_root(root: &Path) -> PathBuf {
    root.join(VERSION_DIRECTORY)
}

/// Discovery never downloads. The caller may use an explicit development bundle
/// instead. Full pre-execution verification still occurs in the speech actor.
pub fn managed_runtime(root: &Path) -> Result<Option<RuntimeBundle>> {
    if !cfg!(windows) {
        return Ok(None);
    }
    require_supported_host()?;
    verify_sources()?;
    reject_unsafe_ancestors(root)?;
    let path = version_root(root).join("bundle");
    reject_unsafe_ancestors(&path)?;
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
        Ok(_) => RuntimeBundle::open(path, MANAGED_RUNTIME_SHA256).map(Some),
    }
}

fn open_store(root: &Path) -> Result<File> {
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir_all(root)?,
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    directory(root)?;
    let owner = root.join("store.json");
    if regular(&owner)?.is_none() {
        ensure!(
            fs::read_dir(root)?.next().is_none(),
            "Refusing to adopt a nonempty unknown runtime directory"
        );
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&owner)?;
        file.write_all(OWNER)?;
        file.sync_all()?;
    }
    regular(&owner)?.context("Missing runtime ownership record")?;
    let mut marker = Vec::with_capacity(OWNER.len() + 1);
    File::open(&owner)?
        .take(OWNER.len() as u64 + 1)
        .read_to_end(&mut marker)?;
    ensure!(marker == OWNER, "Unrecognized runtime ownership record");
    let lock = root.join("installation.lock");
    regular(&lock)?;
    let lease = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock)?;
    lease
        .try_lock_exclusive()
        .context("Another local speech setup is in progress")?;
    Ok(lease)
}

fn cache_path(cache: &Path, asset: &Asset, partial: bool) -> PathBuf {
    cache.join(if partial {
        format!("{}.partial", asset.filename)
    } else {
        asset.filename.clone()
    })
}
fn verified_asset(path: &Path, asset: &Asset, cancel: &Setup<'_>) -> Result<bool> {
    Ok(
        regular(path)?.is_some_and(|metadata| metadata.len() == asset.bytes)
            && hash_file_checked(path, &|| check(cancel))? == asset.sha256,
    )
}
fn prepare_cache(cache: &Path, lock: &AssetLock) -> Result<()> {
    match fs::symlink_metadata(cache) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(cache)?,
        Err(error) => return Err(error.into()),
        Ok(_) => directory(cache)?,
    }
    let expected: BTreeSet<_> = lock
        .all()
        .flat_map(|asset| {
            [
                asset.filename.clone(),
                format!("{}.partial", asset.filename),
            ]
        })
        .collect();
    for entry in fs::read_dir(cache)? {
        let entry = entry?;
        ensure!(
            entry.file_type()?.is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| expected.contains(name)),
            "Unknown files in runtime download cache were not changed"
        );
    }
    Ok(())
}
fn check_space(root: &Path, lock: &AssetLock, cache: &Path) -> Result<()> {
    let mut remaining = 0u64;
    for asset in lock.all() {
        if regular(&cache_path(cache, asset, false))?.is_none() {
            let partial = regular(&cache_path(cache, asset, true))?
                .map(|m| m.len())
                .unwrap_or_default();
            remaining = remaining
                .checked_add(asset.bytes.saturating_sub(partial))
                .context("Runtime size overflow")?;
        }
    }
    // Upper bound includes link-time .lib inputs before intentional omission,
    // bootstrap files and one allocation unit per archive entry/directory.
    // Free-space checks are advisory: every subsequent write remains fallible.
    let allocation = fs2::allocation_granularity(root)?;
    let overhead = allocation
        .checked_mul(ARCHIVE_ENTRY_COUNT * 2 + BOOTSTRAP_ENTRIES as u64 + 128)
        .context("Filesystem allocation estimate overflow")?;
    let needed = remaining
        .checked_add(UNPACKED_UPPER_BYTES)
        .and_then(|n| n.checked_add(BOOTSTRAP_BYTES))
        .and_then(|n| n.checked_add(overhead))
        .context("Runtime size overflow")?;
    ensure!(
        fs2::available_space(root)? >= needed,
        SetupFailure::InsufficientSpace
    );
    Ok(())
}
async fn interruptible<F: Future>(future: F, cancel: &Setup<'_>) -> Result<F::Output> {
    tokio::pin!(future);
    loop {
        check(cancel)?;
        tokio::select! { value = &mut future => return Ok(value), _ = tokio::time::sleep(Duration::from_millis(50)) => {} }
    }
}
async fn fetch(
    client: &reqwest::Client,
    cache: &Path,
    asset: &Asset,
    cancel: &Setup<'_>,
) -> Result<()> {
    let target = cache_path(cache, asset, false);
    if regular(&target)?.is_some() {
        ensure!(
            verified_asset(&target, asset, cancel)?,
            "Existing runtime asset is not the pinned file; it was not replaced"
        );
        return Ok(());
    }
    let partial = cache_path(cache, asset, true);
    let mut offset = regular(&partial)?.map(|m| m.len()).unwrap_or_default();
    ensure!(offset <= asset.bytes, SetupFailure::RecoveryRequired);
    if offset == asset.bytes {
        ensure!(
            verified_asset(&partial, asset, cancel)?,
            SetupFailure::RecoveryRequired
        );
    } else {
        let mut request = client
            .get(&asset.url)
            .header(reqwest::header::ACCEPT_ENCODING, "identity");
        if offset != 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }
        let mut response = interruptible(request.send(), cancel).await??;
        if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            ensure!(
                response
                    .headers()
                    .get(reqwest::header::CONTENT_RANGE)
                    .and_then(|v| v.to_str().ok())
                    == Some(format!("bytes {offset}-{}/{}", asset.bytes - 1, asset.bytes).as_str()),
                "Unexpected runtime download range"
            );
        } else {
            ensure!(
                response.status() == reqwest::StatusCode::OK,
                "Runtime asset download failed"
            );
            offset = 0;
        }
        if let Some(length) = response.content_length() {
            ensure!(
                length == asset.bytes - offset,
                "Unexpected runtime asset length"
            );
        }
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(offset == 0)
            .append(offset != 0)
            .open(&partial)?;
        let mut received = offset;
        while let Some(bytes) = interruptible(response.chunk(), cancel).await?? {
            received = received
                .checked_add(bytes.len() as u64)
                .context("Runtime download overflow")?;
            ensure!(
                received <= asset.bytes,
                "Runtime download exceeded its pinned size"
            );
            file.write_all(&bytes)?;
        }
        file.sync_all()?;
        ensure!(
            verified_asset(&partial, asset, cancel)?,
            SetupFailure::RecoveryRequired
        );
    }
    check(cancel)?;
    // Atomic no-replacement publication; an unexpected destination is retained.
    publish(&partial, &target)?;
    Ok(())
}

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        // Only this call's successfully created staging directory is owned here.
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn extract_bootstrap(archive: &Path, output: &Path, cancel: &Setup<'_>) -> Result<()> {
    fs::create_dir(output)?;
    let mut archive = zip::ZipArchive::new(File::open(archive)?)?;
    ensure!(
        archive.len() == BOOTSTRAP_ENTRIES,
        "Unexpected pinned Python archive layout"
    );
    let mut total = 0u64;
    let mut block = [0u8; 128 * 1024];
    for index in 0..archive.len() {
        check(cancel)?;
        let mut entry = archive.by_index(index)?;
        let relative = entry
            .enclosed_name()
            .context("Unsafe Python archive path")?;
        ensure!(
            !entry.is_symlink()
                && relative
                    .components()
                    .all(|c| matches!(c, Component::Normal(_)))
                && !relative.to_string_lossy().contains(['\\', ':', '\0']),
            "Unsupported Python archive member"
        );
        let path = output.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let expected = entry.size();
        let mut written = 0u64;
        let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
        loop {
            check(cancel)?;
            let length = entry.read(&mut block)?;
            if length == 0 {
                break;
            }
            written = written
                .checked_add(length as u64)
                .context("Python archive overflow")?;
            total = total
                .checked_add(length as u64)
                .context("Python archive overflow")?;
            ensure!(
                written <= expected && total <= BOOTSTRAP_BYTES,
                "Python archive exceeds its pinned expansion size"
            );
            file.write_all(&block[..length])?;
        }
        ensure!(written == expected, "Python archive member is incomplete");
        file.sync_all()?;
    }
    ensure!(
        total == BOOTSTRAP_BYTES,
        "Python archive expansion is incomplete"
    );
    Ok(())
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn assemble(stage: &Path, cache: &Path, cancel: &Setup<'_>) -> Result<PathBuf> {
    let source = stage.join("source");
    for (name, bytes) in SOURCES {
        let path = source.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
        file.write_all(&canonical_source(bytes))?;
    }
    let output = stage.join("bundle");
    let python = stage.join("python/python.exe");
    let mut command = Command::new(&python);
    command
        .args(["-I", "-B"])
        .arg(source.join("scripts/phonon/assemble.py"))
        .arg("--cache")
        .arg(cache)
        .arg("--output")
        .arg(&output)
        .arg("--offline")
        .arg("--owner-pid")
        .arg(std::process::id().to_string())
        .current_dir(stage)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env_clear();
    for name in ["SYSTEMROOT", "WINDIR", "SYSTEMDRIVE", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command.env("PYTHONDONTWRITEBYTECODE", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    check(cancel)?;
    let mut child = OwnedChild(
        command
            .spawn()
            .map_err(|_| SetupFailure::BootstrapUnavailable)?,
    );
    let deadline = Instant::now() + Duration::from_secs(1800);
    loop {
        check(cancel)?;
        if let Some(status) = child.0.try_wait()? {
            ensure!(
                status.success(),
                "Offline runtime preparation failed; no installation was published"
            );
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "Offline runtime preparation exceeded its deadline"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(output)
}

/// Explicit user-action boundary. Downloads only the compiled upstream lock;
/// never invokes pip, a system Python, a shell or a system prerequisite installer.
/// Progress callbacks run on this background caller and must not block the UI.
pub fn install_runtime_requested(
    root: &Path,
    cancel: &Cancellation,
    progress: impl Fn(SetupProgress),
) -> Result<RuntimeBundle> {
    ensure!(cfg!(windows), SetupFailure::UnsupportedHost);
    require_supported_host().map_err(|_| SetupFailure::UnsupportedHost)?;
    verify_sources()?;
    let setup = Setup {
        cancellation: cancel,
        deadline: Instant::now() + Duration::from_secs(1800),
    };
    let cancel = &setup;
    check(cancel)?;
    reject_unsafe_ancestors(root)?;
    let root = version_root(root);
    let _lease = open_store(&root)?;
    let root = root.canonicalize()?;
    let target = root.join("bundle");
    match fs::symlink_metadata(&target) {
        Ok(_) => {
            let bundle =
                RuntimeBundle::open_checked(&target, MANAGED_RUNTIME_SHA256, &|| check(cancel))?;
            progress(SetupProgress::Ready);
            return Ok(bundle);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let lock = AssetLock::read()?;
    let cache = root.join("downloads");
    prepare_cache(&cache, &lock)?;
    let stage = root.join(".preparing");
    // A process crash leaves one fixed staging location, never another UUID
    // directory per retry. Ambiguous old contents require explicit recovery.
    ensure!(
        matches!(fs::symlink_metadata(&stage), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
        SetupFailure::RecoveryRequired
    );
    check_space(&root, &lock, &cache)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let client = reqwest::Client::builder()
            .https_only(true)
            .no_proxy()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(1800))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()?;
        let total_assets = lock.all().count();
        for (index, asset) in lock.all().enumerate() {
            progress(SetupProgress::Downloading {
                completed_assets: index,
                total_assets,
            });
            fetch(&client, &cache, asset, cancel).await?;
        }
        Ok::<_, anyhow::Error>(())
    })?;
    check(cancel)?;
    // Recheck after potentially long downloads and concurrent disk use.
    check_space(&root, &lock, &cache)?;
    fs::create_dir(&stage)?;
    let staging = Staging(stage);
    progress(SetupProgress::Preparing);
    let bootstrap = cache_path(&cache, &lock.python, false);
    ensure!(
        verified_asset(&bootstrap, &lock.python, cancel)?,
        "Pinned Python bootstrap changed before extraction"
    );
    extract_bootstrap(&bootstrap, &staging.0.join("python"), cancel)?;
    let output = assemble(&staging.0, &cache, cancel)?;
    progress(SetupProgress::Verifying);
    check(cancel)?;
    RuntimeBundle::open_checked(&output, MANAGED_RUNTIME_SHA256, &|| check(cancel))?;
    check(cancel)?;
    publish(&output, &target)?;
    let bundle = RuntimeBundle::open_checked(&target, MANAGED_RUNTIME_SHA256, &|| Ok(()))?;
    // Successful installation no longer needs compressed cache duplicates. Only
    // exact pinned regular files are removed; changed/unrecognized data stays.
    for asset in lock.all() {
        let path = cache_path(&cache, asset, false);
        if verified_asset(&path, asset, cancel).unwrap_or(false) {
            let _ = fs::remove_file(path);
        }
    }
    progress(SetupProgress::Ready);
    Ok(bundle)
}
