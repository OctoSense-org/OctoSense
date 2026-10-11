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
fn missing_google_registration_refuses_refresh_without_network_or_developer_instructions() {
    let profile = Profile::new();
    let (mut store, handle) = profile.connect(Provider::Google, &[CALENDAR_LIST_SCOPE]);
    store
        .replace_tokens(
            "fixture.app",
            &handle,
            Tokens {
                access: "fixture-expired-token".into(),
                refresh: Some("fixture-refresh-token".into()),
                expires_at: Some(0),
                scopes: [CALENDAR_LIST_SCOPE.into()].into_iter().collect(),
            },
        )
        .unwrap();
    let transport = Fixture::new(vec![]);
    let error = api(&mut store, &transport)
        .calendars("fixture.app", &handle, None)
        .unwrap_err();
    assert_eq!(error, Provider::Google.sign_in_unavailable());
    assert!(!error.contains("clients.json"));
    assert!(transport.calls.lock().unwrap().is_empty());
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
fn calendar_window_query_is_bounded_and_provider_and_etag_errors_are_explicit() {
    let profile = Profile::new();
    let (mut store, handle) = profile.connect(Provider::Google, &[CALENDAR_SCOPE]);
    let fixture = Fixture::new(vec![(410, json!({})), (412, json!({}))]);
    let mut api = api(&mut store, &fixture);
    let window = crate::calendar_cache::AgendaWindow::around(calendar_now()).unwrap();
    assert!(api
        .calendar_window("fixture.app", &handle, "primary", &window, Some("page2"))
        .is_err());
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
    let query = calls[0]
        .url
        .query_pairs()
        .into_owned()
        .collect::<BTreeMap<_, _>>();
    assert_eq!(query.get("singleEvents").map(String::as_str), Some("true"));
    assert_eq!(
        query.get("timeMin").map(String::as_str),
        Some(window.time_min())
    );
    assert_eq!(
        query.get("timeMax").map(String::as_str),
        Some(window.time_max())
    );
    assert_eq!(query.get("pageToken").map(String::as_str), Some("page2"));
    assert!(!query.contains_key("syncToken"));
    assert_eq!(calls[1].method, "PATCH");
    assert_eq!(calls[1].if_match.as_deref(), Some("\"revision1\""));
    assert!(calls[1]
        .url
        .query_pairs()
        .any(|(k, v)| k == "sendUpdates" && v == "none"));
}

/// Stateful provider-boundary acceptance, not Google/host-approval/UI evidence.
/// Production connection ownership, event encoding, conditional writes and
/// durable incremental cache all run; only the HTTPS transport is replaced.
#[test]
fn calendar_provider_boundary_create_reopen_edit_conflict_and_restart() {
    use crate::calendar_cache;
    const APP: &str = "org.octosense.samples.googlecalendar";
    const CALENDAR: &str = "synthetic-calendar";
    #[derive(Default)]
    struct Remote {
        event: Option<Value>,
        revision: u64,
        offline: bool,
        calls: Vec<(&'static str, String, Option<String>)>,
    }
    #[derive(Default)]
    struct CalendarProvider(Mutex<Remote>);
    impl Transport for CalendarProvider {
        fn send(&self, request: Request) -> Result<Response, String> {
            let mut remote = self.0.lock().unwrap();
            assert_eq!(request.url.host_str(), Some("www.googleapis.com"));
            assert_eq!(request.bearer.as_deref(), Some("synthetic-calendar-token"));
            let path = request.url.path().to_owned();
            remote
                .calls
                .push((request.method, path.clone(), request.if_match.clone()));
            if remote.offline {
                return Err("Synthetic offline provider".into());
            }
            let collection = format!("/calendar/v3/calendars/{CALENDAR}/events");
            let mut status = 200;
            let body = match (request.method, path.as_str()) {
                ("GET", "/calendar/v3/users/me/calendarList") => json!({"items":[{
                    "id":CALENDAR, "summary":"Synthetic acceptance calendar",
                    "accessRole":"owner", "timeZone":"America/Los_Angeles"
                }]}),
                ("GET", path) if path == collection => json!({
                    "items":remote.event.iter().collect::<Vec<_>>(),
                    "timeZone":"America/Los_Angeles",
                    "nextSyncToken":format!("synthetic-sync-{}",remote.revision)
                }),
                ("GET", path) if path.starts_with(&(collection.clone() + "/")) => {
                    let event = remote.event.as_ref().expect("Create before reading");
                    assert_eq!(
                        path,
                        format!("{collection}/{}", event["id"].as_str().unwrap())
                    );
                    event.clone()
                }
                ("POST" | "PATCH", _) => {
                    assert!(request
                        .url
                        .query_pairs()
                        .any(|(k, v)| k == "sendUpdates" && v == "none"));
                    let Body::Json(mut draft) = request.body else {
                        panic!("Calendar writes use JSON")
                    };
                    let conflict = if request.method == "PATCH" {
                        let event = remote.event.as_ref().expect("Create before editing");
                        assert_eq!(
                            path,
                            format!("{collection}/{}", event["id"].as_str().unwrap())
                        );
                        draft["id"] = event["id"].clone();
                        request.if_match.as_deref() != event["etag"].as_str()
                    } else {
                        assert_eq!(path, collection);
                        assert!(request.if_match.is_none());
                        remote.event.is_some()
                    };
                    if conflict {
                        status = 412;
                        json!({"error":"Synthetic stale revision, never expose raw body"})
                    } else {
                        remote.revision += 1;
                        draft["etag"] =
                            json!(format!("\"synthetic-revision-{}\"", remote.revision));
                        draft["status"] = json!("confirmed");
                        remote.event = Some(draft.clone());
                        draft
                    }
                }
                _ => panic!("Unexpected Calendar fixture request"),
            };
            Ok(Response {
                status,
                body,
                etag: None,
            })
        }
    }
    let profile = Profile::new();
    let vault = Arc::new(Vault::default());
    let mut store = Connections::open(&profile.0, vault.clone()).unwrap();
    let connection = store
        .connect(
            APP,
            Provider::Google,
            "synthetic-user",
            "Synthetic Calendar account",
            Tokens {
                access: "synthetic-calendar-token".into(),
                refresh: None,
                expires_at: None,
                scopes: [CALENDAR_SCOPE, CALENDAR_LIST_SCOPE]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            },
        )
        .unwrap();
    let provider = CalendarProvider::default();
    let prepared = calendar_cache::prepare_draft(json!({
        "summary":"Synthetic appointment", "description":"Review this exact local draft", "location":"Test room",
        "start_date":"2026-10-08", "start_time":"09:00", "end_date":"2026-10-08", "end_time":"09:30",
        "timezone":"America/Los_Angeles", "all_day":false
    })).unwrap();
    let draft: CalendarEvent = serde_json::from_value(prepared.clone()).unwrap();
    let (saved, first_snapshot) = {
        let mut api = Api {
            connections: &mut store,
            transport: &provider,
            google_client: None,
            google_client_secret: None,
            now: 1,
        };
        let calendars = api.calendars(APP, &connection.handle, None).unwrap();
        assert_eq!(calendars["items"][0]["id"], CALENDAR);
        let saved = api
            .calendar_save(
                APP,
                &connection.handle,
                CALENDAR,
                &draft,
                None,
                "abcde12345",
            )
            .unwrap();
        assert_eq!(saved["start"], prepared["start"]);
        assert_eq!(saved["start"]["dateTime"], "2026-10-08T09:00:00-07:00");
        assert_eq!(
            api.calendar_get(APP, &connection.handle, CALENDAR, "abcde12345")
                .unwrap(),
            saved
        );
        let snapshot = calendar_cache::refresh(
            &profile.0,
            APP,
            &connection.handle,
            CALENDAR,
            calendar_now(),
            |window, page| api.calendar_window(APP, &connection.handle, CALENDAR, window, page),
        )
        .unwrap();
        (saved, snapshot)
    };
    let original_card = first_snapshot["events"][0]["card_id"].clone();
    assert_eq!(first_snapshot["events"][0]["start_time"], "09:00");
    assert_eq!(
        first_snapshot["events"][0]["timezone"],
        "America/Los_Angeles"
    );
    drop(store);
    let mut store = Connections::open(&profile.0, vault).unwrap();
    assert_eq!(store.active(APP).unwrap().handle, connection.handle);
    store
        .authorized(APP, &connection.handle, Provider::Google, CALENDAR_SCOPE)
        .unwrap();
    assert_eq!(
        calendar_cache::cached(&profile.0, APP, &connection.handle, CALENDAR).unwrap(),
        first_snapshot
    );
    let mut api = Api {
        connections: &mut store,
        transport: &provider,
        google_client: None,
        google_client_secret: None,
        now: 2,
    };
    let mut edited = draft.clone();
    edited.summary = "Synthetic appointment — edited".into();
    let saved_edit = api
        .calendar_save(
            APP,
            &connection.handle,
            CALENDAR,
            &edited,
            Some(("abcde12345", saved["etag"].as_str().unwrap())),
            "",
        )
        .unwrap();
    assert_eq!(saved_edit["summary"], edited.summary);
    let latest = calendar_cache::refresh(
        &profile.0,
        APP,
        &connection.handle,
        CALENDAR,
        calendar_now() + 1,
        |window, page| api.calendar_window(APP, &connection.handle, CALENDAR, window, page),
    )
    .unwrap();
    assert_eq!(
        latest["events"][0]["card_id"], original_card,
        "event routes survive edits"
    );
    assert_ne!(
        latest["events"][0]["etag"],
        first_snapshot["events"][0]["etag"]
    );
    let before = provider.0.lock().unwrap().calls.len();
    let conflict = api
        .calendar_save(
            APP,
            &connection.handle,
            CALENDAR,
            &draft,
            Some(("abcde12345", saved["etag"].as_str().unwrap())),
            "",
        )
        .unwrap_err();
    assert!(conflict.contains("remote content changed"));
    assert!(!conflict.contains("raw body"));
    assert_eq!(
        provider.0.lock().unwrap().calls.len(),
        before + 1,
        "stale writes never retry"
    );
    assert_eq!(
        api.calendar_get(APP, &connection.handle, CALENDAR, "abcde12345")
            .unwrap(),
        saved_edit
    );
    provider.0.lock().unwrap().offline = true;
    assert!(calendar_cache::refresh(
        &profile.0,
        APP,
        &connection.handle,
        CALENDAR,
        calendar_now() + 2,
        |window, page| api.calendar_window(APP, &connection.handle, CALENDAR, window, page)
    )
    .is_err());
    assert_eq!(
        calendar_cache::cached(&profile.0, APP, &connection.handle, CALENDAR).unwrap(),
        latest,
        "offline refresh preserves the committed event and agenda window"
    );
    assert!(
        calendar_cache::cached(&profile.0, "other.app", &connection.handle, CALENDAR).unwrap()
            ["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

fn calendar_now() -> u64 {
    chrono::DateTime::parse_from_rfc3339("2026-10-06T12:00:00Z")
        .unwrap()
        .timestamp() as u64
}

#[test]
fn refresh_commit_preserves_another_apps_new_connection() {
    let profile = Profile::new();
    let vault = Arc::new(Vault::default());
    let mut first = Connections::open(&profile.0, vault.clone()).unwrap();
    let account = first
        .connect(
            "fixture.app",
            Provider::Google,
            "synthetic-a",
            "A",
            Tokens {
                access: "synthetic-expired".into(),
                refresh: Some("synthetic-refresh".into()),
                expires_at: Some(0),
                scopes: [CALENDAR_LIST_SCOPE.into()].into_iter().collect(),
            },
        )
        .unwrap();
    let mut other = Connections::open(&profile.0, vault.clone()).unwrap();
    let second = other
        .connect(
            "other.app",
            Provider::Github,
            "synthetic-b",
            "B",
            Tokens {
                access: "synthetic-other".into(),
                refresh: None,
                expires_at: None,
                scopes: ["public_repo".into()].into_iter().collect(),
            },
        )
        .unwrap();
    let transport = Fixture::new(vec![
        (
            200,
            json!({"access_token":"synthetic-refreshed","token_type":"Bearer","expires_in":3600,"scope":CALENDAR_LIST_SCOPE}),
        ),
        (200, json!({"items":[]})),
    ]);
    let client = crate::providers::ClientRegistration {
        client_id: "synthetic-client".into(),
    };
    Api {
        connections: &mut first,
        transport: &transport,
        google_client: Some(&client),
        google_client_secret: None,
        now: 100,
    }
    .calendars("fixture.app", &account.handle, None)
    .unwrap();
    let current = Connections::open(&profile.0, vault).unwrap();
    assert_eq!(current.active("other.app").unwrap().handle, second.handle);
    assert!(current.active("fixture.app").unwrap().expires_at.unwrap() > 100);
}

#[test]
fn refresh_response_cannot_restore_a_revoked_connection() {
    let profile = Profile::new();
    let vault = Arc::new(Vault::default());
    let mut store = Connections::open(&profile.0, vault.clone()).unwrap();
    let account = store
        .connect(
            "fixture.app",
            Provider::Google,
            "synthetic-a",
            "A",
            Tokens {
                access: "synthetic-expired".into(),
                refresh: Some("synthetic-refresh".into()),
                expires_at: Some(0),
                scopes: [CALENDAR_LIST_SCOPE.into()].into_iter().collect(),
            },
        )
        .unwrap();
    struct RevokeDuringRefresh {
        root: std::path::PathBuf,
        vault: Arc<Vault>,
        handle: String,
        calls: std::sync::atomic::AtomicUsize,
    }
    impl Transport for RevokeDuringRefresh {
        fn send(&self, _: Request) -> Result<Response, String> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut current = Connections::open(&self.root, self.vault.clone())?;
            current.disconnect("fixture.app", &self.handle)?;
            Ok(Response {
                status: 200,
                etag: None,
                body: json!({"access_token":"must-not-be-saved","token_type":"Bearer","expires_in":3600,"scope":CALENDAR_LIST_SCOPE}),
            })
        }
    }
    let transport = RevokeDuringRefresh {
        root: profile.0.clone(),
        vault: vault.clone(),
        handle: account.handle.clone(),
        calls: Default::default(),
    };
    let client = crate::providers::ClientRegistration {
        client_id: "synthetic-client".into(),
    };
    assert!(Api {
        connections: &mut store,
        transport: &transport,
        google_client: Some(&client),
        google_client_secret: None,
        now: 100
    }
    .calendars("fixture.app", &account.handle, None)
    .is_err());
    assert_eq!(
        transport.calls.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "no resource call after revocation"
    );
    assert!(Connections::open(&profile.0, vault.clone())
        .unwrap()
        .list("fixture.app")
        .is_empty());
    assert!(vault.get(&account.handle).is_err());
}
