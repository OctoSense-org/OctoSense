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
//! What an app sends is input text only. It never names a session, profile,
//! workspace or provider, and it cannot decide a tool approval: the Card
//! runner has no approval sheet yet, so every approval the peer raises is
//! declined here and listed in the reply's `denied_approvals`.
//!
//! The logic does not need a kernel: peers come from a [`PeerFactory`]. The
//! shell's factory (`cfg(kernel)`) launches them through
//! `octosense_app_peers::hosted`; tests use a fake one.

use octosense_app_peers::{ContextEvent, ContextOp, ContextSpec, EventSink, OctosAppService, OctosContext};
use octosense_appstore::services::{HostService, Replier, ServiceCall, ServiceHost};
use serde_json::Value;
use std::collections::HashMap;
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

/// Where contained apps' peers come from.
pub trait PeerFactory: Send + Sync {
    /// The scoped assistant service for `app_id`'s peer `peer_id`, or `None`
    /// when this device cannot give one.
    fn launch(&self, peer_id: &str, app_id: &str) -> Option<Arc<dyn OctosAppService>>;
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

/// The operation an app's call asks for. Only `text` for a turn; nothing for
/// the rest.
pub fn parse(service: &str, args: &Value) -> Result<ContextOp, String> {
    let allowed: &[&str] = match service {
        "octos.turn.start" => &["text"],
        "octos.session.open" | "octos.session.history" | "octos.turn.interrupt" => &[],
        other => return Err(format!("Unknown Octos service {other}")),
    };
    if args.as_object().is_none_or(|o| o.keys().any(|k| !allowed.contains(&k.as_str()))) {
        return Err(UNSUPPORTED_ARGS.into());
    }
    Ok(match service {
        "octos.turn.start" => ContextOp::Turn {
            text: args["text"]
                .as_str()
                .filter(|s| !s.trim().is_empty() && s.len() <= MAX_TEXT_BYTES)
                .ok_or(BAD_TEXT)?
                .to_owned(),
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
    enabled: bool,
    factory: Arc<dyn PeerFactory>,
    apps: HashMap<String, AppPeer>,
}

impl ContainedOctos {
    /// `enabled` is the shell's switch ([`crate::Policy::contained_apps`]).
    pub fn new(enabled: bool, factory: Arc<dyn PeerFactory>) -> Self {
        ContainedOctos { enabled, factory, apps: HashMap::new() }
    }

    /// The app's open request context, creating its peer and (re)opening
    /// the context as needed.
    fn context_for(&mut self, app_id: &str) -> Result<Arc<dyn OctosContext>, String> {
        if !self.apps.contains_key(app_id) {
            let peer = peer_id(app_id)?;
            let service = self.factory.launch(&peer, app_id).ok_or(UNAVAILABLE)?;
            service.set_account(Some(ACCOUNT));
            self.apps.insert(app_id.to_owned(), AppPeer { peer, service, context: None, generation: 0 });
        }
        let app = self.apps.get_mut(app_id).expect("inserted above");
        if let Some(context) = app.context.as_ref().filter(|c| c.is_open()) {
            return Ok(context.clone());
        }
        app.generation += 1;
        let context = app.service.open_context(ContextSpec {
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

/// Where one call's events go: approvals are declined, the completion is
/// the app's answer, streamed text is dropped (the answer carries it).
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
        if !self.enabled {
            return reply.send(Err(TURNED_OFF.into()));
        }
        let op = match parse(&call.service, &call.args) {
            Ok(op) => op,
            Err(e) => return reply.send(Err(e)),
        };
        let context = match self.context_for(&call.app_id) {
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
    fn launch(&self, peer_id: &str, app_id: &str) -> Option<Arc<dyn OctosAppService>> {
        let services = octosense_app_peers::OCTOS_SERVICES;
        crate::host_policy().allow(peer_id, services);
        let broker = octosense_app_peers::hosted::launch(peer_id, app_id, services, crate::host_policy())?;
        Some(Arc::new(broker))
    }
}

#[cfg(test)]
mod tests;
