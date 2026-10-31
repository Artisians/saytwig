# First-run local speech setup — implementation review

The new `runtime_install.rs` is connected to managed discovery and an explicit
first-run consent/progress UI in the current source. Its initial production
workspace Clippy and formatting passed; the subsequent build-recovery backend
adjustment below awaits the UI owner's final check/rebuild. None is evidence of
native Windows execution.
The earlier frozen Linux QA binary predates this installer. Native installation,
clean-machine prerequisites and interrupted-stage recovery remain review gates.

## Intended normal-user flow

On native Windows x64, an explicit **Set up local speech** action downloads the
compiled official CPython/wheel/notice lock, prepares a private runtime, then
uses the existing explicit model installer. Startup discovery does not download.
The application does not run pip, use system Python, install system packages,
request administrator access or copy arbitrary DLLs. Microsoft Visual C++ x64
prerequisites must already be present; failure must be explained rather than
silently installing them. Managed setup is not implemented for Linux/macOS.

Pinned runtime inputs total **314,446,024 bytes**; the model is a separate
**157,616,598-byte** archive. First setup therefore downloads **472,062,622 bytes**
when none are cached. The model's three installed files total 177,757,399 bytes.
The runtime remains approximately 665 MB unpacked. These are asset inventory
sizes, not network-transfer billing or installed disk-allocation measurements.

## Integrity and ownership

- The app embeds the asset lock, own assembler/helpers/license, a canonical
  source-input fingerprint, and an independently reviewed expected inventory
  SHA-256. No hash read from the installed directory becomes a trust anchor.
- HTTPS-only, sequential, size-bounded downloads resume only managed partials;
  each asset's exact SHA-256 is checked before publication. Unknown complete
  files are not replaced. Download cache membership is restricted to the
  compiled lock and its reserved partial filenames.
- The pinned Python ZIP is verified again before bounded Rust ZIP extraction.
  Its isolated embedded Python runs the own assembler with `--offline` and an
  owner-process watchdog. The helper requires every cached pinned artifact and
  prohibits audited Python network/subprocess operations. This is cooperative
  Python isolation, not an OS sandbox against arbitrary native code.
- Inventory order is explicit case-sensitive component order on every platform;
  generated text and Saytwig integration text use LF. Upstream license/archive
  bytes are retained verbatim. The reviewed inventory remains
  `291c56eed6455f85b2f4a57e00d74925c058e2bff593f79fdd69969f59fea83c`.
- A version-scoped ownership marker and nonblocking installation lease guard
  managed storage. Windows reparse paths/non-directory ancestors are rejected.
  Marker reads are bounded. The completed runtime undergoes full verification
  before atomic no-replacement publication; recognition still verifies before
  executing its owned runtime.
- Cancellation owns/kills/reaps the helper. Normal error/cancellation removes
  only this call's staging directory and preserves bounded partial downloads.
  One shared **30-minute pre-commit deadline** covers all runtime downloads,
  hashing, extraction and preparation. Post-commit confirmation/cleanup is
  separate; a completed installation is not rolled back because of a late
  cancellation. The separate model installer has its existing deadlines.

## Space and recovery limitations

The pinned archive central directories declare 1,524,568,002 expanded bytes
before omitting the 19 link-time `.lib` files, across 21,208 entries. A separate
bootstrap expands to 22,640,669 bytes/34 entries. Free-space checks include
remaining downloads, both expansion amounts and filesystem allocation overhead,
and repeat after downloads. They are advisory; concurrent disk use can still
cause a safe write failure. No 1.5 GB second runtime was assembled for this review.

Process death can leave one fixed `.preparing` directory. A later attempt refuses
ambiguous leftovers instead of accumulating UUID directories or deleting unknown
files. An explicit ownership-aware recovery action is still needed; until then
this case requires manual review. A completed but hash-invalid partial also
requires review rather than being silently overwritten. These are usability gaps,
not claims of complete first-run recovery support.

Direct upstream downloads do **not** establish a legal exemption. Full retained
notices are required, and the separate corresponding-source/relinking review
still blocks redistribution of our bundled runtime. No signed installer, runtime
binary release, native Windows hands-on result or performance improvement is
claimed by this document.

## Verification evidence — 2026-10-30

- `cargo fmt --all -- --check`: passed.
- Jobs=1 production `cargo clippy --locked --workspace --lib --bins -- -D warnings`:
  passed after the cancellable inventory-walk change (9.46 seconds; earlier
  installer integration pass 1 minute 25 seconds). These are check durations,
  not application performance measurements.
- Existing-toolchain Windows x64 **GNU compile-only** check of `phonon-service`
  passed after the final integrity change (4.66 seconds; initial installer pass
  20.43 seconds). This is neither native Windows execution nor a Windows MSVC
  application build.
- Production `scripts/release/notices.py`, with existing cargo-about **0.9.2**,
  produced fresh `READY.json` inventories with **zero blockers** for Windows
  MSVC x64 (476 production packages) and Linux x64 (578 production packages).
  Both identify Cargo.lock SHA-256
  `0e65f93af442ba58151ff71182d3e1682e90e6915d4ce7f7d39414e4693c156c`.
  That initial check used ZIP **9.1.3** with `deflate-flate2-zlib-rs`;
  `typed-path 0.13.0` and other-target `nix 0.31.3` were the other lock
  additions. The later build-recovery feature adjustment is recorded below. Notice readiness is scoped to those
  production Rust dependencies, not legal clearance for the separate Python
  speech runtime or a claim of actual binary linkage.
- Metadata-only inspection found no unknown directories relative to the trusted
  file-ancestor inventory in the existing Windows bundle (1,739 directories) or
  Linux development bundle (1,833 directories). The runtime verifier now checks
  cancellation on every walk entry, rejects unknown directories before entering
  them, and bounds retained file membership by the trusted inventory. Manifest
  bytes are bounded, hashed and parsed from the same snapshot.
- Assembler syntax was parsed. Its canonical integration sources and explicit
  ordering match the reviewed inventory; embedded source fingerprint:
  `35ce1e4538735f06f2587f785e92eefd4790add8bc03061c6d986dc7365ed5f8`.
  No new runtime/model download, second runtime assembly, native Windows setup,
  synthetic harness or automated test suite was run for this change.

The independently reviewed installer source remains separate from the earlier
frozen Linux QA binary. That binary exposed a GPUI recording-overlay reentrant
entity panic during Start; the UI owner corrected the source. It was not evidence
of a speech-runtime failure or successful end-to-end recognition. A new native
app rebuild and hands-on flow remain the UI/coordinator's responsibility.

Owned helper destruction kills/reaps normally. Parent-exit watchdogs start before
model loading, but they are cooperative Python mechanisms, not an OS-enforced
kill-on-parent-exit guarantee during arbitrary native/OS stalls. Actual crash
cleanup should be confirmed using only the launched application's known child
processes, not broad process-name termination.

### Build-recovery backend adjustment

After an optimized GPUI compile was terminated by SIGKILL under memory pressure,
the ZIP feature was changed to `deflate-flate2` plus an explicit direct
`flate2` dependency with defaults disabled and `rust_backend` enabled. ZIP remains
exactly 9.1.3. This keeps the speech crate independently buildable while matching
the pre-existing GPUI/miniz backend rather than enabling a second `zlib-rs`
backend throughout the shared image graph. Existing release fingerprints confirm
the prior graph lacked `any_zlib`/`zlib-rs`; this is not a measured binary-size or
application-speed improvement.

Fresh sequential notice generation for the adjusted lock passed with zero
blockers: Windows MSVC x64 **475 packages**, Linux x64 **577 packages**. Both
`READY.json` receipts match Cargo.lock SHA-256
`2b25339e7850b73f60242ef9d77efabfa259be217fb724e268bbd923f97241ef`.
Receipt hashes are respectively
`a9276ba6803f16b2b3621f2e52d782d9f45f3f212e09004c1e298857f936aa44`
and `36d59cb24198b6f69fbefed1ea9a6ac1397836517414e3704bbf6b2d570552d3`.
The UI owner coordinates final formatting, Clippy and optimized rebuild for this
adjusted graph and the recording-overlay correction. No additional runtime
copy, native Windows execution, automated tests or publication was performed by
this notice-validation pass.
