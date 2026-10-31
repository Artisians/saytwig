//! Integrity boundaries for a bundle provisioned by an explicit installation.
use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

pub const MODEL_HASH: &str = "c57119c0f04756932acc1d60d90506346e3708d69f00e654084acc7fe507fe27";
pub const MODEL_BYTES: u64 = 177_479_389;
pub const ARCHIVE_HASH: &str = "87dacb412f42c736967345a42455c05fa7b1835fd2d0b40ce2fbfbfd415bdadd";
pub const ARCHIVE_BYTES: u64 = 157_616_598;
pub const MODEL_URL: &str = "https://huggingface.co/FermionResearch/Phonon-2.5-English/resolve/da200358d638886bc96c758c672ad5568828b119/phonon-2.5-en.fsq";

/// `manifest_sha256` must come from the application's reviewed release metadata,
/// not from an adjacent mutable file or an untrusted download response.
#[derive(Clone)]
pub struct RuntimeBundle {
    root: PathBuf,
    manifest_sha256: String,
    interpreter: PathBuf,
}

#[derive(Deserialize)]
struct Inventory {
    schema: u32,
    target: Option<String>,
    interpreter: Option<String>,
    files: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    path: String,
    sha256: String,
    bytes: u64,
}

pub(crate) fn hash_file_checked(path: &Path, check: &impl Fn() -> Result<()>) -> Result<String> {
    check()?;
    let mut file = fs::File::open(path).context("Cannot read an installed speech asset")?;
    let mut digest = Sha256::new();
    let mut block = [0; 128 * 1024];
    loop {
        check()?;
        let length = file.read(&mut block)?;
        if length == 0 {
            break;
        }
        digest.update(&block[..length]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn relative_path(path: &str) -> Result<PathBuf> {
    let path = PathBuf::from(path);
    ensure!(
        !path.as_os_str().is_empty()
            && path.components().all(|p| matches!(p, Component::Normal(_))),
        "Invalid bundle member path"
    );
    ensure!(
        !path.to_string_lossy().contains(['\\', ':']),
        "Invalid bundle member path"
    );
    Ok(path)
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
fn members(
    root: &Path,
    directory: &Path,
    result: &mut BTreeSet<PathBuf>,
    expected_files: &BTreeSet<PathBuf>,
    expected_directories: &BTreeSet<PathBuf>,
    check: &impl Fn() -> Result<()>,
) -> Result<()> {
    check()?;
    for entry in fs::read_dir(directory)? {
        check()?;
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root)?;
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            !is_reparse(&metadata),
            "Bundle contains a linked or reparse member"
        );
        if metadata.is_dir() {
            // Refuse an unknown directory before entering it, including empty
            // directories: depth/count are limited to the trusted inventory's
            // ancestor set, not to whatever currently exists on disk.
            ensure!(
                expected_directories.contains(relative),
                "Bundle contains an unexpected directory"
            );
            members(
                root,
                &path,
                result,
                expected_files,
                expected_directories,
                check,
            )?;
        } else if metadata.is_file() {
            ensure!(
                expected_files.contains(relative),
                "Bundle contains an unexpected file"
            );
            ensure!(
                result.insert(relative.to_owned()) && result.len() <= expected_files.len(),
                "Bundle inventory member count is inconsistent"
            );
        } else {
            bail!("Bundle contains a non-regular member");
        }
    }
    Ok(())
}

impl RuntimeBundle {
    pub fn open(root: impl Into<PathBuf>, manifest_sha256: &str) -> Result<Self> {
        Self::open_checked(root, manifest_sha256, &|| Ok(()))
    }
    pub(crate) fn open_checked(
        root: impl Into<PathBuf>,
        manifest_sha256: &str,
        check: &impl Fn() -> Result<()>,
    ) -> Result<Self> {
        check()?;
        ensure!(
            manifest_sha256.len() == 64 && manifest_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid trusted bundle digest"
        );
        let root = root.into();
        let metadata = fs::symlink_metadata(&root)?;
        ensure!(
            metadata.is_dir() && !is_reparse(&metadata),
            "Runtime root must be a regular directory"
        );
        let mut bundle = Self {
            root: root
                .canonicalize()
                .context("Cannot resolve runtime directory")?,
            manifest_sha256: manifest_sha256.to_ascii_lowercase(),
            interpreter: PathBuf::from("python/python.exe"),
        };
        bundle.interpreter = bundle.verified_interpreter_checked(check)?;
        Ok(bundle)
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn interpreter(&self) -> PathBuf {
        self.root.join(&self.interpreter)
    }
    pub(crate) fn bridge(&self) -> PathBuf {
        self.root.join("bridge.py")
    }
    pub fn verify(&self) -> Result<()> {
        self.verify_checked(&|| Ok(()))
    }
    pub(crate) fn verify_checked(&self, check: &impl Fn() -> Result<()>) -> Result<()> {
        ensure!(
            self.verified_interpreter_checked(check)? == self.interpreter,
            "Runtime interpreter changed"
        );
        Ok(())
    }
    fn verified_interpreter_checked(&self, check: &impl Fn() -> Result<()>) -> Result<PathBuf> {
        check()?;
        require_supported_host()?;
        let manifest = self.root.join("bundle.json");
        let metadata = fs::symlink_metadata(&manifest)?;
        ensure!(
            metadata.is_file() && !is_reparse(&metadata),
            "Bundle inventory must be a regular file"
        );
        ensure!(
            fs::metadata(&manifest)?.len() <= 8 * 1024 * 1024,
            "Bundle inventory is oversized"
        );
        // Hash and parse the same bounded snapshot, not separate file opens.
        // A concurrent change must not substitute unchecked inventory bytes.
        let mut file = fs::File::open(&manifest)?;
        let mut bytes = Vec::new();
        let mut block = [0u8; 128 * 1024];
        loop {
            check()?;
            let length = file.read(&mut block)?;
            if length == 0 {
                break;
            }
            ensure!(
                bytes.len() + length <= 8 * 1024 * 1024,
                "Bundle inventory is oversized"
            );
            bytes.extend_from_slice(&block[..length]);
        }
        ensure!(
            hex::encode(Sha256::digest(&bytes)) == self.manifest_sha256,
            "Bundle inventory does not match the trusted release"
        );
        let inventory: Inventory = serde_json::from_slice(&bytes)?;
        #[cfg(windows)]
        ensure!(
            inventory.target.as_deref() == Some("windows-x86_64"),
            "Runtime bundle is not declared for Windows x64"
        );
        #[cfg(not(windows))]
        if let Some(target) = inventory.target.as_deref() {
            ensure!(
                Some(target) == host_target(),
                "Runtime bundle does not match this application target"
            );
        }
        let interpreter = relative_path(
            inventory
                .interpreter
                .as_deref()
                .unwrap_or("python/python.exe"),
        )?;
        ensure!(
            inventory.schema == 1,
            "Unsupported bundle inventory version"
        );
        let mut expected = BTreeSet::from([PathBuf::from("bundle.json")]);
        for entry in inventory.files {
            check()?;
            let relative = relative_path(&entry.path)?;
            ensure!(
                expected.insert(relative.clone()),
                "Duplicate bundle inventory member"
            );
            let file = self.root.join(relative);
            let metadata = fs::symlink_metadata(&file)?;
            ensure!(
                metadata.is_file() && !is_reparse(&metadata),
                "Bundle member is not a regular file"
            );
            ensure!(
                fs::metadata(&file)?.len() == entry.bytes
                    && hash_file_checked(&file, check)? == entry.sha256,
                "Speech bundle integrity check failed"
            );
        }
        let mut expected_directories = BTreeSet::new();
        for file in &expected {
            for ancestor in file
                .ancestors()
                .skip(1)
                .filter(|path| !path.as_os_str().is_empty())
            {
                expected_directories.insert(ancestor.to_owned());
            }
        }
        let mut actual = BTreeSet::new();
        members(
            &self.root,
            &self.root,
            &mut actual,
            &expected,
            &expected_directories,
            check,
        )?;
        ensure!(
            actual == expected,
            "Speech bundle has missing or unexpected files"
        );
        ensure!(
            expected.contains(&interpreter)
                && self.root.join(&interpreter).is_file()
                && expected.contains(Path::new("bridge.py")),
            "Speech bundle is incomplete"
        );
        Ok(interpreter)
    }
}

pub(crate) fn host_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some("windows-x86_64"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        _ => None,
    }
}

pub(crate) fn require_supported_host() -> Result<()> {
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        let mut translated: libc::c_int = 0;
        let mut size = std::mem::size_of_val(&translated);
        // SAFETY: the name is NUL-terminated and both output pointers reference
        // live values with the documented sysctl integer layout.
        let queried = unsafe {
            libc::sysctlbyname(
                c"sysctl.proc_translated".as_ptr(),
                std::ptr::from_mut(&mut translated).cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if queried != 0 {
            ensure!(
                std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT),
                "Cannot determine the native macOS architecture"
            );
        } else {
            ensure!(
                size == std::mem::size_of_val(&translated) && translated == 0,
                "Use the native Apple Silicon application instead of an Intel build under Rosetta"
            );
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::{
            SystemInformation::{IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_UNKNOWN},
            Threading::{GetCurrentProcess, IsWow64Process2},
        };
        let mut process_machine = 0;
        let mut native_machine = 0;
        // SAFETY: the pseudo-handle is valid for this process and both output
        // pointers reference live, correctly sized variables for the call.
        let queried = unsafe {
            IsWow64Process2(
                GetCurrentProcess(),
                &mut process_machine,
                &mut native_machine,
            )
        };
        ensure!(
            queried != 0,
            "Cannot determine the native Windows architecture"
        );
        ensure!(
            native_machine == IMAGE_FILE_MACHINE_AMD64
                && process_machine == IMAGE_FILE_MACHINE_UNKNOWN,
            "Saytwig requires native Windows x64; Windows on ARM and x86 are not supported"
        );
    }
    Ok(())
}

/// Checks all three reconstructed inputs, not just the model's claimed digest.
pub fn verify_model(directory: &Path) -> Result<()> {
    verify_model_checked(directory, &|| Ok(()))
}

pub(crate) fn verify_model_checked(
    directory: &Path,
    check: &impl Fn() -> Result<()>,
) -> Result<()> {
    check()?;
    ensure!(
        fs::symlink_metadata(directory)?.is_dir(),
        "Model root must be a regular directory"
    );
    for (name, digest) in [
        ("model.fermion", MODEL_HASH),
        (
            "config.json",
            "9e5dfcc4e49a6c169d53540f07f4115e591007759b835132c849166b235dac95",
        ),
        (
            "packed_manifest.json",
            "9584d9ad34be1fd5fa1347310fc8c37cd7ee185e10036b2b3615c9f989759ae0",
        ),
    ] {
        let file = directory.join(name);
        ensure!(
            fs::symlink_metadata(&file)?.is_file(),
            "Model input must be a regular file"
        );
        ensure!(
            hash_file_checked(&file, check)? == digest,
            "Phonon model integrity check failed; install the pinned release"
        );
    }
    ensure!(
        fs::metadata(directory.join("model.fermion"))?.len() == MODEL_BYTES,
        "Incorrect model length"
    );
    Ok(())
}
