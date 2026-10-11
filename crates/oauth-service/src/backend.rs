//! Public-client login and declared business operations for one developer backend.
//!
//! This is not an open HTTP proxy. Every operation has one immutable
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
    collections::{BTreeMap, BTreeSet},
    io::Read,
    time::{Duration, Instant},
};
use url::Url;

pub const SESSION_SCOPE: &str = "app.session";
/// Host-owned interception target, never fetched by the embedded login view.
/// A contained app or backend registration cannot choose this destination.
pub const WEBVIEW_CALLBACK_URL: &str = "https://octosense.invalid/auth/callback";
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub operations: BTreeMap<String, BackendOperation>,
}

/// Bundle metadata. The host supplies the admitted app identity separately.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendDeclaration {
    pub id: String,
    pub client_id: String,
    pub authorization_url: String,
    pub token_url: String,
    pub me_url: String,
    pub logout_url: String,
    pub scopes: BTreeSet<String>,
    #[serde(default)]
    pub operations: BTreeMap<String, BackendOperation>,
}
impl BackendDeclaration {
    pub fn into_registration(self, app_id: &str) -> BackendRegistration {
        BackendRegistration {
            id: self.id,
            app_id: app_id.into(),
            client_id: self.client_id,
            authorization_url: self.authorization_url,
            token_url: self.token_url,
            me_url: self.me_url,
            logout_url: self.logout_url,
            scopes: self.scopes,
            operations: self.operations,
        }
    }
}

/// Exact same-origin endpoint; dynamic data goes in declared query keys or JSON.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BackendOperation {
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub query_keys: BTreeSet<String>,
}
impl BackendOperation {
    pub fn mutates(&self) -> bool {
        self.method != "GET"
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendRequest {
    pub connection: String,
    pub operation: String,
    #[serde(default)]
    pub query: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
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
        if registration.operations.len() > 64 {
            return Err("Backend declares too many operations".into());
        }
        for (name, operation) in &registration.operations {
            let valid_name = |s: &str| {
                !s.is_empty()
                    && s.len() <= 128
                    && s.bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
            };
            if !valid_name(name)
                || !matches!(
                    operation.method.as_str(),
                    "GET" | "POST" | "PUT" | "PATCH" | "DELETE"
                )
                || operation.query_keys.len() > 32
                || operation.query_keys.iter().any(|s| !valid_name(s))
            {
                return Err("Invalid backend operation name, method or query keys".into());
            }
            // Exact ASCII paths avoid percent decoding, dot-segment and authority ambiguity.
            let path = &operation.path;
            if !path.starts_with('/')
                || path.len() > 1024
                || path == "/"
                || path.contains("//")
                || path.split('/').any(|s| s == "." || s == "..")
                || !path
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"/_-.~".contains(&c))
            {
                return Err("Backend operation requires an exact absolute path".into());
            }
            let endpoint = urls[0]
                .join(path)
                .map_err(|_| "Invalid backend operation path")?;
            if urls.contains(&endpoint) {
                return Err("Business operations cannot target authentication endpoints".into());
            }
        }
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
        self.begin_with_redirect(caller, redirect, now)
    }

    /// Begin an embedded backend login. The platform must intercept this exact
    /// callback before navigation; the authorization code stays in host Rust.
    /// Browser login remains a separate loopback-only entry point.
    pub fn begin_webview(&self, caller: &str, now: Instant) -> Result<BackendAttempt, String> {
        self.caller(caller)?;
        let redirect = Url::parse(WEBVIEW_CALLBACK_URL).expect("Fixed host WebView callback");
        self.begin_with_redirect(caller, redirect, now)
    }

    fn begin_with_redirect(
        &self,
        caller: &str,
        redirect: Url,
        now: Instant,
    ) -> Result<BackendAttempt, String> {
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

    pub fn operation(&self, name: &str) -> Result<&BackendOperation, String> {
        self.registration
            .operations
            .get(name)
            .ok_or_else(|| "Backend operation is not declared by this app".into())
    }

    /// The host checks the connection binding and approval before this transport.
    /// No URL, method, header or credential override is accepted from the app.
    pub fn request(
        &self,
        caller: &str,
        tokens: &Tokens,
        request: &BackendRequest,
    ) -> Result<serde_json::Value, String> {
        self.caller(caller)?;
        self.validate_request(request)?;
        let operation = self.operation(&request.operation)?;
        let body = request
            .body
            .as_ref()
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|_| "Invalid backend request body")?;
        let mut url = Url::parse(&self.registration.authorization_url)
            .unwrap()
            .join(&operation.path)
            .map_err(|_| "Invalid backend operation path")?;
        if !request.query.is_empty() {
            url.query_pairs_mut().extend_pairs(&request.query);
        }
        let method = reqwest::Method::from_bytes(operation.method.as_bytes())
            .map_err(|_| "Invalid backend method")?;
        let mut outgoing = self
            .http
            .request(method, url)
            .bearer_auth(&tokens.access)
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(body) = body {
            outgoing = outgoing
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body);
        }
        let response = outgoing
            .send()
            .map_err(|_| "Backend operation connection failed")?;
        let value = response_json(response)?;
        // A faulty endpoint must not accidentally expose its Authorization header.
        let encoded = serde_json::to_string(&value).map_err(|_| "Invalid backend response")?;
        if encoded.contains(&tokens.access)
            || tokens
                .refresh
                .as_ref()
                .is_some_and(|token| encoded.contains(token))
        {
            return Err("Backend response exposed credential material".into());
        }
        Ok(value)
    }

    pub fn validate_request(&self, request: &BackendRequest) -> Result<(), String> {
        let operation = self.operation(&request.operation)?;
        if request.query.len() > 32
            || request.query.iter().any(|(key, value)| {
                !operation.query_keys.contains(key)
                    || value.len() > 2048
                    || value.chars().any(char::is_control)
            })
        {
            return Err("Backend query contains an undeclared key or oversized value".into());
        }
        if !operation.mutates() && request.body.is_some() {
            return Err("Backend GET cannot carry a body".into());
        }
        let body = request
            .body
            .as_ref()
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|_| "Invalid backend request body")?;
        if body
            .as_ref()
            .is_some_and(|body| body.len() > RESPONSE_LIMIT)
        {
            return Err("Backend request exceeds 64 KiB".into());
        }
        Ok(())
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
    if bytes.is_empty() {
        return Ok(serde_json::Value::Null);
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
        // Keep the embedded callback byte-exact. URL parsing otherwise accepts
        // equivalent-looking ports, host casing or dot-segment path aliases.
        if self.redirect.as_str() == WEBVIEW_CALLBACK_URL
            && !callback
                .strip_prefix(WEBVIEW_CALLBACK_URL)
                .is_some_and(|suffix| suffix.starts_with('?'))
        {
            return Err("Backend callback destination mismatch".into());
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
