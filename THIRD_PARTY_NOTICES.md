# Third-party notices and attribution

Saytwig's application license is MIT. Third-party code, fonts, icons, libraries,
model weights, and bundled binaries keep their own licenses; a release must ship
the notices applicable to its actual contents.

## Speech assets

The pinned runtime/model inventory, upstream URLs, hashes, retained license paths,
and attribution are maintained in [scripts/phonon/NOTICES.md](scripts/phonon/NOTICES.md)
and [scripts/phonon/assets.json](scripts/phonon/assets.json). Phonon 2.5 English
is credited to Fermion Research and derives from NVIDIA Parakeet. The original
model NOTICE and both model/code license files must accompany distribution.

Model download is explicit. Do not replace the weights license with the app's
MIT license or imply endorsement by Fermion Research or NVIDIA.

## Native application dependencies

Cargo.lock records exact Rust dependency versions. GPUI and GPUI Kit are
Apache-2.0 projects. Build tooling must collect the actual dependency license
texts for distributed artifacts; this short summary is not a replacement for
those texts. Optional components must be audited before being enabled.

## Project origin

Handy by cjpais and its contributors inspired the original dictation workflow.
The new Saytwig source tree does not intentionally copy Handy implementation.
If Handy code is later reused, preserve its MIT copyright/license with that code
and describe the derivation accurately. Historical GitHub fork metadata is not a
license declaration and a history rewrite does not erase third-party obligations.

## Development guidance

The pinned BeautifulRust best-practices/review skills by Morgan Linton are retained
under `.agents/skills/` with their MIT copyright/license and source commit. They
are development documentation, not bundled application runtime code.
