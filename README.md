<p align="center">
  <img src="assets/saytwig.svg" width="88" height="88" alt="Saytwig mark">
</p>

<h1 align="center">Saytwig</h1>

<p align="center">A local speech recognition app for desktop, built in Rust with GPUI.</p>

<p align="center">
  <a href="https://github.com/Artisians/saytwig/releases">Experimental downloads</a> ·
  <a href="#development">Build from source</a> ·
  <a href="docs/account.md">Accounts &amp; privacy</a>
</p>

## Local speech. Optional cleanup.

English speech recognition runs on your device, without a transcription account.
Optional ChatGPT cleanup requires a connected account and explicit transcript-sharing
consent. Audio stays local. A failed or cancelled cleanup keeps the original text.

Windows has managed speech setup and guarded paste for shortcut-started sessions.
macOS and Linux builds are experimental. Managed speech setup is Windows-only;
other platforms require a separately provisioned, verified runtime. Paste is not
implemented on macOS or Linux. Native microphone, permission, and dictation flows
still need platform QA. A successful build does not establish working dictation.

## Downloads and channels

`main` contains the application and the [landing page source](web/).
Pull requests run formatting, production Clippy, native linking, and packaging.
Merges to `main` publish commit-addressed experimental nightly prereleases with
Windows x64 setup, macOS Apple Silicon and Intel app bundles, and Linux x64 archives.
Windows packages are unsigned. macOS packages use ad-hoc signatures and are not
notarized. macOS packaging and nightly publication currently remain blocked on
[license evidence](scripts/release/STATUS.md). Nightly packages do not include
the speech runtime or model and are not stable releases.

The [first Windows ZIP prerelease](https://github.com/Artisians/saytwig/releases/tag/saytwig-experimental-20261030-b8c08a0)
remains available. New Windows nightlies use a setup executable instead.
Existing `nightly` and `stable` branches are retained as history, not development
or promotion branches. Stable publication remains a separate reviewed action
using a `saytwig-vX.Y.Z` tag from `main`, after native QA.

See [release requirements and package limitations](docs/release.md).

## Development

Use the pinned Rust toolchain and the native graphics/audio dependencies listed
in the [CI workflow](.github/workflows/ci.yml). Speech setup is separate from the build.

```sh
git clone https://github.com/Artisians/saytwig.git
cd saytwig
cargo build --release --locked --bin saytwig
cargo fmt --all -- --check
CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --lib --bins --locked -- -D warnings
```

Read [Contributing](CONTRIBUTING.md), [architecture](docs/architecture.md), and
[speech runtime details](crates/phonon/README.md).

## Credits

Saytwig uses GPUI, GPUI Kit, and Fermion Research's Phonon 2.5 English engine,
derived from NVIDIA Parakeet. New application code is [MIT-licensed](LICENSE).
Dependencies and model weights retain their own licenses, including Apache-2.0
for Fermion's runtime and CC-BY-4.0 for Phonon weights. See
[third-party notices](THIRD_PARTY_NOTICES.md) and the
[speech license inventory](scripts/phonon/NOTICES.md).
