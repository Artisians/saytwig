# Architecture and verification boundaries

Saytwig is a new Rust workspace rather than a refactoring of the previous Tauri
application. Old project research may inform requirements; implementation must
not be copied and then described as original.

| Boundary | Responsibility |
| --- | --- |
| `dictation-domain` | Request generations, state transitions, cleanup consent, original-text recovery, versioned settings/history |
| `dictation-audio` | Explicit CPAL microphone lifecycle, recording bound, 16 kHz mono conversion |
| `phonon-service` | Owned resident child, hash-verified local runtime/model, authenticated loopback, bounded/cancellable speech requests |
| `cleanup-service` | Transcript-only cloud transport, separate consent, conservative prompt, account model catalog, completed-result validation |
| `dictation-desktop` | Global shortcut lifecycle and explicit copy/guarded text delivery; platform capabilities |
| `dictation-app` | GPUI composition and user interaction; no blocking model work on the UI thread |

The session generation is not a persistent history identifier. History entries
need independent unique IDs across launches. A cancellation must invalidate the
session **and** cancel microphone, model, and cleanup work. Consent revocation
cancels pending cloud I/O and rejects a late output; bytes already sent cannot be
recalled from a provider.

## Data movement

1. Recording starts only after an explicit user action.
2. Audio stays local and is normalized to 16 kHz mono.
3. Local inference returns the original transcript.
4. If the user enabled cleanup and an account is connected, only that transcript
   is submitted. Dictated text is data, separate from editing instructions.
5. Only a valid current result may be copied or pasted. Changing focus, cancelling,
   or revoking consent must not cause stale insertion.
6. History is local, with retention/deletion controlled by the user. No automatic
   import from the old fork is performed.

## Evidence required before release

- Real Windows startup and ASR timings, including cold/warm definitions, runtime,
  model hashes, hardware, repetitions, and raw samples.
- Actual microphone permissions/device disconnection, hotkey collisions,
  foreground-window changes, clipboard formats, paste modifiers, and cancellation.
- Native UI screenshots and keyboard/accessibility flows—not just concept PNGs.
- Runtime ABI/import checks on each packaged architecture.
- Fresh install, upgrade, rollback behavior, and signed update verification.
- License/notice inventory of the exact shipped artifact.

Linux measurements of the runtime are useful development evidence, but cannot
be presented as Windows latency or as a complete application benchmark.
