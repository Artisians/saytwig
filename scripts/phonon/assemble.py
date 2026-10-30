"""Build-time Windows CPU bundle assembler; never run by the speech service.

Inputs are immutable upstream artifacts in assets.json. The generated manifest
hash must be pinned by the app independently of the mutable bundle directory.
"""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
from email.parser import BytesParser
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
TARGET = "windows-x86_64"


def require_x64_assets(assets: dict) -> None:
    if assets.get("target") != TARGET or not assets["python"]["filename"].endswith("-embed-amd64.zip"):
        raise ValueError("Only the pinned Windows x64 runtime may be assembled")
    for wheel in assets["wheels"]:
        # Pure-Python any wheels are portable; every native wheel must be x64.
        platform_tags = wheel["filename"].removesuffix(".whl").rsplit("-", 1)[-1].split(".")
        if not set(platform_tags) <= {"any", "win_amd64"}:
            raise ValueError("Runtime lock contains a non-Windows-x64 wheel")


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


def write_notice_index(root: Path, assets: dict, cache: Path) -> None:
    """Index complete retained upstream texts, including bundled-library notices.

    This is not a legal certification or a substitute for source/relinking
    obligations associated with LGPL and other vendor components.
    """
    packages = []
    for wheel in assets["wheels"]:
        with zipfile.ZipFile(cache / wheel["filename"]) as archive:
            metadata_name = next(name for name in archive.namelist() if name.endswith(".dist-info/METADATA"))
            metadata = BytesParser().parsebytes(archive.read(metadata_name))
            texts = []
            for name in archive.namelist():
                if name.endswith("/") or not any(term in name.rsplit("/", 1)[-1].upper() for term in ("LICENSE", "LICENCE", "COPYING", "NOTICE", "AUTHORS", "COPYRIGHT")):
                    continue
                parts = checked_components(name)
                if parts[0].endswith(".data"):
                    raise ValueError("Review relocated wheel license paths before packaging")
                path = root / "python/Lib/site-packages" / name
                if not path.is_file() or path.is_symlink() or path.read_bytes() != archive.read(name):
                    raise ValueError("Upstream license text was not preserved verbatim")
                texts.append({"path": path.relative_to(root).as_posix(), "sha256": sha256_file(path), "bytes": path.stat().st_size})
            if not texts and wheel["name"] == "tokenizers":
                path = root / "notices/TOKENIZERS-LICENSE"
                pin = next(row for row in assets["notices"] if row["filename"] == "TOKENIZERS-LICENSE")
                if not asset_matches(path, pin):
                    raise ValueError("Missing independently pinned tokenizers license")
                texts.append({"path": path.relative_to(root).as_posix(), "sha256": pin["sha256"], "bytes": pin["bytes"]})
            if not texts:
                raise ValueError("A runtime dependency has no retained license text")
            packages.append({"name": wheel["name"], "version": wheel["version"],
                             "upstream_wheel_url": wheel["url"], "wheel_sha256": wheel["sha256"],
                             "license_expression": metadata.get("License-Expression"),
                             "license_classifiers": [value for value in metadata.get_all("Classifier", []) if value.startswith("License ::")],
                             "metadata_path": "python/Lib/site-packages/" + metadata_name,
                             "retained_texts": texts})
    notice_index = {"schema": 1, "target": TARGET, "packages": packages,
                    "scope": "Verbatim upstream runtime license and notice texts; native-component source/relinking compliance needs separate review",
                    "python_license": "python/LICENSE.txt", "model_notices": ["notices/" + row["filename"] for row in assets["notices"] if row["filename"].startswith("MODEL-")]}
    (root / "notices/DEPENDENCIES.json").write_text(json.dumps(notice_index, indent=2) + "\n", encoding="utf-8")


def check_bundle(root: Path) -> dict:
    manifest = json.loads((root / "bundle.json").read_text(encoding="utf-8"))
    if manifest.get("schema") != 1 or manifest.get("target") != TARGET or manifest.get("files") != inventory(root):
        raise ValueError("Bundle inventory mismatch")
    require_x64_assets(json.loads((root / "assets.json").read_text(encoding="utf-8")))
    return manifest


def assemble(cache: Path, destination: Path, source: Path = SOURCE) -> dict:
    if destination.exists():
        raise FileExistsError("Choose a new bundle directory; existing files will not be replaced")
    assets = json.loads((source / "assets.json").read_text(encoding="utf-8"))
    if assets.get("schema") != 1:
        raise ValueError("Unsupported asset schema")
    require_x64_assets(assets)
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
    write_notice_index(destination, assets, cache)
    manifest = {"schema": 1, "target": TARGET, "files": inventory(destination), "omitted": omitted,
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
