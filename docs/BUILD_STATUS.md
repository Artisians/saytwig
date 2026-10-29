# Development build status — 2026-10-29

This is a freshly authored, unreleased Saytwig source snapshot. It is not an
installer or a signed release. The new standalone `Artisians/saytwig` repository
has been created and verified (repository ID 1439053466), leaving the old
repository unchanged. Earlier reset/rename handoff instructions are superseded.
Initial app source is being submitted through a draft PR and review gates.

## Observed checks

- Rust/Cargo 1.99.1; GPUI Kit 0.8.1 with its matching gpui-pre 0.3.10.
- Final workspace `cargo fmt --all -- --check`: passed.
- Final workspace `CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --lib --bins --locked -- -D warnings`: passed.
- Final native Linux `saytwig` executable linked after integrating the reviewed
  account, cleanup, clipboard, cancellation and settings-load fixes.
- Linux and Windows GNU production-only Clippy passed for domain/audio/desktop
  and account; Phonon passed Linux Clippy and Windows GNU compile checking.
- Linux and Windows MSVC CI dependency/feature metadata resolved. GitHub CI has
  not run because this source has not been published.
- Agent-driven execution of the actual Linux binary returned exit 1 with:
  `Saytwig needs a native desktop session. No microphone or account was opened.`
  This validates the unsupported-display path, not a rendered application.
- A separate hands-on call through the fresh production speech bridge used
  verified Fermion 0.2.15 / Phonon 2.5 English and a public 11-second audio clip.
  It completed correctly: readiness 6.419 seconds, inference 1.423 seconds during
  concurrent builds. This is not a Windows or complete application benchmark.
- A second bounded manual bridge operation kept one child resident: new-process
  readiness announcement 5.461 seconds, first decode 1.146 seconds, second decode
  0.646 seconds, identical successful output. The child was explicitly reaped.
  OS caches were uncontrolled and there was only one startup/two requests; this
  is evidence of resident reuse, not a general speedup. It excludes the Rust
  integrity pass/actor, microphone, UI, and Windows. Details are in
  `crates/phonon/README.md`.

The pre-insertion optimized Linux build completed with the pinned toolchain via
`CARGO_BUILD_WARNINGS=deny cargo build --release --locked --bin saytwig`.
Its executable was **41,541,568 bytes (39.6 MiB)**, SHA-256
`ae9c99eba9e3e1d208fd731e81095eecc1d01d3cc5b67af216eea390ff0dc9ad`.
It also reached the same truthful no-desktop-session exit when executed.
This is an executable-only measurement, excluding model weights, the Python /
PyTorch speech runtime, and system libraries. It is not a Windows installer size,
an old-versus-new application comparison, or proof of native UI behavior.

For context only, the unoptimized debug ELF was 561,984,104 bytes, SHA-256
`378107e8f11a02e957a641d9a5521b0d70d38022075aa2488ca1263b2e687686`.
Those debug and optimized artifacts precede the automatic-insertion integration;
the changed source requires a fresh final build before its artifact is delivered.

## Review corrections included

Independent source review found and corrected stale account/model binding,
late-completion cancellation ownership, incorrect save acknowledgements,
deferred clipboard side effects, microphone cancellation during actor work and
startup, dangling installer links, and Windows protected-store durability and
fail-closed persistence. Static review is not proof of every runtime path.

## Remaining release gates

The build execution sandbox has no display and cannot create an Xvfb display
socket. The computer-control tool's full documentation separately exposes a
supported cloud Linux desktop; this route has now been identified, but access to
the compiled application there is not yet established. Actual native rendering /
click flows, microphone permission/device behavior, accessibility, Windows ABI /
runtime execution, and complete app dictation remain unverified. No real app
account sign-in/grant, refresh, or paid cleanup request was performed. macOS
platform execution is also unverified. The user's computer remains out of scope.

The Windows speech payload is approximately 665 MB plus its inventory; only
19 unused PyTorch linker inputs were omitted (859,703,748 bytes). It is not a
signed/distributed runtime. Windows MSVC prerequisites and the exact shipped
license inventory remain packaging gates.

Windows shortcut-triggered automatic paste is now wired into the interface and
independently source-reviewed, with cancellation, focus/modifier checks, a
generation-bound result, and explicit Copy fallback. Manual Start remains
copy-only. A dispatched input batch does not prove target acceptance; a focus or
clipboard change can still race dispatch. Native Windows validation remains
required. The latest integration is awaiting its final native rebuild.

GitHub update-check infrastructure is not yet connected to the interface and
does not install updates. Only `saytwig-vX.Y.Z` tags are eligible, preventing
unrelated or legacy tags from being relabeled as Saytwig updates.
Signed updater installation, installer packaging and release publication are
not configured or claimed complete.

The owner explicitly chose no automated test files/suites. None are shipped in
this source snapshot; build, lint, static review and hands-on app operation are
the current quality workflow. See AGENTS.md and docs/quality.md.
