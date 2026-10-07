//! Public-client login to one host-configured developer backend.
//!
//! This is not an authenticated HTTP proxy. Every operation has one immutable
//! endpoint, credentials stay in Rust, redirects are refused, and callers must
//! also check the connection's persisted registration binding before token use.
use crate::oauth::{Tokens, AUTH_LIFETIME};
use oauth2::{
    basic::BasicClient, AuthType, AuthUrl, AuthorizationCode, ClientId, CsrfToken, HttpRequest,
    HttpResponse, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, RefreshToken, Scope, TokenUrl,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::Read,
    time::{Duration, Instant},
};
use url::Url;

pub const SESSION_SCOPE: &str = "app.session";
const RESPONSE_LIMIT: usize = 64 * 1024;

/// Trusted host metadata; never deserialize this from a contained app request.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendRegistration {
    pub id: String,
    pub app_id: String,
    pub client_id: String,
    pub authorization_url: String,
    pub token_url: String,
    pub me_url: String,
    pub logout_url: String,
    pub scopes: BTreeSet<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BackendIdentity {
    pub sub: String,
    pub label: String,
}

/// No Debug/Serialize: bearer material cannot accidentally form an app reply.
pub struct BackendAuthorized {
    pub identity: BackendIdentity,
    pub(crate) tokens: Tokens,
}

pub struct BackendClient {
    registration: BackendRegistration,
    binding: String,
    http: reqwest::blocking::Client,
}

pub struct BackendAttempt {
    owner: String,
    binding: String,
    authorization_url: Url,
    redirect: Url,
    state: String,
    verifier: String,
    deadline: Instant,
    consumed: bool,
}

pub struct BackendCode {
    owner: String,
    binding: String,
    code: String,
    verifier: String,
    redirect: Url,
}

#[derive(Debug)]
struct ProtocolError;
impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Backend protocol request failed")
    }
}
impl std::error::Error for ProtocolError {}

impl BackendClient {
    pub fn new(registration: BackendRegistration) -> Result<Self, String> {
        Self::validated(registration, false)
    }

    /// Deliberately absent from normal builds. Only a fresh isolated acceptance
    /// host may opt into a real local HTTP fixture; no environment override.
    #[cfg(feature = "acceptance-fixtures")]
    pub fn new_loopback_fixture(registration: BackendRegistration) -> Result<Self, String> {
        Self::validated(registration, true)
    }

    fn validated(
        mut registration: BackendRegistration,
        loopback_fixture: bool,
    ) -> Result<Self, String> {
        for (value, maximum) in [
            (&registration.id, 64),
            (&registration.app_id, 256),
            (&registration.client_id, 256),
        ] {
            if value.is_empty()
                || value.len() > maximum
                || value.chars().any(|c| c.is_whitespace() || c.is_control())
            {
                return Err("Invalid backend registration identity".into());
            }
        }
        if registration.scopes != BTreeSet::from([SESSION_SCOPE.into()]) {
            return Err("Backend login requires only app.session".into());
        }
        let endpoints = [
            &registration.authorization_url,
            &registration.token_url,
            &registration.me_url,
            &registration.logout_url,
        ];
        let mut urls = vec![];
        for endpoint in endpoints {
            let url = Url::parse(endpoint).map_err(|_| "Invalid backend endpoint")?;
            let https = url.scheme() == "https" && url.port_or_known_default() == Some(443);
            let fixture = loopback_fixture
                && url.scheme() == "http"
                && url.host_str() == Some("127.0.0.1")
                && url.port().is_some_and(|p| p != 0);
            if endpoint.len() > 2048
                || (!https && !fixture)
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || url.path() == "/"
            {
                return Err("Backend endpoints require exact HTTPS URLs without credentials, query or fragment".into());
            }
            if let Some(first) = urls.first() {
                let first: &Url = first;
                if first.origin() != url.origin() {
                    return Err("Backend endpoints must share one origin".into());
                }
            }
            if urls.iter().any(|old| old == &url) {
                return Err(
                    "Backend authorization, token, identity and logout endpoints must differ"
                        .into(),
                );
            }
            urls.push(url);
        }
        registration.authorization_url = urls[0].to_string();
        registration.token_url = urls[1].to_string();
        registration.me_url = urls[2].to_string();
        registration.logout_url = urls[3].to_string();
        let canonical =
            serde_json::to_vec(&registration).map_err(|_| "Invalid backend registration")?;
        let binding = format!("{:x}", Sha256::digest(canonical));
        let http = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .user_agent("OctoSense-Backend/1")
            .build()
            .map_err(|_| "Cannot initialize backend transport")?;
        Ok(Self {
            registration,
            binding,
            http,
        })
    }

    pub fn registration(&self) -> &BackendRegistration {
        &self.registration
    }
    pub fn binding(&self) -> &str {
        &self.binding
    }

    fn caller(&self, caller: &str) -> Result<(), String> {
        if caller != self.registration.app_id {
            return Err("Backend belongs to another app".into());
        }
        Ok(())
    }

    pub fn begin(
        &self,
        caller: &str,
        redirect: Url,
        now: Instant,
    ) -> Result<BackendAttempt, String> {
        self.caller(caller)?;
        // Desktop callback is host-owned and fixed, independent of backend URLs.
        if redirect.scheme() != "http"
            || redirect.host_str() != Some("127.0.0.1")
            || !redirect.port().is_some_and(|p| p != 0)
            || redirect.path() != "/oauth/callback"
            || !redirect.username().is_empty()
            || redirect.password().is_some()
            || redirect.query().is_some()
            || redirect.fragment().is_some()
        {
            return Err("Invalid host backend callback listener".into());
        }
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let client = BasicClient::new(ClientId::new(self.registration.client_id.clone()))
            .set_auth_uri(
                AuthUrl::new(self.registration.authorization_url.clone())
                    .map_err(|_| "Invalid backend authorization endpoint")?,
            )
            .set_redirect_uri(RedirectUrl::from_url(redirect.clone()));
        let (authorization_url, state) = client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new(SESSION_SCOPE.into()))
            .set_pkce_challenge(challenge)
            .url();
        Ok(BackendAttempt {
            owner: caller.into(),
            binding: self.binding.clone(),
            authorization_url,
            redirect,
            state: state.secret().clone(),
            verifier: verifier.secret().clone(),
            deadline: now + AUTH_LIFETIME,
            consumed: false,
        })
    }

    pub fn finish(
        &self,
        caller: &str,
        code: BackendCode,
        unix_now: u64,
    ) -> Result<BackendAuthorized, String> {
        self.caller(caller)?;
        if code.owner != caller || code.binding != self.binding {
            return Err("Backend authorization registration changed".into());
        }
        let client = BasicClient::new(ClientId::new(self.registration.client_id.clone()))
            .set_auth_type(AuthType::RequestBody)
            .set_token_uri(
                TokenUrl::new(self.registration.token_url.clone())
                    .map_err(|_| "Invalid backend token endpoint")?,
            )
            .set_redirect_uri(RedirectUrl::from_url(code.redirect));
        let response = client
            .exchange_code(AuthorizationCode::new(code.code))
            .set_pkce_verifier(PkceCodeVerifier::new(code.verifier))
            .request(&|request| self.token_request(request))
            .map_err(|_| "Backend code exchange failed; sign in again")?;
        let value = serde_json::to_value(response).map_err(|_| "Invalid backend token response")?;
        let tokens = Tokens::from_response(&value, &self.registration.scopes, unix_now)?;
        let identity = match self.me(caller, &tokens) {
            Ok(identity) => identity,
            Err(error) => {
                let _ = self.logout(caller, &tokens);
                return Err(error);
            }
        };
        Ok(BackendAuthorized { identity, tokens })
    }

    pub fn refresh(&self, caller: &str, tokens: &Tokens, unix_now: u64) -> Result<Tokens, String> {
        self.caller(caller)?;
        let refresh = tokens
            .refresh
            .as_ref()
            .ok_or("Backend session cannot refresh; sign in again")?;
        let client = BasicClient::new(ClientId::new(self.registration.client_id.clone()))
            .set_auth_type(AuthType::RequestBody)
            .set_token_uri(
                TokenUrl::new(self.registration.token_url.clone())
                    .map_err(|_| "Invalid backend token endpoint")?,
            );
        let response = client
            .exchange_refresh_token(&RefreshToken::new(refresh.clone()))
            .request(&|request| self.token_request(request))
            .map_err(|_| "Backend refresh failed; sign in again")?;
        let value = serde_json::to_value(response).map_err(|_| "Invalid backend token response")?;
        let mut new_tokens = Tokens::from_response(&value, &self.registration.scopes, unix_now)?;
        if new_tokens.refresh.is_none() {
            new_tokens.refresh = tokens.refresh.clone();
        }
        Ok(new_tokens)
    }

    pub fn me(&self, caller: &str, tokens: &Tokens) -> Result<BackendIdentity, String> {
        self.caller(caller)?;
        let response = self
            .http
            .get(&self.registration.me_url)
            .bearer_auth(&tokens.access)
            .send()
            .map_err(|_| "Backend identity connection failed")?;
        let value = response_json(response)?;
        let identity: BackendIdentity =
            serde_json::from_value(value).map_err(|_| "Invalid backend identity response")?;
        if identity.sub.is_empty()
            || identity.sub.len() > 256
            || identity.label.is_empty()
            || identity.label.len() > 256
            || identity.sub.chars().any(char::is_control)
            || identity.label.chars().any(char::is_control)
        {
            return Err("Invalid backend identity response".into());
        }
        Ok(identity)
    }

    /// The host must revoke its local handle before calling this remote endpoint.
    pub fn logout(&self, caller: &str, tokens: &Tokens) -> Result<(), String> {
        self.caller(caller)?;
        let response = self
            .http
            .post(&self.registration.logout_url)
            .bearer_auth(&tokens.access)
            .json(&serde_json::json!({}))
            .send()
            .map_err(|_| "Backend logout connection failed")?;
        let value = response_json(response)?;
        if value.get("logged_out").and_then(serde_json::Value::as_bool) != Some(true) {
            return Err("Backend did not confirm remote logout".into());
        }
        Ok(())
    }

    fn token_request(&self, request: HttpRequest) -> Result<HttpResponse, ProtocolError> {
        if request.method() != oauth2::http::Method::POST
            || request.uri().to_string() != self.registration.token_url
            || request
                .headers()
                .contains_key(oauth2::http::header::AUTHORIZATION)
            || request.body().len() > RESPONSE_LIMIT
        {
            return Err(ProtocolError);
        }
        let response = self
            .http
            .post(&self.registration.token_url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(request.into_body())
            .send()
            .map_err(|_| ProtocolError)?;
        let value = response_json(response).map_err(|_| ProtocolError)?;
        oauth2::http::Response::builder()
            .status(200)
            .header(oauth2::http::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(&value).map_err(|_| ProtocolError)?)
            .map_err(|_| ProtocolError)
    }
}

fn response_json(response: reqwest::blocking::Response) -> Result<serde_json::Value, String> {
    if !response.status().is_success() {
        return Err("Backend request rejected; sign in again if needed".into());
    }
    if response
        .content_length()
        .is_some_and(|length| length > RESPONSE_LIMIT as u64)
    {
        return Err("Backend response exceeds 64 KiB".into());
    }
    let mut bytes = vec![];
    response
        .take(RESPONSE_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read backend response")?;
    if bytes.len() > RESPONSE_LIMIT {
        return Err("Backend response exceeds 64 KiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Backend returned invalid JSON".into())
}

impl BackendAttempt {
    pub fn authorization_url(&self) -> &Url {
        &self.authorization_url
    }
    pub fn cancel(&mut self) {
        self.consumed = true;
    }
    pub fn is_finished(&self) -> bool {
        self.consumed
    }
    pub fn consume_callback(
        &mut self,
        caller: &str,
        callback: &str,
        now: Instant,
    ) -> Result<BackendCode, String> {
        if self.consumed || now >= self.deadline || caller != self.owner {
            return Err("Backend authorization expired or belongs to another app".into());
        }
        if callback.len() > 32_768 {
            return Err("Backend callback exceeds limit".into());
        }
        let callback = Url::parse(callback).map_err(|_| "Invalid backend callback")?;
        if callback.origin() != self.redirect.origin()
            || callback.path() != self.redirect.path()
            || callback.fragment().is_some()
            || !callback.username().is_empty()
            || callback.password().is_some()
        {
            return Err("Backend callback destination mismatch".into());
        }
        let pairs: Vec<_> = callback.query_pairs().collect();
        let states: Vec<_> = pairs.iter().filter(|(k, _)| k == "state").collect();
        if states.len() != 1 || states[0].1 != self.state {
            return Err("Backend OAuth state mismatch".into());
        }
        self.consumed = true;
        if pairs.iter().any(|(k, _)| k == "error") {
            return Err("Backend sign-in was declined".into());
        }
        let codes: Vec<_> = pairs.iter().filter(|(k, _)| k == "code").collect();
        if codes.len() != 1 || codes[0].1.is_empty() || codes[0].1.len() > 16384 {
            return Err("Missing or ambiguous backend authorization code".into());
        }
        Ok(BackendCode {
            owner: self.owner.clone(),
            binding: self.binding.clone(),
            code: codes[0].1.to_string(),
            verifier: self.verifier.clone(),
            redirect: self.redirect.clone(),
        })
    }
}

#[cfg(test)]
#[path = "backend_tests.rs"]
mod tests;
