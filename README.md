<p align="center">
  <img src="assets/saytwig.svg" width="88" height="88" alt="Saytwig mark">
</p>

<h1 align="center">Saytwig</h1>

<p align="center">
  A local speech recognition app for desktop, built in Rust with GPUI.
</p>

<p align="center">
  <a href="https://github.com/Artisians/saytwig/tree/nightly">Explore nightly</a> ·
  <a href="#development">Build from source</a> ·
  <a href="docs/account.md">Accounts &amp; privacy</a>
</p>

<p align="center">
  <a href="https://github.com/Artisians/saytwig/actions/workflows/ci.yml?query=branch%3Anightly">
    <img src="https://github.com/Artisians/saytwig/actions/workflows/ci.yml/badge.svg?branch=nightly" alt="Nightly build and lint status">
  </a>
</p>

---

## Speech recognition on your device

| Local speech | Native workspace | Optional cleanup |
| :--- | :--- | :--- |
| English recognition runs on your device. No transcription account required. | Dictation, local history, preferences, shortcut controls, and a small recording popup. | Connect ChatGPT, choose a model, then explicitly enable transcript sharing. Your original text stays available. |

```text
Your voice → On-device recognition → Original text → Copy / guarded paste¹
                                         │
                                    explicit opt-in
                                         ↓
                                  ChatGPT cleanup → Edited text
```

**Audio stays local.** Connecting an account does not enable sharing. Optional
cleanup sends only the transcript; a failed or cancelled cleanup keeps the original.
No analytics or silent API-key billing is built into the app.

¹ Guarded paste is implemented for Windows sessions started with the global
shortcut. Manual Start and other platforms use Copy. Native Windows insertion
still needs verification; input dispatch is not proof that another app accepted it.

## Build status

**Pre-release · no installers or downloadable nightly binaries yet.** Packaging
and dependency-license review remain release gates.

| Channel | Where it stands |
| :--- | :--- |
| [Nightly](https://github.com/Artisians/saytwig/tree/nightly) | Active development source. CI builds and lints Linux x64 and Windows MSVC x64. |
| [Stable](https://github.com/Artisians/saytwig/tree/stable) | Promotion target; the first application snapshot has not been promoted. |

See [release status](docs/release.md) for packaging requirements.

Microphone capture, Windows paste, and ChatGPT account flows still need native-device
validation. See [build details](docs/BUILD_STATUS.md).

## Development

Requires the pinned Rust toolchain and native graphics/audio libraries; the
[CI workflow](.github/workflows/ci.yml) records platform prerequisites. Speech
runtime/model setup is separate from building the app.

```sh
git clone --branch nightly https://github.com/Artisians/saytwig.git
cd saytwig
cargo build --release --locked --bin saytwig

cargo fmt --all -- --check
CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --lib --bins --locked -- -D warnings
```

Read [Contributing](CONTRIBUTING.md) and the [quality workflow](docs/quality.md).
For the internals: [architecture](docs/architecture.md),
[speech runtime](crates/phonon/README.md), and [account integration](docs/account.md).
Rust + GPUI powers the app; the separate speech runtime still includes Python and
PyTorch. Model weights are not embedded in the executable.

## Built with credit

[Handy](https://github.com/cjpais/Handy) and its contributors inspired Saytwig's
speech-to-text workflow.

The native interface uses [GPUI](https://gpui.rs/) and GPUI Kit. Speech recognition
uses Fermion Research and **Phonon 2.5 English**, derived from **NVIDIA Parakeet**.
Design work uses [Impeccable](https://github.com/pbakaus/impeccable).

New application code is [MIT-licensed](LICENSE). Third-party libraries and model
weights keep their own terms: Fermion's runtime package is Apache-2.0; the Phonon
weights are CC-BY-4.0. The complete speech payload also contains dependencies with
other licenses. See [third-party notices](THIRD_PARTY_NOTICES.md) and the
[speech license inventory](scripts/phonon/NOTICES.md); the app's license does not
relicense those components or imply upstream endorsement.
