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
}

#[derive(Deserialize)]
struct Inventory {
    schema: u32,
    #[cfg(windows)]
    target: Option<String>,
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

fn members(root: &Path, directory: &Path, result: &mut BTreeSet<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(!kind.is_symlink(), "Bundle contains a symbolic link");
        if kind.is_dir() {
            members(root, &entry.path(), result)?;
        } else if kind.is_file() {
            result.insert(entry.path().strip_prefix(root)?.to_owned());
        } else {
            bail!("Bundle contains a non-regular member");
        }
    }
    Ok(())
}

impl RuntimeBundle {
    pub fn open(root: impl Into<PathBuf>, manifest_sha256: &str) -> Result<Self> {
        ensure!(
            manifest_sha256.len() == 64 && manifest_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid trusted bundle digest"
        );
        let root = root.into();
        ensure!(
            fs::symlink_metadata(&root)?.is_dir(),
            "Runtime root must be a regular directory"
        );
        let bundle = Self {
            root: root
                .canonicalize()
                .context("Cannot resolve runtime directory")?,
            manifest_sha256: manifest_sha256.to_ascii_lowercase(),
        };
        bundle.verify()?;
        Ok(bundle)
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn interpreter(&self) -> PathBuf {
        self.root.join("python/python.exe")
    }
    pub(crate) fn bridge(&self) -> PathBuf {
        self.root.join("bridge.py")
    }
    pub fn verify(&self) -> Result<()> {
        self.verify_checked(&|| Ok(()))
    }
    pub(crate) fn verify_checked(&self, check: &impl Fn() -> Result<()>) -> Result<()> {
        check()?;
        require_supported_host()?;
        let manifest = self.root.join("bundle.json");
        ensure!(
            fs::symlink_metadata(&manifest)?.is_file(),
            "Bundle inventory must be a regular file"
        );
        ensure!(
            fs::metadata(&manifest)?.len() <= 8 * 1024 * 1024,
            "Bundle inventory is oversized"
        );
        ensure!(
            hash_file_checked(&manifest, check)? == self.manifest_sha256,
            "Bundle inventory does not match the trusted release"
        );
        let inventory: Inventory = serde_json::from_slice(&fs::read(manifest)?)?;
        #[cfg(windows)]
        ensure!(
            inventory.target.as_deref() == Some("windows-x86_64"),
            "Runtime bundle is not declared for Windows x64"
        );
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
            ensure!(
                fs::symlink_metadata(&file)?.is_file(),
                "Bundle member is not a regular file"
            );
            ensure!(
                fs::metadata(&file)?.len() == entry.bytes
                    && hash_file_checked(&file, check)? == entry.sha256,
                "Speech bundle integrity check failed"
            );
        }
        let mut actual = BTreeSet::new();
        members(&self.root, &self.root, &mut actual)?;
        ensure!(
            actual == expected,
            "Speech bundle has missing or unexpected files"
        );
        ensure!(
            self.interpreter().is_file() && self.bridge().is_file(),
            "Speech bundle is incomplete"
        );
        Ok(())
    }
}

fn require_supported_host() -> Result<()> {
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
