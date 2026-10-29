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

- Read the relevant pinned BeautifulRust guidance in `.agents/skills/rust-best-practices` and `.agents/skills/rust-review`. Root policy and owner instructions override its generic test/trait recommendations; see `.agents/skills/BEAUTIFULRUST_SOURCE.md`. Selected safety lints are inherited from the workspace, not the complete restriction group.
