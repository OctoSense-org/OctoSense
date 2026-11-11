//! The assistant panes on a Mac (`DesktopStyle::Macos`): the system chat
//! ("Ask OctoSense", F8) and "Ask <app>" (Shift+F8) as a floating panel, the
//! way a Mac's assistant stands beside what the person is doing: a rounded
//! panel in the shell's material on the right, under the menu bar, about 380
//! points wide, so an app's sidebar stays in view. It is as tall as what it
//! holds and grows with the conversation, up to the room above the Dock,
//! where the conversation scrolls. It moves by its title and sizes by its
//! edges (a size given by an edge holds); a double click on the title puts it
//! back. Other styles and phones keep `draw_pane` exactly.
//!
//! What it adds to the shared pane: a close light, a centred title and a
//! compose tool instead of labelled buttons; empty, disabled and failure
//! states centred in the panel with one sentence and the button that fixes
//! them; 13-point messages in a clipped list with an overlay scroller; a
//! 28-point field that grows with the text, with a real focus ring; plain
//! words for tools, approvals, speakers and notices. A reply's Copy is a
//! borderless button that shows over the reply under the pointer; Shift+
//! Command+C copies the latest reply, and a right click (or a Control-click)
//! on a message opens a Mac menu, drawn over both panels: on a selection
//! Look Up, Search With Google and Copy; elsewhere Copy.

use super::*;
use crate::shell::ui::{inset, slide};
use crate::shell::{rgb, SurfaceTokens};
use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::sync::atomic::Ordering;

/// The panel's width.
pub const W: f64 = 380.0;
/// Its distance from the screen's edge, the menu bar and the Dock.
pub const GAP: f64 = 8.0;
/// The title's height: what moves the panel.
const HEADER: f64 = 44.0;
const SIDE: f64 = 14.0;
const FOOT: f64 = 12.0;
const BODY: f64 = 13.0;
const SMALL: f64 = 12.0;
/// The field's and the buttons' height.
const CONTROL_H: f64 = 28.0;
const FIELD_PAD: f64 = 8.0;
/// A person's message: its bubble's padding, and the most of the width it
/// takes.
const BUBBLE_X: f64 = 10.0;
const BUBBLE_Y: f64 = 6.0;
const BUBBLE_SHARE: f64 = 0.82;
/// A status row's glyph slot.
const STATUS_INDENT: f64 = 18.0;
/// The running row under the conversation.
const WORKING_H: f64 = 26.0;
/// How long the panel slides in, and how far.
const SLIDE_S: f64 = 0.2;
const SLIDE_PX: f64 = 18.0;
/// How long the overlay scroller stays after the last scroll.
const FLASH_S: f64 = 0.9;
/// The spinner's step (twelve ticks a turn).
const SPIN_S: f64 = 1.0 / 12.0;
/// The room above and below an empty or blocked panel's message.
const CENTRED_PAD: f64 = 30.0;
/// How long the panel takes to grow or shrink to what it holds.
const GROW_S: f64 = 0.18;
/// A reply's Copy button: its width (its height is the actions row's).
const COPY_W: f64 = 26.0;
/// A message's menu, as the shell draws its Mac menus (shell/menu/mac.rs):
/// a row's height, the card's padding, the words' size and inset, the least
/// width.
const MENU_ROW: f64 = 22.0;
const MENU_PAD: f64 = 5.0;
const MENU_TEXT: f64 = 13.0;
const MENU_LABEL_X: f64 = MENU_PAD + 9.0;
const MENU_MIN_W: f64 = 150.0;
/// A rule between a menu's groups, as the shell's own menus have it.
const MENU_RULE: f64 = 11.0;
/// A release this soon after the press that opened the menu, with no move
/// between, leaves it open: that was a click.
const MENU_RELEASE_S: f64 = 0.3;

thread_local! {
    /// What the toasts were last told to keep clear of ([`publish`]).
    static WROTE: Cell<Option<Rect>> = const { Cell::new(None) };
    /// What another surface last asked of them: the glance column.
    static OTHER: Cell<Option<Rect>> = const { Cell::new(None) };
    /// The panels up now: the system chat, "Ask <app>", the assistant's card.
    static PANELS: Cell<[Option<Rect>; 3]> = const { Cell::new([None; 3]) };
    /// Which panes were open when the shell last looked ([`crate::App::mac_chat_focus`]).
    static WAS_OPEN: Cell<(bool, bool)> = const { Cell::new((false, false)) };
    /// Something held the keys when a panel took them, so closing it gives
    /// them back ([`note_keys_before`]).
    static KEYS_BEFORE: Cell<bool> = const { Cell::new(false) };
    /// The panels passed the keys between them: who had them first is lost.
    static KEYS_UNKNOWN: Cell<bool> = const { Cell::new(false) };
    /// The screen a panel last stood on.
    static SCREEN: Cell<Option<Rect>> = const { Cell::new(None) };
    /// The panel whose message menu is up ("Ask <app>" or not): the pointer
    /// and the keys are the menu's until it closes, and nothing is typed.
    static MENU_OWNER: Cell<Option<bool>> = const { Cell::new(None) };
    /// The open menu as its panel laid it out this frame: the pane drawn
    /// last draws it, over both panels ([`ShellSystemChat::mac_draw_menu`]).
    static MENU_LOOK: RefCell<Option<MenuLook>> = const { RefCell::new(None) };
}

/// What must stay of the desk for two panels to stand side by side.
const MIN_APP_ROOM: f64 = 300.0;

/// Whether two panels fit and still leave the app usable.
fn room_for_both() -> bool {
    SCREEN.with(|s| s.get()).is_none_or(|s| s.size.x - GAP * 2.0 >= W * 2.0 + GAP + MIN_APP_ROOM)
}

/// What shows of `window`: on the screen, above the Dock.
fn shown_of(window: Rect) -> Rect {
    let mut shown = window;
    // A window can run past a screen made smaller: what shows of it.
    if let Some(screen) = SCREEN.with(|s| s.get()) {
        let x1 = shown.pos.x.max(screen.pos.x);
        let x2 = (shown.pos.x + shown.size.x).min(screen.pos.x + screen.size.x);
        shown = rect(x1, shown.pos.y, (x2 - x1).max(0.0), shown.size.y);
    }
    let dock = crate::desktop::shelf_bounds();
    if dock.size.y > 0.0 && dock.pos.y < shown.pos.y + shown.size.y {
        shown.size.y = (dock.pos.y - shown.pos.y).max(0.0);
    }
    shown
}

/// What is left of `r` with `hole` taken out: the bands above and under the
/// hole, and the parts beside it. None of them overlap.
fn without(r: Rect, hole: Rect) -> Vec<Rect> {
    let (x1, y1, x2, y2) = (r.pos.x, r.pos.y, r.pos.x + r.size.x, r.pos.y + r.size.y);
    let hx1 = hole.pos.x.max(x1);
    let hy1 = hole.pos.y.max(y1);
    let hx2 = (hole.pos.x + hole.size.x).min(x2);
    let hy2 = (hole.pos.y + hole.size.y).min(y2);
    if hx1 >= hx2 || hy1 >= hy2 {
        return vec![r];
    }
    [
        rect(x1, y1, x2 - x1, hy1 - y1),
        rect(x1, hy2, x2 - x1, y2 - hy2),
        rect(x1, hy1, hx1 - x1, hy2 - hy1),
        rect(hx2, hy1, x2 - hx2, hy2 - hy1),
    ]
    .into_iter()
    .filter(|p| p.size.x > 0.0 && p.size.y > 0.0)
    .collect()
}

/// `pieces` less every panel but the one numbered `but`.
fn off_panels(mut pieces: Vec<Rect>, but: Option<usize>) -> Vec<Rect> {
    for (i, panel) in PANELS.with(|p| p.get()).iter().enumerate() {
        if let Some(panel) = panel.filter(|_| Some(i) != but) {
            pieces = pieces.into_iter().flat_map(|piece| without(piece, panel)).collect();
        }
    }
    pieces.retain(|p| p.size.x > 0.0 && p.size.y > 0.0);
    pieces
}

/// What a sheet's dim covers of `window`: all that shows of it above the
/// Dock, up to the edges of the panels floating over it, in pieces that do
/// not overlap.
pub fn dim_areas(window: Rect) -> Vec<Rect> {
    off_panels(vec![shown_of(window)], None)
}

/// Where `window` shows past the rounding of a panel's corners, the corners
/// `round` points: each with its panel. The dim reaches there too.
pub fn dim_corners(window: Rect, round: f64) -> Vec<(Rect, Rect)> {
    let shown = shown_of(window);
    let mut out = Vec::new();
    for (i, panel) in PANELS.with(|p| p.get()).iter().enumerate() {
        let Some(p) = *panel else { continue };
        let r = round.min(p.size.x * 0.5).min(p.size.y * 0.5);
        if r <= 0.0 {
            continue;
        }
        let (left, right) = (p.pos.x, p.pos.x + p.size.x - r);
        let (top, bottom) = (p.pos.y, p.pos.y + p.size.y - r);
        for (x, y) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
            let x1 = x.max(shown.pos.x);
            let y1 = y.max(shown.pos.y);
            let x2 = (x + r).min(shown.pos.x + shown.size.x);
            let y2 = (y + r).min(shown.pos.y + shown.size.y);
            if x1 < x2 && y1 < y2 {
                out.extend(off_panels(vec![rect(x1, y1, x2 - x1, y2 - y1)], Some(i)).into_iter().map(|c| (c, p)));
            }
        }
    }
    out
}

/// Where a sheet `width` wide goes: centred on `window`, as a Mac's sheet is.
/// Sheets draw above the Assistant's panels, so a panel never hides one; and
/// where a panel floats over that place and what shows of the window beside
/// it holds the sheet, the sheet moves aside just enough to stand clear of
/// it, so neither slices the other's words. Else it stays centred.
pub fn sheet_room(window: Rect, width: f64) -> Rect {
    let centred = rect(window.pos.x + (window.size.x - width) * 0.5, window.pos.y, width, window.size.y);
    let (top, bottom) = (window.pos.y, window.pos.y + window.size.y);
    let panels: Vec<Rect> = PANELS.with(|p| p.get()).into_iter().flatten().filter(|p| p.pos.y < bottom && p.pos.y + p.size.y > top).collect();
    let clear = |x: f64| panels.iter().all(|p| x + width <= p.pos.x - GAP || x >= p.pos.x + p.size.x + GAP);
    if clear(centred.pos.x) {
        return centred;
    }
    let shown = shown_of(window);
    let (left, right) = (shown.pos.x, shown.pos.x + shown.size.x);
    panels
        .iter()
        .flat_map(|p| [p.pos.x - GAP - width, p.pos.x + p.size.x + GAP])
        .filter(|&x| x >= left && x + width <= right && clear(x))
        .min_by(|a, b| (a - centred.pos.x).abs().total_cmp(&(b - centred.pos.x).abs()))
        .map_or(centred, |x| rect(x, window.pos.y, width, window.size.y))
}

/// A panel is about to take the keys: note whether anything had them.
pub(super) fn note_keys_before(cx: &Cx) {
    if crate::system_chat::is_open() && crate::app_chat::is_open() {
        KEYS_UNKNOWN.with(|k| k.set(true));
    }
    let known = !KEYS_UNKNOWN.with(|k| k.get());
    KEYS_BEFORE.with(|k| k.set(known && !cx.key_focus().is_empty()));
}

/// The name an app's assistant shows the person: its own, not its kind's
/// ("Card app").
pub fn named_for_person(mut manifest: makepad_ai_services::wire::ServiceManifest, label: &str) -> makepad_ai_services::wire::ServiceManifest {
    if active() {
        manifest.label = label.to_string();
    }
    manifest
}

/// The Mac's look is in force.
pub fn active() -> bool {
    crate::shell::menu::MAC_KEYS.load(Ordering::Relaxed)
}

// ------------------------------------------------------------- geometry

/// Where the panel may stand: under the menu bar, above the Dock when one
/// shows (`shelf`, empty when it is hidden), inside the screen's edge.
pub fn work_area(screen: Rect, shelf: Rect) -> Rect {
    let top = screen.pos.y + GAP;
    let mut bottom = screen.pos.y + screen.size.y - GAP;
    if shelf.size.y > 0.0 && shelf.pos.y > top {
        bottom = bottom.min(shelf.pos.y - GAP);
    }
    rect(screen.pos.x + GAP, top, (screen.size.x - GAP * 2.0).max(0.0), (bottom - top).max(0.0))
}

/// Where the panel opens: the right of the work area, under the menu bar,
/// and the most it may take (`draw_mac` sizes it to what it holds, see
/// [`panel_height`]). It stands left of a surface already there (`clear`: the glance column,
/// the old assistant's pane), and "Ask <app>" left of the system chat
/// (`beside`), so the two conversations show side by side.
pub fn default_column(work: Rect, clear: &[Rect], beside: bool) -> Rect {
    let mut right = work.pos.x + work.size.x;
    for c in clear {
        if c.size.x > 0.0 && c.pos.x < right && c.pos.x + c.size.x > right - W {
            right = c.pos.x - GAP;
        }
    }
    if beside {
        right -= W + GAP;
    }
    let w = W.min(work.size.x);
    rect((right - w).max(work.pos.x), work.pos.y, w, work.size.y)
}

/// The panel's height: what it holds (`fit`), at least a panel's minimum,
/// never more than the `room` under its top.
pub fn panel_height(fit: f64, room: f64) -> f64 {
    fit.max(MIN_H).min(room).max(0.0)
}

/// The frame's grips: corners and edges size the panel, its title moves it.
fn grips_of(pane: Rect) -> Vec<(Rect, Hit)> {
    let (x, y, w, h) = (pane.pos.x, pane.pos.y, pane.size.x, pane.size.y);
    vec![
        (rect(x, y, CORNER, CORNER), Hit::Grip(Grip::NW)),
        (rect(x + w - CORNER, y, CORNER, CORNER), Hit::Grip(Grip::NE)),
        (rect(x, y + h - CORNER, CORNER, CORNER), Hit::Grip(Grip::SW)),
        (rect(x + w - CORNER, y + h - CORNER, CORNER, CORNER), Hit::Grip(Grip::SE)),
        (rect(x, y, w, EDGE), Hit::Grip(Grip::N)),
        (rect(x, y + h - EDGE, w, EDGE), Hit::Grip(Grip::S)),
        (rect(x, y, EDGE, h), Hit::Grip(Grip::W)),
        (rect(x + w - EDGE, y, EDGE, h), Hit::Grip(Grip::E)),
        (rect(x, y, w, HEADER.min(h)), Hit::Grip(Grip::Move)),
    ]
}

/// What a press hits with a mouse: exactly what is under it.
pub(super) fn hit_exact(hits: &[(Rect, Hit)], p: Vec2d) -> Option<Hit> {
    hits.iter()
        .find(|(r, h)| *h != Hit::Pane && contains(*r, p))
        .or_else(|| hits.iter().find(|(r, _)| contains(*r, p)))
        .map(|(_, h)| h.clone())
}

fn clipped(r: Rect, to: Rect) -> Rect {
    let x1 = r.pos.x.max(to.pos.x);
    let y1 = r.pos.y.max(to.pos.y);
    let x2 = (r.pos.x + r.size.x).min(to.pos.x + to.size.x);
    let y2 = (r.pos.y + r.size.y).min(to.pos.y + to.size.y);
    rect(x1, y1, (x2 - x1).max(0.0), (y2 - y1).max(0.0))
}

// ------------------------------------------------------------ plain words

/// Whether a message reads as machine output: JSON, a path or URL, a
/// program's identifier.
pub fn looks_raw(s: &str) -> bool {
    let s = s.trim();
    s.starts_with('{')
        || s.starts_with('[')
        || s.contains("://")
        || s.contains("::")
        || s.contains("{\"")
        || s.contains("0x")
        || s.len() > 240
        || s.split_whitespace().any(|w| {
            let w = w.trim_matches(|c: char| !c.is_alphanumeric());
            w.contains('_') || (w.contains('/') && w.len() > 3) || w.contains("Error(") || (w.len() > 24 && !w.contains('-'))
        })
}

/// The agents' names in plain words.
pub fn plain_words(s: &str) -> String {
    s.replace("The system agent's", "OctoSense's")
        .replace("the system agent's", "OctoSense's")
        .replace("The system agent", "OctoSense")
        .replace("the system agent", "OctoSense")
        .replace("System agent", "OctoSense")
        .replace("'s agent", "")
}

/// Who spoke, for a line above a message: nobody for the person (whose
/// messages are their own bubbles).
pub fn plain_speaker(speaker: &str) -> Option<String> {
    (speaker != "You").then(|| plain_words(speaker))
}

/// A notice as the person reads it: why something failed in one plain
/// sentence, never what a program said.
pub fn plain_notice(text: &str) -> String {
    let text = plain_words(text.trim());
    if let Some(head) = text.strip_prefix("Could not ") {
        let head = head.split(':').next().unwrap_or(head).trim_end_matches('.');
        return format!("Couldn\u{2019}t {head}.");
    }
    if text.starts_with("The assistant could not be offered") {
        return "The assistant couldn\u{2019}t get some of the tools you allowed.".into();
    }
    if let Some(head) = text.strip_prefix("The assistant could not start") {
        let _ = head;
        return "The assistant couldn\u{2019}t answer. Try again.".into();
    }
    if let Some((asks, _)) = text.split_once(": allow or deny it on the sheet") {
        return format!("{asks}.");
    }
    if let Some(rest) = text.strip_suffix(": answer one above to see the next (unanswered ones expire).") {
        return format!("{rest}. Answer one to see the next.");
    }
    if looks_raw(&text) {
        return "Something went wrong. Try again.".into();
    }
    text
}

/// What a tool call does, doing it, done, and when it failed.
fn tool_phrases(name: &str) -> (String, String, String) {
    let three = |doing: &str, done: &str, failed: &str| (doing.to_string(), done.to_string(), failed.to_string());
    match name {
        "web_search" | "toolbox.search" => three("Searching the web", "Searched the web", "Couldn\u{2019}t search the web"),
        "web_fetch" | "toolbox.web_read" => three("Reading a web page", "Read a web page", "Couldn\u{2019}t read a web page"),
        "read_file" => three("Reading a file", "Read a file", "Couldn\u{2019}t read a file"),
        "write_file" => three("Writing a file", "Wrote a file", "Couldn\u{2019}t write a file"),
        "edit_file" | "diff_edit" | "apply_patch" => three("Editing a file", "Edited a file", "Couldn\u{2019}t edit a file"),
        "glob" | "grep" | "list_dir" | "code_structure" | "check_workspace_contract" => three("Looking through files", "Looked through files", "Couldn\u{2019}t look through files"),
        "send_file" => three("Sending a file", "Sent a file", "Couldn\u{2019}t send a file"),
        "ask_user_question" => three("Asking you a question", "Asked you a question", "Couldn\u{2019}t ask you a question"),
        "view_image" => three("Looking at an image", "Looked at an image", "Couldn\u{2019}t open an image"),
        "view_video" => three("Watching a video", "Watched a video", "Couldn\u{2019}t open a video"),
        "recall" | "recall_memory" | "memory_search" | "memory_load" => three("Checking its notes", "Checked its notes", "Couldn\u{2019}t check its notes"),
        "save_memory" | "memory_note" => three("Saving a note", "Saved a note", "Couldn\u{2019}t save a note"),
        "peer_send_input" => three("Asking an app", "Asked an app", "Couldn\u{2019}t reach an app"),
        "peer_gather" | "peer_list" => three("Checking on your apps", "Checked on your apps", "Couldn\u{2019}t check on your apps"),
        "peer_respond" => three("Answering an app", "Answered an app", "Couldn\u{2019}t answer an app"),
        "terminal.run" => three("Running a command", "Ran a command", "Couldn\u{2019}t run a command"),
        _ => match name.split_once('.') {
            Some((app, rest)) if !app.is_empty() => {
                // os.mail.send: the app is what follows the namespace.
                let app = if app == "os" { rest.split('.').next().unwrap_or(rest) } else { app };
                let app = crate::approvals::sheet::app_label(app);
                (format!("Using {app}"), format!("Used {app}"), format!("Couldn\u{2019}t use {app}"))
            }
            _ => three("Using a tool", "Used a tool", "A tool didn\u{2019}t finish"),
        },
    }
}

/// A tool call's readable line.
pub fn tool_line(name: &str, status: &ToolStatus) -> String {
    let (doing, done, failed) = tool_phrases(name);
    match status {
        ToolStatus::Running => format!("{doing}\u{2026}"),
        ToolStatus::Done => done,
        ToolStatus::Failed => failed,
    }
}

/// An approval's readable line: the sheet answers it, the conversation
/// tells where it stands.
pub fn approval_line(tool: &str, title: &str, state: &ApprovalState) -> String {
    let what = if title.trim().is_empty() || looks_raw(title) { tool_phrases(tool).0 } else { plain_words(title.trim()) };
    match state {
        ApprovalState::Waiting => format!("Waiting for your approval: {what}"),
        ApprovalState::Approved => format!("Approved: {what}"),
        ApprovalState::Denied => format!("Denied: {what}"),
        ApprovalState::Cancelled => format!("Withdrawn: {what}"),
        ApprovalState::External => format!("Another app is answering: {what}"),
    }
}

/// A state that stops the person from asking: what it is, why, and the
/// button that fixes it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Blocked {
    pub title: String,
    pub note: String,
    /// Under way: a spinner instead of a glyph.
    pub busy: bool,
    pub action: Option<(String, Hit)>,
    /// What went wrong, for the log only.
    pub why: Option<String>,
}

fn no_model(who: &str) -> Blocked {
    Blocked { title: "No AI Model".into(), note: format!("{who} needs an AI model before it can answer."), busy: false, action: Some(("Open AI Providers\u{2026}".into(), Hit::OpenProviders)), why: None }
}

fn busy(title: String) -> Blocked {
    Blocked { title, note: String::new(), busy: true, action: None, why: None }
}

/// The system chat's blocked state, if it is in one.
pub(super) fn system_blocked(phase: &Phase) -> Option<Blocked> {
    match phase {
        Phase::Ready | Phase::Running { .. } => None,
        Phase::Idle | Phase::Connecting => Some(busy("Connecting\u{2026}".into())),
        Phase::Reconnecting(_) => Some(busy("Reconnecting\u{2026}".into())),
        Phase::NoProvider => Some(no_model("OctoSense")),
        Phase::NoKernel(why) => Some(Blocked {
            title: "The Assistant Couldn\u{2019}t Start".into(),
            note: "OctoSense couldn\u{2019}t start its assistant on this Mac.".into(),
            busy: false,
            action: Some(("Try Again".into(), Hit::Retry)),
            why: Some(why.clone()),
        }),
    }
}

/// "Ask <app>"'s blocked state, if it is in one.
pub(super) fn app_blocked(name: &str, status: &crate::app_chat::Status) -> Option<Blocked> {
    use crate::app_chat::Status;
    match status {
        Status::Ready => None,
        Status::Idle | Status::Connecting => Some(busy(format!("Connecting to {name}\u{2026}"))),
        Status::NeedsConsent => Some(Blocked {
            title: "Permission Needed".into(),
            note: format!("{name} needs your permission to use its assistant."),
            busy: false,
            action: Some(("Allow\u{2026}".into(), Hit::Retry)),
            why: None,
        }),
        Status::Off => Some(Blocked {
            title: "Assistant Turned Off".into(),
            note: format!("You turned off the assistant for {name}."),
            busy: false,
            action: Some(("Open Settings\u{2026}".into(), Hit::OpenSettings)),
            why: None,
        }),
        Status::Failed(why) => {
            let lower = why.to_lowercase();
            if lower.contains("provider") || lower.contains("model") || lower.contains("llm") {
                return Some(no_model(name));
            }
            Some(Blocked {
                title: format!("Couldn\u{2019}t Connect to {name}"),
                note: format!("{name}\u{2019}s assistant isn\u{2019}t answering right now."),
                busy: false,
                action: Some(("Try Again".into(), Hit::Retry)),
                why: Some(why.clone()),
            })
        }
    }
}

// ------------------------------------------------------------------- ink

#[derive(Clone, Copy)]
pub struct Inks {
    pub ink: Vec4f,
    pub dim: Vec4f,
    /// Text on a disabled control.
    pub faint: Vec4f,
    /// A glyph's `dim` and `faint`: opaque, as a stroked glyph's segments
    /// overlap and an alpha would show every joint.
    pub glyph: Vec4f,
    pub glyph_faint: Vec4f,
    pub accent: Vec4f,
    /// A default button's fill: white reads on it at 4.5:1.
    pub default_fill: Vec4f,
    pub on_accent: Vec4f,
    /// Accent text on the panel (a link, "Copied").
    pub link: Vec4f,
    pub red: Vec4f,
    pub hair: Vec4f,
    pub wash: Vec4f,
    pub press: Vec4f,
    pub bubble: Vec4f,
    /// Over the material: it evens out what shows through.
    pub even: Vec4f,
    pub field: Vec4f,
    pub field_edge: Vec4f,
    pub button: Vec4f,
    /// A Mac panel's title and composer bands, and the conversation's ground
    /// between them: the assistant's card's own (its app's `bar` and
    /// `content`), so every assistant panel (Ask AI, Ask OctoSense, Ask
    /// <app>) has one chrome and one body.
    pub bar: Vec4f,
    pub content: Vec4f,
    pub dark: bool,
}

impl Inks {
    pub fn of(tok: &ShellTokens) -> Inks {
        let ink = tok.popups.text;
        let dark = ink.x > 0.5;
        let accent = tok.notifications.countdown;
        Inks {
            ink,
            dim: alpha(ink, if dark { 0.72 } else { 0.68 }),
            faint: alpha(ink, 0.5),
            glyph: if dark { rgb(185, 185, 187) } else { rgb(104, 104, 106) },
            glyph_faint: if dark { rgb(118, 118, 122) } else { rgb(165, 165, 168) },
            accent,
            default_fill: rgb(0, 113, 227),
            on_accent: tok.menu.selected_text,
            link: if dark { rgb(112, 182, 255) } else { rgb(0, 95, 204) },
            red: if dark { rgb(255, 105, 97) } else { rgb(215, 40, 32) },
            hair: alpha(ink, if dark { 0.16 } else { 0.12 }),
            wash: alpha(ink, if dark { 0.1 } else { 0.07 }),
            press: alpha(ink, if dark { 0.18 } else { 0.14 }),
            bubble: alpha(ink, if dark { 0.13 } else { 0.08 }),
            even: if dark { alpha(rgb(38, 38, 40), 0.86) } else { alpha(rgb(250, 250, 251), 0.9) },
            field: if dark { alpha(rgb(255, 255, 255), 0.07) } else { alpha(rgb(255, 255, 255), 0.85) },
            field_edge: alpha(ink, if dark { 0.2 } else { 0.18 }),
            button: if dark { alpha(rgb(255, 255, 255), 0.14) } else { alpha(rgb(255, 255, 255), 0.92) },
            bar: if dark { rgb(48, 48, 51) } else { rgb(237, 237, 240) },
            content: if dark { rgb(30, 30, 30) } else { rgb(255, 255, 255) },
            dark,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Btn {
    Default,
    Push,
}

/// A Mac push button, 28 points tall: the default one blue, a disabled one
/// gray.
#[allow(clippy::too_many_arguments)]
pub fn push_button(d: &mut ShellDraw, cx: &mut Cx2d, r: Rect, label: &str, kind: Btn, enabled: bool, pressed: bool, inks: &Inks) {
    let (fill, edge, text) = match (kind, enabled) {
        (Btn::Default, true) => (if pressed { darker(inks.default_fill, 1.18) } else { inks.default_fill }, None, inks.on_accent),
        (Btn::Push, true) => (if pressed { inks.press } else { inks.button }, Some(inks.field_edge), inks.ink),
        (_, false) => (inks.wash, Some(alpha(inks.ink, 0.08)), inks.faint),
    };
    d.rounded(cx, r, 3.5, fill);
    if let Some(edge) = edge {
        d.focus_ring(cx, r, 3.5, 1.0, edge);
    }
    d.label(cx, r, false, BODY, text, HAlign::Center, label);
}

/// What a status row's glyph says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum StatusGlyph {
    #[default]
    Running,
    Done,
    Failed,
    Waiting,
    Idle,
}

/// The Mac path's own state.
#[derive(Default)]
pub(super) struct MacState {
    /// The last frame drew the Mac pane.
    pub active: bool,
    was_open: bool,
    was_usable: bool,
    /// Take the keyboard once this frame's area stands.
    want_focus: bool,
    opened_at: f64,
    sliding: bool,
    slide_frame: NextFrame,
    /// The overlay scroller shows until then.
    flash_until: f64,
    flash_timer: Timer,
    /// The pointer is over the scroller's strip.
    over_strip: bool,
    /// Where the pointer is, each message's band, and the message under it.
    pointer: Vec2d,
    bands: Vec<(f64, f64, usize)>,
    pointed: Option<usize>,
    strip: Rect,
    knob: Rect,
    knob_drag: Option<(f64, f64)>,
    /// A spinner showed this frame.
    spun: bool,
    spin_step: u32,
    spin_timer: Timer,
    /// The failure last written to the log.
    logged: Option<String>,
    /// The action of the state on screen (for the widget tree).
    action: Option<String>,
    /// The person sized the panel by an edge: it keeps that size.
    sized: bool,
    /// The height it grows or shrinks to what it holds from, to, and since
    /// when; whether it is still on its way.
    h_from: f64,
    h_to: f64,
    h_at: f64,
    growing: bool,
    /// Each reply with a Copy button, and its band on screen; where the
    /// centred state's words stand (for the widget tree).
    replies: Vec<(usize, Rect)>,
    places: Vec<(&'static str, Rect)>,
    /// A message's menu, while it is up; the overlay the pane drawn last
    /// draws menus in.
    menu: Option<ChatMenu>,
    menu_list: Option<DrawList2d>,
}

/// A message's menu while it is up (a right click or a Control-click on
/// the message).
pub(super) struct ChatMenu {
    /// The message, or `None`: the selection.
    target: Option<usize>,
    /// Where the press that opened it was, and when.
    at: Vec2d,
    opened: f64,
    /// The pointer moved, or pressed in it, since: a release chooses.
    armed: bool,
    /// Each row's words and what it does; a rule has neither.
    rows: Vec<(String, Option<Pick>)>,
    /// The selection it opened on: what Look Up and Search With Google take.
    words: String,
    cursor: Option<usize>,
    /// Where it was laid out: the card and each row's band.
    card: Rect,
    bands: Vec<Rect>,
}

/// What a row of a message's menu does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pick {
    LookUp,
    Search,
    Copy,
}

/// What draws an open menu ([`draw_menu`]): each row's words, a rule's empty.
#[derive(Clone)]
struct MenuLook {
    card: Rect,
    rows: Vec<String>,
    bands: Vec<Rect>,
    cursor: Option<usize>,
    dark: bool,
}

/// The composer's field laid out for a draft ([`ShellSystemChat::mac_field`]).
struct FieldPlan {
    label: &'static str,
    hit: Hit,
    enabled: bool,
    bw: f64,
    field_w: f64,
    inner_w: f64,
    px: f64,
    lines: Vec<String>,
    line_h: f64,
    field_h: f64,
}

/// An empty, disabled or failed conversation's message, laid out
/// ([`ShellSystemChat::mac_centred_plan`]).
struct Centred {
    title: String,
    note: String,
    spinner: bool,
    action: Option<(String, Hit)>,
    note_lines: Vec<String>,
    glyph: f64,
    title_h: f64,
    note_h: f64,
    button_w: Option<f64>,
    height: f64,
}

/// The open question's options: its id, labels and count; each button's
/// width; the rows they wrap to; how tall they stand.
type Options = (Option<(String, Vec<String>, usize)>, Vec<f64>, Vec<std::ops::Range<usize>>, f64);

impl ShellSystemChat {
    fn mac_button(&mut self, cx: &mut Cx2d, r: Rect, label: &str, kind: Btn, enabled: bool, inks: &Inks) {
        let pressed = enabled && self.down.is_some() && self.down_rect == Some(r);
        push_button(&mut self.d, cx, r, label, kind, enabled, pressed, inks);
    }

    fn mac_button_w(&mut self, cx: &mut Cx2d, label: &str) -> f64 {
        let ts = self.d.text_scale();
        (self.d.measure(cx, false, BODY * ts, label).ceil() + 24.0).max(56.0)
    }

    fn mac_spinner(&mut self, cx: &mut Cx2d, r: Rect, color: Vec4f) {
        self.mac.spun = true;
        self.spinner.color = color;
        self.spinner.head = (self.mac.spin_step % 12) as f32;
        self.spinner.draw_abs(cx, r);
    }

    /// The panel, the frame's state, and what is on it.
    pub(super) fn draw_mac(&mut self, cx: &mut Cx2d, screen: Rect) {
        self.hits.clear();
        self.shown.clear();
        self.mac.active = true;
        self.mac.spun = false;
        self.mac.action = None;
        self.mac.replies.clear();
        self.mac.places.clear();
        let source = self.source();
        // The pane drawn first starts the frame with no menu laid out; the
        // one whose menu is up lays it out ([`Self::mac_lay_menu`]).
        if source == Source::System {
            MENU_LOOK.with(|m| *m.borrow_mut() = None);
        }
        if !source.is_open() {
            self.mac_close_menu();
            // Every frame: the glance panel, drawn before this, tells the
            // toasts only of itself, and the assistant's card stands at their
            // corner too.
            publish(source.pane() as usize, None);
            self.pane = Rect::default();
            self.select.clear();
            self.mac.was_open = false;
            self.mac.was_usable = false;
            self.mac.knob_drag = None;
            self.mac.sliding = false;
            self.mac.growing = false;
            return;
        }
        let now = cx.seconds_since_app_start();
        let opening = !self.mac.was_open;
        if opening {
            self.mac.was_open = true;
            self.mac.want_focus = true;
            self.mac.opened_at = now;
            self.mac.sliding = true;
            self.mac.slide_frame = cx.new_next_frame();
        }
        let tok = self.d.tokens(self.tokens);
        let inks = Inks::of(&tok);
        let work = work_area(screen, crate::desktop::shelf_bounds());
        SCREEN.with(|s| s.set(Some(screen)));
        self.screen = work;

        let model = source.snapshot();
        let draft = source.draft();
        let usable = source.usable(&model);
        if usable && !self.mac.was_usable {
            self.mac.want_focus = true;
        }
        self.mac.was_usable = usable;
        self.usable = usable;
        let mut blocked = match source {
            Source::System => system_blocked(model.phase()),
            Source::App => app_blocked(&crate::app_chat::app().map(|a| a.name).unwrap_or_default(), &crate::app_chat::status()),
        };
        // The alert is up and asks the same thing: one question, one place to answer it.
        if source == Source::App && crate::app_chat::status() == crate::app_chat::Status::NeedsConsent && crate::approvals::consent_prompt_up() {
            if let Some(b) = blocked.as_mut() {
                b.action = None;
            }
        }
        if let Some(why) = blocked.as_ref().and_then(|b| b.why.clone()) {
            if self.mac.logged.as_ref() != Some(&why) {
                log!("assistant: {why}");
                self.mac.logged = Some(why);
            }
        }

        // Where it stands, and how tall: as tall as what it holds, growing
        // with the conversation up to the room under it. A size the person
        // gave it by an edge holds, until a double click on the title.
        if self.placed.is_none() {
            self.mac.sized = false;
        } else if self.drag.is_some_and(|d| d.grip != Grip::Move) {
            self.mac.sized = true;
        }
        let column = match self.placed {
            Some(placed) => {
                let kept = placed_pane(placed, work);
                self.placed = Some(kept);
                kept
            }
            None => {
                let mut clear: Vec<Rect> = Vec::new();
                clear.extend(other_column());
                if let Some(ai) = crate::shell::ai_pane::mac_card() {
                    clear.push(ai);
                }
                default_column(work, &clear, source == Source::App && super::super::is_open())
            }
        };
        let at = if self.mac.sized {
            self.mac.growing = false;
            column
        } else {
            let room = work.pos.y + work.size.y - column.pos.y;
            let fit = self.mac_fit(cx, column.size.x, source, &model, &draft, usable, blocked.as_ref());
            let h = self.mac_height(cx, panel_height(fit, room), now, opening);
            rect(column.pos.x, column.pos.y, column.size.x, h)
        };
        // What the list gains once the panel has grown to what it holds.
        let settle = if self.mac.sized { 0.0 } else { (self.mac.h_to - at.size.y).max(0.0) };
        let dx = slide((now - self.mac.opened_at) / SLIDE_S, SLIDE_PX);
        self.mac.sliding = dx > 0.0;
        let pane = rect(at.pos.x + dx, at.pos.y, at.size.x, at.size.y);
        self.pane = pane;
        publish(source.pane() as usize, Some(pane));

        // The material, evened out so the text keeps its contrast whatever
        // shows through. First-used shaders draw first: glass, the washes,
        // the rules, the glyphs, the text.
        let m = self.d.material();
        self.d.card(cx, pane, &tok.popups);
        // One chrome and one body for every assistant panel, as the assistant's
        // card (Ask AI) has them: the title and the composer on the bar's grey,
        // the conversation on the content's ground between their rules.
        let inner = inset(pane, m.border_width.max(1.0));
        self.d.rounded(cx, inner, ((m.corner_radius - 1.0) * 0.5).max(0.0), inks.bar);
        let strip = FOOT * 2.0 + self.mac_field(cx, (pane.size.x - SIDE * 2.0).max(1.0), source, &model, &draft, usable).field_h;
        let (ground_top, ground_bottom) = (pane.pos.y + HEADER, pane.pos.y + pane.size.y - strip);
        self.d.rounded(cx, rect(inner.pos.x, ground_top, inner.size.x, (ground_bottom - ground_top).max(0.0)), 0.0, inks.content);
        // The widget tree's row for this pane (its first area).
        self.draw_bg.color = alpha(inks.ink, 0.0);
        self.draw_bg.draw_abs(cx, pane);
        self.hits.push((pane, Hit::Pane));

        let mut hits: Vec<(Rect, Hit)> = Vec::new();
        let mut shown: Vec<String> = Vec::new();

        self.mac_header(cx, pane, source, &model, &inks, &mut hits, &mut shown);
        let x = pane.pos.x + SIDE;
        let cw = (pane.size.x - SIDE * 2.0).max(1.0);
        let top = pane.pos.y + HEADER;
        let mut foot = self.mac_composer(cx, pane, source, &model, &draft, usable, &inks, &mut hits, &mut shown);

        // A rule over the composer's strip.
        self.d.solid(cx, rect(pane.pos.x, foot, pane.size.x, 1.0), inks.hair);

        // "Ask <app>": OctoSense's own task in this conversation, with its own Stop.
        if source == Source::App && crate::app_chat::system_agent_running() {
            let bw = self.mac_button_w(cx, "Stop");
            let row_y = foot - CONTROL_H - 6.0;
            let r = rect(x + cw - bw, row_y, bw, CONTROL_H);
            self.mac_button(cx, r, "Stop", Btn::Push, true, &inks);
            hits.push((r, Hit::StopSystemAgent));
            let note = "OctoSense is working on something here\u{2026}";
            self.d.label_elided(cx, rect(x, row_y, (cw - bw - 8.0).max(0.0), CONTROL_H), false, SMALL, inks.dim, HAlign::Left, note);
            shown.push(format!("{note} [Stop]"));
            foot = row_y - 2.0;
        }

        // A blocked state with a conversation on screen: one line and its
        // button over the composer; the conversation stays readable.
        if let (Some(b), false) = (&blocked, model.items.is_empty()) {
            let row_y = foot - CONTROL_H - 8.0;
            let mut text_w = cw;
            if let Some((label, hit)) = &b.action {
                let bw = self.mac_button_w(cx, label);
                let r = rect(x + cw - bw, row_y, bw, CONTROL_H);
                self.mac_button(cx, r, label, Btn::Push, true, &inks);
                hits.push((r, hit.clone()));
                self.mac.action = Some(label.clone());
                shown.push(format!("button: {label}"));
                text_w = (cw - bw - 8.0).max(0.0);
            }
            let line = if b.note.is_empty() { &b.title } else { &b.note };
            let mut tx = x;
            if b.busy {
                self.mac_spinner(cx, rect(x, row_y + (CONTROL_H - 14.0) * 0.5, 14.0, 14.0), inks.dim);
                tx += 20.0;
                text_w -= 20.0;
            }
            let wrapped = self.d.wrap(cx, false, SMALL, line, text_w.max(1.0), 2);
            let line_h = (SMALL * 1.3).ceil();
            let mut ty = row_y + (CONTROL_H - wrapped.len() as f64 * line_h) * 0.5;
            for l in &wrapped {
                self.d.label_elided(cx, rect(tx, ty, text_w.max(1.0), line_h), false, SMALL, inks.dim, HAlign::Left, l);
                ty += line_h;
            }
            shown.push(line.clone());
            foot = row_y - 4.0;
        }

        let list = rect(pane.pos.x, top, pane.size.x, (foot - top).max(0.0));
        self.select.tracker.clear();
        self.select.region = rect(x, list.pos.y, cw, list.size.y);
        let running = model.phase().running_turn().is_some();

        if model.items.is_empty() {
            self.max_scroll = 0.0;
            self.laid_out = None;
            self.select.clear();
            self.mac_centred(cx, list, source, blocked.as_ref(), running, &inks, &mut hits, &mut shown);
        } else {
            self.mac_list(cx, list, x, cw, settle, source, &model, running, &inks, &mut hits, &mut shown);
        }

        self.hits.extend(hits);
        self.hits.extend(grips_of(pane));
        self.shown = shown;
        self.mac_lay_menu(cx, screen, inks.dark);
    }

    /// The panel's message menu, if one is up: laid out at the press, for
    /// the pane drawn last to draw ([`MENU_LOOK`]).
    fn mac_lay_menu(&mut self, cx: &mut Cx2d, screen: Rect, dark: bool) {
        let Some(menu) = self.mac.menu.as_mut() else { return };
        let mut width = MENU_MIN_W;
        for (label, pick) in &menu.rows {
            if pick.is_some() {
                width = width.max(MENU_LABEL_X + self.d.measure(cx, false, MENU_TEXT, label) + MENU_PAD + 9.0);
            }
        }
        let tall = |pick: &Option<Pick>| if pick.is_some() { MENU_ROW } else { MENU_RULE };
        let height: f64 = menu.rows.iter().map(|(_, pick)| tall(pick)).sum();
        let card = menu_place(menu.at, dvec2(width.ceil(), MENU_PAD * 2.0 + height), screen);
        menu.card = card;
        let mut y = card.pos.y + MENU_PAD;
        menu.bands = menu
            .rows
            .iter()
            .map(|(_, pick)| {
                let band = rect(card.pos.x + MENU_PAD, y, card.size.x - MENU_PAD * 2.0, tall(pick));
                y += band.size.y;
                band
            })
            .collect();
        let rows = menu.rows.iter().map(|(label, pick)| if pick.is_some() { label.clone() } else { String::new() }).collect();
        let look = MenuLook { card, rows, bands: menu.bands.clone(), cursor: menu.cursor, dark };
        MENU_LOOK.with(|m| *m.borrow_mut() = Some(look));
    }

    /// The message menu over both panels: the pane drawn last ("Ask <app>")
    /// draws whichever panel's is up, in an overlay of its own begun every
    /// frame, so a closed menu leaves nothing behind.
    pub(super) fn mac_draw_menu(&mut self, cx: &mut Cx2d) {
        if self.source() != Source::App {
            return;
        }
        let look = MENU_LOOK.with(|m| m.borrow().clone());
        if self.mac.menu_list.is_none() {
            self.mac.menu_list = Some(DrawList2d::new(cx));
        }
        let Some(list) = self.mac.menu_list.as_mut() else { return };
        list.begin_overlay_reuse(cx);
        if let Some(look) = look {
            // Inside an overlay already: the kit binds its material without
            // hoisting again.
            self.d.begin_surface(cx);
            draw_menu(&mut self.d, cx, &look);
            self.d.end_surface(cx);
        }
        list.end(cx);
    }

    /// How tall the panel is for what it holds: its title, the centred
    /// state or the conversation, the rows over the composer, the composer.
    #[allow(clippy::too_many_arguments)]
    fn mac_fit(&mut self, cx: &mut Cx2d, width: f64, source: Source, model: &ChatModel, draft: &str, usable: bool, blocked: Option<&Blocked>) -> f64 {
        let cw = (width - SIDE * 2.0).max(1.0);
        let field_h = self.mac_field(cx, cw, source, model, draft, usable).field_h;
        let mut h = HEADER + FOOT + field_h + FOOT;
        if source == Source::App && crate::app_chat::system_agent_running() {
            h += CONTROL_H + 8.0;
        }
        let running = model.phase().running_turn().is_some();
        if model.items.is_empty() {
            h += self.mac_centred_plan(cx, width, source, blocked, running).height + CENTRED_PAD * 2.0;
        } else {
            if blocked.is_some() {
                h += CONTROL_H + 12.0;
            }
            h += self.mac_list_total(cx, cw, model, running);
        }
        h.ceil()
    }

    /// The panel's height on its way to `target`: a short ease as the
    /// conversation grows or empties; none as it opens, or with reduced
    /// motion.
    fn mac_height(&mut self, cx: &mut Cx2d, target: f64, now: f64, opening: bool) -> f64 {
        let m = &mut self.mac;
        if opening || m.h_to <= 0.0 || crate::shell::ui::reduce_motion() {
            (m.h_from, m.h_to, m.h_at) = (target, target, now);
            m.growing = false;
            return target;
        }
        let at = |m: &MacState| m.h_to - slide((now - m.h_at) / GROW_S, m.h_to - m.h_from);
        if (target - m.h_to).abs() > 0.5 {
            let shown = at(m);
            (m.h_from, m.h_to, m.h_at) = (shown, target, now);
        }
        let h = at(m);
        let growing = (h - m.h_to).abs() > 0.25;
        if growing && !m.growing {
            m.slide_frame = cx.new_next_frame();
        }
        m.growing = growing;
        if growing {
            h
        } else {
            m.h_to
        }
    }

    /// The title bar: the close light, the centred title, the compose tool.
    fn mac_header(&mut self, cx: &mut Cx2d, pane: Rect, source: Source, model: &ChatModel, inks: &Inks, hits: &mut Vec<(Rect, Hit)>, shown: &mut Vec<String>) {
        let (x, y, w) = (pane.pos.x, pane.pos.y, pane.size.x);
        let cy = y + HEADER * 0.5;
        // The lights, gray while the panel is not the one in front.
        let slot = rect(x + 10.0, cy - 12.0, 24.0, 24.0);
        let hovered = self.hover == Some(slot);
        let front = self.held_keyboard(cx) || self.typing(cx);
        let down = self.down == Some(Hit::Close);
        self.d.mac_panel_lights(cx, slot.pos.x + 6.0, slot.pos.y + 6.0, crate::shell::ui::PanelLights { dark: inks.dark, front, over: hovered, down });
        hits.push((slot, Hit::Close));

        // The title, centred over the whole panel.
        let title = match source {
            Source::System => "Ask OctoSense".to_string(),
            Source::App => source.title(),
        };
        self.d.label_elided(cx, rect(x + 84.0, y, (w - 168.0).max(0.0), HEADER), true, BODY, inks.ink, HAlign::Center, &title);
        shown.push(format!("title: {title}"));

        // New Conversation, a glyph as a Mac's toolbar has it.
        if source == Source::System {
            let r = rect(x + w - 10.0 - 28.0, cy - 14.0, 28.0, 28.0);
            let can = !model.items.is_empty() && matches!(model.phase(), Phase::Ready);
            if can {
                if self.down == Some(Hit::New) {
                    self.d.rounded(cx, r, 3.0, inks.press);
                } else if self.hover == Some(r) {
                    self.d.rounded(cx, r, 3.0, inks.wash);
                }
                hits.push((r, Hit::New));
            }
            self.draw_compose.color = if can { inks.ink } else { inks.glyph_faint };
            let g = rect((r.pos.x + 6.0).round(), (r.pos.y + 6.0).round(), 16.0, 16.0);
            self.draw_compose.draw_abs(cx, g);
            shown.push(format!("tool: New Conversation ({})", if can { "enabled" } else { "disabled" }));
        }
        self.d.solid(cx, rect(x, y + HEADER - 1.0, w, 1.0), inks.hair);
    }

    /// The composer: a field that grows with the text and Send. The y of
    /// the strip's top.
    #[allow(clippy::too_many_arguments)]
    fn mac_composer(&mut self, cx: &mut Cx2d, pane: Rect, source: Source, model: &ChatModel, draft: &str, usable: bool, inks: &Inks, hits: &mut Vec<(Rect, Hit)>, shown: &mut Vec<String>) -> f64 {
        let x = pane.pos.x + SIDE;
        let cw = (pane.size.x - SIDE * 2.0).max(1.0);
        let bottom = pane.pos.y + pane.size.y - FOOT;
        let question = model.open_question().is_some();
        let FieldPlan { label, hit, enabled, bw, field_w, inner_w, px, lines, line_h, field_h } = self.mac_field(cx, cw, source, model, draft, usable);
        let composer = source.composer();
        let layout = layout_prompt(&mut self.d, cx, draft, &lines, px);
        let caret_line = layout.iter().rposition(|l| l.start <= composer.cursor()).unwrap_or(0);
        let n = lines.len().clamp(1, PROMPT_LINES);
        let mut first = self.prompt_first.min(lines.len().saturating_sub(n));
        if caret_line < first {
            first = caret_line;
        } else if caret_line >= first + n {
            first = caret_line + 1 - n;
        }
        self.prompt_first = first;
        let field = rect(x, bottom - field_h, field_w, field_h);
        let keyed = self.typing(cx);
        let hot = usable && keyed;
        // The focus ring, outside the field's edge.
        if hot {
            self.d.focus_ring(cx, rect(field.pos.x - 3.0, field.pos.y - 3.0, field.size.x + 6.0, field.size.y + 6.0), 5.0, 3.0, alpha(inks.accent, if inks.dark { 0.6 } else { 0.5 }));
        }
        let fill = if usable { inks.field } else { alpha(inks.ink, if inks.dark { 0.05 } else { 0.04 }) };
        self.d.rounded(cx, field, 3.5, fill);
        self.d.focus_ring(cx, field, 3.5, 1.0, if usable { inks.field_edge } else { alpha(inks.ink, 0.1) });
        let inner_x = field.pos.x + FIELD_PAD;
        let text_top = field.pos.y + (field_h - n as f64 * line_h) * 0.5;
        let sel = composer.selection();
        if !sel.is_empty() {
            for (i, l) in layout.iter().enumerate().skip(first).take(n) {
                let len = l.xs.len() - 1;
                let (a, z) = (sel.start.max(l.start), sel.end.min(l.start + len));
                let past = sel.end > l.start + len && sel.start <= l.start + len && i + 1 < layout.len();
                if a < z || past {
                    let x1 = l.xs[a.saturating_sub(l.start).min(len)];
                    let x2 = if z > a { l.xs[(z - l.start).min(len)] } else { x1 } + if past { 5.0 } else { 0.0 };
                    let y = text_top + (i - first) as f64 * line_h;
                    self.d.solid(cx, rect(inner_x + x1, y, (x2 - x1).max(1.0), line_h), alpha(inks.accent, 0.3));
                }
            }
        }
        if draft.is_empty() {
            let placeholder = if question {
                "Answer the question\u{2026}".to_string()
            } else {
                match source {
                    Source::System => "Ask anything\u{2026}".to_string(),
                    Source::App => format!("Ask {}\u{2026}", crate::app_chat::app().map(|a| a.name).unwrap_or_default()),
                }
            };
            let color = if usable { inks.dim } else { inks.faint };
            self.d.label_elided(cx, rect(inner_x, text_top, inner_w, line_h), false, BODY, color, HAlign::Left, &placeholder);
            shown.push(format!("placeholder: {placeholder}"));
        } else {
            let color = if usable { inks.ink } else { inks.faint };
            for (i, line) in lines.iter().enumerate().skip(first).take(n) {
                self.d.label_elided(cx, rect(inner_x, text_top + (i - first) as f64 * line_h, inner_w, line_h), false, BODY, color, HAlign::Left, line.trim_end());
            }
        }
        let caret_x = layout.get(caret_line).map_or(0.0, |l| l.xs[(composer.cursor() - l.start).min(l.xs.len() - 1)]).min(inner_w);
        let caret = rect(inner_x + caret_x, text_top + (caret_line.saturating_sub(first)) as f64 * line_h + (line_h - px * 1.2) * 0.5, 1.0, px * 1.2);
        if keyed && usable && !self.blink_off && sel.is_empty() {
            self.d.solid(cx, caret, inks.ink);
        }
        self.field = caret;
        PROMPT_LAYOUT.with(|l| l.borrow_mut()[source.pane() as usize] = layout.clone());
        self.prompt = PromptGeom { first, top: text_top, line_h, inner_x, layout };
        if usable {
            hits.push((field, Hit::Field));
        }
        shown.push(format!("prompt: {draft}"));
        shown.push(format!("field: {} {} {} {} {}", if hot { "focused" } else if usable { "ready" } else { "disabled" }, field.pos.x, field.pos.y, field.size.x, field.size.y));

        let button = rect(x + cw - bw, bottom - CONTROL_H, bw, CONTROL_H);
        let kind = if hit == Hit::Stop { Btn::Push } else { Btn::Default };
        self.mac_button(cx, button, label, kind, enabled, inks);
        if enabled {
            hits.push((button, hit));
        }
        shown.push(format!("button: {label} ({})", if enabled { "enabled" } else { "disabled" }));
        field.pos.y - FOOT
    }

    /// An empty, disabled or failed conversation: centred, one sentence and
    /// the button that fixes it.
    #[allow(clippy::too_many_arguments)]
    fn mac_centred(&mut self, cx: &mut Cx2d, list: Rect, source: Source, blocked: Option<&Blocked>, running: bool, inks: &Inks, hits: &mut Vec<(Rect, Hit)>, shown: &mut Vec<String>) {
        let Centred { title, note, spinner, action, note_lines, glyph, title_h, note_h, button_w, height } = self.mac_centred_plan(cx, list.size.x, source, blocked, running);
        let width = (list.size.x - 56.0).max(1.0);
        let x = list.pos.x + 28.0;
        let mut y = list.pos.y + ((list.size.y - height) * 0.5).max(8.0);
        let gx = (list.pos.x + (list.size.x - glyph) * 0.5).round();
        if spinner {
            self.mac_spinner(cx, rect(gx, y, glyph, glyph), inks.dim);
        } else {
            self.d.icon(cx, Ico::Sparkle, rect(gx, y.round(), glyph, glyph), inks.glyph);
        }
        y += glyph + 8.0;
        if spinner {
            self.d.label_elided(cx, rect(x, y, width, title_h), false, BODY, inks.dim, HAlign::Center, &title);
        } else {
            self.d.label_elided(cx, rect(x, y, width, title_h), true, 15.0, inks.ink, HAlign::Center, &title);
        }
        self.mac.places.push(("empty-title", rect(x, y, width, title_h)));
        shown.push(format!("empty title: {title}"));
        y += title_h;
        if !note_lines.is_empty() {
            y += 4.0;
            self.mac.places.push(("empty-note", rect(x, y, width, note_lines.len() as f64 * note_h)));
            for line in &note_lines {
                self.d.label_elided(cx, rect(x, y, width, note_h), false, BODY, inks.dim, HAlign::Center, line);
                y += note_h;
            }
            shown.push(format!("empty note: {note}"));
        }
        if let (Some((label, hit)), Some(bw)) = (action, button_w) {
            y += 14.0;
            let r = rect((list.pos.x + (list.size.x - bw) * 0.5).round(), y, bw, CONTROL_H);
            self.mac_button(cx, r, &label, Btn::Default, true, inks);
            hits.push((r, hit));
            shown.push(format!("button: {label}"));
            self.mac.action = Some(label);
        }
    }

    /// The centred state laid out for a panel `width` wide: its words, its
    /// glyph and button, and how tall they stand.
    fn mac_centred_plan(&mut self, cx: &mut Cx2d, width: f64, source: Source, blocked: Option<&Blocked>, running: bool) -> Centred {
        let name = crate::app_chat::app().map(|a| a.name).unwrap_or_default();
        let (title, note, spinner, action) = match blocked {
            Some(b) => (b.title.clone(), b.note.clone(), b.busy, b.action.clone()),
            None if running => ("Working\u{2026}".to_string(), String::new(), true, None),
            None => match source {
                Source::System => ("How Can I Help?".to_string(), "Ask a question, or have your apps do something.".to_string(), false, None),
                Source::App => ("How Can I Help?".to_string(), format!("Ask a question about what\u{2019}s in {name}."), false, None),
            },
        };
        let text_w = (width - 56.0).max(1.0);
        let glyph = if spinner { 22.0 } else { 36.0 };
        let title_h = if spinner { 18.0 } else { 20.0 };
        let note_lines = if note.is_empty() { Vec::new() } else { self.d.wrap(cx, false, BODY, &note, text_w, 4) };
        let note_h = (BODY * 1.4).ceil();
        let mut height = glyph + 8.0 + title_h;
        if !note_lines.is_empty() {
            height += 4.0 + note_lines.len() as f64 * note_h;
        }
        let button_w = action.as_ref().map(|(label, _)| label.clone()).map(|label| self.mac_button_w(cx, &label));
        if button_w.is_some() {
            height += 14.0 + CONTROL_H;
        }
        Centred { title, note, spinner, action, note_lines, glyph, title_h, note_h, button_w, height }
    }

    /// The composer's field for `draft` in a panel `cw` wide inside its
    /// margins: Send or Stop, the lines it wraps to, and how tall it is.
    fn mac_field(&mut self, cx: &mut Cx2d, cw: f64, source: Source, model: &ChatModel, draft: &str, usable: bool) -> FieldPlan {
        let question = model.open_question().is_some();
        // With a question open, what is typed answers it, turn running or not.
        let running = source.person_running(model) && !(question && !draft.trim().is_empty());
        let (label, hit, enabled) = composer_button(running, usable, draft);
        let bw = self.mac_button_w(cx, label);
        let field_w = (cw - bw - 8.0).max(1.0);
        let inner_w = (field_w - FIELD_PAD * 2.0).max(1.0);
        let px = BODY * self.d.text_scale();
        let lines = self.d.wrap_input(cx, px, draft, inner_w);
        let line_h = (px * 1.45).ceil();
        let n = lines.len().clamp(1, PROMPT_LINES);
        let field_h = CONTROL_H.max(n as f64 * line_h + (CONTROL_H - line_h));
        FieldPlan { label, hit, enabled, bw, field_w, inner_w, px, lines, line_h, field_h }
    }

    /// The conversation as lines (see [`Line`]): laid out once per change.
    fn mac_transcript(&mut self, cx: &mut Cx2d, model: &ChatModel, width: f64) -> Vec<Line> {
        let ts = self.d.text_scale();
        let running = model.phase().running_turn().is_some();
        let last = model.items.len().saturating_sub(1);
        let mut lines: Vec<Line> = Vec::new();
        // The space a bubble's bottom padding needs from what follows it.
        let mut after = 0.0;
        let mut last_was_status = false;
        let wrap = |d: &mut ShellDraw, cx: &mut Cx2d, text: &str, w: f64, px: f64, bold: bool| -> Vec<(String, &'static str)> {
            let mut out = Vec::new();
            for para in text.split('\n') {
                let runs = vec![(para.to_string(), md::Style::default())];
                let wrapped = if para.trim().is_empty() { vec![(Vec::new(), false)] } else { wrap_runs(d, cx, &runs, w, px, bold) };
                for (i, (r, sp)) in wrapped.into_iter().enumerate() {
                    out.push((r.into_iter().map(|(t, _)| t).collect::<String>(), joint(i == 0, sp)));
                }
            }
            out
        };
        for (i, item) in model.items.iter().enumerate() {
            let from = lines.len();
            let status_item = matches!(item, Item::Tool { .. } | Item::Approval { .. });
            let lead = (if from == 0 {
                0.0
            } else if status_item && last_was_status {
                3.0
            } else {
                12.0
            } + std::mem::take(&mut after))
            .max(0.0);
            match item {
                Item::Message { role, text, speaker, .. } => {
                    let person = *role == Role::User && speaker.as_deref().is_none_or(|s| s == "You");
                    if person {
                        let wrapped = wrap(&mut self.d, cx, text, (width * BUBBLE_SHARE - BUBBLE_X * 2.0).max(1.0), BODY * ts, false);
                        let mut widest: f64 = 0.0;
                        for (line, _) in &wrapped {
                            widest = widest.max(self.d.measure(cx, false, BODY * ts, line));
                        }
                        let indent = (width - BUBBLE_X - widest).max(BUBBLE_X);
                        for (n, (line, joint)) in wrapped.into_iter().enumerate() {
                            let gap = if n == 0 { lead + BUBBLE_Y } else { 0.0 };
                            lines.push(Line { kind: LineKind::Person, indent, joint, ..Line::plain(line, false, false, false, false, gap) });
                        }
                        after = BUBBLE_Y;
                    } else {
                        let who = speaker.as_deref().and_then(plain_speaker);
                        let mut gap = lead;
                        if let Some(who) = &who {
                            lines.push(Line::plain(who.clone(), true, true, true, false, gap));
                            gap = 2.0;
                        }
                        if *role == Role::Assistant {
                            markdown_lines(&mut self.d, cx, &mut lines, text, width, BODY, gap);
                            if !(running && i == last) && !text.trim().is_empty() {
                                // The Copy row is the gap after the reply: what comes next stands
                                // right under it, so a reply is spaced as the rest are.
                                lines.push(Line { kind: LineKind::Actions, joint: "", ..Line::plain(String::new(), false, true, true, false, 0.0) });
                                after = -12.0;
                            }
                        } else {
                            for (n, (line, joint)) in wrap(&mut self.d, cx, text, width, BODY * ts, false).into_iter().enumerate() {
                                lines.push(Line { joint, ..Line::plain(line, false, false, false, false, if n == 0 { gap } else { 0.0 }) });
                            }
                        }
                    }
                }
                Item::Tool { name, status, .. } => {
                    let glyph = match status {
                        ToolStatus::Running => StatusGlyph::Running,
                        ToolStatus::Done => StatusGlyph::Done,
                        ToolStatus::Failed => StatusGlyph::Failed,
                    };
                    self.mac_status_lines(cx, &mut lines, &tool_line(name, status), glyph, lead, width);
                }
                Item::Approval { tool, title, state, .. } => {
                    let glyph = match state {
                        ApprovalState::Waiting | ApprovalState::External => StatusGlyph::Waiting,
                        ApprovalState::Approved => StatusGlyph::Done,
                        ApprovalState::Denied | ApprovalState::Cancelled => StatusGlyph::Failed,
                    };
                    self.mac_status_lines(cx, &mut lines, &approval_line(tool, title, state), glyph, lead, width);
                }
                Item::Question { title, body, answered, .. } => {
                    let head = plain_words(title);
                    let text = plain_words(body);
                    // A question with no heading of its own is said once.
                    let text = if text.trim() == head.trim() { String::new() } else { text };
                    let mut gap = lead;
                    if !head.trim().is_empty() {
                        for (n, (line, joint)) in wrap(&mut self.d, cx, &head, width, BODY * ts, true).into_iter().enumerate() {
                            lines.push(Line { joint, ..Line::plain(line, true, false, false, false, if n == 0 { gap } else { 0.0 }) });
                        }
                        gap = 2.0;
                    }
                    if !text.trim().is_empty() {
                        for (n, (line, joint)) in wrap(&mut self.d, cx, &text, width, BODY * ts, false).into_iter().enumerate() {
                            lines.push(Line { joint, ..Line::plain(line, false, false, false, false, if n == 0 { gap } else { 0.0 }) });
                        }
                    }
                    if let Some(a) = answered {
                        lines.push(Line::plain(format!("You answered: {a}"), false, true, true, false, 3.0));
                    }
                }
                Item::Notice(text) => {
                    self.mac_status_lines(cx, &mut lines, &plain_notice(text), StatusGlyph::Idle, lead, width);
                }
            }
            last_was_status = status_item;
            for line in &mut lines[from..] {
                line.item = i;
            }
        }
        lines
    }

    /// A row of small, muted text; with a glyph when it tells a state.
    fn mac_status_lines(&mut self, cx: &mut Cx2d, lines: &mut Vec<Line>, text: &str, glyph: StatusGlyph, gap: f64, width: f64) {
        let ts = self.d.text_scale();
        let indent = if glyph == StatusGlyph::Idle { 0.0 } else { STATUS_INDENT };
        let runs = vec![(text.to_string(), md::Style::default())];
        for (i, (r, sp)) in wrap_runs(&mut self.d, cx, &runs, (width - indent).max(1.0), SMALL * ts, false).into_iter().enumerate() {
            let line: String = r.into_iter().map(|(t, _)| t).collect();
            let kind = if i == 0 && glyph != StatusGlyph::Idle { LineKind::Status(glyph) } else { LineKind::Text };
            lines.push(Line { kind, indent, joint: joint(i == 0, sp), ..Line::plain(line, false, true, true, false, if i == 0 { gap } else { 0.0 }) });
        }
    }

    /// The transcript laid out `cw` wide, kept until it changes.
    fn mac_laid(&mut self, cx: &mut Cx2d, model: &ChatModel, running: bool, cw: f64) -> Rc<Laid> {
        let key = TranscriptKey { items: model.items.clone(), running, width: cw, body: BODY, small: SMALL, scale: self.d.text_scale() };
        match &self.laid_out {
            Some((k, laid)) if *k == key => laid.clone(),
            _ => {
                let laid = Rc::new(joined(self.mac_transcript(cx, model, cw)));
                self.laid_out = Some((key, laid.clone()));
                laid
            }
        }
    }

    /// The open question's options, as buttons wrapped to rows `cw` wide.
    fn mac_options(&mut self, cx: &mut Cx2d, model: &ChatModel, cw: f64) -> Options {
        let open = model.items.iter().rev().find_map(|i| match i {
            Item::Question { id, options, count, answered: None, .. } if !options.is_empty() => Some((id.clone(), options.clone(), *count)),
            _ => None,
        });
        let mut option_widths = Vec::new();
        if let Some((_, options, _)) = &open {
            for label in options {
                option_widths.push(self.mac_button_w(cx, label).min(cw));
            }
        }
        let option_rows = wrap_widths(&option_widths, cw, 8.0);
        let options_h = if open.is_some() { 8.0 + option_rows.len() as f64 * (CONTROL_H + 6.0) } else { 0.0 };
        (open, option_widths, option_rows, options_h)
    }

    /// How tall the conversation stands: its lines, the running row, the
    /// open question's options and the margins.
    fn mac_list_total(&mut self, cx: &mut Cx2d, cw: f64, model: &ChatModel, running: bool) -> f64 {
        let laid = self.mac_laid(cx, model, running, cw);
        let lines: f64 = laid.lines.iter().map(|l| l.height(BODY, SMALL)).sum();
        let working = running && model.open_question().is_none();
        let options_h = self.mac_options(cx, model, cw).3;
        lines + if working { WORKING_H } else { 0.0 } + options_h + 20.0
    }

    /// The conversation: clipped to the list, scrolled by the pixel, with
    /// an overlay scroller. `settle`: what the list gains once the panel
    /// has grown to what it holds; until then it shows its top, as it will.
    #[allow(clippy::too_many_arguments)]
    fn mac_list(&mut self, cx: &mut Cx2d, list: Rect, x: f64, cw: f64, settle: f64, source: Source, model: &ChatModel, running: bool, inks: &Inks, hits: &mut Vec<(Rect, Hit)>, shown: &mut Vec<String>) {
        let laid = self.mac_laid(cx, model, running, cw);
        let measure = (cw, BODY, SMALL, self.d.text_scale());
        if self.select.measure != measure || model.items.len() < self.select.items {
            self.select.clear();
        }
        self.select.measure = measure;
        self.select.items = model.items.len();

        let lines = &laid.lines;
        let heights: Vec<f64> = lines.iter().map(|l| l.height(BODY, SMALL)).collect();
        // A question waits for the person: nothing is working.
        let working = running && model.open_question().is_none();
        // The open question's options stand right under it, wrapped to rows.
        let (open, option_widths, option_rows, options_h) = self.mac_options(cx, model, cw);
        let total: f64 = heights.iter().sum::<f64>() + if working { WORKING_H } else { 0.0 } + options_h + 20.0;
        let room = list.size.y;
        // It fits, or will once the panel has grown: from its top.
        let fits = total <= room + settle + 0.5;
        self.max_scroll = if fits { 0.0 } else { (total - room).max(0.0) };
        let scroll = source.scroll().min(self.max_scroll);
        let start = if fits { list.pos.y + 10.0 } else { list.pos.y + room - total + scroll + 10.0 };
        let list_bottom = list.pos.y + room;
        cx.push_clip_rect(list);

        // Where each line stands; bubbles and code blocks under whole runs.
        let mut tops = Vec::with_capacity(lines.len());
        let mut ly = start;
        for (line, h) in lines.iter().zip(&heights) {
            tops.push(ly + line.gap_before);
            ly += h;
        }
        let end = ly;
        let mut i = 0;
        while i < lines.len() {
            let kind = lines[i].kind;
            if matches!(kind, LineKind::Person | LineKind::Code) {
                let mut z = i;
                while z + 1 < lines.len() && lines[z + 1].kind == kind && lines[z + 1].item == lines[i].item && lines[z + 1].gap_before == 0.0 {
                    z += 1;
                }
                let lh = heights[z] - lines[z].gap_before;
                let (y1, y2) = (tops[i], tops[z] + lh);
                if y2 >= list.pos.y && y1 <= list_bottom {
                    if kind == LineKind::Person {
                        let bx = x + lines[i].indent - BUBBLE_X;
                        self.d.rounded(cx, rect(bx, y1 - BUBBLE_Y, x + cw - bx, y2 - y1 + BUBBLE_Y * 2.0), 5.0, inks.bubble);
                    } else {
                        self.d.rounded(cx, rect(x, y1 - 2.0, cw, y2 - y1 + 4.0), 3.0, inks.wash);
                    }
                }
                i = z + 1;
            } else {
                i += 1;
            }
        }

        // Each message's band: a reply's Copy shows while the pointer is
        // over it.
        self.mac.bands.clear();
        for (i, (line, h)) in lines.iter().zip(&heights).enumerate() {
            self.mac.bands.push((tops[i] - line.gap_before, tops[i] - line.gap_before + h, line.item));
        }
        for (i, line) in lines.iter().enumerate() {
            if line.kind == LineKind::Actions {
                let first = lines.iter().position(|l| l.item == line.item).unwrap_or(i);
                let band = clipped(rect(x, tops[first], cw, tops[i] + heights[i] - line.gap_before - tops[first]), list);
                if band.size.y > 0.0 {
                    self.mac.replies.push((line.item, band));
                }
            }
        }
        self.mac.pointed = band_at(&self.mac.bands, list, self.mac.pointer);
        let hover = self.hover;
        let ts = self.d.text_scale();
        for (i, (line, h)) in lines.iter().zip(&heights).enumerate() {
            let line_top = tops[i];
            let lh = h - line.gap_before;
            if line_top + lh < list.pos.y || line_top > list_bottom {
                continue;
            }
            let px = if line.small { SMALL } else { BODY } * line.scale;
            let color = if line.accent { inks.link } else if line.dim { inks.dim } else { inks.ink };
            match line.kind {
                LineKind::Rule => {
                    self.d.solid(cx, rect(x, line_top + lh * 0.5, cw, 1.0), inks.hair);
                    continue;
                }
                LineKind::Actions => {
                    // A borderless button, as a Mac's toolbars have them: over
                    // the reply under the pointer, and while it says it copied.
                    let done = self.copied == Some(line.item);
                    if !done && self.mac.pointed != Some(line.item) {
                        continue;
                    }
                    let r = rect(x - 5.0, line_top, COPY_W, lh);
                    let seen = clipped(r, list);
                    if hover == Some(seen) {
                        let fill = if self.down == Some(Hit::Copy(line.item)) { inks.press } else { inks.wash };
                        self.d.rounded(cx, r, 3.0, fill);
                    }
                    let (ico, tint) = if done { (Ico::Check, inks.link) } else { (Ico::Copy, inks.glyph) };
                    self.d.icon(cx, ico, rect(r.pos.x + (COPY_W - 14.0) * 0.5, line_top + (lh - 14.0) * 0.5, 14.0, 14.0), tint);
                    hits.push((seen, Hit::Copy(line.item)));
                    continue;
                }
                LineKind::Quote => self.d.solid(cx, rect(x + 3.0, line_top, 2.0, lh), alpha(inks.ink, 0.35)),
                LineKind::Status(glyph) => {
                    let g = rect(x, line_top + (lh - 13.0) * 0.5, 13.0, 13.0);
                    match glyph {
                        StatusGlyph::Running => self.mac_spinner(cx, g, inks.dim),
                        StatusGlyph::Done => self.d.icon(cx, Ico::Check, g, inks.glyph),
                        StatusGlyph::Failed => self.d.icon(cx, Ico::Close, g, inks.red),
                        StatusGlyph::Waiting => self.d.icon(cx, Ico::Lock, g, inks.glyph),
                        StatusGlyph::Idle => {}
                    }
                }
                LineKind::Text | LineKind::Code | LineKind::Person => {}
            }
            let lx = x + line.indent;
            let start = laid.starts[i];
            if line.runs.is_empty() {
                let r = rect(lx, line_top, (cw - line.indent).max(0.0) + 2.0, lh);
                let trace = if line.kind == LineKind::Code {
                    self.d.with_code(|d| d.label_traced(cx, r, false, px, color, HAlign::Left, &line.text))
                } else {
                    self.d.label_traced(cx, r, line.bold, px, color, HAlign::Left, &line.text)
                };
                self.select.push(trace, start);
                continue;
            }
            let mut rx = lx;
            let mut at = start;
            for (text, style) in &line.runs {
                let bold = line.bold || style.bold;
                let r = rect(rx, line_top, (x + cw - rx).max(0.0) + 2.0, lh);
                let run_color = if style.link { inks.link } else { color };
                let (w, trace) = if style.code {
                    // The chip has its room beside the words around it, not over their spaces.
                    let pad = if text.trim().is_empty() { 0.0 } else { CHIP_PAD };
                    let chip_x = rx + pad;
                    let w = self.d.with_code(|d| d.measure(cx, false, px * ts, text));
                    // A wrapped run keeps its spaces: they count for the line, not for the chip.
                    let lead = text.len() - text.trim_start().len();
                    let lead_w = self.d.with_code(|d| d.measure(cx, false, px * ts, &text[..lead]));
                    let ink_w = self.d.with_code(|d| d.measure(cx, false, px * ts, text.trim()));
                    self.d.rounded(cx, rect(chip_x + lead_w - CHIP_PAD, line_top + lh * 0.1, ink_w + CHIP_PAD * 2.0, lh * 0.8), 1.5, inks.wash);
                    let r = rect(chip_x, line_top, (x + cw - chip_x).max(0.0) + 2.0, lh);
                    (w + pad * 2.0, self.d.with_code(|d| d.label_traced(cx, r, false, px, run_color, HAlign::Left, text)))
                } else {
                    let w = self.d.measure(cx, bold, px * ts, text);
                    (w, self.d.label_traced(cx, r, bold, px, run_color, HAlign::Left, text))
                };
                self.select.push(trace, at);
                at += text.len();
                rx += w;
            }
        }

        // The selection goes under the text (its shader was used first).
        let (a, z) = self.select.range();
        if a < z {
            for r in self.select.tracker.selection_rects(a, z) {
                self.d.solid(cx, r, alpha(inks.accent, 0.3));
            }
        }

        // The turn under way.
        if working {
            let y = end + 4.0;
            self.mac_spinner(cx, rect(x, y + (WORKING_H - 4.0 - 14.0) * 0.5, 14.0, 14.0), inks.dim);
            self.d.label(cx, rect(x + 20.0, y, cw - 20.0, WORKING_H - 4.0), false, SMALL, inks.dim, HAlign::Left, "Working\u{2026}");
            shown.push("Working\u{2026}".into());
        }
        if let Some((id, options, count)) = &open {
            let mut ry = end + 8.0;
            for row in &option_rows {
                let mut ox = x;
                for i in row.clone() {
                    let r = rect(ox, ry, option_widths[i], CONTROL_H);
                    self.mac_button(cx, r, &options[i], Btn::Push, true, inks);
                    hits.push((clipped(r, list), Hit::Option { question: id.clone(), count: *count, label: options[i].clone() }));
                    shown.push(format!("option: {}", options[i]));
                    ox += option_widths[i] + 8.0;
                }
                ry += CONTROL_H + 6.0;
            }
        }
        cx.pop_clip_rect();

        // The overlay scroller: while scrolling, and under the pointer.
        let now = cx.seconds_since_app_start();
        self.mac.strip = rect(list.pos.x + list.size.x - 14.0, list.pos.y, 14.0, room);
        self.mac.knob = Rect::default();
        if self.max_scroll > 0.5 && room > 40.0 {
            let knob_h = (room * room / (room + self.max_scroll)).max(28.0);
            let share = 1.0 - scroll / self.max_scroll;
            let knob = rect(list.pos.x + list.size.x - 10.0, list.pos.y + 3.0 + (room - 6.0 - knob_h) * share, 7.0, knob_h);
            self.mac.knob = knob;
            if now < self.mac.flash_until || self.mac.over_strip || self.mac.knob_drag.is_some() {
                self.d.rounded(cx, knob, 1.75, alpha(inks.ink, if self.mac.over_strip || self.mac.knob_drag.is_some() { 0.5 } else { 0.38 }));
                hits.push((rect(knob.pos.x - 3.0, knob.pos.y, knob.size.x + 6.0, knob.size.y), Hit::Knob));
            }
        }

        shown.extend(lines.iter().filter(|l| l.kind != LineKind::Actions).map(|l| l.text.clone()));
        let _ = source;
    }

    /// After the frame's areas stand: the keyboard the panel wanted, and
    /// the timers its motion needs.
    pub(super) fn mac_after_draw(&mut self, cx: &mut Cx2d) {
        if self.mac.spun && self.mac.spin_timer.is_empty() && !crate::shell::ui::reduce_motion() {
            self.mac.spin_timer = cx.start_timeout(SPIN_S);
        }
        if std::mem::take(&mut self.mac.want_focus) && self.source().is_open() {
            crate::app_chat::focus(self.source() == Source::App);
            self.take_keyboard(cx);
        }
    }

    /// The panel's own timers and frames. True when it should redraw.
    pub(super) fn mac_event(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let mut redraw = false;
        if self.mac.slide_frame.is_event(event).is_some() {
            redraw = true;
            if self.mac.sliding || self.mac.growing {
                self.mac.slide_frame = cx.new_next_frame();
            }
        }
        if self.mac.spin_timer.is_event(event).is_some() {
            self.mac.spin_timer = Timer::empty();
            if self.mac.spun {
                self.mac.spin_step = self.mac.spin_step.wrapping_add(1);
                redraw = true;
            }
        }
        if self.mac.flash_timer.is_event(event).is_some() {
            self.mac.flash_timer = Timer::empty();
            redraw = true;
        }
        redraw
    }

    /// The pointer on a Mac, before the shared pane's handling: the
    /// scroller, the keyboard a press takes, the pressed look. `Some` when
    /// the event ends here.
    pub(super) fn mac_pointer(&mut self, cx: &mut Cx, source: Source, event: &Event) -> Option<Outcome> {
        // A message's menu is up: the pointer is its own wherever it goes,
        // until it closes; the other panel leaves it be.
        if let Some(owner) = MENU_OWNER.with(|m| m.get()) {
            if owner != (source == Source::App) {
                return Some(Outcome::Ignored);
            }
            if self.mac.menu.is_some() {
                return Some(self.mac_menu_pointer(cx, event));
            }
            MENU_OWNER.with(|m| m.set(None));
        }
        if let Some((start_y, start_scroll)) = self.mac.knob_drag {
            match event {
                Event::MouseMove(e) => {
                    let room = self.select.region.size.y;
                    let knob_h = self.mac.knob.size.y;
                    let travel = (room - 6.0 - knob_h).max(1.0);
                    let want = (start_scroll - (e.abs.y - start_y) / travel * self.max_scroll).clamp(0.0, self.max_scroll);
                    source.scroll_by(want - source.scroll(), self.max_scroll);
                    self.redraw(cx);
                    return Some(Outcome::Taken);
                }
                Event::MouseUp(_) => {
                    self.mac.knob_drag = None;
                    self.redraw(cx);
                    return Some(Outcome::Taken);
                }
                _ => {}
            }
        }
        match event {
            Event::Scroll(e) if contains(self.pane, e.abs) => {
                self.mac_flash(cx);
            }
            Event::MouseMove(e) => {
                let over = contains(self.mac.strip, e.abs) && self.mac.knob.size.y > 0.0;
                self.mac.pointer = e.abs;
                let pointed = band_at(&self.mac.bands, self.select.region, e.abs);
                if over != self.mac.over_strip || pointed != self.mac.pointed {
                    self.mac.over_strip = over;
                    self.mac.pointed = pointed;
                    self.redraw(cx);
                }
            }
            Event::MouseDown(e) if contains(self.pane, e.abs) => {
                // A press anywhere in the panel brings it in front.
                crate::app_chat::focus(source == Source::App);
                if !self.has_keyboard(cx) {
                    self.take_keyboard(cx);
                }
                // A right click (or a Control-click): a message's menu.
                if e.button.contains(MouseButton::SECONDARY) || (e.button.contains(MouseButton::PRIMARY) && e.modifiers.control) {
                    self.mac_context_click(cx, source, e.abs);
                    self.redraw(cx);
                    return Some(Outcome::Taken);
                }
                if contains(self.mac.knob, e.abs) {
                    self.mac.knob_drag = Some((e.abs.y, source.scroll()));
                    return Some(Outcome::Taken);
                }
                self.redraw(cx);
            }
            Event::MouseUp(_) => self.redraw(cx),
            _ => {}
        }
        None
    }

    /// A right click on a message: its menu at the pointer. On the selection
    /// a Mac's text menu: Look Up “…” and Search With Google for the words,
    /// a rule, then Copy, which takes the selection; elsewhere Copy, which
    /// takes the message, and the selection goes.
    fn mac_context_click(&mut self, cx: &mut Cx, source: Source, p: Vec2d) {
        let Some(item) = band_at(&self.mac.bands, self.select.region, p) else { return };
        let message = matches!(source.snapshot().items.get(item), Some(Item::Message { text, .. }) if !text.trim().is_empty());
        if !message {
            return;
        }
        let (a, z) = self.select.range();
        let on_selection = a < z && self.select.tracker.point_to_index(cx, p).is_some_and(|i| i >= a && i < z);
        if !on_selection {
            self.select.clear();
        }
        let words = if on_selection { self.selected_text().unwrap_or_default() } else { String::new() };
        let quoted = menu_quote(&words);
        let mut rows = Vec::new();
        if !quoted.is_empty() {
            rows.push((format!("Look Up \u{201c}{quoted}\u{201d}"), Some(Pick::LookUp)));
            rows.push(("Search With Google".to_string(), Some(Pick::Search)));
            rows.push((String::new(), None));
        }
        rows.push(("Copy".to_string(), Some(Pick::Copy)));
        self.mac.menu = Some(ChatMenu {
            target: (!on_selection).then_some(item),
            at: p,
            opened: cx.seconds_since_app_start(),
            armed: false,
            rows,
            words,
            cursor: None,
            card: Rect::default(),
            bands: Vec::new(),
        });
        MENU_OWNER.with(|m| m.set(Some(source == Source::App)));
    }

    /// The pointer while this panel's message menu is up: the row under it
    /// lights; a release on a row chooses it ([`release_chooses`]); a press
    /// off the menu closes it and goes no further, as on a Mac.
    fn mac_menu_pointer(&mut self, cx: &mut Cx, event: &Event) -> Outcome {
        let now = cx.seconds_since_app_start();
        let Some(menu) = self.mac.menu.as_mut() else { return Outcome::Ignored };
        // A rule is no row to light or choose.
        let live: Vec<bool> = menu.rows.iter().map(|(_, pick)| pick.is_some()).collect();
        let row_at = |bands: &[Rect], p: Vec2d| bands.iter().zip(&live).position(|(b, live)| *live && contains(*b, p));
        let (mut chosen, mut close, mut changed) = (None, false, false);
        match event {
            Event::MouseMove(e) => {
                if (e.abs - menu.at).length() > 4.0 {
                    menu.armed = true;
                }
                let cursor = row_at(&menu.bands, e.abs);
                changed = cursor != menu.cursor;
                menu.cursor = cursor;
            }
            Event::MouseDown(e) => {
                if contains(menu.card, e.abs) {
                    menu.armed = true;
                } else {
                    close = true;
                }
            }
            Event::MouseUp(e) => {
                if release_chooses(menu.armed, now - menu.opened) {
                    chosen = row_at(&menu.bands, e.abs);
                }
            }
            _ => {}
        }
        if let Some(row) = chosen {
            self.mac_menu_choose(cx, row);
        } else if close {
            self.mac_close_menu();
            self.redraw(cx);
        } else if changed {
            self.redraw(cx);
        }
        Outcome::Taken
    }

    /// A key while this panel's message menu is up: every key is the
    /// menu's. Up and Down light a row, Return or Space chooses it (or
    /// closes the menu when none is lit), a letter lights the row it
    /// starts, Escape closes the menu.
    pub(super) fn mac_menu_key(&mut self, cx: &mut Cx, e: &KeyEvent) {
        let Some(menu) = self.mac.menu.as_mut() else { return };
        // The rows the keys stop on: a rule is passed over.
        let live: Vec<usize> = menu.rows.iter().enumerate().filter(|(_, (_, pick))| pick.is_some()).map(|(i, _)| i).collect();
        let (mut chosen, mut close) = (None, false);
        match e.key_code {
            KeyCode::Escape => close = true,
            KeyCode::ArrowDown => {
                menu.cursor = match menu.cursor {
                    None => live.first().copied(),
                    Some(c) => live.iter().copied().find(|&i| i > c).or(Some(c)),
                }
            }
            KeyCode::ArrowUp => {
                menu.cursor = match menu.cursor {
                    None => live.last().copied(),
                    Some(c) => live.iter().rev().copied().find(|&i| i < c).or(Some(c)),
                }
            }
            KeyCode::ReturnKey | KeyCode::NumpadEnter | KeyCode::Space => match menu.cursor {
                Some(row) => chosen = Some(row),
                None => close = true,
            },
            code => {
                let name = format!("{code:?}");
                if let Some(letter) = name.strip_prefix("Key").filter(|l| l.len() == 1) {
                    let letter = letter.to_ascii_lowercase();
                    if let Some(row) = menu.rows.iter().position(|(r, pick)| pick.is_some() && r.to_ascii_lowercase().starts_with(&letter)) {
                        menu.cursor = Some(row);
                    }
                }
            }
        }
        if let Some(row) = chosen {
            self.mac_menu_choose(cx, row);
        } else if close {
            self.mac_close_menu();
        }
        self.redraw(cx);
    }

    /// A row of the message menu chosen: Copy takes the message, or the
    /// selection the menu opened on; Look Up and Search With Google open its
    /// words as a text field's menu does ([`open_words`]). The keys stay in
    /// the panel.
    fn mac_menu_choose(&mut self, cx: &mut Cx, row: usize) {
        let Some(menu) = self.mac.menu.take() else { return };
        MENU_OWNER.with(|m| m.set(None));
        let words = percent_encode(menu.words.trim());
        match menu.rows.get(row).and_then(|(_, pick)| *pick) {
            Some(Pick::Copy) => match menu.target {
                Some(item) => self.copy_item(cx, item),
                None => self.mac_copy_selection(cx),
            },
            Some(Pick::LookUp) => open_words(cx, &format!("dict://{words}")),
            Some(Pick::Search) => open_words(cx, &format!("https://www.google.com/search?q={words}")),
            None => {}
        }
        self.redraw(cx);
    }

    /// The message menu goes, with nothing chosen.
    fn mac_close_menu(&mut self) {
        if self.mac.menu.take().is_some() {
            MENU_OWNER.with(|m| m.set(None));
        }
    }

    /// Shift+Command+C: the latest finished reply, as its Copy would.
    fn mac_copy_last_reply(&mut self, cx: &mut Cx) {
        let model = self.source().snapshot();
        let running = model.phase().running_turn().is_some();
        let last = model.items.len().saturating_sub(1);
        let reply = model.items.iter().enumerate().rev().find_map(|(i, item)| match item {
            Item::Message { role: Role::Assistant, text, .. } if !text.trim().is_empty() && !(running && i == last) => Some(i),
            _ => None,
        });
        if let Some(i) = reply {
            self.copy_item(cx, i);
        }
    }

    /// Copy from a message's menu, on the selection.
    fn mac_copy_selection(&mut self, cx: &mut Cx) {
        if let Some(text) = self.selected_text() {
            cx.copy_to_clipboard(&text);
        }
    }

    fn mac_flash(&mut self, cx: &mut Cx) {
        self.mac.flash_until = cx.seconds_since_app_start() + FLASH_S;
        cx.stop_timer(self.mac.flash_timer);
        self.mac.flash_timer = cx.start_timeout(FLASH_S + 0.02);
    }

    /// The widget tree's text: what the panel shows and where its controls
    /// are, so a test reads the state it asserts. Never on screen.
    pub(super) fn mac_tree_text(&self) -> String {
        let mut out = self.shown.join("\n");
        for (r, hit) in &self.hits {
            let name = match hit {
                Hit::Close => "Close".to_string(),
                Hit::New => "New Conversation".to_string(),
                Hit::Send => "Send".to_string(),
                Hit::Stop => "Stop".to_string(),
                Hit::StopSystemAgent => "Stop OctoSense".to_string(),
                Hit::OpenProviders | Hit::Retry | Hit::OpenSettings => self.mac.action.clone().unwrap_or_default(),
                Hit::Option { label, .. } => label.clone(),
                Hit::Copy(item) => if self.copied == Some(*item) { "Copied" } else { "Copy" }.to_string(),
                Hit::Field => "Field".to_string(),
                Hit::Knob => "Scroller".to_string(),
                Hit::Pane | Hit::Grip(_) => continue,
            };
            let _ = write!(out, "\nhit {name} | {} {} {} {}", r.pos.x, r.pos.y, r.size.x, r.size.y);
        }
        for (name, r) in &self.mac.places {
            let _ = write!(out, "\nplace {name} | {} {} {} {}", r.pos.x, r.pos.y, r.size.x, r.size.y);
        }
        for (item, r) in &self.mac.replies {
            let _ = write!(out, "\nreply {item} | {} {} {} {}", r.pos.x, r.pos.y, r.size.x, r.size.y);
        }
        if let Some(menu) = &self.mac.menu {
            let c = menu.card;
            let _ = write!(out, "\nmenu | {} {} {} {}", c.pos.x, c.pos.y, c.size.x, c.size.y);
            for (i, ((label, pick), r)) in menu.rows.iter().zip(&menu.bands).enumerate() {
                if pick.is_none() {
                    let _ = write!(out, "\nmenu-rule | {} {} {} {}", r.pos.x, r.pos.y, r.size.x, r.size.y);
                    continue;
                }
                let _ = write!(out, "\nmenu-row {label} | {} {} {} {}", r.pos.x, r.pos.y, r.size.x, r.size.y);
                if menu.cursor == Some(i) {
                    let _ = write!(out, "\nmenu-on {label}");
                }
            }
        }
        out
    }
}

/// What another surface (the glance column) asked the toasts to keep clear
/// of: what they were told that did not come from [`publish`].
fn other_column() -> Option<Rect> {
    let now = crate::shell::notifications::kept_clear();
    if now != WROTE.with(|w| w.get()) {
        OTHER.with(|o| o.set(now));
    }
    OTHER.with(|o| o.get())
}

/// The panel in `slot` is up at `at`, or down: the toasts stack left of the
/// panels, and of the glance column when it stands beside them.
pub fn publish(slot: usize, at: Option<Rect>) {
    let other = other_column();
    let panels = PANELS.with(|p| {
        let mut panels = p.get();
        panels[slot] = at;
        p.set(panels);
        panels
    });
    let all = panels.iter().flatten().copied().chain(other).reduce(|a, b| {
        let x1 = a.pos.x.min(b.pos.x);
        let y1 = a.pos.y.min(b.pos.y);
        let x2 = (a.pos.x + a.size.x).max(b.pos.x + b.size.x);
        let y2 = (a.pos.y + a.size.y).max(b.pos.y + b.size.y);
        rect(x1, y1, x2 - x1, y2 - y1)
    });
    crate::shell::notifications::keep_clear_of(all);
    WROTE.with(|w| w.set(all));
}

/// The part of `window` no panel floats over, for what is drawn on the
/// window and belongs under the panels: the dim behind a sheet.
pub fn clear_of_panels(window: Rect) -> Rect {
    let panels = PANELS.with(|p| p.get());
    let (mut x1, mut x2) = (window.pos.x, window.pos.x + window.size.x);
    // A window can run past a screen made smaller: what shows of it.
    if let Some(screen) = SCREEN.with(|s| s.get()) {
        x1 = x1.max(screen.pos.x);
        x2 = x2.min(screen.pos.x + screen.size.x);
    }
    for p in panels.iter().flatten() {
        if p.pos.x + p.size.x <= x1 || p.pos.x >= x2 {
            continue;
        }
        if p.pos.x + p.size.x * 0.5 >= (x1 + x2) * 0.5 {
            x2 = x2.min(p.pos.x);
        } else {
            x1 = x1.max(p.pos.x + p.size.x);
        }
    }
    rect(x1, window.pos.y, (x2 - x1).max(0.0), window.size.y)
}

/// Where a menu `size` big opens for a press at `at`: its corner at the
/// pointer, moved in to stay on `screen`, as a Mac's does.
fn menu_place(at: Vec2d, size: Vec2d, screen: Rect) -> Rect {
    let right = screen.pos.x + screen.size.x - 4.0;
    let bottom = screen.pos.y + screen.size.y - 4.0;
    rect(at.x.min(right - size.x).max(screen.pos.x + 4.0), at.y.min(bottom - size.y).max(screen.pos.y + 4.0), size.x, size.y)
}

/// Whether a release on a row chooses it: after a move or a press in the
/// menu, or once the press that opened it was held a moment.
fn release_chooses(armed: bool, held: f64) -> bool {
    armed || held >= MENU_RELEASE_S
}

/// The selection as a Mac's Look Up row quotes it: on one line, and cut
/// short (as a text field's menu quotes its own).
fn menu_quote(text: &str) -> String {
    let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if words.chars().count() > 24 {
        format!("{}\u{2026}", words.chars().take(23).collect::<String>().trim_end())
    } else {
        words
    }
}

/// Words as a URL's query value: unreserved characters as they are, every
/// other byte as %XX, as a text field's menu encodes them.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

/// Look Up and Search With Google, opened as a text field's menu opens them
/// (makepad's `link_label::open_external`): the address made canonical,
/// then the system's handler for it. The panel is the shell's own, not a
/// contained app's, so no page grant applies, and the person chose the row.
/// A hidden test shell only says which.
fn open_words(cx: &mut Cx, url: &str) {
    use makepad_widgets::makepad_platform::{normalize_external_url, CxOsApi as _, OpenUrlInPlace};
    match normalize_external_url(url) {
        Ok(url) => {
            log!("assistant: open {url}");
            if !crate::launch_input::hidden_windows() {
                cx.open_url(&url, OpenUrlInPlace::No);
            }
        }
        Err(error) => log!("assistant: not opened: {error}"),
    }
}

/// A Mac's menu card, as the shell draws its own (shell/menu/mac.rs
/// `draw_mac_card`): the material, a blue band under the lit row and its
/// words in white.
fn draw_menu(d: &mut ShellDraw, cx: &mut Cx2d, look: &MenuLook) {
    // The flat card a Mac's menus fall back to (shell/menu.rs `floating_skin`).
    let border = if look.dark { rgb(82, 82, 88) } else { rgb(160, 160, 165) };
    let card = SurfaceTokens {
        background: if look.dark { rgb(40, 40, 43) } else { rgb(245, 245, 249) },
        text: if look.dark { rgb(242, 242, 245) } else { rgb(24, 24, 28) },
        border,
        border_end: border,
        border_width: 1.0,
        ..SurfaceTokens::default()
    };
    let (ink, accent) = if look.dark { (rgb(245, 245, 247), rgb(10, 96, 200)) } else { (rgb(28, 28, 30), rgb(0, 100, 210)) };
    d.card_no_rounder(cx, look.card, &card, look.card.size.y * 0.25);
    for (i, (label, band)) in look.rows.iter().zip(&look.bands).enumerate() {
        if label.is_empty() {
            // A rule, as the shell's own menus draw one.
            d.solid(cx, rect(look.card.pos.x + 14.0, (band.pos.y + band.size.y * 0.5).floor(), look.card.size.x - 28.0, 1.0), alpha(ink, 0.14));
            continue;
        }
        let on = look.cursor == Some(i);
        if on {
            d.rounded(cx, *band, 2.5, accent);
        }
        let left = look.card.pos.x + MENU_LABEL_X;
        let w = (band.pos.x + band.size.x - 9.0 - left).max(1.0);
        d.label_elided(cx, rect(left, band.pos.y, w, MENU_ROW), false, MENU_TEXT, if on { rgb(255, 255, 255) } else { ink }, HAlign::Left, label);
    }
}

/// The message whose band is under `p`, inside the list.
fn band_at(bands: &[(f64, f64, usize)], list: Rect, p: Vec2d) -> Option<usize> {
    if p.y < list.pos.y || p.y > list.pos.y + list.size.y || p.x < list.pos.x - SIDE || p.x > list.pos.x + list.size.x + SIDE {
        return None;
    }
    bands.iter().find(|(top, bottom, _)| p.y >= *top && p.y < *bottom).map(|(_, _, item)| *item)
}

/// Widths laid out in rows at most `width` wide: each row's index range.
pub fn wrap_widths(widths: &[f64], width: f64, spacing: f64) -> Vec<std::ops::Range<usize>> {
    let mut rows = Vec::new();
    let mut start = 0;
    let mut used = 0.0;
    for (i, w) in widths.iter().enumerate() {
        if i > start && used + spacing + w > width {
            rows.push(start..i);
            start = i;
            used = 0.0;
        }
        used += if i > start { spacing + w } else { *w };
    }
    if start < widths.len() {
        rows.push(start..widths.len());
    }
    rows
}

impl crate::App {
    /// Whether a chat pane answers the keyboard: on a Mac while its prompt
    /// holds it or nothing else does, so a click in an app gives the app
    /// its keys back; elsewhere as it always did.
    pub(crate) fn chat_answers_keys(&mut self, cx: &mut Cx, app_panel: bool) -> bool {
        if !active() {
            return true;
        }
        let pane = if app_panel { self.ui.widget(cx, ids!(shell_app_chat)) } else { self.ui.widget(cx, ids!(shell_system_chat)) };
        let held = pane.borrow::<ShellSystemChat>().is_some_and(|p| p.has_keyboard(cx));
        held || cx.key_focus().is_empty()
    }

    /// A pane that closed gives the keyboard back: to the other pane when
    /// it is still up, else to the window it came from.
    pub(crate) fn mac_chat_focus(&mut self, cx: &mut Cx) {
        if !active() {
            return;
        }
        let mut now = (crate::system_chat::is_open(), crate::app_chat::is_open());
        let was = WAS_OPEN.with(|w| w.get());
        // A small screen has no room for both: the one just opened stays.
        if now.0 && now.1 && !(was.0 && was.1) && !room_for_both() {
            if was.1 {
                crate::app_chat::close();
                now.1 = false;
            } else {
                crate::system_chat::close();
                now.0 = false;
            }
        }
        WAS_OPEN.with(|w| w.set(now));
        let closed = (was.0 && !now.0, was.1 && !now.1);
        if !closed.0 && !closed.1 {
            return;
        }
        let empty = cx.key_focus().is_empty();
        let mut held = false;
        for (gone, id) in [(closed.0, ids!(shell_system_chat)), (closed.1, ids!(shell_app_chat))] {
            if !gone {
                continue;
            }
            let pane = self.ui.widget(cx, id);
            if let Some(mut pane) = pane.borrow_mut::<ShellSystemChat>() {
                held |= pane.held_keyboard(cx);
                pane.release_keyboard(cx);
            };
        }
        let all_closed = !now.0 && !now.1;
        let before = KEYS_BEFORE.with(|k| k.get());
        if all_closed {
            KEYS_BEFORE.with(|k| k.set(false));
            KEYS_UNKNOWN.with(|k| k.set(false));
        }
        // Something else has the keys: they stay there.
        if !held && !empty {
            return;
        }
        if !all_closed {
            KEYS_UNKNOWN.with(|k| k.set(true));
            let id = if now.1 { ids!(shell_app_chat) } else { ids!(shell_system_chat) };
            crate::app_chat::focus(now.1);
            let pane = self.ui.widget(cx, id);
            if let Some(mut pane) = pane.borrow_mut::<ShellSystemChat>() {
                pane.take_keyboard(cx);
            };
        } else if held && before {
            // Back to the very field they came from.
            cx.revert_key_focus();
        } else {
            self.focus_after_layout(cx);
        }
        self.redraw_all(cx);
    }

    /// A chat panel's own keys, before its prompt's: every key while one of
    /// its messages' menus is up (the menu's, as on a Mac), and Shift+
    /// Command+C, its latest reply to the clipboard, the reply's Copy saying
    /// so. True when the key was taken.
    pub(crate) fn mac_chat_keys(&mut self, cx: &mut Cx, e: &KeyEvent) -> bool {
        if !active() {
            return false;
        }
        if let Some(app_panel) = MENU_OWNER.with(|m| m.get()) {
            let id = if app_panel { ids!(shell_app_chat) } else { ids!(shell_system_chat) };
            if let Some(mut pane) = self.ui.widget(cx, id).borrow_mut::<ShellSystemChat>() {
                if pane.mac.menu.is_some() {
                    pane.mac_menu_key(cx, e);
                    return true;
                }
            }
            MENU_OWNER.with(|m| m.set(None));
        }
        let m = &e.modifiers;
        if e.key_code != KeyCode::KeyC || !m.logo || !m.shift || m.control || m.alt {
            return false;
        }
        for (app_panel, open) in [(true, crate::app_chat::is_open()), (false, crate::system_chat::is_open())] {
            if !open || !self.chat_answers_keys(cx, app_panel) {
                continue;
            }
            let id = if app_panel { ids!(shell_app_chat) } else { ids!(shell_system_chat) };
            if let Some(mut pane) = self.ui.widget(cx, id).borrow_mut::<ShellSystemChat>() {
                pane.mac_copy_last_reply(cx);
            }
            return true;
        }
        false
    }

    /// Shift+F8 toggles on a Mac: true when it closed the panel it would
    /// have opened.
    pub(crate) fn mac_ask_toggled(&mut self) -> bool {
        if !active() || !crate::app_chat::is_open() {
            return false;
        }
        let focused = self.focused_agent_app().map(|a| a.id);
        if focused.is_some() && focused == crate::app_chat::app().map(|a| a.id) {
            crate::app_chat::close();
            return true;
        }
        false
    }
}

/// Whether the prompt `pane` can take what is typed now. On a Mac a
/// disabled field really is disabled, and nothing is typed while a
/// message's menu is up; everywhere else it stays as it was.
pub fn typing_blocked(usable: bool) -> bool {
    active() && (!usable || MENU_OWNER.with(|m| m.get()).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Mac's dock: the panel stands under the menu bar and above it.
    #[test]
    fn the_panel_stands_under_the_menu_bar_and_above_the_dock() {
        let screen = rect(0.0, 32.0, 1400.0, 868.0);
        let dock = rect(300.0, 812.0, 800.0, 78.0);
        let work = work_area(screen, dock);
        assert_eq!(work.pos.y, 40.0);
        assert_eq!(work.pos.y + work.size.y, 804.0, "above the dock");
        let hidden = work_area(screen, Rect::default());
        assert_eq!(hidden.pos.y + hidden.size.y, 892.0, "no dock showing: to the screen's edge");
        let pane = default_column(work, &[], false);
        assert_eq!(pane, rect(1012.0, 40.0, W, 764.0), "on the right, 380 wide");
        assert_eq!(default_column(work, &[], true).pos.x, 1012.0 - W - GAP, "Ask <app> left of the system chat");
    }

    /// As tall as what it holds, from a panel's minimum to the room under it.
    #[test]
    fn the_panel_is_as_tall_as_what_it_holds_up_to_the_room_under_it() {
        assert_eq!(panel_height(180.0, 764.0), MIN_H, "a short conversation: the minimum");
        assert_eq!(panel_height(300.0, 764.0), 300.0, "what it holds");
        assert_eq!(panel_height(2000.0, 764.0), 764.0, "a long one: the room above the Dock, then it scrolls");
        assert_eq!(panel_height(300.0, 200.0), 200.0, "a small screen: never past it");
    }

    /// A message's menu opens with its corner at the pointer, moved in to
    /// stay on the screen.
    #[test]
    fn a_messages_menu_opens_at_the_pointer_on_the_screen() {
        let screen = rect(0.0, 0.0, 1400.0, 900.0);
        let size = dvec2(150.0, 32.0);
        assert_eq!(menu_place(dvec2(1100.0, 160.0), size, screen), rect(1100.0, 160.0, 150.0, 32.0));
        assert_eq!(menu_place(dvec2(1350.0, 890.0), size, screen), rect(1246.0, 864.0, 150.0, 32.0), "in from the corner");
    }

    /// The release of the press that opened the menu leaves it open; after
    /// a move, a press in it or a held press, a release chooses.
    #[test]
    fn a_release_chooses_only_after_a_move_or_a_moment() {
        assert!(!release_chooses(false, 0.1), "a click opens it and leaves it open");
        assert!(release_chooses(true, 0.1), "moved onto the row");
        assert!(release_chooses(false, MENU_RELEASE_S), "held, then let go on the row");
    }

    #[test]
    fn the_panel_stands_left_of_a_surface_already_at_the_right() {
        let work = rect(8.0, 40.0, 1384.0, 764.0);
        let glance = rect(1012.0, 40.0, 380.0, 764.0);
        assert_eq!(default_column(work, &[glance], false).pos.x, 1012.0 - GAP - W);
        let left = rect(8.0, 40.0, 300.0, 764.0);
        assert_eq!(default_column(work, &[left], false).pos.x, 1012.0, "nothing in its way");
        // A narrow screen: the whole width, wholly on screen.
        let narrow = rect(8.0, 40.0, 300.0, 400.0);
        let pane = default_column(narrow, &[], false);
        assert_eq!((pane.pos.x, pane.size.x), (8.0, 300.0));
        assert_eq!(default_column(narrow, &[], true).pos.x, 8.0, "beside: never off screen");
    }

    /// The controls win over the frame, with no finger's slop around them.
    #[test]
    fn a_mouse_hits_what_is_under_it() {
        let pane = rect(100.0, 40.0, W, 700.0);
        let close = rect(110.0, 50.0, 24.0, 24.0);
        let send = rect(400.0, 700.0, 56.0, 28.0);
        let mut hits = vec![(pane, Hit::Pane), (close, Hit::Close), (send, Hit::Send)];
        hits.extend(grips_of(pane));
        assert_eq!(hit_exact(&hits, dvec2(120.0, 60.0)), Some(Hit::Close));
        assert_eq!(hit_exact(&hits, dvec2(300.0, 60.0)), Some(Hit::Grip(Grip::Move)), "the title moves it");
        assert_eq!(hit_exact(&hits, dvec2(300.0, 42.0)), Some(Hit::Grip(Grip::N)));
        assert_eq!(hit_exact(&hits, dvec2(350.0, 695.0)), Some(Hit::Pane), "next to Send is not Send");
        assert_eq!(hit_exact(&hits, dvec2(50.0, 60.0)), None);
    }

    /// What is drawn on a window stays off the panel floating over it.
    #[test]
    fn the_dim_stays_off_the_panels() {
        let window = rect(50.0, 40.0, 1200.0, 700.0);
        assert_eq!(clear_of_panels(window), window, "no panel up");
        publish(0, Some(rect(1012.0, 40.0, W, 764.0)));
        assert_eq!(clear_of_panels(window), rect(50.0, 40.0, 962.0, 700.0));
        publish(1, Some(rect(624.0, 40.0, W, 764.0)));
        assert_eq!(clear_of_panels(window), rect(50.0, 40.0, 574.0, 700.0), "beside it");
        publish(0, None);
        publish(1, Some(rect(20.0, 40.0, W, 764.0)));
        assert_eq!(clear_of_panels(window), rect(400.0, 40.0, 850.0, 700.0), "moved to the left");
        publish(1, None);
        assert_eq!(clear_of_panels(window), window);
        assert_eq!(crate::shell::notifications::kept_clear(), None, "the toasts go back");
    }

    /// A sheet centres on its window; a panel over that place moves it aside
    /// just enough when what shows of the window beside the panel holds it,
    /// else it stays centred, over the panel.
    #[test]
    fn a_sheet_centres_on_its_window_and_stands_clear_of_a_panel() {
        let window = rect(50.0, 40.0, 1200.0, 700.0);
        assert_eq!(sheet_room(window, 300.0), rect(500.0, 40.0, 300.0, 700.0), "no panel up");
        publish(0, Some(rect(1012.0, 40.0, W, 764.0)));
        assert_eq!(sheet_room(window, 300.0), rect(500.0, 40.0, 300.0, 700.0), "clear of it");
        assert_eq!(sheet_room(window, 1000.0), rect(150.0, 40.0, 1000.0, 700.0), "no room beside it: over it");
        // A narrow screen: the panel stands over the window's right half.
        let narrow = rect(10.0, 40.0, 780.0, 520.0);
        publish(0, Some(rect(412.0, 40.0, W, 240.0)));
        assert_eq!(sheet_room(narrow, 300.0), rect(104.0, 40.0, 300.0, 520.0), "beside it, a gap from its edge");
        publish(1, Some(rect(24.0, 40.0, W, 240.0)));
        assert_eq!(sheet_room(narrow, 300.0), rect(250.0, 40.0, 300.0, 520.0), "panels both sides: no room, centred");
        publish(1, None);
        publish(0, None);
    }

    /// The Look Up row quotes the selection on one line, cut short; Search
    /// With Google sends its words encoded.
    #[test]
    fn the_reply_menu_quotes_and_encodes_the_selection() {
        assert_eq!(menu_quote("ECHO:\n  Which story"), "ECHO: Which story");
        assert_eq!(menu_quote("a very long selection of words to look up"), "a very long selection o\u{2026}");
        assert_eq!(percent_encode("ECHO: café?"), "ECHO%3A%20caf%C3%A9%3F");
    }

    /// The dim covers all of the window but the panels floating over it:
    /// under a short panel too, and into its rounded corners.
    #[test]
    fn the_dim_covers_the_window_up_to_the_panels() {
        let window = rect(50.0, 40.0, 1200.0, 700.0);
        assert_eq!(dim_areas(window), vec![window], "no panel up");
        assert!(dim_corners(window, 10.0).is_empty());
        let panel = rect(1012.0, 40.0, W, 239.0);
        publish(0, Some(panel));
        let pieces = dim_areas(window);
        let area: f64 = pieces.iter().map(|p| p.size.x * p.size.y).sum();
        assert_eq!(area, 1200.0 * 700.0 - 238.0 * 239.0, "all but the panel, once");
        assert!(pieces.iter().any(|p| p.contains(dvec2(1100.0, 500.0))), "under the panel: {pieces:?}");
        assert!(!pieces.iter().any(|p| p.contains(dvec2(1100.0, 100.0))), "not on it");
        let corners = dim_corners(window, 10.0);
        assert_eq!(corners, vec![(rect(1012.0, 40.0, 10.0, 10.0), panel), (rect(1012.0, 269.0, 10.0, 10.0), panel)], "the corners over the window");
        publish(0, None);
    }

    #[test]
    fn options_wrap_to_rows() {
        assert_eq!(wrap_widths(&[100.0, 100.0, 100.0], 210.0, 8.0), vec![0..2, 2..3]);
        assert_eq!(wrap_widths(&[300.0, 50.0], 210.0, 8.0), vec![0..1, 1..2], "one too wide keeps its own row");
        assert!(wrap_widths(&[], 210.0, 8.0).is_empty());
    }

    /// Nothing of the machine reaches the person.
    #[test]
    fn tools_and_approvals_read_as_plain_words() {
        assert_eq!(tool_line("web_search", &ToolStatus::Running), "Searching the web\u{2026}");
        assert_eq!(tool_line("web_search", &ToolStatus::Done), "Searched the web");
        assert_eq!(tool_line("mcp_x_y", &ToolStatus::Done), "Used a tool");
        assert_eq!(tool_line("news.share_story", &ToolStatus::Done), "Used News");
        assert_eq!(tool_line("os.mail.send", &ToolStatus::Failed), "Couldn\u{2019}t use Mail");
        assert_eq!(approval_line("terminal.run", "", &ApprovalState::Waiting), "Waiting for your approval: Running a command");
        assert_eq!(approval_line("terminal.run", "{\"command\":\"ls\"}", &ApprovalState::Approved), "Approved: Running a command");
        assert_eq!(approval_line("x", "Send the report", &ApprovalState::Denied), "Denied: Send the report");
    }

    #[test]
    fn notices_and_speakers_read_as_plain_words() {
        assert_eq!(plain_speaker("You"), None);
        assert_eq!(plain_speaker("System agent").as_deref(), Some("OctoSense"));
        assert_eq!(plain_speaker("News's agent").as_deref(), Some("News"));
        assert_eq!(plain_speaker("News's agent, to the system agent").as_deref(), Some("News, to OctoSense"));
        assert_eq!(plain_notice("Could not open the conversation: session/open failed {\"a\":1}"), "Couldn\u{2019}t open the conversation.");
        assert_eq!(plain_notice("The system agent asks to use News's assistant: allow or deny it on the sheet."), "OctoSense asks to use News's assistant.");
        assert_eq!(plain_notice("Stopped."), "Stopped.");
        assert_eq!(plain_notice("{\"error\":\"boom\"}"), "Something went wrong. Try again.");
        assert_eq!(plain_notice("request failed at https://example.com/v1"), "Something went wrong. Try again.");
        assert_eq!(plain_notice("provider_unavailable"), "Something went wrong. Try again.");
        assert_eq!(plain_notice("2 more app questions waiting: answer one above to see the next (unanswered ones expire)."), "2 more app questions waiting. Answer one to see the next.");
    }

    /// Every state that stops the person says why in one sentence, with
    /// its fix; no reason from a program on screen.
    #[test]
    fn a_blocked_state_has_a_sentence_and_a_fix() {
        let none = system_blocked(&Phase::NoProvider).unwrap();
        assert_eq!(none.note, "OctoSense needs an AI model before it can answer.");
        assert_eq!(none.action, Some(("Open AI Providers\u{2026}".to_string(), Hit::OpenProviders)));
        let kernel = system_blocked(&Phase::NoKernel("no kernel binary".into())).unwrap();
        assert!(!kernel.note.contains("kernel"));
        assert_eq!(kernel.action.as_ref().map(|a| a.0.as_str()), Some("Try Again"));
        assert!(system_blocked(&Phase::Ready).is_none());
        use crate::app_chat::Status;
        let consent = app_blocked("News", &Status::NeedsConsent).unwrap();
        assert_eq!(consent.note, "News needs your permission to use its assistant.");
        assert_eq!(consent.action.as_ref().map(|a| a.0.as_str()), Some("Allow\u{2026}"));
        let off = app_blocked("News", &Status::Off).unwrap();
        assert_eq!(off.action.map(|a| a.1), Some(Hit::OpenSettings));
        let failed = app_blocked("News", &Status::Failed("peer/prepare: socket closed".into())).unwrap();
        assert!(!failed.note.contains("peer"));
        assert_eq!(failed.action.map(|a| a.1), Some(Hit::Retry));
        assert_eq!(app_blocked("News", &Status::Failed("no model provider".into())).unwrap().title, "No AI Model");
        assert!(app_blocked("News", &Status::Ready).is_none());
    }
}
