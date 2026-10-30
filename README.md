<p align="center">
  <img src="https://raw.githubusercontent.com/Artisians/saytwig/nightly/assets/saytwig.svg" width="88" height="88" alt="Saytwig mark">
</p>

<h1 align="center">Saytwig</h1>

<p align="center">A local speech recognition app for desktop, built in Rust with GPUI.</p>

<p align="center">
  <a href="https://github.com/Artisians/saytwig/tree/nightly">Explore nightly source</a> ·
  <a href="https://github.com/Artisians/saytwig/blob/nightly/README.md#development">Build from source</a> ·
  <a href="https://github.com/Artisians/saytwig/blob/nightly/docs/account.md">Accounts &amp; privacy</a>
</p>

<p align="center">
  <a href="https://github.com/Artisians/saytwig/actions/workflows/ci.yml?query=branch%3Anightly"><img src="https://github.com/Artisians/saytwig/actions/workflows/ci.yml/badge.svg?branch=nightly" alt="Nightly build and lint status"></a>
</p>

## Local speech. Optional cleanup.

English speech recognition runs on your device, without a transcription account.
Optional ChatGPT cleanup requires a connected account and explicit transcript-sharing
consent. Audio stays local; the original text remains available.

## Development channels

| Channel | Status |
| :--- | :--- |
| [Nightly](https://github.com/Artisians/saytwig/tree/nightly) | Application source and native Linux x64 / Windows MSVC x64 build checks. |
| [Stable](https://github.com/Artisians/saytwig/tree/stable) | Promotion target; no application release has been promoted. |

**Pre-release: no installers or downloadable nightly binaries yet.** Native-device
validation and dependency notices are still in progress. See
[release status](https://github.com/Artisians/saytwig/blob/nightly/docs/release.md).

[Build evidence](https://github.com/Artisians/saytwig/blob/nightly/docs/BUILD_STATUS.md) ·
[Contributing](https://github.com/Artisians/saytwig/blob/nightly/CONTRIBUTING.md) ·
[License](https://github.com/Artisians/saytwig/blob/nightly/LICENSE) ·
[Third-party notices](https://github.com/Artisians/saytwig/blob/nightly/THIRD_PARTY_NOTICES.md)

## Credits

[Handy](https://github.com/cjpais/Handy) inspired the speech-to-text workflow.
Saytwig uses GPUI, GPUI Kit, and Fermion Research's Phonon 2.5 English engine.
Application code is MIT-licensed. Dependencies and model weights retain their own licenses.
