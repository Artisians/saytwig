# Native notices status, 2026-10-30

Windows and Linux generation passed on 2026-10-30.
These receipts do not replace independent release review.

Actual cloud harvesting (not automated tests) resolved a Linux production
normal/build graph of 583 packages and a Windows x64 MSVC/tray graph of 481.
The initial Linux generation exited nonzero: 27 published packages omitted
license files. Subsequent source-backed clarifications reduced the completed
older Linux pass to four missing-evidence cases, all now explicitly addressed
by the current recipe. That older failed receipt is not a current approval.

Current authored evidence includes:

- VCS-commit/URL/SHA-256-pinned original upstream texts for 34 packages, including
  GPUI Kit, AccessKit, Lyon and the full Zune Zlib license with original headers.
- Hash-pinned clarifications of complex local license files and the actual
  lowercase Microsoft `license-mit` files, without substituting invented holders.
- Narrow reviewed declaration exceptions for exact mac 0.1.1 and
  pathfinder_geometry 0.5.1: retain original declarations/attribution and every
  Pathfinder copyright/license header, plus unchanged official Apache text.
- Separate exact seahash 4.1.0 receipt retaining its published MIT declaration,
  authored metadata/README and complete checksum-verified unmodified source
  archive, plus authoritative MIT standard text explicitly labeled normative
  with its placeholder unfilled. No holder/year is inferred from Cargo authors.
- Exact option-ext 0.2.0 source archive with Cargo.lock checksum and MPL source
  availability notice; supplied webpki-roots CDLA agreement retained.
- Original combined Lucide/Feather attribution and default embedded SVG inventory.
  The audited current default asset source does not bundle font files.

The declaration receipts are transparent, narrowly version-pinned evidence-policy
exceptions. They do not establish what unavailable upstream material might contain
or constitute legal clearance. Unknown/ambiguous packages and changed hashes still
block generation. Do not turn off those gates.

The corrected Windows generation exited zero with 473 third-party packages,
107 asset records, no blockers, and no cargo-about diagnostics. The receipt uses
`x86_64-pc-windows-msvc` with `dictation-desktop/tray` and cargo-about 0.9.2.
Linux generation also exited zero, with 575 third-party packages, 107 asset
records, no blockers, and no cargo-about diagnostics. Its target is
`x86_64-unknown-linux-gnu`, with default production features. All retained package
evidence hashes from both outputs were reread and matched.

Both receipts use Cargo.lock SHA-256
`bfdeaf07a9cc43e17e927c0d859a6fa0bdcb5d3fa8dfe2c8851f9602ea9cc26f`.
These are cloud artifact-generation results, not native Windows execution.

The corrections retain dpi 0.1.2's existing Apache and libm MIT notices, with an
explicit per-file clarification. Both zune-jpeg versions retain their own exact
upstream notices. Their shared Zlib terms have identical hashes; newer copyright
attribution remains in the full original file shipped beside the report.

The native first-party `assets/saytwig.svg` is now inventoried under Saytwig's MIT
license. Its exact hash and actual `include_bytes!` reference are pinned. The
native recipe does not include separately served website assets or Lato fonts.

Require generator exit zero and READY.json on the immutable packaging checkout.
Check the target, features, tool version, Cargo.lock hash, and retained evidence.
A source CI result or partially populated output directory is not a notices pass.
Source publication remains independent of this binary gate.

## Reviewed macOS exceptions, 2026-10-31

The first native macOS packaging run failed the notices gate on both architectures.
Five additional upstream receipts now cover `accesskit_macos 0.26.3`, `objc-sys 0.3.5`,
`objc2 0.5.3`, `objc2-app-kit 0.2.2`, and `objc2-foundation 0.2.2`. Each retains the
exact license text from the published crate's repository and VCS commit.

The owner approved narrow declaration exceptions for 23 additional exact versions.
Twenty-one newer ObjC packages have upstream declarations but no complete notice
text at their published commits. All retain the full pinned upstream declaration,
including its explicit uncertainty about Apple SDK derived-work redistribution.
The older `block 0.1.6` and `malloc_buf 0.0.6` archives lack published VCS metadata.
No commit is guessed for them. Every exception retains the full unmodified locked
source archive and original metadata. Standard license text has its placeholder
unfilled. No copyright holder or year is inferred.

Cargo-about's name-only recipes now run in separate passes when old and new
versions require different evidence. Each exact version contributes from its
assigned pass only. Old original notices remain intact. SPDX solving, canonical-only
rejection, per-package evidence, source hashes, coverage and READY checks remain enabled.

Actual cloud generation passed for macOS ARM with 487 third-party packages and
macOS Intel with 488. Both exited zero, have no blockers, and emitted READY for
Cargo.lock SHA-256 `2b25339e7850b73f60242ef9d77efabfa259be217fb724e268bbd923f97241ef`
with cargo-about 0.9.2 and no extra features. Independent ARM artifact inspection
verified all 1,024 evidence records, 23 locked archives, 87 metadata files, and
21 complete upstream declarations. The earlier same-lock ARM receipt had 46
blocker messages across 23 versions and no READY.

These are source-harvesting receipts on Linux ARM, not native macOS packaging,
installation or dictation acceptance. Native CI must regenerate evidence on its
own immutable checkout before distribution. The source-retaining exceptions do
not resolve missing original notices or the upstream SDK uncertainty, and are
not legal clearance. Unknown versions and changed hashes still block generation.

The same current recipe also passed Windows generation with 474 third-party
packages and Linux with 577. All 881 Windows and 1,060 Linux retained evidence
records matched their hashes. Linux notice bytes match the previous same-lock
receipt. Windows selected zune-core 0.5.3's explicit MIT alternative instead of
Zlib; its original MIT, Apache and Zlib files all remain retained. This is a
permitted license choice, not a missing or synthesized notice.
