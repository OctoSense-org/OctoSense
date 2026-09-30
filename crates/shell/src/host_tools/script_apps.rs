//! Script apps' tools (ADR 0004 §4, §7, §12; G3): what a store or system
//! app offers its agent and others, from its admitted bundle, and the
//! executor that runs them on its host services.
//!
//! **Where the tools come from.** App Hub's bundle carries `tools.json` next
//! to `manifest.json`; [`load`] reads it with App Hub's own loader
//! (`octosense_app_policy::AgentBundle::load`: the bundle's digest must
//! match its manifest, and every rule the store's gate applies holds), from
//! the unpacked system app (`octosense_appstore::system::prepare`) or the
//! installed bundle (`<apps root>/<id>/bundle`). The manifest's
//! `agent.tools` names both kinds of grant: a plain name is an octos kernel
//! tool its agent keeps (`generic_tools`: only App Hub's `KERNEL_TOOLS`,
//! `ask_user_question`, which App Hub alone admits), a dotted one
//! (`mail.send`) another app's shareable tool, granted at install and
//! marked with its owner.
//!
//! **Where the calls go** ([`HostServiceExecutor`]). A tool the bundle says
//! is `implemented_by: "host-service"` runs on the host service of its
//! namespace (`news.list` → the `news` service), exactly as the app's own
//! `host.request("news.list", …)` would: with the app's identity, never
//! from a sheet, and only when the app's manifest was granted that family.
//! A tool the app's own script implements (`implemented_by: "app"`) needs
//! the app open, and is refused visibly until the Card runner can take it.
//! Answers arrive on App Hub's reply queue; [`poll`] (from
//! `host_tools::pump`) hands each back to its call.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::ai_host::app_peers::host_tools::{HostToolCall, ToolExecutor, ToolOutcome, ToolReply};
use octosense_app_policy::{AgentBundle, AppManifest, ImplementedBy, ToolSpec, MANIFEST_FILE};
use octosense_appstore::services::{ServiceCall, ServiceHost};

/// What one script app's bundle gives the host.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Loaded {
    /// Its own tools, as the kernel takes them (`tools.json` entries).
    pub tools: Vec<Value>,
    /// Exactly the octos kernel tools its agent keeps.
    pub generic: Vec<String>,
    /// Other apps' tools its agent asks for (owner resolved at grant time).
    pub asks: Vec<String>,
    /// Its tools that run on host services, and the families it was granted.
    pub host_service_tools: BTreeSet<String>,
    pub families: BTreeSet<String>,
    /// The admitted manifest (the toolbox reads its grant from it).
    pub manifest: Value,
}

/// A `tools.json` entry for the kernel (octos `ToolDecl`): `implemented_by`,
/// `private_data` are the shell's, not the kernel's.
fn declaration(tool: &ToolSpec) -> Value {
    let mut out = json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.input_schema,
        "output_schema": tool.output_schema,
        "risk": format!("{:?}", tool.risk).to_lowercase(),
        "background": tool.background,
        "shareable": tool.shareable,
    });
    if tool.confirmed_by_app() {
        out["confirm"] = json!("app");
    }
    out
}

/// Read an unpacked bundle's agent block (digest and every gate rule
/// checked by App Hub's loader).
pub fn from_bundle(bundle: &Path) -> Result<Loaded, String> {
    let text = std::fs::read_to_string(bundle.join(MANIFEST_FILE)).map_err(|e| format!("{}: {e}", bundle.display()))?;
    let manifest = AppManifest::parse(&text)?;
    let agent = AgentBundle::load(bundle, &manifest)?;
    let families: BTreeSet<String> = manifest.capabilities.iter().cloned().collect();
    let raw: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let Some(agent) = agent else { return Ok(Loaded { families, manifest: raw, ..Loaded::default() }) };
    let mut loaded = Loaded { families, manifest: raw, ..Loaded::default() };
    for tool in &agent.tools {
        loaded.tools.push(declaration(tool));
        if tool.implemented_by == ImplementedBy::HostService {
            loaded.host_service_tools.insert(tool.name.clone());
        }
    }
    // Dotted names are other apps' tools. Of the kernel's own, App Hub
    // lets a contained agent keep only `KERNEL_TOOLS` (`ask_user_question`)
    // and refuses the rest at admission; `kernel_tools` is exactly those.
    loaded.asks = agent.generic_tools.iter().filter(|name| name.contains('.')).cloned().collect();
    loaded.generic = agent.kernel_tools().into_iter().filter(|name| !super::relay::OCTOS_SHELL.contains(&name.as_str())).collect();
    Ok(loaded)
}

/// The owning app of a tool another app asks for: whoever declares it, else
/// the system app of its namespace (`mail.send` → `os.mail`).
fn owner_for(tool: &str) -> String {
    super::owner_of(tool).unwrap_or_else(|| format!("{}{}", octosense_appstore::system::SYSTEM_ID_PREFIX, tool.split('.').next().unwrap_or(tool)))
}

/// Hand one app's agent block to the relay: its tools, its grants (other
/// apps' tools; the toolbox's, with `toolbox-peers`), its kernel tools and
/// its executor.
pub fn install(app: &str, loaded: Loaded, host_dir: PathBuf) {
    super::declare(app, loaded.tools.clone());
    for tool in &loaded.asks {
        let owner = owner_for(tool);
        if owner != app {
            super::grant(app, &owner, tool);
        }
    }
    super::set_generic(app, loaded.generic.clone());
    // Its toolbox grant, from the same manifest (ADR 0002 §6, #151).
    #[cfg(feature = "toolbox-peers")]
    super::toolbox::grant_manifest(app, &loaded.manifest);
    let executor = HostServiceExecutor { app: app.to_string(), tools: loaded.host_service_tools, families: loaded.families, host_dir };
    super::set_executor(app, Some(Arc::new(executor)));
}

/// Load `app`'s agent block from App Hub: a system app's packed bundle, or
/// an installed one.
pub fn load(app: &str) -> Result<(), String> {
    let root = octosense_appstore::data_root_if_set().ok_or("App Hub has no apps root yet")?;
    let bundle = match octosense_appstore::system::system_app(app) {
        Some(system) => octosense_appstore::system::prepare(&root, &system)?.0,
        None => root.join(app).join("bundle"),
    };
    let loaded = from_bundle(&bundle)?;
    install(app, loaded, root.join(".host"));
    Ok(())
}

// ------------------------------------------------------------ the executor

/// Runs a script app's host-service tools as the app's own
/// `host.request` would, and answers each call once.
pub struct HostServiceExecutor {
    pub app: String,
    /// The tools that run on a host service.
    pub tools: BTreeSet<String>,
    /// The capability families the app's manifest was granted.
    pub families: BTreeSet<String>,
    /// The directory App Hub hands every host service (`<apps root>/.host`).
    pub host_dir: PathBuf,
}

/// Where a call's answer goes: App Hub's reply queue, keyed by a heap key no
/// isolate uses.
struct Waiting {
    call_id: String,
    reply: ToolReply,
}

static WAITING: Mutex<Option<HashMap<usize, Waiting>>> = Mutex::new(None);
/// Heap keys for tool calls: far above any isolate's.
static NEXT_KEY: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(usize::MAX / 2);

/// A tool call never raises a sheet: the person is not in the app.
struct NoSheet;
impl ServiceHost for NoSheet {
    fn open_sheet(&mut self, _body: String) {}
    fn close_sheet(&mut self) {}
}

impl ToolExecutor for HostServiceExecutor {
    fn execute(&self, call: HostToolCall, reply: ToolReply) {
        if !reply.is_open() {
            return;
        }
        if !self.tools.contains(&call.name) {
            reply.finish(ToolOutcome::error("app_tool_unavailable", format!("{} runs in the app's own script; open the app to use it", call.name)));
            return;
        }
        let family = call.name.split('.').next().unwrap_or("");
        if !self.families.contains(family) {
            reply.finish(ToolOutcome::error("not_granted", format!("{} was not granted the {family} service", self.app)));
            return;
        }
        let key = NEXT_KEY.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        WAITING.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(key, Waiting { call_id: call.call_id.clone(), reply });
        let service_call = ServiceCall { app_id: self.app.clone(), service: call.name.clone(), args: call.args.clone(), from_sheet: false,
            // A tool call has no surface for a sheet: the person is not in the app.
            may_prompt: false, host_dir: self.host_dir.clone() };
        octosense_appstore::services::dispatch(service_call, key, 0, &mut NoSheet);
    }

    fn cancel(&self, call_id: &str) {
        // Stop waiting now, and have App Hub drop the request: the service's
        // late answer goes nowhere, and the request no longer counts against
        // the calls that may wait.
        let keys: Vec<usize> = {
            let mut waiting = WAITING.lock().unwrap_or_else(|e| e.into_inner());
            let Some(waiting) = waiting.as_mut() else { return };
            let keys: Vec<usize> = waiting.iter().filter(|(_, w)| w.call_id == call_id).map(|(key, _)| *key).collect();
            for key in &keys {
                waiting.remove(key);
            }
            keys
        };
        for key in keys {
            octosense_appstore::services::cancel_heap(key);
        }
    }
}

/// Deliver the host services' answers to the calls waiting on them.
pub fn poll() {
    let keys: Vec<usize> = match WAITING.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        Some(w) if !w.is_empty() => w.keys().copied().collect(),
        _ => return,
    };
    for (key, _, result) in octosense_appstore::services::take_replies_for(&keys) {
        let Some(waiting) = WAITING.lock().unwrap_or_else(|e| e.into_inner()).as_mut().and_then(|w| w.remove(&key)) else { continue };
        waiting.reply.finish(match result {
            Ok(text) => ToolOutcome::Ok(serde_json::from_str(&text).unwrap_or(Value::String(text))),
            Err(message) => ToolOutcome::error("app_error", message),
        });
    }
    // A call its service never answers times out in App Hub, and the
    // timeout arrives here like any answer.
}

/// How many calls wait on a host service.
pub fn waiting() -> usize {
    WAITING.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map_or(0, HashMap::len)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ai_host::app_peers::host_tools::HostToolCall;
    use octosense_appstore::services::{HostService, Replier};

    /// A copy of `apps/<name>/bundle` stamped as App Hub packs it (the
    /// manifest carries the bundle's digest), with `edit` applied first.
    pub(crate) fn stamped_bundle(name: &str, tag: &str, edit: impl FnOnce(&Path, &mut Value)) -> PathBuf {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps").join(name).join("bundle");
        let dir = std::env::temp_dir().join(format!("octosense-bundle-{name}-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for entry in std::fs::read_dir(&src).unwrap().flatten() {
            if entry.file_type().unwrap().is_file() {
                std::fs::copy(entry.path(), dir.join(entry.file_name())).unwrap();
            }
        }
        let mut manifest: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap()).unwrap();
        edit(&dir, &mut manifest);
        manifest["integrity"]["bundle_blake3"] = json!(octosense_app_policy::digest_dir(&dir).unwrap());
        std::fs::write(dir.join(MANIFEST_FILE), serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
        dir
    }

    /// G3 (e): News's bundle offers its agent (and, shared, others) real
    /// read tools on its host service.
    #[test]
    fn news_offers_its_read_tools_from_its_bundle() {
        let dir = stamped_bundle("news", "tools", |_, _| {});
        let loaded = from_bundle(&dir).unwrap();
        let names: Vec<&str> = loaded.tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, ["news.list", "news.read"]);
        assert!(loaded.tools.iter().all(|t| t["risk"] == "read" && t["shareable"] == true && t.get("implemented_by").is_none()));
        assert_eq!(loaded.host_service_tools.len(), 2);
        assert!(loaded.families.contains("news"), "News is granted its service");
        assert_eq!(loaded.generic, ["ask_user_question"], "News's agent may ask the person");
        // A tampered bundle is refused (App Hub's digest check).
        std::fs::write(dir.join("tools.json"), "{}").unwrap();
        assert!(from_bundle(&dir).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The manifest's `agent.tools`: a plain name is a kernel tool, and App
    /// Hub admits only `ask_user_question` (any other, `web_search` or
    /// octos's shell, refuses the bundle); dotted ones are other apps'
    /// tools, granted with their owner.
    #[test]
    fn a_script_apps_agent_block_splits_kernel_tools_from_other_apps_tools() {
        let dir = stamped_bundle("news", "agent", |_, m| {
            m["agent"] = json!({"profile": "read-only", "tools": ["ask_user_question", "mail.send"], "model": {"needs": ["tool_calling"]}});
        });
        let loaded = from_bundle(&dir).unwrap();
        assert_eq!(loaded.generic, ["ask_user_question"]);
        assert_eq!(loaded.asks, ["mail.send"]);
        assert_eq!(owner_for("mail.send"), "os.mail", "the system app of its namespace until someone declares it");
        let _ = std::fs::remove_dir_all(dir);
        for tool in ["web_search", "shell", "read_file"] {
            let dir = stamped_bundle("news", tool, |_, m| {
                m["agent"] = json!({"profile": "read-only", "tools": ["ask_user_question", tool], "model": {"needs": ["tool_calling"]}});
            });
            let refused = from_bundle(&dir).unwrap_err();
            assert!(refused.contains("may keep only ask_user_question"), "{tool}: {refused}");
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    struct Probe;
    impl HostService for Probe {
        fn family(&self) -> &'static str {
            "g3probe"
        }
        fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
            reply.send(Ok(json!({"app": call.app_id, "service": call.service, "args": call.args, "from_sheet": call.from_sheet})));
        }
    }

    fn call(name: &str) -> HostToolCall {
        HostToolCall::parse(&json!({"peer": "p", "session_id": "s", "turn_id": "t", "call_id": format!("c-{name}"), "name": name, "args": {"q": 1}})).unwrap()
    }

    fn reply() -> (ToolReply, Arc<Mutex<Vec<Value>>>) {
        let sent: Arc<Mutex<Vec<Value>>> = Arc::default();
        let s = sent.clone();
        (ToolReply::new("c", move |v| s.lock().unwrap().push(v)), sent)
    }

    struct Holds(Arc<Mutex<Option<Replier>>>);
    impl HostService for Holds {
        fn family(&self) -> &'static str {
            "g3hold"
        }
        fn call(&mut self, _call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
            *self.0.lock().unwrap() = Some(reply);
        }
    }

    fn is_waiting(call_id: &str) -> bool {
        WAITING.lock().unwrap().as_ref().is_some_and(|w| w.values().any(|waiting| waiting.call_id == call_id))
    }

    /// A cancelled tool call stops waiting at once, and its service's late
    /// answer goes nowhere.
    #[test]
    fn a_cancelled_tool_call_stops_waiting() {
        let held = Arc::new(Mutex::new(None));
        octosense_appstore::services::register_host_service(Box::new(Holds(held.clone())));
        let exec = HostServiceExecutor {
            app: "os.g3hold".into(),
            tools: ["g3hold.wait".to_string()].into_iter().collect(),
            families: ["g3hold".to_string()].into_iter().collect(),
            host_dir: std::env::temp_dir(),
        };
        let (r, sent) = reply();
        exec.execute(call("g3hold.wait"), r);
        assert!(is_waiting("c-g3hold.wait"));
        exec.cancel("c-g3hold.wait");
        assert!(!is_waiting("c-g3hold.wait"), "cancelled");
        held.lock().unwrap().take().expect("the service held it").send(Ok(json!({})));
        poll();
        assert!(sent.lock().unwrap().is_empty(), "the late answer reached nobody");
    }

    /// The executor runs a host-service tool as the app's own
    /// `host.request` would (its identity, never a sheet), only for a
    /// granted family; the answer reaches the call through `poll`.
    #[test]
    fn a_host_service_tool_runs_as_the_apps_own_request() {
        octosense_appstore::services::register_host_service(Box::new(Probe));
        let exec = HostServiceExecutor {
            app: "os.g3probe".into(),
            tools: ["g3probe.echo".to_string(), "other.x".to_string()].into_iter().collect(),
            families: ["g3probe".to_string()].into_iter().collect(),
            host_dir: std::env::temp_dir(),
        };
        let (r, sent) = reply();
        exec.execute(call("g3probe.echo"), r);
        for _ in 0..50 {
            poll();
            if !sent.lock().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let got = sent.lock().unwrap().clone();
        assert_eq!(got[0]["ok"], true, "{got:?}");
        assert_eq!(got[0]["data"], json!({"app": "os.g3probe", "service": "g3probe.echo", "args": {"q": 1}, "from_sheet": false}));
        let (r, sent) = reply();
        exec.execute(call("other.x"), r);
        assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted", "a family the manifest was not granted");
        let (r, sent) = reply();
        exec.execute(call("g3probe.in_script"), r);
        assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "app_tool_unavailable");
    }
}
