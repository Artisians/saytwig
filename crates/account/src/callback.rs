use crate::{oidc, AccountError, CancellationToken, ProfileId, Secret};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use url::Url;

/// Single-use pending authorization. Deliberately neither Debug nor Clone.
/// Dropping it closes the loopback listener. Never log its authorization URL.
pub struct PendingSignIn {
    pub(crate) listener: TcpListener,
    pub(crate) state: Secret,
    pub(crate) nonce: Secret,
    pub(crate) verifier: Secret,
    pub(crate) redirect_uri: String,
    pub(crate) client_id: Option<String>,
    pub(crate) selected: Option<ProfileId>,
    pub(crate) expected_subject: Option<String>,
    pub(crate) provider: oidc::Provider,
    pub(crate) started: Instant,
    url: Secret,
    pub(crate) owner: String,
    pub(crate) session_epoch: CancellationToken,
}
impl PendingSignIn {
    /// Pass directly to an OS browser opener; not a webview, log, or telemetry.
    pub fn authorization_url(&self) -> &str {
        self.url.as_str()
    }

    pub(crate) fn request_plan_consent(&mut self) -> Result<(), AccountError> {
        let mut url = Url::parse(self.url.as_str()).map_err(|_| AccountError::Provider)?;
        // This method is only reached from the explicit enable-plan host API.
        // Ordinary returning sign-in must not repeatedly force consent.
        url.query_pairs_mut().append_pair("prompt", "consent");
        self.url = Secret::new(url.into());
        Ok(())
    }

    pub(crate) async fn new(
        host: &str,
        registration: Option<(ProfileId, String, Option<String>, Option<Secret>)>,
        provider: oidc::Provider,
        owner: String,
    ) -> Result<Self, AccountError> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| AccountError::Listener)?;
        let redirect_uri = format!(
            "http://127.0.0.1:{}/auth/callback",
            listener
                .local_addr()
                .map_err(|_| AccountError::Listener)?
                .port()
        );
        let state = random_secret();
        let nonce = random_secret();
        let verifier = random_secret();
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_str().as_bytes()));
        let (selected, client_id, expected_subject, hint) = match registration {
            Some((id, client, subject, hint)) => (Some(id), Some(client), subject, hint),
            None => (None, None, None, None),
        };
        let mut url = Url::parse(oidc::AUTHORIZE).map_err(|_| AccountError::Provider)?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair(
                    "client_id",
                    client_id.as_deref().unwrap_or("dynamic_agent_client"),
                )
                .append_pair("ext_agent_host_id", host)
                .append_pair("response_type", "code")
                .append_pair("redirect_uri", &redirect_uri)
                .append_pair(
                    "scope",
                    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct",
                )
                .append_pair("resource", oidc::RESOURCE)
                .append_pair("state", state.as_str())
                .append_pair("nonce", nonce.as_str())
                .append_pair("code_challenge_method", "S256")
                .append_pair("code_challenge", &challenge);
            if client_id.is_none() {
                query.append_pair("agent_name_hint", "Saytwig");
            }
            if let Some(hint) = hint {
                query.append_pair("id_token_hint", hint.as_str());
            }
        }
        Ok(Self {
            listener,
            state,
            nonce,
            verifier,
            redirect_uri,
            client_id,
            selected,
            expected_subject,
            provider,
            started: Instant::now(),
            url: Secret::new(url.into()),
            owner,
            session_epoch: CancellationToken::new(),
        })
    }
    pub(crate) async fn callback(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Callback, AccountError> {
        let remaining = Duration::from_secs(180)
            .checked_sub(self.started.elapsed())
            .ok_or(AccountError::TimedOut)?;
        tokio::select! { biased;
            _ = cancel.cancelled() => Err(AccountError::Cancelled),
            result = tokio::time::timeout(remaining, self.listen()) => result.map_err(|_| AccountError::TimedOut)?,
        }
    }
    async fn listen(&self) -> Result<Callback, AccountError> {
        // Ignore unrelated requests (including browser favicon) without consuming
        // state; one slow local socket cannot monopolize the entire attempt.
        loop {
            let (mut socket, peer) = self
                .listener
                .accept()
                .await
                .map_err(|_| AccountError::Listener)?;
            if !peer.ip().is_loopback() {
                continue;
            }
            let request =
                tokio::time::timeout(Duration::from_secs(2), read_request(&mut socket)).await;
            let result = match request {
                Ok(Ok(target)) => self.parse_callback(&target),
                _ => Err(AccountError::Callback),
            };
            let status = if result.is_ok() {
                "200 OK"
            } else {
                "400 Bad Request"
            };
            let body = if result.is_ok() {
                "Return to Saytwig to finish connecting. You can close this tab."
            } else {
                "Sign-in callback was not accepted. Return to Saytwig."
            };
            let reply = format!("HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nPragma: no-cache\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len());
            let _ =
                tokio::time::timeout(Duration::from_secs(1), socket.write_all(reply.as_bytes()))
                    .await;
            match result {
                Err(AccountError::Callback | AccountError::State) => continue,
                other => return other,
            }
        }
    }
    pub(crate) fn parse_callback(&self, target: &str) -> Result<Callback, AccountError> {
        if !target.starts_with("/auth/callback?") || target.len() > 16_384 {
            return Err(AccountError::Callback);
        }
        let url =
            Url::parse(&format!("http://127.0.0.1{target}")).map_err(|_| AccountError::Callback)?;
        if url.path() != "/auth/callback" || url.fragment().is_some() {
            return Err(AccountError::Callback);
        }
        let mut values = BTreeMap::new();
        for (key, value) in url.query_pairs() {
            if values
                .insert(key.into_owned(), value.into_owned())
                .is_some()
            {
                return Err(AccountError::Callback);
            }
        }
        if !oidc::secure_eq(
            values.get("state").map(String::as_str).unwrap_or(""),
            self.state.as_str(),
        ) {
            return Err(AccountError::State);
        }
        if values.contains_key("error") {
            return Err(AccountError::Denied);
        }
        let client = match (&self.client_id, values.get("client_id")) {
            (Some(expected), Some(actual)) if expected != actual => {
                return Err(AccountError::ClientMismatch)
            }
            (Some(expected), _) => expected.clone(),
            (None, Some(actual)) if valid_client_id(actual) => actual.clone(),
            _ => return Err(AccountError::ClientMismatch),
        };
        let code = values
            .remove("code")
            .filter(|code| !code.trim().is_empty())
            .ok_or(AccountError::Callback)?;
        Ok(Callback {
            code: Secret::new(code),
            client_id: client,
        })
    }
}
pub(crate) struct Callback {
    pub(crate) code: Secret,
    pub(crate) client_id: String,
}
pub(crate) fn valid_client_id(id: &str) -> bool {
    !id.is_empty()
        && id != "dynamic_agent_client"
        && id.len() <= 256
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}
fn random_secret() -> Secret {
    let mut bytes = [0; 32];
    OsRng.fill_bytes(&mut bytes);
    Secret::new(URL_SAFE_NO_PAD.encode(bytes))
}
async fn read_request(socket: &mut TcpStream) -> Result<String, AccountError> {
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    let mut chunk = [0; 1024];
    loop {
        let count = socket
            .read(&mut chunk)
            .await
            .map_err(|_| AccountError::Callback)?;
        if count == 0 || bytes.len() + count > 20_480 {
            return Err(AccountError::Callback);
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.windows(4).any(|value| value == b"\r\n\r\n") {
            break;
        }
    }
    let request = std::str::from_utf8(&bytes).map_err(|_| AccountError::Callback)?;
    let mut parts = request
        .lines()
        .next()
        .ok_or(AccountError::Callback)?
        .split_whitespace();
    if parts.next() != Some("GET") {
        return Err(AccountError::Callback);
    }
    let target = parts.next().ok_or(AccountError::Callback)?;
    if parts.next() != Some("HTTP/1.1") || parts.next().is_some() {
        return Err(AccountError::Callback);
    }
    // Exact Host check also rejects DNS-rebinding/absolute-URI proxy requests.
    let expected_host = socket
        .local_addr()
        .map_err(|_| AccountError::Callback)?
        .to_string();
    let hosts: Vec<_> = request
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.eq_ignore_ascii_case("host"))
        .collect();
    if hosts.len() != 1 || hosts[0].1.trim() != expected_host {
        return Err(AccountError::Callback);
    }
    Ok(target.to_owned())
}
