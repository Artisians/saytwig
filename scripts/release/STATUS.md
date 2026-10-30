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

## macOS packages remain blocked

The first native macOS packaging run failed the notices gate on both architectures.
Five additional upstream receipts now cover `accesskit_macos 0.26.3`, `objc-sys 0.3.5`,
`objc2 0.5.3`, `objc2-app-kit 0.2.2`, and `objc2-foundation 0.2.2`. Each retains the
exact license text from the published crate's repository and VCS commit.

Twenty-one newer ObjC packages have upstream license declarations but no complete
notice text at their published commits. The older `block 0.1.6` and `malloc_buf 0.0.6`
archives lack published VCS metadata. The current gate does not substitute a guessed
commit or extend its existing declaration exceptions to those packages. macOS
distribution remains blocked pending source-backed evidence or an explicitly
reviewed change to that policy. No macOS notices pass or legal clearance is claimed.
