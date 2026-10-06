use crate::{api::*, oauth::Tokens, transport::*, Connections, CredentialStore, Provider};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Vault(Mutex<BTreeMap<String, String>>);
impl CredentialStore for Vault {
    fn put(&self, key: &str, value: &str) -> Result<(), String> {
        self.0.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }
    fn get(&self, key: &str) -> Result<String, String> {
        self.0
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or("Missing fixture".into())
    }
    fn remove(&self, key: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}
struct Fixture {
    replies: Mutex<VecDeque<Response>>,
    calls: Mutex<Vec<Request>>,
}
impl Fixture {
    fn new(replies: Vec<(u16, Value)>) -> Self {
        Self {
            replies: Mutex::new(
                replies
                    .into_iter()
                    .map(|(status, body)| Response {
                        status,
                        body,
                        etag: None,
                    })
                    .collect(),
            ),
            calls: Mutex::new(Vec::new()),
        }
    }
}
impl Transport for Fixture {
    fn send(&self, request: Request) -> Result<Response, String> {
        self.calls.lock().unwrap().push(request);
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .ok_or("Unexpected fixture request".into())
    }
}
struct Profile(std::path::PathBuf);
impl Profile {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("oauth-api-test-{}", uuid::Uuid::new_v4())))
    }
    fn connect(&self, provider: Provider, scopes: &[&str]) -> (Connections, String) {
        let mut store = Connections::open(&self.0, Arc::new(Vault::default())).unwrap();
        let connection = store
            .connect(
                "fixture.app",
                provider,
                "fixture-user",
                "Fixture account",
                Tokens {
                    access: "fixture-token".into(),
                    refresh: None,
                    expires_at: None,
                    scopes: scopes
                        .iter()
                        .map(|s| s.to_string())
                        .collect::<BTreeSet<_>>(),
                },
            )
            .unwrap();
        (store, connection.handle)
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn api<'a>(connections: &'a mut Connections, transport: &'a Fixture) -> Api<'a> {
    Api {
        connections,
        transport,
        google_client: None,
        google_client_secret: None,
        now: 1,
    }
}

#[test]
fn github_writes_exact_sha_and_retains_conflict_without_retry() {
    let profile = Profile::new();
    let (mut store, handle) = profile.connect(Provider::Github, &["public_repo"]);
    let fixture = Fixture::new(vec![(
        409,
        json!({"private":"never surface raw provider error"}),
    )]);
    let draft = GithubFile {
        owner: "example".into(),
        repo: "notes".into(),
        branch: "main".into(),
        path: "notes/hello.md".into(),
        content: "# Reviewed version\n".into(),
        message: "Update note".into(),
        sha: Some("a".repeat(40)),
    };
    let error = api(&mut store, &fixture)
        .github_save("fixture.app", &handle, &draft)
        .unwrap_err();
    assert!(error.contains("remote content changed"));
    assert!(!error.contains("private"));
    let calls = fixture.calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "Conflict never retries without review");
    assert_eq!(calls[0].method, "PUT");
    let Body::Json(body) = &calls[0].body else {
        panic!("JSON save")
    };
    assert_eq!(body["sha"], "a".repeat(40));
    use base64::Engine;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(body["content"].as_str().unwrap())
            .unwrap(),
        draft.content.as_bytes()
    );
}
#[test]
fn directory_listing_returns_no_temporary_download_credential() {
    let profile = Profile::new();
    let (mut store, handle) = profile.connect(Provider::Github, &["public_repo"]);
    let fixture = Fixture::new(vec![(
        200,
        json!([
            {"name":"note.md","path":"note.md","type":"file","sha":"abc","download_url":"https://example.invalid/?token=private"},
            {"name":"docs","path":"docs","type":"dir","sha":"def"},
            {"name":"secret.txt","path":"secret.txt","type":"file"},
            {"name":"link.md","path":"link.md","type":"symlink"}
        ]),
    )]);
    let value = api(&mut store, &fixture)
        .github_files("fixture.app", &handle, "example", "notes", "main", "")
        .unwrap();
    assert_eq!(value["files"].as_array().unwrap().len(), 2);
    assert!(!value.to_string().contains("private"));
    assert!(!value.to_string().contains("download_url"));
}
#[test]
fn foreign_revoked_or_wrong_scope_connection_never_reaches_transport() {
    let profile = Profile::new();
    let (mut store, handle) = profile.connect(Provider::Google, &[GMAIL_READ_SCOPE]);
    let fixture = Fixture::new(vec![]);
    assert!(api(&mut store, &fixture)
        .gmail_messages("other.app", &handle, None)
        .is_err());
    assert!(api(&mut store, &fixture)
        .calendars("fixture.app", &handle, None)
        .is_err());
    store.disconnect("fixture.app", &handle).unwrap();
    assert!(api(&mut store, &fixture)
        .gmail_messages("fixture.app", &handle, None)
        .is_err());
    assert!(fixture.calls.lock().unwrap().is_empty());
}
#[test]
fn calendar_expired_sync_and_etag_conflict_are_explicit() {
    let profile = Profile::new();
    let (mut store, handle) = profile.connect(Provider::Google, &[CALENDAR_SCOPE]);
    let fixture = Fixture::new(vec![(410, json!({})), (412, json!({}))]);
    let mut api = api(&mut store, &fixture);
    assert_eq!(
        api.calendar_sync(
            "fixture.app",
            &handle,
            "primary",
            Some("expired"),
            Some("page2")
        )
        .unwrap(),
        json!({"reset_required":true})
    );
    let event:CalendarEvent=serde_json::from_value(json!({"summary":"Fixture","description":"","location":"","start":{"date":"2026-10-08"},"end":{"date":"2026-10-09"}})).unwrap();
    assert!(api
        .calendar_save(
            "fixture.app",
            &handle,
            "primary",
            &event,
            Some(("event1", "\"revision1\"")),
            ""
        )
        .unwrap_err()
        .contains("remote content changed"));
    let calls = fixture.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].method, "PATCH");
    assert_eq!(calls[1].if_match.as_deref(), Some("\"revision1\""));
    assert!(calls[1]
        .url
        .query_pairs()
        .any(|(k, v)| k == "sendUpdates" && v == "none"));
}
