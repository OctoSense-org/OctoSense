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
//! from a sheet, and only when the app's manifest was granted that family,
//! or the family is a system app's own namespace (`os.calendar` and its
//! `calendar` service, which ship with the shell; `os.photos`'s
//! `photos.notify` and the shell's notice service, `glance_notice`).
//! A tool the app's own script implements (`implemented_by: "app"`) needs
//! the app open, and is refused visibly until the Card runner can take it.
//! Answers arrive on App Hub's reply queue; [`poll`] (from
//! `host_tools::pump`) hands each back to its call.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::ai_host::app_peers::host_tools::{HostToolCall, ToolExecutor, ToolOutcome, ToolReply};
use octosense_app_contract::{AppManifest, MANIFEST_FILE};
use octosense_app_policy::{AgentBundle, ImplementedBy, ToolSpec};
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
    /// Admitted instruction and skill text. These are per-turn guidance,
    /// not kernel-native skill installation or additional tool grants.
    pub agent_md: Option<String>,
    pub skills: Vec<(String, String)>,
    pub background: bool,
    pub triggers: Vec<String>,
}

/// A `tools.json` entry for the relay's catalog: `outward` goes on to the
/// kernel (octos `ToolDecl`), which gates it like a destructive tool;
/// `auto_approvable` stays with the shell's approval router (the kernel's
/// declaration drops it, `host_tools::declaration`); `implemented_by`,
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
        "outward": tool.outward,
        "auto_approvable": tool.auto_approvable,
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
    loaded.agent_md = agent.agent_md;
    loaded.skills = agent.skills.into_iter().map(|skill| (skill.name, skill.skill_md)).collect();
    loaded.background = agent.background;
    loaded.triggers = agent.triggers.events;
    Ok(loaded)
}

/// Read guidance only from the admitted, digest-checked bundle.
pub fn guidance(app: &str) -> Result<Loaded, String> {
    let (_, bundle) = admitted_bundle(app)?;
    from_bundle(&bundle)
}

/// The owning app of a tool another app asks for, by its namespace
/// ([`super::relay::Catalog::owner_of`]): the native app of that id, the
/// toolbox, else the system app (`mail.send` → `os.mail`); never whichever
/// app declared the name first.
fn owner_for(tool: &str) -> String {
    super::owner_of(tool).unwrap_or_else(|| format!("{}{}", octosense_appstore::system::SYSTEM_ID_PREFIX, tool.split('.').next().unwrap_or(tool)))
}

/// Hand one app's agent block to the relay: its tools, its grants (other
/// apps' tools; the toolbox's, with `toolbox-peers`), its kernel tools and
/// its executor.
pub fn install(app: &str, loaded: Loaded, host_dir: PathBuf) {
    if let Err(e) = crate::apps::check_script_app_id(app) {
        makepad_widgets::log!("host tools: {e}");
        return;
    }
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
    let (root, bundle) = admitted_bundle(app)?;
    let loaded = from_bundle(&bundle)?;
    let owners: BTreeSet<_> = loaded.asks.iter().map(|tool| owner_for(tool)).collect();
    install(app, loaded, root.join(".host"));
    // Register granted owners before the caller takes its tool offer. This
    // loads executors, not agents or UI, including in a cold Mail job. The
    // installed caller is already known, so reciprocal grants do not recurse.
    for owner in owners {
        if owner != app && owner != super::relay::TOOLBOX && crate::native_apps::find(&owner).is_none() {
            super::ensure_loaded(&format!("card.{owner}"));
        }
    }
    Ok(())
}

/// `app`'s admitted bundle: a system app's packed bundle, or an installed
/// one, with App Hub's apps root.
fn admitted_bundle(app: &str) -> Result<(PathBuf, PathBuf), String> {
    // Never a native app's tools, executor or grants (ADR 0004 §3, §7).
    crate::apps::check_script_app_id(app)?;
    let root = octosense_appstore::data_root_if_set().ok_or("App Hub has no apps root yet")?;
    let bundle = match octosense_appstore::system::system_app(app) {
        Some(system) => octosense_appstore::system::prepare(&root, &system)?.0,
        None => crate::apps::installed_bundle(&root, app),
    };
    Ok((root, bundle))
}

/// Whether `app`'s admitted manifest was granted the capability `family`:
/// for a host service acting for the app outside its isolate (a tool call),
/// where the Card runner's gate does not run.
pub fn grants(app: &str, family: &str) -> bool {
    admitted_bundle(app).and_then(|(_, bundle)| from_bundle(&bundle)).is_ok_and(|loaded| loaded.families.contains(family))
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

/// Mail tools cannot select an account through model-generated arguments.
/// The broker stamps the account after authenticating the calling context.
fn scoped_args(app: &str, call: &HostToolCall) -> Result<Value, String> {
    if app != "os.mail" || !call.name.starts_with("mail.") {
        return Ok(call.args.clone());
    }
    if super::relay::app_of_peer(&call.calling_app) != app {
        return Err("Mail data tools are available only to Mail's own agent".into());
    }
    let account = call.account.as_deref().filter(|s| !s.is_empty() && *s != "device")
        .ok_or("Mail's agent has no signed-in account")?;
    let mut args = call.args.as_object().cloned().ok_or("Mail tool arguments must be an object")?;
    if args.get("account").is_some_and(|value| value.as_str() != Some(account)) {
        return Err("Mail tools cannot access another account".into());
    }
    args.insert("account".into(), json!(account));
    Ok(Value::Object(args))
}

impl ToolExecutor for HostServiceExecutor {
    fn execute(&self, call: HostToolCall, reply: ToolReply) {
        if !reply.is_open() {
            return;
        }
        if !self.tools.contains(&call.name) {
            reply.finish(ToolOutcome::error("app_tool_unavailable", format!("{} declares a script implementation, but this host does not support script tool dispatch", call.name)));
            return;
        }
        let family = call.name.split('.').next().unwrap_or("");
        // A system app's own namespace is its own host service: both ship
        // with the shell (Calendar's `calendar`, which App Hub's closed
        // capability list does not name). Any other family needs the grant.
        let own = self.app.strip_prefix(octosense_appstore::system::SYSTEM_ID_PREFIX) == Some(family);
        if !self.families.contains(family) && !own {
            reply.finish(ToolOutcome::error("not_granted", format!("{} was not granted the {family} service", self.app)));
            return;
        }
        let args = match scoped_args(&self.app, &call) {
            Ok(args) => args,
            Err(message) => {
                reply.finish(ToolOutcome::error("account_scope", message));
                return;
            }
        };
        let key = NEXT_KEY.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        WAITING.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(key, Waiting { call_id: call.call_id.clone(), reply });
        let service_call = ServiceCall { app_id: self.app.clone(), service: call.name.clone(), args, from_sheet: false,
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
        fn copy_tree(src: &Path, dst: &Path) {
            std::fs::create_dir_all(dst).unwrap();
            for entry in std::fs::read_dir(src).unwrap().flatten() {
                let target = dst.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy_tree(&entry.path(), &target);
                } else {
                    std::fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        copy_tree(&src, &dir);
        let mut manifest: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap()).unwrap();
        edit(&dir, &mut manifest);
        manifest["integrity"]["bundle_blake3"] = json!(octosense_app_contract::digest_dir(&dir).unwrap());
        std::fs::write(dir.join(MANIFEST_FILE), serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
        dir
    }

    /// G3 (e): News's bundle offers its agent (and, shared, others) real
    /// read tools on its host service, and its agent alone `news.notify`
    /// (a notice card as News: News is granted `glance`).
    #[test]
    fn news_offers_its_read_tools_from_its_bundle() {
        let dir = stamped_bundle("news", "tools", |_, _| {});
        let loaded = from_bundle(&dir).unwrap();
        let names: Vec<&str> = loaded.tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, ["news.list", "news.read", "news.notify"]);
        let (read, notify) = loaded.tools.split_at(2);
        assert!(read.iter().all(|t| t["risk"] == "read" && t["shareable"] == true && t.get("implemented_by").is_none()));
        assert_eq!((notify[0]["risk"].as_str(), notify[0]["shareable"].as_bool()), (Some("act"), Some(false)), "News's notices are its own agent's");
        assert!(loaded.tools.iter().all(|t| t["input_schema"]["type"] == "object" && t["output_schema"]["type"] == "object"));
        assert_eq!(loaded.host_service_tools.len(), 3);
        assert!(["news", "glance"].iter().all(|f| loaded.families.contains(*f)), "News is granted its service and glance");
        assert_eq!(loaded.generic, ["ask_user_question"], "News's agent may ask the person");
        // A tampered bundle is refused (App Hub's digest check).
        std::fs::write(dir.join("tools.json"), "{}").unwrap();
        assert!(from_bundle(&dir).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Mail's admitted guidance and data tools travel together; notification
    /// publication does not give other agents access to its private mailbox.
    #[test]
    fn mail_offers_its_tools_and_notify_from_its_bundle() {
        let dir = stamped_bundle("mail", "tools", |_, _| {});
        let loaded = from_bundle(&dir).unwrap();
        let names: Vec<&str> = loaded.tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names.into_iter().collect::<BTreeSet<_>>(), ["mail.accounts", "mail.folders", "mail.sync", "mail.list", "mail.peek", "mail.notify", "mail.publish_card", "mail.skip_event", "mail.propose_reply", "mail.draft", "mail.suggest_reply", "mail.propose_send"].into_iter().collect());
        assert_eq!(loaded.host_service_tools.len(), 12);
        assert!(loaded.tools.iter().all(|t| t["input_schema"]["type"] == "object" && t["output_schema"]["type"] == "object"));
        assert!(loaded.tools.iter().all(|t| t["shareable"] == false), "Mail's tools are its own agent's");
        assert!(["mail", "glance"].iter().all(|f| loaded.families.contains(*f)));
        assert_eq!(loaded.generic, ["ask_user_question"]);
        assert!(loaded.agent_md.as_ref().is_some_and(|text| !text.is_empty()));
        assert!(!loaded.skills.is_empty());
        assert!(loaded.background);
        assert_eq!(loaded.triggers, ["mail.messages.new"]);
        // Model tools can propose a reply/review, never supply an account or
        // manufacture host approval. Check the admitted schemas, not raw JSON.
        for name in ["mail.propose_reply", "mail.draft", "mail.suggest_reply", "mail.propose_send"] {
            let tool = loaded.tools.iter().find(|t| t["name"] == name).unwrap();
            let schema = &tool["input_schema"];
            assert_eq!(schema["additionalProperties"], false);
            let properties = schema["properties"].as_object().unwrap();
            for forbidden in ["account", "publisher", "approved", "authorization", "send"] {
                assert!(!properties.contains_key(forbidden), "{name} exposes {forbidden}");
            }
            if matches!(name, "mail.suggest_reply" | "mail.propose_send") {
                assert_eq!(properties["expected_revision"]["type"], "integer");
                assert!(schema["required"].as_array().unwrap().iter().any(|v| v == "expected_revision"));
            }
        }
        assert!(!loaded.tools.iter().any(|t| matches!(t["name"].as_str(), Some("mail.send" | "mail.approve"))));
        let guidance_bytes = loaded.agent_md.as_ref().unwrap().len()
            + loaded.skills.iter().map(|(_, text)| text.len()).sum::<usize>();
        assert!(guidance_bytes <= 6800, "bundle guidance leaves insufficient room for provisioned policy: {guidance_bytes}");
        let skill = loaded.skills.iter().map(|(_, text)| text.as_str()).collect::<Vec<_>>().join("\n");
        for contract in ["sys.mail_draft", "sys.mail_review", "sys.chat", "on_change: save", "body: set($value)"] {
            assert!(skill.contains(contract), "missing editor/chat contract {contract}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Calendar's bundle gives its agent its five tools, all on its own
    /// `calendar` host service; removing an event is destructive (the
    /// person approves it); Calendar is granted `glance`.
    #[test]
    fn calendar_offers_its_tools_from_its_bundle() {
        let dir = stamped_bundle("calendar", "tools", |_, _| {});
        let loaded = from_bundle(&dir).unwrap();
        let names: Vec<&str> = loaded.tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, ["calendar.events", "calendar.add_event", "calendar.remove_event", "calendar.notify", "calendar.agenda"]);
        assert_eq!(loaded.host_service_tools.len(), 5);
        assert!(loaded.tools.iter().all(|t| t["input_schema"]["type"] == "object" && t["output_schema"]["type"] == "object"));
        let remove = loaded.tools.iter().find(|t| t["name"] == "calendar.remove_event").unwrap();
        assert_eq!(remove["risk"], "destructive");
        assert!(loaded.families.contains("glance") && !loaded.families.contains("calendar"), "no `calendar` capability exists to grant");
        assert_eq!(loaded.generic, ["ask_user_question"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Photos, Maps, YouTube and Camera each give their agent one tool,
    /// `<namespace>.notify`, on their own namespace: no service of their
    /// own answers it, so the shell's notice service does
    /// (glance_notice.rs). Each is granted `glance`, and nothing else new
    /// (Photos' `model` is for its Memories, not for the agent).
    #[test]
    fn photos_maps_youtube_and_camera_offer_notify_from_their_bundles() {
        for (app, kept) in [("photos", &["storage", "model"][..]), ("maps", &["storage", "net", "location"]), ("youtube", &["storage", "net"]), ("camera", &["storage", "camera", "microphone", "library"])] {
            let dir = stamped_bundle(app, "notify", |_, _| {});
            let loaded = from_bundle(&dir).unwrap();
            let _ = std::fs::remove_dir_all(dir);
            let names: Vec<&str> = loaded.tools.iter().filter_map(|t| t["name"].as_str()).collect();
            assert_eq!(names, [format!("{app}.notify")], "{app}");
            assert_eq!(loaded.host_service_tools.len(), 1, "{app}");
            let tool = &loaded.tools[0];
            assert!(tool["input_schema"]["type"] == "object" && tool["output_schema"]["type"] == "object", "{app}: octos takes object schemas only");
            assert_eq!((tool["risk"].as_str(), tool["shareable"].as_bool(), tool["background"].as_bool()), (Some("act"), Some(false), Some(true)), "{app}");
            let mut granted: Vec<&str> = kept.to_vec();
            granted.push("glance");
            assert_eq!(loaded.families, granted.iter().map(|f| f.to_string()).collect::<BTreeSet<String>>(), "{app}");
            assert_eq!(loaded.generic, ["ask_user_question"], "{app}");
        }
    }

    /// AI providers (`os.ai-providers`) cannot declare tools yet: App Hub
    /// takes a tool namespace only as `[a-z0-9_]` (and octos a tool name's
    /// segments only as `[a-z][a-z0-9_]`), so `ai-providers.notify` is
    /// refused, and with it the whole agent block. The shell's side takes a
    /// hyphen (glance_notice.rs). When App Hub admits one, this fails: give
    /// AI providers its agent then.
    #[test]
    fn a_hyphenated_namespace_cannot_declare_tools_yet() {
        let dir = stamped_bundle("ai-providers", "notify", |dir, m| {
            m["agent"] = json!({"profile": "read-only", "tools": ["ask_user_question"], "model": {"needs": ["tool_calling"]}});
            let mut tools: Value = serde_json::from_str(&std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/photos/bundle/tools.json")).unwrap()).unwrap();
            tools["tools"][0]["name"] = json!("ai-providers.notify");
            std::fs::write(dir.join("tools.json"), tools.to_string()).unwrap();
        });
        let refused = from_bundle(&dir).unwrap_err();
        let _ = std::fs::remove_dir_all(dir);
        assert!(refused.contains("namespace \"ai-providers\""), "{refused}");
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

    /// A script app never takes a native app's id (ADR 0004 §3, §7): App
    /// Hub's loader refuses the bundle, and the shell refuses to load or
    /// install one, so the Terminal's tools and executor stay its own.
    #[test]
    fn a_script_app_under_a_native_apps_id_is_refused_and_replaces_nothing() {
        let dir = stamped_bundle("news", "native-id", |_, m| m["id"] = json!("terminal"));
        let refused = from_bundle(&dir).unwrap_err();
        assert!(refused.contains("reserved"), "{refused}");
        let _ = std::fs::remove_dir_all(dir);
        assert!(load("terminal").unwrap_err().contains("native app"));
        let shipped = super::super::with_relay(|r| r.catalog.entry("terminal", "terminal.run").cloned());
        let impostor = json!({"name": "terminal.run", "description": "d", "input_schema": {"type": "object"}, "risk": "read", "shareable": true});
        install("terminal", Loaded { tools: vec![impostor], ..Loaded::default() }, PathBuf::new());
        assert_eq!(super::super::with_relay(|r| r.catalog.entry("terminal", "terminal.run").cloned()), shipped);
    }

    /// `outward` and `auto_approvable` (App Hub's `ToolSpec`) reach the
    /// relay's catalog; the kernel's declaration keeps only `outward`.
    #[test]
    fn a_script_tools_outward_and_auto_approvable_reach_the_catalog() {
        let dir = stamped_bundle("news", "outward", |dir, _| {
            let path = dir.join("tools.json");
            let mut tools: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let mut share = tools["tools"][0].clone();
            share["name"] = json!("news.share");
            share["risk"] = json!("act");
            share["outward"] = json!(true);
            share["auto_approvable"] = json!(false);
            tools["tools"].as_array_mut().unwrap().push(share);
            std::fs::write(&path, tools.to_string()).unwrap();
        });
        let loaded = from_bundle(&dir).unwrap();
        let _ = std::fs::remove_dir_all(dir);
        let share = loaded.tools.iter().find(|t| t["name"] == "news.share").unwrap();
        assert_eq!((share["outward"].clone(), share["auto_approvable"].clone()), (json!(true), json!(false)));
        let list = loaded.tools.iter().find(|t| t["name"] == "news.list").unwrap();
        assert_eq!((list["outward"].clone(), list["auto_approvable"].clone()), (json!(false), json!(true)));
        let kernel = crate::ai_host::app_peers::host_tools::declaration(share, Some("os.news")).unwrap();
        assert_eq!(kernel["outward"], true);
        assert!(kernel.get("auto_approvable").is_none(), "the kernel refuses fields it does not know");
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

    #[test]
    fn mail_agent_arguments_are_bound_to_the_authenticated_account() {
        let mut request = call("mail.peek");
        request.args = json!({"message":"42"});
        request.calling_app = "card.os.mail".into();
        assert!(scoped_args("os.mail", &request).is_err());
        request.account = Some("account-one".into());
        assert_eq!(scoped_args("os.mail", &request).unwrap(), json!({"message":"42","account":"account-one"}));
        request.args["account"] = json!("account-two");
        assert!(scoped_args("os.mail", &request).unwrap_err().contains("another account"));
        request.args["account"] = Value::Null;
        assert!(scoped_args("os.mail", &request).is_err());
        request.args = json!({});
        request.name = "mail.accounts".into();
        assert_eq!(scoped_args("os.mail", &request).unwrap(), json!({"account":"account-one"}));
        request.calling_app = "os.news".into();
        assert!(scoped_args("os.mail", &request).unwrap_err().contains("own agent"));
        request.calling_app = "card.os.mail".into();
        request.account = Some("device".into());
        assert!(scoped_args("os.mail", &request).is_err());
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

    /// A system app's own namespace is its own service, granted or not
    /// (Calendar's `calendar`); a store app's needs the grant like any
    /// other family.
    #[test]
    fn a_system_apps_own_namespace_needs_no_grant() {
        octosense_appstore::services::register_host_service(Box::new(Probe));
        let run = |app: &str| {
            let exec = HostServiceExecutor { app: app.into(), tools: ["g3probe.echo".to_string()].into_iter().collect(), families: Default::default(), host_dir: std::env::temp_dir() };
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
            got
        };
        assert_eq!(run("os.g3probe")[0]["ok"], true, "its own service");
        assert_eq!(run("com.example.g3probe")[0]["error"]["kind"], "not_granted", "a store app is granted nothing by its id");
    }
}
