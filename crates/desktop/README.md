# dictation-desktop

Fresh host adapters for Saytwig. Constructing a domain session or importing this
crate has no desktop side effects. Native constructors are explicit, and no code
activates/focuses another window or logs clipboard/transcript contents.

## Host integration

- Keep one `native::NativeHotkeys` inside `hotkey::HotkeyLease` on the native event
  loop thread. Windows requires that thread to pump Win32 messages; macOS requires
  the main event loop. Register a user-selected binding with `bind`, poll signals,
  and drop/unbind on shutdown. Rebinding registers the replacement first. Repeated
  presses are debounced. The global event receiver is a **single host consumer**:
  do not run independent consumers that can steal each other's events.
- Create `native::NativeDesktop` explicitly on its owner thread. Keep it alive
  while offering clipboard contents (particularly on X11).
- At a global-shortcut dictation request, capture `native::foreground_target()` **before**
  opening/focusing app UI, then `DeliveryGate::arm(session_generation, target)`.
  Use a fresh `Arc<DeliveryGate>` per request, arm it before sharing with the
  desktop worker, and call its nonblocking `revoke` on cancel/new start/shutdown.
  A revoked gate is permanently unusable. A commit already past its final check
  may finish; revocation is not undo. Blocking `cancel` remains available off the
  UI thread when a completed-operation fence is required. Generation counters
  are process-local, not history identifiers. Manual app-button starts should
  not capture or auto-paste; Windows also refuses this app's own process.
- Only deliver caller-approved final text with the matching generation. Manual
  starts use explicit Copy; global-shortcut starts may use guarded Windows paste.
  Cancellation must call both domain cancellation and the per-request gate's
  nonblocking `revoke`. A cancelled/superseded/consumed request is rejected.
- Optional Windows paste checks original foreground window + process twice and
  refuses held keyboard modifiers. It checks that the clipboard still has the written revision under the native
  clipboard lock, then submits one Ctrl+V batch without changing focus and
  immediately releases the lock (never waits for the target while locked). Windows `SendInput` cannot atomically target an HWND: a physical focus
  change can still race the check, clipboard contents can change after dispatch
  but before the recipient reads them, and elevated applications may reject input.
  Do not promise guaranteed targeting or paste success; retain explicit Copy.
- `ClipboardPolicy::KeepNewText` never reads prior clipboard contents.
  `PreserveText` supports empty/plain-text Windows clipboards only; rich formats
  are rejected **before** overwrite. Snapshot revision checks and restoration
  happen while the clipboard is locked. Newer clipboard contents are never
  deliberately overwritten during restoration.
- A successful paste receipt means **input dispatched**, not application read.
  Restoration requires an explicit consumer acknowledgement or user action via
  `RestoreTicket::restore`; never use a blind sleep/timer. On ambiguous partial
  input rejection, clipboard text is left available rather than prematurely
  restoring it. Cancellation cannot retract already dispatched OS effects.
- `tray` is optional on Windows/macOS. `TrayHandle` owns an explicitly supplied
  icon/tooltip and supports visibility changes. It does not currently supply a
  menu or a UI action dispatcher. The host event loop requirements still apply.

## Honest platform scope

Windows builds support **x64 only**. Non-x86_64 Windows targets fail compilation
with an explicit diagnostic; no Windows ARM or 32-bit support is offered. This
restriction does not remove macOS or Linux ARM platform paths.

| Capability | Windows x64 | macOS | X11 | Wayland/headless |
| --- | --- | --- | --- | --- |
| Global shortcut | native, host message loop | native, main loop | native | unsupported |
| Explicit clipboard copy | native | arboard | arboard, owner kept alive | unsupported here |
| Guarded paste/text restoration | native | unsupported | unsupported | unsupported |
| Optional tray lifecycle | native | native | unsupported | unsupported |

`Capabilities::detect` also accounts for enabled build features. Permission
fields are unknown/check-at-use, never fictitious grants. The host's audio layer
reports actual microphone-open errors. This crate does not request accessibility
permissions or implement a Wayland portal.

## Implementation references and verification

Primary API references: [global-hotkey 0.8](https://docs.rs/global-hotkey/0.8.0/global_hotkey/),
[arboard](https://docs.rs/arboard/3.7.0/arboard/),
[tray-icon](https://docs.rs/tray-icon/0.26.0/tray_icon/),
[SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput),
[clipboard sequence numbers](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getclipboardsequencenumber).

Cloud compilation/static analysis is not a Windows/macOS device run. Native
shortcut registration, real clipboard ownership, paste permissions, target focus
races and tray rendering still require an authorized native acceptance pass.
Per the owner's latest instruction, do not add or run automated tests.
