# cleanup-service

Fresh transcript-only cleanup through the public OpenAI Responses API. This
crate does not acquire credentials, read another tool's tokens, or implement
sign-in. The host must supply a token from a validated, explicitly authorized
account connection and choose a model from that account's visible catalog.

Consent is off by default. Each change invalidates previous permits; revocation
interrupts pending I/O. The output is accepted only after a successful
`response.completed` event. Partial, failed, empty, reasoning-prefixed and
code-fenced results are rejected. The domain layer retains the original text.
There is no API-key billing fallback and no transcript/provider-body logging.

The fixed public endpoint uses HTTPS, no redirects, bounded time/body sizes,
`store: false`, and streaming. `store: false` is not a promise of zero provider
retention; the account/provider's applicable terms still govern submitted text.

Automated test scaffolding is not shipped, following the owner’s explicit policy.
Build, lint, formatting, and static review do not establish end-to-end account or provider behavior.

Primary protocol references:
- https://developers.openai.com/siwc/token-sharing-open-source/sign-in
- https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference
