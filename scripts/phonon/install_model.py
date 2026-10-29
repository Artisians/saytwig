"""Reconstruct only the pinned model from a locally supplied, verified FSQ file.

No downloads, package installation or replacement of existing directories.
The caller supplies a fresh staging destination and publishes it separately.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
from pathlib import Path
import sys
import threading
import time


def private_environment() -> None:
    keep = {"PATH", "SYSTEMROOT", "WINDIR", "COMSPEC", "SYSTEMDRIVE",
            "TEMP", "TMP", "TMPDIR", "HOME", "USERPROFILE", "LOCALAPPDATA",
            "APPDATA", "PROGRAMDATA", "PROGRAMFILES", "PROGRAMFILES(X86)",
            "LANG", "LC_ALL", "LC_CTYPE", "TZ"}
    for key in tuple(os.environ):
        if key.upper() not in keep:
            del os.environ[key]
    os.environ.update(HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1",
                      HF_HUB_DISABLE_TELEMETRY="1", DO_NOT_TRACK="1")


def prohibit_network(event, _arguments) -> None:
    if event in ("socket.connect", "socket.getaddrinfo"):
        raise PermissionError("Model reconstruction is offline")


def follow_owner(owner: int) -> None:
    if owner < 1:
        raise ValueError("Invalid owner process")
    if os.name == "nt":
        import ctypes
        from ctypes import wintypes
        api = ctypes.WinDLL("kernel32", use_last_error=True)
        api.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
        api.OpenProcess.restype = wintypes.HANDLE
        api.WaitForSingleObject.argtypes = (wintypes.HANDLE, wintypes.DWORD)
        api.WaitForSingleObject.restype = wintypes.DWORD
        handle = api.OpenProcess(0x00100000, False, owner)
        if not handle:
            raise RuntimeError("Owner is unavailable")
        def guard():
            api.WaitForSingleObject(handle, 0xFFFFFFFF)
            os._exit(23)
    else:
        if os.getppid() != owner:
            raise RuntimeError("Owner must be the parent process")
        def guard():
            while os.getppid() == owner:
                time.sleep(0.2)
            os._exit(23)
    threading.Thread(target=guard, daemon=True, name="model-owner").start()


def digest(path: Path) -> str:
    with path.open("rb") as data:
        return hashlib.file_digest(data, "sha256").hexdigest()


def reconstruct(archive: Path, destination: Path, pin: dict) -> None:
    expected_names = {"config.json", "packed_manifest.json", "model.fermion"}
    if len(pin.get("members", [])) != 3 or {row["path"] for row in pin["members"]} != expected_names:
        raise ValueError("Unexpected model member contract")
    # Upstream unpacker uses this sibling name. Do not allow its recovery logic
    # to remove someone else's directory or follow an existing symlink.
    partial = destination.with_name(destination.name + ".partial")
    if os.path.lexists(destination) or os.path.lexists(partial):
        raise FileExistsError("Model destination or unpack staging already exists")
    if archive.is_symlink() or not archive.is_file():
        raise ValueError("Model archive must be a regular file")
    if archive.stat().st_size != pin["bytes"] or digest(archive) != pin["sha256"]:
        raise ValueError("Model archive does not match its immutable pin")
    # Reserve our staging directory exclusively. Upstream may replace its own
    # destination, so give it a private child that no pre-existing caller files
    # can occupy. A failed run leaves this owned staging for the caller to clean.
    destination.mkdir(exist_ok=False)
    reconstruction = destination / ".reconstruction"
    from fermion._speech.fetch import _unpack_fsq
    _unpack_fsq(archive, reconstruction)
    for member in pin["members"]:
        path = reconstruction / member["path"]
        if path.is_symlink() or not path.is_file() or path.stat().st_size != member["bytes"]:
            raise ValueError("Reconstructed model member size mismatch")
        if digest(path) != member["sha256"]:
            raise ValueError("Reconstructed model member hash mismatch")
    manifest = json.loads((reconstruction / "packed_manifest.json").read_text(encoding="utf-8"))
    if manifest.get("container_sha256") != pin["container_sha256"]:
        raise ValueError("Reconstructed model declares a different container")
    expected = {member["path"] for member in pin["members"]}
    if {path.name for path in reconstruction.iterdir()} != expected:
        raise ValueError("Unexpected reconstructed model members")
    for member in pin["members"]:
        source = reconstruction / member["path"]
        with source.open("rb") as input_file, (destination / member["path"]).open("xb") as output:
            shutil.copyfileobj(input_file, output)
    # Remove only the files we created and verified, never arbitrary contents.
    for member in pin["members"]:
        (reconstruction / member["path"]).unlink()
    reconstruction.rmdir()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--owner", type=int)
    args = parser.parse_args()
    if args.owner is not None:
        follow_owner(args.owner)
    private_environment()
    sys.dont_write_bytecode = True
    sys.addaudithook(prohibit_network)
    pin = json.loads((Path(__file__).resolve().parent / "assets.json").read_text(encoding="utf-8"))["model"]
    # Do not resolve destination symlinks before the existence check.
    reconstruct(args.archive.absolute(), args.destination.absolute(), pin)
    print(json.dumps({"status": "ready", "model": pin["repository"],
                      "container_sha256": pin["container_sha256"]}))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        # No file contents, paths, credentials or upstream response bodies.
        print("Pinned local model reconstruction failed; verify the download and use a fresh staging path.", file=sys.stderr)
        raise SystemExit(1)
