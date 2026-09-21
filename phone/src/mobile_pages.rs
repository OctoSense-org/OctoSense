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
use crate::{
    desktop::DesktopStyle,
    mobile::{PhoneHit, PhoneState},
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
        }
    }
    pub fn title(&self) -> &str {
        match self {
            GlanceItem::Weather { place, .. } => place,
            GlanceItem::Event { title, .. } | GlanceItem::Fetch { title, .. } | GlanceItem::Note { title, .. } => title,
        }
    }
}

/// The glance page's data: what the shell itself knows (refreshed by
/// `sync`) under whatever apps posted (newest first).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlanceFeed {
    posted: Vec<GlanceItem>,
    shell: Vec<GlanceItem>,
}

impl GlanceFeed {
    /// An app posted a card: it goes on top, above everything older.
    /// TODO(ai_bus): the AppCard module's ServiceExecutor delivers its cards
    /// here (`glance.push` on the os service); nothing is wired yet.
    pub fn push(&mut self, item: GlanceItem) {
        self.posted.insert(0, item);
    }
    /// Replace the shell's own cards, leaving posted ones alone.
    pub fn seed(&mut self, items: Vec<GlanceItem>) {
        self.shell = items;
    }
    pub fn items(&self) -> impl Iterator<Item = &GlanceItem> {
        self.posted.iter().chain(self.shell.iter())
    }
    pub fn len(&self) -> usize {
        self.posted.len() + self.shell.len()
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

/// How much of a page one full-progress swipe drags before it commits: the
/// recognizer's commit distance is a fraction of the screen, so the page
/// follows the finger at about that rate rather than flying across.
pub const SWIPE_FRACTION: f64 = 0.35;

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
    /// Today's date as the glance header shows it.
    pub date: String,
}

impl Default for PagesState {
    fn default() -> Self {
        Self { pages: Vec::new(), index: 0.0, drag: 0.0, velocity: 0.0, target: 0, open_library: false, pending: None, feed: GlanceFeed::default(), glance_scroll: 0.0, date: String::new() }
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
        if pages != self.pages {
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
        self.index = self.index.clamp(-1.0, last as f64);
    }

    /// Go to page `n` (-1 is the glance page; the library's position and
    /// anything past it open the library). Before the pages are known the
    /// jump waits for `sync`, and lands on an apps page.
    pub fn jump(&mut self, n: i64) {
        self.drag = 0.0;
        if !self.known() { self.pending = Some(n); return; }
        let lib = self.library_index();
        self.target = n.clamp(-1, lib);
        if self.target == lib { self.open_library = true; }
    }

    /// Drive the pager one frame from the shell gesture (None when there is
    /// none, or the home page is not the screen). True while animating.
    pub fn step(&mut self, dt: f64, gesture: Option<ShellGesture>) -> bool {
        let lib = self.library_index() as f64;
        let mut dragging = false;
        match gesture {
            Some(ShellGesture::PageSwipe { dir, progress }) => {
                // A finger moving left reveals the page on the right.
                let sign = match dir { Dir::Left => 1.0, Dir::Right => -1.0 };
                let raw = sign * progress.max(0.0) * SWIPE_FRACTION;
                let (lo, hi) = ((-1.0 - self.index).min(0.0), (lib - self.index).max(0.0));
                // Past either end the page gives a little and stiffens, so
                // the end is felt rather than hit (it springs back on lift).
                let over = if raw < lo { raw - lo } else if raw > hi { raw - hi } else { 0.0 };
                self.drag = raw.clamp(lo, hi) + over.signum() * 0.16 * (1.0 - (-over.abs() / 0.16).exp());
                dragging = true;
            }
            Some(ShellGesture::Commit(GestureKind::Page(dir))) => {
                let step = match dir { Dir::Left => 1, Dir::Right => -1 };
                // Carry on from where the finger left the page.
                self.index += self.drag;
                self.drag = 0.0;
                self.target = (self.index.round() as i64 + step).clamp(-1, lib as i64);
                if self.known() && self.target == lib as i64 { self.open_library = true; }
            }
            Some(ShellGesture::Cancel(GestureKind::Page(_))) => {
                self.target = self.index.round() as i64;
            }
            _ => {}
        }
        if dragging { self.velocity = 0.0; return true; }
        let t = 1.0 - (-dt * 16.0).exp();
        self.drag += (0.0 - self.drag) * t;
        if self.drag.abs() < 0.001 { self.drag = 0.0; }
        let target = self.target as f64;
        // Damping ratio 0.8: about a third of a page per second of overshoot
        // at most, gone within a quarter second.
        let k: f64 = 420.0;
        let c = 2.0 * k.sqrt() * 0.8;
        let dt = dt.min(1.0 / 30.0);
        self.velocity += ((target - self.index) * k - self.velocity * c) * dt;
        self.index += self.velocity * dt;
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

    /// Scroll the glance column by `dy` points on a screen `height` tall.
    pub fn scroll_glance(&mut self, dy: f64, height: f64) {
        let max = (self.feed.column_height(GLANCE_GAP) + GLANCE_HEADER - (height - GLANCE_BOTTOM)).max(0.0);
        self.glance_scroll = (self.glance_scroll + dy).clamp(0.0, max);
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

/// Points above the glance column (its header) and below it (the dock).
const GLANCE_HEADER: f64 = 96.0;
const GLANCE_BOTTOM: f64 = 124.0;
const GLANCE_GAP: f64 = 12.0;

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

// ---------------------------------------------------------------- drawing

impl PhoneSurface {
    /// The glance page at horizontal offset `dx`: a dimmed wallpaper and a
    /// scrollable column of frosted cards under an "At a glance" header.
    pub fn draw_glance(&mut self, cx: &mut Cx2d, phone: &PhoneState, screen: Rect, style: DesktopStyle, dark: bool, opacity: f32, dx: f64) {
        self.use_fonts(style == DesktopStyle::Ios);
        let page = rect(screen.pos.x + dx, screen.pos.y, screen.size.x, screen.size.y);
        self.rounded(cx, page, 0.0, alpha(if dark { rgb(8, 9, 16) } else { rgb(228, 231, 242) }, 0.86 * opacity));
        let ink = alpha(if dark { rgb(255, 255, 255) } else { rgb(26, 26, 32) }, opacity);
        let landscape = screen.size.x > screen.size.y;
        let top = page.pos.y + if landscape { 30.0 } else { 52.0 };
        let left = page.pos.x + 20.0;
        let width = page.size.x - 40.0;
        self.d.label_elided(cx, rect(left, top, width, 30.0), true, 24.0, ink, HAlign::Left, "At a glance");
        self.d.label_elided(cx, rect(left, top + 32.0, width, 20.0), false, 13.0, alpha(ink, 0.7 * opacity), HAlign::Left, &phone.pages.date);
        let bottom = screen.pos.y + screen.size.y - GLANCE_BOTTOM;
        let mut y = top + 64.0 - phone.pages.glance_scroll;
        let items: Vec<GlanceItem> = phone.pages.feed.items().cloned().collect();
        for item in &items {
            let h = item.height();
            if y + h > top + 56.0 && y < bottom {
                self.draw_glance_card(cx, rect(left, y, width, h), item, style, dark, ink, opacity);
            }
            y += h + GLANCE_GAP;
        }
    }

    fn draw_glance_card(&mut self, cx: &mut Cx2d, r: Rect, item: &GlanceItem, style: DesktopStyle, dark: bool, ink: Vec4f, opacity: f32) {
        self.rounded(cx, r, 18.0, alpha(rgb(255, 255, 255), if dark { 0.10 } else { 0.55 } * opacity));
        let pad = 16.0;
        let inner = rect(r.pos.x + pad, r.pos.y + pad, r.size.x - pad * 2.0, r.size.y - pad * 2.0);
        let dim = alpha(ink, 0.65 * opacity);
        let accent = if style == DesktopStyle::Ios { rgb(0, 122, 255) } else { rgb(103, 80, 164) };
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
        self.rounded(cx, page, 0.0, alpha(if dark { rgb(8, 9, 16) } else { rgb(228, 231, 242) }, 0.86 * opacity));
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
    fn swipes_commit_cancel_and_clamp_at_both_ends() {
        let mut p = PagesState::default();
        p.sync(&ids(12), 8, 12);
        // A swipe left drags the page after it in; committing lands on it.
        assert!(p.step(1.0 / 60.0, Some(ShellGesture::PageSwipe { dir: Dir::Left, progress: 0.5 })));
        assert!((p.drag - 0.5 * SWIPE_FRACTION).abs() < 1e-9);
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
        // 434 of cards under a 96 header on a 500 screen with 124 kept for
        // the dock: 154 points hidden.
        p.scroll_glance(1000.0, 500.0);
        assert_eq!(p.glance_scroll, 434.0 + 96.0 - (500.0 - 124.0));
        p.scroll_glance(-1000.0, 500.0);
        assert_eq!(p.glance_scroll, 0.0);
        p.scroll_glance(50.0, 5000.0);
        assert_eq!(p.glance_scroll, 0.0, "a tall screen shows everything");
    }
}
