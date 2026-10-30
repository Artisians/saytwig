# BeautifulRust source

Upstream: https://github.com/morganlinton/BeautifulRust
Release: v1.5.0
Pinned commit: 40db61c6fee84bd121cf288755f0e2f22a50e995
Copyright (c) 2026 Morgan Linton; MIT license retained alongside this file.

The rust-best-practices and rust-review skill documents/references are copied
unchanged. Upstream evaluation fixtures and test scaffolding are intentionally
omitted under the owner’s no-suite policy.
They are development guidance, not runtime dependencies.

**Project overrides:** Root AGENTS.md and current owner instructions take
precedence. In particular, do not follow upstream instructions to create/run
automated tests, test targets, Miri/fuzz suites, or --all-targets checks. Perform
agent-driven hands-on real-app QA instead. Do not derive Debug for secret or
transcript-bearing types merely to satisfy a generic trait checklist. Preserve
sanitized errors rather than exposing provider bodies or user content. Use a
selected practical lint subset for this application, not every library-oriented
pedantic rule or the complete restriction group.
