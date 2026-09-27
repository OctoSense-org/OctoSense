//! W05 — Typed approval card UI.
//!
//! Surfaces `approval/requested` notifications inline above the composer
//! (between `chat_shell` and `composer_row`) and routes Approve / Deny
//! clicks back through `OctosUiAgent::respond_to_approval`.
//!
//! Imports from `octos_core::ui_protocol`:
//! - `ApprovalDecision` (ui_protocol.rs:566), `ApprovalId` (:85),
//!   `ApprovalRequestedEvent` (:1480), `ApprovalRespondParams` (:572),
//!   `ApprovalTypedDetails` (:1432), `ApprovalCommandDetails` (:1346),
//!   `ApprovalDiffDetails` (:1372), `ApprovalFilesystemDetails` (:1387),
//!   `ApprovalNetworkDetails` (:1397), `approval_kinds` (:34),
//!   `approval_scopes` (:42).
//!
//! Capability gating: when `Capabilities::typed_approvals == false` only
//! `{title, body}` + Approve/Deny render; typed sub-views and the scope
//! dropdown are hidden. See `03-PROTOCOL-CONTRACT.md` § Approval / diff
//! preview.
//!
//! ## Layout choice — queue pane, not interleaved
//!
//! W05 § "Approval card design" allows either inline-with-chat or a sibling
//! queue pane. The chat thread layout is `aichat`-verbatim and `CHAT_DATA`
//! carries no `TurnId` plumbing, so interleaving by turn requires invasive
//! surgery. We pick the queue pane: a vertical stack pinned between
//! `chat_shell` and `composer_row` reading from
//! `APP_STATE.approvals.pending_order`. M3 may revisit interleaving once
//! `ChatMessage` carries `turn_id`.

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use makepad_widgets::*;
use octos_app_store::approvals::ApprovalState;
use octos_core::ui_protocol::{
    approval_kinds, approval_scopes, ApprovalDecision, ApprovalId, ApprovalRequestedEvent,
    ApprovalTypedDetails,
};

use crate::app::sessions::APP_STATE;

/// Capability mirror — `OctosUiAgent` writes here when
/// `TransportEvent::CapabilityNegotiated` lands.
pub static APPROVAL_CAPS: LazyLock<RwLock<ApprovalCapState>> =
    LazyLock::new(|| RwLock::new(ApprovalCapState::default()));

#[derive(Debug, Clone, Copy, Default)]
pub struct ApprovalCapState {
    pub typed_approvals: bool,
    pub pane_snapshots: bool,
}

/// Cross-thread reply for an in-flight `approval/respond`. Posted by the
/// agent's helper task once the wire `oneshot` resolves.
#[derive(Debug)]
pub struct ApprovalAsyncAction {
    pub approval_id: ApprovalId,
    pub decision: ApprovalDecision,
    pub outcome: ApprovalAsyncOutcome,
}

#[derive(Debug)]
pub enum ApprovalAsyncOutcome {
    /// Server acknowledged. `runtime_resumed` (ui_protocol.rs:609) tells
    /// the UI whether the agent's tool call has unblocked — surfaced via
    /// a "queued" indicator in M3 (per W05 § "Reconnect semantics"). For
    /// now we only use it to decide whether the toast nudges the user.
    Accepted {
        #[allow(dead_code)]
        runtime_resumed: bool,
    },
    /// Wire reply was an `RpcError` or transport drop. `code` carries the
    /// JSON-RPC `code` (e.g. `-32011 APPROVAL_NOT_PENDING` from
    /// `octos-cli/src/api/ui_protocol_approvals.rs:12`); `data` mirrors the
    /// server's `data` payload (used to recover `recorded_decision` on a
    /// double-click). `code: 0` means the failure originated client-side
    /// (transport drop, channel closed); `data` is `None` for those.
    Failed {
        message: String,
        code: i64,
        data: Option<serde_json::Value>,
    },
}

/// UI-thread action emitted on Approve/Deny click. `App::handle_actions`
/// forwards to `OctosUiAgent::respond_to_approval` and applies the
/// optimistic `pending_response` transition on `APP_STATE.approvals`.
#[derive(Debug, Clone)]
pub struct ApprovalUiAction {
    pub approval_id: ApprovalId,
    pub session_id: octos_core::SessionKey,
    pub decision: ApprovalDecision,
    pub scope: Option<String>,
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    let RiskBadge = RoundedView {
        width: Fit height: Fit show_bg: true
        padding: Inset{left: 8 right: 8 top: 2 bottom: 2} margin: Inset{right: 6}
        draw_bg +: { color: #x42330F radius: 8.0 }
        risk_label := Label { text: "" draw_text.color: #xF6BE63 draw_text.text_style.font_size: 10 }
    }
    let CardButton = ButtonFlat {
        height: 30 padding: Inset{left: 14 right: 14 top: 0 bottom: 0}
        draw_text +: { color: #xF3E3C7 text_style +: { font_size: 12 } }
        draw_bg +: { color: #x08251EB8 color_hover: #x123B31DD border_color: #xEAD8B82D border_size: 1.0 border_radius: 8.0 }
    }
    let DangerButton = ButtonFlat {
        height: 30 padding: Inset{left: 14 right: 14 top: 0 bottom: 0}
        draw_text +: { color: #xFFFFFF text_style +: { font_size: 12 } }
        draw_bg +: { color: #xA32E2EB8 color_hover: #xC03A3ADD border_color: #xFF6B6B66 border_size: 1.0 border_radius: 8.0 }
    }
    let DimLabel = Label { width: Fill height: Fit text: "" draw_text.color: #xCDBF9FCC draw_text.text_style.font_size: 11 }
    let SubLabel = Label { width: Fill height: Fit text: "" draw_text.color: #x72E4FF draw_text.text_style.font_size: 11 }
    let WarnLabel = Label { width: Fill height: Fit text: "" draw_text.color: #xF6BE63 draw_text.text_style.font_size: 10 }
    let DetailsCode = CodeView {
        keep_cursor_at_end: false
        editor +: { height: Fit draw_bg +: { color: #x031510EE } }
    }

    mod.widgets.ApprovalCardView = #(crate::app::approvals::ApprovalCardWidget::register_widget(vm)) {
        width: Fill height: Fit flow: Down spacing: 6 show_bg: true
        margin: Inset{top: 4 bottom: 4 left: 8 right: 8}
        padding: Inset{left: 14 top: 12 right: 14 bottom: 12}
        draw_bg +: { color: #x0A2A22DD radius: 12.0 }

        header := View {
            width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 4
            risk_badge := RiskBadge {}
            tool_label := Label { text: "" draw_text.color: #x72E4FF draw_text.text_style.font_size: 11 }
        }
        title_label := Label { width: Fill height: Fit text: "" draw_text.color: #xF3E3C7 draw_text.text_style.font_size: 14 }
        body_label := DimLabel {}
        // Typed sub-views — siblings, only one visible at a time.
        typed_command := View {
            width: Fill height: Fit flow: Down spacing: 4 visible: false
            command_line_view := DetailsCode {}
            cwd_label := Label { width: Fill height: Fit text: "" draw_text.color: #xCDBF9F88 draw_text.text_style.font_size: 10 }
            envkeys_label := Label { width: Fill height: Fit text: "" draw_text.color: #xCDBF9F88 draw_text.text_style.font_size: 10 }
        }
        typed_diff := View {
            width: Fill height: Fit flow: Down spacing: 4 visible: false
            diff_summary_label := DimLabel {}
            diff_op_label := SubLabel {}
        }
        typed_filesystem := View {
            width: Fill height: Fit flow: Down spacing: 2 visible: false
            fs_op_label := SubLabel {}
            fs_paths_view := DetailsCode {}
            fs_outside_label := WarnLabel {}
        }
        typed_network := View {
            width: Fill height: Fit flow: Down spacing: 2 visible: false
            net_op_label := SubLabel {}
            net_hosts_label := DimLabel {}
            net_urls_view := DetailsCode {}
        }
        controls_row := View {
            width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 8 margin: Inset{top: 6}
            // `labels` carries display strings only; the wire scope value is
            // resolved in Rust via `scope_at(selected_item())` — see
            // `approvals.rs::scope_at` (mapped to `approval_scopes::REQUEST/TURN/SESSION`).
            // DropDown in this fork has no `values` property; bare `Once` /
            // `Turn` / `Session` were parsed as undefined DSL identifiers and
            // crashed the live evaluator at startup.
            scope_dropdown := DropDown { width: 180 height: 30 labels: ["Just this", "Whole turn", "Session"] }
            spacer := View { width: Fill height: 1 }
            deny_button := DangerButton { text: "Deny" }
            approve_button := CardButton { text: "Approve" }
        }
        decided_row := View {
            width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 6 visible: false
            decided_label := SubLabel {}
        }
    }
    mod.widgets.ApprovalsPane = #(crate::app::approvals::ApprovalsPane::register_widget(vm)) {
        width: Fill height: Fit flow: Down spacing: 4
        padding: Inset{left: 8 right: 8 top: 4 bottom: 4}
        visible: false
        list := PortalList {
            width: Fill height: Fit flow: Down drag_scrolling: false auto_tail: false
            // `use mod.widgets.*` at the head of this script_mod imports
            // widgets that were already in `mod.widgets` *before* the block
            // ran; `ApprovalCardView` is registered three statements up,
            // inside this same block, so the bareword lookup misses it.
            // Reference it by its fully-qualified registry path.
            ApprovalItem := mod.widgets.ApprovalCardView {}
        }
    }
}

#[derive(Clone)]
struct CardSnapshot {
    approval_id: ApprovalId,
    event: ApprovalRequestedEvent,
    state: ApprovalState,
}

/// Outer pane widget. Iterates `APP_STATE.approvals.pending_order` and
/// renders one `ApprovalItem` per row.
#[derive(Script, ScriptHook, Widget)]
pub struct ApprovalsPane {
    #[deref]
    view: View,
}

impl Widget for ApprovalsPane {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let snapshots = collect_visible_snapshots();
        let any_visible = !snapshots.is_empty();
        self.view.set_visible(cx, any_visible);
        if !any_visible {
            return self.view.draw_walk(cx, scope, walk);
        }
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = item.as_portal_list().borrow_mut() {
                list.set_item_range(cx, 0, snapshots.len());
                while let Some(item_id) = list.next_visible_item(cx) {
                    let Some(snap) = snapshots.get(item_id) else { continue };
                    let item_widget = list.item(cx, item_id, id!(ApprovalItem));
                    populate_card(cx, &item_widget, snap);
                    item_widget.draw_all_unscoped(cx);
                }
            }
        }
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            let list = self.view.portal_list(cx, ids!(list));
            if !list.any_items_with_actions(actions) {
                return;
            }
            let snapshots = collect_visible_snapshots();
            for (item_id, item) in list.items_with_actions(actions) {
                let Some(snap) = snapshots.get(item_id) else { continue };
                if item.button(cx, ids!(approve_button)).clicked(actions) {
                    post_decision(snap, ApprovalDecision::Approve, &item, cx);
                } else if item.button(cx, ids!(deny_button)).clicked(actions) {
                    post_decision(snap, ApprovalDecision::Deny, &item, cx);
                }
            }
        }
    }
}

/// Inner card widget. Logic lives in `populate_card` because each instance
/// receives its snapshot from the parent pane during `draw_walk`.
#[derive(Script, ScriptHook, Widget)]
pub struct ApprovalCardWidget {
    #[deref]
    view: View,
}

impl Widget for ApprovalCardWidget {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }
}

fn collect_visible_snapshots() -> Vec<CardSnapshot> {
    let Ok(state) = APP_STATE.read() else { return Vec::new() };
    let mut out = Vec::new();
    let mut seen: HashMap<ApprovalId, ()> = HashMap::new();
    for id in &state.approvals.pending_order {
        let Some(ev) = state.approvals.by_id.get(id) else { continue };
        let st = state
            .approvals
            .state
            .get(id)
            .cloned()
            .unwrap_or(ApprovalState::Awaiting);
        seen.insert(id.clone(), ());
        out.push(CardSnapshot {
            approval_id: id.clone(),
            event: ev.clone(),
            state: st,
        });
    }
    // Decided / Failed entries that haven't been cleared yet.
    for (id, st) in state.approvals.state.iter() {
        if seen.contains_key(id) {
            continue;
        }
        if matches!(st, ApprovalState::Decided { .. } | ApprovalState::Failed(_)) {
            if let Some(ev) = state.approvals.by_id.get(id) {
                out.push(CardSnapshot {
                    approval_id: id.clone(),
                    event: ev.clone(),
                    state: st.clone(),
                });
            }
        }
    }
    out
}

fn populate_card(cx: &mut Cx, item: &WidgetRef, snap: &CardSnapshot) {
    let typed_caps_on = APPROVAL_CAPS
        .read()
        .map(|c| c.typed_approvals)
        .unwrap_or(false);

    let risk = snap.event.risk.as_deref().unwrap_or("");
    item.label(cx, ids!(risk_label))
        .set_text(cx, &risk.to_uppercase());
    item.view(cx, ids!(risk_badge))
        .set_visible(cx, !risk.is_empty());

    item.label(cx, ids!(tool_label))
        .set_text(cx, &snap.event.tool_name);
    item.label(cx, ids!(title_label))
        .set_text(cx, &snap.event.title);
    item.label(cx, ids!(body_label))
        .set_text(cx, &snap.event.body);

    item.view(cx, ids!(typed_command)).set_visible(cx, false);
    item.view(cx, ids!(typed_diff)).set_visible(cx, false);
    item.view(cx, ids!(typed_filesystem)).set_visible(cx, false);
    item.view(cx, ids!(typed_network)).set_visible(cx, false);

    if typed_caps_on {
        if let Some(td) = snap.event.typed_details.as_ref() {
            populate_typed(cx, item, td);
        }
    }
    // TODO(W05): if octos-core later exposes a raw `typed_details_json` for
    // unknown kinds, fall back to a serde_json::Value pretty-print here.

    let (controls_visible, decided_visible, decided_text) = match &snap.state {
        ApprovalState::Awaiting => (true, false, String::new()),
        ApprovalState::PendingResponse { decision } => {
            let subtitle = decision_subtitle(decision)
                .map(|s| format!(" — {s}"))
                .unwrap_or_default();
            (
                true,
                false,
                format!(
                    "waiting on server… ({}){}",
                    decision_label(decision.clone()),
                    subtitle
                ),
            )
        }
        ApprovalState::Decided { decision } => {
            let subtitle = decision_subtitle(decision)
                .map(|s| format!(" — {s}"))
                .unwrap_or_default();
            (
                false,
                true,
                format!("decided: {}{}", decision_label(decision.clone()), subtitle),
            )
        }
        ApprovalState::Failed(msg) => (false, true, format!("failed: {msg}")),
    };
    item.view(cx, ids!(controls_row))
        .set_visible(cx, controls_visible);
    item.view(cx, ids!(decided_row))
        .set_visible(cx, decided_visible);
    if decided_visible || matches!(snap.state, ApprovalState::PendingResponse { .. }) {
        item.label(cx, ids!(decided_label))
            .set_text(cx, &decided_text);
    }

    let pending = matches!(snap.state, ApprovalState::PendingResponse { .. });
    item.button(cx, ids!(approve_button)).set_visible(cx, !pending);
    item.button(cx, ids!(deny_button)).set_visible(cx, !pending);
    item.drop_down(cx, ids!(scope_dropdown))
        .set_visible(cx, typed_caps_on && controls_visible);

    if let Some(hints) = snap.event.render_hints.as_ref() {
        if let Some(label) = hints.primary_label.as_deref() {
            item.button(cx, ids!(approve_button)).set_text(cx, label);
        }
        if let Some(label) = hints.secondary_label.as_deref() {
            item.button(cx, ids!(deny_button)).set_text(cx, label);
        }
    }
}

fn populate_typed(cx: &mut Cx, item: &WidgetRef, td: &ApprovalTypedDetails) {
    match td.kind.as_str() {
        approval_kinds::COMMAND => {
            if let Some(cmd) = td.command.as_ref() {
                item.view(cx, ids!(typed_command)).set_visible(cx, true);
                let cmd_line = cmd
                    .command_line
                    .clone()
                    .unwrap_or_else(|| cmd.argv.join(" "));
                item.widget(cx, ids!(command_line_view))
                    .set_text(cx, &cmd_line);
                let cwd = cmd.cwd.as_deref().unwrap_or("");
                let cwd_text = if cwd.is_empty() { String::new() } else { format!("cwd: {cwd}") };
                item.label(cx, ids!(cwd_label)).set_text(cx, &cwd_text);
                item.label(cx, ids!(cwd_label))
                    .set_visible(cx, !cwd_text.is_empty());
                let env = if cmd.env_keys.is_empty() {
                    String::new()
                } else {
                    format!("env: {}", cmd.env_keys.join(", "))
                };
                item.label(cx, ids!(envkeys_label)).set_text(cx, &env);
                item.label(cx, ids!(envkeys_label))
                    .set_visible(cx, !env.is_empty());
            }
        }
        approval_kinds::DIFF => {
            if let Some(d) = td.diff.as_ref() {
                item.view(cx, ids!(typed_diff)).set_visible(cx, true);
                let summary = d.summary.clone().unwrap_or_else(|| {
                    let files = d.file_count.unwrap_or(0);
                    let adds = d.additions.unwrap_or(0);
                    let dels = d.deletions.unwrap_or(0);
                    format!("{files} file(s), +{adds} / -{dels}")
                });
                item.label(cx, ids!(diff_summary_label)).set_text(cx, &summary);
                let op = d.operation.clone().unwrap_or_default();
                let op_text = if op.is_empty() { String::new() } else { format!("op: {op}") };
                item.label(cx, ids!(diff_op_label)).set_text(cx, &op_text);
                // TODO(W05.diff): hydrate via diff/preview/get and render
                // hunks via CodeView (aichat:404-411, :510-543). Deep
                // DiffView is M3 work.
            }
        }
        approval_kinds::FILESYSTEM => {
            if let Some(fs) = td.filesystem.as_ref() {
                item.view(cx, ids!(typed_filesystem)).set_visible(cx, true);
                item.label(cx, ids!(fs_op_label))
                    .set_text(cx, &format!("op: {}", fs.operation));
                item.widget(cx, ids!(fs_paths_view))
                    .set_text(cx, &fs.paths.join("\n"));
                let outside = if fs.outside_workspace {
                    "writes outside workspace".to_owned()
                } else {
                    String::new()
                };
                item.label(cx, ids!(fs_outside_label)).set_text(cx, &outside);
                item.label(cx, ids!(fs_outside_label))
                    .set_visible(cx, !outside.is_empty());
            }
        }
        approval_kinds::NETWORK => {
            if let Some(n) = td.network.as_ref() {
                item.view(cx, ids!(typed_network)).set_visible(cx, true);
                item.label(cx, ids!(net_op_label))
                    .set_text(cx, &format!("op: {}", n.operation));
                let mut hosts = n.hosts.join(", ");
                if !n.ports.is_empty() {
                    let ports = n.ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ");
                    hosts = if hosts.is_empty() {
                        format!("ports: {ports}")
                    } else {
                        format!("{hosts}  ports: {ports}")
                    };
                }
                item.label(cx, ids!(net_hosts_label)).set_text(cx, &hosts);
                item.widget(cx, ids!(net_urls_view))
                    .set_text(cx, &n.urls.join("\n"));
            }
        }
        _ => {
            // Unknown / future kind — body markdown stays the only context.
            // Forward-compat per `03-PROTOCOL-CONTRACT.md` § Capability
            // negotiation. Sandbox-escalation falls in here for now: the
            // protocol exposes the type but we don't have a dedicated
            // sub-view yet (TODO M3 W05.escalation).
        }
    }
    let _ = approval_kinds::SANDBOX_ESCALATION;
}

/// Render label for an `ApprovalDecision`. Forward-compat per spec § 4.1:
/// `Unknown(_)` decisions render as "unrecognized" so a future decision
/// kind doesn't crash the UI; the raw wire string is shown via
/// `decision_subtitle` so a user can see why.
fn decision_label(d: ApprovalDecision) -> &'static str {
    match d {
        ApprovalDecision::Approve => "approve",
        ApprovalDecision::Deny => "deny",
        ApprovalDecision::Unknown(_) => "unrecognized",
    }
}

/// Optional subtitle that surfaces the raw wire string for an `Unknown`
/// decision so users can introspect what the server actually sent.
fn decision_subtitle(d: &ApprovalDecision) -> Option<String> {
    match d {
        ApprovalDecision::Unknown(raw) => Some(format!("raw: {raw}")),
        _ => None,
    }
}

fn scope_at(idx: usize) -> Option<&'static str> {
    match idx {
        0 => Some(approval_scopes::REQUEST),
        1 => Some(approval_scopes::TURN),
        2 => Some(approval_scopes::SESSION),
        _ => None,
    }
}

fn read_scope(item: &WidgetRef, cx: &mut Cx) -> Option<String> {
    let dd = item.drop_down(cx, ids!(scope_dropdown));
    let idx = dd.selected_item();
    scope_at(idx).map(|s| s.to_owned())
}

fn post_decision(snap: &CardSnapshot, decision: ApprovalDecision, item: &WidgetRef, cx: &mut Cx) {
    if matches!(snap.state, ApprovalState::PendingResponse { .. } | ApprovalState::Decided { .. })
    {
        return;
    }
    let scope = read_scope(item, cx);
    Cx::post_action(ApprovalUiAction {
        approval_id: snap.approval_id.clone(),
        session_id: snap.event.session_id.clone(),
        decision,
        scope,
    });
}
