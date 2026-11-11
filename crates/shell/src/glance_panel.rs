//! The desktop's glance panel: the cards apps published to the glance
//! screen (glance.rs), in a column at the right edge of the desk, over the
//! windows, the way the phone shows them on its glance page.
//!
//! F9 (or `--test-action glance`) shows and hides it. Each card is live: the
//! panel hands it the pointer inside its tile and every other event
//! (typing, focus, timers, answers), so it works as it does in its app
//! (glance_card.rs). A card takes the shell's light or dark palette unless
//! it names a theme of its own.
//!
//! - **A card's own controls** (its chips, buttons and fields) work in place.
//!   A press anywhere else on it opens the card in the card window
//!   ([`ShellGlancePanelAction::OpenCard`]), as its notification does. A
//!   field claims its press; a chip (a tap target, `OctoscriptTap`) claims
//!   none and calls its card once the release's handlers have run, so a
//!   release is held for a frame: a click from that card means its control
//!   had it.
//! - **The hovered card** shows its actions over its top-right corner, on a
//!   backdrop: open the app that published it, and dismiss the card. They
//!   are not drawn over every card's content all the time.
//! - **The header** says how many cards there are and has Clear all and the
//!   panel's close; a click outside the column closes it too. With no card
//!   it says so ("You're all caught up").
//! - **The list** starts at the top and the wheel moves it a card at a time;
//!   a new card scrolls it no further than shows that card
//!   ([`first_to_reveal`]). "More above" heads the list and "more below"
//!   ends it; the card that does not fit whole peeks in above that line,
//!   cut at the list's end.
//! - **A card taller than its tile** (the cap, glance_card.rs) scrolls
//!   inside it ([`TileScroll`]), with a thumb at its edge. A card with an
//!   in-card chat (`sys.chat`) sits at its newest rows and follows them as
//!   the conversation grows, so its field and the latest exchange stay in
//!   view; any other card starts at its top. The wheel over such a card
//!   scrolls it first and moves the list only at its ends. The tile's own
//!   layout scrolls, so a press the card's controls do not claim still
//!   opens the card window.
//! - **A dismissal** (a card's close, Delete, Clear all) can be undone: the
//!   shell's toast offers Undo ([`ShellGlancePanelAction::Dismissed`]), and
//!   ⌘Z does it while the panel holds the keyboard. On a Mac there is no
//!   toast and no column-wide Clear All: ⌘Z alone, and a stack's ×.
//! - **The keyboard** ([`ShellGlancePanel::key`]): opened by the person, the
//!   panel holds it. The arrows move a focus ring from card to card,
//!   scrolling as they go, Return opens the card, Delete dismisses it and
//!   Esc closes the panel. Opened by a new card it does not take the
//!   keyboard from what the person was typing in; a press in it does, and
//!   a press on a card's own control hands it to that control.
//!
//! It slides in from the right as it opens ([`crate::shell::ui::slide`]),
//! and its column stops above the dock. While it is open, toasts stack left
//! of it. The column is [`PANEL_WIDTH`]
//! wide, as wide as a toast; each card is a tile of the column's inner width
//! at its measured height (glance_card.rs), in the glance order (priority,
//! then recency).
//!
//! **In `style:macos`** it is the Mac's Notification Center
//! ([`ShellGlancePanel::draw_mac_column`]): no panel and no close, only rounded
//! material cards [`MAC_WIDTH`] wide stacked at the screen's right under the
//! menu bar, reaching no further down than they do, so a click below them is
//! a click away. With no card it says "No Notifications" on the desk itself,
//! with no card behind the words ([`ShellGlancePanel::mac_words_on_desk`]).
//! The list runs from the column's top down to the Dock (or the screen's foot)
//! and scrolls by the point under the wheel, as a Mac's does: the cards are
//! cut at its ends, a thin bar at its right edge shows where it is while it
//! moves, and the keyboard's card scrolls into view as little as that takes
//! ([`mac_reveal`]). There are no "more above" or "more below" lines. A notice and a card
//! with nothing live read as a Mac's notification: the app's icon, a bold title,
//! how long ago, and its text ([`mac_plain`]); a live card keeps its content in a
//! Mac's type and controls (glance_card.rs `on_material`), its buttons drawn
//! only under the pointer, on the keyboard's card or with a field in it (in
//! their places, so the card keeps its height). A hovered card shows
//! a close at its top left corner and a press shades it. A click on a
//! notification opens the app that sent it ([`opens_its_app`]); on a live card it
//! opens the card's window, with its content at full size and its conversation.
//! F9 lights no ring until an arrow is pressed, though Return takes the top card.
//! Everything else (the keyboard, undo) is the same.
use crate::desktop::DesktopStyle;
use crate::glance_card::{GlanceTiles, LiveCards};
use crate::octosense::style::AppIconDraw;
use crate::shell::ui::{contains, rect, slide, DrawShellFill, HAlign, Ico, ShellDraw};
use crate::shell::{alpha, MaterialTokens, ShellTokens};
use makepad_widgets::*;
use std::collections::{HashMap, HashSet};

pub const PANEL_WIDTH: f64 = 380.0;
const PAD: f64 = 16.0;
const GAP: f64 = 12.0;
const HEADER: f64 = 64.0;
/// The line "more above" or "more below" takes.
const HINT_H: f64 = 20.0;
/// How long a card that just came wears its accent mark (its toast says
/// the same; the mark shows which card it was).
const NEW_FOR_MS: u64 = 8_000;
/// The card that does not fit under the others peeks in, cut at the list's
/// end, when at least this much of it shows (else the list ends there).
const PEEK_MIN: f64 = 72.0;
/// The column slides in from the right as it opens.
const SLIDE_IN_S: f64 = 0.24;
const SLIDE_IN_PX: f64 = 40.0;
/// The keyboard's focus ring: this far outside its card's tile, round with
/// the tile's corners (the L0 kit's 12 px).
const RING_GAP: f64 = 3.0;
const TILE_RADIUS: f64 = 12.0;

/// The Mac's Notification Center: the column's width, its distance from the
/// screen's right and from the menu bar, the gap between cards, their corner.
pub const MAC_WIDTH: f64 = 360.0;
const MAC_MARGIN: f64 = 8.0;
const MAC_GAP: f64 = 8.0;
const MAC_RADIUS: f64 = 16.0;
/// The line that says there are none.
const MAC_EMPTY_H: f64 = 40.0;
/// Room under the list's last card for the keyboard's ring round it.
const MAC_TAIL: f64 = 4.0;
/// How long the list's scroller shows after the wheel last moved it.
const MAC_THUMB_S: f64 = 1.2;
/// A summary card: its app's icon, its padding, its lines.
const MAC_ICON: f64 = 36.0;
const MAC_PAD: f64 = 12.0;
const MAC_TITLE_LINE: f64 = 17.0;
const MAC_BODY_LINE: f64 = 17.0;
const MAC_BODY_LINES: usize = 3;
/// A hovered card's close: at its top left corner, half over its edge, as a
/// Mac's banner has it.
const MAC_CLOSE: f64 = 22.0;
const MAC_CLOSE_OUT: f64 = 7.0;
/// The keyboard's ring round a card.
const MAC_RING_GAP: f64 = 2.0;
/// A collapsed stack leaves room for its count, action, and two back edges.
const MAC_STACK_DEPTH: f64 = 8.0;
const MAC_GROUP_HEADER: f64 = 30.0;
const MAC_WIDGET_H: f64 = 138.0;
/// A live card's page margin on a material (glance_card.rs `pad_page_x`).
const MAC_PAGE_X: f64 = 14.0;
/// Edit Widgets under the widgets: its row, its capsule's height.
const MAC_EDIT_H: f64 = 26.0;
/// An editing widget's − or +, half over its top left corner.
const MAC_BADGE: f64 = 20.0;
/// The widget gallery Edit Widgets opens, as a Mac's: its widest, its height,
/// its sidebar (the search field and the apps that offer widgets), a sidebar
/// row, its padding, and a widget's preview with its name and line under it.
const MAC_GALLERY_W: f64 = 720.0;
const MAC_GALLERY_H: f64 = 300.0;
const MAC_GALLERY_SIDE: f64 = 176.0;
const MAC_GALLERY_ROW: f64 = 28.0;
const MAC_GALLERY_PAD: f64 = 14.0;
const MAC_GALLERY_TILE_W: f64 = 176.0;
const MAC_GALLERY_TILE_GAP: f64 = 18.0;

/// Whether the Mac's form of the panel is up.
fn mac_style() -> bool {
    crate::shell::menu::MAC_KEYS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Group by the verified publisher, not a title or the app a route opens.
/// The group's place and each member's order follow the retained feed.
#[derive(Clone, Debug)]
enum MacFeedRow {
    Card(usize),
    Group { app: String, members: Vec<usize>, expanded: bool },
    Widget(MacWidget),
    /// Edit Widgets (Done while editing): the column's last row.
    EditWidgets,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MacWidget { Calendar, Clock }

impl MacWidget {
    fn key(self) -> &'static str {
        match self { Self::Calendar => "\0widget:calendar", Self::Clock => "\0widget:clock" }
    }
    fn app(self) -> &'static str {
        match self { Self::Calendar => "calendar", Self::Clock => "clock" }
    }
    /// Its app's name, as the gallery's sidebar lists it.
    fn app_name(self) -> &'static str {
        match self { Self::Calendar => "Calendar", Self::Clock => "Clock" }
    }
    /// What it shows, under its preview in the gallery.
    fn blurb(self) -> &'static str {
        match self { Self::Calendar => "Today\u{2019}s date and what\u{2019}s next.", Self::Clock => "The time in your city." }
    }
    /// Whether the gallery's search finds it: its app's name or what it shows.
    fn matches(self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty() || self.app_name().to_lowercase().contains(&query) || self.blurb().to_lowercase().contains(&query)
    }
    const ALL: [MacWidget; 2] = [Self::Calendar, Self::Clock];
}

/// What a press in the widget gallery is on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum MacGalleryHit {
    /// The sidebar's All Widgets, or one app's row.
    All,
    App(MacWidget),
    /// A widget's preview: it goes into the column.
    Tile(MacWidget),
    Search,
    Done,
}

/// Where the widgets taken out of a Mac's column are kept: their names, one a line.
fn mac_widgets_off_path() -> std::path::PathBuf {
    crate::octosense::paths::home().join("glance-widgets-off")
}

fn mac_widgets_parse(text: &str) -> Vec<MacWidget> {
    MacWidget::ALL.into_iter().filter(|widget| text.lines().any(|line| line.trim() == widget.app())).collect()
}

fn mac_widgets_off_load() -> Vec<MacWidget> {
    std::fs::read_to_string(mac_widgets_off_path()).map(|text| mac_widgets_parse(&text)).unwrap_or_default()
}

fn mac_widgets_off_save(off: &[MacWidget]) {
    let text: String = off.iter().map(|widget| format!("{}\n", widget.app())).collect();
    if let Err(error) = std::fs::create_dir_all(crate::octosense::paths::home()).and_then(|_| std::fs::write(mac_widgets_off_path(), text)) {
        log!("glance panel: could not keep the widgets: {error}");
    }
}

/// One of today's events in the Calendar widget: its title, and when in local time.
#[derive(Clone, Debug, PartialEq)]
struct MacEvent {
    title: String,
    when: String,
}

/// Today's events from `now` on (one under way counts, an all-day one too),
/// soonest first, as Calendar keeps them: its own service's events, which the
/// app shows (the device's calendars are another service's).
#[cfg(any(feature = "app-hub", native_mobile))]
fn mac_events_today(events: &[octosense_calendar_service::Event], now: chrono::NaiveDateTime) -> Vec<MacEvent> {
    use chrono::{TimeZone, Timelike};
    let clock = |t: chrono::NaiveDateTime| t.format("%-I:%M %p").to_string();
    let mut today = Vec::new();
    for event in events {
        let local = |s: &str| -> Option<chrono::NaiveDateTime> {
            let wall = octosense_calendar_service::parse_time(s).ok()?;
            if event.timezone.is_empty() { return Some(wall); }
            let zone: chrono_tz::Tz = event.timezone.parse().ok()?;
            zone.from_local_datetime(&wall).single().map(|t| t.with_timezone(&chrono::Local).naive_local())
        };
        let Some(start) = local(&event.start) else { continue };
        let end = event.end.as_deref().and_then(local);
        if start.date() != now.date() { continue; }
        let all_day = end.is_none() && start.hour() == 0 && start.minute() == 0;
        let when = match end {
            _ if all_day => "All Day".to_string(),
            Some(end) if end < now => continue,
            None if start < now => continue,
            Some(end) if end.date() == start.date() && end > start => format!("{} \u{2013} {}", clock(start), clock(end)),
            _ => clock(start),
        };
        today.push((!all_day, start, MacEvent { title: event.title.clone(), when }));
    }
    // All-day events first, as a Mac lists them, then by when they start.
    today.sort_by_key(|(timed, start, _)| (*timed, *start));
    today.into_iter().map(|(_, _, event)| event).collect()
}

/// The Calendar widget's events: `None` when this build has no Calendar or it
/// isn't installed (the widget names the month then).
fn mac_calendar_events(now: chrono::NaiveDateTime) -> Option<Vec<MacEvent>> {
    #[cfg(any(feature = "app-hub", native_mobile))]
    {
        let host = crate::app_storage::host()?;
        if !crate::host_tools::script_apps::admitted("os.calendar") { return None; }
        Some(mac_events_today(&octosense_calendar_service::load(&host.layout().apps_root().join(".host")), now))
    }
    #[cfg(not(any(feature = "app-hub", native_mobile)))]
    {
        let _ = now;
        None
    }
}

struct MacWidgetContent {
    weekday: String,
    month: String,
    day: String,
    time: String,
    zone: String,
    /// The clock's hands: hours and minutes, local.
    hour: u32,
    minute: u32,
    /// Where the clock is: the city of the system's time zone, or the zone's
    /// abbreviation when it names no city.
    city: String,
    /// Calendar's events still to come today ([`mac_calendar_events`]).
    events: Option<Vec<MacEvent>>,
}

/// The city of the system's time zone, as a Mac's Clock widget names it: the last
/// part of /etc/localtime's zone ("America/Los_Angeles" is Los Angeles).
fn local_city() -> Option<String> {
    let link = std::fs::read_link("/etc/localtime").ok()?;
    let zone = link.to_string_lossy().into_owned();
    let (_, city) = zone.rsplit_once('/')?;
    let city = city.replace('_', " ");
    (!city.is_empty() && city.chars().next().is_some_and(char::is_alphabetic) && !city.starts_with("GMT") && city != "UTC").then_some(city)
}

fn mac_widget_content() -> MacWidgetContent {
    let clock = crate::shell::bar::sample_mac_clock();
    let time = clock.rsplit_once("  ").map(|(_, time)| time).unwrap_or(&clock).trim().to_string();
    MacWidgetContent {
        weekday: crate::shell::bar::local_time(c"%A"),
        month: crate::shell::bar::local_time(c"%B"),
        day: crate::shell::bar::local_time(c"%-d"),
        time,
        zone: crate::shell::bar::local_time(c"%Z"),
        hour: crate::shell::bar::local_time(c"%H").parse().unwrap_or(0),
        minute: crate::shell::bar::local_time(c"%M").parse().unwrap_or(0),
        city: local_city().unwrap_or_else(|| crate::shell::bar::local_time(c"%Z")),
        events: None,
    }
}

fn mac_group_key(app: &str) -> String { format!("\0group:{app}") }

fn mac_app_name(app: &str) -> String {
    #[cfg(any(feature = "app-hub", native_mobile))]
    {
        if let Some(name) = octosense_oauth_service::host::display_name(app) { return name; }
        if app.starts_with("os.") { return crate::glance_notice::app_name(app); }
    }
    if app.starts_with("os.") || !app.contains('.') { crate::approvals::sheet::app_label(app) } else { "App".to_string() }
}

impl MacFeedRow {
    fn key(&self, cards: &[crate::glance::GlanceCard]) -> String {
        match self {
            Self::Card(i) => cards[*i].key(),
            Self::Group { app, .. } => mac_group_key(app),
            Self::Widget(widget) => widget.key().into(),
            Self::EditWidgets => "\0edit-widgets".into(),
        }
    }
    fn shows_card(&self, cards: &[crate::glance::GlanceCard], key: &str) -> bool {
        match self {
            Self::Card(i) => cards[*i].key() == key,
            Self::Group { members, expanded: false, .. } => members.iter().any(|i| cards[*i].key() == key),
            _ => false,
        }
    }
}

fn mac_feed(cards: &[crate::glance::GlanceCard], expanded: &HashSet<String>, widgets: &[MacWidget]) -> Vec<MacFeedRow> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut owners: HashMap<&str, usize> = HashMap::new();
    for (i, card) in cards.iter().enumerate() {
        let at = *owners.entry(card.app.as_str()).or_insert_with(|| { groups.push(Vec::new()); groups.len() - 1 });
        groups[at].push(i);
    }
    let mut rows = Vec::new();
    for members in groups {
        if members.len() == 1 {
            rows.push(MacFeedRow::Card(members[0]));
        } else {
            let app = cards[members[0]].app.clone();
            let open = expanded.contains(&app);
            rows.push(MacFeedRow::Group { app, members: members.clone(), expanded: open });
            if open { rows.extend(members.into_iter().map(MacFeedRow::Card)); }
        }
    }
    rows.extend(widgets.iter().copied().map(MacFeedRow::Widget));
    rows.push(MacFeedRow::EditWidgets);
    rows
}

/// Which side of its row each row's tile takes: two widgets share a row, the
/// first at the left; anything else is the row's alone (0).
fn mac_sides(rows: &[MacFeedRow]) -> Vec<usize> {
    let mut run = 0;
    rows.iter().map(|row| {
        let widget = matches!(row, MacFeedRow::Widget(_));
        let side = if widget { run % 2 } else { 0 };
        run = if widget { run + 1 } else { 0 };
        side
    }).collect()
}

/// Hit regions for a visible group, with the exact keys present when drawn.
#[derive(Clone)]
struct MacGroupHit {
    rect: Rect,
    app: String,
    keys: Vec<String>,
    first: String,
    expanded: bool,
    clear: Rect,
}

/// Local card state saved for one dismissal, never a session or approval.
struct MacUndoState {
    publication: crate::glance::GlanceCard,
    store: octoscript_ui_l0::InstanceStore,
}

impl MacUndoState {
    fn restorable(&self) -> bool {
        let mail_valid = self.publication.l0.as_ref().and_then(|l0| l0.mail.as_ref())
            .is_none_or(|binding| crate::mail_card::account_valid(&binding.account));
        mail_valid && self.publication.account_valid()
            && crate::glance::card(&self.publication.key()).as_ref() == Some(&self.publication)
    }
}

/// What a notice card (glance_notice.rs) begins with.
const NOTICE_LEDGER: &str = "# ledger os.notice@";

/// What a card with nothing live of its own says, as a Mac's notification has
/// it: its title and its text. `None`: it has live content to run.
fn mac_plain(card: &crate::glance::GlanceCard) -> Option<(String, String)> {
    let Some(l0) = &card.l0 else {
        return Some((card.title.clone(), mac_summary_text(card).to_string()));
    };
    if !l0.source.starts_with(NOTICE_LEDGER) {
        return None;
    }
    let note = l0.data.get("note")?;
    Some((note.get("title")?.as_str()?.to_string(), note.get("summary")?.as_str()?.to_string()))
}

/// How long ago, as a Mac's notification says it.
fn mac_age(now_ms: u64, published_ms: u64) -> String {
    let s = now_ms.saturating_sub(published_ms) / 1000;
    match s {
        0..=59 => "now".to_string(),
        60..=3_599 => format!("{}m ago", s / 60),
        3_600..=86_399 => format!("{}h ago", s / 3_600),
        86_400..=172_799 => "Yesterday".to_string(),
        _ => format!("{}d ago", s / 86_400),
    }
}

/// A click on a card, or on its banner, opens the app that sent it on a Mac
/// when the card reads as a notification ([`mac_plain`]). A live card opens its
/// window, the only place its conversation is, and carries its own button to the
/// app; so does a card with a workspace of its own (a script's, Mail's).
pub fn opens_its_app(card: &crate::glance::GlanceCard) -> bool {
    !card.viewport && !card.open_app.is_empty() && card.l0.is_some() && mac_plain(card).is_some()
}

/// What activating the card `key` asks of the shell: its card window, or on a
/// Mac the app that sent it ([`opens_its_app`]).
fn activation(key: &str, how: &str) -> ShellGlancePanelAction {
    if mac_style() {
        if let Some(card) = crate::glance::listed().into_iter().find(|c| c.key() == key).filter(opens_its_app) {
            log!("wm: glance card {key} opens its app{how}");
            crate::glance::opened(key);
            return ShellGlancePanelAction::Open { app: card.open_app, route: card.route };
        }
    }
    log!("wm: glance card {key} opens in the card window{how}");
    ShellGlancePanelAction::OpenCard { key: key.to_string() }
}

/// Where a Mac's card at `r` has its close: half over its top left corner.
fn mac_dismiss_at(r: Rect) -> Rect {
    rect(r.pos.x - MAC_CLOSE_OUT, r.pos.y - MAC_CLOSE_OUT, MAC_CLOSE, MAC_CLOSE)
}

/// Where a card at `r` has its open action (a Mac's card has none: a click
/// on it opens its app).
fn open_at(r: Rect) -> Rect {
    if mac_style() { Rect::default() } else { crate::glance_card::open_button(r) }
}

/// Where a card at `r` has its dismiss action.
fn dismiss_at(r: Rect) -> Rect {
    if mac_style() { mac_dismiss_at(r) } else { crate::glance_card::dismiss_button(r) }
}

/// Where the Mac's list scrolls from `scroll` to show the card that stands
/// `top` down it and is `h` tall, in a view `view` tall: as little as that
/// takes, and to its top when it is taller than the view. The ring round the
/// last card keeps [`MAC_TAIL`] under it.
pub fn mac_reveal(scroll: f64, top: f64, h: f64, view: f64) -> f64 {
    if top < scroll || h + MAC_TAIL > view {
        top
    } else if top + h + MAC_TAIL > scroll + view {
        top + h + MAC_TAIL - view
    } else {
        scroll
    }
}

/// The first card to show so the card at `n` is on screen, scrolling as
/// little as that takes (from the top when it fits there): `heights` are
/// the cards' tile heights and `room` the list's height. A hint line is
/// kept for "more above" when the list does not start at the top, and one
/// for "more below".
pub fn first_to_reveal(heights: &[f64], n: usize, room: f64) -> usize {
    let n = n.min(heights.len().saturating_sub(1));
    for first in 0..=n {
        let above = if first > 0 { HINT_H } else { 0.0 };
        let used: f64 = heights[first..=n].iter().sum::<f64>() + GAP * (n - first) as f64 + above + HINT_H;
        if used <= room {
            return first;
        }
    }
    n
}

/// Where an arrow key moves the keyboard's focus among `len` cards: from
/// the card at `at` (none yet: the first shown, `first`) one down or up,
/// stopping at the ends.
pub fn step_focus(len: usize, at: Option<usize>, first: usize, down: bool) -> usize {
    let last = len.saturating_sub(1);
    match at {
        None => first.min(last),
        Some(i) if down => (i + 1).min(last),
        Some(i) => i.saturating_sub(1).min(last),
    }
}

/// Where the focus goes when the card at `i` of `keys` is dismissed: the
/// card under it takes its place, or the one above when it was the last.
pub fn focus_after_dismiss(keys: &[String], i: usize) -> Option<String> {
    keys.get(i + 1).or(i.checked_sub(1).and_then(|j| keys.get(j))).cloned()
}

/// Where a card taller than its tile sits in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TileScroll {
    /// At its newest rows, its bottom, followed as the card grows: where a
    /// card with an in-card chat starts, so its field and the latest
    /// exchange stay in view.
    Newest,
    /// This far down from its top: where any other card starts (at 0), and
    /// where the wheel leaves a chat scrolled back from its newest rows.
    Top(f64),
}

impl TileScroll {
    /// Where a card starts: one with a chat at its newest rows, any other at
    /// its top.
    pub fn start(chat: bool) -> Self {
        if chat { TileScroll::Newest } else { TileScroll::Top(0.0) }
    }

    /// How far up the card is scrolled in a tile it overflows by `overflow`.
    pub fn offset(self, overflow: f64) -> f64 {
        let overflow = overflow.max(0.0);
        match self {
            TileScroll::Newest => overflow,
            TileScroll::Top(down) => down.clamp(0.0, overflow),
        }
    }

    /// The wheel's `dy` (positive: further down the card) over a card that
    /// overflows its tile by `overflow`: it scrolls as far as it goes, and a
    /// chat back at its bottom follows its newest rows again. False when it
    /// was at that end already: the wheel is the list's.
    pub fn wheel(&mut self, dy: f64, overflow: f64, chat: bool) -> bool {
        let overflow = overflow.max(0.0);
        let at = self.offset(overflow);
        let to = (at + dy).clamp(0.0, overflow);
        if (to - at).abs() < 0.5 {
            return false;
        }
        *self = if chat && to >= overflow - 0.5 { TileScroll::Newest } else { TileScroll::Top(to) };
        true
    }
}

/// A tall card's place in its tile, as the panel keeps it while the card is
/// drawn tall: the publish it is for (a newer one starts over), whether the
/// card has a chat, and where it sits.
#[derive(Clone, Copy, Debug)]
struct TallCard {
    published_ms: u64,
    chat: bool,
    at: TileScroll,
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.ShellGlancePanelBase = #(ShellGlancePanel::register_widget(vm))
    mod.widgets.ShellGlancePanel = set_type_default() do mod.widgets.ShellGlancePanelBase {
        width: Fill
        height: Fill
        draw_bg +: {}
        d +: {}
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum ShellGlancePanelAction {
    /// A card's open action: open this launcher id (and route, when given).
    Open { app: String, route: Option<String> },
    /// A press on a card, outside its own controls: open the card (its
    /// key) in the card window.
    OpenCard { key: String },
    /// The person dismissed cards (a card's close, Delete, Clear all): the
    /// shell offers to undo it.
    Dismissed { count: usize },
    /// ⌘Z while the panel holds the keyboard: the last dismissed come back.
    Undo,
    #[default]
    None,
}

#[derive(Script, ScriptHook, Widget)]
pub struct ShellGlancePanel {
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
    pub open: bool,
    /// How far the column is pushed down (the bar's height).
    #[rust]
    pub bar_clearance: f64,
    #[rust]
    column: Rect,
    /// Each card drawn: its tile, the app it opens (and route), its key.
    #[rust]
    card_rects: Vec<(Rect, String, Option<String>, String)>,
    /// The header's close button and Clear all.
    #[rust]
    close: Rect,
    #[rust]
    clear_all: Rect,
    /// A Mac's focus until an arrow asks to see it: no ring shows it, Return
    /// takes it, and Delete does not act on what cannot be seen.
    #[rust]
    focus_quiet: bool,
    /// The other styles' "more above" and "more below" lines (a Mac's list
    /// has none: it scrolls).
    #[rust]
    more_above: Rect,
    #[rust]
    more_below: Rect,
    /// The Mac's list: how far down it is scrolled and can be, what of it
    /// shows (presses and the cards' controls count only there), when the
    /// wheel last moved it (its scroller shows a while), and what the last
    /// log of its place said.
    #[rust]
    mac_scroll: f64,
    #[rust]
    mac_max_scroll: f64,
    #[rust]
    mac_clip: Rect,
    #[rust]
    scrolled_at: f64,
    #[rust]
    thumb_timer: Timer,
    #[rust]
    scroll_logged: String,
    /// Mac notification stacks are presentation state; their cards stay live.
    #[rust]
    mac_expanded: HashSet<String>,
    #[rust]
    mac_groups: Vec<MacGroupHit>,
    /// The expanded stack whose × is under the pointer: it reads Clear All then.
    #[rust]
    mac_clear_hover: Option<String>,
    #[rust]
    mac_widgets: Vec<(Rect, MacWidget)>,
    #[rust]
    mac_widget_press: Option<MacWidget>,
    #[rust]
    mac_widget_description: String,
    #[rust]
    mac_widget_timer: Timer,
    /// The widgets the person took out (Edit Widgets), kept in the OctoSense
    /// home; whether the column is being edited; where Edit Widgets and the
    /// editing widgets' − and + are, and which a press went down on.
    #[rust]
    mac_widgets_off: Vec<MacWidget>,
    #[rust]
    mac_editing: bool,
    #[rust]
    mac_edit_button: Rect,
    #[rust]
    mac_edit_press: bool,
    #[rust]
    mac_badges: Vec<(Rect, MacWidget)>,
    #[rust]
    mac_badge_press: Option<MacWidget>,
    /// The widget gallery editing opens beside the column, as a Mac's: the
    /// widgets the apps offer, searchable. Where it is, what is typed in its
    /// search (and how much of that the input method may replace), the app
    /// its sidebar shows (None: All Widgets), where its controls are, which
    /// a press went down on, and what its last log said.
    #[rust]
    mac_gallery: Rect,
    #[rust]
    mac_gallery_query: String,
    #[rust]
    mac_gallery_last: usize,
    #[rust]
    mac_gallery_app: Option<MacWidget>,
    #[rust]
    mac_gallery_hits: Vec<(Rect, MacGalleryHit)>,
    #[rust]
    mac_gallery_press: Option<MacGalleryHit>,
    #[rust]
    mac_gallery_logged: String,
    /// Today's events for the Calendar widget, read again each minute: (the minute, them).
    #[rust]
    mac_events: Option<(u64, Option<Vec<MacEvent>>)>,
    #[rust]
    mac_widgets_logged: String,
    #[rust]
    mac_hidden: Vec<String>,
    #[rust]
    mac_consume_up: bool,
    #[rust]
    mac_undo: Vec<MacUndoState>,
    #[rust]
    mac_restore: Vec<MacUndoState>,
    /// The card under the pointer (its actions show), and the card a press
    /// went down on outside its own controls (a release on it opens it).
    #[rust]
    hover: Option<String>,
    #[rust]
    card_press: Option<String>,
    /// That press released on its card: it opens the card window on the next
    /// frame unless the card clicked by then (one of its chips had it).
    #[rust]
    card_release: Option<String>,
    #[rust]
    release_frame: NextFrame,
    /// A new card came: the next draw scrolls as little as shows it.
    #[rust]
    reveal_newest: bool,
    /// When the newest mark goes (a redraw drops it).
    #[rust]
    new_timer: Timer,
    /// The panel holds the keyboard ([`Self::key`]).
    #[rust]
    pub keyboard: bool,
    /// The card the keyboard is on (by key; its ring shows), and the card
    /// the next draw scrolls to (the keyboard moved onto it).
    #[rust]
    focus: Option<String>,
    #[rust]
    reveal: Option<String>,
    /// Opening for a new card ([`Self::show_newest`]): it does not take the
    /// keyboard.
    #[rust]
    quiet: bool,
    /// The slide in: due on the next draw, when it started, its frames.
    #[rust]
    opening: bool,
    #[rust]
    opened_at: f64,
    #[rust]
    slide_frame: NextFrame,
    #[rust]
    sliding: bool,
    /// What the last layout log said, so it is logged once per change.
    #[rust]
    logged: String,
    /// The cards taller than their tiles: where each sits (by key), the ones
    /// this frame drew with how far each overflows, and what the last log of
    /// their places said.
    #[rust]
    tall: HashMap<String, TallCard>,
    #[rust]
    tall_drawn: Vec<(String, f64)>,
    #[rust]
    tall_logged: String,
    #[rust]
    tiles: GlanceTiles,
    /// The L0 cards' live state, by card key, as the card window keeps its
    /// card's (glance_card.rs `LiveCards`): their chips, buttons and field
    /// edits run here as there, their in-card chat (`sys.chat`) with the
    /// publishing app's own agent included. A card's state lasts while the
    /// panel shows it; closing the panel forgets it.
    #[rust]
    live: LiveCards,
    /// The panel's own area: what `redraw` repaints (`draw_bg` draws
    /// nothing).
    #[redraw]
    #[rust]
    area: Area,
    /// The first card shown (the wheel moves it a card at a time), the
    /// wheel's travel not yet turned into a step, and how many cards did
    /// not fit below the last one drawn.
    #[rust]
    first: usize,
    #[rust]
    wheel: f64,
    #[rust]
    below: usize,
    /// A summary card's app icon (the Mac's form).
    #[rust]
    app_icons: AppIconDraw,
}

/// How far the wheel travels for one card.
const WHEEL_STEP: f64 = 60.0;

impl ShellGlancePanel {
    /// Restore only state whose exact publication and account survived Undo.
    fn apply_mac_restore(&mut self) {
        for saved in std::mem::take(&mut self.mac_restore) {
            let key = saved.publication.key();
            if saved.restorable() {
                self.live.prepare_native(&key, &saved.publication);
                if mac_style() {
                    crate::glance_card::as_notice(|| self.live.restore_store(&key, saved.store));
                } else {
                    self.live.restore_store(&key, saved.store);
                }
            }
        }
    }

    /// Snapshot local L0 state before dismissing; only successfully removed
    /// cards belong to the one Undo step. No tile or host authority survives.
    fn dismiss_cards(&mut self, cx: &mut Cx, keys: &[String]) -> usize {
        let snapshots: Vec<MacUndoState> = if mac_style() {
            keys.iter().filter_map(|key| {
                let publication = crate::glance::card(key)?;
                let store = self.live.snapshot_store(key, &publication)?;
                Some(MacUndoState { publication, store })
            }).collect()
        } else { Vec::new() };
        let count = crate::glance::dismiss_all(keys);
        if count > 0 {
            self.mac_undo = snapshots.into_iter().filter(|saved| crate::glance::card(&saved.publication.key()).is_none()).collect();
            // Retire removed isolates and sessions immediately. The saved
            // state is data only; pending actions and authority do not survive.
            let live: Vec<String> = crate::glance::listed().iter().filter(|c| c.l0.is_some()).map(|c| c.key()).collect();
            self.tiles.sweep(cx, &live);
            self.live.retain(&live);
        }
        count
    }

    /// Whether a pointer event at `p` is the panel's: anywhere in the open
    /// column (its cards' or its own) or the widget gallery, or a press
    /// anywhere (outside them, it closes the panel).
    pub fn owns_pointer(&self, p: DVec2, press: bool) -> bool {
        self.open && (press || contains(self.column, p) || (self.mac_editing && contains(self.mac_gallery, p)))
    }
    /// Bring the newest card on screen (a new card opened the panel),
    /// scrolling as little as that takes ([`first_to_reveal`]). Opening so,
    /// the panel leaves the keyboard where it was.
    pub fn show_newest(&mut self) {
        self.reveal_newest = true;
        self.wheel = 0.0;
        self.quiet = true;
    }
    /// Open or close it. Opened by the person it holds the keyboard; opened
    /// for a new card ([`Self::show_newest`]) it does not.
    pub fn set_open(&mut self, open: bool) {
        if open && !self.open {
            self.opening = true;
            self.apply_mac_restore();
            // A Mac's list opens at its top.
            self.mac_scroll = 0.0;
            if mac_style() {
                // F9 chooses its initial focus before the next draw has a
                // chance to update `first`. Do not keep the old scroll's
                // first card, or an old keyboard reveal, on reopening.
                self.first = 0;
                self.reveal = None;
            }
        }
        self.open = open;
        self.keyboard = open && !std::mem::take(&mut self.quiet);
        self.focus = None;
        self.focus_quiet = false;
        self.mac_editing = false;
        self.mac_edit_press = false;
        self.mac_badge_press = None;
        self.mac_gallery_press = None;
        self.mac_gallery_query.clear();
        self.mac_gallery_app = None;
        if open {
            self.mac_widgets_off = mac_widgets_off_load();
            self.mac_events = None;
        } else {
            self.mac_expanded.clear();
            self.mac_consume_up = false;
            self.mac_widget_press = None;
        }
    }
    fn close(&mut self, cx: &mut Cx, why: &str) {
        self.set_open(false);
        log!("wm: glance panel closed ({why})");
        self.redraw(cx);
    }
    /// The cards an undo brought back: the keyboard's focus goes to the
    /// first, when the panel holds the keyboard.
    pub fn focus_restored(&mut self, keys: &[String]) {
        for saved in std::mem::take(&mut self.mac_undo) {
            let key = saved.publication.key();
            if keys.contains(&key) && saved.restorable() {
                self.mac_restore.retain(|old| old.publication.key() != key);
                self.mac_restore.push(saved);
            }
        }
        if self.open { self.apply_mac_restore(); }
        if self.open && self.keyboard {
            if let Some(key) = keys.first() {
                let cards = crate::glance::listed();
                self.focus = if mac_style() {
                    self.mac_rows(&cards).iter().find(|row| row.shows_card(&cards, key)).map(|row| row.key(&cards))
                } else { Some(key.clone()) };
                self.focus_quiet = mac_style();
                self.reveal = self.focus.clone();
            }
        }
    }
    /// Put the keyboard's focus on the first card shown (F9 opened the
    /// panel: the keyboard is already in use).
    pub fn focus_first(&mut self) {
        let cards = crate::glance::listed();
        self.focus = if mac_style() {
            self.mac_rows(&cards).get(self.first).map(|row| row.key(&cards))
        } else { cards.get(self.first).map(|c| c.key()) };
        // A Mac lights no ring until an arrow asks for it.
        self.focus_quiet = mac_style();
    }
    /// The column's rows: the notifications, the widgets in it and Edit
    /// Widgets (not while editing: the gallery has Done then, as a Mac's).
    fn mac_rows(&self, cards: &[crate::glance::GlanceCard]) -> Vec<MacFeedRow> {
        let mut rows = mac_feed(cards, &self.mac_expanded, &self.mac_widgets_shown());
        if self.mac_editing {
            rows.retain(|row| !matches!(row, MacFeedRow::EditWidgets));
        }
        rows
    }
    /// The widgets in the column: one taken out leaves it, and the gallery
    /// offers it again.
    fn mac_widgets_shown(&self) -> Vec<MacWidget> {
        MacWidget::ALL.into_iter().filter(|widget| !self.mac_widgets_off.contains(widget)).collect()
    }
    /// What the gallery offers: the widgets of the app its sidebar shows (all
    /// of them under All Widgets) that its search finds.
    fn mac_gallery_widgets(&self) -> Vec<MacWidget> {
        MacWidget::ALL.into_iter()
            .filter(|widget| self.mac_gallery_app.is_none_or(|app| app == *widget) && widget.matches(&self.mac_gallery_query))
            .collect()
    }
    /// What is typed while the widget gallery is up and the panel holds the
    /// keyboard searches it, as a Mac's gallery search does: true when the
    /// text was the gallery's. An input method's replacement takes back what
    /// it typed last.
    pub fn gallery_text(&mut self, cx: &mut Cx, input: &str, replace_last: bool) -> bool {
        if !(self.open && self.keyboard && self.mac_editing && mac_style()) {
            return false;
        }
        if replace_last {
            for _ in 0..std::mem::take(&mut self.mac_gallery_last) { self.mac_gallery_query.pop(); }
        }
        let typed: String = input.chars().filter(|c| !c.is_control()).collect();
        let room = 40usize.saturating_sub(self.mac_gallery_query.chars().count());
        let typed: String = typed.chars().take(room).collect();
        self.mac_gallery_last = typed.chars().count();
        self.mac_gallery_query.push_str(&typed);
        self.mac_gallery_searched(cx);
        true
    }
    /// The search changed: an app the sidebar showed that has nothing found
    /// gives way to All Widgets, and the ring leaves the column for the search.
    fn mac_gallery_searched(&mut self, cx: &mut Cx) {
        if self.mac_gallery_app.is_some_and(|app| !app.matches(&self.mac_gallery_query)) {
            self.mac_gallery_app = None;
        }
        self.focus_quiet = true;
        log!("glance panel: gallery search \"{}\"", self.mac_gallery_query);
        self.redraw(cx);
    }
    /// A press in the gallery released where it went down.
    fn mac_gallery_choose(&mut self, cx: &mut Cx, hit: MacGalleryHit) {
        match hit {
            MacGalleryHit::All => {
                self.mac_gallery_app = None;
                log!("glance panel: gallery shows All Widgets");
            }
            MacGalleryHit::App(widget) => {
                self.mac_gallery_app = Some(widget);
                log!("glance panel: gallery shows {}", widget.app_name());
            }
            // A widget not in the column goes into it; one there already is.
            MacGalleryHit::Tile(widget) => {
                if self.mac_widgets_off.contains(&widget) {
                    self.mac_toggle_widget(cx, widget);
                }
            }
            MacGalleryHit::Search => {}
            MacGalleryHit::Done => self.mac_set_editing(cx, false),
        }
        self.redraw(cx);
    }
    /// The desktop menu's Edit Widgets…: the open column starts editing its widgets.
    pub fn edit_widgets(&mut self, cx: &mut Cx) {
        if self.open && mac_style() {
            self.mac_set_editing(cx, true);
        }
    }
    /// Edit Widgets, or Done: while editing, the column's widgets wear a −
    /// that takes one out (it leaves the column), no widget opens its app,
    /// and the widget gallery opens beside the column with its search
    /// holding the keyboard; a widget there that is not in the column goes
    /// back in with its +. Done, Esc or closing the column ends it.
    fn mac_set_editing(&mut self, cx: &mut Cx, editing: bool) {
        self.mac_editing = editing;
        self.mac_widget_press = None;
        self.mac_badge_press = None;
        self.mac_gallery_press = None;
        self.mac_gallery_query.clear();
        self.mac_gallery_last = 0;
        self.mac_gallery_app = None;
        // The search takes what is typed; done, the focus is on Edit Widgets again.
        cx.set_key_focus(Area::Empty);
        cx.hide_text_ime();
        self.keyboard = true;
        self.focus = if editing { None } else { Some("\0edit-widgets".into()) };
        self.focus_quiet = true;
        log!("glance panel: {}", if editing { "editing widgets" } else { "done editing widgets" });
        self.redraw(cx);
    }
    /// A press on the column's own controls takes the keyboard (Esc, the
    /// arrows), with no ring until an arrow asks for it.
    fn mac_take_keyboard(&mut self, cx: &mut Cx, focus: String) {
        cx.set_key_focus(Area::Empty);
        cx.hide_text_ime();
        self.keyboard = true;
        self.focus = Some(focus);
        self.focus_quiet = true;
    }
    fn mac_toggle_widget(&mut self, cx: &mut Cx, widget: MacWidget) {
        if let Some(at) = self.mac_widgets_off.iter().position(|off| *off == widget) {
            self.mac_widgets_off.remove(at);
            log!("glance panel: widget {} added", widget.app());
        } else {
            self.mac_widgets_off.push(widget);
            log!("glance panel: widget {} removed", widget.app());
        }
        mac_widgets_off_save(&self.mac_widgets_off);
        self.redraw(cx);
    }
    fn mac_toggle_group(&mut self, cx: &mut Cx, app: &str, expand: bool, by_keyboard: bool) {
        if expand { self.mac_expanded.insert(app.to_string()); } else { self.mac_expanded.remove(app); }
        // An editor in the stack must not keep receiving keys while hidden.
        cx.set_key_focus(Area::Empty);
        cx.hide_text_ime();
        self.keyboard = true;
        self.focus = Some(mac_group_key(app));
        self.focus_quiet = !by_keyboard;
        self.reveal = self.focus.clone();
        log!("glance panel: {} group {app}", if expand { "expanded" } else { "collapsed" });
    }
    fn mac_focus_after_group_clear(&mut self, app: &str, previous: usize) {
        let cards = crate::glance::listed();
        let rows = self.mac_rows(&cards);
        let same = rows.iter().find(|row| match row {
            MacFeedRow::Group { app: owner, .. } => owner == app,
            MacFeedRow::Card(i) => cards[*i].app == app,
            MacFeedRow::Widget(_) | MacFeedRow::EditWidgets => false,
        });
        self.focus = same.or_else(|| rows.get(previous.min(rows.len().saturating_sub(1)))).map(|row| row.key(&cards));
        self.reveal = self.focus.clone();
    }
    /// A key while the panel holds the keyboard: what it asks of the shell
    /// (`Some(None)` when it was the panel's alone), or `None` when it is not
    /// the panel's, for the shell to route on. The arrows move the focus,
    /// Return opens the focused card, Delete (or Backspace) dismisses it, ⌘Z
    /// brings the last dismissed back, Esc closes the panel.
    pub fn key(&mut self, cx: &mut Cx, e: &KeyEvent) -> Option<ShellGlancePanelAction> {
        if !self.open || !self.keyboard {
            return None;
        }
        let m = &e.modifiers;
        let plain = !m.logo && !m.control && !m.alt;
        let cards = crate::glance::listed();
        let mac = mac_style();
        let rows = if mac { self.mac_rows(&cards) } else { Vec::new() };
        let keys: Vec<String> = if mac { rows.iter().map(|row| row.key(&cards)).collect() } else { cards.iter().map(|c| c.key()).collect() };
        let at = self.focus.as_ref().and_then(|f| keys.iter().position(|k| k == f));
        // While the widget gallery is up its search holds the keyboard: Esc
        // clears what is typed there, then ends editing as Done does; Delete
        // takes back a letter, never a notification; a key that types goes to
        // the search as its text (`gallery_text`); Return acts only on a row
        // the ring shows. The arrows still move in the column.
        if mac && self.mac_editing {
            let typing = plain && e.key_code.to_char(false).is_some_and(|c| !c.is_control());
            let done = match e.key_code {
                KeyCode::Escape if plain => {
                    if self.mac_gallery_query.is_empty() {
                        self.mac_set_editing(cx, false);
                    } else {
                        self.mac_gallery_query.clear();
                        self.mac_gallery_last = 0;
                        self.mac_gallery_searched(cx);
                    }
                    true
                }
                KeyCode::Delete | KeyCode::Backspace if plain => {
                    if self.mac_gallery_query.pop().is_some() {
                        self.mac_gallery_last = 0;
                        self.mac_gallery_searched(cx);
                    }
                    true
                }
                KeyCode::ReturnKey | KeyCode::NumpadEnter if plain && (self.focus_quiet || at.is_none()) => true,
                _ => typing,
            };
            if done {
                self.redraw(cx);
                return Some(ShellGlancePanelAction::None);
            }
        }
        let act = match e.key_code {
            KeyCode::Escape if plain => {
                self.close(cx, "Esc");
                ShellGlancePanelAction::None
            }
            KeyCode::ArrowDown | KeyCode::ArrowUp if plain && !keys.is_empty() => {
                // A Mac's first arrow lights the card that has the focus.
                if !(std::mem::take(&mut self.focus_quiet) && at.is_some()) {
                    let n = step_focus(keys.len(), at, self.first, e.key_code == KeyCode::ArrowDown);
                    self.focus = Some(keys[n].clone());
                }
                self.reveal = self.focus.clone();
                ShellGlancePanelAction::None
            }
            KeyCode::ReturnKey | KeyCode::NumpadEnter if plain && at.is_some() => {
                let i = at.unwrap();
                if let Some(MacFeedRow::Group { app, expanded, .. }) = rows.get(i) {
                    self.mac_toggle_group(cx, app, !*expanded, true);
                    ShellGlancePanelAction::None
                } else if let Some(MacFeedRow::EditWidgets) = rows.get(i) {
                    self.mac_set_editing(cx, !self.mac_editing);
                    ShellGlancePanelAction::None
                } else if let (true, Some(MacFeedRow::Widget(widget))) = (self.mac_editing, rows.get(i)) {
                    // Its − takes it out; the ring goes to the widget left, or to the search.
                    self.mac_toggle_widget(cx, *widget);
                    self.focus = self.mac_widgets_shown().first().map(|left| left.key().to_string());
                    self.focus_quiet = self.focus.is_none();
                    ShellGlancePanelAction::None
                } else if let Some(MacFeedRow::Widget(widget)) = rows.get(i) {
                    log!("wm: glance widget {} opens its app (Return)", widget.app());
                    ShellGlancePanelAction::Open { app: widget.app().into(), route: None }
                } else {
                    activation(&keys[i], " (Return)")
                }
            }
            KeyCode::ArrowRight if plain && mac && at.is_some() => {
                let n = at.unwrap();
                match rows.get(n) {
                    Some(MacFeedRow::Group { app, expanded: false, .. }) => self.mac_toggle_group(cx, app, true, true),
                    // The widget beside it, in the same row.
                    Some(MacFeedRow::Widget(_)) if mac_sides(&rows)[n] == 0 && matches!(rows.get(n + 1), Some(MacFeedRow::Widget(_))) => {
                        self.focus = Some(keys[n + 1].clone());
                        self.focus_quiet = false;
                        self.reveal = self.focus.clone();
                    }
                    _ => {}
                }
                ShellGlancePanelAction::None
            }
            KeyCode::ArrowLeft if plain && mac && at.is_some() => {
                let n = at.unwrap();
                if matches!(rows.get(n), Some(MacFeedRow::Widget(_))) && mac_sides(&rows)[n] == 1 {
                    self.focus = Some(keys[n - 1].clone());
                    self.focus_quiet = false;
                    self.reveal = self.focus.clone();
                } else {
                    let app = match rows.get(at.unwrap()) {
                        Some(MacFeedRow::Group { app, expanded: true, .. }) => Some(app.as_str()),
                        Some(MacFeedRow::Card(i)) if rows.iter().any(|row| matches!(row, MacFeedRow::Group { app, expanded: true, .. } if *app == cards[*i].app)) => Some(cards[*i].app.as_str()),
                        _ => None,
                    };
                    if let Some(app) = app { self.mac_toggle_group(cx, app, false, true); }
                }
                ShellGlancePanelAction::None
            }
            // A Mac's Delete waits for a ring to show what it would clear.
            KeyCode::Delete | KeyCode::Backspace if plain && at.is_some() && self.focus_quiet => ShellGlancePanelAction::None,
            KeyCode::Delete | KeyCode::Backspace if plain && at.is_some()
                && matches!(rows.get(at.unwrap()), Some(MacFeedRow::Widget(_) | MacFeedRow::EditWidgets)) => ShellGlancePanelAction::None,
            KeyCode::Delete | KeyCode::Backspace if plain && at.is_some() => {
                let i = at.unwrap();
                let group = match rows.get(i) {
                    Some(MacFeedRow::Group { app, members, .. }) => Some((app, members)),
                    _ => None,
                };
                let targets: Vec<String> = group.map(|(_, members)| members.iter().map(|j| cards[*j].key()).collect()).unwrap_or_else(|| vec![keys[i].clone()]);
                let count = self.dismiss_cards(cx, &targets);
                if let Some((app, _)) = group {
                    log!("wm: glance group {app} dismissed {count} card(s) (Delete)");
                    if count > 0 { self.mac_focus_after_group_clear(app, i); }
                } else {
                    log!("wm: glance card {} dismissed (Delete)", keys[i]);
                    if count > 0 {
                        self.focus = focus_after_dismiss(&keys, i);
                        self.reveal = self.focus.clone();
                    }
                }
                ShellGlancePanelAction::Dismissed { count }
            }
            KeyCode::KeyZ if (m.logo || m.control) && !m.shift && !m.alt => ShellGlancePanelAction::Undo,
            _ => return None,
        };
        self.redraw(cx);
        Some(act)
    }
    /// The card at `p`, by key.
    fn card_at(&self, p: DVec2) -> Option<String> {
        if !self.in_list(p) {
            return None;
        }
        self.card_rects.iter().find(|(r, ..)| contains(*r, p)).map(|(.., key)| key.clone())
    }
    /// Whether `p` is where the Mac's list shows (anywhere in the other styles):
    /// a card cut at the list's end has no press past it.
    fn in_list(&self, p: DVec2) -> bool {
        self.mac_clip.size.y <= 0.0 || contains(self.mac_clip, p)
    }
    /// How far up the tall card `card` is scrolled in its tile, which it
    /// overflows by `overflow`: where it sits, kept from frame to frame for
    /// its publish (first seen: a card with a chat at its newest rows, any
    /// other at its top), and noted as drawn for the wheel.
    fn tall_scroll(&mut self, key: &str, card: &crate::glance::GlanceCard, overflow: f64) -> f64 {
        let fresh = || {
            let chat = card.l0.as_ref().is_some_and(|l0| crate::glance_chat::reads_chat(&l0.source));
            TallCard { published_ms: card.published_ms, chat, at: TileScroll::start(chat) }
        };
        let tall = self.tall.entry(key.to_string()).or_insert_with(fresh);
        if tall.published_ms != card.published_ms {
            *tall = fresh();
        }
        let at = tall.at.offset(overflow);
        self.tall_drawn.push((key.to_string(), overflow));
        at
    }
    /// The wheel's `dy` at `p`: a tall card there scrolls first. False when
    /// none is there, or it is at its end that way: the wheel is the list's.
    fn scroll_tall(&mut self, p: DVec2, dy: f64) -> bool {
        let Some(key) = self.card_at(p) else { return false };
        let Some(overflow) = self.tall_drawn.iter().find(|(k, _)| *k == key).map(|(_, o)| *o) else { return false };
        let Some(tall) = self.tall.get_mut(&key) else { return false };
        let chat = tall.chat;
        tall.at.wheel(dy, overflow, chat)
    }
    pub fn toggle(&mut self, cx: &mut Cx) {
        self.set_open(!self.open);
        self.redraw(cx);
    }
    pub fn set_material(&mut self, m: MaterialTokens, palette: Option<crate::shell::ShellPalette>) {
        self.d.set_material(m);
        self.d.set_palette(palette);
    }
    /// Where each card is, for a test driver.
    pub fn card_rects(&self) -> Vec<(Rect, String)> {
        self.card_rects.iter().map(|(r, app, _, _)| (*r, app.clone())).collect()
    }
    /// A card's material, as a Mac's notifications wear it.
    fn mac_card(&mut self, cx: &mut Cx2d, r: Rect, surface: &crate::shell::SurfaceTokens) {
        if self.d.material().is_glass() {
            self.d.glass_pill(cx, r, MAC_RADIUS, 1.0);
        } else {
            self.d.card(cx, r, surface);
        }
    }
    /// A capsule of the material with a word in it.
    fn mac_plate(&mut self, cx: &mut Cx2d, r: Rect, surface: &crate::shell::SurfaceTokens, ink: Vec4f, text: &str) {
        if self.d.material().is_glass() {
            self.d.glass_pill(cx, r, 6.0, 1.0);
        } else {
            self.d.card(cx, r, surface);
        }
        self.d.label(cx, r, false, 12.0, ink, HAlign::Center, text);
    }
    /// A small round control over a card: the hovered card's close.
    fn mac_round_button(&mut self, cx: &mut Cx2d, r: Rect, ico: Ico, size: f64, ink: Vec4f) {
        let dark = ink.x + ink.y + ink.z > 1.5;
        let (fill, rim) = if dark {
            (vec4(0.27, 0.27, 0.28, 1.0), vec4(1.0, 1.0, 1.0, 0.22))
        } else {
            (vec4(0.97, 0.97, 0.98, 1.0), vec4(0.0, 0.0, 0.0, 0.22))
        };
        let face = rect(r.pos.x + 1.0, r.pos.y + 1.0, r.size.x - 2.0, r.size.y - 2.0);
        let radius = face.size.x * 0.25;
        self.d.ring.new_draw_call(cx);
        self.d.rounded(cx, face, radius, fill);
        self.d.focus_ring(cx, face, radius, 1.0, rim);
        self.d.icon_centered(cx, ico, face, size, alpha(ink, 0.75));
    }
    /// How tall a card with nothing live stands: its title and its text's
    /// lines beside its app's icon.
    fn mac_summary_height(&mut self, cx: &mut Cx2d, text: &str) -> f64 {
        let width = MAC_WIDTH - MAC_PAD * 2.0 - MAC_ICON - 10.0;
        let lines = self.d.wrap(cx, false, 13.0, text, width, MAC_BODY_LINES).len();
        (MAC_TITLE_LINE + lines as f64 * MAC_BODY_LINE).max(MAC_ICON) + MAC_PAD * 2.0
    }
    /// A card with nothing live: its app's icon, its title in bold beside how
    /// long ago, and its text, as a Mac's notification reads.
    fn draw_mac_summary(&mut self, cx: &mut Cx2d, card: &crate::glance::GlanceCard, title: &str, text: &str, r: Rect, ink: Vec4f, dim: Vec4f) {
        let icon = rect(r.pos.x + MAC_PAD, r.pos.y + MAC_PAD, MAC_ICON, MAC_ICON);
        self.app_icons.draw(cx, &card.open_app, DesktopStyle::Macos, icon, 1.0, ink);
        let tx = icon.pos.x + MAC_ICON + 10.0;
        let tw = r.pos.x + r.size.x - MAC_PAD - tx;
        let mut y = r.pos.y + MAC_PAD;
        let age = mac_age(crate::glance::now_ms(), card.published_ms);
        let age_w = self.d.measure(cx, false, 11.0 * self.d.text_scale(), &age) + 2.0;
        self.d.label(cx, rect(tx + tw - age_w, y, age_w, MAC_TITLE_LINE), false, 11.0, dim, HAlign::Right, &age);
        self.d.label_elided(cx, rect(tx, y, tw - age_w - 6.0, MAC_TITLE_LINE), true, 13.0, ink, HAlign::Left, title);
        y += MAC_TITLE_LINE;
        for line in self.d.wrap(cx, false, 13.0, text, tw, MAC_BODY_LINES) {
            self.d.label_elided(cx, rect(tx, y, tw, MAC_BODY_LINE), false, 13.0, dim, HAlign::Left, &line);
            y += MAC_BODY_LINE;
        }
    }
    /// A stack is a summary until expanded; the underlying tiles stay live.
    fn draw_mac_group(&mut self, cx: &mut Cx2d, cards: &[crate::glance::GlanceCard], app: &str,
                      members: &[usize], expanded: bool, r: Rect, surface: &crate::shell::SurfaceTokens,
                      ink: Vec4f, dim: Vec4f, accent: Vec4f) {
        let first = &cards[members[0]];
        let first_key = first.key();
        let keys = members.iter().map(|i| cards[*i].key()).collect();
        let group_key = mac_group_key(app);
        let clear;
        if expanded {
            // As a Mac's stack: its app's name on the desk over the notifications, Show
            // Less, and a × that reads Clear All under the pointer.
            let wide = self.mac_clear_hover.as_deref() == Some(app);
            let clear_w = if wide { self.d.measure(cx, false, 12.0 * self.d.text_scale(), "Clear All") + 18.0 } else { 24.0 };
            let less_w = self.d.measure(cx, false, 12.0 * self.d.text_scale(), "Show Less") + 18.0;
            clear = rect(r.pos.x + r.size.x - clear_w, r.pos.y + 3.0, clear_w, 24.0);
            let less = rect(clear.pos.x - less_w - 6.0, r.pos.y + 3.0, less_w, 24.0);
            self.mac_words_on_desk(cx, rect(r.pos.x + 4.0, r.pos.y + 3.0, less.pos.x - r.pos.x - 10.0, 24.0), ink, &mac_app_name(app), false);
            self.mac_plate(cx, less, surface, ink, "Show Less");
            if wide {
                self.mac_plate(cx, clear, surface, ink, "Clear All");
            } else {
                self.mac_round_button(cx, clear, Ico::Close, 8.0, ink);
            }
        } else {
            let face = rect(r.pos.x, r.pos.y, r.size.x, r.size.y - MAC_STACK_DEPTH);
            self.mac_card(cx, rect(r.pos.x + 6.0, r.pos.y + 8.0, r.size.x - 12.0, face.size.y), surface);
            self.mac_card(cx, rect(r.pos.x + 3.0, r.pos.y + 4.0, r.size.x - 6.0, face.size.y), surface);
            self.mac_card(cx, face, surface);
            // The newest on top, the others' edges under it: a click shows them all.
            let (title, text) = mac_plain(first).unwrap_or_else(|| (first.title.clone(), first.summary.clone()));
            self.draw_mac_summary(cx, first, &title, &text, face, ink, dim);
            clear = mac_dismiss_at(face);
            if self.hover.as_deref() == Some(first_key.as_str()) {
                self.mac_round_button(cx, clear, Ico::Close, 8.0, ink);
            }
            self.card_rects.push((face, first.open_app.clone(), first.route.clone(), first_key.clone()));
        }
        if self.keyboard && !self.focus_quiet && self.focus.as_deref() == Some(group_key.as_str()) {
            let ring = rect(r.pos.x - MAC_RING_GAP, r.pos.y - MAC_RING_GAP, r.size.x + MAC_RING_GAP * 2.0, r.size.y + MAC_RING_GAP * 2.0);
            self.d.ring.new_draw_call(cx);
            self.d.focus_ring(cx, ring, (MAC_RADIUS + MAC_RING_GAP) * 0.5, 2.5, alpha(accent, 0.75));
        }
        self.mac_groups.push(MacGroupHit { rect: r, app: app.to_string(), keys, first: first_key, expanded, clear });
    }

    /// Two local widgets stay below the notifications, as a Mac's small ones: Calendar's
    /// weekday in red over the date and the month, Clock's face with the city under it.
    /// A press opens the app.
    fn draw_mac_widget(&mut self, cx: &mut Cx2d, widget: MacWidget, values: &MacWidgetContent,
                       r: Rect, surface: &crate::shell::SurfaceTokens, ink: Vec4f, dim: Vec4f, accent: Vec4f) {
        self.draw_mac_widget_face(cx, widget, values, r, surface, ink, dim);
        let dark = ink.x + ink.y + ink.z > 1.5;
        if self.mac_editing {
            // Its − takes it out of the column; the gallery offers it again.
            let badge = rect(r.pos.x - 6.0, r.pos.y - 6.0, MAC_BADGE, MAC_BADGE);
            self.mac_badge(cx, badge, true, dark);
            self.mac_badges.push((badge, widget));
        } else if self.mac_widget_press == Some(widget) {
            self.d.ring.new_draw_call(cx);
            self.d.rounded(cx, r, MAC_RADIUS * 0.5, alpha(ink, 0.08));
        }
        if self.keyboard && !self.focus_quiet && self.focus.as_deref() == Some(widget.key()) {
            let ring = rect(r.pos.x - MAC_RING_GAP, r.pos.y - MAC_RING_GAP, r.size.x + MAC_RING_GAP * 2.0, r.size.y + MAC_RING_GAP * 2.0);
            self.d.ring.new_draw_call(cx);
            self.d.focus_ring(cx, ring, (MAC_RADIUS + MAC_RING_GAP) * 0.5, 2.5, alpha(accent, 0.75));
        }
        self.mac_widgets.push((r, widget));
    }
    /// A widget as it shows, in the column or as the gallery's preview of it:
    /// its material and what it says.
    fn draw_mac_widget_face(&mut self, cx: &mut Cx2d, widget: MacWidget, values: &MacWidgetContent,
                            r: Rect, surface: &crate::shell::SurfaceTokens, ink: Vec4f, dim: Vec4f) {
        self.mac_card(cx, r, surface);
        let dark = ink.x + ink.y + ink.z > 1.5;
        match widget {
            MacWidget::Calendar => {
                let red = if dark { vec4(1.0, 0.27, 0.23, 1.0) } else { vec4(1.0, 0.23, 0.19, 1.0) };
                self.d.label_elided(cx, rect(r.pos.x + 14.0, r.pos.y + 13.0, r.size.x - 28.0, 16.0), true, 11.0, red, HAlign::Left, &values.weekday.to_uppercase());
                match values.events.as_deref() {
                    // The day smaller over the next two events, each by a bar of its calendar's blue.
                    Some(events) if !events.is_empty() => {
                        self.d.label_elided(cx, rect(r.pos.x + 13.0, r.pos.y + 27.0, r.size.x - 26.0, 36.0), false, 28.0, ink, HAlign::Left, &values.day);
                        let blue = if dark { vec4(0.04, 0.52, 1.0, 1.0) } else { vec4(0.0, 0.48, 1.0, 1.0) };
                        let mut y = r.pos.y + 66.0;
                        self.d.ring.new_draw_call(cx);
                        for event in events.iter().take(2) {
                            self.d.rounded(cx, rect(r.pos.x + 14.0, y + 1.0, 3.0, 28.0), 0.75, blue);
                            self.d.label_elided(cx, rect(r.pos.x + 23.0, y, r.size.x - 37.0, 15.0), true, 11.0, ink, HAlign::Left, &event.title);
                            self.d.label_elided(cx, rect(r.pos.x + 23.0, y + 15.0, r.size.x - 37.0, 15.0), false, 11.0, dim, HAlign::Left, &event.when);
                            y += 32.0;
                        }
                    }
                    // The day, and that nothing more is on it (the month when there is no Calendar).
                    events => {
                        self.d.label_elided(cx, rect(r.pos.x + 13.0, r.pos.y + 28.0, r.size.x - 26.0, 50.0), false, 42.0, ink, HAlign::Left, &values.day);
                        let foot = if events.is_some() { "No more events today" } else { values.month.as_str() };
                        self.d.label_elided(cx, rect(r.pos.x + 14.0, r.pos.y + r.size.y - 34.0, r.size.x - 28.0, 18.0), false, 12.0, dim, HAlign::Left, foot);
                    }
                }
            }
            MacWidget::Clock => {
                let size = (r.size.y - 46.0).min(r.size.x - 40.0).floor();
                let face = rect((r.pos.x + (r.size.x - size) * 0.5).round(), r.pos.y + 12.0, size, size);
                self.draw_mac_clock_face(cx, face, values.hour, values.minute, ink, dark);
                self.d.label_elided(cx, rect(r.pos.x + 10.0, face.pos.y + size + 7.0, r.size.x - 20.0, 16.0), true, 11.0, dim, HAlign::Center, &values.city);
            }
        }
    }
    /// An editing widget's badge: a − takes it out of the column, a green +
    /// brings it back, as a Mac's.
    fn mac_badge(&mut self, cx: &mut Cx2d, r: Rect, minus: bool, dark: bool) {
        let (fill, rim, mark) = if !minus {
            (vec4(0.2, 0.78, 0.35, 1.0), vec4(0.0, 0.0, 0.0, 0.16), vec4(1.0, 1.0, 1.0, 1.0))
        } else if dark {
            (vec4(0.33, 0.33, 0.35, 1.0), vec4(1.0, 1.0, 1.0, 0.22), vec4(1.0, 1.0, 1.0, 0.9))
        } else {
            (vec4(0.94, 0.94, 0.95, 1.0), vec4(0.0, 0.0, 0.0, 0.22), vec4(0.0, 0.0, 0.0, 0.75))
        };
        self.d.ring.new_draw_call(cx);
        self.d.rounded(cx, r, r.size.x * 0.25, fill);
        self.d.focus_ring(cx, r, r.size.x * 0.25, 0.5, rim);
        let (mid_x, mid_y) = (r.pos.x + r.size.x * 0.5, r.pos.y + r.size.y * 0.5);
        let (long, thick) = ((r.size.x * 0.45).round(), 2.0);
        self.d.rounded(cx, rect(mid_x - long * 0.5, mid_y - thick * 0.5, long, thick), 0.5, mark);
        if !minus {
            self.d.rounded(cx, rect(mid_x - thick * 0.5, mid_y - long * 0.5, thick, long), 0.5, mark);
        }
    }
    /// Edit Widgets under the widgets, a capsule of the material at the
    /// column's middle; Done while editing.
    fn draw_mac_edit_button(&mut self, cx: &mut Cx2d, r: Rect, surface: &crate::shell::SurfaceTokens, ink: Vec4f, accent: Vec4f) {
        let text = if self.mac_editing { "Done" } else { "Edit Widgets" };
        let w = (self.d.measure(cx, false, 12.0 * self.d.text_scale(), text) + 24.0).ceil();
        let button = rect((r.pos.x + (r.size.x - w) * 0.5).round(), r.pos.y, w, MAC_EDIT_H);
        self.mac_plate(cx, button, surface, ink, text);
        if self.mac_edit_press {
            self.d.ring.new_draw_call(cx);
            self.d.rounded(cx, button, 3.0, alpha(ink, 0.1));
        }
        if self.keyboard && !self.focus_quiet && self.focus.as_deref() == Some("\0edit-widgets") {
            let ring = rect(button.pos.x - MAC_RING_GAP, button.pos.y - MAC_RING_GAP, button.size.x + MAC_RING_GAP * 2.0, button.size.y + MAC_RING_GAP * 2.0);
            self.d.ring.new_draw_call(cx);
            self.d.focus_ring(cx, ring, (6.0 + MAC_RING_GAP) * 0.5, 2.5, alpha(accent, 0.75));
        }
        self.mac_edit_button = button;
    }
    /// The widget gallery editing opens, as a Mac's: a sheet of the material
    /// at the foot of the screen beside the column (above the Dock), its
    /// search over a sidebar of All Widgets and the apps that offer widgets,
    /// their widgets as they will show with each one's name and what it
    /// shows, a green + on one not in the column (a check on one there), and
    /// Done. What its search finds narrows both; nothing found says No Results.
    fn draw_mac_gallery(&mut self, cx: &mut Cx2d, screen: Rect, values: &MacWidgetContent,
                        surface: &crate::shell::SurfaceTokens, ink: Vec4f, dim: Vec4f, accent: Vec4f) {
        self.mac_gallery_hits.clear();
        let dark = ink.x + ink.y + ink.z > 1.5;
        // Left of where the column rests (not where its slide is), above the Dock.
        let left = screen.pos.x + MAC_MARGIN * 2.0;
        let right = screen.pos.x + screen.size.x - MAC_MARGIN - MAC_WIDTH - MAC_MARGIN * 3.0;
        let top = screen.pos.y + self.bar_clearance + MAC_MARGIN;
        let mut foot = screen.pos.y + screen.size.y - MAC_MARGIN * 2.0;
        let shelf = crate::desktop::shelf_bounds();
        if shelf.size.y > 0.0 && shelf.pos.y > top {
            foot = foot.min(shelf.pos.y - MAC_MARGIN * 2.0);
        }
        let w = (right - left).min(MAC_GALLERY_W).floor();
        let h = MAC_GALLERY_H.min(foot - top).floor();
        if w < MAC_GALLERY_SIDE + MAC_GALLERY_TILE_W + MAC_GALLERY_PAD * 2.0 || h < MAC_WIDGET_H + 80.0 {
            self.mac_gallery = Rect::default();
            return;
        }
        let g = rect((left + (right - left - w) * 0.5).round(), (foot - h).round(), w, h);
        self.mac_gallery = g;
        let pad = MAC_GALLERY_PAD;
        self.d.ring.new_draw_call(cx);
        if self.d.material().is_glass() {
            self.d.glass_pill(cx, g, MAC_RADIUS + 4.0, 1.0);
        } else {
            self.d.card(cx, g, surface);
        }
        // The search, which has what is typed: a Mac's blue ring while it has
        // the keyboard (no ring lit in the column), and its caret.
        let search = rect(g.pos.x + pad, g.pos.y + pad, MAC_GALLERY_SIDE - pad - 8.0, 26.0);
        let typing = self.keyboard && (self.focus_quiet || self.focus.is_none());
        self.d.ring.new_draw_call(cx);
        self.d.rounded(cx, search, 3.5, if dark { vec4(1.0, 1.0, 1.0, 0.1) } else { vec4(0.0, 0.0, 0.0, 0.06) });
        if typing {
            let ring = rect(search.pos.x - 1.5, search.pos.y - 1.5, search.size.x + 3.0, search.size.y + 3.0);
            self.d.focus_ring(cx, ring, 4.25, 2.5, alpha(accent, 0.6));
        }
        // The sidebar: All Widgets, then each app with a widget the search finds.
        let mut rows = vec![(MacGalleryHit::All, "All Widgets")];
        rows.extend(MacWidget::ALL.into_iter().filter(|widget| widget.matches(&self.mac_gallery_query)).map(|widget| (MacGalleryHit::App(widget), widget.app_name())));
        let mut y = search.pos.y + search.size.y + 10.0;
        let mut row_rects = Vec::with_capacity(rows.len());
        for (hit, _) in &rows {
            let r = rect(g.pos.x + pad - 4.0, y, MAC_GALLERY_SIDE - pad, MAC_GALLERY_ROW);
            let chosen = match hit {
                MacGalleryHit::App(widget) => self.mac_gallery_app == Some(*widget),
                _ => self.mac_gallery_app.is_none(),
            };
            if chosen {
                self.d.rounded(cx, r, 3.0, alpha(ink, if dark { 0.14 } else { 0.1 }));
            }
            row_rects.push(r);
            y += MAC_GALLERY_ROW + 2.0;
        }
        self.d.icon_centered(cx, Ico::Search, rect(search.pos.x + 4.0, search.pos.y, 18.0, search.size.y), 12.0, alpha(ink, 0.6));
        let words = rect(search.pos.x + 24.0, search.pos.y, search.size.x - 30.0, search.size.y);
        if self.mac_gallery_query.is_empty() {
            self.d.label_elided(cx, words, false, 13.0, alpha(ink, 0.6), HAlign::Left, "Search Widgets");
        } else {
            self.d.label_elided(cx, words, false, 13.0, ink, HAlign::Left, &self.mac_gallery_query);
        }
        if typing {
            let typed = if self.mac_gallery_query.is_empty() { 0.0 } else { self.d.measure(cx, false, 13.0 * self.d.text_scale(), &self.mac_gallery_query) + 1.0 };
            self.d.solid(cx, rect(words.pos.x + typed.min(words.size.x), search.pos.y + 6.0, 1.0, search.size.y - 12.0), ink);
        }
        self.mac_gallery_hits.push((search, MacGalleryHit::Search));
        for ((hit, name), r) in rows.iter().zip(&row_rects) {
            let icon = rect(r.pos.x + 6.0, r.pos.y + (r.size.y - 18.0) * 0.5, 18.0, 18.0);
            match hit {
                MacGalleryHit::App(widget) => self.app_icons.draw(cx, widget.app(), DesktopStyle::Macos, icon, 1.0, ink),
                _ => {
                    // All Widgets: four small squares in the accent, as a Mac's sidebar symbol.
                    self.d.ring.new_draw_call(cx);
                    for (dx, dy) in [(0.0, 0.0), (9.0, 0.0), (0.0, 9.0), (9.0, 9.0)] {
                        self.d.rounded(cx, rect(icon.pos.x + 1.0 + dx, icon.pos.y + 1.0 + dy, 7.0, 7.0), 1.0, accent);
                    }
                }
            }
            self.d.label_elided(cx, rect(r.pos.x + 32.0, r.pos.y, r.size.x - 38.0, r.size.y), false, 13.0, ink, HAlign::Left, name);
            self.mac_gallery_hits.push((*r, *hit));
        }
        self.d.solid(cx, rect(g.pos.x + MAC_GALLERY_SIDE, g.pos.y + pad, 1.0, g.size.y - pad * 2.0), alpha(ink, 0.12));
        // The widgets offered: as each will show, its name and what it shows under it.
        let content_x = g.pos.x + MAC_GALLERY_SIDE + pad;
        let content_w = g.pos.x + g.size.x - pad - content_x;
        let heading = self.mac_gallery_app.map_or("All Widgets", |app| app.app_name());
        self.d.label_elided(cx, rect(content_x + 6.0, g.pos.y + pad, content_w - 12.0, 24.0), true, 15.0, ink, HAlign::Left, heading);
        let tiles_top = g.pos.y + pad + 38.0;
        let offered = self.mac_gallery_widgets();
        if offered.is_empty() {
            self.d.label(cx, rect(content_x, tiles_top + MAC_WIDGET_H * 0.5 - 12.0, content_w, 24.0), true, 15.0, dim, HAlign::Center, "No Results");
        }
        let mut x = content_x + 8.0;
        let mut badges = Vec::new();
        for widget in offered {
            let tile = rect(x, tiles_top, MAC_GALLERY_TILE_W, MAC_WIDGET_H);
            if tile.pos.x + tile.size.x > content_x + content_w {
                break;
            }
            self.draw_mac_widget_face(cx, widget, values, tile, surface, ink, dim);
            if self.mac_gallery_press == Some(MacGalleryHit::Tile(widget)) {
                self.d.ring.new_draw_call(cx);
                self.d.rounded(cx, tile, MAC_RADIUS * 0.5, alpha(ink, 0.08));
            }
            self.d.label_elided(cx, rect(tile.pos.x, tile.pos.y + tile.size.y + 8.0, tile.size.x, 16.0), true, 12.0, ink, HAlign::Center, widget.app_name());
            self.d.label_elided(cx, rect(tile.pos.x - 6.0, tile.pos.y + tile.size.y + 25.0, tile.size.x + 12.0, 15.0), false, 11.0, dim, HAlign::Center, widget.blurb());
            self.mac_gallery_hits.push((tile, MacGalleryHit::Tile(widget)));
            badges.push((rect(tile.pos.x - 6.0, tile.pos.y - 6.0, MAC_BADGE, MAC_BADGE), widget));
            x += MAC_GALLERY_TILE_W + MAC_GALLERY_TILE_GAP;
        }
        for (badge, widget) in badges {
            if self.mac_widgets_off.contains(&widget) {
                self.mac_badge(cx, badge, false, dark);
            } else {
                // In the column already: a check where the + would be.
                let (fill, rim) = if dark { (vec4(0.33, 0.33, 0.35, 1.0), vec4(1.0, 1.0, 1.0, 0.22)) } else { (vec4(0.94, 0.94, 0.95, 1.0), vec4(0.0, 0.0, 0.0, 0.22)) };
                self.d.ring.new_draw_call(cx);
                self.d.rounded(cx, badge, badge.size.x * 0.25, fill);
                self.d.focus_ring(cx, badge, badge.size.x * 0.25, 0.5, rim);
                self.d.icon_centered(cx, Ico::Check, badge, 10.0, alpha(ink, 0.8));
            }
            self.mac_gallery_hits.push((badge, MacGalleryHit::Tile(widget)));
        }
        // Done, a Mac's default button: the accent with white words.
        let done_w = (self.d.measure(cx, false, 13.0 * self.d.text_scale(), "Done") + 32.0).ceil();
        let done = rect(g.pos.x + g.size.x - pad - done_w, g.pos.y + g.size.y - pad - 24.0, done_w, 24.0);
        self.d.ring.new_draw_call(cx);
        self.d.rounded(cx, done, 3.0, accent);
        if self.mac_gallery_press == Some(MacGalleryHit::Done) {
            self.d.rounded(cx, done, 3.0, vec4(0.0, 0.0, 0.0, 0.18));
        }
        self.d.label(cx, done, false, 13.0, vec4(1.0, 1.0, 1.0, 1.0), HAlign::Center, "Done");
        self.mac_gallery_hits.push((done, MacGalleryHit::Done));
        // Where its parts landed, once per change: evidence for a remote run.
        let at = |r: &Rect| format!("{},{},{},{}", r.pos.x as i32, r.pos.y as i32, r.size.x as i32, r.size.y as i32);
        let mut parts = vec![format!("frame@{}", at(&g))];
        for (r, hit) in &self.mac_gallery_hits {
            let name = match hit {
                MacGalleryHit::Search => "search".to_string(),
                MacGalleryHit::All => "all".to_string(),
                MacGalleryHit::App(widget) => format!("app_{}", widget.app()),
                MacGalleryHit::Tile(widget) => format!("tile_{}", widget.app()),
                MacGalleryHit::Done => "done".to_string(),
            };
            if !parts.iter().any(|part| part.starts_with(&format!("{name}@"))) {
                parts.push(format!("{name}@{}", at(r)));
            }
        }
        let parts = parts.join(" ");
        if !self.sliding && parts != self.mac_gallery_logged {
            log!("glance panel: gallery {parts}");
            self.mac_gallery_logged = parts;
        }
    }
    /// What the widgets say, for a test driver's /snap: each shown in order
    /// (one taken out says so while editing), "none" with none shown.
    fn mac_describe_widgets(&self, values: &MacWidgetContent) -> String {
        let mut parts: Vec<String> = self.mac_widgets_shown().into_iter().map(|widget| {
            let part = match widget {
                MacWidget::Calendar => {
                    let events = match &values.events {
                        None => String::new(),
                        Some(events) if events.is_empty() => " (No more events today)".to_string(),
                        Some(events) => format!(" ({})", events.iter().map(|e| format!("{} {}", e.title, e.when)).collect::<Vec<_>>().join(", ")),
                    };
                    format!("Calendar {} {} {}{events}", values.weekday, values.month, values.day)
                }
                MacWidget::Clock => format!("Clock {} {}", values.time, values.zone),
            };
            part
        }).collect();
        if parts.is_empty() { parts.push("none".into()); }
        if self.mac_editing {
            parts.push("editing".into());
            // The gallery: what its sidebar shows, what is typed in its search,
            // and the widgets it offers (+ adds one; one in the column says so).
            let offered = self.mac_gallery_widgets();
            let offered = if offered.is_empty() { "No Results".to_string() } else {
                offered.iter().map(|widget| format!("{} {}", widget.app_name(), if self.mac_widgets_off.contains(widget) { "+" } else { "added" }))
                    .collect::<Vec<_>>().join(", ")
            };
            parts.push(format!("gallery {} search \"{}\": {offered}", self.mac_gallery_app.map_or("All Widgets", |app| app.app_name()), self.mac_gallery_query));
        }
        parts.join("; ")
    }
    /// A Mac Clock widget's face: the dial, its twelve marks, the hour and minute
    /// hands from the centre.
    fn draw_mac_clock_face(&mut self, cx: &mut Cx2d, face: Rect, hour: u32, minute: u32, ink: Vec4f, dark: bool) {
        let radius = face.size.x * 0.5;
        let centre = dvec2(face.pos.x + radius, face.pos.y + radius);
        let dial = if dark { vec4(0.11, 0.11, 0.12, 1.0) } else { vec4(1.0, 1.0, 1.0, 1.0) };
        self.d.ring.new_draw_call(cx);
        self.d.rounded(cx, face, radius, dial);
        self.d.focus_ring(cx, face, radius, 0.5, alpha(ink, 0.18));
        let dot = |d: &mut ShellDraw, cx: &mut Cx2d, at: DVec2, size: f64, color: Vec4f| {
            d.rounded(cx, rect(at.x - size * 0.5, at.y - size * 0.5, size, size), size * 0.5, color);
        };
        for mark in 0..12 {
            let a = mark as f64 * std::f64::consts::PI / 6.0;
            let big = mark % 3 == 0;
            let at = dvec2(centre.x + a.sin() * (radius - 7.0), centre.y - a.cos() * (radius - 7.0));
            dot(&mut self.d, cx, at, if big { 3.0 } else { 2.0 }, alpha(ink, if big { 0.85 } else { 0.5 }));
        }
        // Each hand is a run of round dots, as near as the kit draws a line at an angle.
        let hands = [
            (((hour % 12) as f64 + minute as f64 / 60.0) * std::f64::consts::PI / 6.0, radius * 0.5, 3.4),
            (minute as f64 * std::f64::consts::PI / 30.0, radius * 0.78, 2.4),
        ];
        for (a, length, width) in hands {
            let steps = (length * 2.0).ceil() as usize;
            for k in 0..=steps {
                let t = length * k as f64 / steps as f64;
                dot(&mut self.d, cx, dvec2(centre.x + a.sin() * t, centre.y - a.cos() * t), width, ink);
            }
        }
        dot(&mut self.d, cx, centre, 5.0, ink);
    }

    /// The Mac's Notification Center: see this file's header. The list works
    /// as the column of the other styles does (the wheel, the keyboard, a card
    /// that peeks in at the end).
    fn draw_mac_column(&mut self, cx: &mut Cx2d, screen: Rect) {
        let tok = self.d.tokens(self.tokens);
        let surface = &tok.notifications.surface;
        let now_s = cx.seconds_since_app_start();
        if std::mem::take(&mut self.opening) {
            self.opened_at = now_s;
            self.slide_frame = cx.new_next_frame();
        }
        let dx = slide((now_s - self.opened_at) / SLIDE_IN_S, SLIDE_IN_PX);
        self.sliding = dx > 0.0;
        let top = screen.pos.y + self.bar_clearance + MAC_MARGIN;
        let x = screen.pos.x + screen.size.x - MAC_MARGIN - MAC_WIDTH + dx;
        // The cards end above the Dock when the Dock is under them.
        let mut bottom = screen.pos.y + screen.size.y - MAC_MARGIN;
        let shelf = crate::desktop::shelf_bounds();
        if shelf.size.y > 0.0 && shelf.pos.y > top && shelf.pos.x < x + MAC_WIDTH && shelf.pos.x + shelf.size.x > x {
            bottom = bottom.min(shelf.pos.y - MAC_MARGIN);
        }
        let ink = surface.text;
        // Secondary text, faded toward the card: 4.5:1 or more on the material.
        let dim = alpha(ink, 0.72);
        let accent = tok.notifications.countdown;
        let cards = crate::glance::listed();
        let mut values = mac_widget_content();
        // Calendar's events, read again as the minute turns (and on opening).
        let minute = crate::glance::now_ms() / 60_000;
        if self.mac_events.as_ref().map(|(at, _)| *at) != Some(minute) {
            self.mac_events = Some((minute, mac_calendar_events(chrono::Local::now().naive_local())));
        }
        values.events = self.mac_events.as_ref().and_then(|(_, events)| events.clone());
        self.mac_widget_description = self.mac_describe_widgets(&values);
        if self.mac_widget_timer.is_empty() {
            // Wake just after the next minute, and at least every 30 seconds
            // in case the local clock or time zone changes while open.
            let to_minute = (60_000 - crate::glance::now_ms() % 60_000) as f64 / 1000.0;
            self.mac_widget_timer = cx.start_timeout((to_minute + 0.08).min(30.0));
        }
        // A Mac's column has no Clear All of its own: a stack has its ×, a notification its close.
        if cards.is_empty() {
            // Leave the empty notification message above the local widgets.
            let r = rect(x, top, MAC_WIDTH, MAC_EMPTY_H);
            self.mac_words_on_desk(cx, r, ink, "No Notifications", true);
        }
        // The list: from the column's top down to the Dock (or the screen's
        // foot), scrolled by the point.
        let list_top = top + if cards.is_empty() { MAC_EMPTY_H + MAC_GAP } else { 0.0 };
        let view = (bottom - list_top).max(MAC_ICON + MAC_PAD * 2.0);
        let rows = self.mac_rows(&cards);
        let mut heights = Vec::with_capacity(rows.len());
        let plain: Vec<Option<(String, String)>> = cards.iter().map(mac_plain).collect();
        for row in &rows {
            heights.push(match row {
                MacFeedRow::Card(i) => match &plain[*i] {
                    Some((_, text)) => self.mac_summary_height(cx, text),
                    None => crate::glance_card::tile_height(&cards[*i].key()),
                },
                MacFeedRow::Group { expanded: true, .. } => MAC_GROUP_HEADER,
                MacFeedRow::Group { members, expanded: false, .. } => {
                    self.mac_hidden.extend(members.iter().map(|i| cards[*i].key()));
                    let first = &cards[members[0]];
                    let text = plain[members[0]].as_ref().map(|(_, text)| text.as_str()).unwrap_or(first.summary.as_str());
                    self.mac_summary_height(cx, text) + MAC_STACK_DEPTH
                },
                MacFeedRow::Widget(_) => MAC_WIDGET_H,
                MacFeedRow::EditWidgets => MAC_EDIT_H,
            });
        }
        // Each card's place down the list, and how far the list can scroll.
        let sides = mac_sides(&rows);
        let mut tops = Vec::with_capacity(rows.len());
        let mut down = 0.0;
        for (n, (row, h)) in rows.iter().zip(&heights).enumerate() {
            tops.push(down);
            // Two widgets share a row of two equal tiles.
            let shares = matches!(row, MacFeedRow::Widget(_)) && sides[n] == 0 && matches!(rows.get(n + 1), Some(MacFeedRow::Widget(_)));
            if !shares {
                down += h + MAC_GAP;
            }
        }
        let content = down - MAC_GAP + MAC_TAIL;
        self.mac_max_scroll = (content - view).max(0.0);
        if std::mem::take(&mut self.reveal_newest) {
            if let Some(n) = cards.iter().max_by_key(|c| c.published_ms)
                .and_then(|card| rows.iter().position(|row| row.shows_card(&cards, &card.key()))) {
                self.mac_scroll = mac_reveal(self.mac_scroll, tops[n], heights[n], view);
            }
        }
        // The keyboard moved onto a card: scroll as little as shows it.
        if let Some(n) = self.reveal.take().and_then(|key| rows.iter().position(|row| row.key(&cards) == key)) {
            self.mac_scroll = mac_reveal(self.mac_scroll, tops[n], heights[n], view);
        }
        self.mac_scroll = self.mac_scroll.clamp(0.0, self.mac_max_scroll);
        let scroll = self.mac_scroll;
        // The first card in view: where the arrows start.
        self.first = tops.iter().zip(&heights).position(|(t, h)| t + h > scroll).unwrap_or(0);
        // What shows of the list: the cards cut at its ends, with room above
        // the first for a hovered card's close and the keyboard's ring.
        self.mac_clip = rect(x - MAC_MARGIN, list_top - MAC_GAP, MAC_WIDTH + MAC_MARGIN * 2.0, bottom - list_top + MAC_GAP);
        cx.push_clip_rect(self.mac_clip);
        let mut below = 0;
        for (i, row) in rows.iter().enumerate() {
            let h = heights[i];
            let y = list_top + tops[i] - scroll;
            if y >= bottom {
                below += 1;
                continue;
            }
            if y + h <= self.mac_clip.pos.y {
                continue;
            }
            let r = rect(x, y, MAC_WIDTH, h);
            let index = match row {
                MacFeedRow::Card(index) => *index,
                MacFeedRow::Group { app, members, expanded } => {
                    self.draw_mac_group(cx, &cards, app, members, *expanded, r, surface, ink, dim, accent);
                    continue;
                }
                MacFeedRow::Widget(widget) => {
                    let width = (MAC_WIDTH - MAC_GAP) / 2.0;
                    let left = if sides[i] == 0 { x } else { x + width + MAC_GAP };
                    self.draw_mac_widget(cx, *widget, &values, rect(left, y, width, h), surface, ink, dim, accent);
                    continue;
                }
                MacFeedRow::EditWidgets => {
                    self.draw_mac_edit_button(cx, r, surface, ink, accent);
                    continue;
                }
            };
            let card = &cards[index];
            let key = card.key();
            let overflow = if plain[index].is_some() { 0.0 } else { crate::glance_card::overflow(&key) };
            let inner = if overflow > 0.0 { self.tall_scroll(&key, card, overflow) } else { 0.0 };
            self.mac_card(cx, r, surface);
            if let Some((title, text)) = &plain[index] {
                self.draw_mac_summary(cx, card, title, text, r, ink, dim);
            } else {
                // A Mac notification's buttons show under the pointer, on the
                // keyboard's card and while it is pressed; a card with a field
                // keeps them, with what is typed there.
                let shows_buttons = self.hover.as_deref() == Some(key.as_str())
                    || (self.keyboard && !self.focus_quiet && self.focus.as_deref() == Some(key.as_str()))
                    || self.card_press.as_deref() == Some(key.as_str());
                let live = &mut self.live;
                let body = crate::glance_card::as_notice(|| {
                    let body = live.body(&key, card, "glance panel");
                    if shows_buttons || body.contains("TextInput") { body } else { live.quiet_body(&key, card, &body) }
                });
                // As every notification: its app's icon at the left, what it says beside it,
                // lined up with a notification's words (the card's page has its own margin).
                let icon = rect(r.pos.x + MAC_PAD, r.pos.y + MAC_PAD, MAC_ICON, MAC_ICON);
                let name = if card.open_app.is_empty() { crate::approvals::mac_kit::icon_name(&card.app) } else { card.open_app.clone() };
                self.app_icons.draw(cx, &name, DesktopStyle::Macos, icon, 1.0, ink);
                let beside = MAC_PAD + MAC_ICON + 10.0 - MAC_PAGE_X;
                self.tiles.draw_scrolled(cx, &key, &card.app, card.contained, &body, rect(r.pos.x + beside, r.pos.y, r.size.x - beside, r.size.y), inner);
            }
            if overflow > 0.0 {
                let track = rect(r.pos.x + r.size.x - 6.0, r.pos.y + 40.0, 3.0, (r.size.y - 50.0).max(12.0));
                let thumb = (track.size.y * r.size.y / (r.size.y + overflow)).clamp(12.0, track.size.y);
                self.d.ring.new_draw_call(cx);
                self.d.rounded(cx, rect(track.pos.x, track.pos.y + (track.size.y - thumb) * inner / overflow, track.size.x, thumb), 1.5, alpha(ink, 0.35));
            }
            // Pressed, it shades, on its way to opening.
            if self.card_press.as_deref() == Some(key.as_str()) {
                self.d.ring.new_draw_call(cx);
                self.d.rounded(cx, r, MAC_RADIUS * 0.5, alpha(ink, 0.08));
            }
            // The keyboard's card: a ring round it.
            if self.keyboard && !self.focus_quiet && self.focus.as_deref() == Some(key.as_str()) {
                let ring = rect(r.pos.x - MAC_RING_GAP, r.pos.y - MAC_RING_GAP, r.size.x + MAC_RING_GAP * 2.0, r.size.y + MAC_RING_GAP * 2.0);
                self.d.ring.new_draw_call(cx);
                self.d.focus_ring(cx, ring, (MAC_RADIUS + MAC_RING_GAP) * 0.5, 2.5, alpha(accent, 0.75));
            }
            if self.hover.as_deref() == Some(key.as_str()) {
                self.mac_round_button(cx, dismiss_at(r), Ico::Close, 8.0, ink);
            }
            self.card_rects.push((r, card.open_app.clone(), card.route.clone(), key));
        }
        cx.pop_clip_rect();
        self.below = below;
        // Where the list is, at its right edge while it moves: a Mac's
        // overlay scroller.
        if self.mac_max_scroll > 0.0 && now_s - self.scrolled_at < MAC_THUMB_S {
            let track = rect(x + MAC_WIDTH + 2.0, list_top + 2.0, 3.0, view - 4.0);
            let thumb = (track.size.y * view / (view + self.mac_max_scroll)).clamp(24.0, track.size.y);
            let at = track.pos.y + (track.size.y - thumb) * scroll / self.mac_max_scroll;
            self.d.ring.new_draw_call(cx);
            self.d.rounded(cx, rect(track.pos.x, at, track.size.x, thumb), 1.5, alpha(ink, 0.45));
        }
        if !self.sliding {
            let place = format!("{scroll:.0} of {:.0}", self.mac_max_scroll);
            if place != self.scroll_logged {
                log!("glance panel: list at {place}");
                self.scroll_logged = place;
            }
        }
        // As far as the cards reach (the list's foot when they go on), and the
        // hovered close's overhang.
        let end = (list_top + content - MAC_TAIL - scroll).min(bottom);
        self.column = rect(x - MAC_MARGIN, top - MAC_MARGIN, MAC_WIDTH + MAC_MARGIN, end - top + MAC_MARGIN);
        if self.mac_editing {
            self.draw_mac_gallery(cx, screen, &values, surface, ink, dim, accent);
        } else {
            self.mac_gallery = Rect::default();
            self.mac_gallery_hits.clear();
            self.mac_gallery_logged.clear();
        }
        self.draw_bg.color = Vec4f::default();
        self.draw_bg.draw_abs(cx, self.column);
    }

    /// Words straight on the desk, as a Mac's Notification Center says it has
    /// nothing and names a stack's app: in the material's ink over a soft halo of
    /// the other tone, so they read on any wallpaper with no card behind them.
    /// `centred` in `r`, or from its left.
    fn mac_words_on_desk(&mut self, cx: &mut Cx2d, r: Rect, ink: Vec4f, text: &str, centred: bool) {
        // At a whole point, as centring alone can leave a thin letter's stem between pixels.
        let w = self.d.measure(cx, true, 15.0 * self.d.text_scale(), text).min(r.size.x);
        let x = if centred { r.pos.x + (r.size.x - w) * 0.5 } else { r.pos.x };
        let r = rect(x.round(), r.pos.y.round(), w.ceil() + 1.0, r.size.y);
        let light_ink = ink.x + ink.y + ink.z > 1.5;
        let halo = if light_ink { vec4(0.0, 0.0, 0.0, 1.0) } else { vec4(1.0, 1.0, 1.0, 1.0) };
        for (radius, strength) in [(2.0, 0.22), (1.0, 0.5)] {
            for k in 0..8 {
                let a = k as f64 * std::f64::consts::FRAC_PI_4;
                let at = rect(r.pos.x + radius * a.cos(), r.pos.y + radius * a.sin(), r.size.x, r.size.y);
                self.d.label(cx, at, true, 15.0, alpha(halo, strength), HAlign::Left, text);
            }
        }
        self.d.label(cx, r, true, 15.0, ink, HAlign::Left, text);
    }
}

/// What a card with no live content of its own says.
fn mac_summary_text(card: &crate::glance::GlanceCard) -> &str {
    if card.summary.is_empty() { "Click to open." } else { &card.summary }
}

impl Widget for ShellGlancePanel {
    /// What the Mac's column shows, for a test driver's /snap.
    fn text(&self) -> String {
        if !self.open || !mac_style() {
            return String::new();
        }
        if crate::glance::listed().is_empty() {
            return format!("No Notifications; widgets: {}", self.mac_widget_description);
        }
        let keys: Vec<&str> = self.card_rects.iter().map(|(.., key)| key.as_str()).collect();
        let groups: Vec<String> = self.mac_groups.iter().map(|group|
            format!("{}={}:{}", group.app, group.keys.len(), if group.expanded { "expanded" } else { "collapsed" })).collect();
        let suffix = if groups.is_empty() { String::new() } else { format!("; groups: {}", groups.join(", ")) };
        format!("{} notification(s): {}{suffix}; widgets: {}", keys.len(), keys.join(", "), self.mac_widget_description)
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let screen = cx.turtle().rect();
        self.card_rects.clear();
        self.tall_drawn.clear();
        self.mac_groups.clear();
        self.mac_widgets.clear();
        self.mac_badges.clear();
        self.mac_edit_button = Rect::default();
        self.mac_hidden.clear();
        self.close = Rect::default();
        // Every frame, open or closed: the kit keeps its overlay in tree order.
        self.d.begin_surface(cx);
        self.clear_all = Rect::default();
        self.more_above = Rect::default();
        self.more_below = Rect::default();
        self.mac_clip = Rect::default();
        if self.open && mac_style() {
            self.draw_mac_column(cx, screen);
        } else if self.open {
            let tok = self.d.tokens(self.tokens);
            // Sliding in, from the right.
            let now_s = cx.seconds_since_app_start();
            if std::mem::take(&mut self.opening) {
                self.opened_at = now_s;
                self.slide_frame = cx.new_next_frame();
            }
            let top = screen.pos.y + self.bar_clearance + tok.spacing.gaps_out;
            let dx = slide((now_s - self.opened_at) / SLIDE_IN_S, SLIDE_IN_PX);
            self.sliding = dx > 0.0;
            let x = screen.pos.x + screen.size.x - tok.spacing.gaps_out - PANEL_WIDTH + dx;
            // The column ends above the dock when the dock is under it.
            let mut end = screen.pos.y + screen.size.y - tok.spacing.gaps_out;
            let shelf = crate::desktop::shelf_bounds();
            if shelf.size.y > 0.0 && shelf.pos.y > top && shelf.pos.x < x + PANEL_WIDTH && shelf.pos.x + shelf.size.x > x {
                end = end.min(shelf.pos.y - tok.spacing.gaps_out);
            }
            let column = rect(x, top, PANEL_WIDTH, (end - top).max(HEADER));
            self.column = column;
            self.d.card(cx, column, &tok.notifications.surface);
            let ink = tok.notifications.surface.text;
            // Secondary text, faded toward the column: 4.5:1 or more on a
            // flat one, light or dark.
            let dim = alpha(ink, 0.72);
            let accent = tok.notifications.countdown;
            let cards = crate::glance::listed();
            // The header: the title and how many; Clear all and close.
            self.close = rect(x + PANEL_WIDTH - PAD - 28.0, top + 12.0, 28.0, 28.0);
            self.d.icon_centered(cx, Ico::Close, self.close, 14.0, alpha(ink, 0.8));
            let mut title_w = PANEL_WIDTH - PAD * 2.0 - 32.0;
            if !cards.is_empty() {
                let w = self.d.measure(cx, false, 13.0 * self.d.text_scale(), "Clear all") + 16.0;
                self.clear_all = rect(self.close.pos.x - 6.0 - w, top + 14.0, w, 24.0);
                self.d.label(cx, self.clear_all, false, 13.0, accent, HAlign::Center, "Clear all");
                title_w = self.clear_all.pos.x - x - PAD - 8.0;
            }
            self.d.label_elided(cx, rect(x + PAD, top + 14.0, title_w, 24.0), true, 18.0, ink, HAlign::Left, "At a glance");
            let count = match cards.len() {
                0 => "Nothing new".to_string(),
                1 => "1 card from your apps".to_string(),
                n => format!("{n} cards from your apps"),
            };
            self.d.label_elided(cx, rect(x + PAD, top + 38.0, PANEL_WIDTH - PAD * 2.0, 18.0), false, 12.0, dim, HAlign::Left, &count);
            let list_top = top + HEADER;
            let bottom = column.pos.y + column.size.y - PAD;
            if cards.is_empty() {
                // Nothing to show: say so, in the middle of the column.
                let mid = list_top + (bottom - list_top) * 0.38;
                self.d.icon_centered(cx, Ico::Check, rect(x, mid - 44.0, PANEL_WIDTH, 32.0), 26.0, dim);
                self.d.label(cx, rect(x + PAD, mid, PANEL_WIDTH - PAD * 2.0, 22.0), true, 15.0, ink, HAlign::Center, "You\u{2019}re all caught up");
                self.d.label(cx, rect(x + PAD, mid + 24.0, PANEL_WIDTH - PAD * 2.0, 18.0), false, 12.0, dim, HAlign::Center, "Cards from your apps appear here.");
            } else {
                let heights: Vec<f64> = cards.iter().map(|c| if c.l0.is_none() {crate::glance_card::TILE_DEFAULT_HEIGHT} else {crate::glance_card::tile_height(&c.key())}).collect();
                if std::mem::take(&mut self.reveal_newest) {
                    if let Some(n) = cards.iter().enumerate().max_by_key(|(_, c)| c.published_ms).map(|(i, _)| i) {
                        self.first = first_to_reveal(&heights, n, bottom - list_top);
                    }
                }
                // The keyboard moved onto a card: scroll as little as shows it.
                if let Some(n) = self.reveal.take().and_then(|key| cards.iter().position(|c| c.key() == key)) {
                    self.first = if n < self.first { n } else { self.first.max(first_to_reveal(&heights, n, bottom - list_top)) };
                }
                self.first = self.first.min(cards.len() - 1);
                let mut y = list_top;
                if self.first > 0 {
                    let more = format!("\u{2191} {} more above", self.first);
                    self.d.label(cx, rect(x + PAD, y - 2.0, PANEL_WIDTH - PAD * 2.0, HINT_H - 4.0), false, 12.0, dim, HAlign::Center, &more);
                    y += HINT_H;
                }
                let limit = bottom - HINT_H;
                let now = crate::glance::now_ms();
                let mut mark_left: Option<u64> = None;
                let mut drawn = 0;
                for (i, card) in cards.iter().enumerate().skip(self.first) {
                    let key = card.key();
                    let mut h = heights[i];
                    // The first card shown always draws (clipped by its cap).
                    // The one that does not fit under the others peeks in,
                    // cut at the list's end (its tile clips), so the list
                    // reads as going on; it still counts as below.
                    let cut = drawn > 0 && y + h > limit;
                    if cut {
                        if limit - y < PEEK_MIN {
                            break;
                        }
                        h = limit - y;
                    } else {
                        drawn += 1;
                    }
                    let r = rect(x + PAD, y, PANEL_WIDTH - PAD * 2.0, h);
                    // A card taller than its tile scrolls inside it (one that
                    // peeks in shows its top), with a thumb at its edge, under
                    // the card's actions.
                    let overflow = if cut || card.l0.is_none() { 0.0 } else { crate::glance_card::overflow(&key) };
                    let scroll = if overflow > 0.0 { self.tall_scroll(&key, card, overflow) } else { 0.0 };
                    if card.l0.is_none() {
                        // Match the phone's summary-first presentation. A full
                        // script workspace cannot fit inside a feed tile, and
                        // should not start its timers or requests while idle.
                        self.d.card(cx, r, &tok.notifications.surface);
                        cx.begin_turtle(Walk::abs_rect(r), Layout::default());
                        self.d.label_elided(cx, rect(r.pos.x + 14.0, r.pos.y + 14.0, r.size.x - 70.0, 22.0), true, 14.0, ink, HAlign::Left, &card.title);
                        let summary = if card.summary.is_empty() {"Open to view this card"} else {&card.summary};
                        for (n, line) in self.d.wrap(cx, false, 13.0, summary, r.size.x - 28.0, 3).iter().enumerate() {
                            self.d.label(cx, rect(r.pos.x + 14.0, r.pos.y + 48.0 + n as f64 * 20.0, r.size.x - 28.0, 20.0), false, 13.0, dim, HAlign::Left, line);
                        }
                        cx.end_turtle();
                    } else {
                        let body = self.live.body(&key, card, "glance panel");
                        self.tiles.draw_scrolled(cx, &key, &card.app, card.contained, &body, r, scroll);
                    }
                    if overflow > 0.0 {
                        let track = rect(r.pos.x + r.size.x - 6.0, r.pos.y + 40.0, 3.0, (r.size.y - 50.0).max(12.0));
                        let thumb = (track.size.y * r.size.y / (r.size.y + overflow)).clamp(12.0, track.size.y);
                        self.d.solid(cx, rect(track.pos.x, track.pos.y + (track.size.y - thumb) * scroll / overflow, track.size.x, thumb), alpha(ink, 0.35));
                    }
                    // A card that just came: an accent mark in the gutter
                    // beside it for a few seconds, the one its toast is about.
                    let age = now.saturating_sub(card.published_ms);
                    if age < NEW_FOR_MS {
                        self.d.solid(cx, rect(r.pos.x - 9.0, r.pos.y + 14.0, 3.0, (r.size.y - 28.0).max(8.0)), accent);
                        let left = NEW_FOR_MS - age;
                        mark_left = Some(mark_left.map_or(left, |m| m.min(left)));
                    }
                    // The keyboard's card: a ring round it, and its actions.
                    let focused = self.keyboard && self.focus.as_deref() == Some(key.as_str());
                    if focused {
                        let ring = rect(r.pos.x - RING_GAP, r.pos.y - RING_GAP, r.size.x + RING_GAP * 2.0, r.size.y + RING_GAP * 2.0);
                        self.d.focus_ring(cx, ring, TILE_RADIUS + RING_GAP, 2.0, accent);
                    }
                    // The hovered card's actions, on a backdrop so they read
                    // over its content: dismiss, and open its app.
                    if focused || self.hover.as_deref() == Some(key.as_str()) {
                        let open = crate::glance_card::open_button(r);
                        let dismiss = crate::glance_card::dismiss_button(r);
                        let backdrop = rect(dismiss.pos.x - 4.0, dismiss.pos.y - 4.0, open.pos.x + open.size.x - dismiss.pos.x + 8.0, open.size.y + 8.0);
                        self.d.card(cx, backdrop, &tok.notifications.surface);
                        self.d.icon_centered(cx, Ico::Close, dismiss, 12.0, ink);
                        self.d.icon_centered(cx, Ico::ChevronRight, open, 14.0, ink);
                    }
                    self.card_rects.push((r, card.open_app.clone(), card.route.clone(), key));
                    y += h + GAP;
                    if cut {
                        break;
                    }
                }
                if let Some(left) = mark_left {
                    if self.new_timer.is_empty() {
                        self.new_timer = cx.start_timeout(left as f64 / 1000.0 + 0.05);
                    }
                }
                // The cards the column does not show: the wheel brings them.
                self.below = cards.len().saturating_sub(self.first + drawn);
                if self.below > 0 {
                    let more = format!("{} more below \u{2193}", self.below);
                    self.d.label(cx, rect(x + PAD, bottom - HINT_H + 2.0, PANEL_WIDTH - PAD * 2.0, HINT_H - 4.0), false, 12.0, dim, HAlign::Center, &more);
                }
            }
        }
        self.d.end_surface(cx);
        // The toasts stack clear of the open column (notifications.rs).
        crate::shell::notifications::keep_clear_of(self.open.then_some(self.column));
        // Where the cards landed, once per change and once the slide is
        // done: evidence for a remote run.
        let layout: Vec<String> = self.card_rects.iter().map(|(r, app, _, _)| format!("{app}@{},{},{},{}", r.pos.x as i32, r.pos.y as i32, r.size.x as i32, r.size.y as i32)).collect();
        let layout = layout.join(" ");
        if self.open && !self.sliding && layout != self.logged {
            log!("glance panel: {} card(s) {}", self.card_rects.len(), layout);
            self.logged = layout;
        }
        // Where the widgets and Edit Widgets landed, the same way.
        let at = |r: Rect| format!("{},{},{},{}", r.pos.x as i32, r.pos.y as i32, r.size.x as i32, r.size.y as i32);
        let mut widgets: Vec<String> = self.mac_widgets.iter().map(|(r, widget)| format!("{}@{}", widget.app(), at(*r))).collect();
        if self.mac_edit_button.size.x > 0.0 { widgets.push(format!("edit@{}", at(self.mac_edit_button))); }
        let widgets = widgets.join(" ");
        if self.open && !self.sliding && mac_style() && widgets != self.mac_widgets_logged {
            log!("glance panel: widgets {widgets}");
            self.mac_widgets_logged = widgets;
        }
        // Where the tall cards sit, once per change: evidence for a remote run.
        let places: Vec<String> = self
            .tall_drawn
            .iter()
            .filter_map(|(key, overflow)| {
                let tall = self.tall.get(key)?;
                let newest = if tall.at == TileScroll::Newest { " newest" } else { "" };
                Some(format!("{key} {:.0}/{:.0}{newest}", tall.at.offset(*overflow), overflow))
            })
            .collect();
        let places = places.join(", ");
        if self.open && !self.sliding && places != self.tall_logged {
            if !places.is_empty() {
                log!("glance panel: scrolled {places}");
            }
            self.tall_logged = places;
        }
        let live: Vec<String> = if self.open { crate::glance::listed().iter().filter(|c| c.l0.is_some()).map(|c| c.key()).collect() } else { Vec::new() };
        self.tiles.sweep(cx, &live);
        self.live.retain(&live);
        // A card that is not tall now (it fits, peeks in or is scrolled out
        // of the list) starts over when it is again.
        let tall_drawn = &self.tall_drawn;
        let hidden = &self.mac_hidden;
        self.tall.retain(|key, _| tall_drawn.iter().any(|(k, _)| k == key) || hidden.contains(key));
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        // The slide in's frames.
        if self.slide_frame.is_event(event).is_some() {
            if cx.seconds_since_app_start() - self.opened_at < SLIDE_IN_S {
                self.slide_frame = cx.new_next_frame();
            }
            self.redraw(cx);
        }
        // A newest mark's time is up.
        if self.new_timer.is_event(event).is_some() {
            self.new_timer = Timer::empty();
            self.redraw(cx);
        }
        // The Mac list's scroller has shown long enough.
        if self.thumb_timer.is_event(event).is_some() {
            self.thumb_timer = Timer::empty();
            self.redraw(cx);
        }
        if self.mac_widget_timer.is_event(event).is_some() {
            self.mac_widget_timer = Timer::empty();
            if self.open && mac_style() { self.redraw(cx); }
        }
        if let Event::MouseUp(e) = event {
            if let Some(hit) = self.mac_gallery_press.take() {
                if self.open && mac_style() && self.mac_editing && self.mac_gallery_hits.iter().any(|(r, shown)| *shown == hit && contains(*r, e.abs)) {
                    self.mac_gallery_choose(cx, hit);
                }
                self.redraw(cx);
                return;
            }
            if let Some(widget) = self.mac_badge_press.take() {
                if self.open && mac_style() && self.mac_editing && self.mac_badges.iter().any(|(r, shown)| *shown == widget && contains(*r, e.abs)) {
                    self.mac_toggle_widget(cx, widget);
                }
                return;
            }
            if std::mem::take(&mut self.mac_edit_press) {
                if self.open && mac_style() && contains(self.mac_edit_button, e.abs) {
                    self.mac_set_editing(cx, !self.mac_editing);
                }
                self.redraw(cx);
                return;
            }
            if let Some(widget) = self.mac_widget_press.take() {
                if self.open && mac_style() && self.in_list(e.abs)
                    && self.mac_widgets.iter().any(|(r, visible)| *visible == widget && contains(*r, e.abs)) {
                    log!("wm: glance widget {} opens its app (pointer)", widget.app());
                    cx.widget_action(self.uid, ShellGlancePanelAction::Open { app: widget.app().into(), route: None });
                }
                self.redraw(cx);
                return;
            }
        }
        if matches!(event, Event::MouseUp(_)) && std::mem::take(&mut self.mac_consume_up) {
            return;
        }
        // The wheel over the column: a card at a time, after a tall card
        // under it has scrolled to its end that way.
        if let Event::Scroll(e) = event {
            if self.open && contains(self.column, e.abs) {
                if self.scroll_tall(e.abs, e.scroll.y) {
                    self.wheel = 0.0;
                    self.redraw(cx);
                    return;
                }
                // A Mac's list scrolls by the point, and shows its scroller.
                if mac_style() {
                    let to = (self.mac_scroll + e.scroll.y).clamp(0.0, self.mac_max_scroll);
                    if (to - self.mac_scroll).abs() > 0.1 {
                        self.mac_scroll = to;
                        self.scrolled_at = cx.seconds_since_app_start();
                        self.thumb_timer = cx.start_timeout(MAC_THUMB_S + 0.05);
                        self.redraw(cx);
                    }
                    return;
                }
                self.wheel += e.scroll.y;
                let mut moved = false;
                while self.wheel >= WHEEL_STEP {
                    self.wheel -= WHEEL_STEP;
                    if self.below > 0 {
                        self.first += 1;
                        self.below -= 1;
                        moved = true;
                    }
                }
                while self.wheel <= -WHEEL_STEP {
                    self.wheel += WHEEL_STEP;
                    if self.first > 0 {
                        self.first -= 1;
                        moved = true;
                    }
                }
                if moved {
                    self.redraw(cx);
                }
                return;
            }
        }
        // The card under the pointer shows its actions.
        if let Event::MouseMove(e) = event {
            if self.open {
                // The pointer in use puts the keyboard's ring out, as a Mac's Notification
                // Center shows none then; the focus stays, and the next arrow lights it again.
                if mac_style() && self.keyboard && !self.focus_quiet && self.focus.is_some() {
                    self.focus_quiet = true;
                    self.redraw(cx);
                }
                // A Mac's close stands half over the card's corner: it keeps
                // its card hovered.
                let on_close = |(r, .., key): &(Rect, String, Option<String>, String)| self.hover.as_deref() == Some(key.as_str()) && contains(dismiss_at(*r), e.abs) && self.in_list(e.abs);
                let hover = self.card_at(e.abs).or_else(|| self.card_rects.iter().find(|c| on_close(c)).map(|(.., key)| key.clone()));
                if hover != self.hover {
                    self.hover = hover;
                    self.redraw(cx);
                }
                let clear = self.mac_groups.iter().find(|group| group.expanded && contains(group.clear, e.abs) && self.in_list(e.abs)).map(|group| group.app.clone());
                if clear != self.mac_clear_hover {
                    self.mac_clear_hover = clear;
                    self.redraw(cx);
                }
            }
        }
        if let Event::MouseDown(e) = event {
            if !self.open {
                return;
            }
            self.mac_consume_up = false;
            self.mac_gallery_press = None;
            // The widget gallery's own presses: its search takes the keyboard,
            // the rest act on their release; one on its material does nothing.
            if mac_style() && self.mac_editing && contains(self.mac_gallery, e.abs) {
                self.mac_gallery_press = self.mac_gallery_hits.iter().find(|(r, _)| contains(*r, e.abs)).map(|(_, hit)| *hit);
                self.mac_consume_up = self.mac_gallery_press.is_none();
                self.card_press = None;
                cx.set_key_focus(Area::Empty);
                cx.hide_text_ime();
                self.keyboard = true;
                self.focus_quiet = true;
                self.redraw(cx);
                return;
            }
            if contains(self.close, e.abs) {
                self.close(cx, "its close button");
                return;
            }
            if contains(self.more_below, e.abs) && self.below > 0 {
                self.first += 1;
                self.below -= 1;
                self.redraw(cx);
                return;
            }
            if contains(self.more_above, e.abs) && self.first > 0 {
                self.first -= 1;
                self.redraw(cx);
                return;
            }
            if contains(self.clear_all, e.abs) {
                let keys: Vec<String> = crate::glance::listed().iter().map(|c| c.key()).collect();
                let count = self.dismiss_cards(cx, &keys);
                log!("wm: glance panel cleared {count} card(s)");
                self.first = 0;
                self.mac_scroll = 0.0;
                self.focus = None;
                cx.set_key_focus(Area::Empty);
                cx.hide_text_ime();
                self.keyboard = true;
                self.focus_quiet = mac_style();
                cx.widget_action(self.uid, ShellGlancePanelAction::Dismissed { count });
                self.redraw(cx);
                return;
            }
            if mac_style() && self.in_list(e.abs) {
                if let Some((_, widget)) = self.mac_badges.iter().find(|(r, _)| contains(*r, e.abs)).copied() {
                    self.mac_badge_press = Some(widget);
                    self.card_press = None;
                    self.mac_take_keyboard(cx, widget.key().into());
                    return;
                }
                if contains(self.mac_edit_button, e.abs) {
                    self.mac_edit_press = true;
                    self.card_press = None;
                    self.mac_take_keyboard(cx, "\0edit-widgets".into());
                    self.redraw(cx);
                    return;
                }
                if let Some((_, widget)) = self.mac_widgets.iter().find(|(r, _)| contains(*r, e.abs)).copied() {
                    // While editing, a widget answers only its − or +.
                    if !self.mac_editing { self.mac_widget_press = Some(widget); }
                    self.card_press = None;
                    self.redraw(cx);
                    return;
                }
                let hit = self.mac_groups.iter().find(|group| contains(group.rect, e.abs)
                    || (!group.expanded && self.hover.as_deref() == Some(group.first.as_str()) && contains(group.clear, e.abs))).cloned();
                if let Some(group) = hit {
                    self.mac_consume_up = true;
                    self.card_press = None;
                    let clearing = contains(group.clear, e.abs) && (group.expanded || self.hover.as_deref() == Some(group.first.as_str()));
                    if clearing {
                        let cards = crate::glance::listed();
                        let rows = self.mac_rows(&cards);
                        let previous = rows.iter().position(|row| row.key(&cards) == mac_group_key(&group.app)).unwrap_or(0);
                        let count = self.dismiss_cards(cx, &group.keys);
                        log!("wm: glance group {} dismissed {count} card(s)", group.app);
                        cx.set_key_focus(Area::Empty);
                        cx.hide_text_ime();
                        self.keyboard = true;
                        self.focus_quiet = true;
                        if count > 0 { self.mac_focus_after_group_clear(&group.app, previous); }
                        self.hover = None;
                        cx.widget_action(self.uid, ShellGlancePanelAction::Dismissed { count });
                    } else {
                        self.mac_toggle_group(cx, &group.app, !group.expanded, false);
                    }
                    self.redraw(cx);
                    return;
                }
            }
            // The hovered card's actions.
            let hovered = self.card_rects.iter().find(|(.., key)| self.hover.as_deref() == Some(key.as_str())).cloned();
            if let Some((r, app, route, key)) = hovered {
                if contains(open_at(r), e.abs) {
                    cx.widget_action(self.uid, ShellGlancePanelAction::Open { app, route });
                    return;
                }
                if contains(dismiss_at(r), e.abs) && self.in_list(e.abs) {
                    let count = self.dismiss_cards(cx, &[key.clone()]);
                    if count > 0 {
                        log!("wm: glance card {key} dismissed");
                        cx.set_key_focus(Area::Empty);
                        cx.hide_text_ime();
                        self.keyboard = true;
                        self.focus = None;
                        self.focus_quiet = mac_style();
                        cx.widget_action(self.uid, ShellGlancePanelAction::Dismissed { count });
                    }
                    self.hover = None;
                    self.redraw(cx);
                    return;
                }
            }
            if !contains(self.column, e.abs) {
                self.close(cx, "a press outside it");
                return;
            }
        }
        // The cards' own input and answers (glance_card.rs). A closed panel
        // has swept its tiles; pointer events reach them only while open.
        if self.open || !event.requires_visibility() {
            // A press its card's own controls do not claim (the claim shows
            // across this dispatch) opens the card on its release.
            let claim_before = event.pointer_claimed_area();
            if mac_style() && self.open {
                let visible: Vec<String> = self.card_rects.iter().map(|(.., key)| key.clone()).filter(|key| !self.mac_hidden.contains(key)).collect();
                self.tiles.handle_event_visible(cx, event, Some(&visible));
            } else {
                self.tiles.handle_event(cx, event);
            }
            match event {
                Event::MouseDown(e) => {
                    let claimed = event.pointer_claimed_area() != claim_before;
                    self.card_press = if claimed { None } else { self.card_at(e.abs) };
                    if mac_style() {
                        self.redraw(cx);
                    }
                    // A press in the column takes the keyboard, unless a
                    // card's own control claimed it (its field types then).
                    // The pointer hides the focus ring until a key brings it.
                    if self.open && contains(self.column, e.abs) {
                        self.keyboard = !claimed;
                        self.focus = None;
                    }
                }
                Event::MouseUp(e) => {
                    if mac_style() && self.card_press.is_some() {
                        self.redraw(cx);
                    }
                    if let Some(key) = self.card_press.take() {
                        if self.card_at(e.abs).as_deref() == Some(key.as_str()) {
                            // A chip claims no press and clicks once this
                            // release's handlers have run: wait a frame.
                            self.card_release = Some(key);
                            self.release_frame = cx.new_next_frame();
                        }
                    }
                }
                _ => {}
            }
            // The released card clicked: the press was one of its chips',
            // not the card's. Seen before its taps run, on any event after
            // the release's.
            if let Some(key) = self.card_release.as_deref() {
                if self.tiles.heap_key(cx, key).is_some_and(crate::glance_card::has_clicks) {
                    self.card_release = None;
                }
            }
            if self.release_frame.is_event(event).is_some() {
                if let Some(key) = self.card_release.take() {
                    cx.widget_action(self.uid, activation(&key, ""));
                }
            }
            // Then the L0 taps those tiles queued, as the card window runs
            // its card's, and the cards whose chat moved (a reply came).
            // On a Mac, a card that lowers again stays on its material.
            let (live, tiles) = (&mut self.live, &self.tiles);
            let moved = if mac_style() {
                crate::glance_card::as_notice(|| live.dispatch(cx, tiles, "glance panel"))
            } else {
                live.dispatch(cx, tiles, "glance panel")
            };
            if moved {
                self.redraw(cx);
            }
            if let Some((app, route)) = self.live.take_open_request() {
                cx.widget_action(self.uid, ShellGlancePanelAction::Open { app, route });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new card is brought on screen scrolling as little as that takes:
    /// from the top when everything up to it fits, else as far as needed.
    #[test]
    fn a_new_card_scrolls_the_list_no_further_than_shows_it() {
        let heights = [300.0, 200.0, 250.0];
        assert_eq!(first_to_reveal(&heights, 1, 800.0), 0, "the first two fit: the list stays at the top");
        // All three take 750 + two gaps (24) + the "more below" line (20).
        assert_eq!(first_to_reveal(&heights, 2, 794.0), 0, "all three fit exactly");
        assert_eq!(first_to_reveal(&heights, 2, 780.0), 1, "all three do not: one card scrolls away");
        assert_eq!(first_to_reveal(&heights, 2, 200.0), 2, "the card alone does not fit: it heads the list");
        assert_eq!(first_to_reveal(&heights, 9, 2000.0), 0, "an index past the end is the last card");
    }

    /// The arrows move the focus a card at a time and stop at the ends; the
    /// first press lands on the first card shown.
    #[test]
    fn the_arrows_step_through_the_cards_and_stop_at_the_ends() {
        assert_eq!(step_focus(5, None, 2, true), 2, "first press: the first card shown");
        assert_eq!(step_focus(5, None, 2, false), 2);
        assert_eq!(step_focus(5, Some(2), 0, true), 3);
        assert_eq!(step_focus(5, Some(2), 0, false), 1);
        assert_eq!(step_focus(5, Some(4), 0, true), 4, "the last card stays");
        assert_eq!(step_focus(5, Some(0), 0, false), 0, "the first card stays");
        assert_eq!(step_focus(3, None, 7, true), 2, "a stale first is clamped");
    }

    /// A chat card past its tile's cap (after the first exchange) sits at
    /// its newest rows, so its field and the latest exchange show, and stays
    /// there as the conversation grows; any other card shows its top.
    #[test]
    fn a_chat_card_taller_than_its_tile_shows_its_newest_rows() {
        let chat = TileScroll::start(true);
        assert_eq!(chat.offset(80.0), 80.0, "scrolled up by all it overflows: its bottom shows");
        assert_eq!(chat.offset(140.0), 140.0, "a reply came: still at the newest rows");
        assert_eq!(chat.offset(0.0), 0.0, "it fits: nothing to scroll");
        let other = TileScroll::start(false);
        assert_eq!((other.offset(80.0), other.offset(140.0)), (0.0, 0.0), "its top, as before");
    }

    /// The wheel over a tall card scrolls it as far as it goes and is the
    /// list's at the card's ends; a chat scrolled back to its bottom follows
    /// its newest rows again, and one scrolled up stays where it was read.
    #[test]
    fn the_wheel_scrolls_a_tall_card_and_leaves_the_list_its_ends() {
        let mut at = TileScroll::start(true);
        assert!(!at.wheel(40.0, 80.0, true), "at the newest rows already: the list's");
        assert!(at.wheel(-30.0, 80.0, true));
        assert_eq!(at, TileScroll::Top(50.0));
        assert_eq!(at.offset(120.0), 50.0, "a reply while it was scrolled back: the rows read stay put");
        assert!(at.wheel(-100.0, 80.0, true));
        assert_eq!(at, TileScroll::Top(0.0));
        assert!(!at.wheel(-10.0, 80.0, true), "at its top: the list's");
        assert!(at.wheel(500.0, 80.0, true));
        assert_eq!(at, TileScroll::Newest, "back at the bottom: it follows the newest rows");
        let mut other = TileScroll::start(false);
        assert!(!other.wheel(-10.0, 80.0, false));
        assert!(other.wheel(30.0, 80.0, false));
        assert!(other.wheel(500.0, 80.0, false));
        assert_eq!(other, TileScroll::Top(80.0), "a card with no chat does not follow its bottom");
    }

    /// The Mac's list scrolls as little as shows a card, to its top when it is
    /// taller than the list, and keeps room under the last for its ring.
    #[test]
    fn a_macs_list_scrolls_as_little_as_shows_a_card() {
        assert_eq!(mac_reveal(0.0, 100.0, 50.0, 400.0), 0.0, "in view: the list stays");
        assert_eq!(mac_reveal(0.0, 380.0, 60.0, 400.0), 380.0 + 60.0 + MAC_TAIL - 400.0, "below: up to its foot and ring");
        assert_eq!(mac_reveal(200.0, 100.0, 50.0, 400.0), 100.0, "above: down to its top");
        assert_eq!(mac_reveal(0.0, 500.0, 600.0, 400.0), 500.0, "taller than the list: its top");
    }

    /// The Mac's hovered card has its close half over its top left corner and
    /// its open inside its top right corner, both at their own places.
    #[test]
    fn a_macs_card_has_its_close_on_its_corner() {
        let r = rect(1032.0, 72.0, MAC_WIDTH, 100.0);
        let close = mac_dismiss_at(r);
        assert!(close.pos.x < r.pos.x && close.pos.y < r.pos.y, "it stands over the corner: {close:?}");
        assert!(close.pos.x + close.size.x > r.pos.x && close.pos.y + close.size.y > r.pos.y, "it touches the card");
    }

    /// How long ago, as a Mac's notification says it.
    #[test]
    fn a_macs_notification_says_how_long_ago() {
        let t = 1_000_000_000_u64;
        assert_eq!(mac_age(t, t), "now");
        assert_eq!(mac_age(t + 59_000, t), "now");
        assert_eq!(mac_age(t + 3 * 60_000, t), "3m ago");
        assert_eq!(mac_age(t + 2 * 3_600_000, t), "2h ago");
        assert_eq!(mac_age(t + 30 * 3_600_000, t), "Yesterday");
        assert_eq!(mac_age(t + 3 * 86_400_000, t), "3d ago");
        assert_eq!(mac_age(t, t + 5_000), "now", "a clock that moved back");
    }

    fn sample_card(l0: Option<crate::glance::L0Source>) -> crate::glance::GlanceCard {
        crate::glance::GlanceCard {
            app: "os.calendar".into(), account: None, card_id: "c".into(), title: "Calendar \u{b7} Dentist".into(),
            summary: "The summary".into(), viewport: false, priority: 50, published_ms: 0, expires_ms: u64::MAX,
            open_app: "calendar".into(), route: None, body: "".into(), contained: false, digests: Vec::new(),
            l0: l0.map(std::sync::Arc::new),
        }
    }

    #[test]
    fn mac_stacks_keep_publisher_identity_and_member_order() {
        let mut first = sample_card(None);
        first.app = "os.mail".into(); first.card_id = "first".into(); first.open_app = "shared".into();
        let mut other = sample_card(None);
        other.app = "os.maps".into(); other.card_id = "other".into(); other.open_app = "shared".into();
        let mut second = first.clone(); second.card_id = "second".into();
        let cards = vec![first, other, second];
        let mut expanded = HashSet::new();
        let rows = mac_feed(&cards, &expanded, &MacWidget::ALL);
        assert_eq!(rows.len(), 5);
        assert!(matches!(&rows[0], MacFeedRow::Group { app, members, expanded: false } if app == "os.mail" && members == &[0, 2]));
        assert!(matches!(rows[1], MacFeedRow::Card(1)), "the same open route does not merge publishers");
        assert!(matches!(rows[2], MacFeedRow::Widget(MacWidget::Calendar)));
        assert!(matches!(rows[3], MacFeedRow::Widget(MacWidget::Clock)));
        assert!(matches!(rows[4], MacFeedRow::EditWidgets), "Edit Widgets ends the column");
        assert_eq!(mac_sides(&rows), vec![0, 0, 0, 1, 0], "Calendar and Clock share a row");
        assert!(rows[0].shows_card(&cards, "os.mail/second"));
        assert_ne!(rows[0].key(&cards), cards[0].key());
        expanded.insert("os.mail".into());
        let rows = mac_feed(&cards, &expanded, &MacWidget::ALL);
        assert_eq!(rows.len(), 7);
        assert!(matches!(&rows[0], MacFeedRow::Group { expanded: true, .. }));
        assert!(matches!(rows[1], MacFeedRow::Card(0)));
        assert!(matches!(rows[2], MacFeedRow::Card(2)));
        assert!(matches!(rows[3], MacFeedRow::Card(1)));
        assert!(!rows[0].shows_card(&cards, "os.mail/second"));
        assert!(rows[2].shows_card(&cards, "os.mail/second"));
    }

    /// Edit Widgets takes a widget out: the other keeps the row's left, and
    /// Edit Widgets stays under them, with no widget at all too.
    #[test]
    fn a_widget_taken_out_leaves_the_other_and_edit_widgets() {
        let cards = vec![sample_card(None)];
        let rows = mac_feed(&cards, &HashSet::new(), &[MacWidget::Clock]);
        assert!(matches!(rows[..], [MacFeedRow::Card(0), MacFeedRow::Widget(MacWidget::Clock), MacFeedRow::EditWidgets]));
        assert_eq!(mac_sides(&rows), vec![0, 0, 0], "the Clock alone takes the left");
        let rows = mac_feed(&cards, &HashSet::new(), &[]);
        assert!(matches!(rows[..], [MacFeedRow::Card(0), MacFeedRow::EditWidgets]));
        assert_eq!(rows[1].key(&cards), "\0edit-widgets");
        assert_eq!(mac_widgets_parse("clock\n"), vec![MacWidget::Clock]);
        assert_eq!(mac_widgets_parse("calendar\nclock\n"), MacWidget::ALL.to_vec());
        assert!(mac_widgets_parse("").is_empty() && mac_widgets_parse("weather\n").is_empty());
    }

    /// The widget gallery's search finds a widget by its app's name or by
    /// what it shows, in any case; nothing typed finds them all.
    #[test]
    fn the_gallery_search_finds_widgets_by_app_or_what_they_show() {
        let found = |query: &str| MacWidget::ALL.into_iter().filter(|widget| widget.matches(query)).collect::<Vec<_>>();
        assert_eq!(found(""), MacWidget::ALL.to_vec());
        assert_eq!(found("  "), MacWidget::ALL.to_vec());
        assert_eq!(found("clo"), vec![MacWidget::Clock]);
        assert_eq!(found("CALENDAR"), vec![MacWidget::Calendar]);
        assert_eq!(found("date"), vec![MacWidget::Calendar], "Calendar's line says what it shows");
        assert_eq!(found("city"), vec![MacWidget::Clock]);
        assert!(found("weather").is_empty());
    }

    /// The Calendar widget lists what is left of today, as Calendar keeps it:
    /// an all-day event first, then one under way and one to come; one that
    /// ended, and tomorrow's, are not.
    #[cfg(any(feature = "app-hub", native_mobile))]
    #[test]
    fn the_calendar_widget_lists_what_is_left_of_today() {
        let event = |title: &str, start: &str, end: Option<&str>| octosense_calendar_service::Event {
            id: title.into(), title: title.into(), start: start.into(), end: end.map(Into::into),
            location: String::new(), notes: String::new(), request_id: String::new(), timezone: String::new(),
        };
        let events = vec![
            event("Breakfast", "2026-11-10T08:00", Some("2026-11-10T09:00")),
            event("Standup", "2026-11-10T13:30", Some("2026-11-10T14:30")),
            event("Review", "2026-11-10T16:00", None),
            event("Holiday", "2026-11-10T00:00", None),
            event("Tomorrow", "2026-11-11T10:00", Some("2026-11-11T11:00")),
        ];
        let now = chrono::NaiveDate::from_ymd_opt(2026, 11, 10).unwrap().and_hms_opt(14, 0, 0).unwrap();
        let today = mac_events_today(&events, now);
        let said: Vec<(&str, &str)> = today.iter().map(|e| (e.title.as_str(), e.when.as_str())).collect();
        assert_eq!(said, vec![("Holiday", "All Day"), ("Standup", "1:30 PM \u{2013} 2:30 PM"), ("Review", "4:00 PM")]);
        let late = chrono::NaiveDate::from_ymd_opt(2026, 11, 10).unwrap().and_hms_opt(23, 0, 0).unwrap();
        assert_eq!(mac_events_today(&events, late).iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), vec!["Holiday"]);
    }

    /// A notice reads as a Mac's notification and a click opens its app; a live
    /// card keeps its content and opens its window, as a card with a workspace does.
    #[test]
    fn a_macs_notices_are_plain_and_open_their_app() {
        let notice = crate::glance::L0Source {
            source: format!("{NOTICE_LEDGER}0.2.0\nview root Surface() {{}}"),
            data: serde_json::json!({"note": {"title": "Dentist at 3 pm", "summary": "Main St 12."}}),
            mail: None,
        };
        let card = sample_card(Some(notice));
        assert_eq!(mac_plain(&card), Some(("Dentist at 3 pm".to_string(), "Main St 12.".to_string())));
        assert!(opens_its_app(&card));
        let live = sample_card(Some(crate::glance::L0Source { source: "# ledger mail.action@0.2.0".into(), data: serde_json::json!({}), mail: None }));
        assert_eq!(mac_plain(&live), None);
        assert!(!opens_its_app(&live), "a live card opens its window, where its conversation is");
        let script = sample_card(None);
        assert_eq!(mac_plain(&script), Some(("Calendar \u{b7} Dentist".to_string(), "The summary".to_string())));
        assert!(!opens_its_app(&script), "a script card opens its workspace");
    }

    /// A dismissed card's focus passes to the card under it, or above it
    /// when it was the last, or to none when it was the only one.
    #[test]
    fn a_dismissed_cards_focus_passes_to_its_neighbour() {
        let keys: Vec<String> = ["a", "b", "c"].iter().map(|k| k.to_string()).collect();
        assert_eq!(focus_after_dismiss(&keys, 0).as_deref(), Some("b"));
        assert_eq!(focus_after_dismiss(&keys, 1).as_deref(), Some("c"));
        assert_eq!(focus_after_dismiss(&keys, 2).as_deref(), Some("b"), "the last: the one above");
        assert_eq!(focus_after_dismiss(&keys[..1], 0), None, "the only card: no focus");
    }
}
