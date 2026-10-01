//! The window manager as a module host (aicontrol.md §3): app instances
//! that run IN-PROCESS, one splash isolate each, instead of as child
//! processes.
//!
//! Creating one: allocate the isolate (the widget universe is installed
//! by the allocation itself), retint its stock theme from the WM palette,
//! let the module register its own families, and call `create` — all
//! inside ONE trusted entry into the isolate, so the module never holds a
//! second `&mut Cx` beside the VM. The root comes back minted in that
//! heap; the tile (`module_view.rs`) draws it; the executor answers the
//! assistant's calls through the bus's in-process leg (`ai_bus.rs`).
//!
//! Tearing one down, in order: the tile drops the root FIRST (so nothing
//! draws a widget whose heap is about to go), the instance's `shutdown`
//! runs in the isolate, the executor and the host's own root ref are
//! dropped, the isolate is freed — its script timers stop with it. What
//! the scope token does NOT yet reach — native timers, audio lanes,
//! native layers, HTTP requests the instance opened through the platform
//! — is the InstanceScope gap the next phase closes.
//!
//! PANIC CONTAINMENT (ADR 0004 §2, plan step 9). An in-process module is
//! a crash domain of its own: every call the shell makes into one — its
//! `register` and `create`, the events and the draw its tile gives the
//! root (`module_view.rs`), the host's own `Event::Custom`s, Back and
//! keyboard deliveries, the executor's `execute` / `cancel` / `subscribe`
//! / `unsubscribe` / `chat_open`, a restyle, its `shutdown` — goes through
//! [`contain`] or [`contain_outside`], which run it under `catch_unwind`
//! (inside the isolate for the first, restored on the way up by Makepad's
//! `with_isolate`). A panic there does not reach the platform: the
//! instance's isolate is marked failed on `Cx` ([`ModuleFaults`]) so
//! NOTHING dispatches to it again, even before the shell has looked, and
//! the fault is queued. The shell drains the queue after every event
//! ([`ModuleHost::take_faults`], `App::contain_module_faults`): it logs the
//! module id and message, lets the tile show the app closed with a
//! Restart, drops the instance's extra windows and its tools, and calls
//! [`ModuleHost::release_failed`], which frees what the instance held.
//!
//! A second panic is the one that used to abort the process (2026-09-27:
//! the terminal's font panic, then one in the recovery path). So after a
//! fault nothing of the failed module runs unguarded: its shutdown runs
//! under a catch of its own, its executor — native state last seen
//! mid-panic — is LEAKED rather than dropped (a panic inside a drop that
//! is itself unwinding is an abort no catch can stop), every other drop
//! and the isolate's free run under a catch, and a panic payload whose own
//! `Drop` panics is forgotten. What stays out of reach: a panic raised
//! while the module's own frames are already unwinding (Rust aborts
//! before any catch runs), a `panic = "abort"` build, and FFI.

use crate::hub::ClientId;
use makepad_ai_services::wire::{ServiceCall, ServiceManifest};
use makepad_app_module::*;
use makepad_widgets::*;
use makepad_widgets::widget_async::{with_isolate, IsolateCx};
use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::Receiver;

// ---- panic containment ----

/// Isolates whose module panicked, and the faults the shell has not taken
/// yet. On `Cx`, so the tile (which knows only its isolate) and the host
/// see one answer to "may this instance run?".
#[derive(Default)]
pub struct ModuleFaults {
    failed: HashSet<SplashVmId>,
    pending: Vec<ModuleFault>,
}

/// One contained panic: which isolate, in what, and what it said.
#[derive(Clone, Debug)]
pub struct ModuleFault {
    pub vm_id: SplashVmId,
    pub what: &'static str,
    pub message: String,
}

/// What a panic said, for the log and the tile.
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "a panic with no message".to_string()
    }
}

/// Drop a caught payload without letting its `Drop` panic out of here: a
/// payload that panics while dropping is forgotten, and so is that one's.
pub fn discard_payload(payload: Box<dyn std::any::Any + Send>) {
    if let Err(again) = catch_unwind(AssertUnwindSafe(move || drop(payload))) {
        std::mem::forget(again);
    }
}

/// Whether the module in `vm_id` has panicked: nothing may call into it.
pub fn is_failed(cx: &mut Cx, vm_id: SplashVmId) -> bool {
    cx.global::<ModuleFaults>().failed.contains(&vm_id)
}

/// Record that the module in `vm_id` panicked in `what`: its isolate is
/// closed to every later call from now on, and the shell hears of it after
/// this event. Returns the message.
pub fn report_fault(cx: &mut Cx, vm_id: SplashVmId, what: &'static str, payload: Box<dyn std::any::Any + Send>) -> String {
    let message = panic_message(&*payload);
    discard_payload(payload);
    let faults = cx.global::<ModuleFaults>();
    // The first fault is the one that says why; later ones (a restyle
    // walking every instance, the same event reaching a second tile of it)
    // cannot happen once the isolate is closed, but are dropped if they do.
    if faults.failed.insert(vm_id) {
        faults.pending.push(ModuleFault { vm_id, what, message: message.clone() });
    }
    error!("wm: contained a module panic in isolate {vm_id:?} ({what}): {message}");
    message
}

// ---- the peer link's in-process leg (ADR 0004 §5, #142) ----

/// Peer links module code opened (Makepad's `OctosPeer::open`, which parks
/// the host's end in `PendingPeerLinks`), each with the isolate whose code
/// was running: the instance that opened it. [`ModuleHost::pump_peer_links`]
/// hands them to the shell's peer link. On `Cx`, beside [`ModuleFaults`].
#[derive(Default)]
pub struct OpenedPeerLinks {
    links: Vec<(SplashVmId, makepad_ai_services::peer::PeerLink)>,
}

/// The isolates whose module code is running now, innermost last: a
/// `contain` inside another (a host call a module's code made) must not
/// take the outer module's links as stray or as its own.
#[derive(Default)]
pub struct RunningIsolates(Vec<SplashVmId>);

/// Before module code of `vm_id` runs: links parked so far are the
/// enclosing module's (claimed for it), or, outside any module, nobody's
/// (dropped), so nothing can be attributed to the wrong instance.
fn enter_isolate(cx: &mut Cx, vm_id: SplashVmId) {
    match cx.global::<RunningIsolates>().0.last().copied() {
        Some(outer) => claim_peer_links(cx, outer),
        None => {
            let stray = cx.global::<makepad_ai_services::peer::PendingPeerLinks>().take();
            if !stray.is_empty() {
                log!("wm: {} peer link(s) opened outside any module instance dropped", stray.len());
            }
        }
    }
    cx.global::<RunningIsolates>().0.push(vm_id);
}

/// After module code of `vm_id` ran: what it parked is its own.
fn leave_isolate(cx: &mut Cx, vm_id: SplashVmId) {
    claim_peer_links(cx, vm_id);
    let running = &mut cx.global::<RunningIsolates>().0;
    if let Some(at) = running.iter().rposition(|v| *v == vm_id) {
        running.remove(at);
    }
}

/// After module code of isolate `vm_id` ran: the links it opened are its.
fn claim_peer_links(cx: &mut Cx, vm_id: SplashVmId) {
    let links = cx.global::<makepad_ai_services::peer::PendingPeerLinks>().take();
    if !links.is_empty() {
        cx.global::<OpenedPeerLinks>().links.extend(links.into_iter().map(|link| (vm_id, link)));
    }
}

/// Run `f` inside the module isolate `vm_id` with its panics contained:
/// `None` when the instance has already failed (nothing runs) or `f`
/// panicked (the fault is reported). The isolate is left and the outer VM
/// restored either way (`with_isolate`). The one choke point every call
/// into a module's widgets goes through.
pub fn contain<C: IsolateCx, R>(cx: &mut C, vm_id: SplashVmId, what: &'static str, f: impl FnOnce(&mut C) -> R) -> Option<R> {
    if is_failed(cx.isolate_cx(), vm_id) {
        return None;
    }
    enter_isolate(cx.isolate_cx(), vm_id);
    let ran = catch_unwind(AssertUnwindSafe(|| with_isolate(cx, vm_id, f)));
    leave_isolate(cx.isolate_cx(), vm_id);
    match ran {
        Ok(out) => Some(out),
        Err(payload) => {
            report_fault(cx.isolate_cx(), vm_id, what, payload);
            None
        }
    }
}

/// [`contain`] for module code the host calls outside the isolate (the
/// executor's bookkeeping, a restyle that enters the isolate itself).
pub fn contain_outside<R>(cx: &mut Cx, vm_id: SplashVmId, what: &'static str, f: impl FnOnce(&mut Cx) -> R) -> Option<R> {
    if is_failed(cx, vm_id) {
        return None;
    }
    enter_isolate(cx, vm_id);
    let ran = catch_unwind(AssertUnwindSafe(|| f(cx)));
    leave_isolate(cx, vm_id);
    match ran {
        Ok(out) => Some(out),
        Err(payload) => {
            report_fault(cx, vm_id, what, payload);
            None
        }
    }
}

/// Run cleanup that must not take the shell down with it: a panic is
/// logged and its payload discarded. False when it panicked.
fn guarded_cleanup(what: &str, f: impl FnOnce()) -> bool {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(()) => true,
        Err(payload) => {
            error!("wm: contained a panic while {what}: {}", panic_message(&*payload));
            discard_payload(payload);
            false
        }
    }
}

/// What a failed instance's tools answer: the executor that panicked is
/// gone (leaked), and every call says so.
struct StoppedExecutor(ServiceManifest);
impl ServiceExecutor for StoppedExecutor {
    fn manifest(&self) -> ServiceManifest {
        self.0.clone()
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(makepad_ai_services::wire::ToolResult::unavailable(&call.call_id, STOPPED_REASON))
    }
}

/// What a tool call to a failed instance answers.
pub const STOPPED_REASON: &str = "the app stopped after an error; restart it to use it again";

/// What `data` says on an outcome-unknown answer, for a router or a model
/// that reads it rather than the text.
pub const OUTCOME_UNKNOWN_DATA: &str = r#"{"outcome":"unknown","reason":"app_panicked","retry":"only_after_checking_with_the_person"}"#;

/// The answer to a call that may have partly run when its app panicked:
/// an `Act` or `Destructive` tool (or one the manifest does not name)
/// that was executing, or had answered `Pending`, when the module failed.
/// It must never look succeeded, and never like a plain, safely retryable
/// error: the effect may have happened, half-happened or not happened.
///
/// The wire has no `Unknown` outcome yet, so this is `TimedOut` — the
/// wire's one existing "the service never finished answering; what it did
/// is unknown" outcome, which is never `is_ok()` — with text and `data`
/// that say so and `Disposition::EndTurn`, so the turn stops with the
/// person rather than the model retrying on its own. A proper
/// `ToolOutcome::Unknown` (slug `unknown`) in makepad-ai-services' wire is
/// the Makepad change that would replace this; see the PR.
pub fn outcome_unknown(call_id: &str, tool: &str, label: &str) -> makepad_ai_services::wire::ToolResult {
    makepad_ai_services::wire::ToolResult::timed_out(
        call_id,
        format!(
            "Outcome unknown: {label} stopped after an error while `{tool}` was running. It may have partly \
             happened. Do not retry it; check with the person what actually changed first."
        ),
    )
    .with_data(OUTCOME_UNKNOWN_DATA)
    .with_disposition(makepad_ai_services::wire::Disposition::EndTurn)
}

/// Whether a call to `tool` can have changed something: anything but a
/// declared `Read`. A tool the manifest does not name counts as able to.
fn may_have_acted(manifest: &ServiceManifest, tool: &str) -> bool {
    manifest.tools.iter().find(|t| t.name == tool)
        .is_none_or(|t| t.risk != makepad_ai_services::wire::Risk::Read)
}

/// The answer to a call interrupted by its app's panic: outcome unknown
/// when it may have acted, plainly unavailable when it only read.
fn interrupted(manifest: &ServiceManifest, label: &str, call_id: &str, tool: &str) -> makepad_ai_services::wire::ToolResult {
    if may_have_acted(manifest, tool) {
        outcome_unknown(call_id, tool, label)
    } else {
        makepad_ai_services::wire::ToolResult::unavailable(call_id, STOPPED_REASON)
    }
}

pub struct AppInstance {
    pub client: ClientId,
    pub module: &'static dyn AppModule,
    pub vm_id: SplashVmId,
    pub scope: InstanceScope,
    /// The n-th instance of this app in this session: `sheets.2`.
    pub instance_no: u64,
    pub root: WidgetRef,
    executor: Box<dyn ServiceExecutor>,
    shutdown: Option<Box<dyn FnOnce(&mut ScriptVm)>>,
    /// Results and publications the executor sent later.
    upstream: Receiver<ModuleUpstream>,
    /// The instance's requests for extra windows, and our reports of closes.
    pub windows: ModuleWindows,
    /// The instance's scoped assistant service (Rinx ADR 0007), when the
    /// module declares and is granted `octos.*` services.
    assistant: Option<crate::ai_host::Assistant>,
    /// Its peer link, when its code opened Makepad's `OctosPeer` (#142):
    /// served by `crate::peer_link` exactly as a process's socket.
    peer: Option<crate::ai_host::module_peer::ModulePeerLink>,
    /// A link it opened without a granted agent: not served, but each of
    /// its requests is answered `no_agent` (not a link: [`ModuleHost::has_peer_link`] is false).
    refused_peer: Option<crate::ai_host::module_peer::ModulePeerLink>,
    /// The executor's manifest, read once (contained) at creation: the
    /// shell asks for it again after a failure, when the executor is gone.
    manifest: ServiceManifest,
    /// The module panicked, with what it said: nothing calls into it again.
    failed: Option<String>,
    /// A failed instance's shutdown ran and its isolate is freed.
    released: bool,
    /// Calls its executor answered `Pending` and has not answered yet:
    /// (call id, tool). What is still here when the module panics is
    /// answered for it ([`outcome_unknown`]).
    in_flight: Vec<(String, String)>,
    /// Answers the host owes the pane for a failed instance's interrupted
    /// calls, handed out by `drain_upstream`.
    owed: Vec<makepad_ai_services::wire::ToolResult>,
}

impl AppInstance {
    pub fn manifest(&self) -> ServiceManifest {
        self.manifest.clone()
    }

    /// What the module's panic said, once it has panicked.
    pub fn failure(&self) -> Option<&str> {
        self.failed.as_deref()
    }

    /// `module.instance_no`, as the log names it.
    pub fn label(&self) -> String {
        format!("{}.{}", self.module.id(), self.instance_no)
    }
}

/// Rinx runs one instance per process: tests that create it take turns.
#[cfg(test)]
pub static RINX_INSTANCE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Default)]
pub struct ModuleHost {
    /// Whether new instances may open extra windows: the desktop shell,
    /// not the phone shell (whose apps are full-screen and present their
    /// secondary surfaces as modals in their own pane).
    pub extra_windows: bool,
    instances: HashMap<ClientId, AppInstance>,
    next_scope: u64,
    per_app: HashMap<String, u64>,
    style: Option<desktop_style::StyleSheet>,
    /// Closes an instance refused while it asks the person (makepad#65).
    close_gate: CloseGate,
}

/// The closes instances refused (`AppModule::close_requested` answered
/// `Veto`) and a shell quit waiting on them.
///
/// A veto means the instance is showing its own question in its root (the
/// terminal: "Closing the terminal ends 1 running job…"). The host keeps
/// the instance and remembers its root's uid; the root emits
/// `ModuleCloseAction::Confirmed` from that uid on a yes, and the shell
/// tears the instance down then. A no emits nothing: the mark stays until
/// the next close asks again or the instance goes another way.
///
/// A quit asks every instance: with no veto it quits at once; otherwise it
/// waits, and quits when the last refusing instance confirms. The person
/// can decline in any of them; the quit then just never completes, and a
/// later close the person asks for (or a new quit, which asks again)
/// drops the waiting quit, so confirming that close later ends only that
/// instance, never the shell.
#[derive(Debug, Default)]
pub struct CloseGate {
    pending: HashMap<ClientId, WidgetUid>,
    quit_waiting: bool,
    /// The person's closes of each instance: the third inside the window
    /// ends one that keeps refusing (`process_close::Insistence`).
    insistence: crate::process_close::Insistence,
}

impl CloseGate {
    /// An instance answered a close: `true` when the host should tear it
    /// down now (`Allow`); on `Veto` it stays and `root_uid` is remembered.
    pub fn answered(&mut self, client: ClientId, decision: CloseDecision, root_uid: WidgetUid) -> bool {
        match decision {
            CloseDecision::Allow => {
                self.pending.remove(&client);
                true
            }
            CloseDecision::Veto => {
                self.pending.insert(client, root_uid);
                false
            }
        }
    }

    /// A root emitted `ModuleCloseAction::Confirmed`: the instance to tear
    /// down, when the uid is one whose close is pending. Any other uid (an
    /// instance nobody asked to close, a stale root) is ignored.
    pub fn confirmed(&mut self, uid: WidgetUid) -> Option<ClientId> {
        let client = self.pending.iter().find(|(_, pending)| **pending == uid).map(|(client, _)| *client)?;
        self.pending.remove(&client);
        Some(client)
    }

    pub fn is_pending(&self, client: ClientId) -> bool {
        self.pending.contains_key(&client)
    }

    /// The instance went (torn down, failed): nothing to wait for.
    pub fn forget(&mut self, client: ClientId) {
        self.pending.remove(&client);
        self.insistence.forget(client);
    }

    /// The person closes (or quits past) `client` at `now`: `true` when
    /// this close is insisting (`process_close::FORCE_CLOSES` inside the
    /// window) and ends the instance whatever it answers. Its pending close
    /// is dropped then.
    pub fn insist(&mut self, client: ClientId, now: f64) -> bool {
        let forced = crate::process_close::Insistence::forced(self.insistence.close(client, now));
        if forced {
            self.pending.remove(&client);
            self.insistence.forget(client);
        }
        forced
    }

    /// Whether the next close of `client` ends it.
    pub fn next_close_forces(&self, client: ClientId) -> bool {
        crate::process_close::Insistence::forced(self.insistence.count(client) + 1)
    }

    /// A person-initiated close of one instance: a quit that was waiting
    /// is abandoned (the person turned to something else).
    pub fn close_asked(&mut self) {
        self.quit_waiting = false;
    }

    /// A shell quit asked every live instance and these refused. `true`:
    /// quit now (nobody refused). Otherwise the quit waits for them.
    pub fn quit_asked(&mut self, refused: impl IntoIterator<Item = (ClientId, WidgetUid)>) -> bool {
        self.pending.extend(refused);
        self.quit_waiting = !self.pending.is_empty();
        !self.quit_waiting
    }

    /// A waiting quit whose last refusing instance has now confirmed.
    pub fn take_quit_ready(&mut self) -> bool {
        let ready = self.quit_waiting && self.pending.is_empty();
        if ready {
            self.quit_waiting = false;
        }
        ready
    }

    pub fn quit_waiting(&self) -> bool {
        self.quit_waiting
    }

    /// No instance is asking the person.
    pub fn idle(&self) -> bool {
        self.pending.is_empty()
    }
}

/// The isolate removes mod.res after bootstrap. Trusted framework themes
/// still need its crate resource resolver for their bundled fonts. Expose
/// only that existing resolver during theme registration, then remove it.
fn apply_module_style(vm: &mut ScriptVm, sheet: &desktop_style::StyleSheet) {
    let mut inherited = sheet.clone();
    // A nested Splash (a Card app the `card` module runs) replays this
    // trusted theme after its ambient `mod.res` has been stripped. Bind only
    // the existing bundled-resource resolver in the theme's lexical scope, so
    // that replay can still load the style's fonts. This does not publish a
    // resource module to the card's source.
    inherited.theme = format!(
        "mod._octosense_widgets_before_style = mod.widgets\n\
         mod._octosense_prelude_before_style = mod.prelude.widgets\n\
         let crate_resource = mod.prelude.widgets.crate_resource\n{}", inherited.theme);
    // widgets_mod rebuilds these namespaces, including the prelude a Card's
    // lowered body uses. Retain host additions (DesignSurface and the kit)
    // while letting the freshly themed framework names replace their old ones.
    inherited.widgets = format!(
        "{}\n\
         mod.widgets = {{..mod._octosense_widgets_before_style, ..mod.widgets}}\n\
         mod.prelude.widgets = {{..mod._octosense_prelude_before_style, ..mod.prelude.widgets}}\n\
         mod._octosense_widgets_before_style = nil\n\
         mod._octosense_prelude_before_style = nil\n", inherited.widgets);
    desktop_style::install(vm, inherited);
    vm.with_reload(|vm| {
        script_eval!(vm, { mod.res = {crate_resource: mod.prelude.widgets.crate_resource} });
        makepad_widgets::widgets_mod(vm);
        desktop_style::apply_widgets(vm);
        script_eval!(vm, { mod.res = nil });
    });
}

impl ModuleHost {
    /// Build one instance of `module` for the client id the WM gave it.
    /// `viewport` is the tile size the layout will give it.
    pub fn create(
        &mut self,
        cx: &mut Cx,
        client: ClientId,
        module: &'static dyn AppModule,
        open: ValidatedOpen,
        viewport: DVec2,
    ) -> Result<(), String> {
        if self.instances.contains_key(&client) {
            return Err(format!("client {client} already hosts an instance"));
        }
        // The package's trusted system UI (the phone's Settings) is a singleton.
        if crate::ext::trusted_module(module) && self.settings_instance().is_some() {
            return Err(format!("{} already has a live instance", module.label()));
        }
        self.next_scope += 1;
        let scope = InstanceScope::new(client, self.next_scope);
        let instance_no = {
            let n = self.per_app.entry(module.id().to_string()).or_insert(0);
            *n += 1;
            *n
        };
        // The storage jail: a namespace of the Cx storage API, one per
        // instance (§3b's mount and the web's IndexedDB sit under it).
        let storage = cx.storage(&format!("{}.{}", module.id(), instance_no));
        let (replies, upstream) = ReplySink::pair();
        // No extra host windows on a phone (`extra_windows` off): `windows`
        // stays unsupported, so a module presents its secondary surfaces as
        // modals in its own pane.
        let windows = ModuleWindows::new(self.extra_windows);
        let handles = InstanceHandles {
            scope, storage, viewport: Viewport { size: viewport }, replies,
            windows: windows.clone(),
        };
        let vm_id = cx.alloc_splash_vm_with_network(false);
        // The assistant is offered to THIS instance for the duration of its
        // create only; the module takes it there or never gets it.
        // In developer mode a covered module gets every service it declares
        // (dev_mode.rs); the grant lives and dies with this instance.
        // The system toolbox's tools it declares (the broker registers them
        // once the person allowed its agent).
        #[cfg(feature = "toolbox-peers")]
        crate::host_tools::toolbox::grant_module(module.id(), module.capabilities());
        let offer = crate::ai_host::offer_with(module, &scope, crate::dev_mode::grants_all(module.id()));
        // Consent at first use (ADR 0004 §4, approvals/consent.rs): a module
        // the person has not allowed an agent is not offered one. The first
        // time, the shell's first-use sheet asks; this instance goes without
        // and the next one gets it once allowed. Developer mode asks nothing.
        let offer = if offer.is_offered() && !crate::approvals::consent_for_module(module.id(), module.label(), module.capabilities()) {
            let _ = offer.finish();
            None
        } else {
            Some(offer)
        };
        // Its storage (jail, account folders, secrets; ADR 0004 §11) the
        // same way, when it declares `storage` and the host has storage.
        let scope_key = scope.to_string();
        crate::app_storage::offer(module.id(), module.capabilities(), &scope_key);
        // The module's `register`, `create` and first `manifest` run under
        // the containment every later call gets: a module that panics here
        // never becomes an instance, and its isolate goes at once.
        let style = self.style.as_ref();
        let created = contain_outside(cx, vm_id, "create", |cx| {
            let parts = cx.with_script_vm_id_trusted(vm_id, |vm| {
                // The isolate came up with the stock theme; the WM's palette
                // retints it exactly as it retints a child process's.
                if let Some(sheet) = style {
                    apply_module_style(vm, sheet);
                }
                makepad_wm_theme::apply(vm);
                module.register(vm);
                module.create(vm, open, handles)
            });
            let manifest = parts.executor.manifest();
            (parts, manifest)
        });
        // What the module did not take is withdrawn; what it took stays
        // with the instance until teardown.
        let assistant = offer.and_then(|offer| offer.finish());
        crate::app_storage::withdraw(module.id(), &scope_key);
        let Some((parts, manifest)) = created else {
            // Already reported; the shell has no client to show it on, so
            // the fault is taken here and the launch fails with it.
            let fault = take_fault_of(cx, vm_id);
            let message = fault.map(|f| f.message).unwrap_or_default();
            if let Some(assistant) = assistant {
                guarded_cleanup("releasing a failed create's assistant", || assistant.release());
            }
            guarded_cleanup("freeing a failed create's isolate", || cx.free_splash_vm(vm_id));
            error!("wm: module {} panicked in create: {message}", module.id());
            return Err(format!("{} panicked while starting: {message}", module.label()));
        };
        log!(
            "wm: module instance {}.{} for client {} in isolate {:?} (scope {})",
            module.id(),
            instance_no,
            client,
            vm_id,
            scope
        );
        self.instances.insert(
            client,
            AppInstance {
                client,
                module,
                vm_id,
                scope,
                instance_no,
                executor: host_executor(module, &parts.root, parts.executor),
                root: parts.root,
                shutdown: Some(parts.shutdown),
                upstream,
                windows,
                assistant,
                peer: None,
                refused_peer: None,
                manifest,
                failed: None,
                released: false,
                in_flight: Vec::new(),
                owed: Vec::new(),
            },
        );
        Ok(())
    }

    pub fn apply_style(&mut self,cx:&mut Cx,sheet:&desktop_style::StyleSheet) {
        self.style=Some(sheet.clone());
        for instance in self.instances.values_mut().filter(|i| i.failed.is_none()) {
            let vm_id = instance.vm_id;
            contain_outside(cx, vm_id, "a restyle", |cx| {
                cx.with_script_vm_id_trusted(vm_id,|vm| {
                    apply_module_style(vm, sheet);
                    vm.with_reload(|vm| {
                        makepad_wm_theme::apply(vm);
                        instance.module.register(vm);
                    });
                    let source=instance.root.widget_type_id().and_then(|ty|vm.bx.heap.type_default_for_id(ty)).unwrap_or_else(||instance.root.script_source());
                    instance.root.script_apply(vm,&Apply::ScriptReapply,&mut Scope::empty(),source.into());
                });
                instance.root.redraw(cx);
            });
        }
    }

    /// The person turned `app`'s agent off (ADR 0004 §4): every instance of
    /// it loses its assistant service now (its contexts close, its leases
    /// go); the next instance is offered one only once allowed again. How
    /// many were revoked.
    pub fn revoke_assistant(&mut self, app: &str) -> usize {
        let mut n = 0;
        for instance in self.instances.values_mut().filter(|i| i.module.id() == app) {
            if let Some(assistant) = instance.assistant.take() {
                guarded_cleanup("revoking an instance's assistant", || assistant.release());
                n += 1;
            }
        }
        n
    }

    /// The assistant service the shell gave this instance, if any.
    pub fn assistant_of(&self, client: ClientId) -> Option<&crate::ai_host::Assistant> {
        self.instances.get(&client)?.assistant.as_ref()
    }

    pub fn is_module(&self, client: ClientId) -> bool {
        self.instances.contains_key(&client)
    }

    pub fn get(&self, client: ClientId) -> Option<&AppInstance> {
        self.instances.get(&client)
    }

    /// The lowest client id hosting an instance of module `id`, if any.
    pub fn client_of_module(&self, id: &str) -> Option<ClientId> {
        self.instances
            .values()
            .filter(|i| i.module.id() == id)
            .map(|i| i.client)
            .min()
    }

    /// The instance whose root minted this widget uid, if any: how a
    /// widget action posted by a module root is attributed to its client.
    pub fn client_of_root_uid(&self, uid: WidgetUid) -> Option<ClientId> {
        self.instances.values().find(|i| i.root.widget_uid() == uid).map(|i| i.client)
    }

    /// The live instance of the package's trusted system UI (the phone's
    /// Settings; none on the desktop). Privilege derives from the compiled
    /// singleton and live root, never a script-supplied module ID or
    /// self-declared capability string.
    pub fn settings_instance(&self) -> Option<&AppInstance> {
        self.instances.values().find(|i| crate::ext::trusted_module(i.module))
    }
    /// The client whose trusted root minted `uid`, if any.
    pub fn settings_client(&self, uid: WidgetUid) -> Option<ClientId> {
        self.instances.values().find(|i| !i.root.is_empty() && i.root.widget_uid() == uid && crate::ext::trusted_module(i.module)).map(|i| i.client)
    }

    /// Deliver a JSON message to an instance as `Event::Custom`, inside its
    /// isolate: the module half of what `send_wm_event` does for a process.
    /// False when no instance has this client id.
    pub fn send_custom(&mut self, cx: &mut Cx, client: ClientId, json: String) -> bool {
        if !self.instances.contains_key(&client) {
            return false;
        }
        self.dispatch(cx, client, "a host message", |cx, root| {
            root.handle_event(cx, &Event::Custom(json), &mut Scope::empty())
        });
        true
    }

    /// Run host code against an instance's root inside its isolate, with
    /// its panics contained: how the shell hands a module anything outside
    /// its tile's own event and draw (a face, the keyboard, Back, a host
    /// message). `None` when there is no live instance or it panicked.
    pub fn dispatch<R>(&mut self, cx: &mut Cx, client: ClientId, what: &'static str, f: impl FnOnce(&mut Cx, &WidgetRef) -> R) -> Option<R> {
        let instance = self.instances.get(&client).filter(|i| i.failed.is_none())?;
        let (root, vm_id) = (instance.root.clone(), instance.vm_id);
        contain(cx, vm_id, what, |cx| f(cx, &root))
    }

    /// Ask the live instance `client` whether it may close
    /// (`AppModule::close_requested`, outside its isolate, contained) and
    /// remember a refusal. `Allow` for a failed instance, a non-module and
    /// a module that panicked while answering: nothing is left to protect.
    pub fn ask_close(&mut self, cx: &mut Cx, client: ClientId) -> CloseDecision {
        self.close_gate.close_asked();
        self.ask_one(cx, client)
    }

    fn ask_one(&mut self, cx: &mut Cx, client: ClientId) -> CloseDecision {
        let Some(instance) = self.instances.get(&client).filter(|i| i.failed.is_none()) else {
            self.close_gate.forget(client);
            return CloseDecision::Allow;
        };
        let (module, root, vm_id) = (instance.module, instance.root.clone(), instance.vm_id);
        let decision = contain_outside(cx, vm_id, "close_requested", |cx| module.close_requested(cx, &root))
            .unwrap_or(CloseDecision::Allow);
        self.close_gate.answered(client, decision, root.widget_uid());
        decision
    }

    /// The shell is quitting: ask every live instance. The clients that
    /// refused (each now asking the person); empty means quit now.
    pub fn ask_quit(&mut self, cx: &mut Cx) -> Vec<ClientId> {
        let mut clients: Vec<ClientId> = self.instances.keys().copied().collect();
        clients.sort_unstable();
        let mut refused = Vec::new();
        for client in clients {
            if self.ask_one(cx, client) == CloseDecision::Veto {
                let uid = self.instances[&client].root.widget_uid();
                refused.push((client, uid));
            }
        }
        let vetoed = refused.iter().map(|(client, _)| *client).collect();
        self.close_gate.quit_asked(refused);
        vetoed
    }

    /// A root emitted `ModuleCloseAction::Confirmed`: the instance to tear
    /// down now, when its close was pending.
    pub fn close_confirmed(&mut self, uid: WidgetUid) -> Option<ClientId> {
        self.close_gate.confirmed(uid)
    }

    /// Whether `client` refused a close and is asking the person.
    pub fn close_pending(&self, client: ClientId) -> bool {
        self.close_gate.is_pending(client)
    }

    /// A waiting quit can go ahead now (every refusing instance confirmed).
    pub fn take_quit_ready(&mut self) -> bool {
        self.close_gate.take_quit_ready()
    }

    /// The closes instances refused, for a quit that also waits on
    /// process apps (`process_close::take_quit_ready`).
    pub fn close_gate_mut(&mut self) -> &mut CloseGate {
        &mut self.close_gate
    }

    /// Drop a waiting quit (a close of one app, or the quit went).
    pub fn abandon_quit(&mut self) {
        self.close_gate.close_asked();
    }

    /// The peer links modules opened since the last call, and what each
    /// live instance sent on its link: to the shell's peer link, the same
    /// code that serves a process's socket (#142). A link belongs to the
    /// instance whose code opened it; one per instance (a second is
    /// dropped, and its requests go unanswered). Call after every event.
    pub fn pump_peer_links(&mut self, cx: &mut Cx) {
        let opened = std::mem::take(&mut cx.global::<OpenedPeerLinks>().links);
        for (vm_id, link) in opened {
            let Some(instance) = self.instances.values_mut().find(|i| i.vm_id == vm_id && i.failed.is_none()) else {
                log!("wm: a peer link opened in isolate {vm_id:?}, which hosts no live instance, dropped");
                continue;
            };
            if instance.peer.is_some() || instance.refused_peer.is_some() {
                log!("wm: {} opened a second peer link; one per instance, dropped", instance.label());
                continue;
            }
            let link = crate::ai_host::module_peer::ModulePeerLink::new(link);
            if crate::peer_link::module_connected(instance.client, instance.module.id(), link.frames_down()) {
                log!("wm: {} opened its peer link", instance.label());
                instance.peer = Some(link);
            } else {
                // No granted agent: not a link, but its requests are still
                // answered (`no_agent`), so the app can say so.
                log!("wm: {} opened a peer link but has no agent; its requests are refused", instance.label());
                instance.refused_peer = Some(link);
            }
        }
        for instance in self.instances.values().filter(|i| i.failed.is_none()) {
            for link in instance.peer.iter().chain(instance.refused_peer.iter()) {
                for frame in link.take_up() {
                    crate::peer_link::on_module_frame(instance.client, instance.module.id(), &frame, link.frames_down());
                }
            }
        }
    }

    /// Whether `client` has a peer link (it opened Makepad's `OctosPeer`).
    pub fn has_peer_link(&self, client: ClientId) -> bool {
        self.instances.get(&client).is_some_and(|i| i.peer.is_some())
    }

    /// Whether `client` is an instance whose module panicked.
    pub fn is_failed(&self, client: ClientId) -> bool {
        self.instances.get(&client).is_some_and(|i| i.failed.is_some())
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// One of the assistant's calls, to the instance's executor — inside
    /// the instance's isolate, as the tile dispatches events: an executor
    /// reaches into its app's widgets (AppCard's `ask` is the composer).
    ///
    /// A failed instance answers "unavailable": the call never ran. One
    /// whose executor panics ON this call answers [`outcome_unknown`] when
    /// the tool may have acted (it may have partly run), "unavailable"
    /// when it only reads. A `Pending` call is remembered until its answer
    /// comes up, so a later panic can answer it the same way.
    pub fn execute(&mut self, cx: &mut Cx, client: ClientId, call: &ServiceCall) -> Option<ExecOutcome> {
        let instance = self.instances.get_mut(&client)?;
        // Failed, or panicked earlier in this event and not yet taken by
        // the shell: the call never runs, so it is plainly unavailable.
        if instance.failed.is_some() || is_failed(cx, instance.vm_id) {
            return Some(ExecOutcome::Done(makepad_ai_services::wire::ToolResult::unavailable(&call.call_id, STOPPED_REASON)));
        }
        let vm_id = instance.vm_id;
        let executor = &mut instance.executor;
        match contain(cx, vm_id, "a tool call", |cx| executor.execute(cx, call)) {
            Some(ExecOutcome::Pending) => {
                instance.in_flight.push((call.call_id.clone(), call.tool.clone()));
                Some(ExecOutcome::Pending)
            }
            Some(done) => Some(done),
            None => Some(ExecOutcome::Done(interrupted(&instance.manifest, instance.module.label(), &call.call_id, &call.tool))),
        }
    }

    pub fn cancel(&mut self, cx: &mut Cx, client: ClientId, call_id: &str) {
        if let Some(instance) = self.instances.get_mut(&client).filter(|i| i.failed.is_none()) {
            let executor = &mut instance.executor;
            contain_outside(cx, instance.vm_id, "a cancel", |cx| executor.cancel(cx, call_id));
            // The router answered it `Cancelled` already.
            instance.in_flight.retain(|(id, _)| id != call_id);
        }
    }

    pub fn subscribe(
        &mut self,
        cx: &mut Cx,
        client: ClientId,
        sub_id: &str,
        topic: &str,
        filter: Option<&str>,
    ) {
        if let Some(instance) = self.instances.get_mut(&client).filter(|i| i.failed.is_none()) {
            let executor = &mut instance.executor;
            contain_outside(cx, instance.vm_id, "a subscribe", |cx| executor.subscribe(cx, sub_id, topic, filter));
        }
    }

    pub fn unsubscribe(&mut self, cx: &mut Cx, client: ClientId, sub_id: &str) {
        if let Some(instance) = self.instances.get_mut(&client).filter(|i| i.failed.is_none()) {
            let executor = &mut instance.executor;
            contain_outside(cx, instance.vm_id, "an unsubscribe", |cx| executor.unsubscribe(cx, sub_id));
        }
    }

    pub fn chat_open(&mut self, cx: &mut Cx, open: bool) {
        for instance in self.instances.values_mut().filter(|i| i.failed.is_none()) {
            let executor = &mut instance.executor;
            contain_outside(cx, instance.vm_id, "chat_open", |cx| executor.chat_open(cx, open));
        }
    }

    /// Instances' pending window requests: (owner, its isolate, its app id, request).
    pub fn take_window_requests(&mut self) -> Vec<(ClientId, SplashVmId, &'static str, WindowRequest)> {
        let mut out = Vec::new();
        for (client, instance) in self.instances.iter().filter(|(_, i)| i.failed.is_none()) {
            for request in instance.windows.take_requests() {
                out.push((*client, instance.vm_id, instance.module.id(), request));
            }
        }
        out
    }

    /// The host starts or stops showing extra windows (desktop vs phone shell).
    pub fn set_extra_windows(&mut self, on: bool) {
        self.extra_windows = on;
        for instance in self.instances.values() {
            instance.windows.set_supported(on);
        }
    }

    /// The person closed `owner`'s window `key`.
    pub fn notify_window_closed(&self, owner: ClientId, key: LiveId) {
        if let Some(instance) = self.instances.get(&owner).filter(|i| i.failed.is_none()) {
            instance.windows.notify_closed(key);
        }
    }

    /// Every result or publication an executor sent later, with its client.
    ///
    /// A failed instance yields what its executor sent before the panic
    /// for calls still in flight (a real answer beats an unknown one), then
    /// the outcome-unknown answers owed for the rest — and nothing after:
    /// an answer arriving late for a call already answered is dropped.
    pub fn drain_upstream(&mut self) -> Vec<(ClientId, ModuleUpstream)> {
        let mut out = Vec::new();
        for (client, instance) in self.instances.iter_mut() {
            while let Ok(message) = instance.upstream.try_recv() {
                if let ModuleUpstream::Result(result) = &message {
                    let before = instance.in_flight.len();
                    instance.in_flight.retain(|(id, _)| *id != result.call_id);
                    if instance.failed.is_some() && instance.in_flight.len() == before {
                        continue;
                    }
                } else if instance.failed.is_some() {
                    continue;
                }
                out.push((*client, message));
            }
            for result in instance.owed.drain(..) {
                out.push((*client, ModuleUpstream::Result(result)));
            }
        }
        out
    }

    /// End the instance: its shutdown runs in its isolate, then the isolate
    /// is freed. The caller has already cleared the tile's root.
    ///
    /// A failed instance was released when it failed; this only forgets
    /// it. A live one's shutdown, drops and free each run guarded, so a
    /// module that panics while being closed still closes.
    pub fn teardown(&mut self, cx: &mut Cx, client: ClientId) -> bool {
        let Some(mut instance) = self.instances.remove(&client) else {
            return false;
        };
        self.close_gate.forget(client);
        let label = instance.label();
        if instance.failed.is_some() {
            if !instance.released {
                self.release_instance(cx, &mut instance);
            }
            // Its executor was leaked at release; the rest holds nothing
            // of the module's.
            guarded_cleanup("dropping a failed instance", move || drop(instance));
            log!("wm: failed module instance {label} closed");
            return true;
        }
        let vm_id = instance.vm_id;
        if let Some(shutdown) = instance.shutdown.take() {
            if contain_outside(cx, vm_id, "shutdown", |cx| cx.with_script_vm_id_trusted(vm_id, |vm| shutdown(vm))).is_none() {
                // Closed anyway; the fault stays queued only for the log.
                take_fault_of(cx, vm_id);
                error!("wm: module instance {label} panicked in shutdown; freeing its isolate anyway");
            }
        }
        // Release the app's assistant leases; the shared kernel stays.
        if let Some(assistant) = instance.assistant.take() {
            guarded_cleanup("releasing an instance's assistant", || assistant.release());
        }
        close_peer_link(&mut instance);
        // The last refs into the isolate's heap go before the heap does.
        guarded_cleanup("dropping a module instance", move || drop(instance));
        guarded_cleanup("freeing a module isolate", || cx.free_splash_vm(vm_id));
        log!("wm: module instance {label} torn down; isolate {vm_id:?} freed");
        true
    }

    /// The faults contained since the last call, attributed: each instance
    /// whose module panicked is marked failed (once) and named with its
    /// label, for the shell to show and then [`release_failed`](Self::release_failed).
    /// Faults of isolates no instance owns (one already torn down) are
    /// logged and dropped.
    pub fn take_faults(&mut self, cx: &mut Cx) -> Vec<(ClientId, String)> {
        let pending = std::mem::take(&mut cx.global::<ModuleFaults>().pending);
        let mut out = Vec::new();
        for fault in pending {
            let Some(instance) = self.instances.values_mut().find(|i| i.vm_id == fault.vm_id) else {
                log!("wm: a panic in isolate {:?} ({}) belongs to no live instance: {}", fault.vm_id, fault.what, fault.message);
                continue;
            };
            if instance.failed.is_some() {
                continue;
            }
            error!(
                "wm: module {} (instance {}, client {}) panicked in {}: {}; the instance is stopped, the shell goes on",
                instance.module.id(), instance.label(), instance.client, fault.what, fault.message
            );
            instance.failed = Some(fault.message);
            // Answers the executor already sent are genuine: take them
            // before the in-flight rest is declared unknown.
            while let Ok(message) = instance.upstream.try_recv() {
                if let ModuleUpstream::Result(result) = message {
                    if let Some(at) = instance.in_flight.iter().position(|(id, _)| *id == result.call_id) {
                        instance.in_flight.remove(at);
                        instance.owed.push(result);
                    }
                }
            }
            let label = instance.module.label();
            for (call_id, tool) in std::mem::take(&mut instance.in_flight) {
                let answer = interrupted(&instance.manifest, label, &call_id, &tool);
                instance.owed.push(answer);
            }
            out.push((instance.client, instance.module.label().to_string()));
        }
        out
    }

    /// Free what a failed instance held, once its tiles have let go of its
    /// root: its shutdown runs (guarded), its assistant leases go, its
    /// executor is leaked (see the module doc), and its isolate is freed.
    /// The entry stays, failed, until the shell closes or restarts it.
    pub fn release_failed(&mut self, cx: &mut Cx, client: ClientId) {
        let Some(mut instance) = self.instances.remove(&client) else { return };
        // A failed instance will never confirm the close it was asking about.
        self.close_gate.forget(client);
        if instance.failed.is_some() && !instance.released {
            self.release_instance(cx, &mut instance);
        }
        self.instances.insert(client, instance);
    }

    fn release_instance(&mut self, cx: &mut Cx, instance: &mut AppInstance) {
        let vm_id = instance.vm_id;
        instance.released = true;
        // Its own last word, for the resources it opened (a pty, a
        // socket); run in the isolate even though that is marked failed.
        if let Some(shutdown) = instance.shutdown.take() {
            let ran = catch_unwind(AssertUnwindSafe(|| cx.with_script_vm_id_trusted(vm_id, |vm| shutdown(vm))));
            if let Err(payload) = ran {
                error!("wm: failed module instance {} panicked again in shutdown: {}", instance.label(), panic_message(&*payload));
                discard_payload(payload);
            }
        }
        if let Some(assistant) = instance.assistant.take() {
            guarded_cleanup("releasing a failed instance's assistant", || assistant.release());
        }
        close_peer_link(instance);
        let executor = std::mem::replace(&mut instance.executor, Box::new(StoppedExecutor(instance.manifest.clone())));
        std::mem::forget(executor);
        let root = std::mem::replace(&mut instance.root, WidgetRef::empty());
        guarded_cleanup("dropping a failed instance's root", move || drop(root));
        guarded_cleanup("freeing a failed instance's isolate", || cx.free_splash_vm(vm_id));
        log!("wm: failed module instance {} released; isolate {vm_id:?} freed, executor leaked", instance.label());
    }
}

/// The instance's peer link goes as a process's does when it exits: its
/// calls fail, its contexts close, its app's peer stays.
fn close_peer_link(instance: &mut AppInstance) {
    instance.refused_peer = None;
    if instance.peer.take().is_some() {
        crate::peer_link::process_gone(instance.client);
    }
}

/// Take the queued fault of `vm_id`, if any (one the host handles itself).
fn take_fault_of(cx: &mut Cx, vm_id: SplashVmId) -> Option<ModuleFault> {
    let pending = &mut cx.global::<ModuleFaults>().pending;
    let at = pending.iter().position(|f| f.vm_id == vm_id)?;
    Some(pending.remove(at))
}

#[cfg(test)]
mod nested_style_tests {
    use super::*;

    #[test]
    fn nested_card_isolate_reloads_mobile_fonts_without_resource_authority() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        let outer = cx.alloc_splash_vm_with_network(false);
        let inherited = cx.with_script_vm_id_trusted(outer, |vm| {
            apply_module_style(vm, &desktop_style::StyleSheet::load(desktop_style::DesktopStyle::Android));
            #[cfg(feature = "app-hub")]
            octosense_app_hub_app::CARD_MODULE.register(vm);
            desktop_style::current(vm).unwrap()
        });
        let nested = cx.alloc_splash_vm_with_network(false);
        cx.with_script_vm_id_trusted(nested, |vm| {
            vm.bx.captured_errors = Some(Vec::new());
            // Splash replays the inherited stylesheet inside a fresh isolate.
            // Its resource module has already been stripped at allocation.
            desktop_style::install(vm, inherited);
            vm.with_reload(|vm| {
                makepad_widgets::widgets_mod(vm);
                desktop_style::apply_widgets(vm);
            });
            let root = script_eval!(vm, {use mod.widgets.* Label{text: "Trail Notes"}});
            assert!(root.as_object().is_some());
            #[cfg(feature = "app-hub")]
            {
                let card = script_eval!(vm, {use mod.prelude.widgets.* DesignSurface{title := Label{text: "Trail Notes"}}});
                let root = WidgetRef::script_from_value(vm, card);
                assert_eq!(root.label(vm.cx_mut(), ids!(title)).text(), "Trail Notes", "card errors: {:?}", vm.take_errors());
            }
            assert!(script_eval!(vm, {mod.res}).is_nil(), "the card must not gain a resource module");
            assert!(script_eval!(vm, {mod.run}).is_nil(), "the card must not gain process access");
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "nested card theme errors: {errors:?}");
        });
        cx.free_splash_vm(nested);
        cx.free_splash_vm(outer);
    }
}

#[cfg(all(test, feature="app-sheets"))]
mod style_tests {
    use super::*;
    #[test]
    fn phone_presets_restyle_existing_module_without_recreating_it() {
        use crate::mobile_theme::{Preset,Selection};
        let mut cx=Cx::new(Box::new(|_,_|{}));
        cx.with_vm(makepad_widgets::script_mod);
        let mut host=ModuleHost::default();
        let module=&makepad_sheets::module::SHEETS_MODULE;
        host.create(&mut cx,1,module,module.open_schema().validate("{}", &[]).unwrap(),dvec2(390.0,780.0)).unwrap();
        let uid=host.get(1).unwrap().root.widget_uid();
        let isolate=host.get(1).unwrap().vm_id;
        for preset in Preset::ALL { for dark in [false,true] {
            let choice=Selection {preset,..Default::default()};
            host.apply_style(&mut cx,&choice.sheet(crate::desktop::DesktopStyle::Android,dark));
            let instance=host.get(1).unwrap();
            assert_eq!(instance.root.widget_uid(),uid);
            assert_eq!(instance.vm_id,isolate);
            cx.with_script_vm_id_trusted(isolate,|vm| {
                let palette=makepad_wm_theme::current_for_vm(vm).unwrap();
                let p=choice.palette(dark).background;
                let expected=format!("#{:02x}{:02x}{:02x}",(p.x*255.0).round() as u8,(p.y*255.0).round() as u8,(p.z*255.0).round() as u8);
                assert_eq!(palette.get("background"),Some(expected.as_str()),"{} dark={dark}",preset.id());
                assert!(vm.take_errors().is_empty(),"{} dark={dark}",preset.id());
            });
        } }
        host.teardown(&mut cx,1);
    }
    #[test]
    fn module_restyle_updates_custom_roles_and_keeps_instance() {
        let mut cx=Cx::new(Box::new(|_,_|{}));
        cx.with_vm(makepad_widgets::script_mod);
        let mut host=ModuleHost::default();
        let module=&makepad_sheets::module::SHEETS_MODULE;
        let open=module.open_schema().validate("{}", &[]).unwrap();
        host.create(&mut cx,1,module,open,dvec2(900.0,700.0)).unwrap();
        let uid=host.get(1).unwrap().root.widget_uid();
        host.apply_style(&mut cx,&desktop_style::StyleSheet::load(desktop_style::DesktopStyle::Macos));
        let instance=host.get(1).unwrap();
        assert_eq!(instance.root.widget_uid(),uid);
        cx.with_script_vm_id_trusted(instance.vm_id,|vm| {
            let palette=makepad_wm_theme::current_for_vm(vm).unwrap();
            assert_eq!(palette.get("background"),Some("#ececec"));
            let sheets=vm.module(id!(sheets));
            assert_eq!(vm.bx.heap.value(sheets,id!(bg).into(),NoTrap).as_color(),Some(0xecececff));
            assert!(vm.take_errors().is_empty());
        });
        host.teardown(&mut cx,1);
    }
}

#[cfg(all(test, any(feature = "app-reference", native_mobile)))]
mod channel_tests {
    use super::*;

    /// The two halves of the host channel a module's `WmRequest` and the
    /// `wm_unavailable` reply travel: a root's widget uid names its client,
    /// and a json message reaches the instance as `Event::Custom` inside
    /// its isolate.
    #[test]
    fn a_root_uid_names_its_client_and_a_custom_event_reaches_the_instance() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        let mut host = ModuleHost::default();
        host.apply_style(&mut cx, &desktop_style::StyleSheet::load(desktop_style::DesktopStyle::Android));
        let module = &octosense_reference::REFERENCE_MODULE;
        host.create(&mut cx, 7, module, module.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
        let root = host.get(7).unwrap().root.clone();
        assert_eq!(host.client_of_root_uid(root.widget_uid()), Some(7));
        assert_eq!(host.client_of_root_uid(WidgetUid(0)), None, "a uid no root minted names nobody");
        let json = crate::wm_reply::WmUnavailable { app: "browser".into(), path: "https://x/a".into() }.to_json();
        assert!(host.send_custom(&mut cx, 7, json.clone()));
        assert!(!host.send_custom(&mut cx, 8, json), "no instance, nothing sent");
        cx.with_script_vm_id_trusted(host.get(7).unwrap().vm_id, |vm| assert!(vm.take_errors().is_empty()));
        // The last refs into the isolate's heap go before the heap does.
        drop(root);
        assert!(host.teardown(&mut cx, 7));
    }
}

// The shell side of apps' assistant access (Rinx ADR 0007) is
// octosense-ai-host (`offer` above); these check it through a real create.
#[cfg(all(test, any(feature = "octos-core", native_mobile)))]
mod assistant_tests {
    use makepad_app_module::*;
    use makepad_widgets::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    static CLAIMED: AtomicBool = AtomicBool::new(false);

    /// A module that declares the assistant and records whether it got one.
    struct Probe(&'static str, &'static [&'static str]);
    impl AppModule for Probe {
        fn id(&self) -> &'static str {
            self.0
        }
        fn label(&self) -> &'static str {
            "Probe"
        }
        fn capabilities(&self) -> &'static [&'static str] {
            self.1
        }
        fn open_schema(&self) -> OpenSchema {
            OpenSchema::new(1)
        }
        fn register(&self, _vm: &mut ScriptVm) {}
        fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, handles: InstanceHandles) -> InstanceParts {
            let service = octosense_ai_host::app_peers::injection::claim(self.0, &handles.scope.to_string());
            CLAIMED.store(service.is_some(), Ordering::SeqCst);
            let value = script_eval!(vm, { use mod.prelude.widgets.* View {} });
            InstanceParts {
                root: WidgetRef::script_from_value(vm, value),
                executor: Box::new(NoExecutor),
                shutdown: Box::new(|_| {}),
            }
        }
    }
    struct NoExecutor;
    impl ServiceExecutor for NoExecutor {
        fn manifest(&self) -> makepad_ai_services::wire::ServiceManifest {
            makepad_ai_services::wire::ServiceManifest::new("probe", "Probe", "test")
        }
        fn execute(&mut self, _cx: &mut Cx, call: &makepad_ai_services::wire::ServiceCall) -> ExecOutcome {
            ExecOutcome::Done(makepad_ai_services::wire::ToolResult::unavailable(&call.call_id, "test"))
        }
    }

    static AI_PROBE: Probe = Probe("assistant-probe", &["storage", "octos.session.open", "octos.turn.start"]);
    static PLAIN_PROBE: Probe = Probe("plain-probe", &["storage", "net"]);
    static UNGRANTED_PROBE: Probe = Probe("ungranted-probe", &["octos.turn.start"]);

    #[test]
    fn a_granted_module_gets_its_service_at_creation_and_others_get_none() {
        octosense_ai_host::grant("assistant-probe", ["octos.session.open", "octos.turn.start"]);
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        let mut host = crate::module_host::ModuleHost::default();
        host.create(&mut cx, 1, &AI_PROBE, AI_PROBE.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
        assert!(CLAIMED.load(Ordering::SeqCst), "the granted module got a scoped service");
        assert!(host.assistant_of(1).is_some());
        host.create(&mut cx, 2, &PLAIN_PROBE, PLAIN_PROBE.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
        assert!(!CLAIMED.load(Ordering::SeqCst), "a module without assistant services gets none");
        assert!(host.assistant_of(2).is_none(), "no peer is allocated for it");
        host.create(&mut cx, 3, &UNGRANTED_PROBE, UNGRANTED_PROBE.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
        assert!(!CLAIMED.load(Ordering::SeqCst), "declaring is not being granted");
        assert!(host.assistant_of(3).is_none());
        // An offer never outlives its create: nothing is left to claim.
        assert!(octosense_ai_host::app_peers::injection::claim("assistant-probe", "i1g1").is_none());
        // The person turns its agent off: the live instance loses its
        // service now, not at its next creation (G10).
        assert_eq!(host.revoke_assistant("plain-probe"), 0);
        assert_eq!(host.revoke_assistant("assistant-probe"), 1);
        assert!(host.assistant_of(1).is_none());
        assert_eq!(host.revoke_assistant("assistant-probe"), 0, "once");
        assert!(host.teardown(&mut cx, 1));
    }

    /// The real Rinx module: hosted from creation, with the shell's service,
    /// and no kernel started by creating it (ADR 0007 criterion 7).
    #[cfg(feature = "app-rinx")]
    #[test]
    fn rinx_is_hosted_with_the_shells_service_and_starts_no_kernel() {
        let _one_rinx = super::RINX_INSTANCE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        let mut host = crate::module_host::ModuleHost::default();
        let module = &rinx::module::RINX_MODULE;
        assert!(module.capabilities().contains(&"octos.turn.start"), "Rinx declares its assistant needs");
        host.create(&mut cx, 7, module, module.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
        assert!(host.assistant_of(7).is_some(), "Home gave Rinx a scoped service");
        assert!(rinx::octos_service::is_hosted(), "hosted mode comes from module creation");
        let service = rinx::octos_service::service().expect("Rinx took the injected service");
        assert_eq!(service.deployment(), octosense_ai_host::app_peers::Deployment::Hosted);
        assert_eq!(service.settings_entry(), octosense_ai_host::app_peers::SettingsEntry::Host);
        assert!(!octosense_ai_host::kernel_running(), "creating Rinx starts no kernel");
        assert!(host.teardown(&mut cx, 7));
    }
}

/// The executor the shell runs for an instance: the module's own, except
/// the Terminal's. Its module answers the read tools only, while ADR 0004
/// §10 gives the Terminal's AI the same tools in every hosting: read, and
/// type a command (`run`), each command behind the shell's live
/// confirmation (`confirm: host` in native-apps.json; `ai_bus` registers it
/// destructive, so the pane parks every call). So the in-process Terminal
/// offers makepad-terminal's full tool set, answered from the live
/// emulator, exactly as its process form does.
fn host_executor(module: &dyn AppModule, root: &WidgetRef, own: Box<dyn ServiceExecutor>) -> Box<dyn ServiceExecutor> {
    #[cfg(feature = "app-terminal")]
    if module.id() == "terminal" {
        return Box::new(TerminalExecutor { root: root.clone() });
    }
    let _ = (module, root);
    own
}

#[cfg(feature = "app-terminal")]
struct TerminalExecutor {
    root: WidgetRef,
}

#[cfg(feature = "app-terminal")]
impl ServiceExecutor for TerminalExecutor {
    fn manifest(&self) -> ServiceManifest {
        makepad_terminal::ai::manifest()
    }

    fn execute(&mut self, cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        let result = active_terminal(cx, &self.root)
            .and_then(|term| term.borrow_mut::<makepad_terminal::widget::MpTerm>().map(|mut term| makepad_terminal::ai::answer(call, &mut *term)))
            .unwrap_or_else(|| makepad_ai_services::wire::ToolResult::unavailable(&call.call_id, "the terminal is not open"));
        ExecOutcome::Done(result)
    }
}

/// The emulator the Terminal's tools act on: the selected tab's focused
/// pane. The module's root is a `TermTabs`, not an `MpTerm`, and
/// `WidgetRef::borrow_mut` downcasts only the root itself, so this walks to
/// the active terminal the way the module's own executor does. `None` when
/// the root is not a `TermTabs` or it has no terminal to give.
#[cfg(feature = "app-terminal")]
fn active_terminal(cx: &mut Cx, root: &WidgetRef) -> Option<WidgetRef> {
    let term = root.borrow_mut::<makepad_terminal::tabs::TermTabs>().map(|mut tabs| tabs.active_term(cx))?;
    (!term.is_empty()).then_some(term)
}

#[cfg(all(test, feature = "app-terminal"))]
mod terminal_tests {
    /// The in-process Terminal offers what its process offers: the reads
    /// and `run`, which the bus then puts behind the host's confirmation.
    #[test]
    fn the_in_process_terminal_offers_the_process_tool_set() {
        use makepad_app_module::ServiceExecutor;
        use makepad_widgets::WidgetRef;
        struct ReadsOnly;
        impl ServiceExecutor for ReadsOnly {
            fn manifest(&self) -> makepad_ai_services::wire::ServiceManifest {
                makepad_terminal::ai::read_only_manifest()
            }
            fn execute(&mut self, _cx: &mut makepad_widgets::Cx, call: &makepad_ai_services::wire::ServiceCall) -> makepad_app_module::ExecOutcome {
                makepad_app_module::ExecOutcome::Done(makepad_ai_services::wire::ToolResult::unavailable(&call.call_id, "test"))
            }
        }
        let names = |m: makepad_ai_services::wire::ServiceManifest| m.tools.into_iter().map(|t| t.name).collect::<Vec<_>>();
        let terminal = super::host_executor(&makepad_terminal::TERMINAL_MODULE, &WidgetRef::empty(), Box::new(ReadsOnly));
        assert_eq!(names(terminal.manifest()), names(makepad_terminal::ai::manifest()));
        assert!(names(terminal.manifest()).contains(&"run".to_string()));
        // On the bus it is `run` behind the host's card, as a process's is.
        let mut bus = crate::ai_bus::AiBus::default();
        let frame = bus.register_local(1, terminal.manifest());
        assert!(frame.contains("\"run\""));
    }

    /// Every tool call reaches the live emulator through the real module's
    /// root. The root is a `TermTabs`, so a lookup that downcasts the root
    /// to `MpTerm` answers "the terminal is not open" for every call; this
    /// runs real calls through `ModuleHost::execute` against the created
    /// module to keep that from coming back.
    #[test]
    fn the_terminals_tools_reach_the_active_terminal() {
        use makepad_ai_services::wire::{ServiceCall, ToolOutcome};
        use makepad_app_module::{AppModule, ExecOutcome};
        use makepad_widgets::{dvec2, Cx};
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        let mut host = crate::module_host::ModuleHost::default();
        let module = &makepad_terminal::TERMINAL_MODULE;
        host.create(&mut cx, 9, module, module.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
        for (tool, args) in [("read_screen", "{}"), ("read_scrollback", r#"{"lines":10}"#), ("run", r#"{"command":"true"}"#)] {
            let call = ServiceCall { call_id: format!("c-{tool}"), tool: tool.into(), args: args.into() };
            let Some(ExecOutcome::Done(result)) = host.execute(&mut cx, 9, &call) else {
                panic!("{tool}: the Terminal answers at once");
            };
            assert_ne!(result.text, "the terminal is not open", "{tool} found no terminal behind the TermTabs root");
            // With no frame drawn the session may not have started yet;
            // either way the call reached the emulator.
            assert!(
                result.outcome == ToolOutcome::Ok || result.text == "the terminal session is not ready",
                "{tool}: {:?} {}",
                result.outcome,
                result.text
            );
        }
        // A root that is not a `TermTabs` still answers plainly.
        let bare = makepad_widgets::WidgetRef::empty();
        assert!(super::active_terminal(&mut cx, &bare).is_none());
        assert!(host.teardown(&mut cx, 9));
    }
}
