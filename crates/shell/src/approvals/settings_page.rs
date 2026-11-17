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
//! Drawn by the shell like the approval sheet (`view.rs`), as a modal page. On a
//! Mac it is a sheet with Done, on System Settings' window when that is in front
//! (the page is one of its own), and it stops above the Dock: grouped lists, a
//! switch where something is on or off, one control to a row, a rule by a short
//! title with its conditions in its details (the ⓘ beside it), Turn Every Rule
//! Off under the rules, and the time box in one row of its own with a pull-down
//! of the apps. What a switch sets applies at once; Done puts the sheet away.
//! Each control does exactly what its button does on every other style ([`Hit`]).

use makepad_widgets::*;

use super::audit::Entry;
use super::consent::State;
use super::mac_kit::*;
use super::rules::{ApprovalGesture, Rule, RuleDraft, RuleOrigin, MAX_EVERYTHING_MINUTES};
use super::sheet::app_label;
use super::types::RuleId;
use super::view::Buttons;
use super::words;
use crate::desk::WmState;
use crate::octosense::style::AppIconDraw;
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

/// A Mac's sheet.
const PANEL_W: f64 = 640.0;
const PANEL_MAX_H: f64 = 680.0;
const PANEL_PAD: f64 = 20.0;
const PANEL_FOOT: f64 = 52.0;
const PANEL_ROW_PAD: f64 = 12.0;
const PANEL_ICON: f64 = 24.0;
/// The Assistant's icon at the head of the sheet.
const PANEL_HEAD_ICON: f64 = 40.0;
/// A rule's details button (ⓘ).
const INFO: f64 = 20.0;
/// A rule's details: a sheet on the sheet.
const DETAILS_W: f64 = 440.0;
const DETAILS_ICON: f64 = 32.0;
/// The page is one of System Settings': its sheet goes on that window.
const SETTINGS_APP: &str = "os.settings";
const ALL_OFF: &str = "Turn Every Rule Off";
const INCOMING_NOTE: &str = "Anything started by what someone else sent still asks you.";
const PANEL_INTRO: &str = "Assistants ask you before they act. A rule answers for you only when a request matches it exactly, and everything else still asks.";
const TIME_BOX_CHOOSE: &str = "Choose an App\u{2026}";
const TIME_BOX_NOTE: &str = "Except what always needs your approval. Anything started by what someone else sent still asks you.";

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
    /// A Mac's pull-down of the apps a time box can be for: it opens, or a press
    /// outside closes it.
    TimeBox { button: Rect },
    TimeBoxDismiss,
    /// A rule's ⓘ: its details, over the sheet.
    Details(RuleId),
    /// The details' Done.
    DetailsDone,
}

/// A row of a Mac's panel.
struct PanelRow {
    title: Vec<String>,
    note: Vec<String>,
    /// For the control surface: the row's words, whole.
    said: String,
    h: f64,
    kind: PanelKind,
}

enum PanelKind {
    /// Words only (a muted first line when `dim`).
    Plain { dim: bool },
    Contacts { on: bool },
    /// A rule, and what its controls call it: its title, with its conditions
    /// when another rule has the same title.
    Rule { rule: Rule, name: String },
    TimeBox,
    Agent { app: String, state: State },
}

struct PanelSection {
    /// `None`: a group with no heading, as a Mac's settings that follow a list.
    title: Option<&'static str>,
    rows: Vec<PanelRow>,
    /// Turn Every Rule Off goes under this list.
    all_off: bool,
}

/// Whether a rule is a time box that is running now.
fn time_box_runs(rule: &Rule, now: u64) -> bool {
    rule.is_everything() && rule.enabled && !rule.expired(now)
}

/// The time box's row, in words.
fn time_box_title() -> String {
    format!("Approve everything one app asks for {}", if MAX_EVERYTHING_MINUTES == 60 { "1 hour".to_string() } else { format!("{MAX_EVERYTHING_MINUTES} minutes") })
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
/// An app agent's row: its consent state, and (ADR 0004 §11) that the
/// memory of a signed-out or removed account's agent remains in octos.
pub fn agent_state_text(state: &State, memory_notice: Option<String>) -> String {
    let state = match state {
        State::Allowed => "Allowed",
        State::Denied => "Off",
        State::Undecided => "Not asked yet",
    };
    match memory_notice {
        Some(notice) => format!("{state} \u{00b7} {notice}"),
        None => state.to_owned(),
    }
}

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
    /// A Mac's desk, read once a frame; `None` on every other style.
    #[rust]
    mac: Option<MacPlace>,
    #[rust]
    app_icons: AppIconDraw,
    #[rust]
    scroller: Scroller,
    /// Controls outside this have nothing to press, so they are not listed.
    #[rust]
    note_clip: Option<Rect>,
    /// The time box's pull-down, while its menu is open.
    #[rust]
    popup: Option<Rect>,
    /// What each of the open menu's rows does.
    #[rust]
    menu_rows: Vec<Hit>,
    /// The row of the open menu that the arrow keys are on.
    #[rust]
    popup_cursor: Option<usize>,
    /// Where the press that opened the menu came down, until its release.
    #[rust]
    popup_press: Option<Vec2d>,
    /// On a Mac, the time box's menu as the page laid it out this frame: drawn
    /// over all of the page in a list of its own ([`Self::draw_popup`]).
    #[rust]
    popup_look: Option<PopupLook>,
    #[rust]
    popup_list: Option<DrawList2d>,
    /// The rule whose details are up, over the sheet.
    #[rust]
    details: Option<RuleId>,
    /// The sheet is under a rule's details: its controls take no press, so
    /// they are not listed.
    #[rust]
    covered: bool,
    #[rust]
    pointer_at: Option<Vec2d>,
    #[rust]
    down_rect: Option<Rect>,
    /// The buttons drawn, for the control surface.
    #[rust]
    controls: Vec<String>,
    #[area]
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
    /// The shell's theme (light or dark) and material, as its other
    /// surfaces take them (desktop_app.rs `apply_material_to_chrome`): a
    /// sheet is lifted with them, so it stays over the chat panes.
    pub fn set_material(&mut self, m: crate::shell::MaterialTokens, palette: Option<crate::shell::ShellPalette>) {
        self.d.set_material(m);
        self.d.set_palette(palette);
    }

    fn hit_at(&self, p: Vec2d) -> Option<Hit> {
        self.hits.iter().find(|(r, h)| *h != Hit::Page && contains(*r, p)).or_else(|| self.hits.iter().find(|(r, _)| contains(*r, p))).map(|(_, h)| h.clone())
    }

    /// The shell's pointer hook. Modal while open; a press outside closes.
    pub fn pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        if !self.open {
            return false;
        }
        if self.mac.is_some() {
            return self.mac_pointer(cx, event);
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
        self.controls.clear();
        let f = frame();
        self.open = f.open;
        if !f.open {
            self.popup = None;
            self.details = None;
            self.scroller.clear();
            return;
        }
        if let Some(place) = self.mac.clone() {
            // A Mac lists apps by name.
            let mut f = f;
            f.agents.sort_by_cached_key(|(_, name, _)| name.to_lowercase());
            self.mac_page(cx, screen, &place, &f);
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
                let state_text = agent_state_text(state, crate::app_storage::host().and_then(|s| crate::app_storage::lifecycle::memory_notice(s, app)));
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
                b.d.label_elided(cx, rect(x, y + 20.0, tw, 16.0), false, tok.font.body_small, dim, HAlign::Left, &state_text);
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

/// A Mac's sheet.
impl ShellApprovalsSettings {
    fn panel_sections(&mut self, cx: &mut Cx2d, f: &Frame, width: f64) -> Vec<PanelSection> {
        let mut sections = Vec::new();
        let text_room = |right: f64, left: f64| (width - PANEL_ROW_PAD * 2.0 - right - left - THUMB_GAP).max(120.0);
        let row = |this: &mut Self, cx: &mut Cx2d, title: &str, note: &str, room: f64, kind: PanelKind| {
            let t = wrapped(&mut this.d, cx, false, TEXT, title, room);
            let n = if note.is_empty() { Vec::new() } else { wrapped(&mut this.d, cx, false, NOTE, note, room) };
            let h = (t.len() as f64 * TEXT_LINE + n.len() as f64 * NOTE_LINE + 16.0).max(36.0);
            let said = if note.is_empty() { title.to_string() } else { format!("{title} | {note}") };
            PanelRow { title: t, note: n, said, h, kind }
        };
        // The rules, each by a short title and its state: its conditions are in its details.
        let mut rules = Vec::new();
        if f.rules.is_empty() {
            rules.push(row(self, cx, "No rules. Every approval asks you.", "", text_room(0.0, 0.0), PanelKind::Plain { dim: true }));
        }
        let titles: Vec<String> = f.rules.iter().map(words::rule_title).collect();
        for (rule, title) in f.rules.iter().zip(&titles) {
            // Two rules of one title are told apart by their conditions.
            let twin = titles.iter().filter(|t| *t == title).count() > 1;
            let qualifier = if twin { words::rule_qualifier(rule) } else { String::new() };
            let state = words::rule_state_said(rule, f.now);
            let (note, name) = if qualifier.is_empty() { (state, title.clone()) } else { (format!("{qualifier} \u{00b7} {state}"), format!("{title} ({qualifier})")) };
            let button_w = button_small_width(&mut self.d, cx, rule_button(rule, f.now));
            rules.push(row(self, cx, title, &note, text_room(INFO + 6.0 + button_w + 12.0, PANEL_ICON + 10.0), PanelKind::Rule { rule: rule.clone(), name }));
        }
        sections.push(PanelSection { title: Some("Rules"), rows: rules, all_off: true });
        // What makes a time box, and what "my contacts" means to a rule: settings, not rules.
        let mut options = Vec::new();
        if !f.agents.is_empty() {
            let popup_w = popup_width(&mut self.d, cx, &[TIME_BOX_CHOOSE.to_string()], 12.0, 120.0);
            options.push(row(self, cx, &time_box_title(), TIME_BOX_NOTE, text_room(popup_w + 12.0, 0.0), PanelKind::TimeBox));
        }
        let label = "Use my contacts in approval rules";
        let status = if f.contacts { "On: your mail accounts and the people you sent mail to" } else { "Off: \u{201c}people in my contacts\u{201d} never matches" };
        options.push(row(self, cx, label, status, text_room(SWITCH_W + 12.0, 0.0), PanelKind::Contacts { on: f.contacts }));
        sections.push(PanelSection { title: None, rows: options, all_off: false });
        // App agents.
        let mut agents = Vec::new();
        if f.agents.is_empty() {
            agents.push(row(self, cx, "No app has asked for its assistant yet.", "", text_room(0.0, 0.0), PanelKind::Plain { dim: true }));
        }
        for (app, name, state) in &f.agents {
            let mut note = agent_state_text(state, crate::app_storage::host().and_then(|s| crate::app_storage::lifecycle::memory_notice(s, app)));
            if let Some(rule) = f.rules.iter().find(|r| r.app == *app && time_box_runs(r, f.now)) {
                note.push_str(&format!(" \u{00b7} Approving everything, {} min left", rule.minutes_left(f.now).unwrap_or(0)));
            }
            agents.push(row(self, cx, name, &note, text_room(SWITCH_W + 12.0, PANEL_ICON + 10.0), PanelKind::Agent { app: app.clone(), state: state.clone() }));
        }
        sections.push(PanelSection { title: Some("App assistants"), rows: agents, all_off: false });
        // Recent automatic approvals.
        let mut recent = Vec::new();
        if f.log.is_empty() {
            recent.push(row(self, cx, "None yet.", "", text_room(0.0, 0.0), PanelKind::Plain { dim: true }));
        }
        for entry in &f.log {
            let (what, when) = words::log_said(entry, f.now);
            recent.push(row(self, cx, &what, &when, text_room(0.0, 0.0), PanelKind::Plain { dim: false }));
        }
        sections.push(PanelSection { title: Some("Recent automatic approvals"), rows: recent, all_off: false });
        sections
    }

    fn note_control(&mut self, label: &str, r: Rect, on: bool) {
        // Under a rule's details nothing takes a press, so nothing there is listed.
        if self.covered {
            return;
        }
        let middle = dvec2(r.pos.x + r.size.x * 0.5, r.pos.y + r.size.y * 0.5);
        if self.note_clip.is_some_and(|area| !contains(area, middle)) {
            return;
        }
        self.controls.push(format!("\u{27e6}{label}|{}|{}|{}|{}|{}\u{27e7}", r.pos.x, r.pos.y, r.size.x, r.size.y, if on { "on" } else { "off" }));
    }

    fn state_of(&self, r: Rect) -> (bool, bool) {
        (self.hover == Some(r), self.down_rect == Some(r) && self.hover == Some(r))
    }

    fn mac_page(&mut self, cx: &mut Cx2d, screen: Rect, place: &MacPlace, f: &Frame) {
        let k = mac_ink(place.dark);
        self.covered = false;
        // Details of a rule that has gone (it ran out, or it was deleted) go with it.
        if self.details.as_ref().is_some_and(|id| !f.rules.iter().any(|r| r.id == *id)) {
            self.details = None;
        }
        let w = PANEL_W.min(screen.size.x - 32.0).max(320.0);
        let inner = w - PANEL_PAD * 2.0;
        // The head, as a sheet's: the Assistant's icon, the title and what the page is for.
        let head_w = inner - PANEL_HEAD_ICON - 14.0;
        let intro = wrapped(&mut self.d, cx, false, NOTE, PANEL_INTRO, head_w);
        let head_h = (TEXT_LINE + 2.0 + intro.len() as f64 * NOTE_LINE).max(PANEL_HEAD_ICON);
        let sections = self.panel_sections(cx, f, inner);
        let content = 8.0 + sections.iter().map(section_h).sum::<f64>();
        // A sheet: on System Settings' window when that is in front, the page being one of
        // its own; else where a sheet with no window of its own goes. Never under the Dock.
        let window = related_window(place, &[SETTINGS_APP]).filter(|win| win.rect.size.x >= w + 24.0).cloned();
        let floor = screen.pos.y + screen.size.y - place.dock - 12.0;
        let top = window.as_ref().map_or(screen.pos.y + 16.0, |win| (win.rect.pos.y + win.top).max(screen.pos.y + 8.0));
        let h = (PANEL_PAD + head_h + 8.0 + content + PANEL_FOOT).min(PANEL_MAX_H).min(floor - top).max(200.0);
        let page = match &window {
            Some(win) => {
                let over = crate::system_chat::view::mac::sheet_room(win.rect, w);
                let x = (over.pos.x + (over.size.x - w) * 0.5).clamp(screen.pos.x + 8.0, (screen.pos.x + screen.size.x - w - 8.0).max(screen.pos.x + 8.0));
                rect(x.floor(), top.min(floor - h).max(screen.pos.y + 8.0).floor(), w, h)
            }
            None => {
                let over = crate::system_chat::view::mac::sheet_room(screen, w);
                rect((over.pos.x + (over.size.x - w) * 0.5).floor(), (top + ((floor - top - h) * 0.25).max(0.0)).floor(), w, h)
            }
        };
        let (x, y) = (page.pos.x, page.pos.y);
        if let Some(win) = &window {
            dim_window(&mut self.d, cx, win.rect, &k);
        }
        draw_card(&mut self.d, cx, page, 12.0, &k);
        self.note_control("panel", page, false);
        self.hits.push((page, Hit::Page));
        self.covered = self.details.is_some();
        let left = x + PANEL_PAD;
        self.app_icons.draw(cx, crate::desktop::ASSISTANT_ICON, place.style, rect(left, y + PANEL_PAD, PANEL_HEAD_ICON, PANEL_HEAD_ICON), 1.0, k.ink);
        let tx = left + PANEL_HEAD_ICON + 14.0;
        let mut ty = y + PANEL_PAD;
        self.d.label(cx, rect(tx, ty, head_w, TEXT_LINE), true, TEXT, k.ink, HAlign::Left, "Approvals");
        ty += TEXT_LINE + 2.0;
        for line in &intro {
            self.d.label(cx, rect(tx, ty, head_w, NOTE_LINE), false, NOTE, k.muted, HAlign::Left, line);
            ty += NOTE_LINE;
        }
        let mut shown = vec!["Approvals".to_string(), PANEL_INTRO.to_string()];

        let area_top = y + PANEL_PAD + head_h + 8.0;
        let area = rect(x + 1.0, area_top, w - 2.0, y + h - PANEL_FOOT - area_top);
        self.scroller.layout(&format!("settings {w}"), area, content);
        self.note_clip = Some(area);
        cx.push_clip_rect(area);
        let mut cy = area.pos.y + 8.0 - self.scroller.offset;
        let mut popup_at: Option<Rect> = None;
        for section in &sections {
            if let Some(title) = section.title {
                self.d.label(cx, rect(left, cy, inner, 22.0), true, TEXT, k.ink, HAlign::Left, title);
                shown.push(title.to_string());
                cy += 24.0;
            }
            let group_h = section.rows.iter().map(|r| r.h).sum::<f64>();
            draw_group(&mut self.d, cx, rect(left, cy, inner, group_h), &k);
            for (i, row) in section.rows.iter().enumerate() {
                if i > 0 {
                    self.d.solid(cx, rect(left + PANEL_ROW_PAD, cy, inner - PANEL_ROW_PAD * 2.0, 1.0), k.line);
                }
                shown.push(row.said.clone());
                let row_right = left + inner - PANEL_ROW_PAD;
                let center = cy + row.h * 0.5;
                let mut tx = left + PANEL_ROW_PAD;
                match &row.kind {
                    PanelKind::Plain { .. } => {}
                    PanelKind::Contacts { on } => {
                        let r = rect(row_right - SWITCH_W, center - SWITCH_H * 0.5, SWITCH_W, SWITCH_H);
                        let (hover, _) = self.state_of(r);
                        switch(&mut self.d, cx, r, &k, *on, hover);
                        self.note_control(if *on { "Use my contacts: on" } else { "Use my contacts: off" }, r, true);
                        if let Some(c) = clipped(r, area) {
                            self.hits.push((c, if *on { Hit::ContactsOff } else { Hit::ContactsOn }));
                        }
                    }
                    PanelKind::Rule { rule, name } => {
                        self.app_icons.draw(cx, &icon_name(&rule.app), place.style, rect(tx, center - PANEL_ICON * 0.5, PANEL_ICON, PANEL_ICON), 1.0, k.ink);
                        tx += PANEL_ICON + 10.0;
                        // Its details (ⓘ) at the trailing edge, as a Mac's list rows have them,
                        // and one button before it.
                        let info = rect(row_right - INFO, center - INFO * 0.5, INFO, INFO);
                        let label = rule_button(rule, f.now);
                        let button_w = button_small_width(&mut self.d, cx, label);
                        let r = rect(info.pos.x - 6.0 - button_w, center - SMALL_H * 0.5, button_w, SMALL_H);
                        let (hover, down) = self.state_of(r);
                        small_button(&mut self.d, cx, r, &k, Face::Plain, hover, down, label);
                        self.note_control(&format!("{label}: {name}"), r, true);
                        if let Some(c) = clipped(r, area) {
                            self.hits.push((c, if rule.enabled { Hit::Delete(rule.id.clone()) } else { Hit::Enable(rule.id.clone()) }));
                        }
                        let (hover, down) = self.state_of(info);
                        info_button(&mut self.d, cx, info, &k, hover, down);
                        self.note_control(&format!("Details: {name}"), info, true);
                        if let Some(c) = clipped(info, area) {
                            self.hits.push((c, Hit::Details(rule.id.clone())));
                        }
                    }
                    PanelKind::TimeBox => {
                        let popup_w = popup_width(&mut self.d, cx, &[TIME_BOX_CHOOSE.to_string()], 12.0, 120.0);
                        let button = rect(row_right - popup_w, center - SMALL_H * 0.5, popup_w, SMALL_H);
                        let (hover, down) = self.state_of(button);
                        pull_down_button(&mut self.d, cx, button, &k, 12.0, hover, down, TIME_BOX_CHOOSE);
                        self.note_control(TIME_BOX_CHOOSE, button, true);
                        if let Some(c) = clipped(button, area) {
                            self.hits.push((c, Hit::TimeBox { button }));
                        }
                        if self.popup.is_some() {
                            popup_at = Some(button);
                        }
                    }
                    PanelKind::Agent { app, state } => {
                        self.app_icons.draw(cx, &icon_name(app), place.style, rect(tx, center - PANEL_ICON * 0.5, PANEL_ICON, PANEL_ICON), 1.0, k.ink);
                        tx += PANEL_ICON + 10.0;
                        let on = *state == State::Allowed;
                        let r = rect(row_right - SWITCH_W, center - SWITCH_H * 0.5, SWITCH_W, SWITCH_H);
                        let (hover, _) = self.state_of(r);
                        switch(&mut self.d, cx, r, &k, on, hover);
                        self.note_control(&format!("{} assistant: {}", app_label(app), if on { "on" } else { "off" }), r, true);
                        if let Some(c) = clipped(r, area) {
                            self.hits.push((c, if on { Hit::AgentOff(app.clone()) } else { Hit::AgentAllow(app.clone()) }));
                        }
                    }
                }
                let block = row.title.len() as f64 * TEXT_LINE + row.note.len() as f64 * NOTE_LINE;
                let mut ty = cy + (row.h - block) * 0.5;
                let dim = matches!(row.kind, PanelKind::Plain { dim: true }) || matches!(&row.kind, PanelKind::Rule { rule, .. } if !rule.enabled);
                for line in &row.title {
                    self.d.label(cx, rect(tx, ty, inner, TEXT_LINE), false, TEXT, if dim { k.muted } else { k.ink }, HAlign::Left, line);
                    ty += TEXT_LINE;
                }
                for line in &row.note {
                    self.d.label(cx, rect(tx, ty, inner, NOTE_LINE), false, NOTE, k.muted, HAlign::Left, line);
                    ty += NOTE_LINE;
                }
                cy += row.h;
            }
            // Under the list, at its trailing edge, as a Mac's Remove All: every rule off at once.
            if section.all_off {
                cy += 8.0;
                let any_on = f.rules.iter().any(|r| r.enabled);
                let off_w = button_width(&mut self.d, cx, ALL_OFF, 72.0);
                let off = rect(left + inner - off_w, cy, off_w, CONTROL_H);
                let (hover, down) = self.state_of(off);
                push_button(&mut self.d, cx, off, &k, if any_on { Face::Plain } else { Face::Disabled }, hover && any_on, down && any_on, ALL_OFF);
                self.note_control(ALL_OFF, off, any_on);
                if any_on {
                    if let Some(c) = clipped(off, area) {
                        self.hits.push((c, Hit::AllOff));
                    }
                }
                cy += CONTROL_H;
            }
            cy += 16.0;
        }
        cx.pop_clip_rect();
        self.note_clip = None;
        self.scroller.draw_thumb(&mut self.d, cx, &k);
        // Hairlines where the list runs under the head or the foot, as a Mac's do.
        if self.scroller.offset > 0.5 {
            self.d.solid(cx, rect(x, area.pos.y - 1.0, w, 1.0), k.line);
        }

        // The foot: Done. What is set here applies at once; Done puts the sheet away.
        let foot = rect(x, y + h - PANEL_FOOT, w, PANEL_FOOT);
        if self.scroller.offset < self.scroller.max - 0.5 {
            self.d.solid(cx, rect(x, foot.pos.y, w, 1.0), k.line);
        }
        let by = foot.pos.y + (PANEL_FOOT - CONTROL_H) * 0.5;
        let done_w = button_width(&mut self.d, cx, "Done", 72.0);
        let done = rect(x + w - PANEL_PAD - done_w, by, done_w, CONTROL_H);
        let (hover, down) = self.state_of(done);
        // Under a rule's details the sheet is not the one taking the keys: its Done is not blue.
        push_button(&mut self.d, cx, done, &k, if self.covered { Face::Plain } else { Face::Default }, hover, down, "Done");
        self.note_control("Done", done, true);
        if !self.covered {
            self.controls.push("\u{27e6}default|Done\u{27e7}".to_string());
        }
        self.hits.push((done, Hit::Close));

        // The time box's menu, over everything: every app, those it runs for checked.
        // Choosing one makes a time box, as "Everything for 60 min" does; choosing a
        // checked one ends it, as its rule's Stop does.
        self.menu_rows.clear();
        if let Some(button) = popup_at {
            let titles: Vec<String> = f.agents.iter().map(|(_, name, _)| name.clone()).collect();
            let running: Vec<Option<RuleId>> = f.agents.iter().map(|(app, _, _)| f.rules.iter().find(|r| r.app == *app && time_box_runs(r, f.now)).map(|r| r.id.clone())).collect();
            let chosen: Vec<usize> = running.iter().enumerate().filter(|(_, rule)| rule.is_some()).map(|(i, _)| i).collect();
            let width = (menu_width(&mut self.d, cx, &titles, 0.0) + MENU_ICON_LEAD).ceil().max(button.size.x);
            let inside = rect(page.pos.x + 4.0, page.pos.y + 4.0, page.size.x - 8.0, page.size.y - 8.0);
            let mut menu = menu_rect(button, width, titles.len(), None, inside, screen);
            // Off the foot as well where there is room, as a sheet's menu stays off its answers.
            let above = rect(inside.pos.x, inside.pos.y, inside.size.x, foot.pos.y - 4.0 - inside.pos.y);
            if overlap(menu, foot) && menu.size.y <= above.size.y {
                menu = menu_rect(button, width, titles.len(), None, above, screen);
            }
            // Drawn over all of the page once it is done ([`Self::draw_popup`]): each
            // app with its icon before its name, as a Mac's app chooser lists them.
            let rows = menu_bands(menu, titles.len());
            let icons = f.agents.iter().map(|(app, _, _)| icon_name(app)).collect();
            self.popup_look = Some(PopupLook { menu, titles: titles.clone(), icons, chosen, k, style: place.style });
            self.hits.push((screen, Hit::TimeBoxDismiss));
            self.hits.push((menu, Hit::Page));
            for (i, row) in rows.iter().enumerate() {
                let hit = match &running[i] {
                    Some(rule) => Hit::Delete(rule.clone()),
                    None => Hit::Everything(f.agents[i].0.clone()),
                };
                self.note_control(&titles[i], *row, true);
                self.hits.push((*row, hit.clone()));
                self.menu_rows.push(hit);
            }
        } else if self.popup.is_some() {
            self.popup = None;
        }
        self.shown = shown;

        // A rule's details, over all of it.
        if let Some(rule) = self.details.as_ref().and_then(|id| f.rules.iter().find(|r| r.id == *id)).cloned() {
            let said = self.mac_details(cx, screen, page, place, f, &k, &rule);
            self.shown.extend(said);
        }
    }

    /// A rule's details: a sheet on the sheet, from its top edge. What the row's
    /// short title leaves out (its conditions, its limits, where it came from),
    /// what can be done to the rule, and Done. Returns its words.
    #[allow(clippy::too_many_arguments)]
    fn mac_details(&mut self, cx: &mut Cx2d, screen: Rect, page: Rect, place: &MacPlace, f: &Frame, k: &MacInk, rule: &Rule) -> Vec<String> {
        // The sheet under it is dimmed and takes no press.
        self.covered = false;
        self.hits.clear();
        self.hits.push((screen, Hit::Page));
        self.d.rounded(cx, page, 6.0, k.dim);
        let w = DETAILS_W.min(page.size.x - 40.0);
        let inner = w - PANEL_PAD * 2.0;
        let title = words::rule_title(rule);
        let state = words::rule_on_said(rule, f.now);
        let text_w = inner - DETAILS_ICON - 12.0;
        let title_lines = wrapped(&mut self.d, cx, true, TEXT, &title, text_w);
        let head_h = (title_lines.len() as f64 * TEXT_LINE + 2.0 + NOTE_LINE).max(DETAILS_ICON);
        let rows = words::rule_details(rule, f.now);
        let mut label_w: f64 = 0.0;
        for (label, _) in &rows {
            label_w = label_w.max(measure(&mut self.d, cx, false, TEXT, label));
        }
        let label_w = (label_w + 1.0).ceil().min(inner * 0.4);
        let value_w = (inner - PANEL_ROW_PAD * 2.0 - label_w - 10.0).max(80.0);
        let laid: Vec<Vec<String>> = rows.iter().map(|(_, value)| wrapped(&mut self.d, cx, false, TEXT, value, value_w)).collect();
        let row_h = |lines: &Vec<String>| lines.len().max(1) as f64 * TEXT_LINE + 10.0;
        let group_h: f64 = laid.iter().map(row_h).sum();
        let note = if rule.include_incoming { Vec::new() } else { wrapped(&mut self.d, cx, false, NOTE, INCOMING_NOTE, inner) };
        let note_h = if note.is_empty() { 0.0 } else { 8.0 + note.len() as f64 * NOTE_LINE };
        let floor = screen.pos.y + screen.size.y - place.dock - 12.0;
        let h = (PANEL_PAD + head_h + 12.0 + group_h + note_h + 16.0 + CONTROL_H + PANEL_PAD).min(floor - page.pos.y);
        let card = rect((page.pos.x + (page.size.x - w) * 0.5).floor(), page.pos.y, w, h);
        draw_card(&mut self.d, cx, card, 12.0, k);
        self.note_control("details card", card, false);
        self.hits.push((card, Hit::Page));
        let x = card.pos.x + PANEL_PAD;
        let mut y = card.pos.y + PANEL_PAD;
        self.app_icons.draw(cx, &icon_name(&rule.app), place.style, rect(x, y, DETAILS_ICON, DETAILS_ICON), 1.0, k.ink);
        let tx = x + DETAILS_ICON + 12.0;
        let mut ty = y;
        for line in &title_lines {
            self.d.label(cx, rect(tx, ty, text_w, TEXT_LINE), true, TEXT, k.ink, HAlign::Left, line);
            ty += TEXT_LINE;
        }
        self.d.label(cx, rect(tx, ty + 2.0, text_w, NOTE_LINE), false, NOTE, k.muted, HAlign::Left, &state);
        let mut shown = vec![title, state];
        y += head_h + 12.0;
        draw_group(&mut self.d, cx, rect(x, y, inner, group_h), k);
        for (i, ((label, value), lines)) in rows.iter().zip(&laid).enumerate() {
            if i > 0 {
                self.d.solid(cx, rect(x + PANEL_ROW_PAD, y, inner - PANEL_ROW_PAD * 2.0, 1.0), k.line);
            }
            self.d.label(cx, rect(x + PANEL_ROW_PAD, y + 5.0, label_w, TEXT_LINE), false, TEXT, k.muted, HAlign::Right, label);
            let mut vy = y + 5.0;
            for line in lines {
                self.d.label(cx, rect(x + PANEL_ROW_PAD + label_w + 10.0, vy, value_w, TEXT_LINE), false, TEXT, k.ink, HAlign::Left, line);
                vy += TEXT_LINE;
            }
            shown.push(format!("{label}: {value}"));
            y += row_h(lines);
        }
        if !note.is_empty() {
            y += 8.0;
            for line in &note {
                self.d.label(cx, rect(x, y, inner, NOTE_LINE), false, NOTE, k.muted, HAlign::Left, line);
                y += NOTE_LINE;
            }
            shown.push(INCOMING_NOTE.to_string());
        }
        // The foot: what can be done to the rule at the leading edge, Done at the trailing.
        let by = card.pos.y + card.size.y - PANEL_PAD - CONTROL_H;
        let done_w = button_width(&mut self.d, cx, "Done", 72.0);
        let done = rect(card.pos.x + w - PANEL_PAD - done_w, by, done_w, CONTROL_H);
        let (hover, down) = self.state_of(done);
        push_button(&mut self.d, cx, done, k, Face::Default, hover, down, "Done");
        self.note_control("Done", done, true);
        self.controls.push("\u{27e6}default|Done\u{27e7}".to_string());
        self.hits.push((done, Hit::DetailsDone));
        // A time box that runs stops, ahead of its time; any other rule goes for good.
        let remove = if time_box_runs(rule, f.now) { "Stop" } else { "Delete Rule" };
        let remove_w = button_width(&mut self.d, cx, remove, 72.0);
        let r = rect(x, by, remove_w, CONTROL_H);
        let (hover, down) = self.state_of(r);
        push_button(&mut self.d, cx, r, k, Face::Plain, hover, down, remove);
        self.note_control(remove, r, true);
        self.hits.push((r, Hit::Delete(rule.id.clone())));
        if !rule.enabled {
            let on_w = button_width(&mut self.d, cx, "Turn On", 72.0);
            let on = rect(r.pos.x + remove_w + 8.0, by, on_w, CONTROL_H);
            let (hover, down) = self.state_of(on);
            push_button(&mut self.d, cx, on, k, Face::Plain, hover, down, "Turn On");
            self.note_control("Turn On", on, true);
            self.hits.push((on, Hit::Enable(rule.id.clone())));
        }
        shown
    }

    fn mac_hit_at(&self, p: Vec2d) -> Option<Hit> {
        self.hits.iter().rev().find(|(r, h)| *h != Hit::Page && contains(*r, p)).or_else(|| self.hits.iter().find(|(r, _)| contains(*r, p))).map(|(_, h)| h.clone())
    }

    fn mac_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        // Under a rule's details nothing moves: the list is dimmed under them.
        let list = self.popup.is_none() && self.details.is_none();
        match event {
            Event::Scroll(e) => {
                if list {
                    self.scroller.by(e.scroll.y);
                    self.redraw(cx);
                }
            }
            Event::MouseMove(e) => {
                self.pointer_at = Some(e.abs);
                self.popup_cursor = None;
                if self.scroller.dragged(e.abs) {
                    self.redraw(cx);
                    return true;
                }
                let thumb = self.details.is_none() && self.scroller.pointer_at(e.abs);
                let hover = self.hits.iter().rev().find(|(r, h)| !matches!(h, Hit::Page | Hit::TimeBoxDismiss) && contains(*r, e.abs)).map(|(r, _)| *r);
                if hover != self.hover || self.popup.is_some() || thumb {
                    self.hover = hover;
                    self.redraw(cx);
                }
            }
            Event::MouseDown(e) => self.mac_down(cx, e.abs),
            Event::MouseUp(e) => self.mac_up(cx, e.abs),
            Event::TouchUpdate(e) => {
                use makepad_widgets::makepad_platform::event::TouchState;
                for touch in &e.touches {
                    match touch.state {
                        TouchState::Start => self.mac_down(cx, touch.abs),
                        TouchState::Stop => self.mac_up(cx, touch.abs),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        true
    }

    fn mac_down(&mut self, cx: &mut Cx, p: Vec2d) {
        if self.popup.is_none() && self.details.is_none() && self.scroller.press(p) {
            return;
        }
        self.pointer_at = Some(p);
        // The menu opens on the press: its release may be the end of a drag to a row.
        if let Some(hit @ Hit::TimeBox { .. }) = self.mac_hit_at(p).filter(|_| self.popup.is_none()) {
            self.mac_press(hit);
            self.popup_press = Some(p);
            self.redraw(cx);
            return;
        }
        self.down = self.mac_hit_at(p);
        self.down_rect = self.hits.iter().rev().find(|(r, h)| *h != Hit::Page && contains(*r, p)).map(|(r, _)| *r);
        self.hover = self.down_rect;
        self.redraw(cx);
    }

    fn mac_up(&mut self, cx: &mut Cx, p: Vec2d) {
        if self.scroller.release() {
            return;
        }
        // The press that opened the menu: a release where it came down leaves it
        // open, and one after a drag chooses the row it ends on, or nothing.
        if let Some(from) = self.popup_press.take() {
            if (p.x - from.x).abs().max((p.y - from.y).abs()) >= 4.0 {
                match self.mac_hit_at(p) {
                    Some(hit @ (Hit::Everything(_) | Hit::Delete(_))) => self.mac_press(hit),
                    _ => self.popup = None,
                }
            }
            self.redraw(cx);
            return;
        }
        let hit = self.mac_hit_at(p);
        let down = self.down.take();
        self.down_rect = None;
        // Only Done closes the sheet: a press outside it does nothing.
        if let Some(hit) = hit.filter(|hit| down.as_ref() == Some(hit)) {
            self.mac_press(hit);
        }
        self.redraw(cx);
    }

    fn mac_press(&mut self, hit: Hit) {
        match hit {
            Hit::TimeBox { button } => {
                self.popup = if self.popup.is_some() { None } else { Some(button) };
                self.popup_cursor = None;
            }
            Hit::TimeBoxDismiss => self.popup = None,
            Hit::Details(id) => {
                self.popup = None;
                self.details = Some(id);
            }
            Hit::DetailsDone => self.details = None,
            hit => {
                self.popup = None;
                // Delete and Stop end the rule the details are about.
                if matches!(hit, Hit::Delete(_)) {
                    self.details = None;
                }
                act(hit);
            }
        }
    }

    /// The keyboard on a Mac: Escape and Return are Done, of a rule's details
    /// when they are up, and the arrows and the page keys scroll. In an open menu
    /// the arrows move along its rows, Return chooses and Escape closes it.
    pub fn key(&mut self, cx: &mut Cx, e: &KeyEvent) -> bool {
        if self.mac.is_none() || !self.open {
            return false;
        }
        if self.details.is_some() {
            match e.key_code {
                KeyCode::Escape | KeyCode::ReturnKey | KeyCode::NumpadEnter => self.details = None,
                _ => return false,
            }
            self.redraw(cx);
            return true;
        }
        if self.popup.is_some() {
            match e.key_code {
                KeyCode::Escape => self.popup = None,
                KeyCode::ArrowDown | KeyCode::ArrowUp => {
                    let by = if e.key_code == KeyCode::ArrowDown { 1 } else { -1 };
                    self.popup_cursor = menu_step(self.popup_cursor, None, self.menu_rows.len(), by);
                    // The pointer may rest on another row: the keys have it now.
                    self.pointer_at = None;
                }
                KeyCode::ReturnKey | KeyCode::NumpadEnter | KeyCode::Space => match self.popup_cursor.and_then(|i| self.menu_rows.get(i).cloned()) {
                    Some(row) => self.mac_press(row),
                    None => self.popup = None,
                },
                _ => {}
            }
            self.redraw(cx);
            return true;
        }
        let page = self.scroller.page();
        match e.key_code {
            KeyCode::Escape | KeyCode::ReturnKey | KeyCode::NumpadEnter => act(Hit::Close),
            KeyCode::ArrowDown => self.scroller.by(TEXT_LINE * 2.0),
            KeyCode::ArrowUp => self.scroller.by(-TEXT_LINE * 2.0),
            KeyCode::PageDown => self.scroller.by(page),
            KeyCode::PageUp => self.scroller.by(-page),
            _ => return false,
        }
        self.redraw(cx);
        true
    }
}

/// A section's height on the sheet: its heading, its list, the button under the
/// list and the room after it.
fn section_h(section: &PanelSection) -> f64 {
    let heading = if section.title.is_some() { 24.0 } else { 0.0 };
    let under = if section.all_off { 8.0 + CONTROL_H } else { 0.0 };
    heading + section.rows.iter().map(|r| r.h).sum::<f64>() + under + 16.0
}

/// The one button at a rule's trailing edge: Stop for a time box that runs (it
/// ends ahead of its time), Turn On for a rule that is off, Delete for any other.
/// Its details offer the rest.
fn rule_button(rule: &Rule, now: u64) -> &'static str {
    if time_box_runs(rule, now) {
        "Stop"
    } else if !rule.enabled {
        "Turn On"
    } else {
        "Delete"
    }
}

/// Room the overlay thumb may cover.
const THUMB_GAP: f64 = 6.0;

fn button_small_width(d: &mut ShellDraw, cx: &mut Cx2d, label: &str) -> f64 {
    (measure(d, cx, false, 12.0, label) + 20.0).ceil().max(56.0)
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
        Hit::Page | Hit::TimeBox { .. } | Hit::TimeBoxDismiss | Hit::Details(_) | Hit::DetailsDone => {}
    });
}

/// A Mac's time box menu, laid out: its card, each app's name and icon, the
/// apps it runs for, the ink and the style the icons take.
struct PopupLook {
    menu: Rect,
    titles: Vec<String>,
    icons: Vec<String>,
    chosen: Vec<usize>,
    k: MacInk,
    style: crate::desktop::DesktopStyle,
}

impl ShellApprovalsSettings {
    /// The time box's menu over all of the page, in a list of its own begun every
    /// frame on a Mac (a closed menu leaves nothing behind), so the icons before
    /// its apps' names draw over its card, not under it with the page's.
    fn draw_popup(&mut self, cx: &mut Cx2d) {
        let look = self.popup_look.take();
        if self.mac.is_none() && self.popup_list.is_none() {
            return;
        }
        let mut list = self.popup_list.take().unwrap_or_else(|| DrawList2d::new(cx));
        list.begin_overlay_reuse(cx);
        if let Some(look) = look {
            self.d.begin_surface(cx);
            let rows = popup_menu_lead(&mut self.d, cx, look.menu, &look.k, &look.titles, &look.chosen, self.pointer_at, self.popup_cursor, MENU_ICON_LEAD);
            for (row, icon) in rows.iter().zip(&look.icons) {
                self.app_icons.draw(cx, icon, look.style, menu_icon_at(look.menu, *row), 1.0, look.k.ink);
            }
            self.d.end_surface(cx);
        }
        list.end(cx);
        self.popup_list = Some(list);
    }
}

impl Widget for ShellApprovalsSettings {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let screen = cx.turtle().rect();
        self.mac = scope.data.get_mut::<WmState>().and_then(|state| mac_place(state, screen));
        self.d.begin_surface(cx);
        self.draw_page(cx, screen);
        self.d.end_surface(cx);
        self.draw_popup(cx);
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    // The shell routes presses. All that is left here is a Mac's overlay thumb going away.
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        if self.scroller.faded(event) {
            self.redraw(cx);
        }
    }

    /// What the control surface reads: the words on screen, and each control.
    fn text(&self) -> String {
        self.shown.iter().chain(self.controls.iter()).cloned().collect::<Vec<_>>().join("\n")
    }
}
