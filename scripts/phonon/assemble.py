"""Build-time Windows CPU bundle assembler; never run by the speech service.

Inputs are immutable upstream artifacts in assets.json. The generated manifest
hash must be pinned by the app independently of the mutable bundle directory.
"""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import tempfile
import urllib.request
import zipfile

SOURCE = Path(__file__).resolve().parent


def sha256_file(path: Path) -> str:
    checksum = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1 << 20):
            checksum.update(chunk)
    return checksum.hexdigest()


def asset_matches(path: Path, asset: dict) -> bool:
    return (path.is_file() and not path.is_symlink()
            and path.stat().st_size == asset["bytes"]
            and sha256_file(path) == asset["sha256"])


def fetch_asset(asset: dict, cache: Path) -> Path:
    name = asset["filename"]
    if len(checked_components(name)) != 1:
        raise ValueError("Asset filename must be a basename")
    target = cache / name
    if target.exists():
        if not asset_matches(target, asset):
            raise ValueError(f"Existing cached asset is not the pinned file: {name}")
        return target
    if not asset["url"].startswith("https://"):
        raise ValueError("Asset download requires HTTPS")
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=cache, prefix=".asset-", delete=False) as output:
            temporary = Path(output.name)
            received = 0
            with urllib.request.urlopen(asset["url"], timeout=90) as response:
                if not response.geturl().startswith("https://"):
                    raise ValueError("Refusing non-HTTPS redirect")
                while chunk := response.read(1 << 20):
                    received += len(chunk)
                    if received > asset["bytes"]:
                        raise ValueError(f"Asset exceeds pinned size: {name}")
                    output.write(chunk)
        if not asset_matches(temporary, asset):
            raise ValueError(f"Downloaded asset does not match its pin: {name}")
        # No replacement: a concurrent builder cannot overwrite another file.
        os.link(temporary, target)
        return target
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def checked_components(member: str) -> tuple[str, ...]:
    if not member or member.startswith("/") or "\\" in member or "\0" in member:
        raise ValueError("Unsafe archive member")
    parts = tuple(member.rstrip("/").split("/"))
    if any(part in ("", ".", "..") or ":" in part for part in parts):
        raise ValueError("Unsafe archive member")
    return parts


def extract_zip(archive_path: Path, python: Path, wheel: bool = False) -> None:
    with zipfile.ZipFile(archive_path) as archive:
        for entry in archive.infolist():
            parts = checked_components(entry.filename)
            if stat.S_ISLNK(entry.external_attr >> 16):
                raise ValueError("Symlinks are not permitted in runtime archives")
            base = python / "Lib/site-packages" if wheel else python
            if wheel and parts[0].endswith(".data"):
                if len(parts) < 3:
                    if entry.is_dir():
                        continue
                    raise ValueError("Malformed wheel installation scheme")
                scheme = parts[1]
                if scheme == "scripts":
                    continue  # The application owns the only entrypoint.
                if scheme in ("purelib", "platlib"):
                    parts = parts[2:]
                elif scheme in ("headers", "data"):
                    base, parts = python, parts[2:]
                else:
                    raise ValueError("Unsupported wheel installation scheme")
            destination = base.joinpath(*parts)
            if entry.is_dir():
                destination.mkdir(parents=True, exist_ok=True)
            else:
                destination.parent.mkdir(parents=True, exist_ok=True)
                with destination.open("xb") as output, archive.open(entry) as source:
                    shutil.copyfileobj(source, output)


def inventory(root: Path) -> list[dict]:
    rows = []
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise ValueError("Bundle must not contain symlinks")
        if path.is_file() and path != root / "bundle.json":
            rows.append({"path": path.relative_to(root).as_posix(),
                         "sha256": sha256_file(path), "bytes": path.stat().st_size})
    return rows


def omit_link_inputs(root: Path) -> list[dict]:
    rows = []
    directory = root / "python/Lib/site-packages/torch/lib"
    for path in sorted(directory.glob("*.lib")):
        if path.is_symlink() or not path.is_file():
            raise ValueError("Unexpected linker input type")
        rows.append({"path": path.relative_to(root).as_posix(),
                     "sha256": sha256_file(path), "bytes": path.stat().st_size,
                     "reason": "Windows C++ linker input; inference uses prebuilt DLLs"})
        path.unlink()
    return rows


def check_bundle(root: Path) -> dict:
    manifest = json.loads((root / "bundle.json").read_text(encoding="utf-8"))
    if manifest.get("schema") != 1 or manifest.get("files") != inventory(root):
        raise ValueError("Bundle inventory mismatch")
    return manifest


def assemble(cache: Path, destination: Path, source: Path = SOURCE) -> dict:
    if destination.exists():
        raise FileExistsError("Choose a new bundle directory; existing files will not be replaced")
    assets = json.loads((source / "assets.json").read_text(encoding="utf-8"))
    if assets.get("schema") != 1:
        raise ValueError("Unsupported asset schema")
    required = [assets["python"], *assets["wheels"], *assets["notices"]]
    cache.mkdir(parents=True, exist_ok=True)
    with ThreadPoolExecutor(max_workers=6) as pool:
        list(pool.map(lambda row: fetch_asset(row, cache), required))
    # Fail before creating a partial bundle if integration sources are absent.
    for name in ("bridge.py", "install_model.py", "assets.json", "NOTICES.md"):
        if not (source / name).is_file():
            raise FileNotFoundError(f"Missing integration source: {name}")
    destination.mkdir(parents=True, exist_ok=False)
    interpreter = destination / "python"
    interpreter.mkdir()
    extract_zip(cache / assets["python"]["filename"], interpreter)
    pth_name = "python" + "".join(assets["python"]["version"].split(".")[:2])
    # No `import site`, registry, PYTHONPATH, user site or executable .pth hooks.
    (interpreter / (pth_name + "._pth")).write_text(
        pth_name + ".zip\n.\nLib/site-packages\n", encoding="utf-8")
    for wheel in assets["wheels"]:
        extract_zip(cache / wheel["filename"], interpreter, wheel=True)
    omitted = omit_link_inputs(destination)
    for name in ("bridge.py", "install_model.py", "assets.json", "NOTICES.md"):
        shutil.copyfile(source / name, destination / name)
    notices = destination / "notices"
    notices.mkdir()
    for row in assets["notices"]:
        shutil.copyfile(cache / row["filename"], notices / row["filename"])
    app_license = source.parents[1] / "LICENSE"
    if app_license.is_file():
        shutil.copyfile(app_license, notices / "APPLICATION-LICENSE")
    manifest = {"schema": 1, "files": inventory(destination), "omitted": omitted,
                "runtime_version": "0.2.15", "python_version": assets["python"]["version"],
                "contains_model_weights": False}
    (destination / "bundle.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    check_bundle(destination)
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    manifest = check_bundle(args.output) if args.verify else assemble(args.cache, args.output)
    print(json.dumps({"manifest_sha256": sha256_file(args.output / "bundle.json"),
                      "files": len(manifest["files"]),
                      "payload_bytes": sum(row["bytes"] for row in manifest["files"]),
                      "omitted_bytes": sum(row["bytes"] for row in manifest["omitted"])}))


if __name__ == "__main__":
    main()
