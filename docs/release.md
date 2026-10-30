# Saytwig experimental distribution

## Source and release flow

`main` is the application development branch. It also contains the landing page
in `web/`. Feature PRs target `main`. The historical `nightly` and `stable`
branches remain intact but no longer trigger CI or package publication.

```text
feature PR → formatting, Clippy, native link, package candidates, CodeRabbit
    ↓ maintainer merge
main → four native packages → commit-addressed experimental nightly prerelease
    ↓ separate native QA and maintainer approval
saytwig-vX.Y.Z tag → stable package candidate → separate stable publication
```

PRs never publish a release. A push to `main` publishes only after all four
release-profile packaging jobs pass. Every package regenerates complete notices
from that exact checkout before building. Publication uses a separate job with
repository-write permission; build jobs have read-only permission and do not
persist checkout credentials. The publication job downloads only its own run's
artifacts, verifies their checksums, and creates `saytwig-nightly-<full commit>`
as a non-draft prerelease that is not Latest. It never moves an existing tag or
overwrites an existing release. A rerun that encounters the same release requires
maintainer inspection rather than replacing public bytes.

Nightly means an experimental build after each main merge, not a scheduled daily
rebuild of unchanged source. No credentials, branch protections, reviewer access,
or billing settings are created or changed by these workflows.

Stable tags must match the Cargo source version and identify a commit in `main`.
A tag builds candidates but does not publish a stable release. Stable publication,
signing and native acceptance remain separate approved work. Existing releases
and branches are not deleted or relabeled.

## Packages and limits

The following recipes are implemented, not a list of published downloads.
Both macOS packages remain blocked on the [notices gate](../scripts/release/STATUS.md).
Because publication requires all four packages, this also blocks automatic
nightly publication. No macOS notices pass is claimed.

| Platform | Package | Limits |
| :--- | :--- | :--- |
| Windows x64 | `*-setup.exe`, per-user Inno Setup installer | Unsigned. Native Windows x64 only. Windows ARM, including x64 emulation, and 32-bit Windows are rejected. |
| macOS Apple Silicon | Planned `aarch64-apple-darwin.dmg` with `Saytwig.app` | Notices blocked. macOS 13+. Ad-hoc signature only, no Developer ID signature or notarization. |
| macOS Intel | Planned `x86_64-apple-darwin.dmg` with `Saytwig.app` | Notices blocked. macOS 13+. Same experimental limitations. |
| Linux x64 | `x86_64-unknown-linux-gnu.tar.gz` | Built on Ubuntu 24.04. Requires compatible graphics/audio system libraries. Not a self-contained AppImage or distro installer. |

Windows setup installs under the current user's local application directory,
adds a Start menu shortcut, and registers an uninstaller. It does not require
administrator rights, start at login, download speech assets, or delete user
config, history, recordings, or managed models. Windows SmartScreen may warn
because the installer is unsigned. Inno Setup comes from the hosted runner
image and must support the `x64os` architecture restriction.

macOS users copy `Saytwig.app` to Applications from the DMG. Gatekeeper may block
these builds because they lack notarization. No workflow disables platform
security or creates signing credentials. The bundle declares microphone usage,
but permission dialogs and app launch still need hands-on QA. License evidence
is inside `Contents/Resources/licenses`. `BUILD.json` is at the DMG root, outside
the signed bundle, so its executable hash describes the final ad-hoc-signed binary
without creating a circular signature dependency.

Successful packages must contain full target-specific license evidence and build
identity. Keep the notices with the application. Adjacent SHA-256 checksums detect byte
changes; they are not signatures. Rust notices do not cover separately installed
Python/PyTorch, model weights, or redistributed system libraries. See
[the distribution notices contract](../scripts/release/README.md) and
[speech licensing](../scripts/phonon/NOTICES.md).

**The speech runtime and model are not bundled.** Managed first-run speech setup
currently works only on native Windows x64. macOS and Linux need separately
provisioned, verified speech assets and have no automatic paste implementation.
Available packages are not a claim of feature parity or ordinary-user readiness.
The Windows VC++ prerequisite, interrupted setup recovery, runtime redistribution
obligations, and native microphone-to-transcript QA remain release work.

## Checks and acceptance

The workflows pin Rust 1.99.1, Python 3.13.16, Cargo.lock, and third-party Actions
to commit hashes. Hosted OS images and their system tools still change. This
is locked dependency resolution, not bit-for-bit reproducibility.

```sh
python scripts/checks/run.py all
```

This runs rustfmt, production-only workspace Clippy with warnings denied, and
native app linking. Packaging repeats these checks in the release profile and
validates the executable architecture. macOS packaging verifies its ad-hoc
signature and DMG integrity. No automated test suites, test targets, installer
smoke tests, or model downloads run in CI, per the owner's policy. PR caches
are restored but never saved into the trusted-branch cache namespace.

Before calling a platform ready for normal users, an agent must operate the
real native app and installer on an authorized cloud desktop. Verify launch,
install/uninstall while retaining user data, permission allowance and denial,
explicit speech setup and cancellation, first transcript, clipboard delivery,
shortcut/tray lifecycle where implemented, offline behavior, and bounded process
shutdown. Inspect the actual results, not only whether input was dispatched.
Record the exact source, OS, architecture and artifact hash. Linux compilation
is not Windows or macOS execution evidence. No native Windows or macOS desktop
QA is established by these workflows.

CodeRabbit and other installed review apps must actually review the candidate.
A skipped check, draft PR, or green compiler job is not an independent review.
PR #1 was closed without merge. No stable promotion occurred through that PR.
Automated reviews do not replace maintainer approval.

## Update checks

Update checks remain explicitly opt-in, with no startup polling. The app checks
only `Artisians/saytwig/releases/latest`, refuses redirects, bounds the response,
and accepts only non-draft, non-prerelease `saytwig-vX.Y.Z` releases. Nightlies
are not offered as stable updates. Errors retain fixed sanitized messages and
never expose provider bodies, credentials, recordings or transcripts.

Automatic installation remains disabled. There is no pinned update signing key
or verified transactional replacement path. Do not add an unsigned auto-install
path or a skip-verification setting. Signing and automatic updates need a
separate reviewed implementation and owner-managed trust root.
