# Quality workflow

The owner explicitly chose **no automated test files, suites, or scaffolding**.
Agents must not add or run them unless the owner changes that instruction.
Instead, use build/format/lint/static review and agent-driven hands-on operation
of the real app. Do not relabel synthetic test harnesses as manual QA.

## Formatting and linting

- Rustfmt is the canonical formatter; use its standard Rust style.
- Clippy is the canonical Rust linter, including GPUI application code. There is
  no separate GPUI linter that can establish UI correctness.
- The pinned Cargo supports `CARGO_BUILD_WARNINGS=deny`; also deny Rust/Clippy
  warnings. Target library and binary code, not test targets.
- Rust-analyzer provides editor feedback; `.vscode/settings.json` uses the same
  formatter/linter policy without enabling all-targets checks.
- Do not enable the entire Clippy restriction group. Add specific lints only
  when they support an actual project invariant.

```sh
cargo fmt --all -- --check
CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --lib --bins --locked -- -D warnings
cargo build --workspace --lib --bins --locked
```

GPUI review also checks background ownership, cancellation, no model/audio/network
blocking in render/update callbacks, no panics from normal user input, keyboard
focus, accessibility labels, contrast, and honest empty/loading/error states.
Static tools do not replace inspecting those behaviors in the running app.

## Hands-on inspection record

Record the exact app build, operating system, action, observed result, and any
limit. Exercise ordinary and interrupted flows: unavailable runtime, microphone
permission failure, start/stop/cancel/repeat, focus changes before insertion,
cleanup disabled/disconnected/revoked, history retention, and closing while work
is pending. Use non-sensitive sample speech. Never grant account permissions or
access existing credentials just to complete QA without user authorization.

The build execution sandbox has no display; its Xvfb route cannot create listening
sockets. The separately supported cloud Linux desktop shares the workspace, and
application access has now been established through its documented computer-control
API. The optimized Linux artifact identified in [BUILD_STATUS](BUILD_STATUS.md)
rendered using the existing Mesa software Vulkan driver with non-shared-memory
presentation. Navigation, empty history, and a persisted vocabulary preference were
inspected on the earlier `ead464b1` build, without opening account permissions or
the microphone. Those flows were not fully repeated on the optimized artifact.
Its recorded checks cover Start reaching microphone acquisition, recovery from
the unavailable capture device, Escape during startup, and normal shutdown.

Record newer native builds separately: a previous screenshot does not verify a
later source change. This cloud software-rendered inspection is not Windows device
execution or a performance benchmark. The user's computer is explicitly out of
scope. Concept PNGs are not native screenshots; builds and static review alone do
not establish runtime behavior. Keep any remaining gaps in BUILD_STATUS current.
