# Local Phonon service

New application integration with the official Fermion Python API; no UI framework
or microphone capture dependency. Windows distribution is **native x64 only**;
Windows ARM (including x64 emulation) and Windows x86 are unsupported. See the
[development packaging contract](../../scripts/phonon/WINDOWS_X64.md).
All blocking calls belong on a background
thread, including bundle/model verification. The service never downloads on
startup, warm-up, or transcription.

## Host contract

1. On supported native Windows, macOS, and Linux targets,
   `managed_runtime(store_root)` discovers the reviewed target-specific bundle
   without network access. `setup_capability()` supplies immutable consent metadata
   without file access or device queries. Only an explicit setup action calls
   `install_runtime_requested(store_root, &cancellation, progress)`; its expected
   inventory digest and canonical helper/asset fingerprint are compiled into
   the app. See [first-run setup and limitations](../../scripts/phonon/FIRST_RUN_SETUP.md).
   The [macOS and Linux contract](../../scripts/phonon/UNIX_SETUP.md) describes
   their separate asset pins, Python archive receipts, and acceptance limits.
   Development hosts can instead supply a directory and **independently trusted**
   `bundle.json` SHA-256 to `RuntimeBundle::open`. Never derive trust from an
   adjacent mutable digest file. These hashes are integrity pins, not signatures.
2. For an existing model, call `verify_model`. To explicitly provision a model,
   call `install_requested(&bundle, store_root, &cancellation)`. Unknown nonempty
   directories and existing unverified model installations are not adopted or
   replaced. A reserved partial archive may resume after explicit retry; the
   complete immutable hash is required before reconstruction/publication. After
   successful publication, the still-verified compressed cache is removed on a
   best-effort basis; offline use needs only the installed model.
3. Create `PhononService::spawn(bundle, model_directory, policy)`. Choose
   `WarmPolicy::IdleTimeout(Duration::from_secs(900))`, `KeepLoaded`, or
   `ReleaseAfterRequest` deliberately. Spawn itself does not load the engine.
4. `warm(&token)` loads it. `transcribe(Audio16k::new(samples)?, vocabulary,
   &token)` accepts finite mono 16 kHz PCM, at most 120 seconds. Vocabulary is
   whitespace-normalized, case-insensitively deduplicated, limited to 25 phrases
   and 128 characters each; rejected excess is not sent. It is a recognition
   hint, not an instruction prompt or post-transcription text replacement.
5. `snapshot()` exposes phase/error. `Cancellation::cancel()` interrupts the
   associated operation; `stop()` invalidates all pending/in-flight work and
   waits for the owned child to exit. `shutdown()` closes the service. Last-handle
   drop also closes it. Concurrent warm/transcribe calls return a busy error
   rather than accumulating recordings. The host must separately invalidate its
   UI/session generation so stale results cannot be pasted.

The worker owns exactly one child and serializes requests. Launch configuration
and a fresh random secret travel through its private stdin, not arguments or
inherited provider variables. The child binds an OS-assigned IPv4 loopback port
and announces it through stdout. A challenge/HMAC identity proof is checked
before credentials or audio are sent. Upload stays on that **same TCP
connection**; the client does not reconnect to a potentially reclaimed port.
HTTP headers, replies, multipart uploads, WAV frames and transcripts are bounded.
Address/port reuse is disabled; Windows uses `SO_EXCLUSIVEADDRUSE` before bind.

The environment is allowlisted, Python starts isolated with bytecode writes
suppressed, and the production Windows interpreter has an explicit `._pth`
without `import site`. All inventory members and reconstructed model inputs are
hashed before launching. Cancellation is checked during hashing. An owner
watchdog handles parent exit; normal errors/cancellation explicitly terminate and
wait for the child. Python network audit guards and offline flags prevent Python
outbound connections/downloads; these are not an OS sandbox for arbitrary native
code. The pinned upstream native CPU implementation is trusted vendor code.

## Immutable upstream inputs

- Fermion Research **0.2.15**, official wheel SHA-256:
  `21ff7e5dfc6bba40850e82601c16224879f273ea9096a2274f95cec7e6e03ec8`.
- Phonon 2.5 English revision
  `da200358d638886bc96c758c672ad5568828b119`, compressed FSQ **157,616,598 bytes**,
  SHA-256 `87dacb412f42c736967345a42455c05fa7b1835fd2d0b40ce2fbfbfd415bdadd`.
- Reconstructed weight container **177,479,389 bytes**, SHA-256
  `c57119c0f04756932acc1d60d90506346e3708d69f00e654084acc7fe507fe27`.
  Config and packed-manifest hashes are separately pinned in source/assets.

Primary references: [Fermion speech API](https://www.fermionresearch.com/docs/speech/),
[changelog](https://www.fermionresearch.com/docs/changelog/),
[immutable model revision](https://huggingface.co/FermionResearch/Phonon-2.5-English/tree/da200358d638886bc96c758c672ad5568828b119),
[official runtime metadata](https://pypi.org/pypi/fermion-research/0.2.15/json).
Upstream code and weights retain their own licenses; see
[`NOTICES.md`](../../scripts/phonon/NOTICES.md).

## Packaging and current evidence

`python -B scripts/phonon/assemble.py --cache CACHE --output NEW_DIRECTORY`
constructs a new Windows CPU payload from hash-locked upstream artifacts. It
refuses an existing destination and never overwrites a mismatched cached asset.
It preserves executable modules, DLLs, headers, metadata and licenses. Only 19
PyTorch Windows `.lib` link inputs are omitted: **859,703,748 bytes**. The pinned
Fermion CPU code loads prebuilt kernels, not runtime-built C++ extensions.

A cloud assembly on 2026-10-29 contained 20,483 inventory files totaling
**664,949,368 bytes** (plus a 4,192,099-byte inventory). Its manifest digest was
`808cf201f1c68f0da0eecc6571edde2aacbca546f61f4f3bc4960eabd71d7f95`.
This is a development artifact, **not a signed release trust anchor**. Source or
license changes require a fresh assembly and digest. Model weights are separate.

Hands-on cloud runtime inspection on 2026-10-29 used the newly authored bridge,
Linux on an Intel Xeon Platinum 8573C, Python 3.12.14, upstream Fermion 0.2.15 and Torch 2.7.1+cpu, the pinned model,
and the public Whisper JFK fixture (11 seconds, mono 16 kHz). The fixture source
FLAC SHA-256 was `63a4b1e4c1dc655ac70961ffbf518acd249df237e5a0152faae9a4a836949715`.
One manual launch emitted its loaded-engine announcement in **6.419 seconds**;
a subsequent HMAC probe authenticated the process. One manual request returned
HTTP 200 in **1.423 seconds**, 22 words, including the known
public phrase. Transcript SHA-256:
`37d003a932256f11d07e00d0c1478443140ea5e87d817b0f8bc577c1d2aa2e1b`.
The worker remained resident between interactions and was explicitly terminated
and reaped. No private speech or transcript was logged.

These single observations are not a benchmark or speedup claim: other builds
were running, cache state was uncontrolled, and the Linux development bundle
uses the host standard library. They cover the production Python bridge, not the
complete Rust actor/UI/microphone lifecycle or Windows packaging. The Rust
library passed Linux production-only compilation/strict Clippy and a Windows
`x86_64-pc-windows-gnu` cross-compilation check using MinGW. The latter checks
source compatibility, not native execution. No automated test files/suites are
shipped, per the owner's instruction.

**Release blockers:** real Windows execution/ABI and microphone/UI inspection,
Microsoft Visual C++ runtime prerequisite (including `msvcp140.dll`), trusted
runtime delivery/signing, and manual install/cancel/retry/upgrade validation.
Linux assembly or cross-compilation does not establish native Windows behavior.


### Manual resident reuse, 2026-10-29 22:55–22:57 UTC

The current packaged production bridge was operated interactively from a Python
REPL, not a new test program or suite. One new child was launched with the
verified local model; two requests used the same public audio and running child.
For each request, the operator verified the challenge/HMAC response before
sending the multipart WAV on that same connection. Only counts, timings and
hashes were inspected, not private transcript logs.

| Observed operation | Elapsed | Samples |
| --- | ---: | ---: |
| New process to loaded-engine port announcement | 5.461 s | 1 |
| Initial authenticated readiness probe | 18.123 ms | 1 |
| First 11-second public audio request | 1.146 s | 1 |
| Resident-process authenticated readiness probe | 7.327 ms | 1 |
| Second identical audio request, same live child | 0.646 s | 1 |
| Explicit SIGTERM and wait/reap | 115.483 ms | 1 |

Both requests returned HTTP 200 and 22 words, including the known public phrase.
Both transcript hashes were
`37d003a932256f11d07e00d0c1478443140ea5e87d817b0f8bc577c1d2aa2e1b`.
The WAV SHA-256 was
`36002c29a362518b874e502ac121a7d66073f3948d5aa8b5c68fca0b648a3896`.
The existing child remained alive between requests without another spawn, and
was explicitly reaped with exit status -15 at the end. No recoverable lifecycle
failure was observed in this bounded operation.

The launched command was the existing production entry point:
`.local/runtime-linux/python/python.exe -I -B .local/runtime-linux/bridge.py`.
Its required model/owner/secret configuration was supplied through the owned
stdin pipe. No secret was printed or saved. The development inventory SHA-256
was `6e8df8aab730d04cc5d26199ab9f5782ee5cb73db29fd6ed27439fdd6927ea5a`;
this remains a development-only input, not release-signing metadata.

Here “new process” does **not** mean cold operating-system caches. Caches were
not cleared, other cloud work was running, and there was only one startup and
two sequential requests. Readiness announcement time excludes the Rust bundle
integrity pass; HTTP request timings exclude operator pauses between commands.
These observations show resident reuse in this bridge session, not a general
speedup, a comparison against another runtime/model, or full application latency.
They do not exercise the Rust actor's idle timer, cancellation, or shutdown.

The built native application's supported entry point was also invoked with the
same runtime/model configuration:

```sh
SAYTWIG_RUNTIME_DIR="$PWD/.local/runtime-linux" \
SAYTWIG_RUNTIME_SHA256=6e8df8aab730d04cc5d26199ab9f5782ee5cb73db29fd6ed27439fdd6927ea5a \
SAYTWIG_MODEL_STORE="$PWD/.local/development-models" \
target/release/saytwig
```

It exited with status 1 and the expected native-desktop-required message before
opening the worker, microphone, or account. This environment has no usable
native desktop session. There is no supported headless/file-transcription app
route, and none was invented for QA. Rust-actor ASR, native microphone/UI flow,
and Windows execution therefore remain outside this evidence.


### Windows x64 packaging follow-up, 2026-10-29 23:38 UTC

The existing known development payload was integrity-checked and refreshed in
place without duplicating its native runtime. The updated inventory declares
`windows-x86_64`, includes the standalone architecture guards and the complete
retained-text index, and contains **20,484 files / 664,999,327 payload bytes**.
Its development manifest SHA-256 is
`291c56eed6455f85b2f4a57e00d74925c058e2bff593f79fdd69969f59fea83c`.
Previous hashes above describe earlier observations, not the current Windows
payload. None is a signed release trust anchor.

The generated `notices/DEPENDENCIES.json` has **42 distribution records and 99
retained full-text records** (46,032 bytes of index data). Every indexed wheel
text was compared byte-for-byte with its pinned upstream archive; the otherwise
missing tokenizers license uses the existing separately pinned upstream text.
Python and model notices remain separately retained. This completes the retained
text inventory, **not** the open native-source/relinking redistribution review
called out in [NOTICES.md](../../scripts/phonon/NOTICES.md).

The architecture changes passed Linux production-only Clippy and a Windows x64
GNU `cargo check` of this library with the staged MinGW compiler. They have not
been executed on native Windows or Windows-on-ARM. The current Windows app
MSVC build and actual application operation remain separate release gates.
