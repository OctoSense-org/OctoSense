use super::*;

fn registration(origin: &str) -> BackendRegistration {
    BackendRegistration {
        id: "fixture".into(),
        app_id: "org.octosense.samples.backend".into(),
        client_id: "octosense-fixture".into(),
        authorization_url: format!("{origin}/authorize"),
        token_url: format!("{origin}/token"),
        me_url: format!("{origin}/me"),
        logout_url: format!("{origin}/logout"),
        scopes: BTreeSet::from([SESSION_SCOPE.into()]),
    }
}
fn caller() -> &'static str {
    "org.octosense.samples.backend"
}
fn begin(client: &BackendClient) -> BackendAttempt {
    client
        .begin(
            caller(),
            Url::parse("http://127.0.0.1:32123/oauth/callback").unwrap(),
            Instant::now(),
        )
        .unwrap()
}
fn callback(attempt: &BackendAttempt) -> String {
    let mut url = attempt.redirect.clone();
    url.query_pairs_mut()
        .append_pair("code", "fictional-code")
        .append_pair("state", &attempt.state);
    url.to_string()
}

#[test]
fn backend_registration_refuses_http_cross_origin_endpoint_overlap_and_unbounded_grants() {
    assert!(BackendClient::new(registration("http://127.0.0.1:4321")).is_err());
    let original = registration("https://backend.example.test");
    for field in [
        "https://other.example.test/me",
        "https://backend.example.test/me?token=oops",
        "https://user@backend.example.test/me",
        "https://backend.example.test/me#fragment",
        "https://backend.example.test:444/me",
        "https://backend.example.test/authorize",
    ] {
        let mut r = original.clone();
        r.me_url = field.into();
        assert!(BackendClient::new(r).is_err(), "Endpoint accepted: {field}");
    }
    let mut r = original.clone();
    r.scopes.insert("admin".into());
    assert!(BackendClient::new(r).is_err());
    assert!(BackendClient::new(original).is_ok());
}

#[test]
fn authorization_is_caller_callback_state_pkce_bound_and_single_use() {
    let client = BackendClient::new(registration("https://backend.example.test")).unwrap();
    let mut attempt = begin(&client);
    let q: std::collections::BTreeMap<_, _> = attempt.authorization_url().query_pairs().collect();
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["scope"], "app.session");
    assert!(!q.contains_key("client_secret"));
    assert!(!q.contains_key("code_verifier"));
    let valid = callback(&attempt);
    assert!(attempt
        .consume_callback("other.app", &valid, Instant::now())
        .is_err());
    assert!(attempt
        .consume_callback(caller(), &valid.replace("32123", "32124"), Instant::now())
        .is_err());
    assert!(attempt
        .consume_callback(
            caller(),
            &valid.replace("/oauth/callback", "/another"),
            Instant::now()
        )
        .is_err());
    assert!(attempt
        .consume_callback(
            caller(),
            &(valid.clone() + "&state=duplicate"),
            Instant::now()
        )
        .is_err());
    assert!(attempt
        .consume_callback(
            caller(),
            &valid.replace(&attempt.state, "wrong-state"),
            Instant::now()
        )
        .is_err());
    assert!(!attempt.is_finished());
    assert!(attempt
        .consume_callback(caller(), &valid, Instant::now())
        .is_ok());
    assert!(attempt
        .consume_callback(caller(), &valid, Instant::now())
        .is_err());
}

#[test]
fn webview_uses_only_fixed_callback_and_preserves_pkce_owner_state_and_single_use() {
    let client = BackendClient::new(registration("https://backend.example.test")).unwrap();
    assert!(client.begin_webview("another.app", Instant::now()).is_err());
    assert!(client
        .begin(
            caller(),
            Url::parse(WEBVIEW_CALLBACK_URL).unwrap(),
            Instant::now()
        )
        .is_err());
    let mut attempt = client.begin_webview(caller(), Instant::now()).unwrap();
    let query: std::collections::BTreeMap<_, _> =
        attempt.authorization_url().query_pairs().collect();
    assert_eq!(query["redirect_uri"], WEBVIEW_CALLBACK_URL);
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(query["scope"], SESSION_SCOPE);
    assert!(!query.contains_key("code_verifier"));
    let valid = callback(&attempt);
    for invalid in [
        valid.replace("octosense.invalid", "other.invalid"),
        valid.replace("octosense.invalid", "octosense.invalid:443"),
        valid.replace("octosense.invalid", "OCTOSENSE.INVALID"),
        valid.replace("/auth/callback", "/auth/../auth/callback"),
        valid.replace("/auth/callback", "/auth/callback/"),
        valid.replace(&attempt.state, "wrong-state"),
        format!("{valid}#fragment"),
        format!("{valid}&state=duplicate"),
    ] {
        assert!(attempt
            .consume_callback(caller(), &invalid, Instant::now())
            .is_err());
        assert!(!attempt.is_finished());
    }
    assert!(attempt
        .consume_callback("another.app", &valid, Instant::now())
        .is_err());
    let code = attempt
        .consume_callback(caller(), &valid, Instant::now())
        .unwrap();
    assert_eq!(code.redirect.as_str(), WEBVIEW_CALLBACK_URL);
    assert!(attempt
        .consume_callback(caller(), &valid, Instant::now())
        .is_err());
    let mut cancelled = client.begin_webview(caller(), Instant::now()).unwrap();
    let valid = callback(&cancelled);
    cancelled.cancel();
    assert!(cancelled
        .consume_callback(caller(), &valid, Instant::now())
        .is_err());
}

#[test]
fn cancel_expiry_decline_and_registration_change_cannot_complete_authorization() {
    let client = BackendClient::new(registration("https://backend.example.test")).unwrap();
    let mut cancelled = begin(&client);
    let url = callback(&cancelled);
    cancelled.cancel();
    assert!(cancelled
        .consume_callback(caller(), &url, Instant::now())
        .is_err());
    let mut expired = begin(&client);
    let url = callback(&expired);
    assert!(expired
        .consume_callback(caller(), &url, expired.deadline)
        .is_err());
    let mut declined = begin(&client);
    let url = callback(&declined) + "&error=access_denied";
    assert!(declined
        .consume_callback(caller(), &url, Instant::now())
        .is_err());
    assert!(declined.is_finished());
    let mut attempt = begin(&client);
    let url = callback(&attempt);
    let code = attempt
        .consume_callback(caller(), &url, Instant::now())
        .unwrap();
    let mut changed = client.registration().clone();
    changed.token_url = "https://backend.example.test/new-token".into();
    let changed = BackendClient::new(changed).unwrap();
    assert_ne!(client.binding(), changed.binding());
    // Refused before DNS/network, including a changed endpoint on the SAME origin.
    assert_eq!(
        changed.finish(caller(), code, 0).err().unwrap(),
        "Backend authorization registration changed"
    );
}

#[cfg(feature = "acceptance-fixtures")]
mod real_http {
    use super::*;
    use std::{
        path::PathBuf,
        process::{Child, Command, Stdio},
    };

    struct Server {
        child: Child,
        directory: PathBuf,
        client: BackendClient,
    }
    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
    impl Server {
        fn start() -> Self {
            let directory = std::env::temp_dir().join(format!(
                "octosense-backend-protocol-{}",
                uuid::Uuid::new_v4()
            ));
            let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/backend-login-fixture.py");
            let child = Command::new("python3")
                .arg(script)
                .arg("--directory")
                .arg(&directory)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let fallback =
                BackendClient::new(registration("https://backend.example.test")).unwrap();
            let mut server = Self {
                child,
                directory,
                client: fallback,
            };
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Ok(bytes) = std::fs::read(server.directory.join("metadata.json")) {
                    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    server.client = BackendClient::new_loopback_fixture(
                        serde_json::from_value(value["registration"].clone()).unwrap(),
                    )
                    .unwrap();
                    return server;
                }
                assert!(Instant::now() < deadline, "Fixture failed to start");
                assert!(server.child.try_wait().unwrap().is_none(), "Fixture exited");
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        /// Exercises server-owned HTML/forms over real HTTP. This is protocol
        /// acceptance, not a claim that native browser pixels were tested.
        fn browser(&self, attempt: &BackendAttempt, register: bool) -> String {
            let http = reqwest::blocking::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap();
            let html = http
                .get(attempt.authorization_url().clone())
                .send()
                .unwrap()
                .error_for_status()
                .unwrap()
                .text()
                .unwrap();
            let flow = html
                .split("name=\"flow\" value=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap();
            let session_url = Url::parse(&self.client.registration().authorization_url)
                .unwrap()
                .join("/session")
                .unwrap();
            let mut fields = vec![
                ("flow", flow),
                ("username", "fictional-acceptance-user"),
                ("password", "fictional-only-password"),
                ("action", "register"),
            ];
            if register {
                let response = http.post(session_url.clone()).form(&fields).send().unwrap();
                assert_eq!(response.status(), 201);
                assert!(response
                    .text()
                    .unwrap()
                    .contains("Account created. Sign in to continue."));
            }
            fields[3].1 = "login";
            let mut wrong_password = fields.clone();
            wrong_password[2].1 = "wrong-fictional-password";
            let response = http
                .post(session_url.clone())
                .form(&wrong_password)
                .send()
                .unwrap();
            assert_eq!(response.status(), 401);
            assert!(response.headers().get("Location").is_none());
            let response = http.post(session_url).form(&fields).send().unwrap();
            assert_eq!(response.status(), 303);
            response
                .headers()
                .get("Location")
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
        }
    }

    fn duplicate(code: &BackendCode) -> BackendCode {
        BackendCode {
            owner: code.owner.clone(),
            binding: code.binding.clone(),
            code: code.code.clone(),
            verifier: code.verifier.clone(),
            redirect: code.redirect.clone(),
        }
    }

    #[test]
    fn actual_signup_login_exchange_identity_refresh_logout_and_token_replay() {
        let server = Server::start();
        let client = &server.client;
        let mut attempt = begin(client);
        let callback = server.browser(&attempt, true);
        let code = attempt
            .consume_callback(caller(), &callback, Instant::now())
            .unwrap();
        let replay = duplicate(&code);
        let authorized = client.finish(caller(), code, 1_800_000_000).unwrap();
        assert!(authorized.identity.sub.starts_with("synthetic-"));
        assert_eq!(authorized.identity.label, "fictional-acceptance-user");
        assert!(client.finish(caller(), replay, 1_800_000_000).is_err());
        assert!(client.me("another.app", &authorized.tokens).is_err());
        let refreshed = client
            .refresh(caller(), &authorized.tokens, 1_800_000_001)
            .unwrap();
        assert_ne!(refreshed.access, authorized.tokens.access);
        assert!(client
            .refresh(caller(), &authorized.tokens, 1_800_000_002)
            .is_err());
        assert_eq!(
            client.me(caller(), &refreshed).unwrap(),
            authorized.identity
        );
        let mut independent_attempt = begin(client);
        let callback = server.browser(&independent_attempt, false);
        let code = independent_attempt
            .consume_callback(caller(), &callback, Instant::now())
            .unwrap();
        let independent = client.finish(caller(), code, 1_800_000_002).unwrap();
        client.logout(caller(), &refreshed).unwrap();
        assert!(client.me(caller(), &refreshed).is_err());
        assert_eq!(
            client.me(caller(), &independent.tokens).unwrap(),
            independent.identity
        );
        client.logout(caller(), &independent.tokens).unwrap();
        assert!(client.refresh(caller(), &refreshed, 1_800_000_003).is_err());
        let journal = std::fs::read_to_string(server.directory.join("events.jsonl")).unwrap();
        for secret in [
            &authorized.tokens.access,
            refreshed.refresh.as_ref().unwrap(),
            "fictional-only-password",
            &attempt.state,
        ] {
            assert!(!journal.contains(secret));
        }
        assert!(journal.contains("\"event\": \"register\""));
        assert!(journal.contains("\"event\": \"logout\""));
    }

    #[test]
    fn fixed_webview_redirect_uses_same_real_server_exchange_and_pkce() {
        let server = Server::start();
        let client = &server.client;
        let mut attempt = client.begin_webview(caller(), Instant::now()).unwrap();
        // Protocol-only test: native form input/interception is validated by
        // the separate WebView acceptance driver, never inferred from this.
        let callback = server.browser(&attempt, true);
        assert!(callback.starts_with("https://octosense.invalid/auth/callback?"));
        let code = attempt
            .consume_callback(caller(), &callback, Instant::now())
            .unwrap();
        let authorized = client.finish(caller(), code, 1_800_000_000).unwrap();
        assert_eq!(authorized.identity.label, "fictional-acceptance-user");
        client.logout(caller(), &authorized.tokens).unwrap();
        assert!(client.me(caller(), &authorized.tokens).is_err());
    }

    #[test]
    fn server_rejects_wrong_pkce_then_consumes_code_and_cancel_does_not_exchange() {
        let server = Server::start();
        let client = &server.client;
        let mut attempt = begin(client);
        let callback = server.browser(&attempt, true);
        let code = attempt
            .consume_callback(caller(), &callback, Instant::now())
            .unwrap();
        let mut wrong = duplicate(&code);
        wrong.verifier = "different-verifier".repeat(4);
        assert!(client.finish(caller(), wrong, 0).is_err());
        assert!(client.finish(caller(), code, 0).is_err());
        let mut cancelled = begin(client);
        let callback = server.browser(&cancelled, false);
        cancelled.cancel();
        assert!(cancelled
            .consume_callback(caller(), &callback, Instant::now())
            .is_err());
        let journal = std::fs::read_to_string(server.directory.join("events.jsonl")).unwrap();
        assert_eq!(
            journal
                .lines()
                .filter(|line| line.contains("\"event\": \"exchange\""))
                .count(),
            2
        );
        assert!(!journal.contains("\"event\": \"me\""));
    }
}
