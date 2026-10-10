//! Swipeable home pages: the glance page pinned left of the first apps page,
//! one or more pages of app icons, native widget pages, and the App Library.
//!
//! The page model and the pager animation are plain state (`PagesState`)
//! that `PhoneState::step` drives from the shell gesture contract
//! (mobile_gestures.rs): `PageSwipe` moves `drag`, `Commit(Page)` animates
//! `index` one page over, `Cancel` lets `drag` spring back. Until the
//! recognizer lands, pages also change by tapping the indicator dots
//! (`PhoneHit::Page`) and by `--test-action page:<n>`.
//!
//! Positions: apps page `k` is at `k`, the glance page at -1, the library at
//! `apps_count + widget_count`. Reaching the library position opens the existing Drawer /
//! App Library screen; the pager then rests on the last apps page again so
//! coming home lands where the person left.
//!
//! Page assignment: page 0 keeps the tiles and the favorites that fit beside
//! them; favorites beyond that capacity spill to page 1 (and 2, ...), each
//! spill page laid out by `home_layout_for_apps` with no tiles.
//!
//! Drawing lives here too (`impl PhoneSurface`): the glance cards in the
//! frosted style of the App Library, the spill pages, the indicator row.
//!
//! Phone Glance paints bounded summaries without running generated widgets.
//! A tap expands one publication in its feed slot. The host workspace owns
//! its controls; collapsed cards cannot claim input or run background UI.
use crate::{
    desktop::DesktopStyle,
    glance::GlanceCard,
    mobile::{PhoneGesture, PhoneHit, PhoneScreen, PhoneState},
    mobile_gestures::{Dir, GestureKind, ShellGesture},
    mobile_surface::{dock_ids, PhoneSurface},
    mobile_tiles,
    shell::{alpha, rgb, ui::{rect, HAlign, Ico}},
};
use makepad_widgets::*;

/// One page of the pager, in pager order (glance, apps..., widgets..., library).
#[derive(Clone, Debug, PartialEq)]
pub enum HomePage {
    /// The feed of cards left of the first page (position -1).
    Glance,
    /// A page of app icons; page 0 also carries the live tiles.
    Apps { ids: Vec<String> },
    /// An Android widget remains a native view on its own Home page.
    Widget { id: i32 },
    /// The App Library / drawer, the right end of the pager.
    Library,
}

/// One card on the glance page.
#[derive(Clone, Debug, PartialEq)]
pub enum GlanceItem {
    Weather { place: String, temp: String, hi: String, lo: String, cond: String },
    Event { title: String, when: String },
    Fetch { title: String, progress: f64 },
    Note { title: String, body: String },
    /// A card an app published through `glance.publish`, as the glance
    /// service holds it (glance.rs): its publisher (the caller's id, with
    /// its card id the dedupe key), the launcher id its open button opens,
    /// and the L0 card opened on expansion. [`GlanceCards`] keeps summary hit regions only.
    Card(GlanceCard),
}

impl GlanceItem {
    /// The card's height on the glance page, fixed per kind so the column's
    /// extent (and the scroll range) is known without drawing.
    pub fn height(&self) -> f64 {
        match self {
            GlanceItem::Weather { .. } => 128.0,
            GlanceItem::Event { .. } => 78.0,
            GlanceItem::Fetch { .. } => 88.0,
            GlanceItem::Note { .. } => 104.0,
            GlanceItem::Card(_) => SUMMARY_HEIGHT,
        }
    }
    pub fn title(&self) -> &str {
        match self {
            GlanceItem::Weather { place, .. } => place,
            GlanceItem::Event { title, .. } | GlanceItem::Fetch { title, .. } | GlanceItem::Note { title, .. } => title,
            GlanceItem::Card(card) => &card.title,
        }
    }
}

/// The glance page's data: the cards apps published (`glance.publish`,
/// glance.rs; all retained cards by priority then recency),
/// then other posted items (newest first), then what the shell itself
/// knows (refreshed by `sync`). Ranking is the system agent's job later
/// (ADR 0002 §8); until then this order holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlanceFeed {
    /// Only `GlanceItem::Card`s, in glance order.
    cards: Vec<GlanceItem>,
    posted: Vec<GlanceItem>,
    shell: Vec<GlanceItem>,
    /// The glance service's generation `cards` was read at.
    generation: u64,
}

impl GlanceFeed {
    /// Something was posted. A published card replaces the card with the
    /// same `(app, card_id)`; anything else goes on top of the other posts.
    pub fn push(&mut self, item: GlanceItem) {
        match item {
            GlanceItem::Card(card) => {
                self.withdraw(&card.app, &card.card_id);
                self.cards.push(GlanceItem::Card(card));
                self.order_cards();
            }
            other => self.posted.insert(0, other),
        }
    }
    /// A published card went away (withdrawn or expired).
    pub fn withdraw(&mut self, app: &str, card_id: &str) {
        self.cards.retain(|c| !matches!(c, GlanceItem::Card(c) if c.app == app && c.card_id == card_id));
    }
    /// Take the glance service's published set, when it changed.
    pub fn sync_published(&mut self) {
        crate::glance::expire_now();
        let generation = crate::glance::generation();
        if generation == self.generation {
            return;
        }
        self.generation = generation;
        self.replace_cards(crate::glance::shown());
    }
    /// Replace every published card.
    pub fn replace_cards(&mut self, cards: Vec<GlanceCard>) {
        self.cards = cards.into_iter().map(GlanceItem::Card).collect();
        self.order_cards();
    }
    fn order_cards(&mut self) {
        let rank = |i: &GlanceItem| match i {
            GlanceItem::Card(c) => (c.priority, c.published_ms),
            _ => (i64::MIN, 0),
        };
        self.cards.sort_by(|a, b| rank(b).cmp(&rank(a)));
    }
    /// The published cards shown, in glance order.
    pub fn cards(&self) -> impl Iterator<Item = &GlanceCard> {
        self.cards.iter().filter_map(|i| match i {
            GlanceItem::Card(c) => Some(c),
            _ => None,
        })
    }
    /// Replace the shell's own cards, leaving posted ones alone.
    pub fn seed(&mut self, items: Vec<GlanceItem>) {
        self.shell = items;
    }
    pub fn items(&self) -> impl Iterator<Item = &GlanceItem> {
        self.cards.iter().chain(self.posted.iter()).chain(self.shell.iter())
    }
    pub fn len(&self) -> usize {
        self.cards.len() + self.posted.len() + self.shell.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn weather(&self) -> Option<&GlanceItem> {
        self.items().find(|i| matches!(i, GlanceItem::Weather { .. }))
    }
    pub fn next_event(&self) -> Option<&GlanceItem> {
        self.items().find(|i| matches!(i, GlanceItem::Event { .. }))
    }
    /// The whole column's height at `gap` between cards.
    pub fn column_height(&self, gap: f64) -> f64 {
        let n = self.len();
        self.items().map(GlanceItem::height).sum::<f64>() + gap * n.saturating_sub(1) as f64
    }
}

/// The pager: which pages exist, where the pager is, and the glance feed.
#[derive(Clone, Debug)]
pub struct PagesState {
    /// Glance, then every apps page, then the library.
    pub pages: Vec<HomePage>,
    /// Animated position: 0 is the first apps page, -1 the glance page.
    pub index: f64,
    /// Extra offset while a swipe is in progress (springs back on cancel).
    pub drag: f64,
    /// The settle's speed (pages per second): a lightly damped spring, so a
    /// page lands with a hint of overshoot instead of a dead stop.
    velocity: f64,
    held: bool,
    /// Where `index` is heading (a page position).
    target: i64,
    /// A commit or a jump reached the library: the shell opens it once.
    open_library: bool,
    /// A jump asked for before the pages were known (`--test-action page:<n>`
    /// at startup): applied by the first `sync`.
    pending: Option<i64>,
    pub feed: GlanceFeed,
    /// The glance column's scroll offset, in points.
    pub glance_scroll: f64,
    /// Stable return anchor; opening a workspace never changes feed item sizes.
    pub workspace_source: Option<String>,
    glance_velocity: f64,
    pub glance_stretch: f64,
    glance_stretch_velocity: f64,
    glance_track: Vec<(f64, f64)>,
    /// Today's date as the glance header shows it.
    pub date: String,
}

impl Default for PagesState {
    fn default() -> Self {
        Self { pages: Vec::new(), index: 0.0, drag: 0.0, velocity: 0.0, held: false, target: 0, open_library: false, pending: None, feed: GlanceFeed::default(), glance_scroll: 0.0, workspace_source: None, glance_velocity: 0.0, glance_stretch: 0.0, glance_stretch_velocity: 0.0, glance_track: Vec::new(), date: String::new() }
    }
}

/// Cut the favorites into pages: the first `capacity0` on page 0 (beside the
/// tiles), the rest in `capacity_n`-sized spill pages. Always at least one
/// apps page, so the pager has a home even with nothing installed.
pub fn assign_pages(favorites: &[String], capacity0: usize, capacity_n: usize) -> Vec<HomePage> {
    let mut pages = vec![HomePage::Glance];
    let (first, rest) = favorites.split_at(capacity0.min(favorites.len()));
    pages.push(HomePage::Apps { ids: first.to_vec() });
    for chunk in rest.chunks(capacity_n.max(1)) {
        pages.push(HomePage::Apps { ids: chunk.to_vec() });
    }
    pages.push(HomePage::Library);
    pages
}

impl PagesState {
    /// How many apps pages there are (the library's position).
    pub fn apps_count(&self) -> usize {
        self.pages.iter().filter(|p| matches!(p, HomePage::Apps { .. })).count()
    }
    pub fn library_index(&self) -> i64 {
        self.pages.len().saturating_sub(2) as i64
    }
    fn known(&self) -> bool {
        !self.pages.is_empty()
    }
    /// The ids on apps page `k`, empty for any other position.
    pub fn page_ids(&self, k: i64) -> &[String] {
        if k < 0 { return &[]; }
        match self.pages.get(k as usize + 1) {
            Some(HomePage::Apps { ids }) => ids,
            _ => &[],
        }
    }
    pub fn widget_id(&self, k: i64) -> Option<i32> {
        if k<0 {return None;}
        match self.pages.get(k as usize+1) {Some(HomePage::Widget{id})=>Some(*id),_=>None}
    }
    /// Where the pager is right now, drag included.
    pub fn position(&self) -> f64 {
        self.index + self.drag
    }
    /// The page the person is looking at (nearest to the position).
    pub fn current(&self) -> i64 {
        self.position().round() as i64
    }
    pub fn on_glance(&self) -> bool {
        self.current() < 0
    }
    pub(crate) fn glance_requested(&self) -> bool { self.target == -1 || self.pending == Some(-1) }
    /// The horizontal offset of page `k` for a screen `width` wide.
    pub fn page_offset(&self, k: i64, width: f64) -> f64 {
        (k as f64 - self.position()) * width
    }
    /// A page is at least partly on screen.
    pub fn page_visible(&self, k: i64, width: f64) -> bool {
        self.page_offset(k, width).abs() < width - 0.5
    }
    /// Every page position, glance to library.
    pub fn positions(&self) -> std::ops::RangeInclusive<i64> {
        -1..=self.library_index()
    }

    /// The pages changed (apps installed, screen rotated): reassign, and
    /// keep the pager on an apps page inside the new range. Only a commit
    /// or a tap opens the library, never a layout change (the first frame
    /// after a style switch can still be the old window size).
    pub fn sync(&mut self, favorites: &[String], capacity0: usize, capacity_n: usize) {
        self.sync_widgets(favorites,capacity0,capacity_n,&[]);
    }
    pub fn sync_widgets(&mut self, favorites: &[String], capacity0: usize, capacity_n: usize, widgets: &[i32]) {
        let current_widget=self.widget_id(self.current());
        let target_widget=self.widget_id(self.target);
        let mut pages = assign_pages(favorites, capacity0, capacity_n);
        pages.pop();
        pages.extend(widgets.iter().take(16).map(|id|HomePage::Widget{id:*id}));
        pages.push(HomePage::Library);
        let changed = pages != self.pages;
        if changed {
            if let Some(id)=current_widget {
                if let Some(position)=pages.iter().position(|page|matches!(page,HomePage::Widget{id:other} if *other==id)) {
                    self.index+=(position as i64-1-self.current()) as f64;
                }
            }
            if let Some(id)=target_widget {
                if let Some(position)=pages.iter().position(|page|matches!(page,HomePage::Widget{id:other} if *other==id)) {self.target=position as i64-1;}
            }
            self.pages = pages;
        }
        let last = (self.library_index() - 1).max(0);
        if let Some(n) = self.pending.take() {
            self.target = n.clamp(-1, last);
            self.index = self.target as f64;
        }
        self.target = self.target.clamp(-1, last);
        if changed { self.index = self.index.clamp(-1.0, last as f64); }
    }

    /// Go to page `n` (-1 is the glance page; the library's position and
    /// anything past it open the library). Before the pages are known the
    /// jump waits for `sync`, and lands on an apps page.
    pub fn jump(&mut self, n: i64) {
        self.drag = 0.0;
        self.held = false;
        if !self.known() { self.pending = Some(n); return; }
        let lib = self.library_index();
        self.target = n.clamp(-1, lib);
        if self.target == lib { self.open_library = true; }
    }

    /// Grab at the currently displayed position, including an unfinished settle.
    pub fn touch(&mut self) {
        self.index += self.drag;
        self.drag = 0.0;
        self.target = self.index.round().clamp(-1.0, self.library_index() as f64) as i64;
        self.velocity = 0.0;
        self.held = true;
    }
    pub fn release(&mut self, finger_velocity: f64, width: f64) {
        self.held = false;
        self.velocity = (-finger_velocity / width.max(1.0)).clamp(-8.0, 8.0);
    }
    pub fn step(&mut self, dt: f64, gesture: Option<ShellGesture>) -> bool {
        self.step_with_motion(dt, gesture, false)
    }
    pub fn step_with_motion(&mut self, dt: f64, gesture: Option<ShellGesture>, reduced: bool) -> bool {
        let lib = self.library_index() as f64;
        let mut dragging = false;
        match gesture {
            Some(ShellGesture::PageSwipe { dir, progress }) => {
                // A finger moving left reveals the page on the right.
                let sign = match dir { Dir::Left => 1.0, Dir::Right => -1.0 };
                let raw = sign * progress;
                let (lo, hi) = ((-1.0 - self.index).min(0.0), (lib - self.index).max(0.0));
                // Past either end the page gives a little and stiffens, so
                // the end is felt rather than hit (it springs back on lift).
                let over = if raw < lo { raw - lo } else if raw > hi { raw - hi } else { 0.0 };
                self.drag = raw.clamp(lo, hi) + over.signum() * 0.16 * (1.0 - (-over.abs() / 0.16).exp());
                dragging = true;
            }
            Some(ShellGesture::Commit(GestureKind::Page(dir))) => {
                let step = match dir { Dir::Left => 1, Dir::Right => -1 };
                // Choose the adjacent page before folding in the drag. A
                // long swipe past half a page must not skip another page.
                self.target = (self.target + step).clamp(-1, lib as i64);
                self.index += self.drag;
                self.drag = 0.0;
                if self.known() && self.target == lib as i64 { self.open_library = true; }
            }
            Some(ShellGesture::Cancel(GestureKind::Page(_))) => {
                self.target = self.index.round().clamp(-1.0, lib) as i64;
                self.index += self.drag;
                self.drag = 0.0;
            }
            _ => {}
        }
        if dragging { return true; }
        if self.held { return false; }
        let t = if reduced {1.0} else {1.0 - (-dt * 16.0).exp()};
        self.drag += (0.0 - self.drag) * t;
        if self.drag.abs() < 0.001 { self.drag = 0.0; }
        let target = self.target as f64;
        if reduced { self.index = target; self.velocity = 0.0; self.drag = 0.0; return false; }
        // A small overshoot, with release momentum and exact time integration.
        crate::mobile_motion::spring(&mut self.index, &mut self.velocity, target, dt, 420.0, 0.85);
        if (target - self.index).abs() < 0.0015 && self.velocity.abs() < 0.03 { self.index = target; self.velocity = 0.0; }
        self.drag != 0.0 || self.index != target
    }

    /// The pager reached the library: true once, and the pager rests on the
    /// last apps page so the home behind the library is the one left.
    pub fn take_library_request(&mut self) -> bool {
        if !self.open_library { return false; }
        self.open_library = false;
        let last = (self.library_index() - 1).max(0);
        self.target = last;
        self.index = last as f64;
        self.drag = 0.0;
        true
    }

    pub fn anchor_card(&mut self, key: &str) {
        self.workspace_source = Some(key.into());
        self.glance_velocity = 0.0;
        self.glance_stretch_velocity = 0.0;
        self.glance_track.clear();
    }
    pub fn clear_card_anchor(&mut self, height: f64) {
        self.workspace_source = None;
        self.scroll_glance(0.0, height);
    }
    fn glance_height(&self, _height: f64) -> f64 { self.feed.column_height(GLANCE_GAP) }
    fn card_rect(&self, key: &str, screen: Rect) -> Option<Rect> {
        let mut y = screen.pos.y + GLANCE_HEADER - self.glance_scroll + self.glance_stretch;
        for item in self.feed.items() {
            if matches!(item, GlanceItem::Card(card) if card.key() == key) {
                return Some(rect(screen.pos.x + 20.0, y, screen.size.x - 40.0, item.height()));
            }
            y += item.height() + GLANCE_GAP;
        }
        None
    }
    pub fn summary_rect(&self, key: &str, screen: Rect) -> Option<Rect> {
        if !self.on_glance() { return None; }
        let r = tappable(self.card_rect(key, screen)?, glance_column(screen, 0.0));
        (r.size.x > 0.0 && r.size.y > 0.0).then_some(r)
    }
    fn anchor_rect(&self, screen: Rect) -> Option<Rect> { self.card_rect(self.workspace_source.as_deref()?, screen) }
    pub fn glance_touch(&mut self, y: f64, time: f64) {
        self.glance_velocity = 0.0;
        self.glance_stretch_velocity = 0.0;
        self.glance_track.clear();
        self.glance_sample(y, time);
    }
    pub fn glance_sample(&mut self, y: f64, time: f64) {
        self.glance_track.retain(|(_, t)| time - *t <= 0.12);
        if self.glance_track.len() >= 16 { self.glance_track.remove(0); }
        self.glance_track.push((y, time));
    }
    pub fn glance_lift(&mut self, time: f64) {
        self.glance_velocity = match (self.glance_track.first(), self.glance_track.last()) {
            (Some(&(y0, t0)), Some(&(y1, t1))) if t1 - t0 > 0.004 && time - t1 < 0.08 => ((y0 - y1) / (t1 - t0)).clamp(-4000.0, 4000.0),
            _ => 0.0,
        };
        if self.glance_velocity.abs() < 250.0 { self.glance_velocity = 0.0; }
        if self.glance_stretch != 0.0 {
            self.glance_stretch_velocity = crate::mobile_motion::stretch_velocity(self.glance_stretch, -self.glance_velocity);
            self.glance_velocity = 0.0;
        }
        self.glance_track.clear();
    }
    pub fn step_glance(&mut self, dt: f64, height: f64, idle: bool, reduced: bool) -> bool {
        if !idle || !self.on_glance() { return false; }
        let max = self.glance_scroll_max(height);
        crate::mobile_motion::scroll(&mut self.glance_scroll, &mut self.glance_velocity,
            &mut self.glance_stretch, &mut self.glance_stretch_velocity, max, dt, 4.0, reduced)
    }

    /// Scroll the glance column by `dy` points on a screen `height` tall.
    pub fn scroll_glance(&mut self, dy: f64, height: f64) {
        let max = self.glance_scroll_max(height);
        self.glance_scroll = (self.glance_scroll + dy).clamp(0.0, max);
        if dy != 0.0 { self.glance_stretch=0.0; self.glance_stretch_velocity=0.0; self.glance_velocity=0.0; }
    }

    fn glance_scroll_max(&self, height: f64) -> f64 {
        (self.glance_height(height) + GLANCE_HEADER - (height - GLANCE_BOTTOM)).max(0.0)
    }

    /// Lock a vertical body drag to the feed, then follow the finger in
    /// either direction until release. A tap, header pull, shell control,
    /// or already-claimed horizontal page swipe cannot become a feed drag.
    pub fn drag_glance(&mut self, gesture: &mut PhoneGesture, at: Vec2d, screen: Rect, claimed: bool) -> bool {
        if !self.on_glance() || gesture.screen != PhoneScreen::Home { return false; }
        let delta = at - gesture.start;
        let dy = if gesture.glance_scroll {
            gesture.last.y - at.y
        } else {
            // There is no scroll gesture to own when the feed fits. Keeping
            // the page recognizer alive lets a curved thumb swipe turn left
            // after its first sample was mostly vertical.
            if self.glance_scroll_max(screen.size.y) == 0.0 { return false; }
            if claimed || gesture.hit.is_some() || !glance_column(screen, 0.0).contains(gesture.start)
                || delta.length() <= crate::mobile_gestures::SLOP || delta.y.abs() <= delta.x.abs() * 1.2 { return false; }
            gesture.glance_scroll = true;
            gesture.shell = false;
            -delta.y
        };
        let max = self.glance_scroll_max(screen.size.y);
        crate::mobile_motion::drag(&mut self.glance_scroll, &mut self.glance_stretch, dy, max);
        true
    }

    /// The one-line "at a glance" strip on page 0: the date, then the
    /// weather and the next event when the feed has them.
    pub fn strip_text(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.date.is_empty() { parts.push(self.date.clone()); }
        if let Some(GlanceItem::Weather { temp, cond, .. }) = self.feed.weather() {
            parts.push(format!("{temp} {cond}"));
        }
        if let Some(GlanceItem::Event { title, when, .. }) = self.feed.next_event() {
            parts.push(format!("{title} {when}"));
        }
        parts.join("  ·  ")
    }
}

/// Glance owns the page: navigation stays in the header, and the bottom
/// leaves room for the system gesture area rather than the home app dock.
pub(crate) const SUMMARY_HEIGHT: f64 = 124.0;
pub const GLANCE_HEADER: f64 = 112.0;
const GLANCE_BOTTOM: f64 = 32.0;
const GLANCE_GAP: f64 = 12.0;

pub(crate) fn glance_column(screen: Rect, dx: f64) -> Rect {
    rect(screen.pos.x + dx, screen.pos.y + GLANCE_HEADER, screen.size.x,
        (screen.size.y - GLANCE_HEADER - GLANCE_BOTTOM).max(0.0))
}

/// Refresh the page model from the shell each frame before the home draws:
/// the favorites (the launcher's apps minus the dock), the capacities of
/// page 0 and of a spill page on this screen, and the glance feed's shell
/// cards. Cheap when nothing changed.
pub fn sync(phone: &mut PhoneState, style: DesktopStyle, screen: Rect) {
    if screen.size.x < 1.0 || screen.size.y < 1.0 { return; }
    let apps = crate::shell::launcher::apps();
    let ids: Vec<(String, String)> = apps.iter().map(|a| (a.id.trim_start_matches("apps.").to_string(), a.label.clone())).collect();
    // The dock as drawn, stand-ins included: what is docked is not also an icon on a page.
    let dock = dock_ids(&phone.android.dock, |id| ids.iter().any(|(app, _)| app == id));
    let mut favorites: Vec<String> = ids.iter().filter(|(id, _)| !dock.contains(&id.as_str()) && !phone.android.hidden_hosted.contains(id)).map(|(id, _)| id.clone()).collect();
    favorites.extend(phone.android.favorites.iter().filter(|id| !dock.contains(&id.as_str())).cloned());
    // The person's own order (a drag), listed ids first; the rest follow in
    // the default order.
    if !phone.android.order.is_empty() {
        let order = &phone.android.order;
        favorites.sort_by_key(|id| order.iter().position(|o| o == id).unwrap_or(usize::MAX));
    }
    let capacity0 = PhoneSurface::home_layout(style, screen).capacity;
    let spill = mobile_tiles::home_layout_for_apps(screen, PhoneSurface::home_top(style, screen), PhoneSurface::home_dock(screen), &[]);
    let widgets: Vec<_>=phone.android.widgets.iter().map(|widget|widget.id).collect();
    phone.pages.sync_widgets(&favorites, capacity0, spill.capacity.max(1),&widgets);
    // The date changes once a day; the string compare is the cheap check.
    let date = crate::host::fallback_clock(true);
    let weekday = crate::host::fallback_clock(false);
    let weekday = weekday.split_whitespace().next().unwrap_or("");
    let date = if weekday.is_empty() { date } else { format!("{weekday}, {date}") };
    if phone.pages.date != date { phone.pages.date = date; }
    phone.pages.feed.seed(shell_cards(&ids, &phone.tiles));
    // What apps published (glance.rs): re-read only when it changed.
    let anchor = phone.pages.anchor_rect(screen).map(|r| r.pos.y);
    phone.pages.feed.sync_published();
    if let (Some(before), Some(after)) = (anchor, phone.pages.anchor_rect(screen)) {
        phone.pages.glance_scroll += after.pos.y - before;
    }
}

/// The cards the shell can fill in by itself. The weather tile's data lives
/// in the Weather app's own process and is not reachable from here; a
/// Weather card arrives through `GlanceFeed::push` once an app posts one.
fn shell_cards(ids: &[(String, String)], tiles: &mobile_tiles::HomeTiles) -> Vec<GlanceItem> {
    let mut cards = Vec::new();
    let live: Vec<&str> = mobile_tiles::TILE_APPS.iter().map(|(id, _)| *id)
        .filter(|id| tiles.client_of(id).is_some_and(|c| tiles.get(c).is_some_and(|t| t.tile_ready()))).collect();
    if !live.is_empty() {
        let names: Vec<String> = live.iter().map(|id| label_of(ids, id)).collect();
        cards.push(GlanceItem::Note { title: "Live tiles".into(), body: format!("{} on the home page: {}", live.len(), names.join(", ")) });
    }
    let names: Vec<String> = ids.iter().map(|(_, label)| label.clone()).collect();
    let body = if names.is_empty() { "No apps are linked into this build.".to_string() } else { names.join(", ") };
    let title = if ids.len() == 1 { "1 app".to_string() } else { format!("{} apps", ids.len()) };
    cards.push(GlanceItem::Note { title, body });
    cards.push(GlanceItem::Note { title: "At a glance".into(), body: "Weather, events and downloads land here when an app posts them.".into() });
    cards
}

fn label_of(ids: &[(String, String)], id: &str) -> String {
    ids.iter().find(|(i, _)| i == id).map(|(_, l)| l.clone()).unwrap_or_else(|| id.to_string())
}

// ------------------------------------------------------- the live cards

/// How far a finger may travel and still tap: the shell's own rule for its
/// hits (mobile_app.rs), and about a lowered card's tap targets' (Octoscript-
/// Makepad's `OctoscriptTap`, 12 points).
pub const TAP_SLOP: f64 = 12.0;

/// A pointer event as the shell's finger makes it, for the glance page's
/// cards: the shell, not a card, decides what a tap is. A card's tap target
/// fires on a release near its press, but on the glance page the same finger
/// may be a page swipe, a pull, a long press, or a press on the shell's own
/// controls (a card's open button, the page indicator, the dock).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GlanceFinger {
    /// Not the pointer: typing in a card's field, an answer, a timer. The
    /// cards' taps run.
    NotPointer,
    /// The lift of a finger that stayed a plain tap, down at this point:
    /// the card under it may act on it, and no other.
    Tap(Vec2d),
    /// Any other pointer event: no card acts on it.
    NotATap,
}

impl GlanceFinger {
    /// What `event` is, before the shell handles it: `gesture` is the finger
    /// the shell holds (`PhoneState::gesture`), `claimed` whether the
    /// recognizer took it for a swipe or a pull (mobile_gestures.rs, its
    /// gesture in progress), `touch` the touch the shell follows.
    pub fn of(event: &Event, gesture: Option<&PhoneGesture>, claimed: bool, touch: Option<u64>) -> Self {
        use makepad_platform::event::TouchState;
        let lift = match event {
            Event::MouseUp(e) if e.button.contains(MouseButton::PRIMARY) => e.abs,
            Event::TouchUpdate(update) => match update.touches.iter().find(|t| t.state == TouchState::Stop && Some(t.uid) == touch) {
                Some(t) => t.abs,
                None => return Self::NotATap,
            },
            Event::MouseDown(_) | Event::MouseMove(_) | Event::MouseUp(_) | Event::Scroll(_) | Event::LongPress(_) => return Self::NotATap,
            _ => return Self::NotPointer,
        };
        match gesture {
            // A press on the page itself, not on one of the shell's controls,
            // that no gesture took and that stayed put.
            Some(g) if g.screen == PhoneScreen::Home && g.hit.is_none() && !claimed && !g.glance_scroll && (lift - g.start).length() < TAP_SLOP => Self::Tap(g.start),
            _ => Self::NotATap,
        }
    }
}

/// The part of a tile a finger can tap: the tile (a card taller than the
/// tile's cap is clipped there, its taps too) as far as `column` shows it,
/// the page above the page indicator and the dock, which are the shell's.
pub fn tappable(tile: Rect, column: Rect) -> Rect {
    tile.clip((column.pos, column.pos + column.size))
}

const WHO: &str = "glance page";

/// Visible summary hit regions only: no Splash tiles or L0 sessions in the feed.
#[derive(Default)]
pub struct GlanceCards { drawn: Vec<(String, Rect)>, logged: String }
impl GlanceCards {
    pub fn begin(&mut self) { self.drawn.clear(); }
    pub fn record(&mut self, card: &GlanceCard, tile: Rect, column: Rect) {
        self.drawn.push((card.key(), tappable(tile, column)));
    }
    pub fn log_layout(&mut self) {
        let layout = self.drawn.iter().map(|(key, r)| format!("{key}@{},{},{},{}", r.pos.x as i32, r.pos.y as i32, r.size.x as i32, r.size.y as i32)).collect::<Vec<_>>().join(" ");
        if layout != self.logged { log!("{WHO}: {} summaries {layout}", self.drawn.len()); self.logged = layout; }
    }
    pub(crate) fn under(&self, p: Vec2d) -> Option<&str> {
        self.drawn.iter().rev().find(|(_, r)| r.size.x > 0.0 && r.size.y > 0.0 && r.contains(p)).map(|(key, _)| key.as_str())
    }
}

// ---------------------------------------------------------------- drawing

impl PhoneSurface {
    /// The glance page at horizontal offset `dx`: a dimmed wallpaper and a
    /// scrollable column of frosted cards under an "At a glance" header.
    pub fn draw_glance(&mut self, cx: &mut Cx2d, phone: &PhoneState, screen: Rect, style: DesktopStyle, dark: bool, opacity: f32, dx: f64) {
        self.use_fonts(style == DesktopStyle::Ios);
        let page = rect(screen.pos.x + dx, screen.pos.y, screen.size.x, screen.size.y);
        // The dimming runs under the system bars too, so the status and
        // navigation bands match the page instead of showing bare wallpaper.
        let i = &phone.insets;
        let dimmed = rect(page.pos.x, page.pos.y - i.top, page.size.x, page.size.y + i.top + i.bottom);
        self.rounded(cx, dimmed, 0.0, alpha(self.theme_ground(if dark { rgb(8, 9, 16) } else { rgb(228, 231, 242) }), 0.86 * opacity));
        let ink = alpha(self.theme_ink(if dark { rgb(255, 255, 255) } else { rgb(26, 26, 32) }), opacity);
        let top = page.pos.y + 36.0;
        let left = page.pos.x + 20.0;
        let width = page.size.x - 40.0;
        let heading_width = if crate::mobile_navigation::ENABLED { width - 84.0 } else { width };
        self.d.label_elided(cx, rect(left, top, heading_width, 30.0), true, 24.0, ink, HAlign::Left, "At a glance");
        self.d.label_elided(cx, rect(left, top + 32.0, heading_width, 20.0), false, 13.0, alpha(ink, 0.7 * opacity), HAlign::Left, &phone.pages.date);
        let column = glance_column(screen, dx).clip((screen.pos, screen.pos + screen.size));
        let bottom = column.pos.y + column.size.y;
        let mut y = page.pos.y + GLANCE_HEADER - phone.pages.glance_scroll + phone.pages.glance_stretch;
        // Painting and hit testing use the same viewport. Previously only
        // hits were clipped, so text painted behind the launcher controls.
        cx.begin_turtle(Walk::abs_rect(column), Layout::default());
        for item in phone.pages.feed.items() {
            let h = item.height();
            if y + h > column.pos.y && y < bottom {
                self.draw_glance_card(cx, rect(left, y, width, h), column, item, style, dark, ink, opacity);
            }
            y += h + GLANCE_GAP;
        }
        cx.end_turtle();
        if dx == 0.0 && phone.gesture.is_none() && phone.pages.glance_velocity == 0.0 {
            self.glance_cards.log_layout();
        }
    }

    fn draw_glance_card(&mut self, cx: &mut Cx2d, r: Rect, column: Rect, item: &GlanceItem, style: DesktopStyle, dark: bool, ink: Vec4f, opacity: f32) {
        if let GlanceItem::Card(card) = item {
            self.rounded(cx, r, 10.0, alpha(self.theme_face(rgb(255, 255, 255)), if dark { 0.18 } else { 0.92 } * opacity));
            self.glance_cards.record(card, rect(r.pos.x, r.pos.y, r.size.x, SUMMARY_HEIGHT), column);
            let pad = 16.0;
            let w = r.size.x - pad * 2.0;
            self.d.label_elided(cx, rect(r.pos.x + pad, r.pos.y + 12.0, w - 44.0, 18.0), true, 10.0, alpha(ink, 0.62), HAlign::Left, &card.open_app.to_uppercase());
            self.d.icon_centered(cx, Ico::ChevronDown, rect(r.pos.x + r.size.x - 48.0, r.pos.y + 4.0, 44.0, 44.0), 14.0, ink);
            self.d.label_elided(cx, rect(r.pos.x + pad, r.pos.y + 35.0, w, 23.0), true, 15.0, ink, HAlign::Left, &card.title);
            let summary = if card.summary.is_empty() { "Tap to view this card" } else { &card.summary };
            for (n, line) in self.d.wrap(cx, false, 13.0, summary, w, 2).iter().enumerate() {
                self.d.label(cx, rect(r.pos.x + pad, r.pos.y + 65.0 + n as f64 * 19.0, w, 19.0), false, 13.0, alpha(ink, 0.72), HAlign::Left, line);
            }
            return;
        }
        self.rounded(cx, r, 18.0, alpha(self.theme_face(rgb(255, 255, 255)), if dark { 0.10 } else { 0.55 } * opacity));
        let pad = 16.0;
        let inner = rect(r.pos.x + pad, r.pos.y + pad, r.size.x - pad * 2.0, r.size.y - pad * 2.0);
        let dim = alpha(ink, 0.65 * opacity);
        let accent = self.theme_accent(if style == DesktopStyle::Ios { rgb(0, 122, 255) } else { rgb(103, 80, 164) });
        match item {
            GlanceItem::Weather { place, temp, hi, lo, cond } => {
                self.d.label_elided(cx, rect(inner.pos.x, inner.pos.y, inner.size.x * 0.6, 20.0), true, 15.0, ink, HAlign::Left, place);
                self.d.label_elided(cx, rect(inner.pos.x + inner.size.x * 0.5, inner.pos.y, inner.size.x * 0.5, 20.0), false, 13.0, dim, HAlign::Right, cond);
                self.d.label_elided(cx, rect(inner.pos.x, inner.pos.y + 30.0, inner.size.x * 0.6, 50.0), false, 40.0, ink, HAlign::Left, temp);
                self.d.label_elided(cx, rect(inner.pos.x + inner.size.x * 0.5, inner.pos.y + 44.0, inner.size.x * 0.5, 20.0), false, 13.0, dim, HAlign::Right, &format!("H:{hi}  L:{lo}"));
            }
            GlanceItem::Event { title, when } => {
                self.d.icon_centered(cx, Ico::Calendar, rect(inner.pos.x, inner.pos.y, 28.0, inner.size.y), 20.0, alpha(accent, opacity));
                self.d.label_elided(cx, rect(inner.pos.x + 40.0, inner.pos.y, inner.size.x - 40.0, 22.0), true, 15.0, ink, HAlign::Left, title);
                self.d.label_elided(cx, rect(inner.pos.x + 40.0, inner.pos.y + 24.0, inner.size.x - 40.0, 20.0), false, 13.0, dim, HAlign::Left, when);
            }
            GlanceItem::Fetch { title, progress } => {
                let progress = progress.clamp(0.0, 1.0);
                self.d.label_elided(cx, rect(inner.pos.x, inner.pos.y, inner.size.x - 60.0, 22.0), true, 15.0, ink, HAlign::Left, title);
                self.d.label_elided(cx, rect(inner.pos.x + inner.size.x - 60.0, inner.pos.y, 60.0, 22.0), false, 13.0, dim, HAlign::Right, &format!("{}%", (progress * 100.0).round()));
                let track = rect(inner.pos.x, inner.pos.y + 36.0, inner.size.x, 8.0);
                self.rounded(cx, track, 4.0, alpha(ink, 0.15 * opacity));
                if progress > 0.0 {
                    self.rounded(cx, rect(track.pos.x, track.pos.y, (track.size.x * progress).max(8.0), 8.0), 4.0, alpha(accent, opacity));
                }
            }
            // Drawn above, before the frosted frame.
            GlanceItem::Card(_) => {}
            GlanceItem::Note { title, body } => {
                self.d.label_elided(cx, rect(inner.pos.x, inner.pos.y, inner.size.x, 22.0), true, 15.0, ink, HAlign::Left, title);
                let lines = self.d.wrap(cx, false, 13.0, body, inner.size.x, 3);
                for (n, line) in lines.iter().enumerate() {
                    self.d.label(cx, rect(inner.pos.x, inner.pos.y + 26.0 + n as f64 * 18.0, inner.size.x, 18.0), false, 13.0, dim, HAlign::Left, line);
                }
            }
        }
    }

    /// The library's stand-in while it slides in from the right: the pager's
    /// last position opens the real App Library / drawer screen.
    pub fn draw_library_preview(&mut self, cx: &mut Cx2d, screen: Rect, dark: bool, ink: Vec4f, opacity: f32, dx: f64) {
        let page = rect(screen.pos.x + dx, screen.pos.y, screen.size.x, screen.size.y);
        self.rounded(cx, page, 0.0, alpha(self.theme_ground(if dark { rgb(8, 9, 16) } else { rgb(228, 231, 242) }), 0.86 * opacity));
        let glyph = rect(page.pos.x + (page.size.x - 48.0) * 0.5, page.pos.y + page.size.y * 0.42, 48.0, 48.0);
        self.rounded(cx, glyph, 12.0, alpha(ink, 0.35 * opacity));
        for (gx, gy) in [(10.0, 10.0), (26.0, 10.0), (10.0, 26.0), (26.0, 26.0)] {
            self.rounded(cx, rect(glyph.pos.x + gx, glyph.pos.y + gy, 12.0, 12.0), 3.0, alpha(ink, 0.9 * opacity));
        }
        self.label(cx, rect(page.pos.x, glyph.pos.y + 60.0, page.size.x, 24.0), "App Library", 15.0, true, alpha(ink, opacity));
    }

    /// The indicator row above the dock: the glance glyph, a dot per apps
    /// page, the library glyph. Tapping any of them jumps there.
    pub fn draw_page_indicator(&mut self, cx: &mut Cx2d, phone: &PhoneState, dock: Rect, screen: Rect, ink: Vec4f, opacity: f32, hits: bool) {
        let pages = &phone.pages;
        let current = pages.current();
        let count = pages.library_index() as usize;
        // Each dot's slot is a 44-point touch target; the dots stay small.
        let cell = 44.0_f64.min((screen.size.x-24.0)/(count+2).max(1) as f64);
        let total = (count + 2) as f64 * cell;
        let left = screen.pos.x + (screen.size.x - total) * 0.5;
        let y = dock.pos.y - 30.0;
        for (n, k) in pages.positions().enumerate() {
            let slot = rect(left + n as f64 * cell, y - 10.0, cell, 44.0);
            let active = k == current;
            let a = if active { 1.0 } else { 0.45 } * opacity;
            let c = dvec2(slot.pos.x + cell * 0.5, slot.pos.y + 22.0);
            if k < 0 {
                // The glance page: a small card with two lines of text.
                let g = rect(c.x - 6.0, c.y - 6.0, 12.0, 12.0);
                self.rounded(cx, g, 3.0, alpha(ink, a));
                let bar = if dark_on(ink) { alpha(rgb(255, 255, 255), a) } else { alpha(rgb(20, 20, 30), 0.9 * a) };
                self.rounded(cx, rect(g.pos.x + 2.5, g.pos.y + 3.0, 7.0, 2.0), 1.0, bar);
                self.rounded(cx, rect(g.pos.x + 2.5, g.pos.y + 7.0, 5.0, 2.0), 1.0, bar);
            } else if k == pages.library_index() {
                let lib = rect(c.x - 6.0, c.y - 6.0, 12.0, 12.0);
                self.rounded(cx, lib, 3.0, alpha(ink, a));
                let cell_ink = if dark_on(ink) { alpha(rgb(255, 255, 255), a) } else { alpha(rgb(20, 20, 30), 0.9 * a) };
                for (gx, gy) in [(2.5, 2.5), (6.5, 2.5), (2.5, 6.5), (6.5, 6.5)] {
                    self.rounded(cx, rect(lib.pos.x + gx, lib.pos.y + gy, 3.0, 3.0), 1.0, cell_ink);
                }
            } else {
                let d = if active { 8.0 } else { 6.0 };
                self.rounded(cx, rect(c.x - d * 0.5, c.y - d * 0.5, d, d), d as f32 * 0.5, alpha(ink, a));
            }
            if hits { self.hits.push((rect(slot.pos.x, slot.pos.y - 8.0, cell, 40.0), PhoneHit::Page(k))); }
        }
    }
}

/// The indicator ink is light on a dark ground: the glyph insides go dark.
fn dark_on(ink: Vec4f) -> bool {
    ink.x + ink.y + ink.z < 1.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("app{i}")).collect()
    }
    fn settle(pages: &mut PagesState) {
        for _ in 0..120 { pages.step(1.0 / 60.0, None); }
    }

    #[test]
    fn overflow_favorites_spill_to_page_1_and_the_library_stays_last() {
        let pages = assign_pages(&ids(12), 8, 12);
        assert_eq!(pages.len(), 4, "glance, page 0, page 1, library");
        assert_eq!(pages[0], HomePage::Glance);
        assert_eq!(pages[1], HomePage::Apps { ids: ids(8) });
        assert_eq!(pages[2], HomePage::Apps { ids: ids(12)[8..].to_vec() });
        assert_eq!(pages[3], HomePage::Library);
        // Everything fits beside the tiles: one apps page, no spill.
        let pages = assign_pages(&ids(5), 8, 12);
        assert_eq!(pages, vec![HomePage::Glance, HomePage::Apps { ids: ids(5) }, HomePage::Library]);
        // Two spill pages when the spill page is small; nothing installed
        // still leaves one (empty) apps page to come home to.
        assert_eq!(assign_pages(&ids(20), 8, 6).iter().filter(|p| matches!(p, HomePage::Apps { .. })).count(), 3);
        assert_eq!(assign_pages(&[], 8, 12), vec![HomePage::Glance, HomePage::Apps { ids: vec![] }, HomePage::Library]);
        let mut state = PagesState::default();
        state.sync(&ids(12), 8, 12);
        assert_eq!(state.apps_count(), 2);
        assert_eq!(state.library_index(), 2);
        assert_eq!(state.page_ids(1), &ids(12)[8..]);
        assert!(state.page_ids(-1).is_empty() && state.page_ids(2).is_empty());
    }

    #[test]
    fn widgets_keep_identity_and_library_navigation_after_removal() {
        let mut state=PagesState::default();
        state.sync_widgets(&ids(12),8,12,&[71,93]);
        assert_eq!(state.apps_count(),2);
        assert_eq!(state.library_index(),4);
        assert_eq!(state.widget_id(2),Some(71));
        assert_eq!(state.widget_id(3),Some(93));
        assert!(state.page_ids(2).is_empty());
        assert_eq!(state.page_ids(1),&ids(12)[8..]);
        state.jump(3);settle(&mut state);
        assert_eq!(state.current(),3);
        state.sync_widgets(&ids(24),8,12,&[71,93]);
        assert_eq!(state.widget_id(state.current()),Some(93),"installing apps keeps the visible widget identity");
        state.sync_widgets(&ids(12),8,12,&[71,93]);
        assert_eq!(state.current(),3);
        state.sync_widgets(&ids(12),8,12,&[71]);
        assert_eq!(state.current(),2);
        assert_eq!(state.widget_id(state.current()),Some(71));
        assert_eq!(state.library_index(),3);
        state.jump(3);
        assert!(state.open_library);
        state.sync_widgets(&ids(12),8,12,&[]);
        assert_eq!(state.library_index(),2);
        assert_eq!(state.current(),1);
    }

    #[test]
    fn release_keeps_speed_and_a_new_touch_grabs_the_displayed_position() {
        let simulate = |speed| {
            let mut p = PagesState::default(); p.sync(&ids(24),8,12); p.touch();
            p.step(1.0/60.0,Some(ShellGesture::PageSwipe{dir:Dir::Left,progress:0.35}));
            p.release(speed,400.0);
            p.step(1.0/60.0,Some(ShellGesture::Commit(GestureKind::Page(Dir::Left))));
            p
        };
        let slow=simulate(-100.0); let mut fast=simulate(-1800.0);
        assert!(fast.position()>slow.position()+0.03,"a fast release must retain more momentum");
        let displayed=fast.position(); fast.touch();
        assert_eq!(fast.position(),displayed);
        fast.step(1.0/60.0,None);
        assert_eq!(fast.position(),displayed,"a new finger holds the in-flight spring");
        fast.step(1.0/60.0,Some(ShellGesture::PageSwipe{dir:Dir::Right,progress:0.1}));
        assert!((fast.position()-(displayed-0.1)).abs()<1e-9);
        fast.release(200.0,400.0);
        fast.step(1.0/60.0,Some(ShellGesture::Cancel(GestureKind::Page(Dir::Right))));
        settle(&mut fast);
        assert_eq!(fast.position().fract(),0.0);
    }

    #[test]
    fn swipes_commit_cancel_and_clamp_at_both_ends() {
        let mut p = PagesState::default();
        p.sync(&ids(12), 8, 12);
        // A swipe left drags the page after it in; committing lands on it.
        assert!(p.step(1.0 / 60.0, Some(ShellGesture::PageSwipe { dir: Dir::Left, progress: 0.5 })));
        assert!((p.drag - 0.5).abs() < 1e-9);
        assert!(p.page_offset(1, 400.0) < 400.0 && p.page_offset(0, 400.0) < 0.0);
        p.step(1.0 / 60.0, Some(ShellGesture::Commit(GestureKind::Page(Dir::Left))));
        assert_eq!(p.drag, 0.0, "the drag folds into the index on commit");
        settle(&mut p);
        assert_eq!(p.index, 1.0);
        assert_eq!(p.current(), 1);
        assert!(!p.step(1.0 / 60.0, None), "settled");
        // A cancelled swipe springs back to the same page.
        p.step(1.0 / 60.0, Some(ShellGesture::PageSwipe { dir: Dir::Right, progress: 0.3 }));
        assert!(p.drag < 0.0);
        p.step(1.0 / 60.0, Some(ShellGesture::Cancel(GestureKind::Page(Dir::Right))));
        settle(&mut p);
        assert_eq!((p.index, p.drag), (1.0, 0.0));
        // Right past page 0 is the glance page, and nothing left of it.
        for _ in 0..4 { p.step(1.0 / 60.0, Some(ShellGesture::Commit(GestureKind::Page(Dir::Right)))); settle(&mut p); }
        assert_eq!(p.index, -1.0);
        assert!(p.on_glance());
        p.step(1.0 / 60.0, Some(ShellGesture::PageSwipe { dir: Dir::Right, progress: 1.0 }));
        assert!(p.drag < 0.0 && p.drag > -0.16, "past the glance page the pager only stretches a little: {}", p.drag);
        p.step(1.0 / 60.0, Some(ShellGesture::Cancel(GestureKind::Page(Dir::Right))));
        settle(&mut p);
        assert_eq!((p.index, p.drag), (-1.0, 0.0), "and springs back on lift");
        assert!(!p.take_library_request());
        // Left past the last apps page reaches the library exactly once,
        // and the pager rests on the last apps page underneath it.
        for _ in 0..3 { p.step(1.0 / 60.0, Some(ShellGesture::Commit(GestureKind::Page(Dir::Left)))); settle(&mut p); }
        assert!(p.take_library_request());
        assert!(!p.take_library_request());
        assert_eq!((p.index, p.current()), (1.0, 1));
        // Other gestures are not the pager's business.
        p.step(1.0 / 60.0, Some(ShellGesture::Commit(GestureKind::HomeUp)));
        p.step(1.0 / 60.0, Some(ShellGesture::HomeUp { progress: 0.5, held: false }));
        assert_eq!((p.index, p.drag), (1.0, 0.0));
    }

    #[test]
    fn long_swipes_follow_the_finger_and_land_on_only_the_adjacent_page() {
        use crate::mobile_gestures::{FingerPhase, GestureContext, GestureRecognizer, ExclusionZones, SafeInsets};
        for width in [360.0, 600.0] {
            for (dir, sign) in [(Dir::Left, -1.0), (Dir::Right, 1.0)] {
                let mut pages = PagesState::default();
                pages.sync(&ids(40), 8, 12);
                pages.jump(1);
                settle(&mut pages);
                let context = GestureContext { screen: rect(0.0, 0.0, width, 900.0), insets: SafeInsets::default(), phone: crate::mobile::PhoneScreen::Home, body: true, glance: None, system_edges: true, shade: false };
                let mut recognizer = GestureRecognizer::default();
                let zones = ExclusionZones::default();
                let start = dvec2(if sign < 0.0 {width * 0.9} else {width * 0.1}, 400.0);
                recognizer.feed(FingerPhase::Down, start, 0.0, &context, &zones);
                for step in 1..=8 {
                    let distance = width * step as f64 / 10.0;
                    let gesture = recognizer.feed(FingerPhase::Move, start + dvec2(sign * distance, 0.0), step as f64 * 0.1, &context, &zones);
                    pages.step(1.0 / 60.0, gesture);
                    assert!((pages.page_offset(1, width) - sign * distance).abs() < 1e-6, "{dir:?} at {distance}: the page must keep following after commit distance");
                }
                let gesture = recognizer.feed(FingerPhase::Up, start + dvec2(sign * width * 0.8, 0.0), 0.9, &context, &zones);
                pages.step(1.0 / 60.0, gesture);
                settle(&mut pages);
                assert_eq!(pages.current(), if dir == Dir::Left {2} else {0});
                assert!(!pages.take_library_request());
            }
        }
    }

    #[test]
    fn reversing_a_drag_tracks_back_across_its_start_and_cancels() {
        let mut pages = PagesState::default();
        pages.sync(&ids(40), 8, 12);
        for progress in [0.65, 0.4, 0.1, 0.0, -0.1] {
            pages.step(1.0 / 60.0, Some(ShellGesture::PageSwipe { dir: Dir::Left, progress }));
            assert!((pages.position() - progress).abs() < 1e-6);
        }
        pages.step(1.0 / 60.0, Some(ShellGesture::Cancel(GestureKind::Page(Dir::Left))));
        settle(&mut pages);
        assert_eq!(pages.position(), 0.0);
    }

    #[test]
    fn jumps_clamp_and_wait_for_the_pages_when_they_come_first() {
        let mut p = PagesState::default();
        // `--test-action page:1` fires before the first frame lays pages out.
        p.jump(1);
        assert!(!p.take_library_request(), "nothing known yet: not the library");
        p.sync(&ids(12), 8, 12);
        settle(&mut p);
        assert_eq!(p.index, 1.0);
        p.jump(-5);
        settle(&mut p);
        assert_eq!(p.index, -1.0, "clamped to the glance page");
        p.jump(9);
        assert!(p.take_library_request(), "past the end is the library");
        assert_eq!(p.index, 1.0);
        // Fewer apps: the pager is pulled back onto the last apps page, and
        // a layout change never opens the library by itself.
        p.jump(1);
        settle(&mut p);
        p.sync(&ids(3), 8, 12);
        assert_eq!(p.apps_count(), 1);
        assert!(!p.take_library_request());
        assert_eq!((p.index, p.current()), (0.0, 0));
        // A deferred jump past the end lands on the last apps page too: the
        // first layout may be transient, so only a tap opens the library.
        let mut p = PagesState::default();
        p.jump(5);
        p.sync(&ids(12), 8, 12);
        assert!(!p.take_library_request());
        assert_eq!(p.index, 1.0);
    }

    #[test]
    fn the_glance_feed_keeps_posted_cards_newest_first_above_the_shells() {
        let mut feed = GlanceFeed::default();
        feed.seed(vec![GlanceItem::Note { title: "Apps".into(), body: "a, b".into() }]);
        feed.push(GlanceItem::Event { title: "Standup".into(), when: "10:00".into() });
        feed.push(GlanceItem::Weather { place: "Tokyo".into(), temp: "24°".into(), hi: "27°".into(), lo: "19°".into(), cond: "Clear".into() });
        let titles: Vec<&str> = feed.items().map(GlanceItem::title).collect();
        assert_eq!(titles, ["Tokyo", "Standup", "Apps"]);
        // Reseeding the shell's cards leaves the posted ones alone.
        feed.seed(vec![GlanceItem::Note { title: "Apps".into(), body: "a, b, c".into() }, GlanceItem::Fetch { title: "Model".into(), progress: 0.4 }]);
        let titles: Vec<&str> = feed.items().map(GlanceItem::title).collect();
        assert_eq!(titles, ["Tokyo", "Standup", "Apps", "Model"]);
        assert!(matches!(feed.weather(), Some(GlanceItem::Weather { place, .. }) if place == "Tokyo"));
        assert!(matches!(feed.next_event(), Some(GlanceItem::Event { title, .. }) if title == "Standup"));
        assert_eq!(feed.column_height(12.0), 128.0 + 78.0 + 104.0 + 88.0 + 36.0);
        let mut p = PagesState { feed, date: "Monday, 14 September 2026".into(), ..Default::default() };
        assert_eq!(p.strip_text(), "Monday, 14 September 2026  ·  24° Clear  ·  Standup 10:00");
        // The column scrolls only as far as it overflows the screen.
        // 434 of cards under a 112 header on a 500 screen with 32 kept for
        // the system edge: 78 points hidden, all reachable by scrolling.
        p.scroll_glance(1000.0, 500.0);
        assert_eq!(p.glance_scroll, 78.0);
        p.scroll_glance(-1000.0, 500.0);
        assert_eq!(p.glance_scroll, 0.0);
        p.scroll_glance(50.0, 5000.0);
        assert_eq!(p.glance_scroll, 0.0, "a tall screen shows everything");
    }

    #[test]
    fn glance_scroll_reaches_the_last_control_above_the_keyboard() {
        let mut feed = GlanceFeed::default();
        feed.seed((0..8).map(|n| GlanceItem::Note { title: n.to_string(), body: "Long conversation".into() }).collect());
        let mut pages = PagesState { feed, ..Default::default() };
        for height in [800.0, 380.0, 260.0] {
            let screen = rect(8.0, 24.0, 353.0, height);
            let column = glance_column(screen, 0.0);
            pages.scroll_glance(10000.0, height);
            let last_bottom = screen.pos.y + GLANCE_HEADER + pages.feed.column_height(GLANCE_GAP) - pages.glance_scroll;
            assert_eq!(last_bottom, column.pos.y + column.size.y);
            assert_eq!(column.pos.y, screen.pos.y + GLANCE_HEADER);
            assert!(last_bottom <= screen.pos.y + screen.size.y - 32.0);
            let hidden_open = rect(280.0, screen.pos.y + 20.0, 28.0, 28.0);
            assert_eq!(tappable(hidden_open, column).size.y, 0.0, "a scrolled-off arrow cannot intercept the header");
        }
    }

    fn card(app: &str, id: &str, priority: i64, published_ms: u64) -> GlanceItem {
        GlanceItem::Card(GlanceCard {
            account: None,
            app: app.into(), card_id: id.into(), title: format!("{app}/{id}"), summary: String::new(), viewport: false, priority, published_ms, expires_ms: 0,
            open_app: app.trim_start_matches("os.").into(), route: None, body: "".into(), contained: true, digests: Vec::new(), l0: None,
        })
    }

    #[test]
    fn opening_a_workspace_keeps_feed_geometry_and_scroll_unchanged() {
        let mut pages = PagesState { index: -1.0, ..Default::default() };
        for n in 0..6 { pages.feed.push(card("os.mail", &n.to_string(), 0, n)); }
        let screen = rect(0.0, 24.0, 353.0, 400.0);
        pages.scroll_glance(90.0, 400.0);
        let height = pages.glance_height(400.0);
        let scroll = pages.glance_scroll;
        let before = pages.summary_rect("os.mail/4", screen).unwrap();
        pages.anchor_card("os.mail/4");
        pages.step_glance(0.3, 400.0, true, false);
        assert_eq!(pages.summary_rect("os.mail/4", screen), Some(before));
        assert_eq!(pages.glance_height(400.0), height);
        assert_eq!(pages.glance_scroll, scroll);
        assert!(pages.feed.items().all(|i| i.height() == SUMMARY_HEIGHT));
        pages.clear_card_anchor(400.0);
        assert_eq!(pages.glance_scroll, scroll);
        assert!(pages.workspace_source.is_none());
        assert!(pages.summary_rect("os.mail/missing", screen).is_none());
    }

    #[test]
    fn glance_flick_coasts_but_paused_lift_and_new_touch_stop_it() {
        let mut p = PagesState { index: -1.0, ..Default::default() };
        for n in 0..6 { p.feed.push(card("os.mail", &n.to_string(), 0, n)); }
        p.glance_touch(600.0, 1.0);
        for n in 1..=5 { p.glance_sample(600.0 - n as f64 * 20.0, 1.0 + n as f64 * 0.02); }
        p.glance_lift(1.11);
        assert!(p.glance_velocity > 900.0);
        assert!(p.step_glance(1.0/60.0, 400.0, true, false));
        assert!(p.glance_scroll > 0.0);
        for _ in 0..240 { p.step_glance(1.0/60.0, 400.0, true, false); }
        assert_eq!(p.glance_velocity, 0.0);
        let stopped = p.glance_scroll;
        p.glance_touch(500.0, 2.0);
        p.glance_sample(300.0, 2.1); p.glance_lift(2.4);
        assert_eq!(p.glance_velocity, 0.0, "holding before lift is not a fling");
        p.step_glance(0.016, 400.0, true, false);
        assert_eq!(p.glance_scroll, stopped);
        p.glance_velocity = -1000.0; p.glance_touch(400.0, 3.0);
        assert_eq!(p.glance_velocity, 0.0, "touching interrupts a coast");
    }

    #[test]
    fn published_cards_lead_the_feed_by_priority_then_recency() {
        let mut feed = GlanceFeed::default();
        feed.seed(vec![GlanceItem::Note { title: "Apps".into(), body: "a".into() }]);
        feed.push(GlanceItem::Event { title: "Standup".into(), when: "10:00".into() });
        feed.push(card("os.news", "digest", 50, 10));
        feed.push(card("os.maps", "commute", 50, 20));
        feed.push(card("os.mail", "inbox", 80, 5));
        let titles: Vec<&str> = feed.items().map(GlanceItem::title).collect();
        assert_eq!(titles, ["os.mail/inbox", "os.maps/commute", "os.news/digest", "Standup", "Apps"]);
        // The same (app, card_id) replaces: a newer digest moves up.
        feed.push(card("os.news", "digest", 50, 30));
        let titles: Vec<&str> = feed.items().map(GlanceItem::title).collect();
        assert_eq!(&titles[..3], ["os.mail/inbox", "os.news/digest", "os.maps/commute"]);
        assert_eq!(feed.cards().count(), 3);
        // Another app's card of the same id is a different card.
        feed.push(card("os.maps", "digest", 10, 40));
        assert_eq!(feed.cards().count(), 4);
        feed.withdraw("os.news", "digest");
        assert!(feed.cards().all(|c| c.key() != "os.news/digest"));
        assert_eq!(feed.cards().count(), 3);
        // Scrolling must reach older cards beyond the first six summaries.
        for i in 0..10 {
            feed.push(card("os.x", &format!("c{i}"), 60, 100 + i));
        }
        assert_eq!(feed.cards().count(), 13);
        assert_eq!(feed.cards().last().unwrap().key(), "os.maps/digest");
        // Summary height is independent of the generated card body.
        assert_eq!(card("os.y", "new", 1, 1).height(), SUMMARY_HEIGHT);
    }

    /// The feed keeps a published card as the glance service gives it, its
    /// L0 source included: that is what a tile keeps live.
    #[test]
    fn the_feed_keeps_a_cards_l0_source_for_its_tile() {
        let (_, _, source, data) = crate::glance::demo_mail().into_iter().find(|c| c.0 == "ups-lamp").unwrap();
        let GlanceItem::Card(mut published) = card("os.mail", "ups-lamp", 50, 1) else { unreachable!() };
        published.l0 = Some(std::sync::Arc::new(crate::glance::L0Source { source, data, mail: None }));
        let mut feed = GlanceFeed::default();
        feed.replace_cards(vec![published.clone()]);
        assert_eq!(feed.cards().next(), Some(&published));
        assert!(feed.cards().next().unwrap().l0.is_some());
    }

    fn gesture(start: Vec2d, hit: Option<PhoneHit>) -> PhoneGesture {
        PhoneGesture { start, last: start, time: 0.0, hit, shell: true, glance_scroll: false, screen: PhoneScreen::Home }
    }

    #[test]
    fn a_short_glance_feed_keeps_curved_left_swipes_available_at_every_height() {
        use crate::mobile_gestures::{FingerPhase, GestureContext, GestureRecognizer, ExclusionZones, SafeInsets};
        let screen = rect(0.0, 24.0, 384.0, 760.0);
        let ctx = GestureContext { screen, insets: SafeInsets::default(), phone: PhoneScreen::Home, body: true, glance: Some(glance_column(screen, 0.0)), system_edges: true, shade: true };
        for y in [170.0, 300.0, 500.0, 680.0] {
            for vertical in [-1.0, 1.0] {
                let mut feed = GlanceFeed::default();
                feed.push(GlanceItem::Note { title: "At a glance".into(), body: "No events yet".into() });
                let mut pages = PagesState { index: -1.0, feed, ..Default::default() };
                let mut g = gesture(dvec2(300.0, y), None);
                let mut recognizer = GestureRecognizer::default();
                let exclusions = ExclusionZones::default();
                recognizer.feed(FingerPhase::Down, g.start, 0.0, &ctx, &exclusions);
                // A thumb starts on a short vertical arc, then travels left.
                // A feed that cannot scroll must not permanently take it.
                for (i, dx) in [-4.0, -24.0, -80.0, -160.0, -210.0].into_iter().enumerate() {
                    let at = g.start + dvec2(dx, vertical * 14.0);
                    if pages.drag_glance(&mut g, at, screen, recognizer.current().is_some()) {
                        recognizer.cancel();
                    } else {
                        recognizer.feed(FingerPhase::Move, at, 0.04 * (i + 1) as f64, &ctx, &exclusions);
                    }
                    g.last = at;
                }
                assert_eq!(recognizer.feed(FingerPhase::Up, g.last, 0.25, &ctx, &exclusions),
                    Some(ShellGesture::Commit(GestureKind::Page(Dir::Left))), "start y={y}, arc={vertical}");
                assert_eq!(pages.glance_scroll, 0.0);
                assert!(!g.glance_scroll);
            }
        }
    }

    #[test]
    fn glance_finger_drags_reach_the_end_and_never_turn_back_into_a_tap() {
        use makepad_platform::event::TouchState;
        let mut feed = GlanceFeed::default();
        for i in 0..40 { feed.push(card("os.mail", &format!("m{i}"), 50, i)); }
        let mut pages = PagesState { feed, index: -1.0, ..Default::default() };
        let screen = rect(0.0, 24.0, 380.0, 700.0);
        let mut g = gesture(dvec2(180.0, 600.0), None);
        assert!(!pages.drag_glance(&mut g, dvec2(181.0, 598.0), screen, false));
        for y in [550.0, 400.0, 100.0, -20_000.0] {
            let at = dvec2(180.0, y);
            assert!(pages.drag_glance(&mut g, at, screen, false));
            g.last = at;
        }
        let end = pages.feed.column_height(GLANCE_GAP) + GLANCE_HEADER - screen.size.y + GLANCE_BOTTOM;
        assert_eq!(pages.glance_scroll, end, "finger drags reach the last control, just like a wheel");
        let start = g.start;
        assert!(pages.drag_glance(&mut g, start, screen, false), "reversing direction retains ownership");
        assert_eq!(GlanceFinger::of(&touch(TouchState::Stop, g.start, 7), Some(&g), false, Some(7)), GlanceFinger::NotATap);
        assert!(!g.shell);
    }

    #[test]
    fn glance_drag_leaves_headers_controls_and_horizontal_paging_alone() {
        let mut feed = GlanceFeed::default();
        for i in 0..10 { feed.push(card("os.mail", &format!("m{i}"), 50, i)); }
        let mut pages = PagesState { feed, index: -1.0, ..Default::default() };
        let screen = rect(0.0, 0.0, 380.0, 700.0);
        for (start, delta, hit, claimed) in [
            (dvec2(180.0, 60.0), dvec2(0.0, 100.0), None, false),
            (dvec2(180.0, 500.0), dvec2(100.0, 20.0), None, false),
            (dvec2(180.0, 500.0), dvec2(0.0, 100.0), Some(PhoneHit::ExpandGlance("mail/card".into())), false),
            (dvec2(180.0, 500.0), dvec2(0.0, 100.0), None, true),
        ] {
            let mut g = gesture(start, hit);
            assert!(!pages.drag_glance(&mut g, start + delta, screen, claimed));
            assert!(!g.glance_scroll);
        }
        let mut g = gesture(dvec2(180.0, 500.0), None);
        pages.index = 0.0;
        assert!(!pages.drag_glance(&mut g, dvec2(180.0, 400.0), screen, false));
    }

    #[test]
    fn slow_glance_drags_claim_before_home_search_or_shade() {
        use crate::mobile_gestures::{FingerPhase, GestureContext, GestureRecognizer, ExclusionZones, SafeInsets};
        let screen = rect(0.0, 0.0, 380.0, 700.0);
        let ctx = GestureContext { screen, insets: SafeInsets::default(), phone: PhoneScreen::Home, body: true, glance: Some(glance_column(screen, 0.0)), system_edges: true, shade: true };
        for x in [50.0, 190.0, 330.0] {
            for direction in [-1.0, 1.0] {
                let mut feed = GlanceFeed::default();
                for i in 0..10 { feed.push(card("os.mail", &format!("m{i}"), 50, i)); }
                let mut pages = PagesState { feed, index: -1.0, ..Default::default() };
                let mut g = gesture(dvec2(x, 400.0), None);
                let mut recognizer = GestureRecognizer::default();
                let exclusions = ExclusionZones::default();
                recognizer.feed(FingerPhase::Down, g.start, 0.0, &ctx, &exclusions);
                for distance in [4.0, 8.0, 11.0, 30.0, 80.0] {
                    let at = g.start + dvec2(0.0, direction * distance);
                    if pages.drag_glance(&mut g, at, screen, recognizer.current().is_some()) {
                        recognizer.cancel();
                    } else {
                        assert!(recognizer.feed(FingerPhase::Move, at, distance / 100.0, &ctx, &exclusions).is_none());
                    }
                    g.last = at;
                }
                assert!(g.glance_scroll);
                assert!(!recognizer.active());
            }
        }
    }
    fn mouse_up(abs: Vec2d) -> Event {
        Event::MouseUp(MouseUpEvent { abs, button: MouseButton::PRIMARY, window_id: CxWindowPool::id_zero(), modifiers: Default::default(), time: 0.0 })
    }
    fn touch(state: makepad_platform::event::TouchState, abs: Vec2d, uid: u64) -> Event {
        use makepad_platform::event::{TouchPoint, TouchUpdateEvent};
        Event::TouchUpdate(TouchUpdateEvent {
            time: 0.0, window_id: CxWindowPool::id_zero(), modifiers: Default::default(),
            touches: vec![TouchPoint { state, abs, time: 0.0, uid, rotation_angle: 0.0, force: 0.0, radius: dvec2(1.0, 1.0), handled: Default::default(), sweep_lock: Default::default() }],
        })
    }

    #[test]
    fn only_a_plain_taps_lift_is_a_tap_for_a_card() {
        use makepad_platform::event::TouchState;
        let at = dvec2(120.0, 300.0);
        let g = gesture(at, None);
        assert_eq!(GlanceFinger::of(&mouse_up(at + dvec2(3.0, 2.0)), Some(&g), false, None), GlanceFinger::Tap(at));
        assert_eq!(GlanceFinger::of(&mouse_up(at + dvec2(11.0, 0.0)), Some(&g), true, None), GlanceFinger::NotATap, "a page swipe the recognizer took, short as it was");
        assert_eq!(GlanceFinger::of(&mouse_up(at + dvec2(0.0, -40.0)), Some(&g), false, None), GlanceFinger::NotATap, "a drag no gesture took");
        for hit in [PhoneHit::Glance("mail".into()), PhoneHit::ExpandGlance("os.mail/reply-42".into())] {
            let open = gesture(at, Some(hit));
            assert_eq!(GlanceFinger::of(&mouse_up(at), Some(&open), false, None), GlanceFinger::NotATap, "the shell arrow must not also click the card below");
        }
        assert_eq!(GlanceFinger::of(&mouse_up(at), None, false, None), GlanceFinger::NotATap, "a long press took the finger");
        let elsewhere = PhoneGesture { screen: PhoneScreen::Drawer, ..g.clone() };
        assert_eq!(GlanceFinger::of(&mouse_up(at), Some(&elsewhere), false, None), GlanceFinger::NotATap);
        // The touch the shell follows; another finger's lift, a press and a
        // move are no tap.
        assert_eq!(GlanceFinger::of(&touch(TouchState::Stop, at, 7), Some(&g), false, Some(7)), GlanceFinger::Tap(at));
        assert_eq!(GlanceFinger::of(&touch(TouchState::Stop, at, 8), Some(&g), false, Some(7)), GlanceFinger::NotATap);
        assert_eq!(GlanceFinger::of(&touch(TouchState::Start, at, 7), Some(&g), false, Some(7)), GlanceFinger::NotATap);
        assert_eq!(GlanceFinger::of(&touch(TouchState::Move, at, 7), Some(&g), false, Some(7)), GlanceFinger::NotATap);
        let down = Event::MouseDown(MouseDownEvent { abs: at, button: MouseButton::PRIMARY, window_id: CxWindowPool::id_zero(), modifiers: Default::default(), handled: Default::default(), time: 0.0 });
        assert_eq!(GlanceFinger::of(&down, Some(&g), false, None), GlanceFinger::NotATap);
        let typed = Event::TextInput(TextInputEvent { input: "hi".into(), ..Default::default() });
        assert_eq!(GlanceFinger::of(&typed, Some(&g), true, None), GlanceFinger::NotPointer);
        assert_eq!(GlanceFinger::of(&Event::Signal, None, false, None), GlanceFinger::NotPointer);
    }

    /// A card is tappable only where the column shows it: a tall card's part
    /// under the page indicator and the dock is theirs.
    #[test]
    fn a_tile_is_tappable_only_where_the_column_shows_it() {
        let column = rect(0.0, 0.0, 400.0, 776.0);
        assert_eq!(tappable(rect(20.0, 160.0, 360.0, 300.0), column), rect(20.0, 160.0, 360.0, 300.0));
        assert_eq!(tappable(rect(20.0, 600.0, 360.0, 440.0), column), rect(20.0, 600.0, 360.0, 176.0));
        let cards = GlanceCards { drawn: vec![("os.a/x".into(), tappable(rect(20.0, 600.0, 360.0, 440.0), column))], ..Default::default() };
        assert_eq!(cards.under(dvec2(100.0, 700.0)), Some("os.a/x"));
        assert_eq!(cards.under(dvec2(100.0, 800.0)), None, "under the dock");
        let gone = GlanceCards { drawn: vec![("os.a/x".into(), tappable(rect(20.0, 800.0, 360.0, 100.0), column))], ..Default::default() };
        assert_eq!(gone.under(dvec2(100.0, 776.0)), None, "nothing of it shows");
    }

}
