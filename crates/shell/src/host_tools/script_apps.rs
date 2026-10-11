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
//! from a sheet. Manifest families describe usage and do not gate calls.
//! The relay checks tool ownership and sharing; each host service retains
//! its identity, account, consent and review requirements.
//! A declared tool whose engine this build leaves out never runs: Photos'
//! `photos.info` on Home, where the photo engine is desktop only (ADR 0013)
//! and the shell's notice service answers Photos' `notify`, is refused as
//! `unavailable`, plainly and before any folder is made ([`unlinked_engine`]).
//! A tool the app's own script implements (`implemented_by: "app"`) runs on
//! its admitted full-app runner's live UI isolate through App Hub's script
//! tool queue. Closed apps fail visibly; Glance never becomes a second owner.
//! Answers arrive on App Hub's reply queue; [`poll`] (from
//! `host_tools::pump`) hands each back to its call.
//!
//! **An engine's method works in a folder the host picks** (ADR 0013): for
//! a method of an engine's family (`areas::needs_area`), the executor
//! resolves the call's area from its stamped identity and the tool's owner
//! (a craft engine's tool in the calling agent's own folder, an app's own
//! tool in that app's agent folder), grants it until the call is answered,
//! and hands the service its root as the call's `host_dir`
//! ([`super::areas`]).

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
    /// Its tools that run on host services, and the families it declares.
    pub host_service_tools: BTreeSet<String>,
    pub script_tools: BTreeSet<String>,
    pub host_methods: HashMap<String, String>,
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
/// `private_data` are the shell's, not the kernel's. The engines' virtual
/// owners declare theirs the same way (`engines.rs`).
pub(crate) fn declaration(tool: &ToolSpec) -> Value {
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
            loaded.host_methods.insert(tool.name.clone(), tool.service_method().to_owned());
        } else {
            loaded.script_tools.insert(tool.name.clone());
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

/// A foreground app or its peer can select a reviewed UI asset from its own
/// installed bundle. The bundle digest is checked again before reading it;
/// model arguments cannot become an arbitrary host filesystem path.
pub(crate) fn glance_template(app: &str, name: &str) -> Result<String, String> {
    if !name.ends_with(".splash") || name.len() > 96 || name.starts_with('.')
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')) {
        return Err("Choose a Splash template basename from this app's admitted bundle".into());
    }
    let (_, bundle) = admitted_bundle(app)?;
    from_bundle(&bundle)?;
    let path = bundle.join(name);
    if !std::fs::symlink_metadata(&path).is_ok_and(|m|m.is_file() && m.len() <= 256 * 1024) {
        return Err("The admitted Glance template is missing or too large".into());
    }
    std::fs::read_to_string(path).map_err(|_|"Cannot read admitted Glance template".into())
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
    let executor = HostServiceExecutor { app: app.to_string(), tools: loaded.host_service_tools, methods: loaded.host_methods, families: loaded.families, host_dir };
    super::set_executor(app, Some(Arc::new(ScriptAppExecutor { host: executor, tools: loaded.script_tools })));
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
pub(crate) fn admitted_bundle(app: &str) -> Result<(PathBuf, PathBuf), String> {
    // Never a native app's tools, executor or grants (ADR 0004 §3, §7).
    crate::apps::check_script_app_id(app)?;
    let root = octosense_appstore::data_root_if_set().ok_or("App Hub has no apps root yet")?;
    let bundle = match octosense_appstore::system::system_app(app) {
        Some(system) => octosense_appstore::system::prepare(&root, &system)?.0,
        None => super::admission::installed_bundle(&root, app)?,
    };
    Ok((root, bundle))
}

/// Whether an admitted manifest declares a family, for disclosure only.
/// Execution must check identity, consent and account scope independently.
pub fn grants(app: &str, family: &str) -> bool {
    admitted_bundle(app).and_then(|(_, bundle)| from_bundle(&bundle)).is_ok_and(|loaded| loaded.families.contains(family))
}

/// The app still has a verified, admitted bundle. Capability declarations
/// describe its intended use; they do not grant access to a host service.
pub fn admitted(app: &str) -> bool {
    guidance(app).is_ok()
}

/// Bind a host request to the current admitted app and host profile before
/// inspecting its account, opening a device, or reading private host state.
pub fn admitted_host(app: &str, host_dir: &Path) -> Result<Loaded, String> {
    let (root, bundle) = admitted_bundle(app)?;
    if root.join(".host") != host_dir {
        return Err("Host request belongs to another app profile".into());
    }
    let loaded = from_bundle(&bundle)?;
    if loaded.manifest["id"].as_str() != Some(app) {
        return Err("Host request belongs to another app identity".into());
    }
    Ok(loaded)
}

// ------------------------------------------------------------ the executor

/// Runs a script app's host-service tools as the app's own
/// `host.request` would, and answers each call once.
pub struct HostServiceExecutor {
    pub app: String,
    /// The tools that run on a host service.
    pub tools: BTreeSet<String>,
    pub methods: HashMap<String, String>,
    /// Declared capability families, retained as disclosure metadata.
    pub families: BTreeSet<String>,
    /// The directory App Hub hands every host service (`<apps root>/.host`).
    pub host_dir: PathBuf,
}

/// The engine (ADR 0013) `method` runs on when this build leaves it out:
/// the photo engine, which only the desktop links (`craft-engines`; weighed
/// for Home and left out), for its own `photo.*` methods and Photos'
/// `photos.info`. `None` when the build links it, or the method needs none.
pub fn unlinked_engine(method: &str) -> Option<&'static str> {
    let photo = method == "photos.info" || method.split('.').next() == Some("photo");
    (photo && !cfg!(feature = "craft-engines")).then_some("photo")
}

/// The plain answer to `tool`, whose `engine` this build leaves out
/// ([`unlinked_engine`]): the call never runs here.
pub fn not_on_this_device(tool: &str, engine: &str) -> String {
    format!("{tool} isn't available on this device: the {engine} engine is only in the desktop build")
}

/// Where an engine method's call works ([`HostServiceExecutor::run`]): the
/// shell's areas (`None`), or a test's.
#[cfg(feature = "app-hub")]
pub(crate) type AreaSource = Option<Arc<dyn super::areas::AreaEnv>>;
#[cfg(not(feature = "app-hub"))]
pub(crate) type AreaSource = ();

/// What a waiting call holds until it is answered or cancelled: the area
/// granted to its agent.
#[cfg(feature = "app-hub")]
type Held = Option<super::areas::Grant>;
#[cfg(not(feature = "app-hub"))]
type Held = ();

/// Where a call's answer goes: App Hub's reply queue, keyed by a heap key no
/// isolate uses.
struct Waiting {
    app: String,
    call_id: String,
    reply: ToolReply,
    _held: Held,
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

fn public_mail_args(
    app: &str,
    call: &HostToolCall,
    account: &str,
    mut args: Value,
) -> Result<Value, String> {
    if super::relay::app_of_peer(&call.calling_app) == app
        && call.account.as_deref() != Some(account)
    {
        return Err("The app Mail account changed; reopen its conversation".into());
    }
    let object = args
        .as_object_mut()
        .ok_or("Mail tool arguments must be an object")?;
    if object
        .get("account")
        .is_some_and(|value| value.as_str() != Some(account))
    {
        return Err("Mail tools cannot access another account".into());
    }
    object.insert("account".into(), json!(account));
    Ok(args)
}

fn connected_args(app: &str, call: &HostToolCall, connection: &str, mut args: Value) -> Result<Value, String> {
    if super::relay::app_of_peer(&call.calling_app) == app && call.account.as_deref() != Some(connection) {
        return Err("The app account changed; reopen its conversation".into());
    }
    let object = args.as_object_mut().ok_or("Tool arguments must be an object")?;
    if object.get("connection").is_some_and(|v|v.as_str()!=Some(connection)) {
        return Err("Tools cannot select a different connection".into());
    }
    object.insert("connection".into(), json!(connection));
    Ok(args)
}

/// Agent-authored cards may select reviewed bundle code or declare an L0 UI,
/// but cannot introduce executable Splash. Check the resolved service method:
/// a bundle's alias (for example `inbox.notify`) has the same boundary.
fn check_agent_publication(method: &str, args: &Value) -> Result<(), String> {
    if method != "glance.publish" { return Ok(()); }
    let args = args.as_object().ok_or("Card publication arguments must be an object")?;
    if args.contains_key("script") {
        return Err("Agents cannot publish executable Splash; choose an admitted template with initial data, or L0 source".into());
    }
    if let Some(template) = args.get("template") {
        if !template.as_str().is_some_and(|name| !name.is_empty())
            || !args.get("initial").is_some_and(Value::is_object)
            || args.contains_key("source") || args.contains_key("data") {
            return Err("Template cards require a template name and initial object, without source or data".into());
        }
        // Glance resolves this name inside the owner's digest-checked bundle;
        // initial values are JSON data, never interpolated executable code.
        return Ok(());
    }
    if args.contains_key("initial") {
        return Err("Initial data requires an admitted template".into());
    }
    let source = args.get("source").and_then(Value::as_str).ok_or("An agent card requires an admitted template or L0 source")?;
    let report = octoscript_ui_l0::check_ui_l0(source);
    if !report.valid || report.level != octoscript_ui_l0::Level::L0 {
        return Err("Agent-authored card source must use valid L0 declarations; executable code and L1 expressions are not allowed".into());
    }
    Ok(())
}

impl ToolExecutor for HostServiceExecutor {
    fn execute(&self, call: HostToolCall, reply: ToolReply) {
        self.run(call, reply, Default::default())
    }

    fn cancel(&self, call_id: &str) {
        // Stop waiting now, and have App Hub drop the request: the service's
        // late answer goes nowhere, and the request no longer counts against
        // the calls that may wait.
        let keys: Vec<usize> = {
            let mut waiting = WAITING.lock().unwrap_or_else(|e| e.into_inner());
            let Some(waiting) = waiting.as_mut() else { return };
            let keys: Vec<usize> = waiting.iter().filter(|(_, w)| w.app == self.app && w.call_id == call_id).map(|(key, _)| *key).collect();
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

impl HostServiceExecutor {
    /// Run `call` as the app's own `host.request` would, and answer it once;
    /// an engine's method in the call's area ([`super::areas::agent_area`]),
    /// from `areas`.
    pub(crate) fn run(&self, call: HostToolCall, reply: ToolReply, areas: AreaSource) {
        if !reply.is_open() {
            return;
        }
        if !self.tools.contains(&call.name) {
            reply.finish(ToolOutcome::error("app_tool_unavailable", format!("{} declares a script implementation, but this host does not support script tool dispatch", call.name)));
            return;
        }
        let method = self.methods.get(&call.name).map(String::as_str).unwrap_or(&call.name);
        if let Some(descriptor) = octosense_appstore::host_api::methods().into_iter().find(|api| api.name == method) {
            if descriptor.agent_access != octosense_appstore::services::AgentAccess::Allowed {
                reply.finish(ToolOutcome::error("agent_access_denied", format!("{method} requires direct foreground app interaction")));
                return;
            }
            if !descriptor.supports(octosense_appstore::host_api::platform()) {
                reply.finish(ToolOutcome::error("api_unavailable", format!("{method} is not supported on this platform")));
                return;
            }
        }
        // A declared tool whose engine this build leaves out (Photos'
        // `photos.info` on Home) never runs: the agent hears so plainly,
        // before any area is made for it, rather than the stand-in notice
        // service's "no method".
        if let Some(engine) = unlinked_engine(method) {
            reply.finish(ToolOutcome::error("unavailable", not_on_this_device(&call.name, engine)));
            return;
        }
        if let Err(message) = check_agent_publication(method, &call.args) {
            reply.finish(ToolOutcome::error("unsafe_card_source", message));
            return;
        }
        let family = method.split('.').next().unwrap_or("");
        // Tool ownership/sharing was checked by the relay. A manifest family
        // is disclosure, not a second grant. The service keeps its own
        // identity, account, consent and foreground-review boundaries.
        let mut args = match scoped_args(&self.app, &call) {
            Ok(args) => args,
            Err(message) => {
                reply.finish(ToolOutcome::error("account_scope", message));
                return;
            }
        };
        if matches!(method, "mail.compose" | "mail.compose_status") && self.app != "os.mail" {
            let Some(account) = octosense_mail_service::active_account(&self.host_dir, &self.app)
            else {
                reply.finish(ToolOutcome::error(
                    "account_scope",
                    "Connect this app's Mail account first",
                ));
                return;
            };
            args = match public_mail_args(&self.app, &call, &account, args) {
                Ok(args) => args,
                Err(error) => {
                    reply.finish(ToolOutcome::error("account_scope", error));
                    return;
                }
            };
        }
        if matches!(family, "gmail" | "gcalendar" | "github")
            || matches!(method, "auth.backend.me" | "auth.backend.request") {
            let Some(connection) = octosense_oauth_service::host::active_connection(&self.host_dir, &self.app) else {
                reply.finish(ToolOutcome::error("account_scope", "Connect this app account first"));
                return;
            };
            args = match connected_args(&self.app,&call,&connection.handle,args) {
                Ok(args)=>args,
                Err(error)=>{reply.finish(ToolOutcome::error("account_scope",error));return;}
            };
        }
        // An engine's method works in the call's area (the calling agent's
        // own folder, or the owning app's for an app's own tool), granted
        // to this call until it is answered, its root the call's host
        // directory (ADR 0013, `areas`). Paths in errors are relative to
        // it.
        #[cfg(feature = "app-hub")]
        let (host_dir, reply, held) = if super::areas::needs_area(method) {
            let env = areas.unwrap_or_else(super::areas::shell);
            match super::areas::agent_area(&*env, &call, &self.app) {
                Ok(area) => {
                    let root = area.root.clone();
                    let reply = super::areas::relative_errors(reply, &root);
                    (root, reply, Some(super::areas::grant(area)))
                }
                Err((kind, message)) => {
                    reply.finish(ToolOutcome::error(kind, message));
                    return;
                }
            }
        } else {
            (self.host_dir.clone(), reply, None)
        };
        #[cfg(not(feature = "app-hub"))]
        let (host_dir, held) = {
            let () = areas;
            (self.host_dir.clone(), ())
        };
        let key = NEXT_KEY.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        WAITING.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(key, Waiting { app: self.app.clone(), call_id: call.call_id.clone(), reply, _held: held });
        let service_call = ServiceCall { app_id: self.app.clone(), service: method.to_owned(), args, from_sheet: false,
            // A tool call has no surface for a sheet: the person is not in the app.
            may_prompt: false, host_dir };
        octosense_appstore::services::dispatch(service_call, key, 0, &mut NoSheet);
    }
}

/// The relay already validated schemas, grants and approval. The runner
/// validates against its own immutable admitted bundle again before dispatch.
struct ScriptAppExecutor { host: HostServiceExecutor, tools: BTreeSet<String> }
struct ScriptWaiting { token: Option<String>, reply: ToolReply, host_dir: PathBuf, account: String, caller: Option<String> }
type ScriptKey = (String, String);
static SCRIPT_WAITING: Mutex<Option<HashMap<ScriptKey, ScriptWaiting>>> = Mutex::new(None);
fn current_script_account(host_dir: &Path, app: &str) -> String {
    octosense_oauth_service::host::active_connection(host_dir, app)
        .map(|connection| connection.handle).unwrap_or_else(|| "device".into())
}
fn script_admission(app: &str, caller: Option<&str>) -> Result<(), String> {
    super::admission::check(app)?;
    if let Some(caller) = caller.filter(|caller| *caller != app) {
        super::admission::check(caller)?;
    }
    Ok(())
}
fn script_outcome(result: Result<Value, String>) -> ToolOutcome {
    match result {
        Ok(value) => ToolOutcome::Ok(value),
        Err(error) => {
            let (kind, message) = error.split_once(": ").unwrap_or(("app_error", &error));
            ToolOutcome::error(kind, message)
        }
    }
}
impl ToolExecutor for ScriptAppExecutor {
    fn execute(&self, call: HostToolCall, reply: ToolReply) {
        if !self.tools.contains(&call.name) { self.host.execute(call, reply); return; }
        if !reply.is_open() { return; }
        // This ABI does not turn a script button into native proof of human
        // confirmation. Apps needing approval use the host approval path.
        if call.confirm_required {
            reply.finish(ToolOutcome::error("app_confirmation_unavailable", "Script tools require host confirmation; confirm: app is not supported by this ABI"));
            return;
        }
        let account = current_script_account(&self.host.host_dir, &self.host.app);
        if super::relay::app_of_peer(&call.calling_app) == self.host.app && call.account.as_deref() != Some(&account) {
            reply.finish(ToolOutcome::error("account_scope", "The app account changed; reopen its conversation")); return;
        }
        octosense_appstore::script_tools::set_account(&self.host.app, &account);
        let key = (self.host.app.clone(), call.call_id.clone());
        let caller = (call.caller_kind == crate::ai_host::app_peers::host_tools::CallerKind::AppPeer)
            .then(|| super::relay::app_of_peer(&call.calling_app).to_owned());
        {
            let mut map = SCRIPT_WAITING.lock().unwrap_or_else(|e|e.into_inner());
            let map = map.get_or_insert_with(HashMap::new);
            if map.contains_key(&key) { reply.finish(ToolOutcome::error("duplicate_call", "This app tool call is already pending")); return; }
            map.insert(key.clone(), ScriptWaiting { token: None, reply: reply.clone(), host_dir: self.host.host_dir.clone(), account: account.clone(), caller });
        }
        let done_key = key.clone();
        let done = Box::new(move |result| complete_script_call(&done_key, result));
        match octosense_appstore::script_tools::submit(&self.host.app, &call.name, call.args,
            &account, &call.calling_app, std::time::Duration::from_millis(call.timeout_ms.max(1)), done) {
            Ok(token) => {
                let registered = {
                    let mut map = SCRIPT_WAITING.lock().unwrap_or_else(|e|e.into_inner());
                    if let Some(waiting) = map.as_mut().and_then(|map|map.get_mut(&key)) { waiting.token = Some(token.clone()); true } else { false }
                };
                if !registered { octosense_appstore::script_tools::cancel(&token); }
            }
            Err(error) => {
                SCRIPT_WAITING.lock().unwrap_or_else(|e|e.into_inner()).as_mut().and_then(|map|map.remove(&key));
                reply.finish(script_outcome(Err(error)));
            }
        }
    }
    fn cancel(&self, call_id: &str) {
        let key = (self.host.app.clone(),call_id.to_owned());
        let waiting = SCRIPT_WAITING.lock().unwrap_or_else(|e|e.into_inner()).as_mut().and_then(|map|map.remove(&key));
        if let Some(waiting) = waiting { if let Some(token) = waiting.token { octosense_appstore::script_tools::cancel(&token); } }
        self.host.cancel(call_id);
    }
}
fn complete_script_call(key: &ScriptKey, result: Result<Value, String>) {
    let waiting = SCRIPT_WAITING.lock().unwrap_or_else(|e|e.into_inner()).as_mut().and_then(|map|map.remove(key));
    if let Some(waiting) = waiting {
        // A system/cross-app caller also loses the old account's result.
        // This final check closes the interval between UI polls and completion.
        let result = if current_script_account(&waiting.host_dir, &key.0) != waiting.account {
            Err("account_scope: the app account changed".into())
        } else if let Err(error) = script_admission(&key.0, waiting.caller.as_deref()) {
            // The catalog or executable bundle can change while a tool awaits
            // an asynchronous callback. Never return data after withdrawal.
            Err(format!("app_unavailable: {error}"))
        } else { result };
        waiting.reply.finish(script_outcome(result));
    }
}

fn poll_script_accounts() {
    let entries: Vec<_> = SCRIPT_WAITING.lock().unwrap_or_else(|e|e.into_inner()).as_ref().map(|map| map.iter().map(|(key,w)| (key.clone(),w.host_dir.clone(),w.token.clone(),w.reply.is_open(),w.caller.clone())).collect()).unwrap_or_default();
    for (key, host_dir, token, open, caller) in entries {
        if !open {
            SCRIPT_WAITING.lock().unwrap_or_else(|e|e.into_inner()).as_mut().and_then(|map|map.remove(&key));
            if let Some(token) = token { octosense_appstore::script_tools::cancel(&token); }
            continue;
        }
        if let Err(error) = script_admission(&key.0, caller.as_deref()) {
            complete_script_call(&key, Err(format!("app_unavailable: {error}")));
            if let Some(token) = token { octosense_appstore::script_tools::cancel(&token); }
            continue;
        }
        octosense_appstore::script_tools::set_account(&key.0,&current_script_account(&host_dir,&key.0));
    }
}

/// Deliver the host services' answers to the calls waiting on them.
pub fn poll() {
    poll_script_accounts();
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

    /// Register a real packed fixture for declaration/identity boundary tests.
    /// Each caller runs in a child process before setting App Hub's data root.
    pub(crate) fn declaration_fixture(id: &'static str, capabilities: &[&str]) {
        let dir = stamped_bundle("camera", id, |dir, manifest| {
            manifest["id"] = json!(id);
            manifest["capabilities"] = json!(capabilities);
            manifest["requires"] = json!(["host-api-v1"]);
            for key in ["agent", "host_api"] { manifest.as_object_mut().unwrap().remove(key); }
            for path in ["tools.json", "AGENT.md"] { let _ = std::fs::remove_file(dir.join(path)); }
            std::fs::write(dir.join("main.splash"), "use mod.widgets.*\nApp { Label { text: \"Fixture\" } }\n").unwrap();
        });
        let packed = octosense_app_hub::pack::pack_system_app(&dir).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        octosense_appstore::system::register_system_app(octosense_appstore::system::SystemApp {
            id, name: "Declaration fixture", pack: Box::leak(packed.pack_json.into_boxed_str()), assets: &[],
        });
    }

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

    /// Calendar's bundle gives its agent its six tools, all on its own
    /// `calendar` host service; removing an event is destructive (the
    /// person approves it); Calendar is granted `glance`.
    #[test]
    fn calendar_offers_its_tools_from_its_bundle() {
        let dir = stamped_bundle("calendar", "tools", |_, _| {});
        let loaded = from_bundle(&dir).unwrap();
        let names: Vec<&str> = loaded.tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, ["calendar.events", "calendar.add_event", "calendar.update_event", "calendar.remove_event", "calendar.notify", "calendar.agenda"]);
        assert_eq!(loaded.host_service_tools.len(), 6);
        assert!(loaded.tools.iter().all(|t| t["input_schema"]["type"] == "object" && t["output_schema"]["type"] == "object"));
        let remove = loaded.tools.iter().find(|t| t["name"] == "calendar.remove_event").unwrap();
        assert_eq!(remove["risk"], "destructive");
        assert!(loaded.families.contains("glance") && loaded.families.contains("calendar"));
        let edit = loaded.tools.iter().find(|t| t["name"] == "calendar.update_event").unwrap();
        assert_ne!(edit["shareable"], true, "editing is not granted to other apps");
        assert!(edit["input_schema"]["required"].as_array().unwrap().iter().any(|v| v == "expected"));
        assert_eq!(loaded.generic, ["ask_user_question"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Maps, YouTube and Camera each give their agent one tool,
    /// `<namespace>.notify`, on their own namespace: no service of their
    /// own answers it, so the shell's notice service does
    /// (glance_notice.rs). Each is granted `glance`, and nothing else new
    /// (Maps' `web` is for a place's website in its reader, not for the
    /// agent).
    #[test]
    fn maps_youtube_and_camera_offer_notify_from_their_bundles() {
        for (app, kept) in [("maps", &["storage", "net", "location", "web"][..]), ("youtube", &["storage", "net"]), ("camera", &["storage", "camera", "microphone", "library"])] {
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

    /// Photos' agent keeps `photos.notify` and gains `photos.info` (the
    /// photo engine, ADR 0013), both on its own `photos` service — and
    /// nothing else new: the same families (`model` is for its Memories,
    /// not for the agent), no new capability, `info` its only read tool,
    /// shared with no one.
    #[test]
    fn photos_offers_notify_and_info_from_its_bundle() {
        let dir = stamped_bundle("photos", "notify", |_, _| {});
        let loaded = from_bundle(&dir).unwrap();
        let _ = std::fs::remove_dir_all(dir);
        let names: Vec<&str> = loaded.tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, ["photos.notify", "photos.info"]);
        assert_eq!(loaded.host_service_tools.len(), 2);
        assert!(loaded.tools.iter().all(|t| t["input_schema"]["type"] == "object" && t["output_schema"]["type"] == "object"), "octos takes object schemas only");
        let notify = &loaded.tools[0];
        assert_eq!((notify["risk"].as_str(), notify["shareable"].as_bool(), notify["background"].as_bool()), (Some("act"), Some(false), Some(true)));
        let info = &loaded.tools[1];
        assert_eq!((info["risk"].as_str(), info["shareable"].as_bool(), info["background"].as_bool()), (Some("read"), Some(false), Some(false)));
        assert_eq!(loaded.families, ["storage", "model", "glance"].iter().map(|f| f.to_string()).collect::<BTreeSet<String>>());
        assert_eq!(loaded.generic, ["ask_user_question"]);
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
    fn public_mail_tool_aliases_bind_the_owners_live_account() {
        let mut request = call("contestant.compose_reply");
        request.calling_app = "card.sample.mail".into();
        request.account = Some("one".into());
        assert_eq!(
            public_mail_args("sample.mail", &request, "one", json!({"body":"draft"})).unwrap()
                ["account"],
            "one"
        );
        assert!(
            public_mail_args("sample.mail", &request, "one", json!({"account":"two"})).is_err()
        );
        request.account = Some("old".into());
        assert!(public_mail_args("sample.mail", &request, "one", json!({}))
            .unwrap_err()
            .contains("changed"));
        request.calling_app = "card.other.app".into();
        assert_eq!(
            public_mail_args("sample.mail", &request, "one", json!({})).unwrap()["account"],
            "one",
            "an already admitted cross-app call still uses the owning app account"
        );
    }

    #[test]
    fn declared_script_tools_report_closed_app_and_stale_account_without_starting_a_vm() {
        let exec = ScriptAppExecutor {
            host: HostServiceExecutor { app: "org.example.scriptfixture".into(), tools: Default::default(), methods: Default::default(), families: Default::default(), host_dir: std::env::temp_dir().join("script-tools-no-accounts") },
            tools: ["scriptfixture.read".into()].into_iter().collect(),
        };
        let (r,sent) = reply(); exec.execute(call("scriptfixture.read"),r);
        assert_eq!(sent.lock().unwrap()[0]["error"]["kind"],"app_not_running");
        let mut stale = call("scriptfixture.read"); stale.calling_app = "card.org.example.scriptfixture".into(); stale.account = Some("retired".into());
        let (r,sent) = reply(); exec.execute(stale,r);
        assert_eq!(sent.lock().unwrap()[0]["error"]["kind"],"account_scope");
        let mut confirmation = call("scriptfixture.read"); confirmation.confirm_required = true;
        let (r,sent) = reply(); exec.execute(confirmation,r);
        assert_eq!(sent.lock().unwrap()[0]["error"]["kind"],"app_confirmation_unavailable");
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

    #[test]
    fn connected_tools_refuse_stale_peers_and_model_selected_accounts() {
        let mut request=call("inbox.message");
        request.calling_app="card.org.example.inbox".into();
        request.account=Some("old".into());
        assert!(connected_args("org.example.inbox",&request,"current",json!({})).is_err());
        request.account=Some("current".into());
        assert_eq!(connected_args("org.example.inbox",&request,"current",json!({"message_id":"123"})).unwrap(),json!({"message_id":"123","connection":"current"}));
        assert!(connected_args("org.example.inbox",&request,"current",json!({"connection":"other"})).is_err());
        // Cross-app permission is checked by the relay before this executor.
        // Even an authorized caller can only use the owner's active account.
        request.calling_app="card.org.example.calendar".into();
        request.account=Some("caller-account".into());
        assert_eq!(connected_args("org.example.inbox",&request,"current",json!({})).unwrap()["connection"],"current");
        assert!(connected_args("org.example.inbox",&request,"current",json!({"connection":"caller-account"})).is_err());
    }

    #[test]
    fn backend_data_aliases_cannot_reuse_a_stale_peer_with_the_current_handle() {
        let app = "org.example.backend-scope";
        let current = "a0b1c2d3-1111-4222-8333-444455556666";
        let host_dir = std::env::temp_dir().join(format!("backend-tool-scope-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(host_dir.join("oauth")).unwrap();
        // Synthetic metadata only. No credential is created or read: the
        // account broker must refuse before the service or vault is reached.
        std::fs::write(host_dir.join("oauth/connections.json"), json!({
            "entries": {current: {"handle":current,"app_id":app,"provider":"github","subject":"synthetic-scope-test","label":"Synthetic","scopes":[],"expires_at":null}},
            "active": {app:current}
        }).to_string()).unwrap();
        assert_eq!(octosense_oauth_service::host::active_connection(&host_dir,app).unwrap().handle,current);
        for method in ["auth.backend.me", "auth.backend.request"] {
            let alias = "backend.lookup";
            let exec = HostServiceExecutor {app:app.into(), tools:[alias.into()].into_iter().collect(),
                methods:HashMap::from([(alias.into(),method.into())]),families:["auth".into()].into_iter().collect(),host_dir:host_dir.clone()};
            let mut request = call(alias);
            request.calling_app = format!("card.{app}");
            request.account = Some("retired-connection".into());
            request.args = json!({"connection":current,"operation":"notes.list"});
            let (r,sent) = reply();exec.execute(request,r);
            let output = sent.lock().unwrap();
            if output[0]["error"]["kind"] == "api_unavailable" {
                // A host without the backend API (auth is macOS and Android
                // only) refuses it before the account check, once any test
                // has registered the auth catalog in this process.
                assert!(octosense_appstore::host_api::methods().iter()
                    .any(|api| api.name == method && !api.supports(octosense_appstore::host_api::platform())),"{}",output[0]);
            } else {
                assert_eq!(output[0]["error"]["kind"],"account_scope", "{method}");
                assert!(output[0]["error"]["message"].as_str().unwrap().contains("account changed"),"{}",output[0]);
            }
            assert!(!is_waiting(&format!("c-{alias}")),"stale peer must not reach a backend service");
        }
        std::fs::remove_dir_all(host_dir).unwrap();
    }

    #[test]
    fn account_change_before_script_completion_drops_the_old_result_without_a_poll() {
        let app = "org.example.pending-account";
        let key = (app.into(),"pending-switch".into());
        let host_dir = std::env::temp_dir().join(format!("script-result-account-{}", uuid::Uuid::new_v4()));
        let (r,sent) = reply();
        SCRIPT_WAITING.lock().unwrap().get_or_insert_with(HashMap::new).insert(key.clone(), ScriptWaiting {
            token:None,reply:r,host_dir:host_dir.clone(),account:"retired-account".into(),caller:None
        });
        // The disconnected app is now on its device account; no poll has run.
        complete_script_call(&key, Ok(json!({"private":"old account data"})));
        let output=sent.lock().unwrap();
        assert_eq!(output[0]["error"]["kind"],"account_scope");
        assert!(output[0].get("data").is_none(),"old data must not be forwarded");
    }

    #[test]
    fn script_result_and_poll_refuse_an_owner_no_longer_admitted() {
        // No catalog admits this identity. It models an admission that was
        // valid when queued and is unavailable at the later completion/poll.
        // The signed withdrawal/tamper cases themselves are tested by admission.
        for poll_first in [false, true] {
            let key = (format!("org.example.expired-{}", uuid::Uuid::new_v4()), "pending".into());
            let host_dir = std::env::temp_dir().join(format!("expired-tool-{}", uuid::Uuid::new_v4()));
            let (r, sent) = reply();
            SCRIPT_WAITING.lock().unwrap().get_or_insert_with(HashMap::new).insert(key.clone(), ScriptWaiting {
                token: None, reply: r, host_dir, account: "device".into(), caller: None,
            });
            if poll_first { poll_script_accounts(); }
            complete_script_call(&key, Ok(json!({"private":"must not escape"})));
            let output = sent.lock().unwrap();
            assert_eq!(output.len(), 1, "a late callback cannot answer twice");
            assert_eq!(output[0]["error"]["kind"], "app_unavailable");
            assert!(output[0].get("data").is_none());
        }
    }

    #[test]
    fn withdrawn_cross_app_caller_cannot_receive_an_admitted_owners_pending_result() {
        for poll_first in [false, true] {
            // The host identity remains admitted; only the requesting app has
            // disappeared. Isolate the late-result check from initial routing.
            let key = (super::super::relay::SYSTEM.into(), format!("cross-app-{}", uuid::Uuid::new_v4()));
            let host_dir = std::env::temp_dir().join(format!("cross-app-tool-{}", uuid::Uuid::new_v4()));
            let (r, sent) = reply();
            SCRIPT_WAITING.lock().unwrap().get_or_insert_with(HashMap::new).insert(key.clone(), ScriptWaiting {
                token: None, reply: r, host_dir, account: "device".into(),
                caller: Some(format!("org.example.withdrawn-{}", uuid::Uuid::new_v4())),
            });
            if poll_first { poll_script_accounts(); }
            complete_script_call(&key, Ok(json!({"private":"owner data"})));
            let output = sent.lock().unwrap();
            assert_eq!(output.len(), 1);
            assert_eq!(output[0]["error"]["kind"], "app_unavailable");
            assert!(output[0].get("data").is_none());
        }
    }

    #[test]
    fn admitted_alias_dispatches_with_owner_identity_without_a_family_declaration() {
        octosense_appstore::services::register_host_service(Box::new(Probe));
        let exec=HostServiceExecutor {app:"org.example.notes".into(),
            tools:["notes.lookup".to_owned()].into_iter().collect(),
            methods:HashMap::from([("notes.lookup".into(),"g3probe.echo".into())]),
            families:["notes".to_owned()].into_iter().collect(),host_dir:std::env::temp_dir()};
        let (r,sent)=reply();exec.execute(call("notes.lookup"),r);
        for _ in 0..50 {poll();if !sent.lock().unwrap().is_empty(){break;} std::thread::sleep(std::time::Duration::from_millis(10));}
        let answer=sent.lock().unwrap();
        assert_eq!(answer[0]["data"]["app"],"org.example.notes");
        assert_eq!(answer[0]["data"]["service"],"g3probe.echo");
        assert_eq!(answer[0]["data"]["from_sheet"],false);
    }

    fn reply() -> (ToolReply, Arc<Mutex<Vec<Value>>>) {
        let sent: Arc<Mutex<Vec<Value>>> = Arc::default();
        let s = sent.clone();
        (ToolReply::new("c", move |v| s.lock().unwrap().push(v)), sent)
    }

    #[test]
    fn described_host_methods_enforce_agent_access_before_dispatch() {
        use octosense_appstore::services::{AgentAccess, HostApiMethod};
        struct GateProbe;
        impl HostService for GateProbe {
            fn family(&self) -> &'static str { "script_agent_gate" }
            fn api_methods(&self) -> Vec<HostApiMethod> {
                [("deny", AgentAccess::Denied), ("foreground", AgentAccess::ForegroundOnly), ("allow", AgentAccess::Allowed)]
                    .into_iter().map(|(method, access)| HostApiMethod::new(format!("script_agent_gate.{method}"), 1, "storage", "Fixture", json!({"type":"object"}), json!({"type":"object"}))
                    .with_platforms(&[octosense_appstore::host_api::platform()]).with_agent_access(access)).collect()
            }
            fn call(&mut self, call: ServiceCall, reply: Replier, _: &mut dyn ServiceHost) { reply.send(Ok(json!({"method":call.method()}))); }
        }
        octosense_appstore::services::register_host_service(Box::new(GateProbe));
        let exec = HostServiceExecutor { app:"org.example.gate".into(), tools:["gate.denied".into(),"gate.foreground".into()].into_iter().collect(),
            methods: HashMap::from([("gate.denied".into(),"script_agent_gate.deny".into()),("gate.foreground".into(),"script_agent_gate.foreground".into())]),
            families:["script_agent_gate".into()].into_iter().collect(),host_dir:std::env::temp_dir() };
        for tool in ["gate.denied","gate.foreground"] {
            let (reply,received)=reply();exec.execute(call(tool),reply);
            assert_eq!(received.lock().unwrap()[0]["error"]["kind"],"agent_access_denied");
        }
    }

    #[test]
    fn agent_publication_aliases_refuse_executable_or_mixed_sources_before_dispatch() {
        for name in ["inbox.notify", "glance.publish"] {
            let exec = HostServiceExecutor {
                app: "org.example.inbox".into(),
                tools: [name.to_string()].into_iter().collect(),
                methods: HashMap::from([(name.into(), "glance.publish".into())]),
                families: ["glance".to_string()].into_iter().collect(),
                host_dir: std::env::temp_dir(),
            };
            for args in [
                json!({"script":"host.request(\"gmail.send\", {})"}),
                json!({"template":"glance-workspace.splash","initial":{},"script":null}),
                json!({"template":"glance-workspace.splash","initial":{},"source":"ignored"}),
                json!({"template":"glance-workspace.splash","initial":{},"data":{}}),
                json!({"source":"ui.label(\"x\").set_text(\"y\")"}),
                json!({"source":"# level: L1\nsource quote sys.quote(ticker: state.sym, fields: [last])\nstate sym { shape: text, initial: \"NVDA\" }\nstate shares { shape: number, initial: 10 }\nview root Surface { TextHero(value: shares * quote.last) }\n"}),
            ] {
                let mut request = call(name);
                request.args = args;
                let (r, sent) = reply();
                exec.execute(request, r);
                assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "unsafe_card_source", "{name}");
                assert!(!is_waiting(&format!("c-{name}")), "unsafe code must never enter the service queue");
            }
        }
    }

    #[test]
    fn agent_publication_allows_reviewed_templates_and_declaration_only_l0() {
        let initial = json!({"message":{"body":"\"; host.request(\"gmail.send\", {}) //"}});
        assert!(check_agent_publication("glance.publish", &json!({"template":"glance-workspace.splash","initial":initial})).is_ok());
        let source = "source note sys.dataset(fields: [title])\nview root Surface { TextTitle(text: note.title) }";
        assert!(check_agent_publication("glance.publish", &json!({"source":source,"data":{"note":{"title":"Delivery"}}})).is_ok());
        // Other services have their own schemas; this policy must not change them.
        assert!(check_agent_publication("github.read", &json!({"source":"README.md"})).is_ok());
        for args in [json!({}), json!([]), json!({"template":"x.splash"}), json!({"source":source,"initial":{}})] {
            assert!(check_agent_publication("glance.publish", &args).is_err());
        }
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

    #[test]
    fn cancelling_one_app_does_not_cancel_another_apps_identical_call_id() {
        // Hold replies until cancellation: a parallel test may pump globally,
        // so an immediate service reply could win before this test cancels it.
        struct CancelProbe(Arc<Mutex<Vec<(String, Replier)>>>);
        impl HostService for CancelProbe {
            fn family(&self) -> &'static str { "g3cancelprobe" }
            fn call(&mut self, call: ServiceCall, reply: Replier, _: &mut dyn ServiceHost) {
                self.0.lock().unwrap().push((call.app_id, reply));
            }
        }
        let held = Arc::new(Mutex::new(Vec::new()));
        octosense_appstore::services::register_host_service(Box::new(CancelProbe(held.clone())));
        let executor = |app: &str| HostServiceExecutor { app:app.into(), tools:["g3cancelprobe.echo".into()].into_iter().collect(), methods:Default::default(),families:["g3cancelprobe".into()].into_iter().collect(),host_dir:std::env::temp_dir() };
        let first=executor("org.example.cancel_first");let second=executor("org.example.cancel_second");
        let mut one=call("g3cancelprobe.echo");one.call_id="same-id-two-apps".into();
        let (reply_one,sent_one)=reply();let (reply_two,sent_two)=reply();
        first.execute(one.clone(),reply_one);second.execute(one,reply_two);
        first.cancel("same-id-two-apps");
        for (app, reply) in std::mem::take(&mut *held.lock().unwrap()) {
            reply.send(Ok(json!({"app":app})));
        }
        poll();
        assert!(sent_one.lock().unwrap().is_empty());
        assert_eq!(sent_two.lock().unwrap()[0]["data"]["app"],"org.example.cancel_second");
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
            methods: Default::default(),
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
    /// `host.request` would (its identity, never a sheet); the answer reaches
    /// the call through `poll`. The tool offer still bounds callable tools.
    #[test]
    fn a_host_service_tool_runs_as_the_apps_own_request() {
        octosense_appstore::services::register_host_service(Box::new(Probe));
        let exec = HostServiceExecutor {
            app: "os.g3probe".into(),
            tools: ["g3probe.echo".to_string(), "other.x".to_string()].into_iter().collect(),
            methods: Default::default(),
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
        for _ in 0..50 {
            poll();
            if !sent.lock().unwrap().is_empty() { break; }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(sent.lock().unwrap()[0]["error"]["message"].as_str().unwrap().contains("no service answers"), "unavailable service, not a declaration denial");
        let (r, sent) = reply();
        exec.execute(call("g3probe.in_script"), r);
        assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "app_tool_unavailable");
    }

    /// System and installed apps use the same host dispatch path even when
    /// neither declares the tool's service family.
    #[test]
    fn system_and_installed_tools_dispatch_without_family_declarations() {
        octosense_appstore::services::register_host_service(Box::new(Probe));
        let run = |app: &str| {
            let exec = HostServiceExecutor { app: app.into(), tools: ["g3probe.echo".to_string()].into_iter().collect(), methods: Default::default(), families: Default::default(), host_dir: std::env::temp_dir() };
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
        assert_eq!(run("com.example.g3probe")[0]["ok"], true, "an admitted tool uses the same dispatcher regardless of declarations");
    }
}
