use crate::providers::{ClientRegistration, Provider};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
use url::Url;

pub const AUTH_LIFETIME: Duration = Duration::from_secs(600);

/// Secrets deliberately have no Debug implementation and never form a service reply.
pub struct Tokens {
    pub(crate) access: String,
    pub(crate) refresh: Option<String>,
    pub(crate) expires_at: Option<u64>,
    pub(crate) scopes: BTreeSet<String>,
}

impl Tokens {
    /// Provider token responses are only passed here by the trusted adapter.
    pub fn from_response(
        value: &serde_json::Value,
        requested: &BTreeSet<String>,
        now: u64,
    ) -> Result<Self, String> {
        let access = value["access_token"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 16384)
            .ok_or("Provider did not return an access token")?
            .to_string();
        if !value["token_type"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("bearer"))
        {
            return Err("Unsupported OAuth token type".into());
        }
        // OAuth permits omission when the issued scope equals the requested scope.
        let scopes = match value["scope"].as_str() {
            Some(scope) => scope
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            None => requested.clone(),
        };
        if !requested.is_subset(&scopes) {
            return Err("Required permissions were not granted".into());
        }
        Ok(Self {
            access,
            refresh: value["refresh_token"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 16384)
                .map(str::to_string),
            expires_at: value["expires_in"]
                .as_u64()
                .map(|seconds| now.saturating_add(seconds)),
            // The host retains only the scope requested for this app, even if a
            // provider reuses a previously granted, broader authorization.
            scopes: requested.clone(),
        })
    }
}

/// One desktop browser authorization. The caller identity comes from the host.
pub struct GoogleAttempt {
    owner: String,
    state: String,
    verifier: String,
    redirect: Url,
    deadline: Instant,
    consumed: bool,
    pub(crate) scopes: BTreeSet<String>,
    pub(crate) client_id: String,
}

pub struct AuthorizationCode {
    pub(crate) code: String,
    pub(crate) verifier: String,
    pub(crate) redirect: String,
}

impl GoogleAttempt {
    pub fn desktop(
        owner: &str,
        client: &ClientRegistration,
        scopes: &[String],
        port: u16,
        now: Instant,
    ) -> Result<Self, String> {
        client.validate()?;
        if owner.is_empty() || port == 0 {
            return Err("Missing OAuth caller or callback listener".into());
        }
        let mut random = [0_u8; 32];
        let mut state = [0_u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut random);
        rand::rngs::OsRng.fill_bytes(&mut state);
        Ok(Self {
            owner: owner.into(),
            state: URL_SAFE_NO_PAD.encode(state),
            verifier: URL_SAFE_NO_PAD.encode(random),
            redirect: Url::parse(&format!("http://127.0.0.1:{port}/oauth/callback")).unwrap(),
            deadline: now + AUTH_LIFETIME,
            consumed: false,
            scopes: Provider::Google.validate_scopes(scopes)?,
            client_id: client.client_id.clone(),
        })
    }
    pub fn authorization_url(&self) -> Url {
        let mut url = Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap();
        url.query_pairs_mut().extend_pairs([
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", self.redirect.as_str()),
            ("response_type", "code"),
            (
                "scope",
                &self.scopes.iter().cloned().collect::<Vec<_>>().join(" "),
            ),
            ("state", self.state.as_str()),
            ("code_challenge_method", "S256"),
            (
                "code_challenge",
                &URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.as_bytes())),
            ),
            ("access_type", "offline"),
            ("prompt", "consent select_account"),
        ]);
        url
    }
    pub fn cancel(&mut self) {
        self.consumed = true;
    }
    pub fn is_finished(&self) -> bool {
        self.consumed
    }
    pub fn consume_callback(
        &mut self,
        owner: &str,
        callback: &str,
        now: Instant,
    ) -> Result<AuthorizationCode, String> {
        if self.consumed || now >= self.deadline || owner != self.owner {
            return Err("Authorization expired or belongs to another app".into());
        }
        let callback = Url::parse(callback).map_err(|_| "Invalid OAuth callback")?;
        if callback.origin() != self.redirect.origin()
            || callback.path() != self.redirect.path()
            || callback.fragment().is_some()
            || !callback.username().is_empty()
            || callback.password().is_some()
        {
            return Err("OAuth callback destination mismatch".into());
        }
        let pairs: Vec<_> = callback.query_pairs().collect();
        let states: Vec<_> = pairs.iter().filter(|(key, _)| key == "state").collect();
        if states.len() != 1 || states[0].1 != self.state {
            return Err("OAuth state mismatch".into());
        }
        self.consumed = true;
        if pairs.iter().any(|(key, _)| key == "error") {
            return Err("Authorization was declined".into());
        }
        let codes: Vec<_> = pairs.iter().filter(|(key, _)| key == "code").collect();
        if codes.len() != 1 || codes[0].1.is_empty() || codes[0].1.len() > 16384 {
            return Err("Missing or ambiguous authorization code".into());
        }
        Ok(AuthorizationCode {
            code: codes[0].1.to_string(),
            verifier: self.verifier.clone(),
            redirect: self.redirect.to_string(),
        })
    }
}

/// Device-flow status: never expose a device code or access token to an app.
#[derive(Debug, PartialEq, Eq)]
pub enum DeviceStatus {
    Pending,
    SlowDown,
    Complete,
    Denied,
    Expired,
}

pub fn device_status(value: &serde_json::Value) -> Result<DeviceStatus, String> {
    match value["error"].as_str() {
        Some("authorization_pending") => Ok(DeviceStatus::Pending),
        Some("slow_down") => Ok(DeviceStatus::SlowDown),
        Some("access_denied") => Ok(DeviceStatus::Denied),
        Some("expired_token") => Ok(DeviceStatus::Expired),
        Some(_) => Err("GitHub authorization failed; check the host registration".into()),
        None if value["access_token"]
            .as_str()
            .is_some_and(|s| !s.is_empty()) =>
        {
            Ok(DeviceStatus::Complete)
        }
        None => Err("Invalid GitHub authorization response".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn attempt() -> GoogleAttempt {
        GoogleAttempt::desktop(
            "sample.calendar",
            &ClientRegistration {
                client_id: "test.apps.googleusercontent.com".into(),
            },
            &["openid".into()],
            32123,
            Instant::now(),
        )
        .unwrap()
    }
    fn callback(a: &GoogleAttempt) -> String {
        format!("{}?code=fixture-code&state={}", a.redirect, a.state)
    }
    #[test]
    fn browser_authorization_uses_pkce_and_no_secret() {
        let a = attempt();
        let url = a.authorization_url();
        assert_eq!(url.host_str(), Some("accounts.google.com"));
        let q: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(q["code_challenge_method"], "S256");
        assert_ne!(q["code_challenge"], a.verifier);
        assert!(a.verifier.len() >= 43);
        assert!(!q.contains_key("client_secret"));
    }
    #[test]
    fn callback_cannot_cross_apps_or_be_replayed() {
        let mut a = attempt();
        let callback = callback(&a);
        assert!(a
            .consume_callback("other", &callback, Instant::now())
            .is_err());
        let code = a
            .consume_callback("sample.calendar", &callback, Instant::now())
            .unwrap();
        assert_eq!(code.code, "fixture-code");
        assert!(!code.verifier.is_empty());
        assert_eq!(code.redirect, a.redirect.as_str());
        assert!(a
            .consume_callback("sample.calendar", &callback, Instant::now())
            .is_err());
    }
    #[test]
    fn rejects_wrong_state_origin_and_ambiguous_callback() {
        let mut a = attempt();
        let callback = callback(&a);
        for invalid in [
            callback.replace("127.0.0.1", "attacker.invalid"),
            callback.replace(&a.state, "wrong"),
            format!("{callback}&state={}", a.state),
        ] {
            assert!(a
                .consume_callback("sample.calendar", &invalid, Instant::now())
                .is_err());
        }
        assert!(a
            .consume_callback(
                "sample.calendar",
                &format!("{callback}&code=second"),
                Instant::now()
            )
            .is_err());
    }
    #[test]
    fn cancelled_and_expired_attempts_fail() {
        let mut a = attempt();
        let cancelled_callback = callback(&a);
        a.cancel();
        assert!(a
            .consume_callback("sample.calendar", &cancelled_callback, Instant::now())
            .is_err());
        let mut a = attempt();
        let callback = callback(&a);
        assert!(a
            .consume_callback("sample.calendar", &callback, Instant::now() + AUTH_LIFETIME)
            .is_err());
    }
    #[test]
    fn provider_grants_cannot_expand_app_scope() {
        let wanted = BTreeSet::from(["read:user".into()]);
        let token = Tokens::from_response(&serde_json::json!({"access_token":"fixture", "token_type":"bearer", "scope":"read:user,repo"}), &wanted, 0).unwrap();
        assert_eq!(token.scopes, wanted);
        assert!(Tokens::from_response(
            &serde_json::json!({"access_token":"fixture", "token_type":"bearer", "scope":"repo"}),
            &wanted,
            0
        )
        .is_err());
    }
}
