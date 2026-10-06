//! An agent for every app that declares one (ADR 0004 §4, §6).
//!
//! **Which apps.** [`crate::apps::agent_apps`]: a native app whose
//! `native-apps.json` entry grants `octos.*`; a script app whose manifest
//! declares `octos.*` or an `agent` block, or whose admitted bundle ships
//! `tools.json` (App Hub's `AgentBundle::load`). From manifests and grants,
//! never names. Settings › Assistant lists every one of them.
//!
//! **Peers exist once allowed.** The person allows an app's agent on the
//! first-use sheet (from the app's "Ask <app>" panel, the app's own `octos`
//! call, or the system agent's `agents.ask`) or in Settings. From then on
//! the shell PREPARES its peer ([`prepare`]: `peer/prepare`, its tools
//! registered, its session open), at startup for the apps already allowed
//! and at the moment one is allowed, so the system agent's `peer_list`
//! shows it and `peer_send_input` reaches it. A script app's peer is the
//! contained `octos` service's (`card.<app id>`), shared with the app's own
//! calls and the panel. A native app's peer is its instance's (the module
//! host offers it at `create`), so it exists while the app is open; a native
//! app that keeps accounts cannot be prepared without its signed-in account.
//! Turning an agent off releases its peer (`contained::revoke`, lib.rs
//! `revoke_agents`); the shell refuses the system agent's input to it
//! (`host_tools::ShellToolHost::admit_input`).
//!
//! **What the system agent is told.** octos lists only prepared peers, and
//! has no hook for "this app has an agent you may not use yet". The shell
//! says it in two places of its own: a short note with the system chat's
//! turns whenever the apps' agents changed ([`system_note`]), and two host
//! tools on the system session, `agents.list` and `agents.ask`
//! ([`declarations`], answered by the system chat itself: [`call`]).
//! `agents.ask` shows the first-use sheet (over the system chat); only the
//! person answers it. The system chat holds the call until they did and the
//! agent's peer is ready ([`ask_settled`]), so the system agent goes on with
//! the person's request in the same turn instead of ending it to wait.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::ai_host::app_peers::host_tools::ToolOutcome;
use crate::ai_host::app_peers::OctosContext;
use crate::apps::AgentApp;

/// Tests that install the contained service's peer factory (it is global)
/// hold this.
#[cfg(test)]
pub(crate) static FACTORY_TESTS: Mutex<()> = Mutex::new(());

/// The host tools the system chat registers on the system session.
pub const LIST_TOOL: &str = "agents.list";
pub const ASK_TOOL: &str = "agents.ask";
pub const PROVISION_TOOL: &str = "agents.provision";
pub const STATUS_TOOL: &str = "agents.status";
/// Their owner, as the kernel shows it.
pub const OWNER: &str = "agents";

/// Where one app's agent stands for the person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// The person has not decided (the first use asks).
    NotAsked,
    Allowed,
    /// Turned off (Settings, or denied on the first-use sheet).
    Off,
}

impl Access {
    pub fn as_str(self) -> &'static str {
        match self {
            Access::NotAsked => "not yet allowed",
            Access::Allowed => "allowed",
            Access::Off => "off",
        }
    }
}

/// `app`'s access: developer mode grants every agent (ADR 0004 §13).
pub fn access(app: &str) -> Access {
    if crate::approvals::consent_granted(app) {
        return Access::Allowed;
    }
    match crate::approvals::with(|a| a.consent.state(app)) {
        Some(crate::approvals::consent::State::Denied) => Access::Off,
        _ => Access::NotAsked,
    }
}

/// The peer id of an app's agent: a native app's id, a script app's
/// `card.<app id>`.
pub fn peer_of(app: &AgentApp) -> String {
    if app.native {
        app.id.clone()
    } else {
        format!("{}{}", crate::ai_host::contained::PEER_PREFIX, app.id)
    }
}

/// Every app with an agent.
pub fn all() -> Vec<AgentApp> {
    crate::apps::agent_apps()
}

/// The app with an agent that `name` means: its id, its peer id, its
/// display name (any case), or a launcher id (`news` for `os.news`,
/// `hub:<id>` for an installed app).
pub fn find(name: &str) -> Option<AgentApp> {
    find_in(&all(), name)
}

pub fn find_in(apps: &[AgentApp], name: &str) -> Option<AgentApp> {
    let name = name.trim();
    let bare = name.strip_prefix("hub:").unwrap_or(name);
    apps.iter()
        .find(|a| a.id == bare || peer_of(a) == bare || a.name.eq_ignore_ascii_case(bare) || a.id == format!("os.{bare}"))
        .cloned()
}

// ------------------------------------------------------------ peers

/// What the shell knows of each preparation: running, done, or why not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prepared {
    Preparing,
    Ready,
    Failed(String),
}

static PREPARED: Mutex<BTreeMap<String, Prepared>> = Mutex::new(BTreeMap::new());

fn set_prepared(app: &str, state: Option<Prepared>) {
    let mut map = PREPARED.lock().unwrap_or_else(|e| e.into_inner());
    match state {
        Some(state) => map.insert(app.to_string(), state),
        None => map.remove(app),
    };
    makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
}

/// Where `app`'s preparation stands (`None`: never tried in this run).
pub fn prepared(app: &str) -> Option<Prepared> {
    PREPARED.lock().unwrap_or_else(|e| e.into_inner()).get(app).cloned()
}

/// Prepare `app`'s agent now, off the UI thread, if the person allowed it
/// and it is not prepared or preparing. A native app's peer is its open
/// instance's: nothing to prepare here.
pub fn prepare(app: &AgentApp) {
    if app.native || access(&app.id) != Access::Allowed {
        return;
    }
    if matches!(prepared(&app.id), Some(Prepared::Preparing)) {
        return;
    }
    if matches!(prepared(&app.id), Some(Prepared::Ready)) && crate::ai_host::contained::is_live(&app.id) {
        return;
    }
    set_prepared(&app.id, Some(Prepared::Preparing));
    let id = app.id.clone();
    let spawned = std::thread::Builder::new().name(format!("prepare-{id}")).spawn(move || {
        #[cfg(any(feature = "app-hub", native_mobile))]
        if let Err(error) = crate::agent_events::install_guidance(&id) {
            set_prepared(&id, Some(Prepared::Failed(error)));
            return;
        }
        // Its manifest's storage block first: which account it acts for.
        let outcome = match crate::app_storage::host() {
            Some(storage) => crate::app_storage::lifecycle::prepare_agent_with(storage, storage.layout().apps_root(), &id, |_| crate::ai_host::contained::prepare(&id)),
            None => crate::ai_host::contained::prepare(&id),
        };
        match &outcome {
            Ok(()) => makepad_widgets::log!("agents: {id}'s agent is prepared (its peer is listed for the system agent)"),
            Err(e) => makepad_widgets::log!("agents: {id}'s agent could not be prepared: {e}"),
        }
        // Turned off meanwhile: the revocation wins.
        if access(&id) != Access::Allowed {
            crate::ai_host::contained::revoke(&id);
            set_prepared(&id, None);
            return;
        }
        set_prepared(&id, Some(match outcome {
            Ok(()) => Prepared::Ready,
            Err(e) => Prepared::Failed(e),
        }));
    });
    if spawned.is_err() {
        set_prepared(&app.id, Some(Prepared::Failed("no thread".into())));
    }
}

/// Every allowed app's agent (startup, and when the assistant comes up).
pub fn prepare_allowed() {
    for app in all() {
        prepare(&app);
    }
}

/// At startup, once the kernel is configured: prepare the agents the person
/// already allowed.
pub fn start() {
    let _ = std::thread::Builder::new().name("agents-start".into()).spawn(prepare_allowed);
    #[cfg(any(feature = "app-hub", native_mobile))]
    crate::agent_events::start();
}

/// The apps the person just allowed (the first-use sheet, Settings) get
/// their peer now; the ones turned off are forgotten (lib.rs revokes their
/// services).
pub fn pump(revoked: &[String]) {
    for app in revoked {
        set_prepared(app, None);
    }
    let allowed = crate::approvals::take_allowed();
    if allowed.is_empty() {
        return;
    }
    let apps = all();
    for id in allowed {
        if let Some(app) = find_in(&apps, &id) {
            prepare(&app);
        }
    }
}

/// The app's conversation for a shell surface (the "Ask <app>" panel):
/// the person's lane on the app's one peer. Only once the person allowed
/// the agent. A native app's is its open instance's peer.
pub fn conversation(app: &AgentApp, instance: &str) -> Result<Arc<dyn OctosContext>, String> {
    if access(&app.id) != Access::Allowed {
        return Err(format!("{}'s assistant is {}", app.name, access(&app.id).as_str()));
    }
    if !app.native {
        #[cfg(any(feature = "app-hub", native_mobile))]
        crate::agent_events::install_guidance(&app.id)?;
        let context = crate::ai_host::contained::conversation(&app.id, instance)?;
        set_prepared(&app.id, Some(Prepared::Ready));
        return Ok(context);
    }
    native_conversation(app, instance)
}

/// Conversation pinned to a host-owned publication account, never a card argument.
pub fn conversation_for_account(app: &AgentApp, instance: &str, account: &str) -> Result<Arc<dyn OctosContext>, String> {
    if access(&app.id) != Access::Allowed {
        return Err(format!("{}'s assistant is {}", app.name, access(&app.id).as_str()));
    }
    if app.native { return Err("Account-bound card chat requires a contained app".into()); }
    if crate::ai_host::contained::account_of(&app.id).as_deref() != Some(account) {
        return Err("Account changed; reopen the card under its original account".into());
    }
    #[cfg(any(feature = "app-hub", native_mobile))]
    crate::agent_events::install_guidance(&app.id)?;
    let context = crate::ai_host::contained::conversation_for_account(&app.id, instance, account)?;
    set_prepared(&app.id, Some(Prepared::Ready));
    Ok(context)
}

#[cfg(kernel)]
fn native_conversation(app: &AgentApp, instance: &str) -> Result<Arc<dyn OctosContext>, String> {
    use crate::ai_host::app_peers::{ContextSpec, OctosAppService};
    let broker = crate::ai_host::app_peers::broker::live(&app.id).ok_or_else(|| format!("Open {} first: its assistant runs while it is open", app.name))?;
    let account = broker.account().ok_or_else(|| format!("Sign in to {} to use its assistant", app.name))?;
    let services = broker.services();
    broker.open_conversation(ContextSpec { account, instance: instance.to_string(), services })
}

#[cfg(not(kernel))]
fn native_conversation(app: &AgentApp, _instance: &str) -> Result<Arc<dyn OctosContext>, String> {
    Err(format!("{}'s assistant is not available in this build", app.name))
}

/// Ask the person to allow `app`'s agent: the first-use sheet, once (only
/// the person answers it). The state it is in now.
pub fn ask(app: &AgentApp) -> Access {
    if access(&app.id) == Access::Allowed {
        return Access::Allowed;
    }
    let summary = crate::approvals::consent::AgentSummary::from_manifest(&app.id, &app.name, &app.manifest, &app.octos, "The model set in AI providers");
    match crate::approvals::consent_ask(summary) {
        crate::approvals::consent::State::Allowed => Access::Allowed,
        _ => access(&app.id),
    }
}

// ------------------------------------------------------------ the system agent

/// The kernel's slug of `app`'s peer, once it is bound: what the system
/// agent's `peer_list` shows and `peer_send_input` takes (`os-news-22a12f90`,
/// never the app id `os.news`).
pub fn peer_slug(app: &AgentApp) -> Option<String> {
    if !app.native {
        return crate::ai_host::contained::peer_slug(&app.id);
    }
    native_slug(&app.id)
}

#[cfg(kernel)]
fn native_slug(app: &str) -> Option<String> {
    crate::ai_host::app_peers::broker::live(app)?.peer().map(|(slug, _)| slug)
}

#[cfg(not(kernel))]
fn native_slug(_app: &str) -> Option<String> {
    None
}

/// How the system agent reaches an allowed app's agent: by its peer slug
/// with `peer_send_input`, never by the app id.
fn reach(app: &AgentApp, slug: Option<&str>) -> String {
    match slug {
        Some(slug) => format!("Reach it with peer_send_input and the peer slug \"{slug}\" (not the app id {})", app.id),
        None => format!(
            "Find its peer slug with peer_list (the peer named \"{} \u{2026}\"), then use that slug with peer_send_input (not the app id {})",
            peer_label(app),
            app.id
        ),
    }
}

/// One app's line for the system agent: its name, its peer and where it
/// stands, and what to do.
pub(crate) fn line(app: &AgentApp) -> Value {
    let access = access(&app.id);
    let peer = match (access, prepared(&app.id)) {
        (Access::Allowed, Some(Prepared::Ready)) => "ready: in peer_list".to_string(),
        (Access::Allowed, Some(Prepared::Preparing)) => "starting".to_string(),
        (Access::Allowed, Some(Prepared::Failed(e))) => format!("could not start: {e}"),
        (Access::Allowed, None) if app.native => format!("runs while {} is open", app.name),
        (Access::Allowed, None) => "not started yet".to_string(),
        _ => "none".to_string(),
    };
    let slug = if access == Access::Allowed { peer_slug(app) } else { None };
    let what = match access {
        Access::Allowed => format!("{}.", reach(app, slug.as_deref())),
        Access::NotAsked => format!("{}'s assistant is not yet allowed. Call agents.ask to ask the person, or tell them to open {} and use Ask {}.", app.name, app.name, app.name),
        Access::Off => format!("{}'s assistant is off. Only the person can turn it on (Settings › Assistant › Approvals); tell them.", app.name),
    };
    json!({"app": app.id, "name": app.name, "access": access.as_str(), "peer": peer, "peer_slug": slug, "what_to_do": what})
}

/// The name the kernel gives the app's peer (`<label> <account tag>`): the
/// label is the peer's app id for a script app, the module's label for a
/// native one.
fn peer_label(app: &AgentApp) -> String {
    if app.native {
        app.name.clone()
    } else {
        app.id.clone()
    }
}

/// `agents.list`'s answer.
pub fn list() -> Value {
    json!({"apps": all().iter().map(line).collect::<Vec<_>>()})
}

/// One app's part of [`system_note`]: its name, its id and where it
/// stands, with its peer slug once allowed and bound.
pub(crate) fn note_part(app: &AgentApp) -> String {
    let state = match access(&app.id) {
        Access::Allowed => format!("allowed (in peer_list; {})", reach(app, peer_slug(app).as_deref())),
        Access::NotAsked => "not yet allowed (not in peer_list; say so, and call agents.ask or ask the person to allow it)".to_string(),
        Access::Off => "off (not in peer_list; only the person can turn it on in Settings)".to_string(),
    };
    format!("{} [{}]: {state}", app.name, app.id)
}

/// The note the system chat sends with a turn when the apps' agents
/// changed since it last told the system agent (`None`: nothing to say).
/// One line per app: its name and where it stands.
pub fn system_note() -> Option<String> {
    let apps = all();
    if apps.is_empty() {
        return None;
    }
    let parts: Vec<String> = apps.iter().map(note_part).collect();
    Some(format!("[OctoSense: apps with an agent: {}. peer_send_input takes a peer slug from peer_list, never an app id; never guess a peer that peer_list does not show.]", parts.join("; ")))
}

/// Strip [`system_note`] from a message's text (the transcript keeps it; the
/// person does not need to see it).
pub fn strip_note(text: &str) -> &str {
    match text.strip_prefix("[OctoSense: apps with an agent:") {
        Some(rest) => match rest.find("]\n") {
            Some(end) => rest[end + 2..].trim_start(),
            None => text,
        },
        None => text,
    }
}

/// The host tools for the system session.
pub fn declarations() -> Vec<Value> {
    let mut tools = vec![
        json!({
            "name": LIST_TOOL,
            "app": OWNER,
            "description": "List every app on this device that has an agent, whether the person allowed it, whether its peer is ready (in peer_list), and its peer slug (`peer_slug`: pass it to peer_send_input; the app id is not a peer). Use it before delegating to an app's agent, and when peer_list does not show the app you need.",
            "input_schema": {"type": "object", "properties": {}, "additionalProperties": false},
            "risk": "read",
        }),
        json!({
            "name": ASK_TOOL,
            "app": OWNER,
            "description": "Ask the person to allow one app's agent (the shell shows its first-use sheet; only the person answers). Use it when you need an app's agent that is not yet allowed. The call waits for their answer and for the agent's peer to start, then returns its peer slug: send the person's request to it with peer_send_input in this same turn. If they did not allow it, say so.",
            "input_schema": {"type": "object", "properties": {"app": {"type": "string", "description": "The app's name or id, e.g. News or os.news"}}, "required": ["app"], "additionalProperties": false},
            // The shell's own sheet asks the person, so the kernel holds the
            // call as long as an approval (`confirm: app` on a gated tool);
            // a `read` tool gets 30 s, and the person had not answered by
            // then in the live run. `outward`: it lets another app's agent
            // in, past the system agent.
            "risk": "act",
            "outward": true,
            "confirm": "app",
        }),
    ];
    #[cfg(any(feature = "app-hub", native_mobile))]
    tools.extend([
        json!({
            "name": PROVISION_TOOL, "app": OWNER,
            "description": "Configure instructions, skill text and incoming-email processing for an already allowed Mail agent. Use only when the person requests this automation. Bound to Mail's current signed-in account; does not grant access, tools or credentials. The initial inbox sync establishes a baseline; only subsequent new mail triggers the agent. Runs at the configured interval while OctoSense is active; Android also schedules quiet periodic background jobs (15-minute period, subject to OS delays). Set enabled=false to stop. Instructions and skills replace the previous host provision, supplementing the app's admitted base guidance.",
            "input_schema": {"type":"object","properties":{
                "app":{"type":"string","enum":["os.mail"]},
                "enabled":{"type":"boolean"},
                "instructions":{"type":"string","maxLength":8192},
                "skills":{"type":"array","maxItems":8,"items":{"type":"object","properties":{"name":{"type":"string","maxLength":64},"text":{"type":"string","maxLength":8192}},"required":["name","text"],"additionalProperties":false}},
                "poll_interval_secs":{"type":"integer","minimum":30,"maximum":3600}
            },"required":["app","enabled","instructions","skills"],"additionalProperties":false},
            "risk":"act", "outward":false
        }),
        json!({
            "name": STATUS_TOOL, "app": OWNER,
            "description":"Read Mail's current background configuration, poll state and event processing receipts. Reports actual host state without exposing message bodies or credentials.",
            "input_schema":{"type":"object","properties":{"app":{"type":"string","enum":["os.mail"]}},"required":["app"],"additionalProperties":false},
            "risk":"read"
        }),
    ]);
    tools
}

/// Whether `tool` is one of [`declarations`].
pub fn is_agents_tool(tool: &str) -> bool {
    tool == LIST_TOOL || tool == ASK_TOOL || cfg!(any(feature = "app-hub", native_mobile)) && matches!(tool, PROVISION_TOOL | STATUS_TOOL)
}

/// Answer the system agent's call of one of [`declarations`] (the system
/// chat, on the UI thread).
pub fn call(tool: &str, args: &Value) -> ToolOutcome {
    match tool {
        LIST_TOOL => ToolOutcome::Ok(list()),
        #[cfg(any(feature = "app-hub", native_mobile))]
        PROVISION_TOOL => match crate::agent_events::provision(args.clone()) {
            Ok(value) => ToolOutcome::Ok(value),
            Err(error) => ToolOutcome::error("agent_provision", error),
        },
        #[cfg(any(feature = "app-hub", native_mobile))]
        STATUS_TOOL => match crate::agent_events::status(args.clone()) {
            Ok(value) => ToolOutcome::Ok(value),
            Err(error) => ToolOutcome::error("agent_status", error),
        },
        // Answered at once (the system chat holds the call instead, until
        // the person answered: system_chat `pump`).
        ASK_TOOL => match ask_app(args) {
            Ok(app) => {
                begin_ask(&app);
                ask_settled(&app).unwrap_or_else(|| ask_pending(&app))
            }
            Err(outcome) => outcome,
        },
        other => ToolOutcome::error("unknown_tool", format!("{other} is not an agents tool")),
    }
}

/// The app `agents.ask` names, or the error it answers.
pub fn ask_app(args: &Value) -> Result<AgentApp, ToolOutcome> {
    let name = args["app"].as_str().unwrap_or("");
    find(name).ok_or_else(|| {
        let names: BTreeSet<String> = all().into_iter().map(|a| a.name).collect();
        ToolOutcome::error("no_such_agent", format!("No app with an agent is called {name:?}. Apps with an agent: {}.", names.into_iter().collect::<Vec<_>>().join(", ")))
    })
}

/// `agents.ask` for `app`: the first-use sheet if the person has not
/// decided, and its peer started once allowed. Whether the sheet is up.
pub fn begin_ask(app: &AgentApp) -> bool {
    let now = ask(app);
    if now == Access::Allowed {
        prepare(app);
    }
    now == Access::NotAsked
}

/// `agents.ask`'s answer once there is one: the person allowed the agent
/// and its peer is ready (its slug to pass to peer_send_input) or could not
/// start, or the agent is off. None while the person has not answered the
/// first-use sheet, or the allowed agent's peer is still starting.
pub fn ask_settled(app: &AgentApp) -> Option<ToolOutcome> {
    let now = access(&app.id);
    let (text, slug) = match (now, prepared(&app.id)) {
        (Access::Off, _) => (format!("{}'s assistant is off: the person did not allow it. Only they can turn it on (Settings › Assistant › Approvals); tell them.", app.name), None),
        (Access::Allowed, _) if app.native => (format!("{}'s assistant is allowed. {}.", app.name, reach(app, peer_slug(app).as_deref())), peer_slug(app)),
        (Access::Allowed, Some(Prepared::Ready)) => {
            let slug = peer_slug(app)?;
            (format!("{}'s assistant is allowed and ready. {} now, in this turn.", app.name, reach(app, Some(&slug))), Some(slug))
        }
        (Access::Allowed, Some(Prepared::Failed(e))) => (format!("{}'s assistant is allowed but could not start: {e}", app.name), None),
        _ => return None,
    };
    Some(ToolOutcome::Ok(json!({"app": app.id, "name": app.name, "access": now.as_str(), "peer_slug": slug, "text": text})))
}

/// `agents.ask`'s answer while the person has not answered, or the peer
/// has not started (a call that waited too long).
pub fn ask_pending(app: &AgentApp) -> ToolOutcome {
    let now = access(&app.id);
    let text = match now {
        Access::Allowed => format!("{}'s assistant is allowed. Its peer is still starting; it shows in peer_list shortly (use its peer slug from peer_list or agents.list with peer_send_input, not the app id).", app.name),
        Access::NotAsked => format!("{}'s assistant is not yet allowed: the person has not answered the first-use sheet. Tell them it is waiting for them; its peer shows in peer_list once they allow it.", app.name),
        Access::Off => format!("{}'s assistant is off. Only the person can turn it on (Settings › Assistant › Approvals).", app.name),
    };
    ToolOutcome::Ok(json!({"app": app.id, "name": app.name, "access": now.as_str(), "peer_slug": peer_slug(app), "text": text}))
}
