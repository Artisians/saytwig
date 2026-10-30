#!/usr/bin/env python3
"""Build native experimental packages with same-source notices; never publish."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import subprocess
import sys
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
RECIPES = {
    "x86_64-unknown-linux-gnu": [],
    "x86_64-pc-windows-msvc": ["dictation-desktop/tray"],
    "aarch64-apple-darwin": [],
    "x86_64-apple-darwin": [],
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
    if ref == "refs/heads/main":
        return "nightly", "nightly-" + commit[:12]
    if re.fullmatch(r"refs/pull/[1-9][0-9]*/merge", ref):
        return "development", "candidate-" + commit[:12]
    match = re.fullmatch(r"refs/tags/saytwig-v((?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*))", ref)
    if not match or match.group(1) != version:
        raise ValueError("Only main, PR merge candidates or exact source-version stable tags can build")
    if git("rev-parse", ref + "^{commit}") != commit:
        raise ValueError("Stable tag does not identify the checked-out commit")
    # Membership is necessary, not a substitute for CONTRIBUTING's human review.
    run("git", "merge-base", "--is-ancestor", commit, "refs/remotes/origin/main")
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
        elif target.endswith("apple-darwin"):
            cpu = b"\x0c\x00\x00\x01" if target.startswith("aarch64") else b"\x07\x00\x00\x01"
            if header[:4] != b"\xcf\xfa\xed\xfe" or header[4:8] != cpu:
                raise ValueError("Expected a matching 64-bit Mach-O executable")
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
    hosts = {
        "x86_64-pc-windows-msvc": ("win32", {"amd64", "x86_64"}),
        "x86_64-unknown-linux-gnu": ("linux", {"x86_64"}),
        "aarch64-apple-darwin": ("darwin", {"arm64"}),
        "x86_64-apple-darwin": ("darwin", {"x86_64"}),
    }
    host_os, machines = hosts[target]
    if sys.platform != host_os or platform.machine().lower() not in machines:
        raise ValueError("Packages must be built on the matching native OS and architecture")
    if sys.platform == "win32" and os.environ.get("PROCESSOR_ARCHITEW6432", "").lower() == "arm64":
        raise ValueError("Windows ARM emulation is not supported")
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
    stem = f"saytwig-{label}-{target}"
    stage = output / "payload"
    stage.mkdir()
    shutil.copy2(executable, stage / executable.name)
    shutil.copytree(licenses, stage / "licenses")
    metadata = {
        "schema_version": 1, "channel": name, "source_version": version,
        "commit": commit, "target": target, "profile": "release",
        "cargo_lock_sha256": lock_hash, "executable_sha256": sha256(executable),
        "app_only": True, "signed": False, "runtime_bundled": False,
        "model_bundled": False, "installer": sys.platform == "win32",
        "ad_hoc_signed": sys.platform == "darwin", "notarized": False,
    }
    files = sorted(p for p in licenses.rglob("*") if p.is_file())
    if any(p.is_symlink() for p in licenses.rglob("*")):
        raise ValueError("License output must not contain symlinks")
    metadata["license_files"] = {p.relative_to(licenses).as_posix(): sha256(p) for p in files}
    notice = (
        "Saytwig experimental build\n\n"
        f"Channel: {name}\nCommit: {commit}\nTarget: {target}\n\n"
        "Not a stable or supported release. No developer signature or notarization.\n"
        "The speech runtime and model are NOT included. Native operation still needs validation.\n"
        "Retain the entire licenses/ directory. Checksums are not signatures.\n"
        f"See https://github.com/Artisians/saytwig/blob/{commit}/docs/release.md\n"
    )
    (stage / "README.txt").write_text(notice, encoding="utf-8")
    if sys.platform == "win32":
        (stage / "BUILD.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
        compiler = Path(os.environ["ProgramFiles(x86)"]) / "Inno Setup 6" / "ISCC.exe"
        archive = output / (stem + "-setup.exe")
        run(str(compiler), f"/DSourceDir={stage}", f"/DOutputDir={output}",
            f"/DOutputName={archive.stem}", f"/DAppVersion={version}",
            str(ROOT / "scripts/release/windows.iss"))
    elif sys.platform == "darwin":
        app = output / "image" / "Saytwig.app" / "Contents"
        (app / "MacOS").mkdir(parents=True)
        shutil.move(stage / "saytwig", app / "MacOS/saytwig")
        binary = app / "MacOS/saytwig"
        binary.chmod(0o755)
        shutil.move(stage, app / "Resources")
        with (app / "Info.plist").open("wb") as stream:
            plistlib.dump({
                "CFBundleName": "Saytwig", "CFBundleDisplayName": "Saytwig",
                "CFBundleIdentifier": "com.saytwig.desktop",
                "CFBundleExecutable": "saytwig", "CFBundlePackageType": "APPL",
                "CFBundleShortVersionString": version, "CFBundleVersion": version,
                "LSMinimumSystemVersion": "13.0", "NSHighResolutionCapable": True,
                "NSMicrophoneUsageDescription": "Saytwig uses your microphone for local speech recognition when you start recording.",
            }, stream)
        run("codesign", "--force", "--sign", "-", str(app.parent))
        run("codesign", "--verify", "--deep", "--strict", str(app.parent))
        metadata["executable_sha256"] = sha256(binary)
        (app.parent.parent / "BUILD.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
        os.symlink("/Applications", app.parent.parent / "Applications")
        archive = output / (stem + ".dmg")
        run("hdiutil", "create", "-volname", "Saytwig", "-srcfolder", str(app.parent.parent),
            "-format", "UDZO", str(archive))
        run("hdiutil", "verify", str(archive))
    else:
        (stage / "BUILD.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
        (stage / "saytwig").chmod(0o755)
        archive = output / (stem + ".tar.gz")
        with tarfile.open(archive, "x:gz") as package:
            package.add(stage, arcname="saytwig")
    if not archive.is_file() or archive.stat().st_size == 0:
        raise ValueError("No native package produced")
    archive.with_name(archive.name + ".sha256").write_text(f"{sha256(archive)}  {archive.name}\n", encoding="utf-8")
    print(f"Experimental package: {archive}")


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
