//! W04 / M2 — TaskDock widget under the chat composer.
//!
//! Renders the live `tool/*` and `task/*` notifications buffered into the
//! global `AppState` by the OctosUiAgent (see
//! `app/src/backend/octos_ui.rs::translate_notification`). Listens to
//! `octos_app_store::tasks::{Task, ToolCall}` via the same `APP_STATE` global
//! that backs the session list (`app/src/app/sessions.rs:39`).
//!
//! Notification types (octos-core ui_protocol.rs):
//!   - `ToolStartedEvent`   — :1311
//!   - `ToolProgressEvent`  — :1321
//!   - `ToolCompletedEvent` — :1332
//!   - `TaskUpdatedEvent`   — :1531
//!   - `TaskOutputDeltaEvent` — :1541
//!
//! Two visual states:
//!   - **Collapsed**: a thin pill with `🔧 N tools · M tasks · X% running`
//!     and an expand chevron. When zero tools and zero tasks, the pill shrinks
//!     to zero height (idle behaviour, per the W04 brief).
//!   - **Expanded**: a small column of dock rows, one per ToolCall + Task,
//!     filtered to the current session if any. Each row shows an icon, name,
//!     status pip and a placeholder "tap to view tail" affordance.
//!
//! Read-only access to `APP_STATE` — the widget never writes. The drain of
//! events into the store happens upstream in `octos_ui::translate`. Mirrors
//! the read-only-widget pattern in `app/src/app/sessions.rs:185-230`.
//!
//! Smoothing animation lifted from `aichat/examples/aichat/src/main.rs:480`
//! (`RubberView { smoothing: 0.3 }` wrapping the assistant message body); the
//! same wrapper handles our expand/collapse transition without a custom
//! Animator block.

use makepad_widgets::*;
use octos_app_store::state::AppState;
use octos_app_store::tasks::{Task, ToolCall};
use octos_core::SessionKey;

use crate::app::sessions::APP_STATE;

/// Per-row snapshot taken under the `APP_STATE` read lock so the lock is
/// released before the redraw / widget-mutation loop runs. Mirrors the
/// `RowSnapshot` pattern in `sessions.rs:271`.
#[derive(Clone, Debug)]
struct DockRow {
    /// Icon glyph — wrench for tool calls, gear for tasks.
    icon: &'static str,
    /// Short label rendered in the row.
    name: String,
    /// One of `running`, `done`, `error`. Drives the status pip color.
    status: RowStatus,
    /// Trailing detail line (lifecycle phase / output preview / nothing).
    detail: Option<String>,
    /// `tool_call_id` or `task_id` as a string — surfaced to the App for
    /// `task/output/read` lookup once the M2 stretch lands.
    #[allow(dead_code)]
    correlation_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowStatus {
    Running,
    Done,
    Error,
}

/// Aggregate header counts driving the collapsed pill.
#[derive(Clone, Copy, Debug, Default)]
struct DockHeader {
    tool_count: usize,
    task_count: usize,
    /// Number of tool+task rows still in `Running`. Pairs with the totals
    /// to render "running X/Y" in the pill.
    running_count: usize,
    /// Average `tool/progress.progress_pct` across tools that report a
    /// fraction (`Some(p)`); `None` if no tool reports progress yet.
    /// W04 follow-up #4 — see octos-core ui_protocol.rs:1328.
    avg_progress_pct: Option<f32>,
}

impl DockHeader {
    fn label(&self) -> String {
        let total = self.tool_count + self.task_count;
        let mut s = format!(
            "🔧 {} tools · {} tasks · running {}/{}",
            self.tool_count, self.task_count, self.running_count, total,
        );
        if let Some(p) = self.avg_progress_pct {
            s.push_str(&format!(" · {:.0}%", (p * 100.0).clamp(0.0, 100.0)));
        }
        s
    }

    fn is_idle(&self) -> bool {
        self.tool_count == 0 && self.task_count == 0
    }
}

fn project_tool(tc: &ToolCall) -> DockRow {
    let status = match (tc.success, tc.completed_at) {
        (None, None) => RowStatus::Running,
        (Some(false), _) => RowStatus::Error,
        _ => RowStatus::Done,
    };
    DockRow {
        icon: "🔧",
        name: if tc.tool_name.is_empty() {
            "<tool>".into()
        } else {
            tc.tool_name.clone()
        },
        status,
        detail: tc.output_preview.as_ref().map(|s| short_line(s, 64)),
        correlation_id: tc.id.as_str().to_owned(),
    }
}

fn project_task(t: &Task) -> DockRow {
    let status = match t.runtime_state.as_str() {
        "completed" => RowStatus::Done,
        "failed" => RowStatus::Error,
        _ => RowStatus::Running,
    };
    let detail = if !t.lifecycle_state.is_empty() {
        Some(t.lifecycle_state.clone())
    } else {
        None
    };
    DockRow {
        icon: "⚙",
        name: t.summary.clone().unwrap_or_else(|| "task".to_owned()),
        status,
        detail,
        correlation_id: format!("{}", t.id),
    }
}

fn short_line(s: &str, max_chars: usize) -> String {
    let trimmed = s.trim();
    let mut out = String::new();
    for (i, ch) in trimmed.chars().enumerate() {
        if i >= max_chars {
            out.push('…');
            break;
        }
        if ch == '\n' || ch == '\r' {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

/// Pull the snapshot under a single read lock, optionally filtered by the
/// current session (matches the W04 brief: "Events carry `session_id`;
/// non-current drop").
fn snapshot(state: &AppState, current: Option<&SessionKey>) -> (Vec<DockRow>, DockHeader) {
    let belongs = |sid: &SessionKey| -> bool {
        match current {
            Some(c) => sid == c,
            None => true,
        }
    };

    let mut rows: Vec<DockRow> = Vec::new();
    let mut header = DockHeader::default();
    let mut progress_acc = 0.0f32;
    let mut progress_n = 0u32;

    for tc in state.tool_calls.values() {
        if !belongs(&tc.session_id) {
            continue;
        }
        let row = project_tool(tc);
        if matches!(row.status, RowStatus::Running) {
            header.running_count += 1;
        }
        if let Some(p) = tc.progress_pct {
            progress_acc += p;
            progress_n += 1;
        }
        rows.push(row);
        header.tool_count += 1;
    }
    for t in state.tasks.values() {
        if !belongs(&t.session_id) {
            continue;
        }
        let row = project_task(t);
        if matches!(row.status, RowStatus::Running) {
            header.running_count += 1;
        }
        rows.push(row);
        header.task_count += 1;
    }

    if progress_n > 0 {
        header.avg_progress_pct = Some(progress_acc / progress_n as f32);
    }
    (rows, header)
}

/// `TaskDock` — collapsible dock under the composer.
///
/// Pattern: read-only `View` wrapper that re-projects `APP_STATE` on each
/// `draw_walk`. Click on the header pill toggles `expanded`. The widget owns
/// no event-handling state beyond `expanded`; all data flows from
/// `APP_STATE` (W04 § "Task dock sub-surface").
#[derive(Script, ScriptHook, Widget)]
pub struct TaskDock {
    #[deref]
    view: View,
    /// User toggled the dock open. Flipped by clicks on the header pill.
    /// `[rust]` so it survives DSL re-instantiation; default is `false`.
    #[rust]
    expanded: bool,
}

impl Widget for TaskDock {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Snapshot under one read lock — release before any `set_text` /
        // `set_visible` allocates. Lifted shape from `sessions.rs:190-202`.
        let (rows, header) = {
            let state = match APP_STATE.read() {
                Ok(g) => g,
                Err(_) => return DrawStep::done(),
            };
            let current = state.current_session.clone();
            snapshot(&state, current.as_ref())
        };

        // Idle: zero tools, zero tasks. Hide everything so we don't reserve
        // vertical space (per the brief's "Idle behaviour" requirement).
        let idle = header.is_idle();
        self.view.set_visible(cx, !idle);
        if idle {
            return self.view.draw_walk(cx, scope, walk);
        }

        // Header pill: `[chevron] 🔧 N tools · M tasks · X% running`.
        let pill = self.view.button(cx, ids!(header_pill));
        pill.set_text(cx, &header.label());

        let chevron = self.view.label(cx, ids!(chevron));
        chevron.set_text(cx, if self.expanded { "▾" } else { "▸" });

        // Body — the row list lives inside a `RubberView` (smoothing 0.3,
        // mirrors `aichat:480`) so expand/collapse fades the height change.
        let body = self.view.view(cx, ids!(body));
        body.set_visible(cx, self.expanded);

        if self.expanded {
            // Up to 8 rows in M2; the proper PortalList virtualisation lands
            // when we wire `task/output/read` pagination (see open question in
            // W04 § 14). 8 is enough for typical concurrent-task counts.
            let row_widgets = [
                self.view.view(cx, ids!(row_0)),
                self.view.view(cx, ids!(row_1)),
                self.view.view(cx, ids!(row_2)),
                self.view.view(cx, ids!(row_3)),
                self.view.view(cx, ids!(row_4)),
                self.view.view(cx, ids!(row_5)),
                self.view.view(cx, ids!(row_6)),
                self.view.view(cx, ids!(row_7)),
            ];
            for (i, slot) in row_widgets.iter().enumerate() {
                if let Some(r) = rows.get(i) {
                    slot.set_visible(cx, true);
                    slot.label(cx, ids!(row_icon)).set_text(cx, r.icon);
                    slot.label(cx, ids!(row_name)).set_text(cx, &r.name);
                    let status_text = match r.status {
                        RowStatus::Running => "● running",
                        RowStatus::Done => "✓ done",
                        RowStatus::Error => "✗ error",
                    };
                    slot.label(cx, ids!(row_status)).set_text(cx, status_text);
                    let detail = slot.label(cx, ids!(row_detail));
                    if let Some(d) = &r.detail {
                        detail.set_text(cx, d);
                        detail.set_visible(cx, true);
                    } else {
                        // Stretch — placeholder for tail viewer until the M2
                        // `task/output/read` plumbing lands. Stays hidden when
                        // there's no preview to avoid empty rows.
                        detail.set_visible(cx, false);
                    }
                } else {
                    slot.set_visible(cx, false);
                }
            }
            // Overflow indicator — when more than 8 rows, surface the count.
            let overflow = self.view.label(cx, ids!(overflow));
            if rows.len() > 8 {
                overflow.set_visible(cx, true);
                overflow.set_text(cx, &format!("+{} more", rows.len() - 8));
            } else {
                overflow.set_visible(cx, false);
            }
        }

        self.view.draw_walk(cx, scope, walk)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            // Click on the header pill toggles the expanded state. Chevron is
            // a separate label; the pill behind it is a `ButtonFlat` covering
            // the whole row.
            if self.view.button(cx, ids!(header_pill)).clicked(actions) {
                self.expanded = !self.expanded;
                self.view.redraw(cx);
            }
        }
    }
}
