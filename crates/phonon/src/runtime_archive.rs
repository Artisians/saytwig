//! Bounded extraction of pinned portable-Python archives into owned staging.
use anyhow::{ensure, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

fn member(path: &Path) -> Result<()> {
    ensure!(
        path.starts_with("python")
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            && !path.to_string_lossy().contains(['\\', ':', '\0']),
        "Unsafe Python archive path"
    );
    Ok(())
}

fn copy(
    reader: &mut impl Read,
    target: &Path,
    expected: u64,
    total: &mut u64,
    limit: u64,
    check: &impl Fn() -> Result<()>,
) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(target)?;
    let mut block = [0u8; 128 * 1024];
    let mut written = 0u64;
    loop {
        check()?;
        let length = reader.read(&mut block)?;
        if length == 0 {
            break;
        }
        written = written
            .checked_add(length as u64)
            .context("Archive size overflow")?;
        *total = total
            .checked_add(length as u64)
            .context("Archive size overflow")?;
        ensure!(
            written <= expected && *total <= limit,
            "Python archive exceeds its pinned bounds"
        );
        output.write_all(&block[..length])?;
    }
    ensure!(written == expected, "Incomplete Python archive member");
    output.sync_all()?;
    Ok(())
}

pub(crate) fn extract(
    archive: &Path,
    output: &Path,
    entries: usize,
    bytes: u64,
    check: &impl Fn() -> Result<()>,
) -> Result<()> {
    fs::create_dir(output)?;
    let decoder = flate2::read::GzDecoder::new(File::open(archive)?);
    let mut archive = tar::Archive::new(decoder);
    let mut names = BTreeSet::new();
    let mut links = BTreeMap::new();
    let mut total = 0u64;
    for entry in archive.entries()? {
        check()?;
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        member(&path)?;
        ensure!(
            names.insert(path.clone()) && names.len() <= entries,
            "Unexpected Python archive membership"
        );
        let kind = entry.header().entry_type();
        let destination = output.join(&path);
        if kind.is_dir() {
            fs::create_dir_all(destination)?;
        } else if kind.is_file() {
            let expected = entry.size();
            let executable = entry.header().mode()? & 0o111 != 0;
            copy(&mut entry, &destination, expected, &mut total, bytes, check)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(
                    destination,
                    fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
                )?;
            }
            #[cfg(not(unix))]
            let _ = executable;
        } else {
            ensure!(
                kind.is_symlink() || kind.is_hard_link(),
                "Unsupported Python archive member"
            );
            let target = entry
                .link_name()?
                .context("Missing archive link target")?
                .into_owned();
            ensure!(!target.is_absolute(), "Absolute Python archive link");
            let mut resolved = if kind.is_symlink() {
                path.parent().context("Invalid archive link")?.to_path_buf()
            } else {
                PathBuf::new()
            };
            for part in target.components() {
                match part {
                    Component::Normal(name) => resolved.push(name),
                    Component::CurDir => {}
                    Component::ParentDir => {
                        ensure!(
                            resolved.components().count() > 1,
                            "Archive link escapes Python root"
                        );
                        resolved.pop();
                    }
                    _ => anyhow::bail!("Unsafe Python archive link"),
                }
            }
            member(&resolved)?;
            links.insert(path, resolved);
        }
    }
    ensure!(
        names.len() == entries,
        "Python archive membership is incomplete"
    );
    for (path, target) in &links {
        check()?;
        let mut target = target;
        let mut seen = BTreeSet::new();
        while let Some(next) = links.get(target) {
            ensure!(
                seen.insert(target) && seen.len() <= 16,
                "Cyclic Python archive link"
            );
            target = next;
        }
        ensure!(
            names.contains(target),
            "Python archive link is missing its target"
        );
        let source = output.join(target);
        let metadata = fs::symlink_metadata(&source)?;
        ensure!(
            metadata.is_file(),
            "Python archive link does not reference a regular file"
        );
        let destination = output.join(path);
        copy(
            &mut File::open(source)?,
            &destination,
            metadata.len(),
            &mut total,
            bytes,
            check,
        )?;
        fs::set_permissions(destination, metadata.permissions())?;
    }
    ensure!(total == bytes, "Python archive expansion is incomplete");
    Ok(())
}
