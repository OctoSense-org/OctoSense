//! Synthetic Gmail transport for the explicitly compiled native acceptance app.
//! These messages are fictional. No OAuth login or real mail delivery is proved.
use crate::{
    acceptance_fixtures::{self, Backend},
    api::{GMAIL_READ_SCOPE, GMAIL_SEND_SCOPE},
    oauth::Tokens,
    transport::{Request, Response, Transport},
    Connection, Connections, CredentialStore, Provider,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const ACCESS: &str = "fictional-inbox-acceptance-credential";
pub const CLINIC: &str = "fixtureclinic20261006";
pub const NEWSLETTER: &str = "fixturenewsletter20261006";

#[derive(Default)]
struct MemoryVault(Mutex<BTreeMap<String, String>>);
impl CredentialStore for MemoryVault {
    fn put(&self, key: &str, value: &str) -> Result<(), String> {
        let token: Value =
            serde_json::from_str(value).map_err(|_| "Invalid synthetic credential")?;
        if token["access"] != ACCESS || !token["refresh"].is_null() {
            return Err("Synthetic Gmail refuses real credentials".into());
        }
        self.0.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }
    fn get(&self, key: &str) -> Result<String, String> {
        self.0
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or("Synthetic credential missing".into())
    }
    fn remove(&self, key: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}
#[derive(Default, Serialize, Deserialize)]
struct State {
    synthetic_gmail: bool,
    released: bool,
    #[serde(default)]
    requests: BTreeMap<String, usize>,
    #[serde(default)]
    denied_send_attempts: usize,
}
struct Gmail {
    path: PathBuf,
    state: Mutex<State>,
}
impl Gmail {
    fn save(&self, state: &State) -> Result<(), String> {
        crate::inbox::persist(&self.path, state)
    }
}
impl Transport for Gmail {
    fn send(&self, request: Request) -> Result<Response, String> {
        if request.bearer.as_deref() != Some(ACCESS)
            || request.url.host_str() != Some("gmail.googleapis.com")
        {
            return Err("Synthetic Gmail refused this credential or provider".into());
        }
        let mut state = self.state.lock().unwrap();
        let path = request.url.path().trim_end_matches('/');
        if request.method != "GET" {
            state.denied_send_attempts += 1;
            self.save(&state)?;
            return Err("Synthetic acceptance never sends real mail".into());
        }
        let category = if path.ends_with("/profile") {
            "profile"
        } else if path.ends_with("/history") {
            "history"
        } else if path.ends_with("/messages") {
            "messages"
        } else {
            "message"
        };
        *state.requests.entry(category.into()).or_default() += 1;
        let history = if state.released { "102" } else { "100" };
        let response = if path.ends_with("/profile") {
            json!({"emailAddress":"person@example.test","historyId":history,"messagesTotal":if state.released{2}else{0}})
        } else if path.ends_with("/history") {
            let before = request
                .url
                .query_pairs()
                .find(|(k, _)| k == "startHistoryId")
                .map(|(_, v)| v.into_owned())
                .unwrap_or_default();
            if state.released && before == "100" {
                json!({"historyId":"102","history":[{"id":"101","messagesAdded":[{"message":{"id":CLINIC,"labelIds":["INBOX","IMPORTANT"]}},{"message":{"id":NEWSLETTER,"labelIds":["INBOX"]}}]}]})
            } else {
                json!({"historyId":history,"history":[]})
            }
        } else if path.ends_with("/messages") {
            let label = request
                .url
                .query_pairs()
                .find(|(k, _)| k == "labelIds")
                .map(|(_, v)| v.into_owned())
                .unwrap_or_else(|| "INBOX".into());
            let mut items = vec![];
            if state.released && label != "SENT" {
                items.push(json!({"id":CLINIC,"threadId":"fixture-clinic-thread"}));
                if label != "IMPORTANT" {
                    items.push(json!({"id":NEWSLETTER,"threadId":"fixture-newsletter-thread"}));
                }
            }
            json!({"messages":items,"resultSizeEstimate":items.len()})
        } else if path.ends_with("/labels") {
            json!({"labels":[{"id":"INBOX","name":"Inbox"},{"id":"IMPORTANT","name":"Important"},{"id":"SENT","name":"Sent"}]})
        } else if state.released && path.ends_with(CLINIC) {
            message(CLINIC)
        } else if state.released && path.ends_with(NEWSLETTER) {
            message(NEWSLETTER)
        } else {
            self.save(&state)?;
            return Ok(Response {
                status: 404,
                body: json!({"error":{"code":404}}),
                etag: None,
            });
        };
        self.save(&state)?;
        Ok(Response {
            status: 200,
            body: response,
            etag: None,
        })
    }
}
fn message(id: &str) -> Value {
    let (from, subject, body, thread, labels) = if id == CLINIC {
        ("Lakeview Clinic <appointments@example.test>","Please confirm your follow-up appointment","Hello, your follow-up appointment is Tuesday, October 13, 2026 at 9:00 AM America/Los_Angeles. Please reply to confirm, or tell us another time that morning. Bring your medication list. Thank you, Lakeview Clinic.","fixture-clinic-thread",vec!["INBOX","IMPORTANT"])
    } else {
        ("Store Newsletter <offers@example.test>","This week's store offers","Our general newsletter lists this week's furniture offers. Browse whenever you like. This is a promotional mailing with no order, delivery, appointment, family or work action for you.","fixture-newsletter-thread",vec!["INBOX"])
    };
    json!({"id":id,"threadId":thread,"labelIds":labels,"snippet":body,"payload":{"mimeType":"text/plain","headers":[{"name":"From","value":from},{"name":"To","value":"person@example.test"},{"name":"Subject","value":subject},{"name":"Message-ID","value":format!("<{id}@example.test>")}],"body":{"data":URL_SAFE_NO_PAD.encode(body)}}})
}
pub struct Fixture {
    pub connection: Connection,
    provider: Arc<Gmail>,
}
impl Fixture {
    pub fn release_new_mail(&self) -> Result<(), String> {
        let mut state = self.provider.state.lock().unwrap();
        state.released = true;
        self.provider.save(&state)
    }
    pub fn receipt(&self) -> Value {
        serde_json::to_value(&*self.provider.state.lock().unwrap()).unwrap()
    }
}
/// Only a dedicated acceptance executable calls this before host startup.
/// Reject non-fixture state so this cannot replace an existing real profile.
pub fn install(root: &Path, app: &str) -> Result<Fixture, String> {
    std::fs::create_dir_all(root).map_err(|_| "Cannot create isolated Inbox fixture")?;
    acceptance_fixtures::validate_root(root)?;
    let path = root.join("acceptance-inbox.json");
    let state = if path.exists() {
        let state: State =
            serde_json::from_slice(&std::fs::read(&path).map_err(|_| "Cannot read Inbox fixture")?)
                .map_err(|_| "Invalid Inbox fixture")?;
        if !state.synthetic_gmail {
            return Err("Not a synthetic Gmail profile".into());
        }
        state
    } else {
        if root.join("oauth").exists() {
            return Err("Refusing to replace an existing OAuth profile".into());
        }
        State {
            synthetic_gmail: true,
            ..State::default()
        }
    };
    let vault = Arc::new(MemoryVault::default());
    let provider = Arc::new(Gmail {
        path,
        state: Mutex::new(state),
    });
    let oauth = root.join("oauth");
    std::fs::create_dir_all(&oauth).map_err(|_| "Cannot create synthetic OAuth directory")?;
    crate::inbox::persist(&oauth.join("clients.json"), &json!({}))?;
    let mut store = Connections::open(&oauth, vault.clone())?;
    let scopes = [
        "openid",
        "email",
        "profile",
        GMAIL_READ_SCOPE,
        GMAIL_SEND_SCOPE,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let connection = if let Some(connection) = store.active(app) {
        if connection.subject != "person@example.test" || connection.provider != Provider::Google {
            return Err("Unexpected fixture account identity".into());
        }
        vault.put(
            &connection.handle,
            &json!({"access":ACCESS,"refresh":null,"expires_at":null}).to_string(),
        )?;
        connection
    } else {
        store.connect(
            app,
            Provider::Google,
            "person@example.test",
            "Synthetic Gmail <person@example.test>",
            Tokens::from_response(
                &json!({"access_token":ACCESS,"token_type":"Bearer"}),
                &scopes,
                0,
            )?,
        )?
    };
    provider.save(&provider.state.lock().unwrap())?;
    acceptance_fixtures::install(
        root,
        Backend {
            vault,
            transport: provider.clone(),
        },
    )?;
    Ok(Fixture {
        connection,
        provider,
    })
}
