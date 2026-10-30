# Development build status — 2026-10-29

Saytwig is a newly authored, unreleased local speech recognition app. The standalone
[repository](https://github.com/Artisians/saytwig) is public; the previous Handy
fork is unchanged. Reset/rename handoff instructions are superseded.

## Published source and CI

- Initial source: [`9fed89d`](https://github.com/Artisians/saytwig/tree/9fed89dae1b9666dd9e913ac0443888fa8a15ac2), on `nightly` (86 remotely verified files).
- [Draft PR #1](https://github.com/Artisians/saytwig/pull/1) proposes promotion to `stable`. `stable` and `main` still contain the repository seed, not a validated app release.
- Both initial [push CI](https://github.com/Artisians/saytwig/actions/runs/40973785994) and [PR CI](https://github.com/Artisians/saytwig/actions/runs/40973825844) passed: formatting, strict production Clippy, native Linux linking and native Windows x64/MSVC linking.
- A successful Windows build is **not** Windows device, microphone, shortcut, paste or UI execution evidence.
- Follow-up source changes are under review. Initial green CI does not validate a newer commit automatically. No stable release, signed installer or distributable speech runtime is published.

## Build and native inspection evidence

Pinned Rust/Cargo 1.99.1; GPUI Kit 0.8.1 with matching gpui-pre 0.3.10.
Workspace formatting and production-only strict Clippy passed for the integrated
paste-enabled source before its optimized Linux build. Relevant later account,
platform and interface changes have separate format/lint checks; a refreshed
native debug build is in progress and is not represented by the artifact below.

The paste-enabled optimized Linux executable is **41,552,320 bytes (39.6 MiB)**:

```text
SHA-256 5b394883de11e9611c0fe32716b8a19caf08988df0bd72e09d3c41a14ca805e8
```

This executable-only measurement excludes model weights, Python/PyTorch runtime
and system libraries. It is not a Windows installer size or an old-versus-new
performance comparison. The earlier pre-insertion executable was 41,541,568 bytes.

The paste-enabled binary was launched on the supported **cloud Linux desktop**.
It rendered through the existing Mesa software Vulkan driver using its documented
non-shared-memory presentation option. Observed hands-on flows:

- Missing-runtime state rendered; Start was unavailable and cloud cleanup stayed off.
- Dictation, History and Preferences navigation worked.
- A vocabulary preference was entered and saved; the success notice and versioned
  local settings file agreed. No account credentials or audio were used.
- Empty history rendered correctly.

These observations concern that exact binary, not subsequent design refinements.
Software-rendered cloud inspection does not establish native GPU performance,
Windows behavior, screen-reader support, or complete dictation. The headless
executor also correctly exits with a native-desktop-required message rather than
opening microphone/account resources.

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
- Follow-up UI source wires explicit, default-off GitHub contact and stable-release
  checking. It does **not** download or install updates. Only `saytwig-vX.Y.Z`
  releases are eligible. No signing keys or automatic installation are configured.
- Consumer runtime/model installation, application packaging and full dependency
  redistribution notices/source obligations. The development Windows speech payload
  is approximately 665 MB; it is not a signed or redistributable runtime. In particular,
  the bundled soundfile/libsndfile dependency still needs exact corresponding-source
  and relinking obligations resolved before redistribution.
- Performance audit findings include producer/event backlog and history allocation
  bounds; policies are not proof these implementation gaps are closed. User history
  must not be silently pruned to meet a memory target.
- macOS native execution is unverified. The user's computer remains out of scope.

The owner chose **no automated test files/suites**. Build, formatting, lint,
independent source review and hands-on app inspection are the quality workflow.
See [quality guidance](quality.md) and [engineering rules](../AGENTS.md).
