//! Public, documented Sign in with ChatGPT for the Saytwig desktop host.
//!
//! Construction never imports another application's credentials. The app must
//! call `open` only for an explicit account action. Cloud transcript consent is
//! separate and remains off until enabled by the user. No token type implements
//! Debug; all public errors deliberately omit provider bodies and URLs.
mod callback;
mod oidc;
mod vault;

pub use callback::PendingSignIn;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
pub use tokio_util::sync::CancellationToken;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const MAX_VAULT: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum AccountError {
    #[error("The protected account store is unavailable; no plaintext fallback is allowed")]
    Vault,
    #[error("Saytwig account access is already open in another process")]
    AlreadyOpen,
    #[error("Stored account data cannot be safely read; it has not been overwritten")]
    StoredData,
    #[error("Sign-in was cancelled")]
    Cancelled,
    #[error("Sign-in timed out; start again")]
    TimedOut,
    #[error("The local sign-in callback could not start")]
    Listener,
    #[error("The sign-in callback was invalid")]
    Callback,
    #[error("The sign-in state did not match")]
    State,
    #[error("ChatGPT authorization was declined")]
    Denied,
    #[error("The callback registration did not match the selected account")]
    ClientMismatch,
    #[error("The ChatGPT identity could not be verified")]
    Identity,
    #[error("ChatGPT could not be reached")]
    Network,
    #[error("ChatGPT did not complete the account request; sign in again")]
    Provider,
    #[error("Choose a connected ChatGPT account")]
    NotConnected,
    #[error("This account has not granted ChatGPT plan use")]
    Permission,
    #[error("Sign in again to renew this account")]
    Reauthorize,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(String);
impl ProfileId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Display-only snapshot. Email is not used as an account or workspace key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub id: ProfileId,
    pub label: String,
    pub email: Option<String>,
    pub connected: bool,
    pub plan_use_granted: bool,
    pub active: bool,
}

/// Opaque credential usable only by the cleanup transport. The host MUST select
/// on `revoked` as well as its job cancellation token for model/cleanup calls.
/// Switching account or signing out cancels previous leases synchronously.
pub struct AuthorizedBearer {
    pub bearer: cleanup_service::Bearer,
    pub revoked: CancellationToken,
    pub profile_id: ProfileId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogoutOutcome {
    Revoked,
    /// Local credentials are cleared. Tell the user to disconnect Saytwig in
    /// ChatGPT Settings if they also need confirmation of remote revocation.
    LocalOnly,
    AlreadySignedOut,
}

#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
pub(crate) struct Secret(String);
impl Secret {
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
    fn valid(&self) -> bool {
        !self.0.is_empty()
            && self.0.len() <= 65_536
            && !self.0.bytes().any(|b| b.is_ascii_control())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Tokens {
    access: Secret,
    refresh: Secret,
    id: Secret,
    nonce: Secret,
    expires_at: u64,
    earliest_refresh_at: Option<u64>,
    scopes: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    id: ProfileId,
    client_id: String,
    subject: Option<String>,
    email: Option<String>,
    label: String,
    tokens: Option<Tokens>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u32,
    host_id: String,
    active: Option<ProfileId>,
    registrations: Vec<Registration>,
}
struct State {
    saved: Saved,
    failed: bool,
    attempt: Option<String>,
}
impl State {
    fn healthy(&self) -> Result<(), AccountError> {
        if self.failed {
            Err(AccountError::Vault)
        } else {
            Ok(())
        }
    }
    fn invalidate(&mut self) {
        self.attempt = None;
    }
}
struct Inner {
    state: Mutex<State>,
    vault: Arc<dyn vault::Vault>,
    lock: Arc<File>,
    owner: String,
    epoch: std::sync::Mutex<CancellationToken>,
    vault_writes: Arc<Mutex<()>>,
}
#[derive(Clone)]
pub struct AccountManager(Arc<Inner>);
impl AccountManager {
    /// Opens only Saytwig's native protected store. No OAuth or provider request.
    /// A missing/locked vault is an error, never a file/environment fallback.
    /// Holds a per-user process lock for this manager's lifetime.
    pub async fn open() -> Result<Self, AccountError> {
        let dirs = directories::ProjectDirs::from("com", "Saytwig", "Saytwig")
            .ok_or(AccountError::Vault)?;
        let path = dirs.data_local_dir().join("account");
        std::fs::create_dir_all(&path).map_err(|_| AccountError::Vault)?;
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = Arc::new(
            options
                .open(path.join("session.lock"))
                .map_err(|_| AccountError::Vault)?,
        );
        fs2::FileExt::try_lock_exclusive(lock.as_ref()).map_err(|_| AccountError::AlreadyOpen)?;
        // Native builders may talk to the vault; keep those off the UI executor.
        let held_lock = lock.clone();
        let vault = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio::task::spawn_blocking(move || {
                let _held = held_lock;
                vault::platform(&path)
            }),
        )
        .await
        .map_err(|_| AccountError::Vault)?
        .map_err(|_| AccountError::Vault)??;
        Self::load(vault, lock).await
    }
    async fn load(vault: Arc<dyn vault::Vault>, lock: Arc<File>) -> Result<Self, AccountError> {
        let bytes = vault::read(vault.clone(), lock.clone()).await?;
        let fresh = bytes.is_none();
        let saved = match bytes {
            Some(bytes) => {
                if bytes.len() > MAX_VAULT {
                    return Err(AccountError::StoredData);
                }
                let saved: Saved =
                    serde_json::from_slice(&bytes).map_err(|_| AccountError::StoredData)?;
                validate_saved(&saved)?;
                saved
            }
            None => Saved {
                version: 1,
                host_id: format!("urn:uuid:{}", uuid::Uuid::new_v4()),
                active: None,
                registrations: Vec::new(),
            },
        };
        let manager = Self(Arc::new(Inner {
            state: Mutex::new(State {
                saved,
                failed: false,
                attempt: None,
            }),
            vault,
            lock,
            owner: uuid::Uuid::new_v4().to_string(),
            epoch: std::sync::Mutex::new(CancellationToken::new()),
            vault_writes: Arc::new(Mutex::new(())),
        }));
        if fresh {
            let mut state = manager.0.state.lock().await;
            manager.persist(&mut state).await?;
        }
        Ok(manager)
    }
    async fn persist(&self, state: &mut State) -> Result<(), AccountError> {
        let bytes = match serde_json::to_vec(&state.saved) {
            Ok(bytes) => Zeroizing::new(bytes),
            Err(_) => {
                self.disable(state);
                return Err(AccountError::StoredData);
            }
        };
        if bytes.len() > MAX_VAULT {
            self.disable(state);
            return Err(AccountError::StoredData);
        }
        if vault::write(
            self.0.vault.clone(),
            self.0.lock.clone(),
            self.0.vault_writes.clone(),
            bytes,
        )
        .await
        .is_err()
        {
            self.disable(state);
            return Err(AccountError::Vault);
        }
        Ok(())
    }
    fn disable(&self, state: &mut State) {
        self.revoke_leases();
        state.failed = true;
        state.invalidate();
        for registration in &mut state.saved.registrations {
            registration.tokens = None;
        }
    }
    fn lease_token(&self) -> CancellationToken {
        self.0
            .epoch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
    fn revoke_leases(&self) {
        let mut epoch = self
            .0
            .epoch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        epoch.cancel();
        *epoch = CancellationToken::new();
    }
    pub async fn profiles(&self) -> Result<Vec<Profile>, AccountError> {
        let state = self.0.state.lock().await;
        state.healthy()?;
        Ok(state
            .saved
            .registrations
            .iter()
            .map(|registration| snapshot(registration, &state.saved.active))
            .collect())
    }
    /// Selecting an existing *connected* registration cancels old account jobs.
    pub async fn activate(&self, id: &ProfileId) -> Result<(), AccountError> {
        self.revoke_leases();
        let mut state = self.0.state.lock().await;
        self.revoke_leases();
        state.healthy()?;
        if !state
            .saved
            .registrations
            .iter()
            .any(|r| &r.id == id && r.tokens.is_some())
        {
            return Err(AccountError::NotConnected);
        }
        state.invalidate();
        state.saved.active = Some(id.clone());
        self.persist(&mut state).await
    }
    pub async fn begin_sign_in(
        &self,
        selected: Option<&ProfileId>,
        cancel: &CancellationToken,
    ) -> Result<PendingSignIn, AccountError> {
        let session_epoch = self.lease_token();
        let linked = LinkedCancellation::new(cancel, &session_epoch);
        let cancel = &linked.cancel;
        let (host, registration, attempt) = {
            let mut state = tokio::select! { biased;
                _ = cancel.cancelled() => return Err(AccountError::Cancelled),
                state = self.0.state.lock() => state,
            };
            state.healthy()?;
            if cancel.is_cancelled() {
                return Err(AccountError::Cancelled);
            }
            let registration = if let Some(id) = selected {
                let registration = state
                    .saved
                    .registrations
                    .iter()
                    .find(|r| &r.id == id)
                    .ok_or(AccountError::NotConnected)?;
                Some((
                    id.clone(),
                    registration.client_id.clone(),
                    registration.subject.clone(),
                    registration.tokens.as_ref().map(|t| t.id.clone()),
                ))
            } else {
                None
            };
            let attempt = uuid::Uuid::new_v4().to_string();
            state.attempt = Some(attempt.clone());
            (state.saved.host_id.clone(), registration, attempt)
        };
        let provider = oidc::Provider::discover(cancel).await?;
        let mut pending =
            PendingSignIn::new(&host, registration, provider, self.0.owner.clone()).await?;
        pending.owner = format!("{}:{attempt}", self.0.owner);
        pending.session_epoch = session_epoch;
        Ok(pending)
    }
    /// Consumes an attempt once. Invalid or cancelled attempts never replace the
    /// active account. No callback scope or unsigned JWT claim is trusted.
    pub async fn complete_sign_in(
        &self,
        mut pending: PendingSignIn,
        cancel: &CancellationToken,
    ) -> Result<Profile, AccountError> {
        let linked = LinkedCancellation::new(cancel, &pending.session_epoch);
        let cancel = &linked.cancel;
        let expected = {
            let state = tokio::select! { biased;
                _ = cancel.cancelled() => return Err(AccountError::Cancelled),
                state = self.0.state.lock() => state,
            };
            state.healthy()?;
            format!(
                "{}:{}",
                self.0.owner,
                state.attempt.as_deref().ok_or(AccountError::Cancelled)?
            )
        };
        if pending.owner != expected {
            return Err(AccountError::Cancelled);
        }
        let callback = pending.callback(cancel).await?;
        if pending.selected.is_none() {
            // An issued client is registration metadata, NOT an authenticated
            // account. Retain it before exchange so invalid_grant, cancellation,
            // or network loss never causes another dynamic registration.
            let mut state = tokio::select! { biased;
                _ = cancel.cancelled() => return Err(AccountError::Cancelled),
                state = self.0.state.lock() => state,
            };
            state.healthy()?;
            if cancel.is_cancelled()
                || pending.session_epoch.is_cancelled()
                || state
                    .attempt
                    .as_ref()
                    .map(|attempt| format!("{}:{attempt}", self.0.owner))
                    != Some(expected.clone())
            {
                return Err(AccountError::Cancelled);
            }
            if state.saved.registrations.len() >= 100
                || state
                    .saved
                    .registrations
                    .iter()
                    .any(|r| r.client_id == callback.client_id)
            {
                return Err(AccountError::ClientMismatch);
            }
            let id = ProfileId(uuid::Uuid::new_v4().to_string());
            let label = format!("ChatGPT account {}", state.saved.registrations.len() + 1);
            state.saved.registrations.push(Registration {
                id: id.clone(),
                client_id: callback.client_id.clone(),
                subject: None,
                email: None,
                label,
                tokens: None,
            });
            self.persist(&mut state).await?;
            pending.selected = Some(id);
            pending.client_id = Some(callback.client_id.clone());
        }
        let response = pending
            .provider
            .token(
                &[
                    ("grant_type", "authorization_code"),
                    ("client_id", &callback.client_id),
                    ("code", callback.code.as_str()),
                    ("code_verifier", pending.verifier.as_str()),
                    ("redirect_uri", &pending.redirect_uri),
                    ("resource", oidc::RESOURCE),
                ],
                cancel,
            )
            .await?;
        let now = now()?;
        let id_token = response.id_token.as_ref().ok_or(AccountError::Identity)?;
        let claims = pending
            .provider
            .verify(
                id_token,
                oidc::ExpectedIdentity {
                    client_id: &callback.client_id,
                    nonce: Some(pending.nonce.as_str()),
                    subject: pending.expected_subject.as_deref(),
                    now,
                },
                cancel,
            )
            .await?;
        let email = claims.email;
        let tokens = tokens_from_response(response, None, pending.nonce.clone(), now)?;
        let mut state = tokio::select! { biased;
            _ = cancel.cancelled() => return Err(AccountError::Cancelled),
            state = self.0.state.lock() => state,
        };
        state.healthy()?;
        if pending.session_epoch.is_cancelled() || cancel.is_cancelled() {
            return Err(AccountError::Cancelled);
        }
        if state
            .attempt
            .as_ref()
            .map(|attempt| format!("{}:{attempt}", self.0.owner))
            != Some(expected)
        {
            return Err(AccountError::Cancelled);
        }
        let id = pending.selected.ok_or(AccountError::StoredData)?;
        let registration = state
            .saved
            .registrations
            .iter_mut()
            .find(|registration| registration.id == id)
            .ok_or(AccountError::NotConnected)?;
        if registration.client_id != callback.client_id
            || registration
                .subject
                .as_ref()
                .is_some_and(|subject| subject != &claims.sub)
        {
            return Err(AccountError::Identity);
        }
        registration.subject = Some(claims.sub);
        registration.tokens = Some(tokens);
        registration.email = email;
        self.revoke_leases();
        state.invalidate();
        state.saved.active = Some(id.clone());
        self.persist(&mut state).await?;
        let registration = state
            .saved
            .registrations
            .iter()
            .find(|r| r.id == id)
            .ok_or(AccountError::StoredData)?;
        Ok(snapshot(registration, &state.saved.active))
    }
    /// Serializes refresh across this process and, via the lifetime file lock,
    /// all other Saytwig processes. Never returns a raw access-token string.
    pub async fn authorized_bearer(
        &self,
        cancel: &CancellationToken,
    ) -> Result<AuthorizedBearer, AccountError> {
        let session_epoch = self.lease_token();
        let linked = LinkedCancellation::new(cancel, &session_epoch);
        let cancel = &linked.cancel;
        let mut state = tokio::select! { biased;
            _ = cancel.cancelled() => return Err(AccountError::Cancelled),
            state = self.0.state.lock() => state,
        };
        state.healthy()?;
        let id = state
            .saved
            .active
            .clone()
            .ok_or(AccountError::NotConnected)?;
        let index = state
            .saved
            .registrations
            .iter()
            .position(|r| r.id == id)
            .ok_or(AccountError::StoredData)?;
        let tokens = state.saved.registrations[index]
            .tokens
            .as_ref()
            .ok_or(AccountError::Reauthorize)?;
        if !tokens
            .scopes
            .iter()
            .any(|scope| scope == oidc::DIRECT_SCOPE)
        {
            return Err(AccountError::Permission);
        }
        let now = now()?;
        if tokens.expires_at <= now.saturating_add(60) {
            if tokens
                .earliest_refresh_at
                .is_some_and(|earliest| earliest > now)
            {
                return Err(AccountError::Reauthorize);
            }
            let provider = oidc::Provider::discover(cancel).await?;
            self.refresh(&mut state, index, &provider, cancel).await?;
        }
        if cancel.is_cancelled() {
            return Err(AccountError::Cancelled);
        }
        let tokens = state.saved.registrations[index]
            .tokens
            .as_ref()
            .ok_or(AccountError::Reauthorize)?;
        if !tokens
            .scopes
            .iter()
            .any(|scope| scope == oidc::DIRECT_SCOPE)
        {
            return Err(AccountError::Permission);
        }
        let bearer = cleanup_service::Bearer::new(tokens.access.as_str().to_owned())
            .map_err(|_| AccountError::Identity)?;
        if session_epoch.is_cancelled() {
            return Err(AccountError::Cancelled);
        }
        let revoked = session_epoch;
        Ok(AuthorizedBearer {
            bearer,
            revoked,
            profile_id: id,
        })
    }
    async fn refresh(
        &self,
        state: &mut State,
        index: usize,
        provider: &oidc::Provider,
        cancel: &CancellationToken,
    ) -> Result<(), AccountError> {
        let registration = &mut state.saved.registrations[index];
        let client = registration.client_id.clone();
        let subject = registration.subject.clone().ok_or(AccountError::Identity)?;
        let old = registration
            .tokens
            .take()
            .ok_or(AccountError::Reauthorize)?;
        // Persist a disconnected intent BEFORE sending the rotating token.
        // Crash/cancel/ambiguous response means reauthorization, not unsafe retry
        // of a possibly consumed refresh token after restart.
        self.persist(state).await?;
        let response = provider
            .token(
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", &client),
                    ("refresh_token", old.refresh.as_str()),
                    ("resource", oidc::RESOURCE),
                ],
                cancel,
            )
            .await?;
        let now = now()?;
        if let Some(id_token) = response.id_token.as_ref() {
            let claims = provider
                .verify(
                    id_token,
                    oidc::ExpectedIdentity {
                        client_id: &client,
                        nonce: None,
                        subject: Some(&subject),
                        now,
                    },
                    cancel,
                )
                .await?;
            if claims
                .nonce
                .as_ref()
                .is_some_and(|nonce| !oidc::secure_eq(nonce, old.nonce.as_str()))
            {
                return Err(AccountError::Identity);
            }
        }
        let tokens = tokens_from_response(response, Some(old.id), old.nonce, now)?;
        state.saved.registrations[index].tokens = Some(tokens);
        self.persist(state).await
    }
    /// Clears tokens locally even if cancellation/network prevents revocation.
    /// Account/client mapping and host ID are retained for returning sign-in.
    pub async fn sign_out(
        &self,
        id: &ProfileId,
        cancel: &CancellationToken,
    ) -> Result<LogoutOutcome, AccountError> {
        self.revoke_leases();
        let mut state = self.0.state.lock().await;
        self.revoke_leases();
        state.healthy()?;
        let index = state
            .saved
            .registrations
            .iter()
            .position(|r| &r.id == id)
            .ok_or(AccountError::NotConnected)?;
        let client = state.saved.registrations[index].client_id.clone();
        let old = state.saved.registrations[index].tokens.take();
        state.attempt = None;
        if state.saved.active.as_ref() == Some(id) {
            state.saved.active = None;
        }
        self.persist(&mut state).await?;
        drop(state);
        let Some(tokens) = old else {
            return Ok(LogoutOutcome::AlreadySignedOut);
        };
        let provider = match oidc::Provider::discover(cancel).await {
            Ok(provider) => provider,
            Err(_) => return Ok(LogoutOutcome::LocalOnly),
        };
        Ok(if provider.revoke(&client, &tokens.refresh, cancel).await {
            LogoutOutcome::Revoked
        } else {
            LogoutOutcome::LocalOnly
        })
    }
}
fn tokens_from_response(
    response: oidc::TokenResponse,
    retained_id: Option<Secret>,
    nonce: Secret,
    now: u64,
) -> Result<Tokens, AccountError> {
    if !response.token_type.eq_ignore_ascii_case("bearer")
        || !response.access_token.valid()
        || !response.refresh_token.valid()
        || response.expires_in == 0
        || response.expires_in > 86_400
    {
        return Err(AccountError::Identity);
    }
    let id = response
        .id_token
        .or(retained_id)
        .ok_or(AccountError::Identity)?;
    if !id.valid() {
        return Err(AccountError::Identity);
    }
    let scopes: Vec<_> = response
        .scope
        .split_ascii_whitespace()
        .map(str::to_owned)
        .collect();
    Ok(Tokens {
        access: response.access_token,
        refresh: response.refresh_token,
        id,
        nonce,
        expires_at: now
            .checked_add(response.expires_in)
            .ok_or(AccountError::Identity)?,
        earliest_refresh_at: response.earliest_refresh_at,
        scopes,
    })
}
fn snapshot(registration: &Registration, active: &Option<ProfileId>) -> Profile {
    Profile {
        id: registration.id.clone(),
        label: registration.label.clone(),
        email: registration.email.clone(),
        connected: registration.tokens.is_some(),
        plan_use_granted: registration
            .tokens
            .as_ref()
            .is_some_and(|t| t.scopes.iter().any(|s| s == oidc::DIRECT_SCOPE)),
        active: active.as_ref() == Some(&registration.id),
    }
}
fn now() -> Result<u64, AccountError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| AccountError::Identity)
}
fn validate_saved(saved: &Saved) -> Result<(), AccountError> {
    if saved.version != 1
        || !saved.host_id.starts_with("urn:uuid:")
        || saved
            .host_id
            .strip_prefix("urn:uuid:")
            .is_none_or(|value| uuid::Uuid::parse_str(value).is_err())
        || saved.registrations.len() > 100
    {
        return Err(AccountError::StoredData);
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut clients = std::collections::BTreeSet::new();
    for registration in &saved.registrations {
        if !ids.insert(&registration.id)
            || !clients.insert(&registration.client_id)
            || !callback::valid_client_id(&registration.client_id)
            || registration
                .subject
                .as_ref()
                .is_some_and(|subject| subject.trim().is_empty())
            || (registration.subject.is_none() && registration.tokens.is_some())
            || registration.label.trim().is_empty()
            || uuid::Uuid::parse_str(registration.id.as_str()).is_err()
        {
            return Err(AccountError::StoredData);
        }
        if registration.tokens.as_ref().is_some_and(|t| {
            !t.access.valid() || !t.refresh.valid() || !t.id.valid() || !t.nonce.valid()
        }) {
            return Err(AccountError::StoredData);
        }
    }
    if saved.active.as_ref().is_some_and(|id| !ids.contains(id)) {
        return Err(AccountError::StoredData);
    }
    Ok(())
}

// Forward both cancellation sources without dropping an in-flight native-vault
// commit. The tiny task owns no credentials and is always aborted on drop.
struct LinkedCancellation {
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}
impl LinkedCancellation {
    fn new(caller: &CancellationToken, epoch: &CancellationToken) -> Self {
        let cancel = caller.child_token();
        if epoch.is_cancelled() {
            cancel.cancel();
        }
        let notify = cancel.clone();
        let epoch = epoch.clone();
        let task = tokio::spawn(async move {
            tokio::select! {
                _ = notify.cancelled() => {},
                _ = epoch.cancelled() => notify.cancel(),
            }
        });
        Self { cancel, task }
    }
}
impl Drop for LinkedCancellation {
    fn drop(&mut self) {
        self.task.abort();
    }
}
