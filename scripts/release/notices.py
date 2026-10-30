#!/usr/bin/env python3
"""Generate fail-closed, target-specific distribution notices (no test execution)."""
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
ABOUT_VERSION = "0.9.2"
TARGETS = {
    "x86_64-pc-windows-msvc": ["dictation-desktop/tray"],
    "x86_64-unknown-linux-gnu": [],
    "aarch64-apple-darwin": [],
    "x86_64-apple-darwin": [],
}
NOTICE_NAME = re.compile(r"^(licen[cs]e|copying|copyright|notice|authors|ofl)([._-].*)?$", re.I)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def run(args):
    return subprocess.run(args, cwd=ROOT, check=True, capture_output=True, text=True, encoding="utf-8",
                          timeout=600).stdout


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def feature_args(features):
    return ["--features", ",".join(features)] if features else []


def package_key(package):
    return package["name"], package["version"]


def upstream_evidence(packages, output, blockers):
    selected = {package_key(p): p for p in packages}
    evidence = {}
    configured_upstream = {}
    config = (HERE / "about.toml").read_text(encoding="utf-8")
    for item in json.loads((HERE / "upstream-evidence.json").read_text(encoding="utf-8")):
        key = (item["package"], item["version"])
        if key not in selected:
            continue
        package = selected[key]
        vcs_path = Path(package["manifest_path"]).parent / ".cargo_vcs_info.json"
        if (package["repository"] != item["repository"] or not vcs_path.is_file()
                or json.loads(vcs_path.read_text())["git"]["sha1"] != item["commit"]
                or package["license"] != item["declared_license"]):
            blockers.append(f"Upstream license provenance mismatch: {key}")
            continue
        files = []
        for file in item["files"]:
            source = HERE / file["local_path"]
            if not source.is_file() or digest(source.read_bytes()) != file["sha256"]:
                blockers.append(f"Upstream evidence missing/changed: {file['local_path']}")
                continue
            destination = output / "upstream" / f"{key[0]}-{key[1]}" / file["path"]
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
            files.append({"path": str(destination.relative_to(output)),
                          "sha256": file["sha256"], "source_url": file["url"]})
        evidence[key] = files
        file = next(f for f in item["files"] if f["local_path"] == item["selected_file"])
        checksum = item.get("selected_text_sha256", file["sha256"])
        if item.get("selected_start"):
            text = (HERE / file["local_path"]).read_text(encoding="utf-8")
            selected_text = text[text.index(item["selected_start"]):]
            if digest(selected_text.encode("utf-8")) != checksum:
                blockers.append(f"Selected upstream text changed: {key}")
        signature = (item["selected_license"], checksum)
        if item["package"] in configured_upstream:
            if configured_upstream[item["package"]] != signature:
                blockers.append(f"Multiple versions need separate upstream recipes: {item['package']}")
            continue
        configured_upstream[item["package"]] = signature
        section = json.dumps(item["package"])
        config += (f"\n[{section}.clarify]\nlicense = {json.dumps(item['selected_license'])}\n"
                   f"[[{section}.clarify.files]]\n"
                   f"path = {json.dumps(str((HERE / file['local_path']).resolve()))}\n"
                   f"checksum = {json.dumps(checksum)}\n")
        if item.get("selected_start"):
            config += f"start = {json.dumps(item['selected_start'])}\n"
    config_path = output / "harvest-config.toml"
    # Combined-license files and inherited notices can confuse the scanner.
    # These reviewed clarifications preserve the complete original files.
    configured = {}
    for item in json.loads((HERE / "local-clarifications.json").read_text(encoding="utf-8")):
        key = (item["package"], item["version"])
        if key not in selected:
            continue
        package = selected[key]
        if package["license"] != item["declared_license"]:
            blockers.append(f"Local license declaration changed: {key}")
            continue
        base = Path(package["manifest_path"]).parent
        section = json.dumps(item["package"])
        for file in item["files"]:
            source = base / file["path"]
            if not source.is_file() or digest(source.read_bytes()) != file["sha256"]:
                blockers.append(f"Local clarification evidence changed: {key} {file['path']}")
        signature = (item["selected_license"], json.dumps(item["files"], sort_keys=True))
        if item["package"] in configured:
            if configured[item["package"]] != signature:
                blockers.append(f"Multiple versions need separate clarification recipes: {item['package']}")
            continue
        configured[item["package"]] = signature
        config += f"\n[{section}.clarify]\nlicense = {json.dumps(item['selected_license'])}\n"
        for file in item["files"]:
            config += (f"[[{section}.clarify.files]]\npath = {json.dumps(file['path'])}\n"
                       f"checksum = {json.dumps(file['sha256'])}\n")
            if file.get("license"):
                config += f"license = {json.dumps(file['license'])}\n"
    exceptions = json.loads((HERE / "declaration-exceptions.json").read_text(encoding="utf-8"))
    for item in exceptions["packages"]:
        key = (item["package"], item["version"])
        if key not in selected:
            continue
        package = selected[key]
        base = Path(package["manifest_path"]).parent
        normative_record = exceptions["normative_licenses"][item["selected_license"]]
        normative = HERE / normative_record["path"]
        if digest(normative.read_bytes()) != normative_record["sha256"]:
            raise RuntimeError("Normative license evidence changed")
        if package["license"] != item["declared_license"]:
            blockers.append(f"Reviewed declaration changed: {key}")
            continue
        files = []
        for file in item["files"]:
            source = base / file["path"]
            if digest(source.read_bytes()) != file["sha256"]:
                blockers.append(f"Declaration evidence changed: {key} {file['path']}")
            files.append((source, file["path"], file["sha256"]))
        for header in item["headers"]:
            source = HERE / header["local_path"]
            if (digest((base / header["source_path"]).read_bytes()) != header["source_sha256"]
                    or digest(source.read_bytes()) != header["sha256"]):
                blockers.append(f"Source attribution changed: {key} {header['source_path']}")
            files.append((source, source.name, header["sha256"]))
        files.append((normative, f"STANDARD-{item['selected_license']}.txt", normative_record["sha256"]))
        evidence[key] = []
        for source, name, checksum in files:
            destination = output / "declaration-evidence" / f"{key[0]}-{key[1]}" / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
            evidence[key].append({"path": str(destination.relative_to(output)), "sha256": checksum,
                                  "reviewed_exception": item["rationale"]})
        if item.get("retain_source_archive"):
            archive = retain_source_archive(package, output, blockers)
            if archive:
                evidence[key].append(archive)
        destination = output / "declaration-evidence" / f"{key[0]}-{key[1]}" / "EVIDENCE.txt"
        destination.write_text(item["rationale"] + "\nStandard text source: " + normative_record["url"]
                               + "\nThe standard text's placeholder is intentionally unfilled.\n",
                               encoding="utf-8")
        section = json.dumps(item["package"])
        config += (f"\n[{section}.clarify]\nlicense = {json.dumps(item['selected_license'])}\n"
                   f"[[{section}.clarify.files]]\npath = {json.dumps(str(normative.resolve()))}\n"
                   f"checksum = {json.dumps(normative_record['sha256'])}\n")
    config_path.write_text(config, encoding="utf-8")
    return evidence, config_path


def retain_source_archive(package, output, blockers):
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    records = [p for p in lock["package"] if package_key(p) == package_key(package)
               and p.get("source") == package["source"]]
    name = f"{package['name']}-{package['version']}.crate"
    archives = list((cargo_home / "registry" / "cache").glob("*/" + name))
    if len(records) != 1 or not archives:
        blockers.append(f"Missing locked source archive: {name}")
        return None
    archive = next((p for p in archives if digest(p.read_bytes()) == records[0]["checksum"]), None)
    if archive is None:
        blockers.append(f"Source archive checksum mismatch: {name}")
        return None
    shutil.copyfile(archive, output / name)
    return {"path": name, "sha256": records[0]["checksum"], "unmodified_source_archive": True}


def covered_source(packages, output, blockers):
    """Retain the unmodified MPL source, not just a pointer to a mutable branch."""
    notices = []
    for package in packages:
        if package["license"] != "MPL-2.0":
            continue
        if package_key(package) not in {("option-ext", "0.2.0"), ("cbindgen", "0.28.0")}:
            blockers.append(f"New MPL source obligation needs review: {package_key(package)}")
            continue
        archive = retain_source_archive(package, output, blockers)
        if archive is None:
            continue
        name, version = package_key(package)
        notices.append(
            f"{name} {version} is included under Mozilla Public License 2.0.\n"
            "Its complete, unmodified published Source Code Form is provided in\n"
            f"{archive['path']} (gzip-compressed tar archive) in this directory.\n"
            "Retain its source notices and MPL license; Saytwig's MIT license does\n"
            "not limit your rights to that covered source under the MPL.\n"
            f"SHA-256: {archive['sha256']}\n"
            f"Upstream: https://crates.io/crates/{name}/{version}\n")
    if notices:
        (output / "MPL-SOURCE.txt").write_text("\n".join(notices), encoding="utf-8")


def collect_assets(packages, output, blockers):
    """Pinned source review: current application embeds GPUI Kit default icons,
    not bundled fonts. Any changed reviewed package/input needs a new review.
    """
    evidence = json.loads((HERE / "assets.json").read_text(encoding="utf-8"))
    indexed = {package_key(p): p for p in packages}
    inventory = []
    for item in evidence["sources"]:
        package = indexed.get((item["package"], item["version"]))
        if package is None:
            blockers.append(f"Reviewed asset package absent: {item['package']} {item['version']}")
            continue
        base = Path(package["manifest_path"]).parent
        for relative, expected in item["files"].items():
            path = base / relative
            if not path.is_file() or digest(path.read_bytes()) != expected:
                blockers.append(f"Asset evidence changed or missing: {item['package']}/{relative}")
        if item["package"] == "gpui-kit-assets":
            notice = base / "LICENSE-LUCIDE"
            if notice.is_file():
                shutil.copyfile(notice, output / "LUCIDE-NOTICE.txt")
            for line in (base / "default-icons.txt").read_text(encoding="utf-8").splitlines():
                if not line:
                    continue
                path = base / "assets" / line
                if not path.is_file() or not path.resolve().is_relative_to(base.resolve()):
                    blockers.append(f"Missing or invalid bundled icon: {line}")
                    continue
                inventory.append({"package": item["package"], "version": item["version"],
                                  "path": "assets/" + line, "sha256": digest(path.read_bytes()),
                                  "notice": "LUCIDE-NOTICE.txt"})
    # Local assets have no upstream crate policy to cover them. Fail for a review,
    # rather than allowing a new font/image to silently enter the binary bundle.
    reviewed_assets = set()
    for item in evidence.get("application_assets", []):
        path = ROOT / item["path"]
        source = ROOT / item["source"]
        if (not path.is_file() or digest(path.read_bytes()) != item["sha256"]
                or item["embedding"] not in source.read_text(encoding="utf-8")):
            blockers.append(f"Application asset evidence changed: {item['path']}")
        reviewed_assets.add(path)
        inventory.append({**item, "notice": "SAYTWIG-LICENSE.txt"})
    paths = list((ROOT / "crates").rglob("*")) + list((ROOT / "assets").rglob("*"))
    for path in sorted(paths):
        if path.suffix.lower() in {".ttf", ".otf", ".woff", ".woff2", ".svg", ".png", ".ico", ".jpg"}:
            if path not in reviewed_assets:
                blockers.append(f"Unreviewed application asset: {path.relative_to(ROOT)}")
    write_json(output / "ASSET-INVENTORY.json", {
        "assets": inventory, "font_policy": evidence["font_policy"],
        "scope": evidence["scope"],
    })


def generate(args):
    output = args.output.resolve()
    # Never leave a stale success marker from an earlier run. A fresh directory
    # is mandatory, and READY.json is written only after every gate succeeds.
    output.mkdir(parents=True, exist_ok=False)
    blockers = []
    lock_hash = digest((ROOT / "Cargo.lock").read_bytes())
    version = run(["cargo-about", "--version"]).strip()
    if version != f"cargo-about {ABOUT_VERSION}":
        raise RuntimeError(f"Expected cargo-about {ABOUT_VERSION}; found {version}")
    # cargo-about's metadata reader resolves before target filtering. Fetch the
    # locked source archives first (no compilation), then harvest offline so
    # external license lookups cannot change a report between runs.
    run(["cargo", "fetch", "--locked"])
    features = TARGETS[args.target]
    common = ["--locked", "--target", args.target, *feature_args(features)]
    tree = run(["cargo", "tree", "-p", "dictation-app", "--edges", "normal,build",
                "--prefix", "none", "--format", "{p}", *common])
    keys = set()
    for line in tree.splitlines():
        match = re.match(r"^(\S+) v([^\s]+)(?:\s|$)", line)
        if not match:
            raise RuntimeError(f"Unrecognized cargo tree record: {line}")
        keys.add(match.groups())
    metadata = json.loads(run(["cargo", "metadata", "--locked", "--format-version", "1",
                               "--filter-platform", args.target, *feature_args(features)]))
    packages = [p for p in metadata["packages"] if package_key(p) in keys]
    if len(packages) != len(keys):
        raise RuntimeError("Cargo tree/package metadata mismatch or ambiguous package identities")
    upstream, config_path = upstream_evidence(packages, output, blockers)
    covered_source(packages, output, blockers)
    inventory = []
    source_texts = {}
    for package in sorted(packages, key=package_key):
        if package["source"] is None:
            continue  # First-party workspace license is included separately.
        base = Path(package["manifest_path"]).parent
        destination = output / "sources" / f"{package['name']}-{package['version']}"
        files = list(upstream.get(package_key(package), []))
        candidates = {p for p in base.rglob("*") if p.is_file() and (
            NOTICE_NAME.match(p.name) or any(part.lower() in {"licenses", ".licenses"}
                                            for part in p.relative_to(base).parts[:-1]))}
        if package.get("license_file"):
            candidates.add(base / package["license_file"])
        for path in sorted(candidates):
            if not path.is_file() or not path.resolve().is_relative_to(base.resolve()):
                blockers.append(f"Invalid license path: {package['name']} {path.name}")
                continue
            relative = path.relative_to(base)
            data = path.read_bytes()
            if not data.strip():
                continue
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            files.append({"path": str(target.relative_to(output)), "sha256": digest(data)})
        if not files:
            blockers.append(f"No packaged license/notice evidence: {package['name']} {package['version']}")
        source_texts[package_key(package)] = [
            re.sub(r"\s+", " ", (output / file["path"]).read_text(encoding="utf-8", errors="replace")).strip()
            for file in files if not file.get("unmodified_source_archive")
        ]
        inventory.append({"name": package["name"], "version": package["version"],
                          "source": package["source"], "repository": package.get("repository"),
                          "declared_license": package["license"], "evidence": files})
    collect_assets(packages, output, blockers)
    shutil.copyfile(ROOT / "LICENSE", output / "SAYTWIG-LICENSE.txt")
    shutil.copyfile(ROOT / "Cargo.lock", output / "Cargo.lock")
    harvest_path = output / "HARVEST.json"
    # cargo-about 0.9.2's macOS filter omits keyring's security-framework packages.
    # Harvest all platforms there; membership below still uses the target Cargo tree.
    harvest_target = [] if args.target.endswith("-apple-darwin") else ["--target", args.target]
    about = subprocess.run([
        "cargo-about", "generate", "--manifest-path", "Cargo.toml", "--workspace",
        "--config", str(config_path), "--locked", "--fail", "--offline",
        *harvest_target, *feature_args(features), "--format", "json",
        "--output-file", str(harvest_path),
    ], cwd=ROOT, capture_output=True, text=True, encoding="utf-8",
        env={**os.environ, "RAYON_NUM_THREADS": "2"}, timeout=600)
    # Retain diagnostics for reviewers, not for license text substitution.
    (output / "HARVEST-DIAGNOSTICS.txt").write_text(about.stderr, encoding="utf-8")
    texts = []
    covered = set()
    addenda = []
    if about.returncode:
        blockers.append("cargo-about could not satisfy every expression; see HARVEST-DIAGNOSTICS.txt")
        print(about.stderr[:16384], file=sys.stderr)
    else:
        result = json.loads(harvest_path.read_text(encoding="utf-8"))
        selected_packages = {package_key(p): p for p in packages}
        for license in result["licenses"]:
            users = sorted({package_key(u["crate"]) for u in license["used_by"]
                            if package_key(u["crate"]) in keys})
            if not users:
                continue
            covered.update(users)
            if not license.get("source_path"):
                blockers.append(f"Canonical-only license text ({license['id']}): {users}")
            # cargo-about deduplicates equal texts and keeps one source_path;
            # a sourced entry must not mask another crate's canonical fallback.
            normalized = re.sub(r"\s+", " ", license["text"]).strip()
            for user in users:
                if user in source_texts and not any(normalized in text for text in source_texts[user]):
                    # Harvesting also extracts embedded source-file copyright
                    # addenda, not just conventionally named LICENSE files.
                    source = Path(license["source_path"]) if license.get("source_path") else None
                    package = selected_packages.get(user)
                    base = Path(package["manifest_path"]).parent if package else None
                    if source and base and source.is_file() and source.resolve().is_relative_to(base.resolve()):
                        relative = source.relative_to(base)
                        addenda.append({"package": user[0], "version": user[1],
                                        "source_path": str(relative),
                                        "source_sha256": digest(source.read_bytes()),
                                        "license": license["id"],
                                        "text_sha256": digest(license["text"].encode("utf-8"))})
                    else:
                        blockers.append(f"Resolved text lacks per-package evidence ({license['id']}): {user[0]} {user[1]}")
            texts.append("\n" + "=" * 72 + "\n" + license["name"] + "\n" +
                         "Used by: " + ", ".join(f"{n} {v}" for n, v in users) +
                         "\n\n" + license["text"] + "\n")
        for package in packages:
            if package["source"] is not None and package_key(package) not in covered:
                blockers.append(f"Missing resolved notice: {package['name']} {package['version']}")
    (output / "RUST-THIRD-PARTY-NOTICES.txt").write_text("".join(texts), encoding="utf-8")
    if digest((ROOT / "Cargo.lock").read_bytes()) != lock_hash:
        blockers.append("Cargo.lock changed while generating notices; rerun against frozen source")
    report = {"schema_version": 1, "target": args.target, "features": features,
              "cargo_lock_sha256": lock_hash, "cargo_about": version,
              "scope": "Production normal/build dependencies; build-only tooling included conservatively. Not a binary linkage claim or legal clearance.",
              "packages": inventory, "source_license_addenda": addenda,
              "blockers": sorted(set(blockers))}
    write_json(output / "INVENTORY.json", report)
    if blockers:
        print("NOT READY: " + "\n".join(sorted(set(blockers))), file=sys.stderr)
        return 1
    write_json(output / "READY.json", {k: report[k] for k in
                                     ("schema_version", "target", "features", "cargo_lock_sha256", "cargo_about")})
    print(f"Notices generated: {output}")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True, choices=TARGETS)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        return generate(args)
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Notice generation blocked: {error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError):
            print(error.stderr, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
