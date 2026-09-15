//! The live island (灵动岛): a black pill at the top of the phone, under
//! the status bar, that shows up to THREE ongoing activities — an elapsed
//! timer ticking mm:ss, a segmented progress, a countdown — and expands on
//! tap into a card with each activity's detail and action buttons.
//!
//! Contract with the shell:
//! - The model is `PhoneState::island` (`IslandState`). Producers call
//!   `push` / `update` / `finish` on it, or post a `Report` through the
//!   module's inbox from anywhere (`report`); the inbox is drained on the
//!   phone's animation step (`PhoneState::step` → `IslandState::step`).
//! - Drawing: `draw`, called once from `PhoneSurface::draw_overlay` — under
//!   the shade, over the apps. Taps arrive as `PhoneHit::Island`; the
//!   island needs no gesture beyond tap.
//! - The shade: the island reads `ShellGesture::ShadePull`, `Commit(Shade)`
//!   and `Cancel(Shade)` from `gesture_out` and docks (slides up under the
//!   status bar) while the shade is pulled or open; `shade_open` is the flag
//!   it keeps from those, and the shade module clears it when it closes
//!   (`set_shade_open(false)`).
//! - Cap: `CAP` activities. When a fourth arrives, the oldest finished one
//!   (else the lowest-priority one) is dropped into the shade's
//!   notifications through `dock(DockedNote)` — the note's shape (id, app
//!   label, title, body, time) is `DockedNote`; the shade drains
//!   `take_docked()` until integration wires `dock` to its own push.
//! - Finished activities linger `LINGER` seconds with a checkmark, then
//!   leave.
//!
//! Producers wired here: the hosted AppCard's kernel turns (the wrapper's
//! `ask` tool reports `begin`; the turn's end — and turns typed into the
//! card's own composer — come from polling the wrapper's `turn_in_flight`),
//! and a demo (`--test-action island:demo`, or three quick taps on the
//! status-bar clock on a phone).
use crate::{
    desktop::{DesktopStyle, DrawDesktopChrome},
    mobile::PhoneHit,
    mobile_gestures::{GestureKind, ShellGesture},
    octosense::style::AppIconDraw,
    shell::{alpha, rgb, ui::{rect, HAlign, Ico, ShellDraw}},
    App,
};
use makepad_widgets::*;
use std::cell::Cell;
use std::sync::Mutex;

/// How many activities the island shows at once.
pub const CAP: usize = 3;
/// How long a finished activity stays, with its checkmark, in seconds.
pub const LINGER: f64 = 4.0;
/// Three taps on the clock within this window push the demo (a phone has
/// no `--test-action`).
const CLOCK_TAP_WINDOW: f64 = 1.6;

/// What an activity measures.
#[derive(Clone, Debug, PartialEq)]
pub enum ActivityKind {
    /// Ticking since `started` (mm:ss).
    Elapsed { started: f64 },
    /// `done` of `total` steps; `segments` are each step's own fill 0..1
    /// (empty: a step is full when it is done).
    Progress { done: usize, total: usize, segments: Vec<f32> },
    /// Ticking down to `until`; finishes itself when it gets there.
    Countdown { until: f64 },
}

/// What an activity's button does.
#[derive(Clone, Debug, PartialEq)]
pub enum ActivityAction {
    /// Bring the source app to the front.
    Open(String),
    /// Stop the activity (it finishes now).
    Cancel,
    /// Drop it from the island without finishing it.
    Dismiss,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LiveActivity {
    pub id: String,
    /// The producing app's id (`appcard`), for its glyph.
    pub source: String,
    /// The producing app's label (`AppCard`), for the docked note.
    pub label: String,
    pub title: String,
    pub kind: ActivityKind,
    pub detail: String,
    pub actions: Vec<(String, ActivityAction)>,
    pub started: f64,
    pub updated: f64,
    /// When `finish` was called: the activity lingers `LINGER` seconds.
    pub finished: Option<f64>,
}

impl LiveActivity {
    pub fn new(id: &str, source: &str, label: &str, title: &str, kind: ActivityKind, now: f64) -> Self {
        Self {
            id: id.into(), source: source.into(), label: label.into(), title: title.into(), kind,
            detail: String::new(), actions: Vec::new(), started: now, updated: now, finished: None,
        }
    }
    pub fn with_detail(mut self, detail: &str) -> Self { self.detail = detail.into(); self }
    pub fn with_action(mut self, label: &str, action: ActivityAction) -> Self {
        self.actions.push((label.into(), action));
        self
    }
    pub fn done(&self) -> bool { self.finished.is_some() }
    /// What survives when the island is full: finished ones go first, then
    /// timers, then countdowns; a progress in flight stays.
    pub fn priority(&self) -> u8 {
        if self.done() { return 0; }
        match self.kind {
            ActivityKind::Elapsed { .. } => 1,
            ActivityKind::Countdown { .. } => 2,
            ActivityKind::Progress { .. } => 3,
        }
    }
    /// The fraction complete of a `Progress` (the mean of its segments).
    pub fn fraction(&self) -> Option<f32> {
        match &self.kind {
            ActivityKind::Progress { done, total, segments } => {
                if *total == 0 { return Some(1.0); }
                if segments.is_empty() { return Some((*done as f32 / *total as f32).clamp(0.0, 1.0)); }
                let n = (*total).max(segments.len()) as f32;
                Some((segments.iter().map(|s| s.clamp(0.0, 1.0)).sum::<f32>() / n).clamp(0.0, 1.0))
            }
            _ => None,
        }
    }
    /// The fill of each segment of a `Progress` bar, `total` entries.
    pub fn segment_fills(&self) -> Vec<f32> {
        match &self.kind {
            ActivityKind::Progress { done, total, segments } => (0..*total)
                .map(|i| segments.get(i).copied().unwrap_or(if i < *done { 1.0 } else { 0.0 }).clamp(0.0, 1.0))
                .collect(),
            _ => Vec::new(),
        }
    }
    /// The right-hand text at `now`: the elapsed or remaining time, the
    /// step count, or "Done".
    pub fn status(&self, now: f64) -> String {
        if self.done() { return "Done".into(); }
        match &self.kind {
            ActivityKind::Elapsed { started } => format_clock(now - started),
            ActivityKind::Countdown { until } => format_clock(until - now),
            ActivityKind::Progress { done, total, .. } => format!("{done}/{total}"),
        }
    }
    /// The body of the note this activity leaves in the shade.
    fn note_body(&self, now: f64) -> String {
        if !self.detail.is_empty() { return self.detail.clone(); }
        match &self.kind {
            ActivityKind::Elapsed { .. } if self.done() => format!("Finished after {}", format_clock(self.finished.unwrap_or(now) - self.started)),
            ActivityKind::Elapsed { .. } => format!("Running for {}", self.status(now)),
            ActivityKind::Countdown { .. } if self.done() => "Time is up".into(),
            ActivityKind::Countdown { .. } => format!("{} left", self.status(now)),
            ActivityKind::Progress { .. } if self.done() => "Complete".into(),
            ActivityKind::Progress { done, total, .. } => format!("{done} of {total} done"),
        }
    }
}

/// `mm:ss`, `h:mm:ss` past an hour; never negative.
pub fn format_clock(secs: f64) -> String {
    let s = secs.max(0.0).floor() as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) }
}

// ------------------------------------------------------------------ shade

/// A note the island hands to the shade's notifications: the shape the
/// shade keeps (`mobile_shade`): id, app label, title, body, time.
#[derive(Clone, Debug, PartialEq)]
pub struct DockedNote {
    pub id: String,
    pub app: String,
    pub title: String,
    pub body: String,
    pub time: f64,
}

static DOCKED: Mutex<Vec<DockedNote>> = Mutex::new(Vec::new());

/// Drop a note into the shade. Until the shade's push is connected here,
/// the notes wait in a queue the shade drains with `take_docked`.
pub fn dock(note: DockedNote) {
    if let Ok(mut q) = DOCKED.lock() { q.push(note); }
}
/// The notes docked since the last call (the shade's side of `dock`).
pub fn take_docked() -> Vec<DockedNote> {
    DOCKED.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}

// -------------------------------------------------------------- producers

/// What a producer posts from anywhere (a module wrapper, a closure with
/// no shell access); applied on the next island step.
#[derive(Clone, Debug, PartialEq)]
pub enum Report {
    Begin { id: String, source: String, label: String, title: String },
    Progress { id: String, done: usize, total: usize, segments: Vec<f32>, detail: String },
    Finish { id: String },
}

static INBOX: Mutex<Vec<Report>> = Mutex::new(Vec::new());

/// Post a report; the island applies it on its next step.
pub fn report(r: Report) {
    if let Ok(mut q) = INBOX.lock() { q.push(r); }
}
fn take_reports() -> Vec<Report> {
    INBOX.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}
fn inbox_waiting() -> bool {
    INBOX.lock().map(|q| !q.is_empty()).unwrap_or(false)
}

/// Install the producers this build links. Called once at startup.
pub fn install_producers() {
    #[cfg(feature = "app-appcard")]
    {
        // The AppCard wrapper's `ask` tool reports the turn it submits.
        octosense_appcard::set_activity_reporter(Box::new(IslandReporter));
    }
}

/// The island as the AppCard wrapper's `ActivityReporter`: reports go
/// through the inbox, so the wrapper needs no shell access.
#[cfg(feature = "app-appcard")]
struct IslandReporter;
#[cfg(feature = "app-appcard")]
impl octosense_appcard::ActivityReporter for IslandReporter {
    fn begin(&self, id: &str, title: &str) {
        report(Report::Begin { id: id.into(), source: "appcard".into(), label: "AppCard".into(), title: title.into() });
    }
    fn progress(&self, id: &str, done: usize, total: usize, detail: &str) {
        report(Report::Progress { id: id.into(), done, total, segments: Vec::new(), detail: detail.into() });
    }
    fn finish(&self, id: &str) {
        report(Report::Finish { id: id.into() });
    }
}

// ------------------------------------------------------------------ model

/// A tap on the island.
#[derive(Clone, Debug, PartialEq)]
pub enum IslandHit {
    /// The pill: expand or collapse.
    Toggle,
    /// Outside the expanded card: collapse.
    Collapse,
    /// An activity's button: the activity id and the action index.
    Action(String, usize),
    /// The status-bar clock: three quick taps push the demo.
    Clock,
}

/// The draw-side tween of the pill's geometry, kept in a `Cell` so the
/// surface (which draws from `&WmState`) can animate width and height
/// changes with the frame time.
#[derive(Clone, Copy, Debug)]
struct Anim { w: f64, h: f64, t: f64, settled: bool }
impl Default for Anim {
    /// Never drawn yet: nothing to settle.
    fn default() -> Self { Self { w: 0.0, h: 0.0, t: 0.0, settled: true } }
}

#[derive(Clone, Debug, Default)]
pub struct IslandState {
    pub activities: Vec<LiveActivity>,
    pub expanded: bool,
    /// The activity the pill shows first; else the most important one.
    pub focus: Option<String>,
    /// The shade is open: the island stays hidden until it closes.
    pub shade_open: bool,
    /// A shade pull in progress, 0..1: the island docks with it.
    pub shade_pull: f64,
    /// Animated expansion, 0 (pill) .. 1 (card).
    pub open: f64,
    /// Animated visibility, 0 (docked away) .. 1 (shown).
    pub presence: f64,
    /// Activities the demo finishes at a given time.
    auto_finish: Vec<(String, f64)>,
    clock_taps: Vec<f64>,
    anim: Cell<Anim>,
}

impl IslandState {
    /// Add an activity (replacing one with the same id, keeping its start).
    /// Past the cap, the oldest finished — else the lowest-priority — one
    /// is docked into the shade.
    pub fn push(&mut self, mut activity: LiveActivity) {
        if let Some(existing) = self.activities.iter_mut().find(|a| a.id == activity.id) {
            activity.started = existing.started;
            *existing = activity;
            return;
        }
        let now = activity.updated;
        while self.activities.len() >= CAP {
            let victim = self.activities.iter().enumerate()
                .min_by(|(_, a), (_, b)| {
                    (a.priority(), a.finished.unwrap_or(a.started)).partial_cmp(&(b.priority(), b.finished.unwrap_or(b.started))).unwrap()
                })
                .map(|(i, _)| i)
                .unwrap();
            let dropped = self.activities.remove(victim);
            if self.focus.as_deref() == Some(dropped.id.as_str()) { self.focus = None; }
            dock(DockedNote { id: dropped.id.clone(), app: dropped.label.clone(), title: dropped.title.clone(), body: dropped.note_body(now), time: now });
        }
        self.activities.push(activity);
    }
    pub fn get(&self, id: &str) -> Option<&LiveActivity> { self.activities.iter().find(|a| a.id == id) }
    /// Change an activity in place; it counts as updated at `now`.
    pub fn update(&mut self, id: &str, now: f64, f: impl FnOnce(&mut LiveActivity)) -> bool {
        match self.activities.iter_mut().find(|a| a.id == id) {
            Some(a) => { f(a); a.updated = now; true }
            None => false,
        }
    }
    /// Mark an activity finished: it shows a checkmark for `LINGER`
    /// seconds, then leaves.
    pub fn finish(&mut self, id: &str, now: f64) -> bool {
        self.update(id, now, |a| {
            if a.finished.is_none() { a.finished = Some(now); }
            if let ActivityKind::Progress { done, total, segments } = &mut a.kind {
                *done = *total;
                for s in segments.iter_mut() { *s = 1.0; }
            }
        })
    }
    /// Remove an activity outright (no linger, no note).
    pub fn remove(&mut self, id: &str) {
        self.activities.retain(|a| a.id != id);
        if self.focus.as_deref() == Some(id) { self.focus = None; }
    }
    pub fn set_shade_open(&mut self, open: bool) { self.shade_open = open; if open { self.expanded = false; } }
    /// Nothing to show: no activity, or the shade owns the top edge.
    pub fn hidden(&self) -> bool { self.activities.is_empty() || self.shade_open || self.shade_pull > 0.001 }
    /// The activity the pill shows: the focused one, else the unfinished
    /// one of highest priority, most recently updated.
    pub fn primary(&self) -> Option<&LiveActivity> {
        if let Some(a) = self.focus.as_ref().and_then(|id| self.get(id)) { return Some(a); }
        self.activities.iter().max_by(|a, b| {
            (!a.done(), a.priority(), a.updated).partial_cmp(&(!b.done(), b.priority(), b.updated)).unwrap()
        })
    }
    /// A tap. `Open` comes back for the shell to bring the app forward.
    pub fn on_hit(&mut self, hit: IslandHit, now: f64) -> Option<ActivityAction> {
        match hit {
            IslandHit::Toggle => { if !self.hidden() { self.expanded = !self.expanded; } }
            IslandHit::Collapse => self.expanded = false,
            IslandHit::Action(id, index) => {
                let action = self.get(&id).and_then(|a| a.actions.get(index)).map(|(_, a)| a.clone());
                match action {
                    Some(ActivityAction::Cancel) => { self.finish(&id, now); }
                    Some(ActivityAction::Dismiss) => self.remove(&id),
                    Some(ActivityAction::Open(app)) => return Some(ActivityAction::Open(app)),
                    None => {}
                }
            }
            IslandHit::Clock => {
                self.clock_taps.retain(|t| now - t < CLOCK_TAP_WINDOW);
                self.clock_taps.push(now);
                if self.clock_taps.len() >= 3 { self.clock_taps.clear(); self.demo(now); }
            }
        }
        None
    }
    /// Three demo activities: a ticking timer that finishes after 6 s (the
    /// linger), a three-segment progress, and a countdown with a button.
    pub fn demo(&mut self, now: f64) {
        self.push(LiveActivity::new("demo:writing", "appcard", "AppCard", "Writing weather card", ActivityKind::Elapsed { started: now }, now)
            .with_detail("Kernel turn: weather, Tokyo"));
        self.push(LiveActivity::new("demo:forecast", "appcard", "AppCard", "Fetching forecast", ActivityKind::Progress { done: 1, total: 3, segments: vec![1.0, 0.45, 0.0] }, now)
            .with_detail("Open-Meteo: current, hourly, daily")
            .with_action("Open", ActivityAction::Open("appcard".into())));
        self.push(LiveActivity::new("demo:pomodoro", "clock", "Clock", "Pomodoro", ActivityKind::Countdown { until: now + 25.0 * 60.0 }, now)
            .with_detail("Focus, then a 5 minute break")
            .with_action("Stop", ActivityAction::Cancel));
        self.auto_finish.retain(|(id, _)| id != "demo:writing");
        self.auto_finish.push(("demo:writing".into(), now + 6.0));
    }
    /// Apply a producer's report.
    pub fn apply(&mut self, r: Report, now: f64) {
        match r {
            Report::Begin { id, source, label, title } => {
                self.push(LiveActivity::new(&id, &source, &label, &title, ActivityKind::Elapsed { started: now }, now));
            }
            Report::Progress { id, done, total, segments, detail } => {
                if self.get(&id).is_none() {
                    self.push(LiveActivity::new(&id, "", "", &id, ActivityKind::Progress { done, total, segments: segments.clone() }, now));
                }
                self.update(&id, now, |a| {
                    a.kind = ActivityKind::Progress { done, total, segments };
                    if !detail.is_empty() { a.detail = detail; }
                });
            }
            Report::Finish { id } => { self.finish(&id, now); }
        }
    }
    /// The hosted AppCard's turns, from the wrapper: a turn in flight with
    /// no activity of its own (typed into the card's composer) begins one;
    /// no turn in flight finishes the ones there are.
    #[cfg(feature = "app-appcard")]
    fn poll_appcard(&mut self, now: f64) {
        let running: Vec<String> = self.activities.iter().filter(|a| a.source == "appcard" && !a.done()).map(|a| a.id.clone()).collect();
        match octosense_appcard::turn_in_flight() {
            Some(text) => {
                if running.is_empty() {
                    let title = if text.trim().is_empty() { "AppCard is thinking".to_string() } else { text.trim().to_string() };
                    self.push(LiveActivity::new("appcard:turn", "appcard", "AppCard", &title, ActivityKind::Elapsed { started: now }, now)
                        .with_detail("Kernel turn")
                        .with_action("Open", ActivityAction::Open("appcard".into())));
                }
            }
            None => {
                for id in running {
                    // A submit races the first poll by a frame: give it half a second.
                    if self.get(&id).is_some_and(|a| now - a.started > 0.5) { self.finish(&id, now); }
                }
            }
        }
    }
    /// Whether the shell should run a frame for the island now (it is
    /// called once a second from the clock tick while nothing animates).
    pub fn needs_step(&self) -> bool {
        if !self.activities.is_empty() || inbox_waiting() { return true; }
        #[cfg(feature = "app-appcard")]
        if octosense_appcard::turn_in_flight().is_some() { return true; }
        false
    }
    /// One animation step. Drains the producers, times out finished
    /// activities and countdowns, follows the shade, tweens the pill.
    /// True while something still moves.
    pub fn step(&mut self, dt: f64, now: f64, gesture: Option<ShellGesture>) -> bool {
        for r in take_reports() { self.apply(r, now); }
        #[cfg(feature = "app-appcard")]
        self.poll_appcard(now);
        let due: Vec<String> = self.auto_finish.iter().filter(|(_, at)| now >= *at).map(|(id, _)| id.clone()).collect();
        self.auto_finish.retain(|(_, at)| now < *at);
        for id in due { self.finish(&id, now); }
        let ended: Vec<String> = self.activities.iter()
            .filter(|a| !a.done() && matches!(a.kind, ActivityKind::Countdown { until } if now >= until))
            .map(|a| a.id.clone()).collect();
        for id in ended { self.finish(&id, now); }
        let gone: Vec<String> = self.activities.iter().filter(|a| a.finished.is_some_and(|t| now - t >= LINGER)).map(|a| a.id.clone()).collect();
        for id in gone { self.remove(&id); }
        match gesture {
            Some(ShellGesture::ShadePull { progress, .. }) => { self.shade_pull = progress.clamp(0.0, 1.0); self.expanded = false; }
            Some(ShellGesture::Commit(GestureKind::Shade(_))) => { self.shade_pull = 0.0; self.set_shade_open(true); }
            Some(ShellGesture::Cancel(GestureKind::Shade(_))) => self.shade_pull = 0.0,
            _ => {}
        }
        if self.activities.is_empty() { self.expanded = false; }
        let t = 1.0 - (-dt * 16.0).exp();
        let presence = if self.activities.is_empty() || self.shade_open { 0.0 } else { 1.0 - self.shade_pull };
        let open = if self.expanded && presence > 0.5 { 1.0 } else { 0.0 };
        let mut moving = false;
        for (value, target) in [(&mut self.presence, presence), (&mut self.open, open)] {
            *value += (target - *value) * t;
            if (*value - target).abs() < 0.002 { *value = target; }
            moving |= *value != target;
        }
        // A deadline within a frame or two is stepped on the frame loop;
        // a farther one is the 1 s tick's (`needs_step`), so a demo with a
        // 25-minute countdown does not hold the loop at 60 fps.
        let soon = |at: f64| at - now < 0.05;
        moving
            || self.auto_finish.iter().any(|(_, at)| soon(*at))
            || self.activities.iter().any(|a| a.done() || matches!(a.kind, ActivityKind::Countdown { until } if soon(until)))
            || !self.anim.get().settled
    }
}

// --------------------------------------------------------------- the shell

impl App {
    /// `--test-action island:demo` pushes the three demo activities (on a
    /// desktop run the shell switches to the Android phone first);
    /// `island:expand` opens the card; `island:phone` only switches, for a
    /// run that watches a real producer. True when `name` was one of ours.
    pub(crate) fn island_test_action(&mut self, cx: &mut Cx, name: &str) -> bool {
        let Some(what) = name.strip_prefix("island:") else { return false };
        log!("wm: --test-action island:{}", what);
        if !self.state_mut().style.target.mobile() { self.set_desktop_style(cx, DesktopStyle::Android); }
        let now = crate::host::now();
        match what {
            "demo" => self.state_mut().phone.island.demo(now),
            "expand" => self.state_mut().phone.island.expanded = true,
            "phone" => {}
            other => log!("wm: unknown --test-action island:{}", other),
        }
        self.animate_phone(cx);
        true
    }
    /// A tap on the island (`PhoneHit::Island`). The app an `Open` button
    /// names comes back for the phone to bring forward.
    pub(crate) fn island_hit(&mut self, hit: IslandHit) -> Option<String> {
        match self.state_mut().phone.island.on_hit(hit, crate::host::now()) {
            Some(ActivityAction::Open(app)) => Some(app),
            _ => None,
        }
    }
    /// From the clock tick: while the island has something to time, run a
    /// frame so its step sees the second pass (the phone's animation loop
    /// stops on its own once nothing moves).
    pub(crate) fn wake_island(&mut self, cx: &mut Cx) {
        if self.state.as_ref().is_some_and(|s| s.style.target.mobile() && s.phone.island.needs_step()) { self.animate_phone(cx); }
    }
}

// ---------------------------------------------------------------- drawing

fn rounded(cx: &mut Cx2d, chrome: &mut DrawDesktopChrome, r: Rect, radius: f64, color: Vec4f) {
    if r.size.x <= 0.0 || r.size.y <= 0.0 || color.w <= 0.0 { return; }
    chrome.radius = (radius * 2.0) as f32;
    chrome.bevel = 0.0;
    chrome.color = color;
    chrome.draw_abs(cx, r);
}
fn mix(a: f64, b: f64, t: f64) -> f64 { a + (b - a) * t }

/// The card's height for these activities.
fn card_height(activities: &[LiveActivity]) -> f64 {
    14.0 + activities.iter().map(|a| 56.0 + if a.fraction().is_some() { 18.0 } else { 0.0 } + if a.actions.is_empty() { 0.0 } else { 40.0 }).sum::<f64>()
}

/// Draw the island for this frame and register its taps. One call from
/// `PhoneSurface::draw_overlay`, after the status bar.
pub fn draw(cx: &mut Cx2d, chrome: &mut DrawDesktopChrome, d: &mut ShellDraw, icons: &mut AppIconDraw, hits: &mut Vec<(Rect, PhoneHit)>, state: &crate::desk::WmState, screen: Rect) {
    let island = &state.phone.island;
    let style = state.style.target;
    let ios = style == DesktopStyle::Ios;
    let now = crate::host::now();
    let landscape = screen.size.x > screen.size.y;
    let status_h = if landscape { 24.0 } else { 42.0 };
    let pill_h = if landscape { 22.0 } else { 34.0 };
    // The clock: three quick taps push the demo on a phone.
    hits.push((rect(screen.pos.x, screen.pos.y, 90.0, status_h), PhoneHit::Island(IslandHit::Clock)));
    let Some(primary) = island.primary() else {
        island.anim.set(Anim::default());
        return;
    };
    let presence = island.presence;
    let open = island.open;
    let white = rgb(255, 255, 255);
    let accent = if ios { rgb(10, 132, 255) } else { rgb(172, 200, 255) };
    let green = rgb(52, 199, 89);
    // ---- geometry: the compact pill is sized by its content, the card by the screen.
    let px = if landscape { 11.0 } else { 13.0 };
    let glyph = if landscape { 16.0 } else { 20.0 };
    let title_w = d.measure(cx, true, px, &primary.title).min(if landscape { 120.0 } else { 168.0 });
    let status = primary.status(now);
    let status_w = if primary.done() { 16.0 } else if primary.fraction().is_some() { 18.0 } else { d.measure(cx, true, px, &status) };
    let dots = island.activities.len().saturating_sub(1) as f64;
    let compact_w = 12.0 + glyph + 8.0 + title_w + 10.0 + status_w + dots * 10.0 + 12.0;
    let card_w = (screen.size.x - 24.0).min(400.0);
    let card_h = card_height(&island.activities);
    let target_w = mix(compact_w, card_w, open);
    let target_h = mix(pill_h, card_h, open);
    let mut a = island.anim.get();
    if a.settled && a.w == 0.0 { a.w = target_w; a.h = target_h; }
    let dt = if a.t == 0.0 { 1.0 / 60.0 } else { (now - a.t).clamp(0.0, 0.05) };
    let k = 1.0 - (-dt * 18.0).exp();
    a.w += (target_w - a.w) * k;
    a.h += (target_h - a.h) * k;
    a.settled = (a.w - target_w).abs() < 0.5 && (a.h - target_h).abs() < 0.5;
    if a.settled { a.w = target_w; a.h = target_h; }
    a.t = now;
    island.anim.set(a);
    if presence < 0.01 { return; }
    let alpha_f = presence as f32;
    let top = screen.pos.y + (status_h - pill_h) * 0.5 - (1.0 - presence) * (pill_h + 10.0);
    let r = rect(screen.pos.x + (screen.size.x - a.w) * 0.5, top, a.w, a.h);
    let radius = mix(pill_h * 0.5, 24.0, open);
    // ---- the surface: a soft shadow, the black pill, a hairline of light.
    rounded(cx, chrome, rect(r.pos.x - 1.0, r.pos.y + 2.0, r.size.x + 2.0, r.size.y + 2.0), radius + 1.0, alpha(rgb(0, 0, 0), 0.28 * alpha_f));
    rounded(cx, chrome, rect(r.pos.x - 0.5, r.pos.y - 0.5, r.size.x + 1.0, r.size.y + 1.0), radius + 0.5, alpha(white, (0.06 + 0.08 * open as f32) * alpha_f));
    rounded(cx, chrome, r, radius, alpha(rgb(6, 6, 9), (0.97 + 0.025 * open as f32) * alpha_f));
    // ---- taps.
    if open < 0.5 {
        hits.push((rect(r.pos.x - 8.0, r.pos.y - 4.0, r.size.x + 16.0, r.size.y + 8.0), PhoneHit::Island(IslandHit::Toggle)));
    } else {
        hits.push((screen, PhoneHit::Island(IslandHit::Collapse)));
    }
    // ---- compact: glyph, title, the time or the ring, the other activities' dots.
    let compact = ((1.0 - open * 2.5).clamp(0.0, 1.0) as f32) * alpha_f;
    if compact > 0.01 {
        let ink = alpha(white, compact);
        let dim = alpha(white, 0.45 * compact);
        let mut x = r.pos.x + 12.0;
        let mid = r.pos.y + pill_h * 0.5;
        icons.draw(cx, &primary.source, style, rect(x, mid - glyph * 0.5, glyph, glyph), compact, ink);
        x += glyph + 8.0;
        d.label_elided(cx, rect(x, r.pos.y, title_w, pill_h), true, px, ink, HAlign::Left, &primary.title);
        x += title_w + 10.0;
        if primary.done() {
            d.icon_centered(cx, Ico::Check, rect(x, r.pos.y, 16.0, pill_h), 14.0, alpha(green, compact));
        } else if let Some(fraction) = primary.fraction() {
            ring(cx, chrome, dvec2(x + 9.0, mid), 7.0, fraction, alpha(accent, compact), dim);
        } else {
            d.label(cx, rect(x, r.pos.y, status_w, pill_h), true, px, ink, HAlign::Left, &status);
        }
        x += status_w + 6.0;
        for other in island.activities.iter().filter(|o| o.id != primary.id) {
            let color = if other.done() { alpha(green, compact) } else if other.fraction().is_some() { alpha(accent, compact) } else { dim };
            rounded(cx, chrome, rect(x + 2.0, mid - 3.0, 6.0, 6.0), 3.0, color);
            x += 10.0;
        }
    }
    // ---- expanded: a row per activity, its bar and its buttons.
    let card = ((open - 0.4) / 0.6).clamp(0.0, 1.0) as f32 * alpha_f;
    if card > 0.01 {
        let ink = alpha(white, card);
        let soft = alpha(white, 0.62 * card);
        let mut y = r.pos.y + 14.0;
        let left = r.pos.x + 18.0;
        let inner_w = r.size.x - 36.0;
        for (index, activity) in island.activities.iter().enumerate() {
            let glyph = 30.0;
            icons.draw(cx, &activity.source, style, rect(left, y + 4.0, glyph, glyph), card, ink);
            let text_x = left + glyph + 12.0;
            let status = activity.status(now);
            let status_w = if activity.done() { 22.0 } else { d.measure(cx, true, 14.0, &status) + 4.0 };
            let text_w = inner_w - glyph - 12.0 - status_w - 8.0;
            d.label_elided(cx, rect(text_x, y, text_w, 22.0), true, 14.0, ink, HAlign::Left, &activity.title);
            let detail = if activity.detail.is_empty() { activity.label.clone() } else { activity.detail.clone() };
            d.label_elided(cx, rect(text_x, y + 22.0, text_w, 20.0), false, 12.0, soft, HAlign::Left, &detail);
            if activity.done() {
                d.icon_centered(cx, Ico::Check, rect(left + inner_w - 22.0, y, 22.0, 22.0), 18.0, alpha(green, card));
            } else {
                d.label(cx, rect(left + inner_w - status_w, y, status_w, 22.0), true, 14.0, ink, HAlign::Right, &status);
            }
            y += 50.0;
            let fills = activity.segment_fills();
            if !fills.is_empty() {
                let gap = 4.0;
                let seg_w = (inner_w - gap * (fills.len() as f64 - 1.0)) / fills.len() as f64;
                for (i, fill) in fills.iter().enumerate() {
                    let sx = left + i as f64 * (seg_w + gap);
                    rounded(cx, chrome, rect(sx, y, seg_w, 6.0), 3.0, alpha(white, 0.16 * card));
                    let done = if activity.done() { green } else { accent };
                    rounded(cx, chrome, rect(sx, y, seg_w * *fill as f64, 6.0), 3.0, alpha(done, card));
                }
                y += 18.0;
            }
            if !activity.actions.is_empty() {
                let mut bx = text_x;
                for (n, (label, _)) in activity.actions.iter().enumerate() {
                    let w = d.measure(cx, true, 12.0, label) + 28.0;
                    let b = rect(bx, y + 4.0, w, 30.0);
                    rounded(cx, chrome, b, 15.0, alpha(white, 0.14 * card));
                    d.label(cx, b, true, 12.0, ink, HAlign::Center, label);
                    if open > 0.9 { hits.push((rect(b.pos.x - 4.0, b.pos.y - 4.0, b.size.x + 8.0, b.size.y + 8.0), PhoneHit::Island(IslandHit::Action(activity.id.clone(), n)))); }
                    bx += w + 10.0;
                }
                y += 40.0;
            }
            y += 6.0;
            if index + 1 < island.activities.len() {
                rounded(cx, chrome, rect(left, y - 3.0, inner_w, 1.0), 0.5, alpha(white, 0.10 * card));
            }
        }
    }
}

/// A tiny progress ring: eight dots around `center`, `fraction` of them
/// lit — no arc shader needed.
fn ring(cx: &mut Cx2d, chrome: &mut DrawDesktopChrome, center: Vec2d, radius: f64, fraction: f32, lit: Vec4f, dim: Vec4f) {
    let n = 8;
    let count = (fraction.clamp(0.0, 1.0) * n as f32).round() as usize;
    for i in 0..n {
        let angle = -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::TAU / n as f64;
        let p = center + dvec2(angle.cos(), angle.sin()) * radius;
        rounded(cx, chrome, rect(p.x - 1.5, p.y - 1.5, 3.0, 3.0), 1.5, if i < count { lit } else { dim });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobile_gestures::ShadeSide;

    fn elapsed(id: &str, now: f64) -> LiveActivity {
        LiveActivity::new(id, "appcard", "AppCard", id, ActivityKind::Elapsed { started: now }, now)
    }
    fn settle(island: &mut IslandState, now: f64) {
        for _ in 0..120 { island.step(1.0 / 60.0, now, None); }
    }

    #[test]
    fn a_fourth_activity_docks_the_oldest_finished_else_the_least_important() {
        let mut island = IslandState::default();
        island.push(elapsed("cap:a", 1.0));
        island.push(LiveActivity::new("cap:b", "appcard", "AppCard", "Fetching", ActivityKind::Progress { done: 0, total: 2, segments: vec![] }, 2.0).with_detail("two calls"));
        island.push(elapsed("cap:c", 3.0));
        assert_eq!(island.activities.len(), 3);
        // Nothing finished: the oldest timer goes, the progress in flight stays.
        island.push(elapsed("cap:d", 4.0));
        let ids: Vec<&str> = island.activities.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["cap:b", "cap:c", "cap:d"]);
        let docked = take_docked();
        let note = docked.iter().find(|n| n.id == "cap:a").expect("the dropped timer became a note");
        assert_eq!((note.app.as_str(), note.title.as_str(), note.time), ("AppCard", "cap:a", 4.0));
        assert_eq!(note.body, "Running for 00:03");
        // A finished one goes before any live one, however important.
        island.finish("cap:b", 5.0);
        island.push(elapsed("cap:e", 6.0));
        let ids: Vec<&str> = island.activities.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["cap:c", "cap:d", "cap:e"]);
        let docked = take_docked();
        let note = docked.iter().find(|n| n.id == "cap:b").expect("the finished progress became a note");
        assert_eq!(note.body, "two calls");
        assert_eq!(island.activities.len(), CAP);
    }

    #[test]
    fn finishing_lingers_with_a_checkmark_then_leaves() {
        let mut island = IslandState::default();
        island.push(LiveActivity::new("lin:p", "appcard", "AppCard", "Fetching", ActivityKind::Progress { done: 1, total: 3, segments: vec![1.0, 0.5, 0.0] }, 0.0));
        assert!(island.finish("lin:p", 10.0));
        let a = island.get("lin:p").unwrap();
        assert!(a.done());
        assert_eq!(a.status(11.0), "Done");
        assert_eq!(a.segment_fills(), vec![1.0, 1.0, 1.0], "a finished progress is full");
        assert!(island.step(1.0 / 60.0, 10.0 + LINGER - 0.1, None), "still lingering: keeps animating");
        assert!(island.get("lin:p").is_some());
        island.step(1.0 / 60.0, 10.0 + LINGER, None);
        assert!(island.get("lin:p").is_none(), "gone after the linger");
        settle(&mut island, 20.0);
        assert!(!island.step(1.0 / 60.0, 20.0, None), "an empty island settles");
        assert!(!island.finish("lin:p", 21.0));
    }

    #[test]
    fn far_deadlines_wake_through_the_tick_not_the_frame_loop() {
        let mut island = IslandState::default();
        island.demo(100.0);
        settle(&mut island, 100.5);
        // The demo's 25-minute countdown and its 6 s auto-finish are due
        // later: the frame loop may stop, the 1 s tick keeps the island.
        assert!(!island.step(1.0 / 60.0, 101.0, None), "nothing moves this frame");
        assert!(island.needs_step(), "the tick still wakes it");
        // Within a frame of the auto-finish the loop runs it.
        assert!(island.step(1.0 / 60.0, 100.0 + 6.0 - 0.02, None));
    }

    #[test]
    fn elapsed_and_countdown_format_as_clocks() {
        assert_eq!(format_clock(0.0), "00:00");
        assert_eq!(format_clock(65.9), "01:05");
        assert_eq!(format_clock(-3.0), "00:00");
        assert_eq!(format_clock(3600.0 + 61.0), "1:01:01");
        let timer = elapsed("fmt:t", 100.0);
        assert_eq!(timer.status(100.0 + 754.0), "12:34");
        let count = LiveActivity::new("fmt:c", "clock", "Clock", "Pomodoro", ActivityKind::Countdown { until: 100.0 + 25.0 * 60.0 }, 100.0);
        assert_eq!(count.status(100.0), "25:00");
        assert_eq!(count.status(100.0 + 24.0 * 60.0 + 59.0), "00:01");
        assert_eq!(count.status(100.0 + 26.0 * 60.0), "00:00");
        let progress = LiveActivity::new("fmt:p", "appcard", "AppCard", "Fetching", ActivityKind::Progress { done: 2, total: 3, segments: vec![1.0, 1.0, 0.5] }, 0.0);
        assert_eq!(progress.status(0.0), "2/3");
        assert!((progress.fraction().unwrap() - 2.5 / 3.0).abs() < 1e-6);
        // A countdown finishes itself when it reaches zero.
        let mut island = IslandState::default();
        island.push(count.clone());
        island.step(1.0 / 60.0, 100.0 + 25.0 * 60.0, None);
        assert!(island.get("fmt:c").unwrap().done());
    }

    #[test]
    fn taps_expand_collapse_and_run_actions() {
        let mut island = IslandState::default();
        assert_eq!(island.on_hit(IslandHit::Toggle, 0.0), None);
        assert!(!island.expanded, "nothing to expand on an empty island");
        island.push(elapsed("tap:a", 0.0).with_action("Open", ActivityAction::Open("appcard".into())).with_action("Stop", ActivityAction::Cancel));
        island.on_hit(IslandHit::Toggle, 1.0);
        assert!(island.expanded);
        settle(&mut island, 1.0);
        assert!((island.open - 1.0).abs() < 0.01 && (island.presence - 1.0).abs() < 0.01);
        island.on_hit(IslandHit::Collapse, 2.0);
        assert!(!island.expanded);
        settle(&mut island, 2.0);
        assert!(island.open.abs() < 0.01);
        assert_eq!(island.on_hit(IslandHit::Action("tap:a".into(), 0), 3.0), Some(ActivityAction::Open("appcard".into())));
        assert_eq!(island.on_hit(IslandHit::Action("tap:a".into(), 1), 4.0), None);
        assert!(island.get("tap:a").unwrap().done(), "Stop finishes the activity");
        assert_eq!(island.on_hit(IslandHit::Action("tap:nope".into(), 0), 5.0), None);
        // Three quick taps on the clock push the demo; slow ones do not.
        for t in [10.0, 12.0, 14.0] { island.on_hit(IslandHit::Clock, t); }
        assert!(island.get("demo:writing").is_none());
        for t in [20.0, 20.3, 20.6] { island.on_hit(IslandHit::Clock, t); }
        assert!(island.get("demo:writing").is_some() && island.get("demo:pomodoro").is_some());
        assert_eq!(island.activities.len(), CAP, "the demo's three fill the island");
        island.step(1.0 / 60.0, 27.0, None);
        assert!(island.get("demo:writing").unwrap().done(), "the demo's timer finishes after six seconds");
    }

    #[test]
    fn hidden_while_the_shade_is_pulled_or_open() {
        let mut island = IslandState::default();
        island.push(elapsed("shade:a", 0.0));
        island.expanded = true;
        settle(&mut island, 0.0);
        assert!(!island.hidden() && island.presence > 0.99);
        island.step(1.0 / 60.0, 1.0, Some(ShellGesture::ShadePull { side: ShadeSide::Notifications, progress: 0.5 }));
        assert!(island.hidden() && !island.expanded, "a pull docks the island and closes the card");
        settle(&mut island, 1.0);
        assert!((island.presence - 0.5).abs() < 0.01, "docked with the pull");
        island.step(1.0 / 60.0, 2.0, Some(ShellGesture::Cancel(GestureKind::Shade(ShadeSide::Notifications))));
        settle(&mut island, 2.0);
        assert!(!island.hidden() && island.presence > 0.99, "a cancelled pull brings it back");
        island.step(1.0 / 60.0, 3.0, Some(ShellGesture::ShadePull { side: ShadeSide::Controls, progress: 1.0 }));
        island.step(1.0 / 60.0, 3.0, Some(ShellGesture::Commit(GestureKind::Shade(ShadeSide::Controls))));
        assert!(island.shade_open && island.hidden());
        settle(&mut island, 3.0);
        assert!(island.presence < 0.01, "gone while the shade is open");
        island.on_hit(IslandHit::Toggle, 4.0);
        assert!(!island.expanded, "no card under the shade");
        island.set_shade_open(false);
        settle(&mut island, 5.0);
        assert!(!island.hidden() && island.presence > 0.99);
    }

    #[test]
    fn reports_begin_update_and_finish_activities() {
        let mut island = IslandState::default();
        island.apply(Report::Begin { id: "rep:1".into(), source: "appcard".into(), label: "AppCard".into(), title: "weather tokyo".into() }, 1.0);
        assert_eq!(island.primary().map(|a| a.title.as_str()), Some("weather tokyo"));
        island.apply(Report::Progress { id: "rep:1".into(), done: 1, total: 2, segments: vec![], detail: "routing".into() }, 2.0);
        let a = island.get("rep:1").unwrap();
        assert_eq!((a.status(2.0).as_str(), a.detail.as_str(), a.started), ("1/2", "routing", 1.0));
        island.apply(Report::Finish { id: "rep:1".into() }, 3.0);
        assert!(island.get("rep:1").unwrap().done());
        // The primary prefers live work over finished work, and the focus over both.
        island.push(elapsed("rep:2", 4.0));
        assert_eq!(island.primary().map(|a| a.id.as_str()), Some("rep:2"));
        island.focus = Some("rep:1".into());
        assert_eq!(island.primary().map(|a| a.id.as_str()), Some("rep:1"));
    }
}
