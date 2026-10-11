use crate::providers::{ClientRegistration, Provider};
use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, CsrfToken, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope,
};
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
        let (_, verifier) = PkceCodeChallenge::new_random_sha256();
        Ok(Self {
            owner: owner.into(),
            state: CsrfToken::new_random_len(32).secret().clone(),
            verifier: verifier.secret().clone(),
            redirect: Url::parse(&format!("http://127.0.0.1:{port}/oauth/callback")).unwrap(),
            deadline: now + AUTH_LIFETIME,
            consumed: false,
            scopes: Provider::Google.validate_scopes(scopes)?,
            client_id: client.client_id.clone(),
        })
    }
    pub fn authorization_url(&self) -> Url {
        let client = BasicClient::new(ClientId::new(self.client_id.clone()))
            .set_auth_uri(
                AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".into()).unwrap(),
            )
            .set_redirect_uri(RedirectUrl::from_url(self.redirect.clone()));
        client
            .authorize_url(|| CsrfToken::new(self.state.clone()))
            .add_scopes(self.scopes.iter().cloned().map(Scope::new))
            .set_pkce_challenge(PkceCodeChallenge::from_code_verifier_sha256(
                &PkceCodeVerifier::new(self.verifier.clone()),
            ))
            .add_extra_param("access_type", "offline")
            .add_extra_param("prompt", "consent select_account")
            .url()
            .0
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
    if value["error"].as_str().is_some() {
        use oauth2::DeviceCodeErrorResponseType as Error;
        let error: oauth2::DeviceCodeErrorResponse = serde_json::from_value(value.clone())
            .map_err(|_| "Invalid GitHub authorization response")?;
        return match error.error() {
            Error::AuthorizationPending => Ok(DeviceStatus::Pending),
            Error::SlowDown => Ok(DeviceStatus::SlowDown),
            Error::AccessDenied => Ok(DeviceStatus::Denied),
            Error::ExpiredToken => Ok(DeviceStatus::Expired),
            // The library error's Display/Debug may contain provider details.
            _ => Err("GitHub authorization failed; check the host registration".into()),
        };
    }
    if value["access_token"]
        .as_str()
        .is_some_and(|s| !s.is_empty())
    {
        Ok(DeviceStatus::Complete)
    } else {
        Err("Invalid GitHub authorization response".into())
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

    #[test]
    fn typed_device_errors_preserve_terminal_states_and_redact_provider_details() {
        for (code, expected) in [
            ("authorization_pending", DeviceStatus::Pending),
            ("slow_down", DeviceStatus::SlowDown),
            ("access_denied", DeviceStatus::Denied),
            ("expired_token", DeviceStatus::Expired),
        ] {
            assert_eq!(
                device_status(&serde_json::json!({"error":code,
                "error_description":"fixture-private-diagnostic"}))
                .unwrap(),
                expected
            );
        }
        assert_eq!(
            device_status(&serde_json::json!({"error":"fixture-private-code",
            "error_description":"fixture-private-diagnostic"}))
            .unwrap_err(),
            "GitHub authorization failed; check the host registration"
        );
    }
}
