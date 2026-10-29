# Saytwig release readiness

## Current state: unreleased development build

Saytwig is newly authored, not a shipped version of the previous Handy fork.
The new standalone `Artisians/saytwig` repository has been created and verified.
Initial source publication proceeds through a draft PR and review gates. Leave
the old repository unchanged: earlier reset/rename plans are superseded. Do not
redirect update checks to the old repository or advertise an installer as ready.
No signing credential is created, no release is published, and no remote setting
is changed by any script or workflow in this tree.

## Reproducible checks

CI pins Rust **1.99.1**, Python **3.13.16**, third-party Actions to immutable commit
SHAs, and Cargo resolution with `Cargo.lock` / `--locked`. Runner OS families are
`ubuntu-24.04` and `windows-2022`. Hosted runner images and Ubuntu security packages
still receive upstream updates: this is deterministic dependency resolution,
not a claim of bit-for-bit reproducible binaries or immutable OS images.

```sh
rustup toolchain install 1.99.1 --profile minimal --component rustfmt --component clippy
# Use this exact toolchain, e.g. RUSTUP_TOOLCHAIN=1.99.1 on POSIX.
python scripts/checks/run.py all
```

`run.py` fails immediately when a gate fails. It runs formatting, workspace
Clippy (`--lib --bins -- -D warnings`) with `CARGO_BUILD_WARNINGS=deny`, and a native link of the `saytwig`
executable. Per owner policy, there are no automated test suites or test targets
in this workflow. Windows additionally enables the
native tray feature. Linux native library requirements are listed in
`.github/workflows/ci.yml`, based on the [GPUI upstream Linux build requirements](https://github.com/zed-industries/zed/blob/main/script/linux).

The CI workflow runs on PRs, main pushes, and explicit dispatch. It has read-only
repository permissions, does not persist checkout credentials, has bounded jobs,
and has **no publication step or release trigger**. No paid reviewer or signing
service is invoked by CI. GitHub Actions availability/quotas remain the owner's
repository-plan responsibility. The [GitHub security guidance](https://docs.github.com/en/actions/reference/security/secure-use)
explains the action-pinning and least-privilege choices.

Build/lint success is not evidence of working microphone capture, GPU display,
accessibility, foreground insertion, tray interaction, installation, or real-model
inference. Hands-on agent-driven app QA remains necessary. Before any release:

- [ ] Native Linux and Windows jobs pass for the exact candidate commit on GitHub.
- [ ] Review the full fresh-app diff; resolve actionable human/AI findings.
- [ ] On an explicitly authorized Windows test machine, verify launch, first-run
      setup, microphone permission/denial, hotkey repeat/cancel, tray lifecycle,
      focus preservation, clipboard fallback, and dictation into target apps.
- [ ] Operate the pinned Phonon 2.5 CPU bundle with real audio by hand; record cold
      start, peak RSS, inference latency, idle CPU and accuracy from actual app operation.
- [ ] Verify checksums and bundled upstream license texts, archive extraction
      safety, offline operation, and uninstall without deleting user data.
- [ ] Record limitations (including any account integration not available),
      supported OS/architecture, artifact size, and model download size.
- [ ] If a prerelease is approved, label it explicitly and do not call it stable.
- [ ] Confirm the standalone repository identity and exact candidate commit.

No native Windows execution or GitHub Actions success can be inferred from a
Linux cross-check. Preserve the actual command/output and commit ID as evidence.
[Clippy's official usage guide](https://doc.rust-lang.org/clippy/usage.html)
recommends `CARGO_BUILD_WARNINGS=deny` for Cargo 1.97 and newer. Use its default
lint set plus the selected workspace safety/maintenance lints; do not enable the entire restriction group or add `--all-targets`/test
commands. Rustfmt is the formatter for GPUI's Rust source too.

## Fresh-app release tags

Reserve **`saytwig-vX.Y.Z`** for freshly authored Saytwig releases, for example
`saytwig-v0.1.0`. A prerelease may use `saytwig-v0.1.0-rc.1`, but the current
stable-only checker does not offer prereleases. Do not use a bare `vX.Y.Z` tag for
new Saytwig builds. The app-specific namespace prevents a legacy or unrelated tag
from being mistaken for this app; repository ownership and a higher version alone
do not establish that identity. The old Handy repository is left unchanged.
No existing tags/releases are deleted or relabeled by this convention.

## Update-check trust boundary

`saytwig-updates` exposes a blocking `check_latest(Consent, current_version)` for a
background worker. `Consent::Disabled` is the default and does no network I/O.
A host may enable it only after explicit user opt-in. This crate has no startup
scheduler. The current app interface does not yet invoke this checker; this is
library infrastructure, not a working in-app update notification. Consent here
concerns a single check; a host scheduling later checks
must check consent again before each call and discard stale results after opt-out.
An already-started request has a 15-second timeout and is not cancellable by this
synchronous API.

The check reads only `https://api.github.com/repos/Artisians/saytwig/releases/latest`,
without authentication. It sends a generic app-version User-Agent, not transcripts,
recordings or a device identifier. GitHub still sees normal network metadata.
All redirects are refused, including HTTPS repository-rename redirects; TLS-only
transport and exact repository/tag URL matching prevent silent trust migration.
Responses are bounded to 256 KiB, including unknown-length bodies. Errors contain
fixed, sanitized text, never response bodies. Stable SemVer precedence ignores
build metadata and never offers a downgrade. Only the exact `saytwig-v` tag
namespace is eligible; unrelated or legacy tags cannot become update notices.
Drafts/prereleases are not offered. `NoCompatibleRelease` means **GitHub's latest
release is not a compatible stable Saytwig release**—not “up to date” and not a
claim about every historical release. The checker does not silently search other
repositories, delete old releases, or fall back to old assets. A 404 means no
published release or an unavailable/not-yet-created repository, not proof that the
installed build is current. See the [GitHub release API](https://docs.github.com/en/rest/releases/releases#get-the-latest-release),
[reqwest redirect policy](https://docs.rs/reqwest/0.12/reqwest/redirect/struct.Policy.html),
and [SemVer precedence API](https://docs.rs/semver/latest/semver/struct.Version.html#method.cmp_precedence).

**Automatic installation is disabled.** There is no pinned update signing public
key, verified installation path, or application asset download function. A GitHub
checksum, HTTPS, or repository ownership alone is not a signature. Do not add a
runtime "skip verification" setting. Adding installation requires a separately
reviewed established signed-update implementation, an owner-managed signing key,
a pinned trust root, rollback protection, target/architecture validation,
transactional replacement/recovery, and hands-on package verification. Do not invent
an ad-hoc signature protocol or generate credentials to unblock it.

No automated test suite is shipped, per owner policy. The implementation needs
static review and hands-on authorized app verification; its safeguards are not
claims of an actual GitHub release, verified signature, or working installation.

## Installed review applications

The owner reports CodeRabbit and Greptile OSS installed. That is not evidence of
repository access, a completed review, a free-plan entitlement, or a passing PR.
No PR has been created for this fresh tree. Keep review gates; do not enable
automatic approvals/merges or broaden application access to make a check green.

When an authorized draft PR exists, verify each app's actual access and review
status for the candidate commit. Follow the installed plan's normal review flow;
do not purchase credits or enable paid tiers. CodeRabbit's repository settings
use `.coderabbit.yaml` ([official reference](https://docs.coderabbit.ai/reference/configuration)).
Greptile recommends `.greptile/config.json`, with optional scoped `rules.md` and
`files.json`; draft review is off by default, so a draft may need an explicit
review request ([configuration](https://www.greptile.com/docs/code-review/greptile-config),
[review triggers](https://www.greptile.com/docs/code-review/greptile-json-reference)).
Configuring reviewer hints is not a substitute for checking the posted review.
This tree leaves provider access, billing, and organization policy unchanged.
