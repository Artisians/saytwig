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

Stable-tag artifacts use `saytwig-candidate-v<version>-<commit>-<target>` filenames and
`stable-candidate` in `BUILD.json`. They compile with the app's `development`
identity, not `stable`. This keeps an unsigned candidate from claiming stable
status or treating its source version as an installed stable release during an
update check. Neither a tag nor a successful packaging job grants stable status.

Create future candidate tags only from `main` commits containing the
[candidate-identity correction](https://github.com/Artisians/saytwig/pull/3).
Tags on older commits run their older workflow and packaging script and can
still produce unsigned artifacts with stable identity. This correction does
not change historical commits or enforce repository-level tag restrictions.
Do not promote those older artifacts as stable.

## Packages and limits

The following recipes are implemented, not a list of published downloads.
Every native job must regenerate and pass the [notices gate](../scripts/release/STATUS.md).
The reviewed macOS declaration exceptions retain exact locked source archives and
upstream caveats. They do not establish legal clearance or native app acceptance.

| Platform | Package | Limits |
| :--- | :--- | :--- |
| Windows x64 | `*-setup.exe`, per-user Inno Setup installer | Unsigned. Native Windows x64 only. Windows ARM, including x64 emulation, and 32-bit Windows are rejected. |
| macOS Apple Silicon | `aarch64-apple-darwin.dmg` with `Saytwig.app` | macOS 13+. Ad-hoc signature only, no Developer ID signature or notarization. |
| macOS Intel | `x86_64-apple-darwin.dmg` with `Saytwig.app` | macOS 13+. Same experimental limitations. |
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

## Trusted stable distribution prerequisites

Stable publication is blocked. The current workflow has only an experimental
publisher and cannot sign, notarize, or promote a candidate. Do not manually
relabel its unsigned packages as stable or Latest. A reviewed trusted packaging
and publication path is still required after the owner supplies signing access.

### Owner-managed signing access

On 2026-10-31, metadata-only reads through `gh` found zero repository Actions
secrets, zero repository Actions variables, and zero repository environments.
The package workflow references no signing service or owner-managed identity.
This establishes no configured signing access in this repository, not the absence
of an owner's certificate or external service elsewhere. No secret values were
read, no identities were created, and no settings were changed.

Before signing can be implemented, the owner must identify and authorize these
existing trust resources through an approved secure configuration route. Do not
paste private keys, certificates with private keys, passwords, or tokens into
issues, PRs, logs, or chat.

| Platform | Required owner setup | Required artifact evidence |
| :--- | :--- | :--- |
| Windows x64 | An existing trusted Authenticode code-signing certificate and authorized private-key access or signing-service route, expected publisher identity, and approved timestamp service. | Verify the installed executable, setup executable, and uninstaller signatures and timestamp chains against that publisher on native Windows x64. A signature alone does not guarantee SmartScreen reputation. |
| macOS ARM and Intel | An existing Developer ID Application identity with authorized private-key access, its Team ID, and authorized Apple notarization access. | Verify Developer ID signatures, hardened runtime and required microphone entitlements, accepted notarization, stapled tickets, and Gatekeeper assessment for each final distributed package and installed app. Ad-hoc signing is insufficient. |
| Linux x64 | An owner-approved artifact authentication method and public verification identity, with an authorized signer or service route. | Verify authentication of the exact final archive and checksum manifest. Adjacent unsigned checksums alone cannot establish the publisher. |

Keep signing access out of PR builds. A future signing job needs a separately
approved protected release environment, least-privilege access, and no persisted
private keys on runners. The owner must configure its reviewers and branch or tag
restrictions before use. Adding an environment name to YAML alone does not
establish those protections. This document creates no such job or environment.

### Acceptance and artifact identity

Every gate below is required before a stable publication decision. Missing
evidence is a blocker, not an implicit pass.

1. Integrate the required reliability, account recovery, Console and onboarding,
   and platform speech-provisioning changes through their separate reviewed PRs.
   Record the final merged source commit, source version, Cargo.lock hash, and
   affected native acceptance evidence. Another PR's build does not validate a
   later source or different package.
2. Regenerate notices for each exact final target and production feature recipe.
   Retain complete version-specific source archives, license files, declarations,
   checksums, and the Apple SDK redistribution caveat. `READY.json` proves notice
   completeness only. The reviewed exceptions do not establish legal clearance.
   Separately provisioned Python, wheels, and models keep their own verified
   hashes and redistribution obligations.
3. Build and sign through the reviewed trusted path. Record the executable hash
   after executable signing and before constructing its build metadata. Record
   final package hashes after all signing, notarization, and stapling operations.
   Reverify package contents, notices, metadata, and authenticated manifests.
   Any changed payload requires renewed affected verification, not copied QA.
4. Operate each final installer and app on an authorized native cloud desktop.
   Record the exact package hash, source, OS, architecture, and observed results
   for the flows in [Checks and acceptance](#checks-and-acceptance). Windows must
   be native x64, never ARM emulation or 32-bit. Include least-privilege install
   and uninstall with retained user data, denied permissions, speech setup,
   offline dictation, clipboard delivery where supported, and bounded shutdown.
   Linux builds and browser prototypes cannot satisfy Windows or macOS QA.
5. Obtain the maintainer's release approval for those exact authenticated bytes.
   A future publisher must verify the version tag's source identity, all required
   package and source checksums, independent reviews, native acceptance, and
   signing evidence before creating a non-prerelease `saytwig-vX.Y.Z` release.
   Read back the public tag commit, release flags, asset inventory, and downloaded
   asset hashes and signatures from `Artisians/saytwig`. Never overwrite public
   bytes or move an existing tag on retry. Stop for inspection after partial
   publication. Existing user data and releases remain intact.

The existing experimental nightly remains separate. No stable publication,
automatic installation, platform-security bypass, or native acceptance is
established by these prerequisites. Required native desktops and signing access
are not provisioned by the package workflow.

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
