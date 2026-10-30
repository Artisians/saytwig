# Development build status, 2026-10-30

Saytwig is an unreleased local speech recognition app.

## Published source and CI

- The first app-only [Windows ZIP prerelease](https://github.com/Artisians/saytwig/releases/tag/saytwig-experimental-20261030-b8c08a0) identifies source `b8c08a0547c9a4af89b98f10541b1c765c0974f6`. It does not include the speech runtime or model.
- [PR #2](https://github.com/Artisians/saytwig/pull/2) migrates application source and the landing page to `main`. Feature PRs target `main`; successful four-platform packaging after each merge publishes a commit-addressed experimental nightly prerelease. See [release flow](release.md).
- [PR #1](https://github.com/Artisians/saytwig/pull/1) was closed without merge. Historical `nightly` and `stable` branches remain intact. No stable application release has been promoted.
- Native Linux and Windows production checks passed on the app baseline. The migration adds macOS native checks, but macOS packaging and all-platform nightly publication remain blocked on [original license evidence](../scripts/release/STATUS.md).
- Native linking does not establish microphone, shortcut, paste, or UI behavior on Windows. No stable release or signed installer is available.

## Build and native inspection evidence

Pinned Rust/Cargo 1.99.1; GPUI Kit 0.8.1 with matching gpui-pre 0.3.10.
Workspace formatting and production-only strict Clippy passed before the latest
optimized Linux build. Cargo completed that build in 9m47s. The executable is
**41,886,272 bytes**, about 39.9 MiB:

```text
SHA-256 7908b8346f9b88759498069dae8ade580b1f2f7a90b27c4ba9d16935248c7267
```

This executable-only measurement excludes model weights, the Python/PyTorch
runtime, and system libraries. It is not a Windows installer size or a performance
comparison.

The binary ran on the supported cloud Linux desktop through the existing Mesa
software Vulkan driver. At 02:00 to 02:06 UTC, hands-on inspection established:

- The verified development runtime passed the installation check and enabled
  **Start**. Start reached microphone acquisition without the previous GPUI panic.
- This desktop has no ALSA capture device. The app displayed an actionable
  microphone error, returned to idle, and allowed dismissal and another Start.
- **Escape** during a fresh-process startup returned the app to idle without a
  microphone error. A subsequent Start displayed the 340-by-52 recording pill
  and reached the same microphone error. The pill disappeared on completion.
- A later click on the pill's **Cancel** arrived after the microphone failure.
  That button's cancellation behavior is not established by this observation.
- Normal window close removed the app. Two later process snapshots found neither
  this app binary nor its configured speech child. No force kill was used.
- Cleanup stayed off. No account connection, provider request, or update contact
  was attempted in this run.

Earlier build `ead464b1` established navigation, persisted vocabulary, visible
keyboard focus, and the 760-by-521 layout. Its Start crash was reproduced before
this fix. Those earlier observations are not a full repeat on the latest binary.
The latest inspection used an 1180-by-812 window.

No full microphone-to-transcript operation, startup benchmark, or RSS measurement
was completed. Software-rendered cloud inspection does not establish native GPU
performance, Windows behavior, or screen-reader support. The accessibility tree
exposed only the X11 window. Headless startup exits with a native-desktop-required
message before opening microphone or account resources.

## Speech engine observations

A hands-on call through the fresh production speech bridge used verified Fermion
0.2.15 / Phonon 2.5 English and a public 11-second audio clip. Initial readiness
was 6.419 seconds and inference 1.423 seconds during concurrent work.

A second bounded manual operation kept one child resident: new-process readiness
5.461 seconds, first decode 1.146 seconds, second decode 0.646 seconds, with
identical successful output and explicit process reaping. OS caches were
uncontrolled; one startup and two requests are evidence of resident reuse, not a
general speedup. These measurements exclude the Rust integrity pass/actor,
microphone, UI and Windows. Details: [speech bridge](../crates/phonon/README.md).

## Remaining product and release gates

- Full microphone-to-transcript operation through the native app; interruption,
  repeated capture, accessibility and target-application insertion on Windows x64.
- Real supported account sign-in/grant/refresh and opt-in cleanup. No credentials,
  OAuth grants or paid cleanup requests were used for development QA.
- Windows shortcut-triggered paste is implemented and independently reviewed;
  manual Start remains copy-only. Cancellation/focus/modifier checks reduce races
  but cannot prove another application accepted a dispatched input batch.
- The interface has explicit, default-off GitHub contact and stable-release
  checking. It does **not** download or install updates. Only `saytwig-vX.Y.Z`
  releases are eligible. No signing keys or automatic installation are configured.
- The explicit Windows x64 runtime and model installer is implemented but has not
  run on Windows. Its pinned inputs total 472,062,622 download bytes. Interrupted
  or unknown preparation directories still require manual recovery. See
  [first-run setup](../scripts/phonon/FIRST_RUN_SETUP.md).
- Native application notices pass for the current lockfile: 474 Windows packages
  and 577 Linux packages, with zero reported blockers. Each artifact workflow must
  regenerate notices from its own checkout. This receipt does not cover the speech
  runtime. The soundfile/libsndfile runtime bundle still needs exact
  corresponding-source and relinking obligations resolved before redistribution.
- The reviewed resource changes bound command and event queues, remove full audio
  buffer copies, and limit history rendering to 40 rows. Search still scans the
  retained history on the UI thread. Native device calls can block inside drivers.
  These source-level limits are not measured latency or RSS improvements. See the
  [resource audit](../crates/phonon/RESOURCE_AUDIT.md).
- macOS native execution is unverified. The user's computer remains out of scope.

The owner chose **no automated test files/suites**. Build, formatting, lint,
independent source review and hands-on app inspection are the quality workflow.
See [quality guidance](quality.md) and [engineering rules](../AGENTS.md).
