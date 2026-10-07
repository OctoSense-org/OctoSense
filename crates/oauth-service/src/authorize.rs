use crate::{
    oauth::{device_status, AuthorizationCode, DeviceStatus, Tokens},
    providers::{ClientRegistration, Provider},
    transport::{form, json_ok, Body, Request, Transport},
};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
use url::Url;

pub struct GithubDeviceAttempt {
    owner: String,
    client_id: String,
    device_code: String,
    scopes: BTreeSet<String>,
    next_poll: Instant,
    deadline: Instant,
    interval: Duration,
    finished: bool,
    /// Display only on the host-owned sheet, not in the script app's reply.
    pub user_code: String,
    pub verification_uri: String,
}

pub enum DevicePoll {
    Wait(Duration),
    Authorized(Tokens),
    Denied,
    Expired,
}

impl GithubDeviceAttempt {
    pub fn begin(
        owner: &str,
        client: &ClientRegistration,
        scopes: &[String],
        transport: &dyn Transport,
        now: Instant,
    ) -> Result<Self, String> {
        client.validate()?;
        if owner.is_empty() {
            return Err("Missing OAuth caller".into());
        }
        let scopes = Provider::Github.validate_scopes(scopes)?;
        let value = crate::protocol::github_device_authorization(client, &scopes, transport)?;
        let device_code = value["device_code"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 4096)
            .ok_or("GitHub did not provide a device authorization")?
            .to_string();
        let user_code = value["user_code"]
            .as_str()
            .filter(|s| {
                !s.is_empty()
                    && s.len() <= 64
                    && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            })
            .ok_or("Invalid GitHub verification code")?
            .to_string();
        let verification_uri = value["verification_uri"]
            .as_str()
            .filter(|s| *s == "https://github.com/login/device")
            .ok_or("Unexpected GitHub verification page")?
            .to_string();
        let interval = Duration::from_secs(value["interval"].as_u64().unwrap_or(5).clamp(5, 60));
        let lifetime = value["expires_in"]
            .as_u64()
            .filter(|s| *s > 0 && *s <= 1800)
            .ok_or("Invalid GitHub authorization expiry")?;
        Ok(Self {
            owner: owner.into(),
            client_id: client.client_id.clone(),
            device_code,
            scopes,
            next_poll: now + interval,
            deadline: now + Duration::from_secs(lifetime),
            interval,
            finished: false,
            user_code,
            verification_uri,
        })
    }
    pub fn cancel(&mut self) {
        self.finished = true;
    }
    pub fn poll(
        &mut self,
        caller: &str,
        transport: &dyn Transport,
        now: Instant,
        unix_now: u64,
    ) -> Result<DevicePoll, String> {
        if caller != self.owner {
            return Err("Authorization belongs to another app".into());
        }
        if self.finished || now >= self.deadline {
            self.finished = true;
            return Ok(DevicePoll::Expired);
        }
        if now < self.next_poll {
            return Ok(DevicePoll::Wait(self.next_poll - now));
        }
        self.next_poll = now + self.interval;
        // oauth2 5 exposes only a complete device-token polling loop, not a
        // single-poll builder. Keep this small wire request and host scheduler
        // so every poll retains caller, cancellation and deadline checks.
        let value = json_ok(transport.send(form(
            Provider::Github.token_endpoint(),
            vec![
                ("client_id", self.client_id.clone()),
                ("device_code", self.device_code.clone()),
                (
                    "grant_type",
                    "urn:ietf:params:oauth:grant-type:device_code".into(),
                ),
            ],
        ))?)?;
        match device_status(&value)? {
            DeviceStatus::Pending => Ok(DevicePoll::Wait(self.interval)),
            DeviceStatus::SlowDown => {
                self.interval += Duration::from_secs(5);
                self.next_poll = now + self.interval;
                Ok(DevicePoll::Wait(self.interval))
            }
            DeviceStatus::Denied => {
                self.finished = true;
                Ok(DevicePoll::Denied)
            }
            DeviceStatus::Expired => {
                self.finished = true;
                Ok(DevicePoll::Expired)
            }
            DeviceStatus::Complete => {
                self.finished = true;
                Ok(DevicePoll::Authorized(Tokens::from_response(
                    &value,
                    &self.scopes,
                    unix_now,
                )?))
            }
        }
    }
}

pub fn exchange_google(
    client: &ClientRegistration,
    client_secret: Option<&str>,
    code: AuthorizationCode,
    scopes: &BTreeSet<String>,
    transport: &dyn Transport,
    now: u64,
) -> Result<Tokens, String> {
    let response = crate::protocol::google_code(client, client_secret, code, transport)?;
    Tokens::from_response(&response, scopes, now)
}

pub(crate) fn refresh_google(
    client: &ClientRegistration,
    client_secret: Option<&str>,
    old: Tokens,
    transport: &dyn Transport,
    now: u64,
) -> Result<Tokens, String> {
    let refresh = old
        .refresh
        .as_deref()
        .ok_or("Sign in again: no refresh credential")?;
    let response = crate::protocol::google_refresh(client, client_secret, refresh, transport)?;
    let mut token = Tokens::from_response(&response, &old.scopes, now)?;
    if token.refresh.is_none() {
        token.refresh = old.refresh;
    }
    Ok(token)
}

/// Verify identity using the provider, rather than an app-supplied email address.
pub fn identity(
    provider: Provider,
    tokens: &Tokens,
    transport: &dyn Transport,
) -> Result<(String, String), String> {
    let endpoint = match provider {
        Provider::Github => "https://api.github.com/user",
        Provider::Google => "https://openidconnect.googleapis.com/v1/userinfo",
    };
    let body = json_ok(transport.send(Request {
        method: "GET",
        url: Url::parse(endpoint).unwrap(),
        bearer: Some(tokens.access.clone()),
        if_match: None,
        body: Body::Empty,
    })?)?;
    match provider {
        Provider::Github => {
            let id = body["id"].as_u64().ok_or("GitHub identity was missing")?;
            let label = body["login"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("GitHub account name was missing")?;
            Ok((id.to_string(), label.into()))
        }
        Provider::Google => {
            let id = body["sub"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("Google identity was missing")?;
            let label = if body["email_verified"] == true {
                body["email"].as_str().unwrap_or("Google account")
            } else {
                "Google account"
            };
            Ok((id.into(), label.into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Response;
    use serde_json::json;
    use std::sync::Mutex;
    struct Fixture(Mutex<Vec<serde_json::Value>>);
    impl Transport for Fixture {
        fn send(&self, _: Request) -> Result<Response, String> {
            Ok(Response {
                status: 200,
                body: self.0.lock().unwrap().remove(0),
                etag: None,
            })
        }
    }
    #[test]
    fn device_flow_obeys_poll_interval_slow_down_and_single_completion() {
        let transport = Fixture(Mutex::new(vec![
            json!({"device_code":"fixture-private-code","user_code":"ABCD-EFGH","verification_uri":"https://github.com/login/device","interval":5,"expires_in":900}),
            json!({"error":"slow_down"}),
            json!({"access_token":"fixture-token","token_type":"bearer","scope":"read:user"}),
        ]));
        let now = Instant::now();
        let mut a = GithubDeviceAttempt::begin(
            "notes",
            &ClientRegistration {
                client_id: "fixture-client".into(),
            },
            &["read:user".into()],
            &transport,
            now,
        )
        .unwrap();
        assert!(matches!(
            a.poll("notes", &transport, now, 0).unwrap(),
            DevicePoll::Wait(_)
        ));
        assert_eq!(transport.0.lock().unwrap().len(), 2);
        assert!(a
            .poll("another-app", &transport, now + Duration::from_secs(5), 5)
            .is_err());
        assert!(
            matches!(a.poll("notes",&transport,now+Duration::from_secs(5),5).unwrap(),DevicePoll::Wait(d) if d==Duration::from_secs(10))
        );
        assert!(matches!(
            a.poll("notes", &transport, now + Duration::from_secs(10), 10)
                .unwrap(),
            DevicePoll::Wait(_)
        ));
        assert!(matches!(
            a.poll("notes", &transport, now + Duration::from_secs(15), 15)
                .unwrap(),
            DevicePoll::Authorized(_)
        ));
        assert!(matches!(
            a.poll("notes", &transport, now + Duration::from_secs(20), 20)
                .unwrap(),
            DevicePoll::Expired
        ));
    }
    #[test]
    fn refresh_keeps_rotating_credential_when_provider_omits_it() {
        let fixture = Fixture(Mutex::new(vec![
            json!({"access_token":"new-fixture","token_type":"Bearer","expires_in":3600}),
        ]));
        let old = Tokens {
            access: "old-fixture".into(),
            refresh: Some("refresh-fixture".into()),
            expires_at: Some(1),
            scopes: BTreeSet::from(["openid".into()]),
        };
        let fresh = refresh_google(
            &ClientRegistration {
                client_id: "fixture-client".into(),
            },
            None,
            old,
            &fixture,
            10,
        )
        .unwrap();
        assert_eq!(fresh.refresh.as_deref(), Some("refresh-fixture"));
        assert_eq!(fresh.expires_at, Some(3610));
    }

    #[test]
    fn cancelled_or_expired_device_attempt_never_polls_transport() {
        let now = Instant::now();
        for cancel in [false, true] {
            let transport = Fixture(Mutex::new(vec![json!({"device_code":"fixture-code",
                "user_code":"ABCD-EFGH","verification_uri":"https://github.com/login/device",
                "expires_in":10})]));
            let mut attempt = GithubDeviceAttempt::begin(
                "notes",
                &ClientRegistration {
                    client_id: "fixture-client".into(),
                },
                &["read:user".into()],
                &transport,
                now,
            )
            .unwrap();
            if cancel {
                attempt.cancel();
            }
            let poll_at = now + Duration::from_secs(if cancel { 5 } else { 10 });
            assert!(matches!(
                attempt.poll("notes", &transport, poll_at, 10).unwrap(),
                DevicePoll::Expired
            ));
            // Fixture has no response left: any additional send would panic.
            assert!(transport.0.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn library_token_exchange_still_caps_scopes_and_rejects_oversized_credentials() {
        let scopes = BTreeSet::from(["openid".into()]);
        for access in ["fixture-token".to_owned(), "x".repeat(16_385)] {
            let transport = Fixture(Mutex::new(vec![json!({"access_token":access,
                "token_type":"Bearer","scope":"openid email"})]));
            let result = exchange_google(
                &ClientRegistration {
                    client_id: "fixture-client".into(),
                },
                None,
                AuthorizationCode {
                    code: "fixture-code".into(),
                    verifier: "fixture-verifier".into(),
                    redirect: "http://127.0.0.1:32123/oauth/callback".into(),
                },
                &scopes,
                &transport,
                0,
            );
            if access.len() > 16_384 {
                assert!(result.is_err());
            } else {
                assert_eq!(result.unwrap().scopes, scopes);
            }
        }
    }
}
