# Contributing to Saytwig

Saytwig is an early-stage local speech recognition app. Keep changes small and
focused on reliable recording, local recognition, deliberate text delivery, and
clear user control. Discuss new model families or additional cloud services
before adding them; they are outside the initial product scope.

## Pull requests

1. Describe the user-visible problem and the proposed behavior.
2. Reproduce the build, formatting, and lint checks in README.md. Do not add or run automated tests unless the owner explicitly changes the AGENTS.md policy.
3. State exactly which platforms/devices were exercised. Compilation, static review, concept renders, and real native/device
   inspection are different evidence.
4. Provide screenshots for interface changes and disclose unfinished controls.
5. Keep PRs draft until the implementation and evidence are ready for review.

AI-assisted contributions are welcome when the contributor understands and can
maintain the result. Disclose material AI assistance and check licenses and
provenance. Never submit credentials, private audio/transcripts, or unrelated
customer data to tooling or examples. A human maintainer must review and approve
merges; automated reviews do not substitute for that decision.

## Compatibility and privacy

- Do not import old application credentials or history automatically.
- Never silently enable transcript sharing or API billing.
- Preserve the original transcript on cleanup failure.
- Cancellation must invalidate late work, release microphone resources, and
  prevent a stale result from reaching another application.
- Version persisted data; refuse destructive migration of unknown formats.
- Keep runtime/model downloads pinned and verified and retain required notices.
- No telemetry or transcript logging by default.

Contributions are provided under the repository's MIT license unless a file
clearly states another compatible license. Do not remove attribution from reused
work. For security reports, avoid posting secrets or exploit details in public. Use
GitHub private vulnerability reporting if the repository offers it; otherwise
ask a maintainer for a private reporting route.
