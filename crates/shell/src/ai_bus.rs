//! The window manager's half of the AI services bus.
//!
//! The aichat child (the pane) is one client; every other client's AI
//! service reaches it through here. The WM parses only the envelope and
//! the routing fields, never the tools:
//!
//! - an up-frame from client C is stamped `from = endpoint(C)` (the WM's
//!   own id for that client — never the sender's claim) and forwarded to
//!   the pane; the last `Register` from each client is remembered so the
//!   pane gets a REPLAY of every registration when it (re)connects;
//! - a down-frame from the pane names a target endpoint: it goes to that
//!   client's socket, or, for the WM's own `os` endpoint, is answered here;
//! - a client that dies produces a synthetic `Unregister` for the pane;
//! - the pane sliding in or out is broadcast as `ChatOpen` to every
//!   registered client, so an app's own embedded chat can step aside.
//!
//! The `os` service is the WM as an app: list, launch, focus, close, and
//! open — a file in its associated app, through the same typed
//! `OpenRequest` a file browser's double-click takes.
//!
//! Developer mode (`dev_mode.rs`, ADR 0004 §13) is consulted here, never
//! set. It overrides every approval of the apps it covers, so such an app is
//! announced to the pane with every tool approved in advance
//! ([`dev_approved`]: the pane confirms only `Destructive`, so those are sent
//! as `Act`, the Terminal's `run` included), and while it is on every call
//! the pane makes is audited, each approval it answered included
//! ([`audit_dev_call`]). When the mode changes the shell sends
//! [`AiBus::reannounce`] so the pane's view follows at once. The same two
//! functions serve the in-process leg (`pane_links.rs`).

use crate::hub::ClientId;
use makepad_ai_services::wire::*;
use makepad_strict_json as json;
use std::collections::{HashMap, HashSet};

/// `manifest` as the pane must see it: unchanged, or, when developer mode
/// answers this app's approvals (`approves`), with no tool left for the pane
/// to confirm. A tool the app confirms itself loses that claim here too:
/// developer mode overrides the app's sheet as well (the hand-off seam is
/// `dev_mode::overrides_app_confirm`).
pub fn dev_approved(manifest: &ServiceManifest, approves: bool) -> ServiceManifest {
    let mut manifest = manifest.clone();
    if approves {
        for tool in &mut manifest.tools {
            if tool.risk == Risk::Destructive {
                tool.risk = Risk::Act;
            }
            tool.self_confirm = None;
        }
    }
    manifest
}

/// What a pane call needed that developer mode answered: the pane's confirm
/// card, the app's own sheet, or nothing (`None`).
pub fn approval_answered(manifest: Option<&ServiceManifest>, tool: &str, approves: bool) -> Option<crate::dev_mode::ApprovalKind> {
    let def = manifest?.tool(tool)?;
    match (approves, def.risk, def.confirms_itself()) {
        (true, Risk::Destructive, true) => Some(crate::dev_mode::ApprovalKind::AppConfirm),
        (true, Risk::Destructive, false) => Some(crate::dev_mode::ApprovalKind::PaneConfirm),
        _ => None,
    }
}

/// While developer mode is on: log the call, and the approval it answered
/// (`manifest` is the app's own, as registered).
pub fn audit_dev_call(service: &str, manifest: Option<&ServiceManifest>, call: &ServiceCall, approves: bool) {
    crate::dev_mode::audit_tool_call(service, &call.tool, &call.args, "assistant");
    if let Some(kind) = approval_answered(manifest, &call.tool, approves) {
        crate::dev_mode::audit_auto_approval(service, &call.tool, &call.args, "assistant", kind);
    }
}

/// The WM's own service endpoint.
pub const OS_ENDPOINT: &str = "os";

/// How much of a call's arguments the pane's confirm card shows: its title
/// is the arguments with the outer braces trimmed, cut at this many bytes
/// (makepad-ai-services `EngineCore::card`).
pub const CARD_ARGS_BYTES: usize = 60;

/// The arguments as the confirm card shows them, when it shows them whole.
pub fn card_shows_in_full(args: &str) -> bool {
    args.trim().trim_start_matches('{').trim_end_matches('}').trim().len() <= CARD_ARGS_BYTES
}

/// The shell's rules for an app's assistant tools, from its native-apps.json
/// entry (`agent.tool_policy`, ADR 0004 §8, §10): a `confirm: host` tool is
/// confirmed by the shell's approval router (`approvals/`), not by the
/// pane's card. It is registered with no claim that the app confirms it and
/// as `Act`, so the pane does not ask a second time; each call is held
/// ([`Route::Approval`]) until the router answers: developer mode first,
/// then the person on the shell's sheet, which shows the exact arguments in
/// full, or a standing rule where the tool is `auto_approvable`. The
/// Terminal's `run` is not, so every typed command asks the person (outside
/// developer mode).
fn host_rules(app: &str) -> Option<&'static crate::native_apps::NativeApp> {
    crate::native_apps::find(app).filter(|entry| !entry.tools.is_empty())
}

fn host_confirmed(rules: Option<&'static crate::native_apps::NativeApp>, tool: &str) -> bool {
    host_rule(rules, tool).is_some()
}

fn host_rule(rules: Option<&'static crate::native_apps::NativeApp>, tool: &str) -> Option<&'static crate::native_apps::ToolPolicy> {
    rules?.tool(tool).filter(|rule| rule.confirm == crate::native_apps::Confirm::Host)
}

/// Tools that type a command for the person (ADR 0004 §10, §12): developer
/// mode answers them through `approves_command`.
const COMMAND_TOOLS: &[(&str, &str)] = &[("terminal", "run")];

/// A pane call to a `confirm: host` tool, held until the shell's approval
/// router answers it ([`AiBus::release`]).
#[derive(Clone, Debug, PartialEq)]
pub struct HeldCall {
    /// The router's request id: `bus:<endpoint>:<call id>`.
    pub key: String,
    /// The owning app (its native-apps.json id).
    pub app: String,
    pub tool: String,
    /// The exact arguments, as the pane sent them.
    pub args: String,
    pub auto_approvable: bool,
    pub command: bool,
}

/// The router's request ids for the bus start with this.
pub const HELD_PREFIX: &str = "bus:";

/// Apply an app's rules to the manifest it registers.
fn apply_rules(manifest: &mut ServiceManifest, rules: Option<&'static crate::native_apps::NativeApp>) {
    for tool in &mut manifest.tools {
        if host_confirmed(rules, &tool.name) {
            // The router confirms each call; the pane must not ask again.
            if tool.risk == Risk::Destructive {
                tool.risk = Risk::Act;
            }
            tool.self_confirm = None;
        }
    }
}

/// What the bus wants the WM to do with a frame.
pub enum Route {
    /// Send this JSON to that client's studio socket.
    ToClient(ClientId, String),
    /// Send this JSON to the pane client.
    ToPane(String),
    /// A call for the WM itself; answer with `os_reply`.
    Os(ServiceCall),
    /// A frame for an IN-PROCESS instance (a module the WM hosts itself):
    /// the host runs its executor and answers with `local_reply`.
    Local(ClientId, ServiceDown),
    /// A `confirm: host` call: ask the approval router, then
    /// [`AiBus::release`] it with the answer.
    Approval(HeldCall),
    /// The answer to one of the shell's own calls ([`AiBus::shell_call`]):
    /// for the host-tool relay (`host_tools`), never the pane.
    ShellResult(ToolResult),
    Drop,
}

#[derive(Default)]
pub struct AiBus {
    pub pane_client: Option<ClientId>,
    /// The last manifest each client registered, for replay.
    manifests: HashMap<ClientId, ServiceManifest>,
    /// Clients that are module instances in this process (`m<id>`
    /// endpoints): no socket, their frames are made and answered here.
    /// This leg is what the web superbuild runs everything on.
    locals: HashSet<ClientId>,
    /// Clients whose app's native-apps.json entry sets tool rules: a
    /// process by the app id the WM launched it as, an in-process module by
    /// its (trusted) id.
    rules: HashMap<ClientId, &'static crate::native_apps::NativeApp>,
    /// The app each client IS: a process by the id the WM launched its slot
    /// as, an in-process module by its (trusted) id. The shell's own calls
    /// ([`AiBus::shell_call`]) pick their target by this, never by the id a
    /// client names in the manifest it registers.
    launched: HashMap<ClientId, String>,
    /// `confirm: host` calls waiting for the approval router, by key.
    held: HashMap<String, (ClientId, ServiceDown)>,
    /// The shell's own calls (a host tool such as `terminal.run`, already
    /// approved through the router) by call id, and the client they went to.
    shell_calls: HashMap<String, ClientId>,
    /// Shell calls whose client died before answering.
    failed_shell_calls: Vec<String>,
    /// Tests only: answers `auto_approve` instead of the process's
    /// developer mode.
    #[cfg(test)]
    dev_check: Option<fn(&str) -> bool>,
}

impl AiBus {
    /// Whether developer mode answers `service`'s approvals.
    fn auto_approves(&self, service: &str) -> bool {
        #[cfg(test)]
        if let Some(check) = self.dev_check {
            return check(service);
        }
        crate::dev_mode::overrides_every_approval(service)
    }

    /// The manifest the pane is told (see [`dev_approved`]).
    fn effective(&self, manifest: &ServiceManifest) -> ServiceManifest {
        dev_approved(manifest, self.auto_approves(&manifest.id))
    }

    /// Every client's registration again, as the pane must now see it: sent
    /// when developer mode turns on, off or expires.
    pub fn reannounce(&self) -> Vec<String> {
        self.registered_clients()
            .into_iter()
            .map(|client| {
                HostedUp {
                    from: Some(self.endpoint_for(client)),
                    msg: ServiceUp::Register { manifest: self.effective(&self.manifests[&client]), port_tag: 0 },
                }
                .to_json()
            })
            .collect()
    }

    fn audit_call(&self, service: &str, manifest: Option<&ServiceManifest>, call: &ServiceCall) {
        audit_dev_call(service, manifest, call, self.auto_approves(service));
    }

    pub fn endpoint_of(client: ClientId) -> EndpointId {
        EndpointId(format!("w{client}"))
    }

    /// `w<id>` for a process client, `m<id>` for an in-process instance.
    pub fn endpoint_for(&self, client: ClientId) -> EndpointId {
        if self.locals.contains(&client) {
            EndpointId(format!("m{client}"))
        } else {
            Self::endpoint_of(client)
        }
    }

    /// (is local, client) from an endpoint string; `None` for neither kind.
    fn client_of(endpoint: &EndpointId) -> Option<(bool, ClientId)> {
        let s = endpoint.as_str();
        let local = match s.chars().next() {
            Some('w') => false,
            Some('m') => true,
            _ => return None,
        };
        s[1..].parse::<ClientId>().ok().map(|c| (local, c))
    }

    /// An in-process instance joins the bus: remembered like any client's
    /// registration (so the replay carries it) and announced to the pane
    /// now with the frame this returns.
    pub fn register_local(&mut self, client: ClientId, mut manifest: ServiceManifest) -> String {
        self.locals.insert(client);
        self.launched.insert(client, manifest.id.clone());
        if let Some(rules) = host_rules(&manifest.id) {
            self.rules.insert(client, rules);
        }
        apply_rules(&mut manifest, self.rules.get(&client).copied());
        self.manifests.insert(client, manifest.clone());
        let manifest = self.effective(&manifest);
        HostedUp { from: Some(self.endpoint_for(client)), msg: ServiceUp::Register { manifest, port_tag: 0 } }.to_json()
    }

    /// An in-process instance's answer, as a frame for the pane.
    pub fn local_reply(&self, client: ClientId, result: ToolResult) -> String {
        self.local_up(client, ServiceUp::Result(result))
    }

    /// An in-process instance's asynchronous publication, stamped with the
    /// same module endpoint as its call results.
    pub fn local_message(&self, client: ClientId, sub_id: String, message: Message) -> String {
        self.local_up(
            client,
            ServiceUp::Message {
                sub_id,
                topic: message.topic,
                text: message.text,
                data: message.data,
                final_: message.final_,
            },
        )
    }

    fn local_up(&self, client: ClientId, msg: ServiceUp) -> String {
        HostedUp { from: Some(self.endpoint_for(client)), msg }.to_json()
    }

    pub fn local_clients(&self) -> Vec<ClientId> {
        let mut out: Vec<ClientId> = self.locals.iter().copied().collect();
        out.sort_unstable();
        out
    }

    pub fn is_pane(&self, client: ClientId) -> bool {
        self.pane_client == Some(client)
    }

    /// Every client with a live registration, oldest first.
    pub fn registered_clients(&self) -> Vec<ClientId> {
        let mut clients: Vec<ClientId> = self.manifests.keys().copied().collect();
        clients.sort_unstable();
        clients
    }

    /// The `os` manifest the pane learns about the WM from.
    pub fn os_manifest(apps: &[(String, String)]) -> ServiceManifest {
        let mut brief = String::from(
            "The desktop itself: which apps exist and run, starting and focusing them. \
             Apps that are not running have no tools until `os.launch` starts them; a running app's \
             tools are already in your table — call them, never launch it again. Known apps: ",
        );
        brief.push_str(
            &apps.iter().map(|(id, label)| format!("{label} (`{id}`)")).collect::<Vec<_>>().join(", "),
        );
        brief.push('.');
        ServiceManifest::new(OS_ENDPOINT, "Desktop", brief)
            .with_tool(ToolDef::new(
                "list",
                "The apps this desktop knows, and which are running.",
                r#"{"type":"object","properties":{}}"#,
                Risk::Read,
            ))
            .with_tool(ToolDef::new(
                "launch",
                "Start an app that is NOT running (see the running list). A running app's tools are already available — call them directly instead of launching.",
                r#"{"type":"object","properties":{"app":{"type":"string","description":"the app id from os.list"}},"required":["app"]}"#,
                Risk::Act,
            ))
            .with_tool(ToolDef::new(
                "focus",
                "Bring a running app to the front.",
                r#"{"type":"object","properties":{"app":{"type":"string"}},"required":["app"]}"#,
                Risk::Act,
            ))
            .with_tool(ToolDef::new(
                "close",
                "Close a running app's window.",
                r#"{"type":"object","properties":{"app":{"type":"string"}},"required":["app"]}"#,
                Risk::Act,
            ))
            .with_tool(ToolDef::new(
                "open",
                "Open a file in its associated app (images, video, csv, pdf, html) as a new window; `app` overrides the association.",
                r#"{"type":"object","properties":{"path":{"type":"string","description":"absolute path of the file"},"app":{"type":"string","description":"an app id from os.list, optional"}},"required":["path"]}"#,
                Risk::Act,
            ))
    }

    /// The frames the pane must see when it connects: the WM's own
    /// registration, then every client's last one.
    pub fn replay(&self, os_manifest: ServiceManifest) -> Vec<String> {
        let mut out = vec![HostedUp {
            from: Some(EndpointId(OS_ENDPOINT.into())),
            msg: ServiceUp::Register { manifest: os_manifest, port_tag: 0 },
        }
        .to_json()];
        for client in self.registered_clients() {
            out.push(
                HostedUp {
                    from: Some(self.endpoint_for(client)),
                    msg: ServiceUp::Register { manifest: self.effective(&self.manifests[&client]), port_tag: 0 },
                }
                .to_json(),
            );
        }
        out
    }

    /// The pane-state broadcast: one `ChatOpen` frame per registered
    /// PROCESS client, addressed to its endpoint (an in-process instance
    /// hears it from the host directly).
    pub fn chat_open_frames(&self, open: bool) -> Vec<(ClientId, String)> {
        self.registered_clients()
            .into_iter()
            .filter(|client| !self.locals.contains(client))
            .map(|client| {
                let frame = HostedDown { to: Some(Self::endpoint_of(client)), msg: ServiceDown::ChatOpen { open } };
                (client, frame.to_json())
            })
            .collect()
    }

    /// A `Custom` frame from `client`. The WM's own `WmRequest` envelope is
    /// not ours and yields `Drop`.
    pub fn on_custom(&mut self, client: ClientId, json: &str) -> Route {
        self.on_custom_from(client, None, json)
    }

    /// `on_custom` for a client the WM launched as `app`: its registration
    /// and calls follow that app's tool rules (`host_rules`).
    pub fn on_custom_from(&mut self, client: ClientId, app: Option<&str>, json: &str) -> Route {
        if let Some(app) = app {
            if !self.is_pane(client) && !self.locals.contains(&client) {
                self.launched.insert(client, app.to_string());
                if let Some(rules) = host_rules(app) {
                    self.rules.insert(client, rules);
                }
            }
        }
        if self.is_pane(client) {
            let Some(down) = HostedDown::parse(json) else { return Route::Drop };
            let Some(to) = down.to.clone() else { return Route::Drop };
            if to.as_str() == OS_ENDPOINT {
                return match down.msg {
                    ServiceDown::Call(call) => {
                        self.audit_call(OS_ENDPOINT, None, &call);
                        Route::Os(call)
                    }
                    _ => Route::Drop,
                };
            }
            let target = match Self::client_of(&to) {
                Some((true, target)) if self.locals.contains(&target) => target,
                Some((false, target)) if !self.locals.contains(&target) && self.manifests.contains_key(&target) => target,
                _ => return Route::Drop,
            };
            if let ServiceDown::Call(call) = &down.msg {
                if let Some(manifest) = self.manifests.get(&target) {
                    self.audit_call(&manifest.id, Some(manifest), call);
                }
            }
            if let ServiceDown::Call(call) = &down.msg {
                // A `confirm: host` tool waits for the approval router (the
                // shell's sheet shows the arguments in full, so no length
                // limit applies; developer mode answers it there first).
                if let (Some(entry), Some(rule)) = (self.rules.get(&target).copied(), host_rule(self.rules.get(&target).copied(), &call.tool)) {
                    let key = format!("{HELD_PREFIX}{}:{}", self.endpoint_for(target).as_str(), call.call_id);
                    let held = HeldCall {
                        key: key.clone(),
                        app: entry.id.to_string(),
                        tool: call.tool.clone(),
                        args: call.args.clone(),
                        auto_approvable: rule.auto_approvable,
                        command: COMMAND_TOOLS.contains(&(entry.id, rule.tool)),
                    };
                    self.held.insert(key, (target, down.msg.clone()));
                    return Route::Approval(held);
                }
            }
            return if self.locals.contains(&target) { Route::Local(target, down.msg) } else { Route::ToClient(target, down.to_json()) };
        }
        let Some(mut up) = HostedUp::parse(json) else { return Route::Drop };
        // The sender's claim is never used: the link IS the identity.
        up.from = Some(Self::endpoint_of(client));
        // The shell's own calls answer the shell, never the pane.
        match &up.msg {
            ServiceUp::Result(result) if self.shell_calls.get(&result.call_id) == Some(&client) => {
                self.shell_calls.remove(&result.call_id);
                return Route::ShellResult(result.clone());
            }
            ServiceUp::Progress { call_id, .. } if self.shell_calls.get(call_id) == Some(&client) => return Route::Drop,
            _ => {}
        }
        match &mut up.msg {
            ServiceUp::Register { manifest, .. } => {
                // Another process cannot vouch for the person's consent: its
                // destructive tools wait for the pane's own confirm card. Only
                // in-process modules (`register_local`, trusted native code)
                // keep a tool's claim that its own sheet confirms it.
                manifest.clear_self_confirm();
                apply_rules(manifest, self.rules.get(&client).copied());
                self.manifests.insert(client, manifest.clone());
                *manifest = self.effective(manifest);
            }
            ServiceUp::Unregister => {
                self.manifests.remove(&client);
            }
            _ => {}
        }
        Route::ToPane(up.to_json())
    }

    /// Call `tool` of the running app whose service is `app` for the shell
    /// itself (the host-tool relay, UPCR-2026-035). The call was authorized
    /// and approved already, so it is not held again; its answer comes back
    /// as [`Route::ShellResult`], never to the pane. `None`: no running
    /// instance of `app` offers `tool` (an in-process Terminal offers its
    /// read tools only). A running instance of `app` is a client the WM
    /// launched as `app` ([`AiBus::launched`]): another app that registers
    /// a manifest named `app` is never picked.
    pub fn shell_call(&mut self, app: &str, tool: &str, args: &str, call_id: &str) -> Option<Route> {
        let client = self
            .manifests
            .iter()
            .filter(|(client, manifest)| {
                self.launched.get(*client).is_some_and(|launched| launched == app)
                    && !self.is_pane(**client)
                    && manifest.tools.iter().any(|t| t.name == tool)
            })
            .map(|(client, _)| *client)
            .max()?;
        let call = ServiceCall { call_id: call_id.to_string(), tool: tool.to_string(), args: args.to_string() };
        if let Some(manifest) = self.manifests.get(&client) {
            self.audit_call(app, Some(manifest), &call);
        }
        self.shell_calls.insert(call_id.to_string(), client);
        let msg = ServiceDown::Call(call);
        Some(if self.locals.contains(&client) {
            Route::Local(client, msg)
        } else {
            Route::ToClient(client, HostedDown { to: Some(Self::endpoint_of(client)), msg }.to_json())
        })
    }

    /// Stop one of the shell's own calls (the kernel cancelled it).
    pub fn shell_cancel(&mut self, call_id: &str) -> Option<Route> {
        let client = self.shell_calls.remove(call_id)?;
        let msg = ServiceDown::Cancel { call_id: call_id.to_string() };
        Some(if self.locals.contains(&client) {
            Route::Local(client, msg)
        } else {
            Route::ToClient(client, HostedDown { to: Some(Self::endpoint_of(client)), msg }.to_json())
        })
    }

    /// An in-process instance answered: true (and the call is done) when it
    /// was one of the shell's own calls, which never reach the pane.
    pub fn take_shell_result(&mut self, client: ClientId, call_id: &str) -> bool {
        if self.shell_calls.get(call_id) == Some(&client) {
            self.shell_calls.remove(call_id);
            return true;
        }
        false
    }

    /// Shell calls whose client died before answering.
    pub fn take_failed_shell_calls(&mut self) -> Vec<String> {
        std::mem::take(&mut self.failed_shell_calls)
    }

    /// The approval router answered a held call: on to the app, or refused
    /// to the pane. A call whose app has gone is dropped.
    pub fn release(&mut self, key: &str, approved: bool, reason: &str) -> Route {
        let Some((target, msg)) = self.held.remove(key) else { return Route::Drop };
        if approved {
            return if self.locals.contains(&target) {
                Route::Local(target, msg)
            } else {
                Route::ToClient(target, HostedDown { to: Some(Self::endpoint_of(target)), msg }.to_json())
            };
        }
        let ServiceDown::Call(call) = msg else { return Route::Drop };
        let refused = ToolResult::refused(&call.call_id, format!("not approved: {reason}"));
        Route::ToPane(HostedUp { from: Some(self.endpoint_for(target)), msg: ServiceUp::Result(refused) }.to_json())
    }

    /// Calls still waiting for the approval router.
    pub fn held(&self) -> usize {
        self.held.len()
    }

    /// A client died: the pane hears an `Unregister` on its behalf.
    pub fn client_died(&mut self, client: ClientId) -> Option<String> {
        if self.is_pane(client) {
            self.pane_client = None;
            return None;
        }
        self.held.retain(|_, (target, _)| *target != client);
        let failed: Vec<String> = self.shell_calls.iter().filter(|(_, c)| **c == client).map(|(id, _)| id.clone()).collect();
        for id in failed {
            self.shell_calls.remove(&id);
            self.failed_shell_calls.push(id);
        }
        self.rules.remove(&client);
        self.launched.remove(&client);
        self.manifests.remove(&client)?;
        let from = Some(self.endpoint_for(client));
        self.locals.remove(&client);
        Some(HostedUp { from, msg: ServiceUp::Unregister }.to_json())
    }

    /// The WM's answer to one of its own calls, as a frame for the pane.
    pub fn os_reply(result: ToolResult) -> String {
        HostedUp { from: Some(EndpointId(OS_ENDPOINT.into())), msg: ServiceUp::Result(result) }.to_json()
    }

    /// A string argument of an os call, trimmed. `None` when absent or
    /// not a string — the caller refuses, it never guesses.
    pub fn str_arg(call: &ServiceCall, key: &str) -> Option<String> {
        match json::parse(call.args.as_bytes()) {
            Ok(json::Value::Obj(fields)) => fields
                .into_iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty()),
            _ => None,
        }
    }

    /// The `app` argument, lowercased like a registry id.
    pub fn app_arg(call: &ServiceCall) -> Option<String> {
        Self::str_arg(call, "app").map(|s| s.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_ai_services::engine::{RegistryUp, ServiceRegistry};
    use makepad_ai_services::port::ServiceLink;

    fn files() -> ServiceManifest {
        ServiceManifest::new("files", "Files", "The file browser.").with_tool(ToolDef::new(
            "stat",
            "Stat a path.",
            r#"{"type":"object","properties":{"path":{"type":"string"}}}"#,
            Risk::Read,
        ))
    }

    fn call(tool: &str, args: &str) -> ServiceCall {
        ServiceCall { call_id: "c".into(), tool: tool.into(), args: args.into() }
    }

    #[test]
    fn the_terminal_types_only_behind_the_approval_routers_answer() {
        let terminal = |run: ToolDef| {
            ServiceManifest::new("terminal", "Terminal", "The live terminal.")
                .with_tool(ToolDef::new("read_screen", "Read the screen.", r#"{"type":"object"}"#, Risk::Read))
                .with_tool(run)
        };
        // A process that under-declares `run`; a module that claims its own sheet.
        let under = || ToolDef::new("run", "Type a line.", r#"{"type":"object"}"#, Risk::Act);
        let own_sheet = || ToolDef::new("run", "Type a line.", r#"{"type":"object"}"#, Risk::Destructive).confirmed_by_app();
        let registered = |json: &str| match HostedUp::parse(json).expect("valid").msg {
            ServiceUp::Register { manifest, .. } => manifest,
            _ => panic!("expected a registration"),
        };
        let mut bus = AiBus { pane_client: Some(9), ..Default::default() };
        // Process (w4) and in-process (m6) Terminals register the same way:
        // `run` kept, confirmed by the shell's approval router (not the
        // pane's card, and not the app's own sheet).
        let up = HostedUp { from: None, msg: ServiceUp::Register { manifest: terminal(under()), port_tag: 0 } };
        let Route::ToPane(json) = bus.on_custom_from(4, Some("terminal"), &up.to_json()) else { panic!("expected ToPane") };
        let local = registered(&bus.register_local(6, terminal(own_sheet())));
        for manifest in [registered(&json), local] {
            assert_eq!(manifest.tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["read_screen", "run"]);
            let run = manifest.tool("run").unwrap();
            assert_ne!(run.risk, Risk::Destructive, "the pane does not confirm a second time");
            assert!(!run.confirms_itself(), "the host draws the confirmation, not the app");
            assert_eq!(manifest.tool("read_screen").unwrap().risk, Risk::Read);
        }
        let rule = crate::native_apps::find("terminal").unwrap().tool("run").unwrap();
        assert_eq!((rule.confirm, rule.auto_approvable), (crate::native_apps::Confirm::Host, false));
        // Every `run` is held for the router, whatever its length (the
        // shell's sheet shows it in full), as a typed command no rule may
        // approve; approved, it reaches the terminal in either hosting.
        let long = format!(r#"{{"command":"echo {}"}}"#, "x".repeat(80));
        for (to, args) in [("w4", r#"{"command":"ls -la"}"#.to_string()), ("m6", long.clone())] {
            let down = HostedDown { to: Some(EndpointId(to.into())), msg: ServiceDown::Call(call("run", &args)) };
            let Route::Approval(held) = bus.on_custom(9, &down.to_json()) else { panic!("expected a held call") };
            assert_eq!(held.key, format!("{HELD_PREFIX}{to}:c"));
            assert_eq!((held.app.as_str(), held.tool.as_str(), held.args.as_str()), ("terminal", "run", args.as_str()));
            assert!(held.command && !held.auto_approvable);
            match bus.release(&held.key, true, "approved on the sheet") {
                Route::ToClient(4, _) => assert_eq!(to, "w4"),
                Route::Local(6, _) => assert_eq!(to, "m6"),
                _ => panic!("expected the call to go on"),
            }
            assert!(matches!(bus.release(&held.key, true, ""), Route::Drop), "released once");
        }
        // Denied: refused to the pane, from the terminal's endpoint.
        let down = HostedDown { to: Some(EndpointId("w4".into())), msg: ServiceDown::Call(call("run", r#"{"command":"rm -rf x"}"#)) };
        let Route::Approval(held) = bus.on_custom(9, &down.to_json()) else { panic!("expected a held call") };
        let Route::ToPane(json) = bus.release(&held.key, false, "denied on the sheet") else { panic!("expected a refusal") };
        let up = HostedUp::parse(&json).unwrap();
        assert_eq!(up.from, Some(EndpointId("w4".into())));
        assert!(matches!(up.msg, ServiceUp::Result(ToolResult { outcome: ToolOutcome::Refused, .. })));
        // Reads pass whatever their length; another app's `run` keeps its declaration.
        let read = HostedDown { to: Some(EndpointId("w4".into())), msg: ServiceDown::Call(call("read_screen", &long)) };
        assert!(matches!(bus.on_custom(9, &read.to_json()), Route::ToClient(4, _)));
        let up = HostedUp { from: None, msg: ServiceUp::Register { manifest: terminal(under()), port_tag: 0 } };
        let Route::ToPane(json) = bus.on_custom_from(5, Some("files"), &up.to_json()) else { panic!("expected ToPane") };
        assert_eq!(registered(&json).tool("run").unwrap().risk, Risk::Act);
        // A held call of a client that dies is dropped.
        let Route::Approval(held) = bus.on_custom(9, &down.to_json()) else { panic!("expected a held call") };
        bus.client_died(4);
        assert!(matches!(bus.release(&held.key, true, ""), Route::Drop));
    }

    #[test]
    fn the_shells_own_calls_reach_the_terminal_unheld_and_answer_the_shell_not_the_pane() {
        let terminal = ServiceManifest::new("terminal", "Terminal", "The live terminal.")
            .with_tool(ToolDef::new("read_screen", "Read the screen.", r#"{"type":"object"}"#, Risk::Read))
            .with_tool(ToolDef::new("run", "Type a line.", r#"{"type":"object"}"#, Risk::Destructive));
        let mut bus = AiBus { pane_client: Some(9), ..Default::default() };
        assert!(bus.shell_call("terminal", "run", r#"{"command":"ls"}"#, "hosttool-c1").is_none(), "no Terminal running");
        let up = HostedUp { from: None, msg: ServiceUp::Register { manifest: terminal, port_tag: 0 } };
        assert!(matches!(bus.on_custom_from(4, Some("terminal"), &up.to_json()), Route::ToPane(_)));
        // Approved by the router already (host_tool): not held a second time.
        let Some(Route::ToClient(4, json)) = bus.shell_call("terminal", "run", r#"{"command":"ls"}"#, "hosttool-c1") else { panic!("expected the call to go to the Terminal") };
        let down = HostedDown::parse(&json).unwrap();
        assert!(matches!(down.msg, ServiceDown::Call(ServiceCall { ref tool, ref args, .. }) if tool == "run" && args == r#"{"command":"ls"}"#));
        assert_eq!(bus.held(), 0);
        // Its answer is the shell's, never the pane's.
        let result = HostedUp { from: None, msg: ServiceUp::Result(ToolResult::ok("hosttool-c1", "typed", "typed")) };
        assert!(matches!(bus.on_custom_from(4, Some("terminal"), &result.to_json()), Route::ShellResult(r) if r.call_id == "hosttool-c1"));
        let again = HostedUp { from: None, msg: ServiceUp::Result(ToolResult::ok("hosttool-c1", "typed", "typed")) };
        assert!(matches!(bus.on_custom_from(4, Some("terminal"), &again.to_json()), Route::ToPane(_)), "answered once");
        // A Terminal that dies fails the shell's calls in flight.
        assert!(bus.shell_call("terminal", "run", r#"{"command":"sleep 9"}"#, "hosttool-c2").is_some());
        bus.client_died(4);
        assert_eq!(bus.take_failed_shell_calls(), vec!["hosttool-c2".to_string()]);
        // An in-process Terminal offering its reads only cannot take `run`.
        let reads = ServiceManifest::new("terminal", "Terminal", "reads").with_tool(ToolDef::new("read_screen", "Read.", r#"{"type":"object"}"#, Risk::Read));
        let _ = bus.register_local(6, reads);
        assert!(bus.shell_call("terminal", "run", "{}", "hosttool-c3").is_none());
    }

    #[test]
    fn should_route_a_shell_call_by_launch_identity_when_another_app_registers_the_same_manifest_id() {
        let impostor = || ServiceManifest::new("terminal", "Terminal", "Not the terminal.")
            .with_tool(ToolDef::new("run", "Type a line.", r#"{"type":"object"}"#, Risk::Destructive));
        let mut bus = AiBus { pane_client: Some(9), ..Default::default() };
        // Sheets (client 7) registers a manifest that says it is the terminal.
        let up = HostedUp { from: None, msg: ServiceUp::Register { manifest: impostor(), port_tag: 0 } };
        assert!(matches!(bus.on_custom_from(7, Some("sheets"), &up.to_json()), Route::ToPane(_)));
        // A frame with no launch identity (no slot) is no terminal either.
        assert!(matches!(bus.on_custom_from(8, None, &up.to_json()), Route::ToPane(_)));
        assert!(bus.shell_call("terminal", "run", r#"{"command":"ls"}"#, "hosttool-x1").is_none(), "no client was launched as the terminal");
        // The real Terminal (client 4) is picked, though client 7 is newer.
        let up = HostedUp { from: None, msg: ServiceUp::Register { manifest: impostor(), port_tag: 0 } };
        assert!(matches!(bus.on_custom_from(4, Some("terminal"), &up.to_json()), Route::ToPane(_)));
        assert!(matches!(bus.shell_call("terminal", "run", r#"{"command":"ls"}"#, "hosttool-x2"), Some(Route::ToClient(4, _))));
        // Its answer is taken only from the client the call went to.
        let result = HostedUp { from: None, msg: ServiceUp::Result(ToolResult::ok("hosttool-x2", "typed", "typed")) };
        assert!(matches!(bus.on_custom_from(7, Some("sheets"), &result.to_json()), Route::ToPane(_)));
        assert!(matches!(bus.on_custom_from(4, Some("terminal"), &result.to_json()), Route::ShellResult(_)));
        // Once it is gone, the impostor still does not get the call.
        bus.client_died(4);
        assert!(bus.shell_call("terminal", "run", "{}", "hosttool-x3").is_none());
    }

    #[test]
    fn up_frames_are_stamped_and_replayed_and_down_frames_are_routed() {
        let mut bus = AiBus { pane_client: Some(9), ..Default::default() };
        // A client registers: stamped with the WM's endpoint, forwarded.
        let up = HostedUp { from: Some(EndpointId("lie".into())), msg: ServiceUp::Register { manifest: files(), port_tag: 3 } };
        match bus.on_custom(4, &up.to_json()) {
            Route::ToPane(json) => {
                let parsed = HostedUp::parse(&json).unwrap();
                assert_eq!(parsed.from, Some(EndpointId("w4".into())), "the sender's claim is overwritten");
            }
            _ => panic!("expected ToPane"),
        }
        assert_eq!(bus.registered_clients(), vec![4]);
        // Replay carries os first, then the client.
        let replay = bus.replay(AiBus::os_manifest(&[("files".into(), "Files".into())]));
        assert_eq!(replay.len(), 2);
        assert!(replay[0].contains("\"os\"") && replay[1].contains("\"w4\""));
        // The pane addresses the client; the WM routes by endpoint.
        let down = HostedDown { to: Some(EndpointId("w4".into())), msg: ServiceDown::Call(call("stat", "{}")) };
        assert!(matches!(bus.on_custom(9, &down.to_json()), Route::ToClient(4, _)));
        // An os call is the WM's own.
        let os = HostedDown { to: Some(EndpointId("os".into())), msg: ServiceDown::Call(call("launch", r#"{"app":"Route"}"#)) };
        match bus.on_custom(9, &os.to_json()) {
            Route::Os(call) => assert_eq!(AiBus::app_arg(&call).as_deref(), Some("route")),
            _ => panic!("expected Os"),
        }
        // A frame to an unknown endpoint, or the WM's own envelope, drops.
        let stray = HostedDown { to: Some(EndpointId("w77".into())), msg: ServiceDown::ChatOpen { open: true } };
        assert!(matches!(bus.on_custom(9, &stray.to_json()), Route::Drop));
        assert!(matches!(bus.on_custom(4, r#"{"wm":{"Close":{}}}"#), Route::Drop));
        // Death → synthetic Unregister; the pane's own death clears the pane.
        let bye = bus.client_died(4).unwrap();
        assert!(bye.contains("Unregister") && bye.contains("\"w4\""));
        assert!(bus.client_died(4).is_none());
        assert!(bus.client_died(9).is_none());
        assert_eq!(bus.pane_client, None);
    }

    #[test]
    fn chat_open_reaches_every_registered_client() {
        let mut bus = AiBus { pane_client: Some(9), ..Default::default() };
        assert!(bus.chat_open_frames(true).is_empty());
        for client in [6, 4] {
            let up = HostedUp { from: None, msg: ServiceUp::Register { manifest: files(), port_tag: 0 } };
            bus.on_custom(client, &up.to_json());
        }
        let frames = bus.chat_open_frames(true);
        assert_eq!(frames.iter().map(|(c, _)| *c).collect::<Vec<_>>(), vec![4, 6]);
        for (client, json) in frames {
            let down = HostedDown::parse(&json).unwrap();
            assert_eq!(down.to, Some(AiBus::endpoint_of(client)));
            assert_eq!(down.msg, ServiceDown::ChatOpen { open: true });
        }
    }

    #[test]
    fn pubsub_frames_cross_the_hosted_bus_in_both_directions() {
        let mut bus = AiBus { pane_client: Some(9), ..Default::default() };
        let manifest = files().with_topic(TopicDef::new("changes", "File changes."));
        let register = HostedUp {
            from: None,
            msg: ServiceUp::Register { manifest: manifest.clone(), port_tag: 0 },
        };
        let register = match bus.on_custom(4, &register.to_json()) {
            Route::ToPane(json) => HostedUp::parse(&json).expect("valid registration"),
            _ => panic!("expected the registration to reach the pane"),
        };

        // This is the aichat side of the same bridge: the registry owns
        // the engine half and the WM endpoint remains its routing identity.
        let registry = ServiceRegistry::new();
        let endpoint = EndpointId("w4".into());
        let (link, host) = ServiceLink::pair(manifest);
        registry.register_as(link, endpoint.clone(), "hosted by wm", None).unwrap();
        host.up.send(register).unwrap();
        assert!(registry.pump().is_empty());
        let _registered = host.down.try_recv().expect("registry acknowledgement");

        assert!(registry.send(
            &endpoint,
            ServiceDown::Subscribe {
                sub_id: "s1".into(),
                topic: "changes".into(),
                filter: Some(r#"{"kind":"done"}"#.into()),
            },
        ));
        let subscribe = host.down.try_recv().expect("subscription from engine");
        let routed = match bus.on_custom(9, &subscribe.to_json()) {
            Route::ToClient(4, json) => HostedDown::parse(&json).expect("valid down-frame"),
            _ => panic!("expected the subscription to reach client 4"),
        };
        assert_eq!(routed, subscribe);

        assert!(registry.send(&endpoint, ServiceDown::Unsubscribe { sub_id: "s1".into() }));
        let unsubscribe = host.down.try_recv().expect("unsubscription from engine");
        let routed = match bus.on_custom(9, &unsubscribe.to_json()) {
            Route::ToClient(4, json) => HostedDown::parse(&json).expect("valid down-frame"),
            _ => panic!("expected the unsubscription to reach client 4"),
        };
        assert_eq!(routed, unsubscribe);

        let message = HostedUp {
            from: Some(EndpointId("forged".into())),
            msg: ServiceUp::Message {
                sub_id: "s1".into(),
                topic: "changes".into(),
                text: "finished".into(),
                data: Some(r#"{"rows":3}"#.into()),
                final_: true,
            },
        };
        let message = match bus.on_custom(4, &message.to_json()) {
            Route::ToPane(json) => HostedUp::parse(&json).expect("valid up-frame"),
            _ => panic!("expected the message to reach the pane"),
        };
        assert_eq!(message.from, Some(endpoint.clone()));
        host.up.send(message).unwrap();
        assert!(matches!(
            registry.pump().as_slice(),
            [RegistryUp::Message { endpoint: from, sub_id, message }]
                if from == &endpoint
                    && sub_id == "s1"
                    && message.topic == "changes"
                    && message.text == "finished"
                    && message.final_
        ));
    }

    #[test]
    fn os_arguments_are_strings_or_nothing() {
        let c = call("open", r#"{"path":"  /tmp/a b.png ","app":"Image"}"#);
        assert_eq!(AiBus::str_arg(&c, "path").as_deref(), Some("/tmp/a b.png"));
        assert_eq!(AiBus::app_arg(&c).as_deref(), Some("image"));
        assert_eq!(AiBus::str_arg(&c, "missing"), None);
        // Wrong type, empty string, no object: refused upstream, never guessed.
        assert_eq!(AiBus::str_arg(&call("open", r#"{"path":7}"#), "path"), None);
        assert_eq!(AiBus::str_arg(&call("open", r#"{"path":"  "}"#), "path"), None);
        assert_eq!(AiBus::str_arg(&call("open", "[]"), "path"), None);
        // The manifest lists the five tools with object schemas.
        let manifest = AiBus::os_manifest(&[]);
        assert!(manifest.validate().is_ok());
        assert!(manifest.tool("open").is_some());
        assert_eq!(manifest.tools.len(), 5);
    }
}

#[cfg(test)]
mod local_tests {
    use super::*;

    fn sheets() -> ServiceManifest {
        ServiceManifest::new("sheets", "Sheets", "The spreadsheet.").with_tool(ToolDef::new(
            "summary",
            "The sheet on screen.",
            r#"{"type":"object","properties":{}}"#,
            Risk::Read,
        ))
    }

    #[test]
    fn an_in_process_instance_is_a_local_endpoint_on_the_same_bus() {
        let mut bus = AiBus { pane_client: Some(9), ..Default::default() };
        // Registering announces it with an `m` endpoint, and the replay
        // carries it like any client's registration.
        let announce = bus.register_local(4, sheets());
        let up = HostedUp::parse(&announce).unwrap();
        assert_eq!(up.from, Some(EndpointId("m4".into())));
        assert!(matches!(up.msg, ServiceUp::Register { .. }));
        assert_eq!(bus.local_clients(), vec![4]);
        let replay = bus.replay(AiBus::os_manifest(&[]));
        assert_eq!(replay.len(), 2);
        assert!(replay[1].contains("\"m4\""));
        // A call addressed to it is the host's to run; the wrong kind of
        // address for the same id drops.
        let call = ServiceCall { call_id: "c1".into(), tool: "summary".into(), args: "{}".into() };
        let down = HostedDown { to: Some(EndpointId("m4".into())), msg: ServiceDown::Call(call.clone()) };
        match bus.on_custom(9, &down.to_json()) {
            Route::Local(4, ServiceDown::Call(c)) => assert_eq!(c.call_id, "c1"),
            _ => panic!("expected Local"),
        }
        let wrong = HostedDown { to: Some(EndpointId("w4".into())), msg: ServiceDown::Call(call) };
        assert!(matches!(bus.on_custom(9, &wrong.to_json()), Route::Drop));
        // The answer goes up from the local endpoint.
        let reply = bus.local_reply(4, ToolResult::ok("c1", "Sheet 1", ""));
        let up = HostedUp::parse(&reply).unwrap();
        assert_eq!(up.from, Some(EndpointId("m4".into())));
        let publication = bus.local_message(4, "lease-s1".into(), Message::new("watch", "changed"));
        let up = HostedUp::parse(&publication).unwrap();
        assert!(matches!(
            up,
            HostedUp {
                from: Some(EndpointId(ref from)),
                msg: ServiceUp::Message { ref sub_id, ref text, .. },
            } if from == "m4" && sub_id == "lease-s1" && text == "changed"
        ));
        // ChatOpen goes to process clients only; the host tells locals itself.
        assert!(bus.chat_open_frames(true).is_empty());
        // Death: an Unregister from the local endpoint, then nothing.
        let bye = bus.client_died(4).unwrap();
        assert!(bye.contains("Unregister") && bye.contains("\"m4\""));
        assert!(bus.local_clients().is_empty());
        assert!(bus.client_died(4).is_none());
    }

    #[test]
    fn only_an_in_process_module_keeps_a_self_confirmed_tool() {
        let send = || {
            sheets().with_tool(
                ToolDef::new("send", "Send the sheet.", r#"{"type":"object","properties":{}}"#, Risk::Destructive)
                    .confirmed_by_app(),
            )
        };
        let registered = |json: &str| match HostedUp::parse(json).expect("valid").msg {
            ServiceUp::Register { manifest, .. } => manifest,
            _ => panic!("expected a registration"),
        };
        let mut bus = AiBus { pane_client: Some(9), ..Default::default() };
        // A module in this process: its own sheet is the one confirmation.
        let local = registered(&bus.register_local(4, send()));
        assert!(local.tool("send").unwrap().confirms_itself());
        // A process client's claim is dropped before the pane sees it, and in
        // the replay, so the pane confirms its destructive tools itself.
        let up = HostedUp { from: None, msg: ServiceUp::Register { manifest: send(), port_tag: 0 } };
        let Route::ToPane(json) = bus.on_custom(5, &up.to_json()) else { panic!("expected ToPane") };
        assert!(!registered(&json).tool("send").unwrap().confirms_itself());
        let replay = bus.replay(AiBus::os_manifest(&[]));
        assert!(registered(&replay[1]).tool("send").unwrap().confirms_itself(), "m4 keeps it");
        assert!(!registered(&replay[2]).tool("send").unwrap().confirms_itself(), "w5 does not");
    }

    /// Developer mode approves a covered app's destructive tools in advance:
    /// the pane is told `Act` (so it shows no confirm card), for that app
    /// only, and is told again when the mode changes.
    #[test]
    fn developer_mode_approves_a_covered_apps_destructive_tools_in_advance() {
        let manifest = |id: &str| {
            ServiceManifest::new(id, "App", "An app.")
                .with_tool(ToolDef::new("send", "Send.", r#"{"type":"object","properties":{}}"#, Risk::Destructive).confirmed_by_app())
                .with_tool(ToolDef::new("peek", "Look.", r#"{"type":"object","properties":{}}"#, Risk::Read))
        };
        let registered = |json: &str| match HostedUp::parse(json).expect("valid").msg {
            ServiceUp::Register { manifest, .. } => manifest,
            _ => panic!("expected a registration"),
        };
        fn only_mail(service: &str) -> bool {
            service == "mail"
        }
        let mut bus = AiBus { pane_client: Some(9), dev_check: Some(only_mail), ..Default::default() };
        let up = |id: &str| HostedUp { from: None, msg: ServiceUp::Register { manifest: manifest(id), port_tag: 0 } }.to_json();
        let Route::ToPane(mail) = bus.on_custom(4, &up("mail")) else { panic!("expected ToPane") };
        let Route::ToPane(news) = bus.on_custom(5, &up("news")) else { panic!("expected ToPane") };
        let local = bus.register_local(6, manifest("mail"));
        for json in [&mail, &local] {
            let m = registered(json);
            assert_eq!(m.tool("send").unwrap().risk, Risk::Act, "approved in advance");
            assert!(m.validate().is_ok(), "and still a valid manifest");
        }
        assert_eq!(registered(&news).tool("send").unwrap().risk, Risk::Destructive, "an app it does not cover");
        assert_eq!(registered(&bus.replay(AiBus::os_manifest(&[]))[1]).tool("send").unwrap().risk, Risk::Act);
        // Off: the pane is told the real risk again, and a module keeps its own sheet.
        fn nobody(_: &str) -> bool {
            false
        }
        bus.dev_check = Some(nobody);
        let again = bus.reannounce();
        assert_eq!(again.len(), 3);
        for json in &again {
            assert_eq!(registered(json).tool("send").unwrap().risk, Risk::Destructive);
        }
        assert!(registered(&again[2]).tool("send").unwrap().confirms_itself(), "m6 keeps its own confirmation");
        assert!(!registered(&again[0]).tool("send").unwrap().confirms_itself());
    }

    /// Every approval a pane call can need today is answered, and named for
    /// the audit: the Terminal's `run` (destructive, host-confirmed,
    /// `auto_approvable: false` in ADR 0004 §10), a tool the app confirms on
    /// its own sheet, and nothing for a read.
    #[test]
    fn developer_mode_answers_the_terminal_run_and_app_sheets() {
        use crate::dev_mode::ApprovalKind;
        // The Terminal's manifest (makepad apps/terminal/src/ai.rs).
        let terminal = ServiceManifest::new("terminal", "Terminal", "The live terminal.")
            .with_tool(ToolDef::new("read_screen", "Read.", r#"{"type":"object","properties":{}}"#, Risk::Read))
            .with_tool(ToolDef::new("run", "Type a command.", r#"{"type":"object","properties":{"command":{"type":"string"}}}"#, Risk::Destructive));
        let rinx = sheets().with_tool(
            ToolDef::new("send", "Send.", r#"{"type":"object","properties":{}}"#, Risk::Destructive).confirmed_by_app(),
        );
        let approved = dev_approved(&terminal, true);
        assert!(approved.tools.iter().all(|t| t.risk != Risk::Destructive), "nothing left for the pane to confirm");
        assert!(approved.validate().is_ok());
        assert!(dev_approved(&rinx, true).tools.iter().all(|t| !t.confirms_itself()), "no app sheet either");
        assert_eq!(dev_approved(&terminal, false), terminal, "outside developer mode, untouched");
        assert_eq!(approval_answered(Some(&terminal), "run", true), Some(ApprovalKind::PaneConfirm));
        assert_eq!(approval_answered(Some(&rinx), "send", true), Some(ApprovalKind::AppConfirm));
        assert_eq!(approval_answered(Some(&terminal), "read_screen", true), None);
        assert_eq!(approval_answered(Some(&terminal), "run", false), None);
        assert_eq!(approval_answered(None, "run", true), None);
    }

    /// End to end through the pane's engine: a module's self-confirmed tool
    /// reaches the module at once (its own sheet asks the person), while the
    /// same claim from another process still gets the pane's confirm card.
    #[test]
    fn the_pane_skips_its_confirm_only_for_a_modules_self_confirmed_tool() {
        use makepad_ai_services::engine::{EngineCore, EngineEvent, Model, ModelEvent, ServiceRegistry, ToolDefinition};
        use makepad_ai_services::port::ServiceLink;
        /// A model that asks for one call.
        struct OneCall(Vec<ModelEvent>);
        impl Model for OneCall {
            fn label(&self) -> String {
                "test".into()
            }
            fn configure(&mut self, _: &str, _: &[ToolDefinition]) -> Result<(), String> {
                Ok(())
            }
            fn send_user(&mut self, _: &str, _: &str) {}
            fn send_tool_result(&mut self, _: &str, _: &str, _: bool) {}
            fn cancel(&mut self) {}
            fn reset(&mut self) {}
            fn poll(&mut self) -> Vec<ModelEvent> {
                std::mem::take(&mut self.0)
            }
        }
        let manifest = || {
            sheets()
                .with_tool(
                    ToolDef::new("send", "Send the sheet.", r#"{"type":"object","properties":{}}"#, Risk::Destructive)
                        .confirmed_by_app(),
                )
                .with_tool(ToolDef::new("delete", "Delete the sheet.", r#"{"type":"object","properties":{}}"#, Risk::Destructive))
        };
        let mut bus = AiBus { pane_client: Some(9), ..Default::default() };
        let local = bus.register_local(4, manifest());
        let up = HostedUp { from: None, msg: ServiceUp::Register { manifest: manifest(), port_tag: 0 } };
        let Route::ToPane(process) = bus.on_custom(5, &up.to_json()) else { panic!("expected ToPane") };
        for (frame, endpoint, tool, pane_confirms) in [
            (&local, "m4", "send", false),
            (&local, "m4", "delete", true),
            (&process, "w5", "send", true),
        ] {
            let up = HostedUp::parse(frame).expect("valid registration");
            let ServiceUp::Register { manifest, .. } = up.msg.clone() else { panic!("expected a registration") };
            let registry = ServiceRegistry::new();
            let (link, host) = ServiceLink::pair(manifest);
            registry.register_as(link, EndpointId(endpoint.into()), "test", None).unwrap();
            host.up.send(up).unwrap();
            registry.pump();
            while host.down.try_recv().is_ok() {}
            let call = ModelEvent::ToolCall { call_id: "m1".into(), name: format!("sheets.{tool}"), args: "{}".into() };
            let model = OneCall(vec![call, ModelEvent::TurnDone { tool_calls: 1 }]);
            let mut core = EngineCore::new(registry, Box::new(model), None, 1);
            core.send("do it", 0.0);
            let events = core.pump(0.1);
            let confirm = events.contains(&EngineEvent::Confirm { call_id: "m1".into() });
            assert_eq!(confirm, pane_confirms, "{endpoint} {tool}: {events:?}");
            let reached = std::iter::from_fn(|| host.down.try_recv().ok())
                .any(|down| matches!(down.msg, ServiceDown::Call(ref c) if c.tool == tool));
            assert_eq!(reached, !pane_confirms, "{endpoint} {tool}: the call reaches the app only without a card");
        }
    }
}
