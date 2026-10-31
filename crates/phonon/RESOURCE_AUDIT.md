# Resource and ownership audit

Source inspection, 2026-10-30. These are structural bounds, **not measured RSS or
latency improvements**. No automated test suite is used. Build/lint and real-app
observations must be reported separately from this audit.

## Changed boundaries

- App effects: one actor thread, one pending start/install, one reserved final
  transcript save, latest settings, coalesced reload/device requests. UI sends
  never wait: rejected commands retain their payload in `TrySendError` so the UI
  can preserve text and display a failure. Stop/cancel/shutdown use priority
  atomics, independent of payload capacity. Four queued events plus one
  producer-owned event provide background backpressure; normal operation does
  not discard completed recognition. Accepted saves are handled before the next
  start and flushed on shutdown. Explicit shutdown cancels pending recognition
  and may suppress remaining UI notifications; it does not silently prune data.
- Account work has two command slots and eight event slots. Foreground sends
  use `try_send` and reject a second active request without replacing its
  cancellation owner. Rejected cleanup retains the original transcript.
  Background event backpressure checks an independent shutdown flag; quitting
  cannot wait on a full event queue or a blocking shutdown enqueue.
- Captured 16 kHz PCM transfers ownership into recognition rather than cloning
  it. At the 120-second limit that removes a 7,680,000-byte float-buffer copy.
  Consuming WAV conversion releases those floats before inference. Multipart
  transfer borrows the WAV between small prefix/suffix segments rather than
  allocating another full audio body. These byte counts describe source-level
  buffers, not allocator/RSS measurements. Vocabulary normalization builds at
  most 128 characters per term, rather than first allocating a vector of every
  whitespace-separated piece of an arbitrarily larger input.
- Microphone start acquires a process-global worker lease. The worker retains it
  through native stream destruction, including after the caller's startup
  timeout/error. A stuck native worker therefore prevents additional capture
  workers instead of accumulating detached threads. Stop/cancel messages have
  one slot; cancellation also sets its atomic flag, so a full slot cannot lose
  cancellation. Thread creation is fallible rather than panicking.
- Persistence uses a nonblocking cooperative file lock and reports contention.
  Serialization stops before exceeding the existing 16 MiB file limit, rather
  than serializing an arbitrarily larger value before checking. Version
  discovery ignores the data payload instead of retaining a full JSON value
  tree alongside the typed history. It still scans JSON twice and preserves
  typed duplicate-field checking. Envelope metadata is borrowed while writing;
  unknown fields, conflict detection, atomic replacement and history remain.

## Existing useful bounds

The speech actor has one owned child process, one active recognition, a
120-second/1,920,000-sample recording limit, bounded HTTP response/text sizes,
startup/inference deadlines and a 15-minute app idle residency policy. Runtime
launch still verifies the pinned complete inventory and model before executing
owned Python; there is no timestamp-only integrity cache. No recording or
transcript logging is introduced.

## Remaining gaps, not hidden by queue bounds

- Native microphone/device calls and stream destruction can still block inside
  an OS driver. The lease bounds worker count, not native-call duration;
  `Capture::stop`, cancellation and destruction can still wait for teardown.
- Runtime discovery currently performs full hashing, and launch repeats it.
  Deferring discovery hashing is a separate trust-path change requiring review;
  full cancellable verification immediately before execution must remain.
- Model installation interrupted by process death can leave UUID staging
  directories. Automatic deletion of ambiguous existing files is not part of
  this patch. Recovery needs explicit ownership-aware handling.
- History is not capped by entry count or silently deleted. It remains bounded
  on disk by 16 MiB and can have a larger typed in-memory representation. A full
  history snapshot still crosses the event channel after each successful save.
  UI pagination and bounded previews are separate foreground work; search
  indexing has not been benchmarked.
- Individual files, recordings and queue lengths are bounded, but that is not a
  total-process memory budget. Native model allocations, allocator overhead,
  device buffers, UI retained state and concurrent application work also matter.

The existing public-fixture measurements in `README.md` exercise the production
Python bridge, not these Rust actor, persistence or microphone changes. No native
Windows execution or new performance measurement is claimed here.
