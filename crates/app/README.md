# Saytwig native app

Rust application using GPUI Kit 0.8.1 and its exact GPUI pre-0.3.10
stack. Only `component` and `assets` features are enabled. Impeccable's owner-approved compact black concept informed the interface;
concept PNGs are not screenshots of this native application.

## Native development launch

The product release target is Windows x64 only. Linux/macOS builds are development
paths, not a promise of supported product releases. Windows ARM builds fail early.

```sh
cargo run -p dictation-app --bin saytwig
```

Linux requires a native X11 or Wayland session and the GPUI platform libraries.
The application reports a clean failure before platform initialization when neither
`DISPLAY` nor `WAYLAND_DISPLAY` is present. These variables are not interpreted as
proof of device permission or successful rendering.

On Windows x64, **Set up** shows the download sources, disk requirements and
prerequisite before you choose **Download setup**. Initial setup downloads about
472 MB for the runtime and English model. It uses about 843 MB after installation
and about 1.9 GB during preparation, plus filesystem overhead. Microsoft Visual
C++ x64 runtime may be required. Saytwig does not install that prerequisite.
Startup never downloads these files. Other development platforms use the explicit
runtime configuration below.

Setup preserves ambiguous files after an interruption rather than deleting them.
Automatic recovery is not available yet. The [first-run setup guide](../../scripts/phonon/FIRST_RUN_SETUP.md)
describes the managed folder, verification and recovery limits.

A development runtime uses these explicit configuration values:

- `SAYTWIG_RUNTIME_DIR`: reviewed bundle containing `bundle.json`, `bridge.py`, and
  the platform Python runtime.
- `SAYTWIG_RUNTIME_SHA256`: independently trusted release digest for `bundle.json`.
  Never derive trust by hashing the adjacent mutable file automatically.
- `SAYTWIG_MODEL_STORE`: optional model store root, containing `phonon-2.5-en`.
- `SAYTWIG_DATA_DIR`: optional new Saytwig settings/history directory. Default is
  the platform's user data location, scoped to Saytwig. No legacy data is imported.

All integrity checking, installation, microphone enumeration/capture, inference,
and history/settings persistence run off the UI thread. Missing or invalid assets
are shown explicitly. The model-only download requires **Download English model**. Full setup requires
**Download setup** after its disclosure. Constructing the app never downloads
a model or opens a microphone.

## Actual flows

- Start / finish / cancel local dictation, with a two-minute recording cap.
- Explicit global-shortcut registration/rebinding on the GPUI event-loop thread;
  a successful saved registration is restored on the next launch. Failure does
  not masquerade as an enabled shortcut. In-window Escape cancels dictation.
- A small non-focusing status popup uses actual session phases. It has no fake
  amplitude waveform, speed, accuracy score, or timer. OS focus behavior still
  requires real-platform inspection before release.
- Explicit copy uses a dedicated clipboard owner, not the potentially blocked
  inference actor. Queue capacity is one, requests expire after two seconds,
  superseded requests are invalidated, and shutdown suppresses queued writes.
  An OS clipboard write already in progress cannot be undone by cancellation.
  Windows global-shortcut dictation captures the external foreground window/PID
  before showing the overlay and attempts one guarded paste of the final local or
  explicitly opted-in cleaned result. Manual in-app Start is Copy-only; unsupported
  platforms, missing targets, changed focus, held modifiers or errors retain the
  result for explicit Copy. Saytwig never focuses a target or pastes into itself.
  Paste keeps the new text on the clipboard; it does not read/restore prior content.
  A receipt means input dispatched, not proof that the receiving app inserted text.
  Failed/partial dispatch is never automatically retried: check the target before
  copying to avoid duplicate insertion.
- History keeps original and cleaned text separately. No audio recordings are
  persisted by the app. There is no implicit retention deletion.
- Microphone selection, vocabulary hints, and shortcut preferences are persisted
  with revision guards. Malformed/newer data disables writes rather than replacing
  existing bytes. Save acknowledgement comes from the background write result.
- ChatGPT accounts are opened only after an explicit account action. Sign-in
  authorization is passed directly to the OS browser, never exposed in UI text or
  logs. Connect/continue/activate/sign-out and model loading are cancellable where
  the account API supports it. No existing application credentials are imported.
- Transcript sharing is a separate, default-off consent action after connecting
  an eligible account and choosing one of its models. Catalogs and requests are
  tied to the active profile identity; the bearer identity is checked immediately
  before cleanup. Account switching/revocation or failure retains original text.
  No API-key or billing fallback exists. Connected accounts without plan-use permission
  have an explicit browser consent action; sharing remains off. Saved accounts always
  offer sign-in recovery and sign-out, including disconnected/pending credentials.
- The first successful plan-enabled sign-in shows a dismissible explanation. Its
  acknowledgement is one versioned boolean in local settings, not a transcript-sharing
  grant. Failed settings writes remain visible. Manage usage opens only
  https://chatgpt.com/settings/usage through the owned account worker.
- An exact structured ChatGPT plan usage-limit error retains the original transcript
  and pauses sharing. It does not imply the entire plan is exhausted. Only an explicit
  sharing action resumes cleanup; generic HTTP 429 errors do not get this diagnosis.
  Account commands have two queue slots and events have eight. UI submission never
  blocks, and shutdown releases background event backpressure.
- Preferences offer session-only, default-off permission for stable release checks.
  Permission does not initiate a request: a separate explicit Check action contacts
  GitHub. A dedicated one-slot worker uses the updater's 15-second/256-KiB bounds;
  cancellation discards the result and prevents a new check until the old request
  finishes. Closing the app suppresses its result. No startup/background checking,
  asset download or automatic installation is implemented. Only validated release
  links can be opened, and only after an explicit click.
- `SAYTWIG_BUILD_CHANNEL` and `SAYTWIG_BUILD_COMMIT` are compile-time CI metadata;
  unknown/absent channel means development. The displayed source version is Cargo's
  package version. Nightly/development checks deliberately use prerelease baseline
  `0.0.0-0` to offer any stable release, not equate development 0.1.0 with stable
  0.1.0. The UI states that such availability is not an upgrade recommendation.

## Ownership and cancellation

Local session events carry `Generation`. Cloud commands and terminal events also
carry monotonic request identities. A stale completion cannot clear a newer
cancellation handle, mutate the current account controls, or replace its result.
Microphone cancellation has its own nonblocking generation-owned handle so a slow
filesystem or device-enumeration operation cannot delay stopping active capture.
The UI cancels an already-stale Recording event's handle immediately. Quit cancels
active audio, model, cloud and clipboard work and requests owned-thread shutdown.
Each shortcut dictation owns a fresh delivery gate armed before sharing with the
clipboard worker. Final output carries the same session generation. New dictation,
explicit Copy, cancellation and quit permanently revoke that gate without blocking
GPUI on the native clipboard. Revocation rejects effects that have not entered their
commit; an already-entered OS commit may finish and dispatched input cannot be
recalled. Focus is checked again by the native adapter at dispatch, but Windows
SendInput cannot atomically target an HWND: physical focus changes can still race.
No claim of guaranteed targeting or delivery is made.

## History rendering

History renders 40 entries per page and checks one additional match to enable Next.
Each row shows up to 100 characters. The selected preview shows up to 4,096
characters and labels shortened text. Copy reads the full stored selection only
when clicked. These limits do not delete or shorten stored history.

Search accepts up to 512 characters and scans original text with streamed Unicode
case folding. It does not allocate a lowercase copy of every entry. The scan still
runs on the UI thread and can examine the retained file, whose size is bounded by
storage. Worst-case search latency has not been measured.

The native header embeds the approved first-party `assets/saytwig.svg` once and
uses GPUI's image cache. It does not load website assets or fonts. This header mark
does not configure the Windows executable's shell icon.

## Guidance and verification

BeautifulRust official source was read at commit
`f92961282b83e11757f076e787b4725b38d07b56`: application-shaped concrete APIs,
message-passing ownership, `Result` at failure boundaries, minimal dependencies,
private fields, safe code, and format/lint review. Local AGENTS.md overrides its
generic test commands: **no automated tests or test scaffolding**.

Allowed checks:

```sh
cargo fmt --all -- --check
CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --lib --bins -- -D warnings
cargo build -p dictation-app --bin saytwig
```

The build execution sandbox has no display; its permitted Xvfb launch cannot bind
listening sockets. A separate supported cloud Linux desktop **did render the actual
optimized app**, after the coordinator used process-local `MESA_VK_WSI_DEBUG=noshm`,
`VK_DRIVER_FILES` pointing to the workspace Lavapipe manifest, and
`LIBGL_ALWAYS_SOFTWARE=true`. This selects normal software rendering and non-SHM
presentation; it does not change host IPC, permissions or sandboxing. The original
black-window/Mesa SHM failure remains relevant environment evidence, not a successful
render. Software rendering is not GPU-performance evidence.

Compilation, source review and rendered controls do not establish physical microphone
behavior, Windows target insertion, OS permission prompts, screen-reader behavior or
end-to-end account consent. These remain separate hands-on acceptance gates. No actual
account grant or transcript cleanup was performed while building this interface.
No automated tests or test scaffolding are added or run.
