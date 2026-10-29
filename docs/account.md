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
   registration appears disconnected; offer Continue with that existing ID.
7. `authorized_bearer(&cancel)` returns an `AuthorizedBearer` containing a
   `cleanup_service::Bearer`, its `profile_id`, and a `revoked` token. There is no
   raw-token accessor. **Select on both `revoked.cancelled()` and the job's
   cancellation token** while loading models or calling cleanup. Account changes
   invalidate existing leases. Keep the selected model associated with that
   profile and reload the model list after switching accounts.
8. Before sign-out, cancel UI cleanup/sign-in work. Await `sign_out(&id, &cancel)`.
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
Before a refresh request, the stored session is marked disconnected. Consequently
an interrupted or ambiguous exchange requires sign-in rather than replaying a
possibly consumed rotating refresh token. Successful replacement persists tokens,
scopes, and expiry together. This intentionally favors safety over silent recovery.

Native vault calls use blocking workers with ten-second caller deadlines. A
worker that outlives its deadline keeps both the process lock and write-ordering
guard; later writes cannot overtake it. A store failure disables all leases.
Network connect/request deadlines are five/fifteen seconds with bounded response
bodies. Sign-out clears the local session first, then attempts discovered-endpoint
revocation with bounded backoff. Cancellation cannot keep local tokens connected.

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
- [OpenID identity validation](https://developers.openai.com/siwc/website)
- [Public OIDC discovery](https://auth.openai.com/.well-known/openid-configuration)

Windows replacement reference: [MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw).
