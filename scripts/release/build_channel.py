#!/usr/bin/env python3
"""Build an app-only channel candidate; no installer, runtime bundle or release mutation."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[2]
RECIPES = {
    "x86_64-unknown-linux-gnu": [],
    "x86_64-pc-windows-msvc": ["dictation-desktop/tray"],
}


def run(*args):
    print("+ " + " ".join(map(str, args)), flush=True)
    return subprocess.run(args, cwd=ROOT, check=True, text=True, capture_output=False)


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def channel(ref, commit, version):
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or git("rev-parse", "HEAD") != commit:
        raise ValueError("Build commit must be the exact checked-out GitHub SHA")
    if ref == "refs/heads/nightly":
        return "nightly", "nightly-" + commit[:12]
    if ref == "refs/heads/stable":
        return "stable-candidate", "stable-candidate-" + commit[:12]
    match = re.fullmatch(r"refs/tags/saytwig-v((?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*))", ref)
    if not match or match.group(1) != version:
        raise ValueError("Only nightly/stable branches or an exact source-version stable tag can build artifacts")
    if git("rev-parse", ref + "^{commit}") != commit:
        raise ValueError("Stable tag does not identify the checked-out commit")
    # Membership is necessary, not a substitute for CONTRIBUTING's human review.
    run("git", "merge-base", "--is-ancestor", commit, "refs/remotes/origin/stable")
    return "stable", "v" + version


def verify_binary(path, target):
    if path.is_symlink() or not path.is_file():
        raise ValueError("Expected a regular native executable")
    with path.open("rb") as binary:
        header = binary.read(64)
        if target.endswith("windows-msvc"):
            if len(header) != 64 or header[:2] != b"MZ":
                raise ValueError("Expected Windows PE executable")
            offset = int.from_bytes(header[60:64], "little")
            if offset > path.stat().st_size - 6:
                raise ValueError("Invalid PE header offset")
            binary.seek(offset)
            if binary.read(6) != b"PE\x00\x00\x64\x86":
                raise ValueError("Only Windows x64 PE artifacts are supported")
        elif len(header) < 20 or header[:6] != b"\x7fELF\x02\x01" or header[18:20] != b"\x3e\x00":
            raise ValueError("Expected a little-endian Linux x64 ELF executable")


def build(target, ref, commit):
    manifest = tomllib.loads((ROOT / "crates/app/Cargo.toml").read_text(encoding="utf-8"))
    version = manifest["package"]["version"]
    if version == {"workspace": True}:
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        version = workspace["workspace"]["package"]["version"]
    if not isinstance(version, str):
        raise ValueError("App version must resolve to a string in the package or workspace")
    name, label = channel(ref, commit, version)
    run("git", "diff", "--quiet", "HEAD", "--")
    if (sys.platform == "win32") != target.endswith("windows-msvc"):
        raise ValueError("Channel artifacts must be built on the matching native OS")
    if sys.platform not in ("win32", "linux"):
        raise ValueError("This pipeline supports only the declared Windows/Linux recipes")
    output = ROOT / "dist" / "channel" / target
    output.mkdir(parents=True, exist_ok=False)
    licenses = output / "licenses"
    lock_hash = sha256(ROOT / "Cargo.lock")
    os.environ["RUSTUP_TOOLCHAIN"] = "1.99.1"
    os.environ["SAYTWIG_BUILD_CHANNEL"] = "stable" if name == "stable" else (
        "nightly" if name == "nightly" else "development"
    )
    os.environ["SAYTWIG_BUILD_COMMIT"] = commit
    tool_bin = ROOT / ".tools" / "cargo-about" / "bin"
    os.environ["PATH"] = str(tool_bin) + os.pathsep + os.environ["PATH"]
    # Fail early on incomplete notices; no old/cloud-generated receipt is reused.
    run(sys.executable, "scripts/release/notices.py", "--target", target, "--output", str(licenses))
    receipt_path = licenses / "READY.json"
    if receipt_path.stat().st_size > 16384:
        raise ValueError("Oversized notices receipt")
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    expected = {
        "schema_version": 1, "target": target, "features": RECIPES[target],
        "cargo_lock_sha256": lock_hash, "cargo_about": "cargo-about 0.9.2",
    }
    if receipt != expected:
        raise ValueError("Notices receipt does not match this source recipe")
    inventory = json.loads((licenses / "INVENTORY.json").read_text(encoding="utf-8"))
    if inventory.get("blockers") != []:
        raise ValueError("Notices inventory still contains blockers")
    run(sys.executable, "scripts/checks/run.py", "all", "--target", target, "--profile", "release")
    run("git", "diff", "--quiet", "HEAD", "--")
    if sha256(ROOT / "Cargo.lock") != lock_hash or git("rev-parse", "HEAD") != commit:
        raise ValueError("Source identity changed during the build")
    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", "target"))
    if not target_dir.is_absolute():
        target_dir = ROOT / target_dir
    executable = target_dir / target / "release" / ("saytwig.exe" if target.endswith("windows-msvc") else "saytwig")
    verify_binary(executable, target)
    metadata = {
        "schema_version": 1, "channel": name, "source_version": version,
        "commit": commit, "target": target, "profile": "release",
        "cargo_lock_sha256": lock_hash, "executable_sha256": sha256(executable),
        "app_only": True, "signed": False, "runtime_bundled": False,
        "model_bundled": False, "installer": False,
    }
    files = sorted(p for p in licenses.rglob("*") if p.is_file())
    if any(p.is_symlink() for p in licenses.rglob("*")):
        raise ValueError("License output must not contain symlinks")
    metadata["license_files"] = {p.relative_to(licenses).as_posix(): sha256(p) for p in files}
    archive = output / f"saytwig-{label}-{target}.zip"
    notice = (
        "Saytwig app-only candidate\n\n"
        f"Channel: {name}\nCommit: {commit}\nTarget: {target}\n\n"
        "Unsigned development/release candidate, not an installer or a supported release.\n"
        "The speech runtime and model are NOT included. Native operation still needs validation.\n"
        "Retain the entire licenses/ directory. Checksums are not signatures.\n"
        f"See https://github.com/Artisians/saytwig/blob/{commit}/docs/release.md\n"
    )
    with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as package:
        package.write(executable, executable.name)
        for path in files:
            package.write(path, "licenses/" + path.relative_to(licenses).as_posix())
        package.writestr("BUILD.json", json.dumps(metadata, indent=2) + "\n")
        package.writestr("README.txt", notice)
    archive.with_suffix(".zip.sha256").write_text(f"{sha256(archive)}  {archive.name}\n", encoding="utf-8")
    print(f"App-only candidate: {archive}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True, choices=RECIPES)
    args = parser.parse_args()
    try:
        build(args.target, os.environ.get("GITHUB_REF", ""), os.environ.get("GITHUB_SHA", ""))
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"Channel artifact blocked: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
