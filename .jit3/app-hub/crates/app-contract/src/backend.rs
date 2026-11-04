//! Public backend registrations are signed app metadata; credentials are not.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct BackendRegistration {
    pub id: String,
    pub client_id: String,
    pub authorization_url: String,
    pub token_url: String,
    pub me_url: String,
    pub logout_url: String,
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub operations: BTreeMap<String, BackendOperation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct BackendOperation {
    pub method: BackendMethod,
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_keys: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
#[non_exhaustive]
pub enum BackendMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl BackendMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
    pub fn is_read(self) -> bool {
        matches!(self, Self::Get)
    }
}

fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// Paths are exact and unencoded so URL normalization cannot change authority
/// or turn a declared path into a different endpoint.
pub fn valid_path(path: &str) -> bool {
    path.starts_with('/')
        && path != "/"
        && !path.contains("//")
        && path.len() <= 1024
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-._~".contains(&b))
        && !path.split('/').any(|part| part == "." || part == "..")
}

impl BackendRegistration {
    pub fn validate(&self) -> Result<(), String> {
        if !token(&self.id) || self.id.len() > 64
            || self.client_id.is_empty()
            || self.client_id.len() > 256
            || self.client_id.chars().any(|c| c.is_control() || c.is_whitespace())
        {
            return Err("invalid backend id or public client_id".into());
        }
        if self.scopes != ["app.session"] {
            return Err("backend scopes must be [app.session]".into());
        }
        let mut origin = None;
        let mut endpoints = BTreeSet::new();
        let mut credential_paths = BTreeSet::new();
        for raw in [
            &self.authorization_url,
            &self.token_url,
            &self.me_url,
            &self.logout_url,
        ] {
            let url = Url::parse(raw).map_err(|_| "invalid backend endpoint URL")?;
            if url.scheme() != "https"
                || url.host_str().is_none()
                || url.port_or_known_default() != Some(443)
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || !valid_path(url.path())
                || raw.contains('%')
                || raw.contains('\\')
                || raw.chars().any(char::is_whitespace)
                || raw.split('/').any(|part| part == "." || part == "..")
            {
                return Err("backend endpoints require exact HTTPS URLs on port 443, without credentials, queries, fragments or traversal".into());
            }
            let here = url.origin().ascii_serialization();
            if origin.as_ref().is_some_and(|expected| expected != &here) {
                return Err("backend endpoints must share one origin".into());
            }
            origin = Some(here);
            credential_paths.insert(url.path().to_owned());
            if !endpoints.insert(url.to_string()) {
                return Err("backend endpoints must be distinct".into());
            }
        }
        if self.operations.len() > 64 {
            return Err("backend declares more than 64 operations".into());
        }
        for (name, operation) in &self.operations {
            if !token(name) || !valid_path(&operation.path) || operation.query_keys.len() > 32 {
                return Err(format!("invalid backend operation: {name}"));
            }
            if credential_paths.contains(&operation.path) {
                return Err(format!("backend operation {name} cannot call an authentication endpoint"));
            }
            let keys: BTreeSet<_> = operation.query_keys.iter().collect();
            if keys.len() != operation.query_keys.len() || keys.iter().any(|key| !token(key)) {
                return Err(format!("invalid backend query keys: {name}"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn registration() -> BackendRegistration {
        serde_json::from_value(serde_json::json!({"id":"sample", "client_id":"public-client", "authorization_url":"https://backend.example/authorize", "token_url":"https://backend.example/token", "me_url":"https://backend.example/me", "logout_url":"https://backend.example/logout", "scopes":["app.session"], "operations":{"orders.list":{"method":"GET","path":"/api/orders","query_keys":["page"]}}})).unwrap()
    }
    #[test]
    fn registration_rejects_credential_redirection_and_traversal() {
        assert!(registration().validate().is_ok());
        for value in [
            "http://backend.example/token",
            "https://other.example/token",
            "https://backend.example/a/../token",
            "https://backend.example/%74oken",
            "https://user@backend.example/token",
        ] {
            let mut r = registration();
            r.token_url = value.into();
            assert!(r.validate().is_err(), "{value}");
        }
        for path in [
            "//other.example/orders",
            "/a/../orders",
            "/api/%2e%2e/orders",
            "/api/orders?admin=true",
            "/token",
            "/authorize",
            "/logout",
            "/me",
        ] {
            let mut r = registration();
            r.operations.get_mut("orders.list").unwrap().path = path.into();
            assert!(r.validate().is_err(), "{path}");
        }
    }
}
