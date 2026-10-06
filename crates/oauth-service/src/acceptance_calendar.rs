//! Synthetic Calendar transport for explicitly compiled native acceptance.
//! No OAuth, live Google traffic, agent result or physical approval is implied.
use crate::{
    acceptance_fixtures::{self, Backend},
    api::{CALENDAR_LIST_SCOPE, CALENDAR_SCOPE},
    oauth::Tokens,
    transport::{Body, Request, Response, Transport},
    Connections, CredentialStore, Provider,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub const APP: &str = "org.octosense.samples.googlecalendar";
pub const CALENDAR: &str = "synthetic-calendar";
const SUBJECT: &str = "synthetic-calendar-acceptance";
const TOKEN: &str = "synthetic-calendar-token-not-a-credential";
const KIND: &str = "synthetic-calendar-provider-v1";

#[derive(Default)]
struct MemoryVault(Mutex<BTreeMap<String, String>>);
impl CredentialStore for MemoryVault {
    fn put(&self, key: &str, value: &str) -> Result<(), String> {
        let token: Value = serde_json::from_str(value).map_err(|_| "Invalid fixture token")?;
        if token["access"] != TOKEN || !token["refresh"].is_null() {
            return Err("Calendar fixture refuses real credentials".into());
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
            .ok_or("Missing synthetic token".into())
    }
    fn remove(&self, key: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}
fn write(path: &Path, value: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| "Cannot encode Calendar fixture")?;
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&temp, bytes).map_err(|_| "Cannot write Calendar fixture")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))
            .map_err(|_| "Cannot protect Calendar fixture")?;
    }
    fs::rename(temp, path).map_err(|_| "Cannot commit Calendar fixture".into())
}
fn initial() -> Value {
    let event = |id: &str, title: &str, day: &str, hour: &str| {
        json!({
            "id":id, "etag":"\"synthetic-1\"", "status":"confirmed", "summary":title,
            "description":"Synthetic event for installed-app acceptance. No real calendar data.", "location":"Fixture room",
            "start":{"dateTime":format!("2026-10-{day}T{hour}:00:00-07:00"),"timeZone":"America/Los_Angeles"},
            "end":{"dateTime":format!("2026-10-{day}T{hour}:30:00-07:00"),"timeZone":"America/Los_Angeles"}
        })
    };
    json!({"kind":KIND,"offline":false,"list_unavailable":false,"revision":1,
        "events":[event("event00001","Synthetic planning session","08","09"),
                  event("event00002","Different synthetic event","09","15")],"calls":[]})
}
struct CalendarTransport {
    path: PathBuf,
    serial: Mutex<()>,
}
impl Transport for CalendarTransport {
    fn send(&self, request: Request) -> Result<Response, String> {
        if request.url.scheme() != "https"
            || request.url.host_str() != Some("www.googleapis.com")
            || request.bearer.as_deref() != Some(TOKEN)
        {
            return Err("Calendar fixture refuses this provider request".into());
        }
        let _guard = self.serial.lock().unwrap();
        let bytes = fs::read(&self.path).map_err(|_| "Cannot read Calendar fixture")?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Calendar fixture exceeds limit".into());
        }
        let mut state: Value =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid Calendar fixture")?;
        if state["kind"] != KIND {
            return Err("Wrong Calendar fixture marker".into());
        }
        let calls = state["calls"]
            .as_array_mut()
            .ok_or("Invalid fixture call log")?;
        if calls.len() >= 512 {
            calls.remove(0);
        }
        calls.push(
            json!({"method":request.method,"path":request.url.path(),"if_match":request.if_match,
            "query":request.url.query()}),
        );
        write(&self.path, &state)?;
        if state["offline"] == true {
            return Err(
                "Synthetic provider is offline; cached agenda should remain visible".into(),
            );
        }
        let path = request.url.path();
        let collection = format!("/calendar/v3/calendars/{CALENDAR}/events");
        let revision = state["revision"]
            .as_u64()
            .ok_or("Invalid fixture revision")?;
        let mut status = 200;
        let body = if request.method == "GET" && path == "/calendar/v3/users/me/calendarList" {
            if state["list_unavailable"] == true {
                return Err("Synthetic calendar list unavailable".into());
            }
            json!({"items":[{"id":CALENDAR,"summary":"Synthetic acceptance calendar","timeZone":"America/Los_Angeles","accessRole":"owner"}]})
        } else if request.method == "GET" && path == collection {
            json!({"items":state["events"],"timeZone":"America/Los_Angeles","nextSyncToken":format!("synthetic-sync-{revision}")})
        } else if request.method == "GET" && path.starts_with(&(collection.clone() + "/")) {
            let id = path.strip_prefix(&(collection.clone() + "/")).unwrap();
            match state["events"]
                .as_array()
                .ok_or("Invalid fixture events")?
                .iter()
                .find(|event| event["id"] == id)
            {
                Some(event) => event.clone(),
                None => {
                    status = 404;
                    json!({})
                }
            }
        } else if matches!(request.method, "POST" | "PATCH") {
            if !request
                .url
                .query_pairs()
                .any(|(key, value)| key == "sendUpdates" && value == "none")
            {
                return Err("Fixture refuses attendee invitations".into());
            }
            let Body::Json(mut draft) = request.body else {
                return Err("Fixture expects JSON event".into());
            };
            let events = state["events"]
                .as_array_mut()
                .ok_or("Invalid fixture events")?;
            let id = if request.method == "POST" {
                if path != collection {
                    return Err("Unexpected Calendar fixture collection".into());
                }
                draft["id"]
                    .as_str()
                    .ok_or("Missing stable event ID")?
                    .to_owned()
            } else {
                path.strip_prefix(&(collection.clone() + "/"))
                    .ok_or("Unexpected Calendar fixture event")?
                    .to_owned()
            };
            let old = events.iter().position(|event| event["id"] == id);
            let conflict = if request.method == "PATCH" {
                old.is_none_or(|index| {
                    request.if_match.as_deref() != events[index]["etag"].as_str()
                })
            } else {
                old.is_some()
            };
            if conflict {
                status = 412;
                json!({"error":"Synthetic stale event"})
            } else {
                draft["id"] = json!(id);
                draft["etag"] = json!(format!("\"synthetic-{}\"", revision + 1));
                draft["status"] = json!("confirmed");
                if let Some(index) = old {
                    events[index] = draft.clone()
                } else {
                    events.push(draft.clone())
                }
                state["revision"] = json!(revision + 1);
                write(&self.path, &state)?;
                draft
            }
        } else {
            return Err("Unsupported Calendar fixture request".into());
        };
        Ok(Response {
            status,
            body,
            etag: None,
        })
    }
}

/// Call only from the explicit acceptance executable, before host registration.
/// A marker and synthetic subject check prevent attaching to a real profile.
pub fn install(root: &Path, app: &str) -> Result<Value, String> {
    acceptance_fixtures::validate_root(root)?;
    if app != APP || !root.is_absolute() || !root.is_dir() {
        return Err("Calendar fixture requires its own installed sample profile".into());
    }
    let directory = root.join("acceptance-calendar");
    let state_path = directory.join("provider.json");
    if root.join("oauth/connections.json").exists() && !state_path.exists() {
        return Err("Calendar fixture refuses a pre-existing provider profile".into());
    }
    fs::create_dir_all(&directory).map_err(|_| "Cannot create Calendar fixture")?;
    fs::create_dir_all(root.join("oauth")).map_err(|_| "Cannot create fixture OAuth metadata")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Cannot protect Calendar fixture")?;
    }
    if !state_path.exists() {
        write(&state_path, &initial())?;
    }
    let clients = root.join("oauth/clients.json");
    if clients.exists()
        && fs::read_to_string(&clients)
            .map_err(|_| "Cannot inspect fixture configuration")?
            .trim()
            != "{}"
    {
        return Err("Calendar fixture refuses real provider configuration".into());
    }
    write(&clients, &json!({}))?;
    let vault = Arc::new(MemoryVault::default());
    let mut connections = Connections::open(&root.join("oauth"), vault.clone())?;
    let scopes = [CALENDAR_SCOPE, CALENDAR_LIST_SCOPE]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let existing = connections.list(APP);
    for connection in &existing {
        if connection.subject != SUBJECT
            || connection.provider != Provider::Google
            || connection.scopes != scopes
        {
            return Err("Calendar fixture refuses a non-synthetic connection".into());
        }
        vault.put(
            &connection.handle,
            &json!({"access":TOKEN,"refresh":null,"expires_at":null}).to_string(),
        )?;
    }
    let connection = if let Some(connection) = connections.active(APP) {
        connection
    } else {
        connections.connect(
            APP,
            Provider::Google,
            SUBJECT,
            "Synthetic Calendar account",
            Tokens {
                access: TOKEN.into(),
                refresh: None,
                expires_at: None,
                scopes,
            },
        )?
    };
    acceptance_fixtures::install(
        root,
        Backend {
            vault,
            transport: Arc::new(CalendarTransport {
                path: state_path.clone(),
                serial: Mutex::new(()),
            }),
        },
    )?;
    Ok(json!({"kind":KIND,"connection":connection.handle,"calendar":CALENDAR,"state":state_path}))
}
