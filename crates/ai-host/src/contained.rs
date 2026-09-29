//! `octos` for contained apps: the Card runner's assistant service.
//!
//! A store app (an App Hub bundle the Card runner hosts) that declares
//! `octos.*` services calls `host.request("octos.turn.start", {text}, …)`.
//! The isolate's gate already refused every service the manifest did not
//! declare; what reaches this service is handed to ONE octos peer per app,
//! owned by the shell's system agent on the shell's kernel — the same
//! host-owned peer contract Rinx uses (ADR 0007), named `card.<app id>` so a
//! store app can never share a native module's peer or memory.
//!
//! The app and its cards talk in the peer's ONE shared conversation (ADR
//! 0004 §6, octos#2626): the peer's own session, which the system agent
//! drives too. A turn is the person's (`origin: person`, labelled with the
//! app) unless the app says it started the run itself (`trigger: app`);
//! `octos.session.history` is the peer's transcript, with each message's
//! speaker. A script app gets no pushed events: it reads the whole
//! conversation, the system agent's turns included, with history.
//!
//! What an app sends is input text (and what started the turn) only. It
//! never names a session, profile, workspace or provider, and it cannot
//! decide a tool approval: the shell's approval router draws every one
//! (ADR 0004 §8).
//!
//! **Consent and grants** (ADR 0004 §4). The service follows
//! [`crate::Policy::contained_gate`]: behind the person's consent at first
//! use (the shipped default), off, or on for every app (the developer
//! override `OCTOSENSE_CONTAINED_APPS=1`). An app's peer is granted only
//! the `octos.*` services its manifest declares ([`declared`]), and turning
//! its agent off in Settings releases the live peer at once ([`revoke`]).
//!
//! The logic does not need a kernel: peers come from a [`PeerFactory`]. The
//! shell's factory (`cfg(kernel)`) launches them through
//! `octosense_app_peers::hosted`; tests use a fake one.

use octosense_app_peers::{ContextEvent, ContextOp, ContextSpec, EventSink, OctosAppService, OctosContext, TurnTrigger};
use octosense_appstore::services::{HostService, Replier, ServiceCall, ServiceHost};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, Weak};

/// A contained app's peer is `card.<app id>`; no native module id starts so.
pub const PEER_PREFIX: &str = "card.";
/// The Card runner has no accounts: every app's peer acts for the device.
pub const ACCOUNT: &str = "device";
/// The longest turn text an app may send.
pub const MAX_TEXT_BYTES: usize = 32 * 1024;
/// The largest reply delivered to an app (serialized JSON).
pub const MAX_REPLY_BYTES: usize = 2 * 1024 * 1024;
/// The longest peer id the kernel's memory namespace takes.
const MAX_PEER_ID: usize = 64;

pub const TURNED_OFF: &str = "The assistant is turned off for apps on this device";
pub const UNAVAILABLE: &str = "The assistant is not available on this device";
pub const UNSUPPORTED_ARGS: &str = "Unsupported Octos arguments";
pub const BAD_TEXT: &str = "Provide text (at most 32 KiB)";
pub const NO_SHEET: &str = "The assistant has no sheet; octos calls come from the app";
pub const NO_CONSENT: &str = "Waiting for the person to allow this app's agent (OctoSense asks the first time)";
pub const NOT_DECLARED: &str = "This app's manifest does not declare that assistant service";

/// The shell's consent at first use (ADR 0004 §4): whether an app may have
/// its agent now; asking the person the first time is the shell's part.
/// Unset (a host without a consent surface, and this crate's tests): no
/// gate beyond the policy switch.
static CONSENT: std::sync::OnceLock<fn(&str) -> bool> = std::sync::OnceLock::new();

/// The shell installs its consent check once, at startup.
pub fn set_consent(check: fn(&str) -> bool) {
    let _ = CONSENT.set(check);
}

/// The `octos.*` services an app's manifest declares (`None`: the shell
/// knows no such app). Unset (this crate's tests): all of them.
static DECLARED: std::sync::OnceLock<fn(&str) -> Option<BTreeSet<String>>> = std::sync::OnceLock::new();

/// The shell installs its manifest lookup once, at startup.
pub fn set_declared(lookup: fn(&str) -> Option<BTreeSet<String>>) {
    let _ = DECLARED.set(lookup);
}

/// What `app_id` may be granted: the `octos.*` services its manifest
/// declares, and only those (ADR 0004 §4).
pub fn declared(app_id: &str) -> Result<BTreeSet<String>, String> {
    let all = || octosense_app_peers::OCTOS_SERVICES.iter().map(|s| s.to_string()).collect::<BTreeSet<String>>();
    let services = match DECLARED.get() {
        None => all(),
        Some(lookup) => lookup(app_id).ok_or(NOT_DECLARED)?.intersection(&all()).cloned().collect(),
    };
    if services.is_empty() {
        return Err(NOT_DECLARED.into());
    }
    Ok(services)
}

/// Every contained app's live peer, by app id, so turning its agent off
/// revokes it at once ([`revoke`]).
static LIVE: Mutex<Option<HashMap<String, Arc<dyn OctosAppService>>>> = Mutex::new(None);

fn live<R>(f: impl FnOnce(&mut HashMap<String, Arc<dyn OctosAppService>>) -> R) -> R {
    f(LIVE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new))
}

/// The person turned `app_id`'s agent off (Settings): its peer is released
/// now, closing its contexts and any running turn. A later call needs
/// consent again and then gets a fresh peer. True when one was live.
pub fn revoke(app_id: &str) -> bool {
    let service = live(|l| l.remove(app_id));
    match service {
        Some(service) => {
            service.release();
            true
        }
        None => false,
    }
}

/// Where contained apps' peers come from.
pub trait PeerFactory: Send + Sync {
    /// The scoped assistant service for `app_id`'s peer `peer_id`, with
    /// exactly `services` (its manifest's `octos.*`), or `None` when this
    /// device cannot give one.
    fn launch(&self, peer_id: &str, app_id: &str, services: &BTreeSet<String>) -> Option<Arc<dyn OctosAppService>>;
}

/// The peer id of `app_id`, or why it cannot name one.
pub fn peer_id(app_id: &str) -> Result<String, String> {
    let id = format!("{PEER_PREFIX}{app_id}");
    if namespace_segment(&id) {
        Ok(id)
    } else {
        Err(format!(
            "The app id {app_id:?} cannot name an assistant peer (lowercase letters, digits, '.', '_' or '-', at most {} characters)",
            MAX_PEER_ID - PEER_PREFIX.len()
        ))
    }
}

/// The kernel's memory-namespace segment rule (`octosense_app_peers::hosted`).
fn namespace_segment(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_PEER_ID
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-'))
}

/// The operation an app's call asks for. Only `text` for a turn, and what
/// started it (`trigger`, `from`: [`TurnTrigger::from_args`]; left out, the
/// turn counts as unknown); nothing for the rest.
pub fn parse(service: &str, args: &Value) -> Result<ContextOp, String> {
    let allowed: &[&str] = match service {
        "octos.turn.start" => &["text", "trigger", "from"],
        "octos.session.open" | "octos.session.history" | "octos.turn.interrupt" => &[],
        other => return Err(format!("Unknown Octos service {other}")),
    };
    if args.as_object().is_none_or(|o| o.keys().any(|k| !allowed.contains(&k.as_str()))) {
        return Err(UNSUPPORTED_ARGS.into());
    }
    Ok(match service {
        "octos.turn.start" => ContextOp::TurnFrom {
            text: args["text"]
                .as_str()
                .filter(|s| !s.trim().is_empty() && s.len() <= MAX_TEXT_BYTES)
                .ok_or(BAD_TEXT)?
                .to_owned(),
            trigger: TurnTrigger::from_args(args),
        },
        "octos.session.open" => ContextOp::Open,
        "octos.session.history" => ContextOp::History,
        _ => ContextOp::Interrupt,
    })
}

struct AppPeer {
    peer: String,
    service: Arc<dyn OctosAppService>,
    context: Option<Arc<dyn OctosContext>>,
    generation: u64,
}

/// The `octos` host service for the Card runner's apps.
pub struct ContainedOctos {
    gate: crate::ContainedGate,
    factory: Arc<dyn PeerFactory>,
    apps: HashMap<String, AppPeer>,
}

impl ContainedOctos {
    /// `enabled`: on behind consent, or off ([`crate::Policy::contained_apps`]).
    pub fn new(enabled: bool, factory: Arc<dyn PeerFactory>) -> Self {
        Self::gated(if enabled { crate::ContainedGate::Consent } else { crate::ContainedGate::Off }, factory)
    }

    /// With the policy's gate ([`crate::Policy::contained_gate`]).
    pub fn gated(gate: crate::ContainedGate, factory: Arc<dyn PeerFactory>) -> Self {
        ContainedOctos { gate, factory, apps: HashMap::new() }
    }

    /// The app's handle on its peer's shared conversation, creating the
    /// peer and (re)opening the handle as needed.
    fn context_for(&mut self, app_id: &str, services: &BTreeSet<String>) -> Result<Arc<dyn OctosContext>, String> {
        // A peer revoked since (Settings turned the agent off) is gone.
        if self.apps.contains_key(app_id) && !live(|l| l.contains_key(app_id)) {
            self.apps.remove(app_id);
        }
        if !self.apps.contains_key(app_id) {
            let peer = peer_id(app_id)?;
            let service = self.factory.launch(&peer, app_id, services).ok_or(UNAVAILABLE)?;
            service.set_account(Some(ACCOUNT));
            live(|l| l.insert(app_id.to_owned(), service.clone()));
            self.apps.insert(app_id.to_owned(), AppPeer { peer, service, context: None, generation: 0 });
        }
        let app = self.apps.get_mut(app_id).expect("inserted above");
        if let Some(context) = app.context.as_ref().filter(|c| c.is_open()) {
            return Ok(context.clone());
        }
        app.generation += 1;
        let context = app.service.open_conversation(ContextSpec {
            account: ACCOUNT.to_owned(),
            instance: format!("{}-g{}", app.peer, app.generation),
            services: app.service.services(),
        })?;
        app.context = Some(context.clone());
        Ok(context)
    }
}

/// One call's answer: sent exactly once.
type Once = Arc<Mutex<Option<Replier>>>;

fn answer(once: &Once, result: Result<Value, String>) {
    if let Some(reply) = once.lock().unwrap_or_else(|e| e.into_inner()).take() {
        reply.send(result);
    }
}

/// The reply an app receives: the kernel's result, with the tools declined
/// during the call, bounded.
fn finish(result: Result<Value, String>, denied: &[String]) -> Result<Value, String> {
    let mut value = result?;
    if !denied.is_empty() {
        if let Some(object) = value.as_object_mut() {
            object.insert("denied_approvals".into(), denied.iter().cloned().map(Value::String).collect());
        }
    }
    if value.to_string().len() > MAX_REPLY_BYTES {
        return Err(format!("The assistant's reply is over MAX_REPLY_BYTES ({MAX_REPLY_BYTES} bytes)"));
    }
    Ok(value)
}

/// Where one call's events go: the completion is the app's answer,
/// streamed text is dropped (the answer carries it).
///
/// Approvals: the broker hands every approval of the app's peer and
/// contexts to the shell's approval router (ADR 0004 §8), and this context
/// only hears `approval/handled_by_host`, which needs nothing from here. A
/// raw `approval/requested` reaches this sink only when no host routes
/// approvals (a host without a router): a script app draws no sheet of its
/// own, so that one is declined rather than left to time out.
fn sink(context: Weak<dyn OctosContext>, once: Once) -> EventSink {
    let denied: Arc<Mutex<Vec<String>>> = Arc::default();
    Arc::new(move |event| match event {
        ContextEvent::Data(data) => {
            if data["method"] != "approval/requested" {
                return;
            }
            let params = &data["params"];
            let Some(id) = params["approval_id"].as_str().map(str::to_owned) else { return };
            let title = params["title"].as_str().unwrap_or("a tool").to_owned();
            denied.lock().unwrap_or_else(|e| e.into_inner()).push(title);
            // Declined off this thread: the context may be delivering this
            // event under its own lock.
            if let Some(context) = context.upgrade() {
                std::thread::spawn(move || {
                    let _ = context.call(ContextOp::Approval { id, approve: false }, Arc::new(|_| {}));
                });
            }
        }
        ContextEvent::Complete(result) => {
            let denied = denied.lock().unwrap_or_else(|e| e.into_inner()).clone();
            answer(&once, finish(result, &denied));
        }
    })
}

impl HostService for ContainedOctos {
    fn family(&self) -> &'static str {
        "octos"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        if call.from_sheet {
            return reply.send(Err(NO_SHEET.into()));
        }
        match self.gate {
            crate::ContainedGate::Off => return reply.send(Err(TURNED_OFF.into())),
            // The developer override asks nobody.
            crate::ContainedGate::Everyone => {}
            crate::ContainedGate::Consent => {
                if CONSENT.get().is_some_and(|granted| !granted(&call.app_id)) {
                    return reply.send(Err(NO_CONSENT.into()));
                }
            }
        }
        let op = match parse(&call.service, &call.args) {
            Ok(op) => op,
            Err(e) => return reply.send(Err(e)),
        };
        // Only what the manifest declares, whatever the isolate let through.
        let services = match declared(&call.app_id) {
            Ok(s) if s.contains(&call.service) => s,
            Ok(_) => return reply.send(Err(NOT_DECLARED.into())),
            Err(e) => return reply.send(Err(e)),
        };
        let context = match self.context_for(&call.app_id, &services) {
            Ok(context) => context,
            Err(e) => return reply.send(Err(e)),
        };
        let once: Once = Arc::new(Mutex::new(Some(reply)));
        if let Err(e) = context.call(op, sink(Arc::downgrade(&context), once.clone())) {
            answer(&once, Err(e));
        }
    }
}

/// The shell's peers: host-owned app peers on the shell's kernel.
#[cfg(kernel)]
pub(crate) struct KernelPeers;

#[cfg(kernel)]
impl PeerFactory for KernelPeers {
    fn launch(&self, peer_id: &str, app_id: &str, services: &BTreeSet<String>) -> Option<Arc<dyn OctosAppService>> {
        // Only the manifest's `octos.*` services, never all of them.
        let services: Vec<&str> = services.iter().map(String::as_str).collect();
        crate::host_policy().allow(peer_id, services.iter().copied());
        let broker = octosense_app_peers::hosted::launch(peer_id, app_id, services.iter().copied(), crate::host_policy())?;
        Some(Arc::new(broker))
    }
}

#[cfg(test)]
mod tests;
