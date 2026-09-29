//! Settings → Assistant → Approvals (ADR 0004 §8, §4): the person's view of
//! what approves on its own.
//!
//! - **Rules**: each standing rule with its conditions, today's use of its
//!   daily cap and the minutes left of a time box; Delete (or Turn on for
//!   one that is off); **Turn every rule off** in one tap.
//! - **Use my contacts in approval rules**: off by default; while off,
//!   "people in my contacts" never matches (`contacts.rs`).
//! - **App agents**: every app's agent with its consent and an off switch;
//!   "Everything for 60 min" creates the time-boxed rule for that app.
//! - **Recent automatic approvals**: the audit's newest automatic entries
//!   (rule or developer mode, app, tool, caller, result).
//!
//! Drawn by the shell like the approval sheet (`view.rs`), as a modal page.

use makepad_widgets::*;

use super::audit::Entry;
use super::consent::State;
use super::rules::{ApprovalGesture, Rule, RuleDraft, RuleOrigin, MAX_EVERYTHING_MINUTES};
use super::sheet::app_label;
use super::types::RuleId;
use super::view::Buttons;
use crate::shell::ui::{contains, rect, DrawShellFill, HAlign, ShellDraw};
use crate::shell::{alpha, ShellTokens};

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.ShellApprovalsSettingsBase = #(ShellApprovalsSettings::register_widget(vm))
    mod.widgets.ShellApprovalsSettings = set_type_default() do mod.widgets.ShellApprovalsSettingsBase {
        width: Fill
        height: Fill
        draw_bg +: {}
        d +: {}
    }
}

const PAGE_MAX_W: f64 = 680.0;
const PAD: f64 = 20.0;
const ROW_H: f64 = 40.0;

#[derive(Clone, Debug, PartialEq)]
enum Hit {
    Close,
    AllOff,
    Delete(RuleId),
    Enable(RuleId),
    ContactsOn,
    ContactsOff,
    AgentOff(String),
    AgentAllow(String),
    Everything(String),
    Page,
}

#[derive(Default)]
struct Frame {
    open: bool,
    rules: Vec<Rule>,
    contacts: bool,
    agents: Vec<(String, String, State)>,
    log: Vec<Entry>,
    now: u64,
}

fn frame() -> Frame {
    super::with(|a| {
        if !a.settings_open {
            return Frame::default();
        }
        Frame {
            open: true,
            rules: a.router.rules.rules().to_vec(),
            contacts: a.router.contacts().allowed(),
            agents: a.consent.agents(),
            log: a.router.audit.recent_automatic(8),
            now: super::now(),
        }
    })
    .unwrap_or_default()
}

/// "3 min ago".
pub fn ago(now: u64, ts: u64) -> String {
    let s = now.saturating_sub(ts);
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86_399 => format!("{} h ago", s / 3600),
        _ => format!("{} d ago", s / 86_400),
    }
}

/// The row's second line: cap use, time left, where it came from, state.
pub fn rule_status(rule: &Rule, now: u64) -> String {
    let mut parts = Vec::new();
    if let Some(cap) = rule.daily_cap {
        parts.push(format!("{} of {cap} today", rule.used_today(now)));
    } else {
        parts.push(format!("{} today", rule.used_today(now)));
    }
    if let Some(m) = rule.minutes_left(now) {
        parts.push(format!("{m} min left"));
    }
    if rule.include_incoming {
        parts.push("also for incoming content".into());
    }
    parts.push(match rule.origin {
        RuleOrigin::Settings => "made in Settings".into(),
        RuleOrigin::Sheet => "made on a sheet".into(),
    });
    if !rule.enabled {
        parts.push("off".into());
    }
    parts.join(" \u{00b7} ")
}

/// An audit line: "2 min ago · Mail · mail.send · rule r3 · Calendar's agent · approved".
pub fn log_line(e: &Entry, now: u64) -> String {
    let by = match (&e.rule, e.by.as_str()) {
        (Some(r), _) => format!("rule {r}"),
        (None, "developer_mode") => "developer mode".into(),
        (None, b) => b.to_string(),
    };
    format!("{} \u{00b7} {} \u{00b7} {} \u{00b7} {by} \u{00b7} {} \u{00b7} {}", ago(now, e.ts), app_label(&e.app), e.tool, e.caller, e.result)
}

#[derive(Script, ScriptHook, Widget)]
pub struct ShellApprovalsSettings {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[redraw]
    #[live]
    draw_bg: DrawShellFill,
    #[live]
    d: ShellDraw,
    #[live]
    tokens: ShellTokens,
    #[rust]
    area: Area,
    #[rust]
    hits: Vec<(Rect, Hit)>,
    #[rust]
    open: bool,
    #[rust]
    down: Option<Hit>,
    #[rust]
    hover: Option<Rect>,
    #[rust]
    pub shown: Vec<String>,
}

impl ShellApprovalsSettings {
    fn hit_at(&self, p: Vec2d) -> Option<Hit> {
        self.hits.iter().find(|(r, h)| *h != Hit::Page && contains(*r, p)).or_else(|| self.hits.iter().find(|(r, _)| contains(*r, p))).map(|(_, h)| h.clone())
    }

    /// The shell's pointer hook. Modal while open; a press outside closes.
    pub fn pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        if !self.open {
            return false;
        }
        let (down, up, moved) = match event {
            Event::MouseDown(e) => (Some(e.abs), None, None),
            Event::MouseUp(e) => (None, Some(e.abs), None),
            Event::MouseMove(e) => (None, None, Some(e.abs)),
            Event::TouchUpdate(e) => (
                e.touches.iter().find(|t| t.state == makepad_widgets::makepad_platform::event::TouchState::Start).map(|t| t.abs),
                e.touches.iter().find(|t| t.state == makepad_widgets::makepad_platform::event::TouchState::Stop).map(|t| t.abs),
                None,
            ),
            _ => (None, None, None),
        };
        if let Some(p) = moved {
            let hover = self.hits.iter().find(|(r, h)| *h != Hit::Page && contains(*r, p)).map(|(r, _)| *r);
            if hover != self.hover {
                self.hover = hover;
                self.redraw(cx);
            }
        }
        if let Some(p) = down {
            self.down = Some(self.hit_at(p).unwrap_or(Hit::Close));
        }
        if let Some(p) = up {
            let hit = self.hit_at(p).unwrap_or(Hit::Close);
            if self.down.take().as_ref() == Some(&hit) {
                act(hit);
                self.redraw(cx);
            }
        }
        true
    }

    fn draw_page(&mut self, cx: &mut Cx2d, screen: Rect) {
        self.hits.clear();
        self.shown.clear();
        let f = frame();
        self.open = f.open;
        if !f.open {
            return;
        }
        let tok = self.d.tokens(self.tokens);
        self.d.solid(cx, screen, Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 0.45 });
        let w = PAGE_MAX_W.min(screen.size.x - 32.0).max(240.0);
        let h = (screen.size.y - 64.0).max(200.0);
        let page = rect(screen.pos.x + (screen.size.x - w) * 0.5, screen.pos.y + 32.0, w, h);
        self.d.card(cx, page, &tok.popups);
        self.hits.push((page, Hit::Page));
        let ink = tok.popups.text;
        let dim = alpha(ink, 0.65);
        let x = page.pos.x + PAD;
        let cw = page.size.x - PAD * 2.0;
        let bottom = page.pos.y + page.size.y - PAD;
        let mut y = page.pos.y + PAD;
        let mut hits = Vec::new();
        let mut shown = Vec::new();
        let mut b = Buttons { d: &mut self.d, tok, hover: self.hover };

        b.d.label_elided(cx, rect(x, y, cw * 0.6, 14.0), false, tok.font.caption, dim, HAlign::Left, "Settings \u{203a} Assistant");
        let close_w = b.width(cx, "Close");
        let close = b.draw(cx, x + cw - close_w, y, close_w, "Close", false);
        hits.push((close, Hit::Close));
        y += 16.0;
        b.d.label_elided(cx, rect(x, y, cw - close_w - 8.0, 26.0), true, tok.font.display * 0.75, ink, HAlign::Left, "Approvals");
        shown.push("Approvals".to_string());
        y += 32.0;
        b.d.label_elided(cx, rect(x, y, cw, 16.0), false, tok.font.body_small, dim, HAlign::Left, "Only you approve. Rules answer the approvals you choose, on their exact arguments; everything else asks you.");
        y += 26.0;

        // Rules.
        let section = |b: &mut Buttons, cx: &mut Cx2d, y: &mut f64, title: &str| {
            b.d.label_elided(cx, rect(x, *y, cw, 20.0), true, tok.font.subtitle, ink, HAlign::Left, title);
            *y += 24.0;
        };
        section(&mut b, cx, &mut y, "Rules");
        let any_on = f.rules.iter().any(|r| r.enabled);
        let off = b.draw(cx, x, y, cw, "Turn every rule off", any_on);
        if any_on {
            hits.push((off, Hit::AllOff));
        }
        y += 28.0 + 10.0;
        {
            let (switch, hit) = if f.contacts { ("Turn off", Hit::ContactsOff) } else { ("Turn on", Hit::ContactsOn) };
            let sw = b.width(cx, switch);
            let s = b.draw(cx, x + cw - sw, y + 6.0, sw, switch, false);
            hits.push((s, hit));
            let tw = cw - sw - 12.0;
            let label = "Use my contacts in approval rules";
            let status = if f.contacts {
                "On: your mail accounts and the people you sent mail to"
            } else {
                "Off: \u{201c}people in my contacts\u{201d} never matches"
            };
            b.d.label_elided(cx, rect(x, y, tw, 20.0), true, tok.font.body, ink, HAlign::Left, label);
            b.d.label_elided(cx, rect(x, y + 20.0, tw, 16.0), false, tok.font.body_small, dim, HAlign::Left, status);
            shown.push(format!("{label} | {status}"));
            y += ROW_H + 4.0;
        }
        if f.rules.is_empty() {
            b.d.label_elided(cx, rect(x, y, cw, 18.0), false, tok.font.body, dim, HAlign::Left, "No rules. Every approval asks you.");
            shown.push("No rules. Every approval asks you.".into());
            y += 26.0;
        }
        for rule in &f.rules {
            if y + ROW_H > bottom {
                break;
            }
            let label = rule.describe();
            let status = rule_status(rule, f.now);
            let del_w = b.width(cx, "Delete");
            let del = b.draw(cx, x + cw - del_w, y + 6.0, del_w, "Delete", false);
            hits.push((del, Hit::Delete(rule.id.clone())));
            let mut text_w = cw - del_w - 12.0;
            if !rule.enabled {
                let on_w = b.width(cx, "Turn on");
                let on = b.draw(cx, del.pos.x - on_w - 8.0, y + 6.0, on_w, "Turn on", false);
                hits.push((on, Hit::Enable(rule.id.clone())));
                text_w -= on_w + 8.0;
            }
            b.d.label_elided(cx, rect(x, y, text_w, 20.0), true, tok.font.body, if rule.enabled { ink } else { dim }, HAlign::Left, &label);
            b.d.label_elided(cx, rect(x, y + 20.0, text_w, 16.0), false, tok.font.body_small, dim, HAlign::Left, &status);
            shown.push(format!("{label} | {status}"));
            y += ROW_H + 4.0;
        }
        y += 8.0;

        // App agents.
        if y + 60.0 < bottom {
            section(&mut b, cx, &mut y, "App agents");
            if f.agents.is_empty() {
                b.d.label_elided(cx, rect(x, y, cw, 18.0), false, tok.font.body, dim, HAlign::Left, "No app has asked for its agent yet.");
                y += 26.0;
            }
            for (app, name, state) in &f.agents {
                if y + ROW_H > bottom {
                    break;
                }
                let state_text = match state {
                    State::Allowed => "Allowed",
                    State::Denied => "Off",
                    State::Undecided => "Not asked yet",
                };
                let (switch, hit) = match state {
                    State::Allowed => ("Turn off", Hit::AgentOff(app.clone())),
                    _ => ("Allow", Hit::AgentAllow(app.clone())),
                };
                let every = format!("Everything for {MAX_EVERYTHING_MINUTES} min");
                let sw = b.width(cx, switch);
                let s = b.draw(cx, x + cw - sw, y + 6.0, sw, switch, false);
                hits.push((s, hit));
                let ew = b.width(cx, &every);
                let e = b.draw(cx, s.pos.x - ew - 8.0, y + 6.0, ew, &every, false);
                hits.push((e, Hit::Everything(app.clone())));
                let tw = e.pos.x - x - 12.0;
                b.d.label_elided(cx, rect(x, y, tw, 20.0), true, tok.font.body, ink, HAlign::Left, &format!("{name}'s agent"));
                b.d.label_elided(cx, rect(x, y + 20.0, tw, 16.0), false, tok.font.body_small, dim, HAlign::Left, state_text);
                shown.push(format!("{name}'s agent | {state_text}"));
                y += ROW_H + 4.0;
            }
            y += 8.0;
        }

        // Recent automatic approvals.
        if y + 44.0 < bottom {
            section(&mut b, cx, &mut y, "Recent automatic approvals");
            if f.log.is_empty() {
                b.d.label_elided(cx, rect(x, y, cw, 18.0), false, tok.font.body, dim, HAlign::Left, "None yet.");
            }
            for e in &f.log {
                if y + 20.0 > bottom {
                    break;
                }
                let line = log_line(e, f.now);
                b.d.label_elided(cx, rect(x, y, cw, 18.0), false, tok.font.body_small, ink, HAlign::Left, &line);
                shown.push(line);
                y += 20.0;
            }
        }
        self.hits.extend(hits);
        self.shown = shown;
    }
}

/// What a press does on the page. Creating or re-enabling a rule, allowing
/// an agent and turning contacts on are the person's gestures.
fn act(hit: Hit) {
    let now = super::now();
    super::with(|a| match hit {
        Hit::Close => a.settings_open = false,
        Hit::AllOff => {
            a.router.all_off();
        }
        Hit::Delete(id) => {
            a.router.delete_rule(&id);
        }
        Hit::Enable(id) => {
            a.router.rules.enable(&ApprovalGesture::settings_tap(), &id);
        }
        Hit::ContactsOn => a.router.contacts_mut().allow(&ApprovalGesture::settings_tap(), now),
        Hit::ContactsOff => a.router.contacts_mut().turn_off(now),
        Hit::AgentOff(app) => a.consent.turn_off(&app, now),
        Hit::AgentAllow(app) => a.consent.set(&ApprovalGesture::settings_tap(), &app, true, now),
        Hit::Everything(app) => {
            if let Err(e) = a.router.create_rule(&ApprovalGesture::settings_tap(), RuleDraft::everything(&app, MAX_EVERYTHING_MINUTES), now) {
                log!("approvals: {e}");
            }
        }
        Hit::Page => {}
    });
}

impl Widget for ShellApprovalsSettings {
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let screen = cx.turtle().rect();
        self.d.begin_surface(cx);
        self.draw_page(cx, screen);
        self.d.end_surface(cx);
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, _cx: &mut Cx, _event: &Event, _scope: &mut Scope) {}
}
