# Saytwig

A local speech recognition app for desktop, built in Rust with GPUI. Saytwig turns speech into text on your device, without a transcription account. Optional transcript cleanup is a separate, explicit choice.

**Status: active, pre-release development.** The modules below and a native GPUI interface have been authored and are being integrated. Native-device behavior is not yet release-verified. There is no production installer or supported release yet. Screenshots of concepts are not evidence of a running application.

## What exists today

- A newly authored, shell-independent dictation state machine. Cancellation invalidates late results rather than allowing stale text to be pasted.
- Explicit microphone acquisition, bounded recording, and anti-aliased conversion to 16 kHz mono.
- Versioned settings and history storage with atomic writes, conflict detection, and refusal to overwrite corrupt or newer-schema data.
- A single speech engine integration: Fermion Research 0.2.15 with Phonon 2.5 English. Runtime and model installation are separate; immutable asset hashes and upstream notices are retained.
- An owned resident speech process with a 15-minute idle-unload policy, avoiding a fresh process for every dictation. Native application latency still requires measurement.
- An opt-in, transcript-only cleanup transport with a conservative editing prompt, cancellation, bounded responses, and rejection of partial/failed output. Original text stays available. The interface now includes account controls and separate sharing consent; live account authorization and inference remain unverified.
- A compact GPUI Kit interface with dictation, history, preferences, account/cleanup controls, global shortcut registration, and a small recording popup. Source compilation is not native visual verification.
- Explicit copy with bounded background ownership. On Windows, dictation begun with the global shortcut attempts one guarded paste into the original external application; manual Start and other platforms use Copy. The app checks target focus, modifiers, clipboard ownership and cancellation, but OS input dispatch is not proof of insertion. Text stays on the clipboard; failed/partial dispatch is never blindly retried.
- Explicit GitHub version-check infrastructure, restricted to new `saytwig-vX.Y.Z` releases so preserved Handy releases are not offered as Saytwig updates. The checker is not yet connected to the interface; no update is downloaded or installed.

## What the first release is intended to include

- Finish native-device validation and polish of the compact black GPUI/GPUI Kit interface.
- Validate guarded insertion into the original application on native Windows, including focus changes, shortcut collisions and partial input dispatch.
- Package the reviewed speech runtime and measure cold/warm application latency on supported target hardware.
- Validate optional account-connected cleanup, with a clear explanation before any transcript leaves the device.
- Local history and explicit retention/deletion controls.
- GitHub-based update checks and signed release verification. Automatic installation remains disabled until a reviewed signing and release process exists.

These are delivery goals, not claims of completed or cross-platform-verified features. Physical microphone permissions, real Windows speech execution, OS accessibility, hotkey collisions, native UI rendering, installer behavior, and signed updates require target-platform validation.

## Smaller by design

Saytwig does not include other speech model families, a model picker, translation modes, analytics, or unrelated AI tools. Model weights are not embedded in the application executable. The speech runtime still includes Python and PyTorch; Rust + GPUI does **not** make those dependencies disappear. Build-size and startup claims will be published with reproducible measurements rather than inferred from the UI framework.

## Development

Read [AGENTS.md](AGENTS.md), [CONTRIBUTING.md](CONTRIBUTING.md), and [the architecture notes](docs/architecture.md).

```sh
cargo fmt --all -- --check
CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --lib --bins -- -D warnings
```

Native GPUI/CPAL builds need the appropriate platform graphics/audio development libraries. See each crate's README and the runtime packaging scripts for more specific prerequisites. Automated tests are intentionally excluded by the owner. Build, formatting, linting, static review, and manual visual inspection remain part of the workflow; they do not prove runtime correctness.

## Origin and credits

This project began as a proposed rework of [Handy](https://github.com/cjpais/Handy) in the `Artisians/handy-phonon2` repository. The owner then chose a newly authored application instead of continuing that implementation. We credit Handy and its contributors for the original project and inspiration. The new application source is being written independently; this is not a claim of a formal clean-room process.

This is the new standalone [Artisians/saytwig](https://github.com/Artisians/saytwig) repository. The previous repository is left unchanged. Earlier reset/rename plans are superseded; this source does not erase or alter the old project or its history.

Speech recognition is powered by [Fermion Research](https://www.fermionresearch.com/) and its [Phonon 2.5 English model](https://huggingface.co/FermionResearch/Phonon-2.5-English), derived from NVIDIA Parakeet. The native interface uses the [GPUI](https://gpui.rs/) ecosystem. Design work uses the actual [Impeccable](https://github.com/pbakaus/impeccable) workflow.

## Licenses

New Saytwig application code is licensed under [MIT](LICENSE). That license does not relicense model weights or bundled dependencies. Fermion's runtime is Apache-2.0; Phonon 2.5 English weights are CC-BY-4.0. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and the pinned [speech dependency notices](scripts/phonon/NOTICES.md). Any reused third-party code must retain its original copyright and license notices.
