//! Synthetic GitHub for installed native acceptance, compiled only on request.
//! It replaces provider I/O at an explicitly marked fixture profile, never the
//! app, host service, account checks, immutable review or save implementation.
use crate::{
    acceptance_fixtures::{self, Backend},
    oauth::Tokens,
    transport::{Body, Request, Response, Transport},
    Connections, CredentialStore, Provider,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const APP: &str = "org.octosense.samples.githubnotes";
const SUBJECT: &str = "synthetic-github-acceptance";
const TOKEN: &str = "synthetic-provider-credential-not-valid-on-github";
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
            .ok_or("Synthetic account was disconnected".into())
    }
    fn remove(&self, key: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}
fn digest(text: &str) -> String {
    // Opaque fixture revisions keep GitHub's 40-hex SHA shape; no real Git object.
    format!("{:x}", Sha256::digest(text.as_bytes()))[..40].to_owned()
}
fn save(path: &Path, value: &Value) -> Result<(), String> {
    let pending = path.with_extension("pending");
    fs::write(
        &pending,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(pending, path).map_err(|e| e.to_string())
}
fn read(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|_| "Cannot read synthetic provider state")?)
        .map_err(|_| "Invalid synthetic provider state".into())
}

struct Github {
    directory: PathBuf,
    lock: Mutex<()>,
}
impl Github {
    fn respond(&self, request: Request) -> Result<Response, String> {
        if request.url.scheme() != "https"
            || request.url.host_str() != Some("api.github.com")
            || request.bearer.as_deref() != Some(TOKEN)
        {
            return Err("Synthetic GitHub refused a nonfixture request".into());
        }
        let control_path = self.directory.join("github-control.json");
        let mut control = if control_path.exists() {
            read(&control_path)?
        } else {
            json!({})
        };
        if control["offline"] == true {
            return Err("Synthetic provider is offline".into());
        }
        if let Some(status) = control["http_status"].as_u64() {
            return Ok(Response {
                status: status as u16,
                body: Value::Null,
                etag: None,
            });
        }
        let state_path = self.directory.join("github-state.json");
        let mut state = read(&state_path)?;
        let path = request.url.path();
        let mut status = 200;
        let body = if request.method == "GET" && path == "/user/repos" {
            let page = request
                .url
                .query_pairs()
                .find(|(k, _)| k == "page")
                .map(|(_, v)| v.into_owned())
                .unwrap_or_default();
            if page == "1" {
                json!([
                    {"name":"notes","full_name":"fixture-author/notes","owner":{"login":"fixture-author"},"default_branch":"main","private":false},
                    {"name":"empty","full_name":"fixture-author/empty","owner":{"login":"fixture-author"},"default_branch":"main","private":false}
                ])
            } else {
                json!([])
            }
        } else if request.method == "GET" && path == "/repos/fixture-author/empty/contents" {
            json!([])
        } else if let Some(file) = path.strip_prefix("/repos/fixture-author/notes/contents") {
            let file = file.trim_start_matches('/');
            if request.method == "GET" {
                if file.is_empty() {
                    json!([
                        {"name":"README.md","path":"README.md","type":"file","sha":state["files"]["README.md"]["sha"]},
                        {"name":"docs","path":"docs","type":"dir","sha":"fixture-directory"},
                        {"name":"picture.png","path":"picture.png","type":"file","sha":"fixture-image"}
                    ])
                } else if file == "docs" {
                    json!([
                        {"name":"second-note.md","path":"docs/second-note.md","type":"file","sha":state["files"]["docs/second-note.md"]["sha"]}
                    ])
                } else if let Some(entry) = state["files"].get(file) {
                    json!({"type":"file","encoding":"base64","path":file,"sha":entry["sha"],
                        "content":STANDARD.encode(entry["content"].as_str().ok_or("Invalid synthetic document")?.as_bytes())})
                } else {
                    status = 404;
                    Value::Null
                }
            } else if request.method == "PUT" {
                let Body::Json(change) = request.body else {
                    return Err("Synthetic save expected JSON".into());
                };
                if change["branch"] != "main" {
                    return Err("Synthetic provider only models branch main".into());
                }
                let next = control["next_save"].as_str().unwrap_or("").to_string();
                if !next.is_empty() {
                    control.as_object_mut().unwrap().remove("next_save");
                    save(&control_path, &control)?;
                }
                if next == "conflict" {
                    let content = "# Concurrent fixture update\n\nAnother synthetic editor changed this file.\n";
                    state["files"][file] = json!({"content":content,"sha":digest(content)});
                    save(&state_path, &state)?;
                }
                let current = state["files"].get(file);
                if current.is_some_and(|entry| change["sha"] != entry["sha"])
                    || (current.is_none() && !change["sha"].is_null())
                {
                    status = 409;
                    Value::Null
                } else {
                    let content = String::from_utf8(
                        STANDARD
                            .decode(
                                change["content"]
                                    .as_str()
                                    .ok_or("Missing synthetic save content")?,
                            )
                            .map_err(|_| "Invalid synthetic save encoding")?,
                    )
                    .map_err(|_| "Synthetic note is not UTF-8")?;
                    let revision = state["revision"].as_u64().unwrap_or(0) + 1;
                    let sha = digest(&content);
                    let commit = digest(&format!("synthetic-commit-{revision}-{sha}"));
                    state["revision"] = json!(revision);
                    state["files"][file] = json!({"content":content,"sha":sha});
                    state["last_commit"] = json!({"path":file,"message":change["message"],"sha":commit,"content_sha":sha});
                    save(&state_path, &state)?;
                    if next == "uncertain" {
                        return Err("Synthetic connection failed after storing the commit; inspect provider state before retrying".into());
                    }
                    status = 201;
                    json!({"content":{"sha":sha},"commit":{"sha":commit,"html_url":Value::Null}})
                }
            } else {
                status = 405;
                Value::Null
            }
        } else {
            status = 404;
            Value::Null
        };
        Ok(Response {
            status,
            body,
            etag: None,
        })
    }
}
impl Transport for Github {
    fn send(&self, request: Request) -> Result<Response, String> {
        let _guard = self.lock.lock().unwrap();
        let method = request.method;
        let path = request.url.path().to_string();
        let result = self.respond(request);
        let line = json!({"method":method,"path":path,"status":result.as_ref().ok().map(|r|r.status),"failed":result.is_err()});
        let mut journal = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.directory.join("github-requests.jsonl"))
            .map_err(|_| "Cannot record synthetic request")?;
        writeln!(journal, "{line}").map_err(|_| "Cannot record synthetic request")?;
        result
    }
}

/// Seed a clearly fictional account and provider only inside a signed fixture.
pub fn install(root: &Path, app: &str) -> Result<Value, String> {
    acceptance_fixtures::validate_root(root)?;
    let parent = root.parent().ok_or("Missing isolated apps root")?;
    if app != APP
        || root.file_name().and_then(|n| n.to_str()) != Some(".host")
        || read(&parent.join(".connected-e2e.json"))?["fixture"] != "connected-e2e"
    {
        return Err("Synthetic GitHub requires a marked installed Notes acceptance profile".into());
    }
    fs::create_dir_all(root.join("oauth")).map_err(|e| e.to_string())?;
    let clients = root.join("oauth/clients.json");
    if clients.exists() && read(&clients)? != json!({}) {
        return Err("Refusing to replace real OAuth provider registrations".into());
    }
    let metadata = root.join("oauth/connections.json");
    if metadata.exists()
        && read(&metadata)?["entries"]
            .as_object()
            .is_none_or(|entries| {
                entries
                    .values()
                    .any(|c| c["app_id"] != APP || c["subject"] != SUBJECT)
            })
    {
        return Err("Refusing a profile containing nonfixture account metadata".into());
    }
    let vault = Arc::new(Vault::default());
    let mut connections = Connections::open(&root.join("oauth"), vault.clone())?;
    if connections
        .list(APP)
        .iter()
        .any(|c| c.subject != SUBJECT || c.provider != Provider::Github)
    {
        return Err("Refusing to reuse real GitHub account metadata".into());
    }
    let scopes: BTreeSet<String> = ["read:user", "public_repo"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let tokens = Tokens::from_response(
        &json!({"access_token":TOKEN,"token_type":"bearer"}),
        &scopes,
        0,
    )?;
    let connection = if let Some(c) = connections.active(APP) {
        connections.replace_tokens(APP, &c.handle, tokens)?;
        c
    } else {
        connections.connect(
            APP,
            Provider::Github,
            SUBJECT,
            "Fixture GitHub · synthetic provider",
            tokens,
        )?
    };
    save(&clients, &json!({}))?;
    let directory = root.join("fixtures");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let state = directory.join("github-state.json");
    if !state.exists() {
        let first="# Fixture notebook\n\nThis repository is synthetic. No live GitHub account was used.\n";
        let second="# Delivery notes\n\nOriginal appointment: Tuesday at 09:00.\n\n- Bring the printed checklist\n- Ask about parking\n";
        save(
            &state,
            &json!({"fixture":"github","revision":0,"files":{
                "README.md":{"content":first,"sha":digest(first)},
                "docs/second-note.md":{"content":second,"sha":digest(second)}
            }}),
        )?;
    } else if read(&state)?["fixture"] != "github" {
        return Err("Refusing nonfixture provider state".into());
    }
    acceptance_fixtures::install(
        root,
        Backend {
            vault,
            transport: Arc::new(Github {
                directory,
                lock: Mutex::new(()),
            }),
        },
    )?;
    Ok(json!({"fixture":"github","connection":connection.handle,"live_provider":false}))
}
