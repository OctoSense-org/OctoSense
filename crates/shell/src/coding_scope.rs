//! The coding scope (ADR 0003, "An app that is an octos client"): what an
//! app whose entry names `kernel: "coding"` (OctosCode) may do through its
//! kernel port. The kernel router holds the app's connection to it on every
//! frame. Over the private pipe octos applies none of its external-client
//! checks and sees the shell's authority, so this is the whole boundary:
//! what is not allowed here is refused.
//!
//! - **Its own sessions only.** Every string under a key naming a session,
//!   at any depth, is one of the app's: `_main:api:code-…`, with no topic
//!   (a `#peer-…` topic would take an app agent's queued input). The id is
//!   the kernel's durable key, so the shell keeps no record and a restart
//!   loses nothing. Every profile named is `_main`; no topic, sandbox,
//!   origin, tool context or host token is set.
//! - **Its own folders.** A working folder (`cwd`) is the app's own jail, or
//!   on a desktop a folder under a root its entry's `storage.external`
//!   grants read-write (the person's home for OctosCode). octos fences a
//!   session's file tools to its working folder, so the whole tree counts:
//!   never OctoSense's own data or the kernel's, a hidden folder at the top
//!   of the home, nor a folder that holds any of them (the home itself). It
//!   is resolved here and sent resolved.
//! - **A coding agent's tools.** Before the app's first frame about a
//!   session, the router sets that session's exact kernel tool list
//!   ([`CODING_TOOLS`]): reading and editing files in the session's
//!   workspace, search, the web, asking the person and memory. No command
//!   runs: octos's own shell asks only about a few patterns, so commands
//!   wait for a shell-run tool approved per command.
//! - **What it hears.** Only frames about its own sessions, and none of the
//!   host's (`peer/…`: the shell's relay answers those). Lists answer its
//!   own sessions, and the methods it is told the kernel supports are the
//!   ones it may call.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use octosense_ai_host::kernel::Scope;
use serde_json::{json, Value};

/// The prefix of every session the app may name.
pub const OWN_SESSION_PREFIX: &str = "_main:api:code-";

/// Each coding session's exact kernel tools: octos's own list for an
/// external client's turn, none of which runs a command.
pub const CODING_TOOLS: &[&str] = octosense_ai_host::kernel::system_tools::EXTERNAL_TURN_TOOLS;

/// Methods that name no session: their answers are not another session's
/// (filtered where they could be).
const SESSIONLESS: &[&str] = &[
    "session/list",
    "config/capabilities/list",
    "profile/skills/list",
    "memory/overview",
    "memory/entity",
    "memory/search",
    "memory/load",
];

/// Methods on one of the app's own sessions (`session_id` required: without
/// one, octos answers for the whole profile).
const ON_OWN_SESSION: &[&str] = &[
    "session/open",
    "session/hydrate",
    "session/status/read",
    "session/btw",
    "session/rollback",
    "session/compact",
    "session/compact/mode/set",
    "session/delete",
    "session/fork",
    "session/goal/get",
    "session/goal/set",
    "session/goal/clear",
    "thread/graph/get",
    "turn/start",
    "turn/steer",
    "turn/interrupt",
    "turn/state/get",
    "approval/respond",
    "approval/scopes/list",
    "user_question/respond",
    "diff/preview/get",
    "snapshot/list",
    "snapshot/restore",
    "tool/status/list",
    "mcp/status/list",
    "skill/action/job/list",
    "review/start",
    "permission/profile/list",
    "profile/llm/list",
    "task/list",
    "task/output/read",
    "task/artifact/list",
    "task/artifact/read",
    "task/cancel",
    "agent/list",
    "agent/status/read",
    "agent/output/read",
    "agent/artifact/list",
    "agent/artifact/read",
    "agent/interrupt",
    "agent/close",
    "loop/create",
    "loop/list",
    "loop/delete",
    "loop/pause",
    "loop/resume",
    "loop/fire_now",
    "monitor/list",
    "monitor/pause",
    "monitor/delete",
];

/// Methods that may name a working folder. (`launch/resolve` is not one of
/// the app's: it could pick another client's session for a folder, so the
/// app opens a session of its own there instead.)
const WITH_CWD: &[&str] = &["session/open", "session/list"];

/// Keys an app may not set at any depth: a topic (folded into the session
/// key), a sandbox override, an origin, a tool context, the host's token.
const FORBIDDEN_KEYS: &[&str] = &["topic", "sandbox", "origin", "tool_context", "host_token"];

/// The scope for `app`'s kernel port.
pub fn for_app(app: &str) -> Arc<dyn Scope> {
    Arc::new(CodingScope { app: app.to_string(), roots: Roots::for_app(app) })
}

/// Whether the app may call `method` at all.
pub fn allows(method: &str) -> bool {
    SESSIONLESS.contains(&method) || ON_OWN_SESSION.contains(&method)
}

/// Whether `session` is one of the app's: `_main:api:code-<id>`, no topic.
pub fn is_own_session(session: &str) -> bool {
    session.strip_prefix(OWN_SESSION_PREFIX).is_some_and(|id| {
        !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    })
}

struct CodingScope {
    app: String,
    roots: Roots,
}

impl Scope for CodingScope {
    fn request(&self, method: &str, params: &mut Value) -> Result<(), String> {
        check(method, params, &self.roots).map_err(|why| format!("{} may not call {method}: {why}", self.app))
    }

    fn notification(&self, method: &str, session: Option<&str>) -> bool {
        !method.starts_with("peer/") && session.is_some_and(|s| is_own_session(s.split('#').next().unwrap_or(s)))
    }

    fn result(&self, method: &str, result: &mut Value) {
        if method == "session/list" {
            if let Some(sessions) = result.get_mut("sessions").and_then(Value::as_array_mut) {
                // octos names a listed session by its key, or without `_main:api:`.
                sessions.retain(|s| {
                    s.get("id").and_then(Value::as_str).is_some_and(|id| is_own_session(id) || is_own_session(&format!("_main:api:{id}")))
                });
            }
        }
        trim_supported_methods(result);
    }

    // octos sends an opened session again as a `session/open` notification
    // (`SessionOpened`) with its whole capability list: trim it as a result's.
    fn rewrite(&self, _method: &str, params: &Value) -> Option<Value> {
        names_methods(params).then(|| {
            let mut params = params.clone();
            trim_supported_methods(&mut params);
            params
        })
    }

    fn prepare(&self, session: &str) -> Option<Value> {
        is_own_session(session).then(|| {
            json!({"method": "session/tool_list/set", "params": {"session_id": session, "generic_tools": CODING_TOOLS}})
        })
    }
}

/// The checks of one request; `params` gets the resolved working folder.
fn check(method: &str, params: &mut Value, roots: &Roots) -> Result<(), String> {
    if !allows(method) {
        return Err("not a coding client's method".into());
    }
    if !params.is_object() {
        return Err("its params must be an object".into());
    }
    if let Some(key) = forbidden_key(params) {
        return Err(format!("it may not set {key}"));
    }
    let mut sessions = Vec::new();
    session_ids(params, false, &mut sessions);
    if let Some(other) = sessions.iter().find(|s| !is_own_session(s)) {
        return Err(format!("session {other} is not this app's"));
    }
    let mut profiles = Vec::new();
    profile_values(params, &mut profiles);
    if let Some(other) = profiles.iter().find(|p| *p != "_main") {
        return Err(format!("profile {other} is not this app's"));
    }
    if ON_OWN_SESSION.contains(&method) && params.get("session_id").and_then(Value::as_str).is_none() {
        return Err("it names none of its sessions".into());
    }
    if method == "session/fork" {
        let child = params.get("new_chat_id").and_then(Value::as_str).unwrap_or_default();
        if !is_own_session(&format!("_main:api:{child}")) {
            return Err(format!("a forked session's id must start with code- (got {child:?})"));
        }
    }
    if has_bad_media(params) {
        return Err("turn media must be upload handles".into());
    }
    match params.get("cwd").filter(|cwd| !cwd.is_null()) {
        Some(_) if !WITH_CWD.contains(&method) => Err("it may not set a working folder here".into()),
        Some(cwd) => {
            let cwd = cwd.as_str().ok_or("cwd must be a path")?;
            let resolved = roots.resolve(cwd)?;
            params["cwd"] = Value::String(resolved.to_string_lossy().into_owned());
            Ok(())
        }
        None => Ok(()),
    }
}

/// The first forbidden key set (non-null) at any depth, or a `cwd` below the
/// top level.
fn forbidden_key(value: &Value) -> Option<String> {
    fn walk(value: &Value, top: bool) -> Option<String> {
        match value {
            Value::Array(items) => items.iter().find_map(|item| walk(item, false)),
            Value::Object(map) => map.iter().find_map(|(key, item)| {
                let lower = key.to_ascii_lowercase();
                let forbidden = FORBIDDEN_KEYS.iter().any(|k| lower.contains(k)) || (lower == "cwd" && !top);
                if forbidden && !item.is_null() {
                    Some(key.clone())
                } else {
                    walk(item, false)
                }
            }),
            _ => None,
        }
    }
    walk(value, true)
}

/// Every string under a key containing `session`, at any depth (octos's own
/// rule for an external client).
fn session_ids(value: &Value, under: bool, out: &mut Vec<String>) {
    match value {
        Value::String(text) if under => out.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| session_ids(item, under, out)),
        Value::Object(map) => {
            for (key, item) in map {
                session_ids(item, under || key.to_ascii_lowercase().contains("session"), out);
            }
        }
        _ => {}
    }
}

/// Every string under a key containing `profile`, at any depth.
fn profile_values(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| profile_values(item, out)),
        Value::Object(map) => {
            for (key, item) in map {
                match item.as_str() {
                    Some(profile) if key.to_ascii_lowercase().contains("profile") => out.push(profile.to_owned()),
                    _ => profile_values(item, out),
                }
            }
        }
        _ => {}
    }
}

/// Turn media that are not upload handles (`up/…`, no `..`): a raw path
/// would reach the model.
fn has_bad_media(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(has_bad_media),
        Value::Object(map) => map.iter().any(|(key, item)| {
            if key == "media" {
                return item.as_array().is_some_and(|media| {
                    media.iter().any(|entry| {
                        let path = entry.get("path").and_then(Value::as_str).or(entry.as_str()).unwrap_or("");
                        !path.starts_with("up/") || path.contains("..")
                    })
                });
            }
            has_bad_media(item)
        }),
        _ => false,
    }
}

/// Whether `value` holds a `supported_methods` list anywhere.
fn names_methods(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(names_methods),
        Value::Object(map) => map.iter().any(|(key, item)| key == "supported_methods" || names_methods(item)),
        _ => false,
    }
}

/// Keep only the methods the app may call in any `supported_methods` list.
fn trim_supported_methods(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(trim_supported_methods),
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if key == "supported_methods" {
                    if let Some(methods) = item.as_array_mut() {
                        methods.retain(|m| m.as_str().is_some_and(allows));
                        continue;
                    }
                }
                trim_supported_methods(item);
            }
        }
        _ => {}
    }
}

/// The folders the app may work in.
#[derive(Debug, Default)]
struct Roots {
    /// The app's own jail.
    jail: Option<PathBuf>,
    /// Read-write roots its entry grants (desktop only).
    granted: Vec<PathBuf>,
    /// Never, even under a granted root: OctoSense's own data, the kernel's
    /// home, and the hidden folders at the top of the person's home.
    denied: Vec<PathBuf>,
    /// The person's home, whose hidden top-level folders are denied.
    home: Option<PathBuf>,
}

impl Roots {
    fn for_app(app: &str) -> Roots {
        let canonical = |p: PathBuf| std::fs::canonicalize(&p).unwrap_or(p);
        let jail = crate::app_storage::host().and_then(|s| s.layout().app(app).ok()).map(|paths| paths.jail);
        let mut roots = Roots { jail: jail.map(canonical), ..Roots::default() };
        roots.denied.push(canonical(crate::octosense::paths::home()));
        if let Some(core) = octosense_ai_host::kernel::core_dir() {
            roots.denied.push(canonical(core));
        }
        if cfg!(any(target_os = "macos", target_os = "linux", target_os = "windows")) && !cfg!(target_env = "ohos") {
            if let Some(home) = crate::sandbox::person_home() {
                let entry = crate::native_apps::find(app);
                for grant in entry.map(|a| a.external).unwrap_or_default() {
                    if let Some((path, crate::sandbox::Access::ReadWrite)) = crate::sandbox::parse_external(grant, &home) {
                        roots.granted.push(canonical(path));
                    }
                }
                roots.denied.push(canonical(home.join("Library")));
                roots.home = Some(canonical(home));
            }
        }
        roots
    }

    /// `cwd` resolved (`~` expanded, links followed), if the app may work
    /// there.
    fn resolve(&self, cwd: &str) -> Result<PathBuf, String> {
        let expanded = match (cwd.strip_prefix('~'), &self.home) {
            (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => home.join(rest.trim_start_matches('/')),
            _ => PathBuf::from(cwd),
        };
        if !expanded.is_absolute() {
            return Err(format!("the working folder {cwd:?} is not an absolute path"));
        }
        let resolved = std::fs::canonicalize(&expanded).map_err(|e| format!("the working folder {cwd:?}: {e}"))?;
        if !resolved.is_dir() {
            return Err(format!("the working folder {cwd:?} is not a folder"));
        }
        if self.jail.as_ref().is_some_and(|jail| resolved.starts_with(jail)) {
            return Ok(resolved);
        }
        if !self.granted.iter().any(|root| resolved.starts_with(root)) {
            return Err(format!("{cwd:?} is outside the folders this app may work in"));
        }
        // In, or holding, a denied folder; the home itself or above it (it
        // holds the hidden folders); a hidden folder at the top of the home.
        let touches = |denied: &PathBuf| resolved.starts_with(denied) || denied.starts_with(&resolved);
        if self.denied.iter().any(touches)
            || self.home.as_ref().is_some_and(|home| home.starts_with(&resolved))
            || self.hidden_at_top_of_home(&resolved)
        {
            return Err(format!("{cwd:?} is a folder this app may not work in"));
        }
        Ok(resolved)
    }

    /// Whether `path` is in a hidden folder at the top of the person's home
    /// (`~/.ssh`, `~/.config`, …).
    fn hidden_at_top_of_home(&self, path: &Path) -> bool {
        let Some(home) = &self.home else { return false };
        path.strip_prefix(home)
            .ok()
            .and_then(|rest| rest.components().next())
            .is_some_and(|first| first.as_os_str().to_string_lossy().starts_with('.'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("coding-scope-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::canonicalize(&dir).unwrap()
    }

    /// A person's home with a project, a hidden folder and OctoSense's own
    /// data inside it, granted as OctosCode's entry grants it.
    fn roots(home: &Path) -> Roots {
        for dir in ["code/app", ".ssh", "octosense/apps/octoscode", "elsewhere"] {
            std::fs::create_dir_all(home.join(dir)).unwrap();
        }
        Roots {
            jail: Some(home.join("octosense/apps/octoscode")),
            granted: vec![home.to_path_buf()],
            denied: vec![home.join("octosense")],
            home: Some(home.to_path_buf()),
        }
    }

    fn checked(method: &str, params: Value, roots: &Roots) -> Result<Value, String> {
        let mut params = params;
        check(method, &mut params, roots).map(|()| params)
    }

    #[test]
    fn only_its_own_sessions_without_a_topic_count_as_its_own() {
        assert!(is_own_session("_main:api:code-1f2e"));
        for other in ["_main:api:octosense#system", "_main:main", "_main:api:code-1#peer-mail", "work:api:code-1", "_main:api:code-", "code-1"] {
            assert!(!is_own_session(other), "{other}");
        }
    }

    #[test]
    fn a_request_naming_another_session_profile_or_host_parameter_is_refused() {
        let roots = Roots::default();
        let own = json!({"session_id": "_main:api:code-a"});
        assert!(checked("turn/start", own.clone(), &roots).is_ok());
        assert!(checked("turn/start", json!({"session_id": "_main:api:octosense#system"}), &roots).is_err(), "the system conversation");
        assert!(checked("turn/start", json!({"session_id": "_main:api:code-a", "target": {"parent_session": "_main:x"}}), &roots).is_err(), "at any depth");
        assert!(checked("turn/start", json!({"session_id": "_main:api:code-a", "topic": "peer-mail"}), &roots).is_err());
        assert!(checked("turn/start", json!({"session_id": "_main:api:code-a", "sandbox": {"read_allow_paths": ["/"]}}), &roots).is_err());
        assert!(checked("turn/start", json!({"session_id": "_main:api:code-a", "profile_id": "work"}), &roots).is_err());
        assert!(checked("turn/start", json!({"session_id": "_main:api:code-a", "media": [{"path": "/etc/passwd"}]}), &roots).is_err());
        assert!(checked("turn/start", json!({"session_id": "_main:api:code-a", "media": [{"path": "up/a.png"}]}), &roots).is_ok());
        assert!(checked("task/list", json!({}), &roots).is_err(), "a profile-wide list");
        for method in ["server/shutdown", "client_hello", "profile/llm/upsert", "peer/dispatch", "monitor/create", "session/tool_list/set", "permission/profile/set", "launch/resolve"] {
            assert!(checked(method, own.clone(), &roots).is_err(), "{method}");
        }
        assert!(checked("session/fork", json!({"session_id": "_main:api:code-a", "new_chat_id": "code-b"}), &roots).is_ok());
        assert!(checked("session/fork", json!({"session_id": "_main:api:code-a", "new_chat_id": "octosense#system"}), &roots).is_err());
    }

    #[test]
    fn a_working_folder_is_its_jail_or_under_a_granted_root_and_is_sent_resolved() {
        let home = scratch("roots");
        let roots = roots(&home);
        let open = |cwd: &str| checked("session/open", json!({"session_id": "_main:api:code-a", "cwd": cwd}), &roots);
        let project = open(&home.join("code/app/../app").to_string_lossy()).unwrap();
        assert_eq!(project["cwd"], home.join("code/app").to_string_lossy().as_ref(), "resolved");
        assert!(open(&home.join("octosense/apps/octoscode").to_string_lossy()).is_ok(), "its own jail");
        assert!(open(&home.join("octosense").to_string_lossy()).is_err(), "OctoSense's own data");
        assert!(open(&home.join(".ssh").to_string_lossy()).is_err(), "a hidden folder at the top of the home");
        assert!(open(&home.to_string_lossy()).is_err(), "the home holds its hidden folders");
        assert!(open(&home.join("elsewhere/..").to_string_lossy()).is_err(), "resolved first: the home again");
        assert!(open("/").is_err(), "outside every granted root");
        assert!(open("code/app").is_err(), "not absolute");
        assert!(open(&home.join("missing").to_string_lossy()).is_err());
        assert!(checked("turn/start", json!({"session_id": "_main:api:code-a", "cwd": home.join("code").to_string_lossy()}), &roots).is_err(), "only where a folder is chosen");
        let jail_only = Roots { granted: Vec::new(), ..roots };
        assert!(checked("session/open", json!({"session_id": "_main:api:code-a", "cwd": home.join("code/app").to_string_lossy()}), &jail_only).is_err(), "a phone: its jail only");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn it_hears_only_its_own_sessions_lists_only_them_and_is_told_only_what_it_may_call() {
        let scope = CodingScope { app: "octoscode".into(), roots: Roots::default() };
        assert!(scope.notification("message/delta", Some("_main:api:code-a")));
        assert!(!scope.notification("message/delta", Some("_main:api:octosense#system")));
        assert!(!scope.notification("peer/tool/call", Some("_main:api:code-a")), "the shell's relay answers those");
        assert!(!scope.notification("background/activity", None));
        let mut list = json!({"sessions": [{"id": "_main:api:code-a"}, {"id": "code-b"}, {"id": "octosense#system"}, {"id": "_main:api:peer-x"}]});
        scope.result("session/list", &mut list);
        assert_eq!(list["sessions"], json!([{"id": "_main:api:code-a"}, {"id": "code-b"}]));
        let mut caps = json!({"capabilities": {"supported_methods": ["turn/start", "server/shutdown", "peer/dispatch", "session/list"]}});
        scope.result("config/capabilities/list", &mut caps);
        assert_eq!(caps["capabilities"]["supported_methods"], json!(["turn/start", "session/list"]));
        // The opened session again, as a notification: told the same.
        let opened = json!({"session_id": "_main:api:code-a", "capabilities": {"supported_methods": ["turn/start", "profile/llm/select", "session/list"]}});
        assert_eq!(scope.rewrite("session/open", &opened).unwrap()["capabilities"]["supported_methods"], json!(["turn/start", "session/list"]));
        assert!(scope.rewrite("message/delta", &json!({"session_id": "_main:api:code-a", "text": "hi"})).is_none(), "anything else passes as it is");
        let prepared = scope.prepare("_main:api:code-a").unwrap();
        assert_eq!(prepared["method"], "session/tool_list/set");
        assert_eq!(prepared["params"]["generic_tools"], json!(CODING_TOOLS));
        assert!(!CODING_TOOLS.iter().any(|t| matches!(*t, "shell" | "bash" | "exec_command" | "monitor_create" | "spawn")), "no command runs");
        assert!(scope.prepare("_main:api:octosense#system").is_none());
    }
}
