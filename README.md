<p align="center">
  <img src="assets/saytwig.svg" width="88" height="88" alt="Saytwig mark">
</p>

<h1 align="center">Saytwig</h1>

<p align="center">Turn speech into text on your device.</p>

<p align="center">
  <a href="https://github.com/Artisians/saytwig/releases">Experimental downloads</a> ·
  <a href="#build-from-source">Build from source</a> ·
  <a href="web/">Landing page source</a>
</p>

Saytwig is an experimental desktop dictation app built in Rust with GPUI.
English speech recognition runs locally, without a transcription account.

Optional ChatGPT cleanup requires a connected account and your permission to share
transcript text. Audio stays local. Failed or cancelled cleanup keeps the original text.

## Experimental downloads

Get available builds from [GitHub Releases](https://github.com/Artisians/saytwig/releases).
There is no stable release yet. The speech runtime and model are separate downloads,
not part of the application package.

| Platform | Package recipe | Current limits |
| :--- | :--- | :--- |
| Windows x64 | Per-user setup executable | Unsigned. Managed speech setup and shortcut-started paste. No Windows ARM or 32-bit support. |
| macOS Apple Silicon and Intel | DMG with `Saytwig.app` | Ad-hoc signed, not notarized. Speech assets require manual provisioning. No automatic paste. |
| Linux x64 | Archive for Ubuntu 24.04-compatible systems | Requires system graphics and audio libraries. Speech assets require manual provisioning. No automatic paste. |

These are package recipes, not a promise that every platform has a published build.
Check the [package status](scripts/release/STATUS.md) and the files attached to each release.
Native installation, microphone permissions, and end-to-end dictation still need
hands-on verification before Saytwig is ready for everyday use.

Nightly prereleases come from `main` only after all four package jobs pass.
Stable releases require separate native QA and approval.
See [release requirements](docs/release.md) for platform warnings and release policy.

## Build from source

Use the pinned Rust toolchain and the native graphics/audio dependencies listed
in the [CI workflow](.github/workflows/ci.yml). Speech setup is separate from the build.

```sh
git clone https://github.com/Artisians/saytwig.git
cd saytwig
cargo build --release --locked --bin saytwig
```

Building the app does not install speech assets. Read the
[speech runtime guide](crates/phonon/README.md) before attempting dictation.
For development checks and project structure, see [Contributing](CONTRIBUTING.md)
and [architecture](docs/architecture.md).

## Credits

Saytwig uses GPUI, GPUI Kit, and Fermion Research's Phonon 2.5 English engine,
derived from NVIDIA Parakeet. New application code is [MIT-licensed](LICENSE).
Dependencies and model weights retain their own licenses, including Apache-2.0
for Fermion's runtime and CC-BY-4.0 for Phonon weights. See
[third-party notices](THIRD_PARTY_NOTICES.md) and the
[speech license inventory](scripts/phonon/NOTICES.md).
