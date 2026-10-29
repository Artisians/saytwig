use crate::{AccountError, CancellationToken, Secret};
use futures_util::StreamExt;
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use subtle::ConstantTimeEq;
use url::Url;
use zeroize::Zeroizing;

pub(crate) const ISSUER: &str = "https://auth.openai.com";
pub(crate) const AUTHORIZE: &str = "https://auth.openai.com/api/accounts/authorize";
pub(crate) const TOKEN: &str = "https://auth.openai.com/api/accounts/oauth/token";
pub(crate) const RESOURCE: &str = "https://api.openai.com/v1";
pub(crate) const DIRECT_SCOPE: &str = "chatgpt.tokens.use.direct";
const DISCOVERY: &str = "https://auth.openai.com/.well-known/openid-configuration";
const MAX_BODY: usize = 256 * 1024;

#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    revocation_endpoint: String,
}

pub(crate) struct Provider {
    client: reqwest::Client,
    jwks: JwkSet,
    jwks_uri: String,
    pub(crate) revocation: String,
    pub(crate) token_endpoint: String,
}
impl Provider {
    pub(crate) async fn discover(cancel: &CancellationToken) -> Result<Self, AccountError> {
        let client = reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| AccountError::Network)?;
        let discovery: Discovery = read_json(client.get(DISCOVERY), cancel).await?;
        if discovery.issuer != ISSUER
            || discovery.authorization_endpoint != AUTHORIZE
            || discovery.token_endpoint != TOKEN
        {
            return Err(AccountError::Provider);
        }
        same_origin(&discovery.jwks_uri)?;
        same_origin(&discovery.revocation_endpoint)?;
        let jwks = read_json(client.get(&discovery.jwks_uri), cancel).await?;
        Ok(Self {
            client,
            jwks,
            jwks_uri: discovery.jwks_uri,
            revocation: discovery.revocation_endpoint,
            token_endpoint: discovery.token_endpoint,
        })
    }
    pub(crate) async fn verify(
        &self,
        token: &Secret,
        expected: ExpectedIdentity<'_>,
        cancel: &CancellationToken,
    ) -> Result<Claims, AccountError> {
        let header = decode_header(token.as_str()).map_err(|_| AccountError::Identity)?;
        let kid = header.kid.ok_or(AccountError::Identity)?;
        if self
            .jwks
            .keys
            .iter()
            .any(|key| key.common.key_id.as_deref() == Some(&kid))
        {
            validate_id(token, &self.jwks, expected)
        } else {
            // One bounded refresh on rotation, still only the discovery-pinned
            // OpenAI origin; JWT-supplied key URLs are never followed.
            let fresh: JwkSet = read_json(self.client.get(&self.jwks_uri), cancel).await?;
            validate_id(token, &fresh, expected)
        }
    }
    pub(crate) async fn token(
        &self,
        fields: &[(&str, &str)],
        cancel: &CancellationToken,
    ) -> Result<TokenResponse, AccountError> {
        read_json(self.client.post(&self.token_endpoint).form(fields), cancel).await
    }
    pub(crate) async fn revoke(
        &self,
        client_id: &str,
        refresh: &Secret,
        cancel: &CancellationToken,
    ) -> bool {
        for attempt in 0..3 {
            if cancel.is_cancelled() {
                return false;
            }
            let request = self.client.post(&self.revocation).form(&[
                ("token", refresh.as_str()),
                ("token_type_hint", "refresh_token"),
                ("client_id", client_id),
            ]);
            let response = tokio::select! { biased;
                _ = cancel.cancelled() => return false,
                response = request.send() => response,
            };
            match response {
                Ok(response) if response.status() == reqwest::StatusCode::OK => return true,
                Ok(response) if !response.status().is_server_error() => return false,
                _ => {}
            }
            if attempt < 2 {
                tokio::select! { biased;
                    _ = cancel.cancelled() => return false,
                    _ = tokio::time::sleep(Duration::from_millis(250 * (1 << attempt))) => {}
                }
            }
        }
        false
    }
}
fn same_origin(value: &str) -> Result<(), AccountError> {
    let url = Url::parse(value).map_err(|_| AccountError::Provider)?;
    if url.scheme() != "https"
        || url.host_str() != Some("auth.openai.com")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(AccountError::Provider);
    }
    Ok(())
}
async fn read_json<T: serde::de::DeserializeOwned>(
    request: reqwest::RequestBuilder,
    cancel: &CancellationToken,
) -> Result<T, AccountError> {
    if cancel.is_cancelled() {
        return Err(AccountError::Cancelled);
    }
    let response = tokio::select! { biased;
        _ = cancel.cancelled() => return Err(AccountError::Cancelled),
        response = request.send() => response.map_err(|_| AccountError::Network)?,
    };
    if response.status() != reqwest::StatusCode::OK {
        return Err(AccountError::Provider);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    let mut stream = response.bytes_stream();
    loop {
        let next = tokio::select! { biased;
            _ = cancel.cancelled() => return Err(AccountError::Cancelled),
            next = stream.next() => next,
        };
        match next {
            Some(Ok(chunk)) if bytes.len().saturating_add(chunk.len()) <= MAX_BODY => {
                bytes.extend_from_slice(&chunk)
            }
            Some(Ok(_)) => return Err(AccountError::Provider),
            Some(Err(_)) => return Err(AccountError::Network),
            None => break,
        }
    }
    serde_json::from_slice(&bytes).map_err(|_| AccountError::Provider)
}

#[derive(Deserialize)]
pub(crate) struct TokenResponse {
    pub(crate) access_token: Secret,
    pub(crate) refresh_token: Secret,
    pub(crate) id_token: Option<Secret>,
    pub(crate) token_type: String,
    pub(crate) expires_in: u64,
    pub(crate) scope: String,
    #[serde(default)]
    pub(crate) earliest_refresh_at: Option<u64>,
}
#[derive(Deserialize, Serialize)]
pub(crate) struct Claims {
    pub(crate) sub: String,
    pub(crate) nonce: Option<String>,
    pub(crate) email: Option<String>,
    pub(crate) iat: u64,
    pub(crate) exp: u64,
    pub(crate) nbf: Option<u64>,
    pub(crate) azp: Option<String>,
    pub(crate) aud: serde_json::Value,
    pub(crate) iss: String,
}

pub(crate) struct ExpectedIdentity<'a> {
    pub(crate) client_id: &'a str,
    pub(crate) nonce: Option<&'a str>,
    pub(crate) subject: Option<&'a str>,
    pub(crate) now: u64,
}
fn validate_id(
    token: &Secret,
    jwks: &JwkSet,
    expected: ExpectedIdentity<'_>,
) -> Result<Claims, AccountError> {
    let ExpectedIdentity {
        client_id,
        nonce,
        subject: expected_subject,
        now,
    } = expected;
    let header = decode_header(token.as_str()).map_err(|_| AccountError::Identity)?;
    // Only asymmetric signing; never accept HS*, none, or a key URL from a JWT.
    if header.alg != Algorithm::RS256 {
        return Err(AccountError::Identity);
    }
    let kid = header.kid.ok_or(AccountError::Identity)?;
    let matches: Vec<_> = jwks
        .keys
        .iter()
        .filter(|key| key.common.key_id.as_deref() == Some(&kid))
        .collect();
    if matches.len() != 1 {
        return Err(AccountError::Identity);
    }
    let key = matches[0];
    if let Some(alg) = key.common.key_algorithm {
        if !matches!(
            (alg, header.alg),
            (jsonwebtoken::jwk::KeyAlgorithm::RS256, Algorithm::RS256)
        ) {
            return Err(AccountError::Identity);
        }
    }
    if key
        .common
        .public_key_use
        .as_ref()
        .is_some_and(|usage| *usage != jsonwebtoken::jwk::PublicKeyUse::Signature)
    {
        return Err(AccountError::Identity);
    }
    if key
        .common
        .key_operations
        .as_ref()
        .is_some_and(|operations| !operations.contains(&jsonwebtoken::jwk::KeyOperations::Verify))
    {
        return Err(AccountError::Identity);
    }
    let key = DecodingKey::from_jwk(key).map_err(|_| AccountError::Identity)?;
    let mut validation = Validation::new(header.alg);
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[client_id]);
    validation.set_required_spec_claims(&["iss", "aud", "sub", "exp", "iat"]);
    // Signature/issuer/audience checked by library. Time checked against explicit
    // clock below with a strict expiration boundary and five-second iat/nbf skew.
    validation.validate_exp = false;
    validation.validate_nbf = false;
    let claims = decode::<Claims>(token.as_str(), &key, &validation)
        .map_err(|_| AccountError::Identity)?
        .claims;
    if claims.sub.trim().is_empty()
        || claims.exp <= now
        || claims.iat > now.saturating_add(5)
        || claims.exp <= claims.iat
        || claims.nbf.is_some_and(|nbf| nbf > now.saturating_add(5))
    {
        return Err(AccountError::Identity);
    }
    if nonce.is_some_and(|expected| !secure_eq(claims.nonce.as_deref().unwrap_or(""), expected))
        || expected_subject.is_some_and(|expected| claims.sub != expected)
    {
        return Err(AccountError::Identity);
    }
    if claims.azp.as_deref().is_some_and(|azp| azp != client_id)
        || (claims.aud.as_array().is_some_and(|aud| aud.len() > 1)
            && claims.azp.as_deref() != Some(client_id))
    {
        return Err(AccountError::Identity);
    }
    Ok(claims)
}
pub(crate) fn secure_eq(left: &str, right: &str) -> bool {
    bool::from(left.as_bytes().ct_eq(right.as_bytes()))
}
