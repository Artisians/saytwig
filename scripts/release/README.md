# Native distribution notices gate

This is artifact-generation tooling, not a test suite. Do not distribute a binary
on the strength of the repository's short `THIRD_PARTY_NOTICES.md` alone.

Install the pinned standard tool (outside the app dependency graph):

```sh
cargo install cargo-about --version 0.9.2 --features cli --locked
```

After the locked production build/fetch, generate for the **same** target. Python
3.11+ and `cargo-about` must be on PATH. Use a fresh output directory each time:

```sh
python3 scripts/release/notices.py --target x86_64-pc-windows-msvc --output dist/windows/licenses
python3 scripts/release/notices.py --target x86_64-unknown-linux-gnu --output dist/linux/licenses
```

The script first runs `cargo fetch --locked` (source download only, not a build)
because cargo-about resolves metadata before target filtering. It then harvests
offline; target-excluded packages are not added to the notices.

The Windows recipe includes `dictation-desktop/tray`, matching the release build;
Linux x64 and macOS Apple Silicon/Intel use the app's existing default production
features. Windows ARM/32-bit targets are rejected. Linux ARM source paths remain
intact but have no hosted package recipe. Profile optimization does
not change this Cargo dependency recipe; changing features/assets requires a new
reviewed recipe. Do not enable all-features or dev dependencies for this report.

## Required packaging contract

1. Freeze Cargo.lock and manifests; build with `--locked`, target and features
   matching the selected recipe. The Rust toolchain remains repository-pinned.
2. Run the generator and require **exit code zero and `READY.json`**. A failed run
   may leave diagnostic evidence, but never produces that success marker. Never
   reuse a prior generation directory or suppress a failing exit code.
3. Ship the **entire** generated `licenses/` directory next to the executable,
   including original package license/notice files, compiled dependency inventory,
   Lucide attribution, the icon inventory and Saytwig's own license.
4. Separately ship the Python/PyTorch/model licenses inventoried by
   `scripts/phonon`. This Rust report does not cover those bundles or separately
   bundled system DLLs/libraries. OS-installed libraries/fonts are not copied by
   this tool; if packaging starts redistributing any, add their evidence first.
5. `READY.json` is a completeness gate for this reviewed source recipe, **not a
   legal clearance**, a claim of binary link analysis, or permission to skip
   platform acceptance/release review. Cargo build-only dependencies are retained
   conservatively rather than silently discarded.

## Why this is stricter than plain cargo-about

`cargo-about generate --locked --fail --offline --format json` harvests actual
source trees for the target/production feature graph. Its SPDX solver selects
from the explicit policy in `about.toml`; unrecognized or unsatisfied expressions
block generation. However, cargo-about can fall back to canonical SPDX text
without an upstream copyright. This wrapper rejects canonical-only results,
requires original packaged or reviewed VCS-pinned evidence for **every** selected third-party package,
checks coverage against `cargo tree --edges normal,build`, and copies all detected
LICENSE/COPYING/COPYRIGHT/NOTICE/AUTHORS/OFL files (including vendored subtrees).
No copyright names are synthesized. Missing evidence must be resolved by an
upstream-source, version/commit/hash-pinned clarification and review, not by
loosening `--fail` or excluding the package. `HARVEST-DIAGNOSTICS.txt` and the
`INVENTORY.json` blockers explain failures.

`assets.json` pins the audited GPUI Kit default asset-source implementation and
icon list. The bundle contains Lucide icons, with the upstream combined ISC and
Feather-derived MIT attribution copied verbatim. The current app does not bundle
font files; GPUI's IBM Plex/Lilex byte inclusions inspected here are test-only,
while production fonts come from the platform or explicitly supplied asset
source. The native first-party `assets/saytwig.svg` is pinned by hash and its actual
`include_bytes!` source reference, under Saytwig's MIT license. New local font/image
assets fail the gate until reviewed. Default icon
hashes are recorded per artifact; locked crate versions/checksums bind the inputs.

## Primary references

- [cargo-about generation flags](https://embarkstudios.github.io/cargo-about/cli/generate/index.html)
- [cargo-about policy and hash-pinned clarifications](https://embarkstudios.github.io/cargo-about/cli/generate/config.html)
- [cargo-about output and source paths](https://embarkstudios.github.io/cargo-about/cli/generate/output.html)
- [Cargo tree dependency kinds and features](https://doc.rust-lang.org/cargo/commands/cargo-tree.html)
- [GPUI Kit source](https://github.com/longbridge/gpui-kit/tree/99576db58ec8103417c41a092e891b4022d24e7e/crates/assets)

No test files, test targets, or test commands are part of this route.

## Reviewed clarifications and current release gate

The script requires Python **3.11+** (`tomllib`). `upstream-evidence.json` records
exact package version, declared SPDX expression, published VCS commit, original
source URL and SHA-256 for license files omitted from crate archives. Their
verbatim copies live under `evidence/`. The generator verifies provenance against
the downloaded crate and verifies every hash before producing temporary local
cargo-about clarifications. `local-clarifications.json` similarly pins combined
license files that require explicit scanner guidance; full files and inherited
notices are retained, not replaced by an invented copyright.

The exact unmodified `option-ext 0.2.0` source archive is included with its
Cargo.lock checksum and `MPL-SOURCE.txt` source-availability notice. It remains
third-party MPL compliance material, not new application test scaffolding; no
code from that archive is executed by this route. See the
[Mozilla MPL 2.0 source distribution terms](https://www.mozilla.org/en-US/MPL/2.0/).
The supplied webpki-roots CDLA agreement is retained too.

Both reviewed target recipes passed cloud generation. See `STATUS.md` for the
receipt scope and remaining release requirements. The packaging workflow must
regenerate notices from its own immutable checkout. A source CI result alone
is not proof that distributable notices have passed.

The Apache declaration exceptions are deliberately limited to `mac 0.1.1` and
`pathfinder_geometry 0.5.1`. The published archive explicitly offers Apache-2.0;
for mac the README independently repeats it, and Pathfinder source headers repeat
it with original project copyright. `declaration-exceptions.json` pins those
original declarations and all Pathfinder source license headers. The exact
[official Apache text](https://www.apache.org/licenses/LICENSE-2.0.txt) is retained
unchanged, including its unfilled template. This is an explicit inference from
upstream declarations, **not** an invented copyright, blanket canonical-text
exception or legal clearance. A separate narrowly pinned `seahash 4.1.0` exception retains its exact published
MIT declaration, authored metadata/README and complete checksum-verified source
archive. It adds the unchanged [SPDX MIT standard text](https://spdx.org/licenses/MIT.html),
explicitly labeled standard with placeholder unfilled. No copyright holder/year
is inferred from Cargo authors. This addresses the stricter tooling evidence
policy; it does not establish what unavailable upstream material might contain
or replace independent release review.

Some Windows crates ship their real Microsoft notices as lowercase `license-mit`;
hash-pinned local clarifications preserve those actual files. The scanner uses
workspace metadata so a workspace-selected production feature (Windows tray) is
not lost by its graph traversal; final notice membership/coverage is filtered
against the target-specific `cargo tree -p dictation-app --edges normal,build`
graph. Extra unrelated workspace package notices are not substituted for missing
production dependencies.

For multiple versions of one crate, a shared cargo-about clarification is used
only when the selected license and exact terms hash match. Each version still
retains its own full original notices. For zune-jpeg, the shared Zlib terms exclude
no obligations; the newer copyright header remains in its retained full file.
The dpi Apache-2.0 AND MIT declaration keeps both original license files.
