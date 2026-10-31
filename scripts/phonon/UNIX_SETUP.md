# macOS and Linux speech provisioning

The existing explicit runtime installer has separate immutable locks for macOS
Apple Silicon, macOS Intel, Linux x64, and Linux ARM64. Windows keeps its original
Python archive, wheel pins, inventory digest, and storage directory. There is no
second installer, pip invocation, system Python dependency, or package-manager
installation on the user's machine.

## Application contract

`phonon_service::setup_capability()` returns installation metadata without file
access, hashing, network access, or microphone queries. It caches the native
process architecture check. `Available` means that this application target has
pinned installation inputs. It does not mean that microphone permission is
granted or native dictation has passed acceptance.

`SetupMetadata` provides the runtime download size, model download size, optional
installed size, sources, prerequisites, and a recovery directory relative to
the application's `runtimes` directory. The peak allocation is unknown because
the filesystem allocation unit and existing cache affect the calculation.
Unknown sizes stay unknown. Model-only consent uses `ARCHIVE_BYTES` and
`MODEL_URL` even when the runtime was separately provisioned and verified.

Only an explicit user action calls `install_runtime_requested`. It retains the
existing 30-minute pre-commit deadline, resumable hash-checked downloads,
nonblocking installation lease, owned helper, cancellation, full inventory
verification, and atomic no-replacement publication. Startup discovery never
downloads. Crash leftovers require explicit review rather than automatic deletion.

The installer uses pinned Astral python-build-standalone CPython 3.12.16 archives
from release `20261026`. Rust extracts a bounded private bootstrap. Archive links
are materialized only when their final target is a regular member inside the
Python archive. No filesystem links enter the runtime. The same assembler then
installs the verified wheels offline. The final interpreter uses a private
`python3.12._pth` with no `import site`, user site, environment paths, or executable
`.pth` hooks. Its location is part of the independently pinned inventory.

macOS model and runtime publication use `renamex_np` with `RENAME_EXCL`.
An existing file, link, or empty directory is not replaced. Unsupported
filesystems fail without a replacement fallback. Windows ARM and 32-bit Windows
remain rejected. An Intel macOS app under Rosetta must be replaced with the
native Apple Silicon app. Linux ARM64 retains its own runtime lock.

## Locked dependencies and notices

All targets use Fermion Research 0.2.15 and the same pinned Phonon 2.5 English
model. Apple Silicon and Linux use Torch 2.7.1 CPU inference. The Intel macOS lock
uses Torch 2.2.2, transformers 5.0.0, NumPy 1.26.4, SciPy 1.17.1, and tokenizers
0.22.2 to satisfy upstream's separate Intel constraints. It cannot use the
Windows dependency set.

Each `assets-<target>.json` records exact URLs, lengths, and SHA-256 hashes.
Each `python-notices-<target>.json` retains base64-encoded, byte-exact upstream
`PYTHON.json` and license texts from that target's full CPython archive. Assembly
retains that entire pinned archive under `notices`, writes the original license
texts under `notices/python`, and keeps the archive receipt. The install-only
archive alone is not treated as complete license evidence. Wheel licenses and
model notices retain their existing full-text index and byte-for-byte checks.

These receipts do not establish legal clearance. Native-component source and
replacement obligations in [NOTICES.md](NOTICES.md) remain open. The separate
exact-version Rust macOS declaration exceptions, source-archive receipts, and
upstream Apple SDK caveat remain unchanged.

For a maintainer assembly, use the existing command with an explicit target.

```sh
python3.12 -B scripts/phonon/assemble.py \
    --target aarch64-unknown-linux-gnu \
    --cache CACHE --output NEW_DIRECTORY
```

The other target names are `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`,
and `x86_64-apple-darwin`. `--offline` requires every pinned download in the
cache. `--verify` checks the complete existing output rather than rebuilding it.
The inventory digest must match the reviewed pin in `runtime_targets.rs` before
the application can execute that bundle. A digest read from the same mutable
directory is never a trust source.

## Acceptance limits

Cross-platform assembly and Linux compilation are not macOS execution evidence.
Native launch, microphone permission allowance and denial, explicit setup and
cancellation, first dictation, clipboard delivery, and shutdown still require
hands-on operation of the actual application on each supported platform.
macOS and Linux have no automatic paste implementation. No signing credentials,
Gatekeeper changes, or other platform-security bypass are part of this setup.
