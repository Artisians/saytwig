# dictation-audio

Newly authored microphone capture and PCM normalization for the native app.
No legacy Handy implementation is imported or copied.

Windows builds support **x64 only**; other Windows architectures fail compilation
with an explicit diagnostic. This restriction does not apply to macOS/Linux ARM.

- Microphone acquisition is explicit through `Microphone::start` or
  `Microphone::start_cancellable`. Prefer the latter with the request's owned,
  nonblocking cancellation predicate. It checks before setup, around stream play,
  and throughout recording. Call off the UI thread after user action/permission.
  Native setup itself cannot be interrupted, but a stream prepared after
  cancellation is dropped without an explicit play call.
- CPAL stream ownership stays on its worker thread. Stop, cancel, device failure,
  and the 120-second audio/wall-clock limit close that owned stream.
- Callback work is bounded, does not allocate, and uses a nonblocking buffer lock.
  Lost/invalid samples fail the recording instead of silently pasting partial text.
- Channels are averaged to mono. Rubato FFT resampling applies its anti-aliasing
  filter; documented output delay is removed and duration is preserved.
- The returned `Pcm16k` contains finite, normalized, mono 16 kHz audio. Pass its
  `samples()` to the separately validated model boundary.
- Keep `Capture::cancellation()` in generation-tagged UI state. Its cloneable
  `CaptureCancellation::cancel()` signals the capture worker without blocking or
  waiting for the application actor. Consuming `Capture::cancel()` still joins
  driver teardown off the UI thread. Cancel handles from stale Recording events.
- No samples or device identifiers are logged. Dropping a capture cancels it.

Explicit `Microphone::list_devices` enumerates names without opening a stream;
call it off the UI thread. Backend names are not guaranteed unique or stable.
Physical Windows/macOS/Linux microphone validation remains a separate native
acceptance pass. Per owner policy, do not add or run automated test suites.

Primary API references used for this implementation:
[CPAL 0.16](https://docs.rs/cpal/0.16.0/cpal/),
[Rubato FFT resampler](https://docs.rs/rubato/0.16.2/rubato/struct.FftFixedInOut.html),
[Rubato delay and partial processing](https://docs.rs/rubato/0.16.2/rubato/trait.Resampler.html).
