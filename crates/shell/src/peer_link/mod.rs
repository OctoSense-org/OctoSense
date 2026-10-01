//! The peer link (ADR 0004 §5): a native app's own channel to ITS octos
//! app agent, under the `"octos_peer"` envelope ([`wire`]); never
//! registered with the AI services bus. A process app's link rides its hub
//! socket; an in-process module's is the channel Makepad's `OctosPeer::open`
//! parks, which `module_host` claims for the instance that opened it and
//! serves here as the same frames ([`module_connected`], [`on_module_frame`];
//! `octosense_ai_host::module_peer`). One code path: an app does not know
//! how it is hosted.
//!
//! - **Identity** is the socket's: the shell launched that process for one
//!   app (its client slot), and every frame is that app's. A tool call's
//!   account, context and client come from the shell's own records of the
//!   contexts the process opened ([`link::ContextOwner`]); `caller` comes
//!   from the relay. A process may use only contexts it opened.
//! - **Grants and consent**: only an app whose `native-apps.json` entry
//!   grants `agent.octos` services gets a link, and each request needs
//!   `consent::granted(app)` (the first-use sheet asks, once).
//! - **Requests** (`octos.session.open|history`, `octos.turn.start|interrupt`,
//!   `octos.context.close`) run on the app's one peer through the same
//!   service in-process modules get (`crates/app-peers`' broker), by an
//!   adapter ([`ShellHost::service`]); that broker registers the app's tools
//!   and hands its `peer/tool/call`s to the relay (`crate::host_tools`).
//! - **Tool calls** (kernel → app) enter at [`tool_call`] and their outcome
//!   leaves through the installed [`ToolRelay`]; confirmations go through
//!   the #120 router. The host obligations are kept here: once per call,
//!   nothing after cancel, and an acknowledgement before any confirmation
//!   sheet (the kernel's, and the app's own `awaiting_confirmation` before a
//!   `confirm: app` result).
//! - **A process that dies** fails its outstanding calls (`outcome_unknown`
//!   unless they only read), closes its request contexts and keeps the peer.
//!
//! The relay (`crate::host_tools`, octos#2567) uses [`set_tool_relay`],
//! [`tool_call`], [`tool_cancel`] and [`has_link`]; a call the kernel already
//! approved comes with `approved` (the link asks nobody again), a `confirm:
//! app` one with `confirm_required`. [`context_owner`] and
//! [`close_account`] are the sign-out seams.

pub mod link;
pub mod wire;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use serde_json::Value;

pub use link::{ContextOwner, FrameOut, KernelToolCall, PeerHost, PeerLinks, RecordingToolRelay, Refused, ToolCallResult, ToolRelay};
pub use wire::Risk;

use crate::ai_host::app_peers::OctosAppService;
use crate::approvals::{Caller, Decision, RequestContext, RequestId, Route, ToolSpec};
use crate::hub::ClientId;
use crate::native_apps::Confirm;

/// The shell's decisions for the link: the manifest, consent, the router
/// and the app's peer.
pub struct ShellHost;

/// The `confirm: app` hand-off: the router tells the app's sheet through
/// the link, which sends the call with `confirm_required` itself.
struct LinkConfirm;

impl crate::approvals::AppConfirm for LinkConfirm {
    fn confirm(&mut self, _request: &crate::approvals::AppConfirmRequest) {}
}

impl PeerHost for ShellHost {
    fn granted(&self, app: &str) -> BTreeSet<String> {
        let Some(entry) = crate::native_apps::find(app) else { return BTreeSet::new() };
        if entry.octos.is_empty() && !crate::dev_mode::grants_all(app) {
            return BTreeSet::new();
        }
        if crate::dev_mode::grants_all(app) {
            return crate::ai_host::app_peers::OCTOS_SERVICES.iter().map(|s| s.to_string()).collect();
        }
        entry.octos.iter().map(|s| s.to_string()).collect()
    }
    fn keeps_accounts(&self, app: &str) -> bool {
        crate::native_apps::find(app).is_some_and(|a| a.accounts)
    }
    fn consent(&mut self, app: &str) -> bool {
        let caps: Vec<String> = self.granted(app).into_iter().collect();
        let caps: Vec<&str> = caps.iter().map(String::as_str).collect();
        let label = crate::approvals::sheet::app_label(app);
        crate::approvals::with(|a| crate::approvals::module_gate(a, app, &label, &caps)).unwrap_or(false)
    }
    #[allow(unused_variables)]
    fn service(&mut self, app: &str, services: &BTreeSet<String>) -> Option<Arc<dyn OctosAppService>> {
        #[cfg(feature = "octos-core")]
        {
            let policy = crate::ai_host::app_peers::hosted::HostPolicy::default();
            policy.allow(app, services.iter().map(String::as_str));
            let label = crate::approvals::sheet::app_label(app);
            let broker = crate::ai_host::app_peers::hosted::launch(app, &label, services.iter().map(String::as_str), &policy)?;
            Some(Arc::new(broker) as Arc<dyn OctosAppService>)
        }
        #[cfg(not(feature = "octos-core"))]
        None
    }
    fn tool_rule(&self, app: &str, tool: &str) -> Option<(Confirm, bool)> {
        crate::native_apps::find(app).and_then(|a| a.tool(tool)).map(|r| (r.confirm, r.auto_approvable))
    }
    fn request_approval(&mut self, app: &str, tool: ToolSpec, args: Value, caller: Caller, context: RequestContext) -> Route {
        crate::approvals::approval_requested(app, tool, args, caller, context)
    }
    fn take_decisions(&mut self) -> Vec<(RequestId, Decision, String)> {
        crate::approvals::take_peer_decisions()
    }
    fn app_confirm_answered(&mut self, id: &RequestId, approved: bool, reason: &str) {
        let _ = crate::approvals::app_confirm_answered(id, approved, reason);
    }
    fn link_opened(&mut self, app: &str) {
        crate::approvals::register_app_confirm(app, Box::new(LinkConfirm));
    }
    fn link_closed(&mut self, app: &str) {
        crate::approvals::unregister_app_confirm(app);
    }
}

/// Until the relay is installed, outcomes queue here and are handed over.
struct Queued(RecordingToolRelay);

impl ToolRelay for Queued {
    fn acknowledged(&mut self, app: &str, call_id: &str) {
        self.0.acknowledged(app, call_id)
    }
    fn finished(&mut self, app: &str, call_id: &str, result: ToolCallResult) {
        self.0.finished(app, call_id, result)
    }
}

static LINKS: Mutex<Option<(PeerLinks, RecordingToolRelay)>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut PeerLinks) -> R) -> R {
    let mut guard = LINKS.lock().unwrap_or_else(|e| e.into_inner());
    let (links, _) = guard.get_or_insert_with(|| {
        let queue = RecordingToolRelay::default();
        (PeerLinks::new(Box::new(ShellHost), Box::new(Queued(queue.clone()))), queue)
    });
    let out = f(links);
    for line in links.log.drain(..) {
        makepad_widgets::log!("{line}");
    }
    out
}

/// A frame sender for a client socket.
pub fn frame_out(sender: std::sync::mpsc::Sender<Vec<u8>>) -> FrameOut {
    let sender = Mutex::new(sender);
    Arc::new(move |json: String| {
        let sender = sender.lock().unwrap_or_else(|e| e.into_inner());
        crate::hub::send_to_app(&sender, vec![makepad_studio_protocol::StudioToApp::Custom(json)]);
    })
}

// ------------------------------------------------------ the shell's hooks

/// A client connected its hub socket (`lib.rs`, `HubEvent::Connected`).
pub fn connected(client: ClientId, app: &str, sender: std::sync::mpsc::Sender<Vec<u8>>) {
    with(|l| l.connected(client, app, frame_out(sender)));
}

/// A `Custom` frame from a client: true when it was the peer link's.
pub fn on_frame(client: ClientId, app: &str, frame: &str, sender: Option<std::sync::mpsc::Sender<Vec<u8>>>) -> bool {
    if !wire::is_peer_frame(frame) {
        return false;
    }
    with(|l| l.on_frame(client, app, frame, sender.map(frame_out)))
}

/// An in-process module instance opened Makepad's peer client
/// (`OctosPeer::open`; `module_host`, #142): the same link as a process's
/// socket, with `out` writing to the instance's channel. `client` is the
/// instance's own client id and `app` its module id, never a claim of the
/// module.
/// False when the link is refused (no granted agent, or the instance
/// already holds one).
pub fn module_connected(client: ClientId, app: &str, out: FrameOut) -> bool {
    with(|l| l.connected(client, app, out))
}

/// A frame an in-process instance sent on its link (see [`on_frame`]).
pub fn on_module_frame(client: ClientId, app: &str, frame: &str, out: FrameOut) -> bool {
    with(|l| l.on_frame(client, app, frame, Some(out)))
}

/// The client's process died or its socket closed; for an in-process
/// instance, it closed or failed.
pub fn process_gone(client: ClientId) {
    with(|l| l.process_gone(client));
}

/// Once a second, with the approvals tick.
pub fn tick() {
    let now = crate::host::now();
    with(|l| l.tick(now));
}

// ------------------------------------------------ the seams for home-96

/// Install octos#2567's relay; outcomes queued before are handed over.
pub fn set_tool_relay(mut relay: Box<dyn ToolRelay>) {
    let mut guard = LINKS.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        drop(guard);
        with(|_| ());
        guard = LINKS.lock().unwrap_or_else(|e| e.into_inner());
    }
    if let Some((links, queue)) = guard.as_mut() {
        for (app, call, result) in queue.take() {
            match result {
                None => relay.acknowledged(&app, &call),
                Some(r) => relay.finished(&app, &call, r),
            }
        }
        links.set_relay(relay);
    }
}

/// A kernel `peer/tool/call` for a process app.
pub fn tool_call(app: &str, call: KernelToolCall) -> Result<(), Refused> {
    let now = crate::host::now();
    with(|l| l.tool_call(app, call, now))
}

/// The kernel cancelled a call.
pub fn tool_cancel(app: &str, call_id: &str) {
    with(|l| l.tool_cancel(app, call_id));
}

/// Who owns a kernel request context of `app` (account and client).
pub fn context_owner(app: &str, kernel_context_id: &str) -> Option<ContextOwner> {
    with(|l| l.context_owner(app, kernel_context_id))
}

/// Whether a process of `app` holds a peer link now.
pub fn has_link(app: &str) -> bool {
    with(|l| l.has_link(app))
}

/// The person turned `app`'s agent off: its contexts close and its service
/// is released.
pub fn revoke(app: &str) {
    with(|l| l.revoke(app));
}

/// Signing out (§11): close `account`'s contexts of `app`.
pub fn close_account(app: &str, account: &str) {
    with(|l| l.close_account(app, account, "signed_out"));
}
