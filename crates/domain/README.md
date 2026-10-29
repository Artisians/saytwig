# dictation-domain

New UI-independent session state and user-data storage, with no device, network
or credential discovery. No prior application's implementation or user data is
imported.

## Request ownership

`Session::start` returns a monotonic `Generation`. Carry it through every
asynchronous operation. Only the matching token in the expected phase can
advance the state. Cancellation invalidates it; late completions cannot replace
new dictation. The sequence never wraps. States are Idle, Preparing, Recording,
Transcribing, Cleaning and Error.

Cleanup is disabled by default. Consent is captured at request start and can be
revoked at any time. Failure, absent/malformed output or revocation retains the
original. The host must cancel its actual device/network handles as well as the
session token. Explicit user cancellation discards that session's unfinished
output. Session debug output never contains transcript content.

## Persistence

Callers supply each `JsonStore<T>` path explicitly. Schema version 1 settings and
history preserve unknown fields; newer or malformed files are not overwritten.
There is no implicit import, history retention/deletion or credential field.

Writes use an opaque revision check, cooperative process lock, same-directory
temporary file, file sync and atomic replacement. Unix also syncs the directory.
A stale writer must reload rather than overwrite somebody else's edits. Files
are bounded to 16 MiB and symlink data/lock files are rejected. Like any filesystem
operation, an error after atomic replacement may mean durability is uncertain;
reload before retrying. The caller controls directory security and lifecycle.

Original and cleaned history text are separate fields. A history identity cannot
be appended twice. Tests use temporary caller-provided directories only.
