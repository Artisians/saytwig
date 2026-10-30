# Saytwig accounts and optional cloud cleanup

`saytwig-account` is newly authored Rust code for OpenAI's **public direct OSS
Sign in with ChatGPT flow**. It does not use a Codex client ID, a private ChatGPT
endpoint, a partner secret, an API key, or credentials belonging to another app.
Speech recognition stays local. Connecting an account is not consent to transmit
a transcript: the separate cleanup consent must remain disabled until enabled by
the user.

## Host integration

All operations run on a Tokio worker/runtime, never the GPUI render thread.

1. On an explicit account action, await `AccountManager::open()`. Keep one manager
   and clone its handle instead of opening the store repeatedly.
2. Use `profiles()` for display-only snapshots. `ProfileId` is the stable UI key;
   email is only a label and is never an identity/workspace key.
3. Label the connect button **Continue with ChatGPT**. For a new registration,
   `begin_sign_in(None, &cancel)` prepares a bound callback listener. For a saved
   registration, pass `Some(&profile.id)` instead.
4. Pass `PendingSignIn::authorization_url()` **directly** to the system browser
   opener. Never log it, retain it in a UI model, use a webview, or send it to
   telemetry: a returning URL may include an ID-token hint.
5. Await `complete_sign_in(pending, &cancel)`. The callback deadline is three
   minutes; dropping a pending attempt closes its listener. Cancel using the
   supplied token and await the result. Do not abort account mutation futures.
6. `activate(&id)` switches a previously connected registration. An unfinished
   registration or uncertain refresh appears disconnected; offer Continue with
   that existing ID. Keep a sign-in-again action available for saved connected
   profiles too, because server-side disconnection may precede local detection.
   If a connected profile lacks `plan_use_granted`, offer a separate explicit
   **Enable ChatGPT plan use** action. Only that action calls
   `begin_plan_use_authorization(&id, &cancel)`; it reuses the saved client and
   requests `prompt=consent` with the full scopes. Ordinary sign-in does not force
   consent, and rollout-only `force_reconsent` is not used. Successful OAuth still
   does not enable transcript sharing; the user must enable that separately.
7. `authorized_bearer(&cancel)` returns an `AuthorizedBearer` containing a
   `cleanup_service::Bearer`, its `profile_id`, and a `revoked` token. There is no
   raw-token accessor. **Select on both `revoked.cancelled()` and the job's
   cancellation token** while loading models or calling cleanup. Account changes
   invalidate existing leases. Keep the selected model associated with that
   profile and reload the model list after switching accounts.
8. Reconcile fresh `profiles()` after model loading and cleanup, including failed
   bearer acquisition: rotation may have changed connection/permission state.
   In the app worker these snapshots carry the original request ID. Cleanup
   completion is emitted first, so original text is retained before UI recovery.
9. Before sign-out, cancel UI cleanup/sign-in work. Await `sign_out(&id, &cancel)`.
   Offer this action for disconnected saved profiles too: an uncertain refresh
   may retain protected credentials that should be explicitly cleared/revoked.
   `LocalOnly` means local tokens were cleared but remote revocation was not
   confirmed; explain that the user can disconnect Saytwig in ChatGPT Settings.
   `AlreadySignedOut` means no local token was available; it is not evidence of
   remote revocation. The profile/client mapping remains available for sign-in.

`AccountError` messages contain no provider bodies, token values, or auth URLs.
A vault error disables account use, not local dictation. Never work around it by
asking for or storing a token/API key. An account without `plan_use_granted` must
not be used for cloud cleanup. Retain the original transcript on any failure.

## Authorization and identity boundary

Initial registration uses `dynamic_agent_client`, `agent_name_hint=Saytwig`, and
a persisted `urn:uuid:` host ID. Returning registration reuses the exact issued
client ID. Every attempt has independent random state, nonce, and S256 PKCE.
The listener binds `127.0.0.1` before the browser opens; `/auth/callback` and the
selected port are unchanged within that attempt. Duplicate callback parameters,
wrong state, wrong client ID, wrong Host/path, and malformed requests are rejected.

The token exchange uses the issued client, never the dynamic entrypoint. The
issued registration is retained *without an identity or credentials* before
exchange: a failed exchange can resume the same client instead of creating a new
registration. It cannot become active until its identity is validated.

Discovery issuer/authorize/token endpoints are checked against the published
OpenAI production values. JWKS and revocation URLs must remain HTTPS on
`auth.openai.com`, without redirect following. ID-token validation requires an
OpenAI JWKS RS256 signature, exact issuer/audience, expiration, issued-at,
nonce, nonempty subject, and authorized-party consistency. A returning account
must match its original verified subject. Unknown signing key IDs cause one
bounded fresh JWKS fetch. Refreshed ID tokens, when present, must match the same
identity and their nonce, if supplied, must match the original nonce.

## Protected storage and lifecycle

- **Windows:** current-user DPAPI (`CryptProtectData`/`CryptUnprotectData`), with UI
  forbidden and without machine-wide protection. Only encrypted bytes reach an
  replaced `account-v1.dpapi` file in Saytwig's local data directory. Ciphertext
  is synced before `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)` completes the
  same-directory replacement; no copy/delete fallback is enabled.
- **macOS:** native Keychain through an explicit keyring builder.
- **Linux:** Secret Service through an explicit native builder (vendored DBus
  client library). A missing or locked service is an error. It never falls back
  to plaintext, environment variables, mock storage, or a session keyring.

The native vault contains a versioned record with stable host ID, registration
mappings, active choice, and token sets. Unsupported/corrupt records are not
silently overwritten. Token-bearing types have no Debug implementation and
zeroize their owned strings; response/serialization buffers are also zeroized.
This does not promise erasure of copies inside TLS/HTTP/OS libraries.

A lifetime per-user file lock excludes concurrent Saytwig account managers in
other processes. Async state serialization prevents parallel refresh rotation.
Before a refresh request, a `refresh_pending` marker is saved with the existing
protected token set. Pending credentials are not eligible for bearer leases,
account activation, or another refresh attempt. An interrupted or ambiguous
exchange therefore cannot replay a possibly consumed rotating refresh token;
credentials remain protected for explicit reconnect/sign-out rather than being
erased merely because a request failed. No HTTP status, including 429 or 5xx,
automatically restores a pending token to reusable state. Only a validated
successful replacement clears the marker and persists tokens/scopes/expiry
together. A failed refresh or removed direct-use permission cancels previously
issued bearer leases as well; revocation happens after the attempt returns, not
at refresh entry, so it cannot cancel its own rotating request. Older records default to non-pending; older app versions reject the new
field rather than silently downgrade it. The current conservative recovery is
explicit reconnect, not automatic retry. Sign-out after an uncertain rotation
reports `LocalOnly` even if the previous-token revocation responds 200, since an
unknown replacement may exist.

Native vault calls use blocking workers with ten-second caller deadlines. A
worker that outlives its deadline keeps both the process lock and write-ordering
guard; later writes cannot overtake it. A store failure disables all leases.
Network connect/request deadlines are five/fifteen seconds with bounded response
bodies. Sign-out clears the local session first, then attempts discovered-endpoint
revocation with bounded backoff. Cancellation cannot keep local tokens connected.
The app's owned Tokio runtime uses a bounded shutdown after its command loop;
an uninterruptible OS-vault worker may outlive that deadline, but cannot keep the
UI shutdown waiting on the default unbounded runtime destructor. Returning from
shutdown is not proof that the native worker's side effect completed; the process
lock and write-order guard remain owned by that worker until it actually finishes.

## Evidence and remaining limits

Implemented against official documentation and public OIDC discovery, checked on
2026-10-29. Public discovery confirms the exact production endpoints and RS256.
No account was connected, no OAuth grant or token was created, no existing
credential was read, and no paid inference was made during implementation.

Focused Linux `cargo check`, production-only Clippy with warnings denied, and
rustfmt passed with Rust/Cargo 1.99.1. The complete account crate also passed
production-only Clippy for `x86_64-pc-windows-gnu` with warnings denied, using a
workspace-local MinGW toolchain (no OS installation). The DPAPI module separately
passed a Windows-target compile/lint check. These are compile-only checks, not
Windows runtime or live-vault validation. macOS native compilation remains
unverified in this Linux cloud environment.

The owner prohibits new automated test suites/scaffolding. Checks are compile,
format, Clippy, and static inspection only. Native Keychain/Secret Service/DPAPI
execution, real browser callback completion, refresh rotation, and real-provider
inference remain **unverified on devices/accounts**. Unsupported or unavailable
vaults and any unverified identity fail closed. Do not describe these as
live-account validation or a completed cross-platform runtime certification.

### Primary references

- [Registration and sign-in](https://developers.openai.com/siwc/token-sharing-open-source/sign-in)
- [Accounts and sessions](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions)
- [Token reference](https://developers.openai.com/siwc/token-sharing-open-source/token-reference)
- [Errors and recovery](https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery)
- [Models and inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference)
- [OpenID identity validation](https://developers.openai.com/siwc/website)
- [Public OIDC discovery](https://auth.openai.com/.well-known/openid-configuration)

Windows replacement reference: [MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw).

## Integration re-audit (2026-10-29)

Static inspection confirmed catalog results are tagged by account, the worker
checks that tag against its bearer lease before inference, and switching accounts
clears the catalog. Transcript sharing defaults off, account/consent changes cancel
old permits, inference uses only public Responses with `store=false` and
`stream=true`, and failed/cancelled cleanup retains the original transcript.

Closed source-level recovery gaps: an explicit supported plan-consent API and
worker command now exist; model/cleanup completion reconciles fresh profile state;
uncertain refresh retains blocked protected credentials instead of erasing them;
and cloud-runtime shutdown is bounded around uninterruptible vault workers.
These are source/build findings, not native OAuth or real-provider execution.
The native window/rendering issue is owned separately and is not evidence that
account or cleanup flows have been exercised.


## Plan-use controls

A successful plan-enabled sign-in schedules a one-time notice. Dismissing it saves
`chatgpt_plan_notice_seen_v1` as one boolean in local settings. The notice does not
enable transcript sharing. Its acknowledgement is per local app user, not per
ChatGPT profile.

The model picker shows whether sharing is enabled and links to
[ChatGPT usage settings](https://chatgpt.com/settings/usage). Opening that fixed
link does not initialize the credential store. The app preserves the original
transcript and disables sharing when the active request receives the documented
`subscription_sharing_usage_limit_exceeded` code. A generic HTTP429 does not
establish exhausted plan usage. Resuming requires an explicit sharing action.

These controls follow the [plan-use UI guidance](https://developers.openai.com/siwc/ui-ux-guidelines).
They passed source review, formatting, and production Clippy. No real account,
OAuth grant, or provider request was used to exercise them.
