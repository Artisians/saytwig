# Engineering rules

- This is a newly authored Rust + GPUI application. Do not copy implementation from the previous Handy fork or describe derivative code as clean-room original.
- Keep speech recognition local. Network cleanup is separate, explicitly opt-in, and must retain the original transcript on error. Never log recordings, transcripts, credentials, or provider response bodies.
- Own child processes; bind only loopback; verify process identity before sending audio. Verify immutable model/runtime hashes before use.
- Latest owner decision: create a new standalone Artisians/saytwig repository. Leave Artisians/handy-phonon2 unchanged; earlier reset/rename instructions are superseded. The parent coordinates authorized creation/publication and verifies remote readback. Do not reset, rename, force-push, broaden access, or create signing credentials. App name is Saytwig.
- User data deletion is never implicit. Keep config/history parsing versioned and non-destructive.
- OWNER REQUIREMENT: Do not create, add, or run automated tests, test suites, test targets, test scaffolding, or test workflow steps unless the owner explicitly asks to change this policy. This applies to all delegated agents. Do not ship newly generated test scaffolding.
- The owner explicitly wants agent-driven hands-on QA of the real application: exercise actual flows, inspect results, and fix failures. This is different from creating automated test files/suites. Use supported cloud application/GUI routes only; do not bypass sandbox restrictions or use T3/the user’s computer.
- Allowed quality checks are builds, formatting, linting, static review, and agent-driven real-app/visual inspection. Use `cargo fmt --all -- --check` and, with the pinned Cargo >= 1.97, `CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --lib --bins -- -D warnings`. Do not add `--all-targets` or test targets. Use selected useful lints, not the entire Clippy restriction group. Rust-analyzer is the recommended editor integration.
- GPUI work must keep model/audio/network work off the UI thread; avoid blocking locks and panics in render/update paths. Preserve keyboard navigation, focus, readable contrast, and truthful loading/error/permission states.
- Distinguish build/lint/static inspection from native-device execution and actual native visual evidence. Never describe unrun behavior as verified.
- Current execution is our cloud computer only, using Astra. No T3 or user-desktop execution.
- Windows support is x86_64 only. Reject Windows ARM and 32-bit targets, including x64 emulation on Windows ARM where native-machine checks apply. Do not remove macOS/Linux ARM paths by inference. Keep CI, packages, update assets and support claims consistent.

- Read the relevant pinned BeautifulRust guidance in `.agents/skills/rust-best-practices` and `.agents/skills/rust-review`. Root policy and owner instructions override its generic test/trait recommendations; see `.agents/skills/BEAUTIFULRUST_SOURCE.md`. Selected safety lints are inherited from the workspace, not the complete restriction group.

## Performance and resource discipline

- Performance and memory are product requirements, not optional cleanup. Before changing a hot path, identify its owners, input limits, queue capacity and shutdown/cancellation behavior. Record existing gaps honestly; this policy is not evidence that every existing path is already bounded.
- Keep one owned speech process per app instance, with a documented warm/idle-unload policy. Never spawn a model per UI render or per partial result. Reap owned processes; use bounded startup/inference deadlines and nonblocking cancellation handles. A timeout is not proof an OS side effect was undone.
- Bound audio duration/sample count, text/body/file sizes, outstanding jobs and cache growth. Use backpressure, coalescing or explicit rejection for producer queues; no unlimited buffers or per-click background workers. Never silently delete history/user data to meet a memory target. Paginate/virtualize history views and report capacity errors while retaining the current transcript.
- Keep model work, file hashing, native device calls, persistence and network requests off the GPUI thread. Avoid blocking locks, polling that forces needless redraws, allocation-heavy render paths and repeated full-buffer copies. Background workers must have clear ownership and bounded shutdown behavior.
- No transcript/audio/credential/provider-body logging. Bound diagnostic retention; avoid permanent debug tracing, unbounded request histories and caches. New dependencies must justify their startup, binary-size and memory cost; disable unused features.
- Measure actual same-environment startup, first/warm inference, peak/resident RSS and executable/runtime sizes when claiming improvement. Record build hash/profile, hardware/OS, cache state, input, sample count and concurrent work. Do not invent latency/RSS budgets, convert a single observation into a general benchmark, or treat bridge-only measurements as full-app/Windows evidence.
- Coordinate resource-heavy work: one full native build/link/tool installation at a time on this shared cloud executor, with `CARGO_BUILD_JOBS=1` unless measured headroom justifies otherwise. Reuse existing caches; do not run duplicate debug/release links or create another large target tree merely to evade a build lock. Check available RAM/disk before large work; stop an owned redundant job safely rather than killing unrelated processes or deleting lock files.
- Parallel agents must own distinct files/tasks and report active heavy jobs. One agent owns GitHub publication and one owns native desktop input. Serialize shared manifest/lockfile changes and final checks; preserve review gates and the no-automated-tests policy above.

## PR communication

- Follow the actual [Matt Pocock PR skill](https://github.com/mattpocock/skills/blob/main/skills/engineering/pr/SKILL.md): concise Summary with the smallest useful diagram/tree/diff sketch, concrete Before/After Evidence, and Merge Danger describing reversibility and blast radius. Keep build, hands-on app and unverified-platform evidence distinct.
- The skill's generic test examples do not override this project's no-test-suite rule. Never fabricate a before failure or after pass, submit approval reviews without required authorization, or present a draft promotion as a stable release.

## Writing and code cleanup

- Apply `.agents/skills/technical-writing/SKILL.md` and its required `.agents/skills/unslop/SKILL.md` whenever writing or reviewing README files, documentation, PR descriptions, or commit messages. Read the files, then edit the actual copy.
- Apply `unslop` to UI and landing-page copy alongside Impeccable and the product's short-label style. Keep privacy consent, actionable errors, and necessary product limitations.
- Apply `.agents/skills/deslop/SKILL.md` to code diffs before publication. Use the actual target branch as the base; `main` currently contains the repository home, not the app. Remove redundant comments and abstractions, not trust-boundary validation, resource limits, or cancellation checks.
- Keep public README files about the product: what it does, how to get or build it, current limitations, and credits. Put agent coordination, migration bookkeeping, raw verification logs, and publication details in engineering documents, not the product introduction.
- Pinned upstream sources, hashes, and MIT licenses are recorded in `.agents/skills/WRITING_SOURCE.json`. These writing skills do not override any project policy above.
