# Native notices status — 2026-10-29

**Not cleared for binary distribution. The generator fails closed.**

Actual cloud harvesting (not automated tests) resolved a Linux production
normal/build graph of 583 packages and a Windows x64 MSVC/tray graph of 481.
The initial Linux generation exited nonzero: 27 published packages omitted
license files, and the initial solver policy did not yet cover MPL/CDLA.

The source now contains VCS-commit/URL/SHA-256-pinned original upstream license
texts for 26 packages, plus hash-pinned scanner clarifications for seven complex
local license files. It retains combined Lucide/Feather attribution and inventories
default embedded SVGs. No bundled font file was found in the reviewed app asset
source. The MPL option-ext archive is copied without modification and verified
against Cargo.lock. None of this is a legal-clearance statement.

Remaining evidence gaps at this snapshot include:

- `mac 0.1.1`: archive has neither license text nor published VCS metadata. Do not
  invent a copyright or attach the current repository's license without proving
  that it applies to this version.
- `pathfinder_geometry 0.5.1`: original published VCS revision is known, but a
  full matching upstream license text has not yet been obtained.
- `seahash 4.1.0`: original VCS revision is known; pinned upstream license evidence
  is not yet available locally.
- `zune-inflate 0.2.54` and Windows `zune-core 0.4.12`: a source license declaration
  exists, but this snapshot does not yet provide the full corresponding texts.
- Additional scanner addenda/combined-license mismatches remain subject to the
  final target-specific generator pass. A successful solver alone does not
  override the wrapper's evidence/coverage blockers.

The latest completed Linux pass before the new complex-file clarifications
returned blockers, wrote the full harvested texts/inventory, and did **not**
create READY.json. The Windows pass and final clarified Linux pass were not
complete at snapshot time. CI must run the current script itself; this document
is not a substitute for its terminal result. No generated binary is authorized
for upload without exit zero and READY.json from the same locked source recipe.
