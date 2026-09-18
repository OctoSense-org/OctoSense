//! Tile groups on the phone home page: a group tile opens as a sub-window
//! holding its members' tiles (live captures where a member runs); a group
//! of exactly two doubles as an app pair, which "Open both" runs as a split
//! screen (two live clients, top/bottom in portrait, side by side in
//! landscape). Recents enters the same split from two cards. The model,
//! the animation state and the geometry live here, pure and tested; the
//! drawing and the App-side transitions sit in `impl` blocks below so the
//! shared files only carry one-line hooks.
//!
//! Gesture contract (mobile_gestures.rs): a committed `HomeUp` leaves the
//! split like leaving an app; the divider's band is an exclusion zone, so
//! no shell gesture starts on it.
use crate::{hub::ClientId, mobile::{self, PhoneHit, PhoneScreen, PhoneState}, mobile_gestures::{GestureKind, ShellGesture}, mobile_tiles::{TileKind, TileSlot, TILE_RADIUS}};
use makepad_widgets::*;

/// The groups every phone starts with, from the linked apps. A member that
/// is not in this build's catalog is simply absent from the face; a group
/// with fewer than two members present is not placed at all (one app is
/// just that app's icon).
/// TODO: read groups from the `.octosense` settings next to the catalog
/// `apps.rs` loads, so people can make their own.
pub const SEED_GROUPS: [(&str, &[&str]); 2] = [
    ("Work", &["sheets", "reference"]),
    ("Media", &["photos", "appcard"]),
];

/// The pairs in effect: the person's own (Android's placements journal,
/// `set_seeds`) or the seeds above. Names are interned so the layout can
/// keep naming tiles by `&'static str`.
static SEEDS: std::sync::RwLock<Option<Vec<(&'static str, Vec<String>)>>> = std::sync::RwLock::new(None);
static NAMES: std::sync::RwLock<Vec<&'static str>> = std::sync::RwLock::new(Vec::new());
fn intern(name: &str) -> &'static str {
    if let Some(known) = NAMES.read().unwrap().iter().find(|n| **n == name) { return known; }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    NAMES.write().unwrap().push(leaked);
    leaked
}
pub fn seeds() -> Vec<(&'static str, Vec<String>)> {
    if let Some(own) = SEEDS.read().unwrap().as_ref() { return own.clone(); }
    SEED_GROUPS.iter().map(|(name, apps)| (*name, apps.iter().map(|a| a.to_string()).collect())).collect()
}
/// The person's pairs and folders (None: back to the seeds). A name is at
/// most 32 characters and a group two to eight distinct apps; anything
/// else is dropped.
pub fn set_seeds(pairs: Option<&[(String, Vec<String>)]>) {
    *SEEDS.write().unwrap() = pairs.map(|pairs| pairs.iter()
        .filter(|(name, apps)| !name.is_empty() && name.chars().count() <= 32 && (2..=8).contains(&apps.len())
            && apps.iter().enumerate().all(|(i, a)| !apps[..i].contains(a)))
        .map(|(name, apps)| (intern(name), apps.clone())).collect());
}
/// A name for a new folder that no group has yet: the two labels, then a
/// number if that is taken.
pub fn fresh_name(first: &str, second: &str) -> String {
    let base: String = format!("{first} & {second}").chars().take(28).collect();
    let taken = seeds();
    if !taken.iter().any(|(n, _)| *n == base) { return base; }
    (2..100).map(|k| format!("{base} {k}")).find(|n| !taken.iter().any(|(t, _)| t == n)).unwrap_or(base)
}

/// Height of a group tile in portrait: a chip under the app tiles that
/// still leaves the favorites their rows.
pub const GROUP_TILE_HEIGHT: f64 = 76.0;
/// The divider's ratio is kept away from the edges so neither app is
/// squeezed to nothing.
pub const SPLIT_MIN_RATIO: f64 = 0.25;
pub const SPLIT_MAX_RATIO: f64 = 0.75;
/// The gap between the two panes, and the band around it that drags the
/// divider (and excludes shell gestures).
pub const SPLIT_GAP: f64 = 8.0;
pub const DIVIDER_BAND: f64 = 32.0;

/// A named set of apps. `pair` is derived: exactly two members.
#[derive(Clone, Debug, PartialEq)]
pub struct TileGroup {
    pub name: String,
    pub apps: Vec<String>,
    pub pair: bool,
}

impl TileGroup {
    pub fn new(name: &str, apps: &[&str]) -> Self {
        let mut group = Self { name: name.to_string(), apps: apps.iter().map(|a| a.to_string()).collect(), pair: false };
        group.refresh();
        group
    }
    fn refresh(&mut self) {
        self.apps.dedup();
        self.pair = self.apps.len() == 2;
    }
    pub fn add(&mut self, app: &str) {
        if !self.apps.iter().any(|a| a == app) { self.apps.push(app.to_string()); }
        self.refresh();
    }
    pub fn remove(&mut self, app: &str) {
        self.apps.retain(|a| a != app);
        self.refresh();
    }
    /// The members this catalog can launch, in group order.
    pub fn present<'a>(&'a self, available: &[&str]) -> Vec<&'a str> {
        self.apps.iter().map(String::as_str).filter(|a| available.contains(a)).collect()
    }
}

/// Two clients sharing the app area. `left` is the first pane (top in
/// portrait); `ratio` is its share of the area along the split axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Split {
    pub left: ClientId,
    pub right: ClientId,
    pub ratio: f64,
}

impl Split {
    pub fn new(left: ClientId, right: ClientId) -> Self { Self { left, right, ratio: 0.5 } }
    pub fn has(&self, client: ClientId) -> bool { self.left == client || self.right == client }
    pub fn set_ratio(&mut self, ratio: f64) { self.ratio = ratio.clamp(SPLIT_MIN_RATIO, SPLIT_MAX_RATIO); }
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.left, &mut self.right);
        self.ratio = 1.0 - self.ratio;
    }
    /// Portrait splits top/bottom, landscape left/right.
    pub fn vertical(app: Rect) -> bool { app.size.x <= app.size.y }
    /// The pane `client` gets of `app`, or None when it is not in the split.
    pub fn pane(&self, client: ClientId, app: Rect) -> Option<Rect> {
        let first = if client == self.left { true } else if client == self.right { false } else { return None };
        let half = SPLIT_GAP * 0.5;
        Some(if Self::vertical(app) {
            let cut = app.size.y * self.ratio;
            if first { Rect { pos: app.pos, size: dvec2(app.size.x, (cut - half).max(1.0)) } }
            else { Rect { pos: app.pos + dvec2(0.0, cut + half), size: dvec2(app.size.x, (app.size.y - cut - half).max(1.0)) } }
        } else {
            let cut = app.size.x * self.ratio;
            if first { Rect { pos: app.pos, size: dvec2((cut - half).max(1.0), app.size.y) } }
            else { Rect { pos: app.pos + dvec2(cut + half, 0.0), size: dvec2((app.size.x - cut - half).max(1.0), app.size.y) } }
        })
    }
    /// The band across the divider that drags it.
    pub fn divider(&self, app: Rect) -> Rect {
        if Self::vertical(app) {
            Rect { pos: dvec2(app.pos.x, app.pos.y + app.size.y * self.ratio - DIVIDER_BAND * 0.5), size: dvec2(app.size.x, DIVIDER_BAND) }
        } else {
            Rect { pos: dvec2(app.pos.x + app.size.x * self.ratio - DIVIDER_BAND * 0.5, app.pos.y), size: dvec2(DIVIDER_BAND, app.size.y) }
        }
    }
    /// The finger is at `p`: the divider follows it. Dragging past the
    /// far side of the other pane swaps the two.
    pub fn drag_to(&mut self, p: Vec2d, app: Rect) {
        let raw = if Self::vertical(app) { (p.y - app.pos.y) / app.size.y.max(1.0) } else { (p.x - app.pos.x) / app.size.x.max(1.0) };
        self.set_ratio(raw);
    }
}

/// The groups on this phone and everything animated around them.
#[derive(Clone, Debug)]
pub struct GroupsState {
    pub groups: Vec<TileGroup>,
    /// The group whose sub-window is open (or closing while `openness` falls).
    pub open: Option<String>,
    /// 0 = tile, 1 = the sub-window fully grown.
    pub openness: f64,
    pub split: Option<Split>,
    /// Recents: the first card picked for a split, waiting for a second.
    pub pick: Option<ClientId>,
    /// The tile the open sub-window grew out of, remembered while it closes
    /// (the home layout may not be at hand then).
    pub origin: Option<Rect>,
    /// The last group name that was closing, so `openness` keeps a target.
    closing: Option<String>,
}

impl Default for GroupsState {
    fn default() -> Self {
        Self {
            groups: seeds().iter().map(|(name, apps)| TileGroup::new(name, &apps.iter().map(String::as_str).collect::<Vec<_>>())).collect(),
            open: None, openness: 0.0, split: None, pick: None, origin: None, closing: None,
        }
    }
}

impl GroupsState {
    /// The pairs changed (`set_seeds`): rebuild the groups, closing a window
    /// whose group is gone.
    pub fn reseed(&mut self) {
        self.groups = seeds().iter().map(|(name, apps)| TileGroup::new(name, &apps.iter().map(String::as_str).collect::<Vec<_>>())).collect();
        if self.open.as_deref().is_some_and(|name| self.get(name).is_none()) { self.close(); }
    }
    pub fn get(&self, name: &str) -> Option<&TileGroup> { self.groups.iter().find(|g| g.name == name) }
    pub fn get_mut(&mut self, name: &str) -> Option<&mut TileGroup> { self.groups.iter_mut().find(|g| g.name == name) }
    /// The group whose window is on screen: the open one, or the one still
    /// shrinking back into its tile.
    pub fn shown(&self) -> Option<&TileGroup> {
        self.open.as_deref().or(self.closing.as_deref()).and_then(|n| self.get(n))
    }
    pub fn is_open(&self, name: &str) -> bool { self.open.as_deref() == Some(name) }
    /// Names of the seeded groups this catalog can show (two or more
    /// members present), in seed order.
    pub fn placed(available: &[&str]) -> Vec<&'static str> {
        seeds().iter().filter(|(_, apps)| apps.iter().filter(|a| available.contains(&a.as_str())).count() >= 2).map(|(name, _)| *name).collect()
    }
    pub fn open_group(&mut self, name: &str, tile: Rect) {
        if self.get(name).is_none() { return; }
        self.open = Some(name.to_string());
        self.closing = None;
        self.origin = Some(tile);
    }
    pub fn close(&mut self) {
        if let Some(name) = self.open.take() { self.closing = Some(name); }
    }
    pub fn in_split(&self, client: ClientId) -> bool { self.split.is_some_and(|s| s.has(client)) }
    /// The rect `client` draws in: its pane of `app` inside a split, the
    /// whole of `app` otherwise.
    pub fn pane(&self, client: ClientId, app: Rect) -> Rect {
        self.split.and_then(|s| s.pane(client, app)).unwrap_or(app)
    }
    pub fn pane_size(&self, client: ClientId, full: Vec2d) -> Vec2d {
        self.pane(client, Rect { pos: dvec2(0.0, 0.0), size: full }).size
    }
    pub fn enter_split(&mut self, left: ClientId, right: ClientId) -> bool {
        if left == right { return false; }
        self.split = Some(Split::new(left, right));
        self.pick = None;
        self.close();
        true
    }
    pub fn leave_split(&mut self) { self.split = None; self.pick = None; }
    /// Recents: a card's split button was tapped. The first tap remembers
    /// the card; the second (on another card) makes the split. Returns the
    /// pair to enter, if this tap completed one.
    pub fn pick_card(&mut self, client: ClientId) -> Option<(ClientId, ClientId)> {
        match self.pick {
            Some(first) if first != client => { self.pick = None; Some((first, client)) }
            Some(_) => { self.pick = None; None }
            None => { self.pick = Some(client); None }
        }
    }
    /// The divider's drag: the finger is at `p` over the app area `app`.
    pub fn drag_divider(&mut self, p: Vec2d, app: Rect) {
        if let Some(split) = self.split.as_mut() { split.drag_to(p, app); }
    }
    /// A shell gesture was recognised: a committed HomeUp leaves the split.
    pub fn on_gesture(&mut self, gesture: &ShellGesture) {
        if matches!(gesture, ShellGesture::Commit(GestureKind::HomeUp)) { self.leave_split(); }
    }
    /// Keep the split tied to the foreground: leaving the app screen, or
    /// bringing a third client in front, ends it (the panes go back to
    /// full size on their next frame).
    pub fn follow(&mut self, screen: PhoneScreen, client: Option<ClientId>) {
        if let Some(split) = self.split {
            let stays = screen == PhoneScreen::App && client.is_some_and(|c| split.has(c));
            if !stays { self.leave_split(); }
        }
        if screen != PhoneScreen::Recents { self.pick = None; }
        if screen != PhoneScreen::Home && self.open.is_some() { self.close(); }
    }
    /// The divider band as an exclusion zone. The desk clears the zones
    /// once per frame and every surface adds its own, so this only adds.
    pub fn add_exclusions(&self, screen: PhoneScreen, app: Rect, zones: &mut crate::mobile_gestures::ExclusionZones) {
        if screen != PhoneScreen::App { return; }
        if let Some(split) = self.split { zones.add(split.divider(app), [true; 4]); }
    }
    /// Animate the sub-window. True while it is still moving.
    pub fn step(&mut self, dt: f64) -> bool {
        let target = if self.open.is_some() { 1.0 } else { 0.0 };
        let t = 1.0 - (-dt * 16.0).exp();
        self.openness += (target - self.openness) * t;
        if (self.openness - target).abs() < 0.002 { self.openness = target; }
        if self.openness == 0.0 { self.closing = None; self.origin = None; }
        self.openness != target
    }
    pub fn window_visible(&self) -> bool { self.openness > 0.001 }
}

/// Everything the animation frame needs from the phone state, in one call.
/// (The divider's exclusion zone is added at draw time, by the desk.)
pub fn follow(phone: &mut PhoneState) {
    if let Some(gesture) = phone.gesture_out.as_ref() { phone.groups.on_gesture(gesture); }
    phone.groups.follow(phone.screen, phone.client);
}

// --------------------------------------------------------------------
// Geometry of the sub-window
// --------------------------------------------------------------------

/// One member's cell in the open group window.
#[derive(Clone, Debug, PartialEq)]
pub struct MemberCell { pub app: String, pub rect: Rect }

/// The group window at one moment of its animation.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupWindow {
    pub panel: Rect,
    pub title: Rect,
    pub cells: Vec<MemberCell>,
    /// The pair's "Open both" button, once it is a pair with both present.
    pub open_both: Option<Rect>,
    pub t: f64,
}

fn ease(t: f64) -> f64 { let t = t.clamp(0.0, 1.0); 1.0 - (1.0 - t) * (1.0 - t) }

/// The window `group` grows into on `screen`, at animation `t` (0 = the
/// tile it came from, 1 = fully open). Members sit two across.
pub fn group_window(screen: Rect, tile: Rect, members: &[&str], pair: bool, t: f64) -> GroupWindow {
    let landscape = screen.size.x > screen.size.y;
    let margin = if landscape { 60.0 } else { 22.0 };
    let width = (screen.size.x - margin * 2.0).min(if landscape { 560.0 } else { 420.0 });
    let columns = if landscape { members.len().clamp(1, 3) } else { members.len().clamp(1, 2) };
    let gap = 12.0;
    let pad = 16.0;
    let cell_w = (width - pad * 2.0 - gap * (columns as f64 - 1.0)) / columns as f64;
    let cell_h = (cell_w * if landscape { 0.62 } else { 0.78 }).round();
    let rows = (members.len().max(1) + columns - 1) / columns;
    let title_h = 46.0;
    let button_h = if pair && members.len() == 2 { 52.0 } else { 0.0 };
    let height = title_h + pad + rows as f64 * (cell_h + gap) - gap + button_h + pad;
    let height = height.min(screen.size.y - 100.0).max(120.0);
    let full = Rect { pos: dvec2(screen.pos.x + (screen.size.x - width) * 0.5, screen.pos.y + (screen.size.y - height) * 0.5 - 20.0), size: dvec2(width, height) };
    let panel = mobile::mix_rect(tile, full, ease(t));
    let scale = (panel.size.x / full.size.x).max(0.01);
    let map = |r: Rect| Rect { pos: panel.pos + (r.pos - full.pos) * scale, size: r.size * scale };
    let title = map(Rect { pos: full.pos + dvec2(pad, 0.0), size: dvec2(width - pad * 2.0, title_h) });
    let cells = members.iter().enumerate().map(|(i, app)| {
        let col = (i % columns) as f64;
        let row = (i / columns) as f64;
        let r = Rect { pos: full.pos + dvec2(pad + col * (cell_w + gap), title_h + pad * 0.5 + row * (cell_h + gap)), size: dvec2(cell_w, cell_h) };
        MemberCell { app: app.to_string(), rect: map(r) }
    }).collect();
    let open_both = (button_h > 0.0).then(|| map(Rect { pos: full.pos + dvec2(pad, height - pad - button_h + 6.0), size: dvec2(width - pad * 2.0, button_h - 6.0) }));
    GroupWindow { panel, title, cells, open_both, t }
}

/// Fit `size`'s aspect inside `cell`, centred.
pub fn fit(cell: Rect, size: Vec2d) -> Rect {
    if size.x < 1.0 || size.y < 1.0 { return cell; }
    let scale = (cell.size.x / size.x).min(cell.size.y / size.y);
    let fitted = size * scale;
    Rect { pos: cell.pos + (cell.size - fitted) * 0.5, size: fitted }
}

/// The group tile in `tiles`, if it is placed.
pub fn tile_of<'a>(tiles: &'a [TileSlot], name: &str) -> Option<&'a TileSlot> {
    tiles.iter().find(|s| matches!(s.kind, TileKind::Group(n) if n == name))
}

// --------------------------------------------------------------------
// Drawing: the group face on the home page and the overlay chrome
// --------------------------------------------------------------------

use crate::{desk::WmDesk, desktop::DesktopStyle, mobile_surface::PhoneSurface, shell::{alpha, rgb, ui::{rect, HAlign}}};

impl PhoneSurface {
    /// The group's face on the home page: a 2x2 mosaic of its members'
    /// icons beside its name. Tapping it opens the sub-window.
    pub(crate) fn draw_group_tile(&mut self, cx: &mut Cx2d, groups: &GroupsState, slot: TileSlot, available: &[&str], style: DesktopStyle, dark: bool, opacity: f32) {
        let TileKind::Group(name) = slot.kind else { return };
        let Some(group) = groups.get(name) else { return };
        let members = group.present(available);
        let r = slot.rect;
        let ios = style == DesktopStyle::Ios;
        let face = if dark { rgb(30, 32, 46) } else if ios { rgb(246, 247, 252) } else { rgb(255, 251, 255) };
        let ink = if dark { rgb(240, 240, 248) } else { rgb(28, 27, 36) };
        let pressed = self.pressed_hit() == Some(&PhoneHit::Group(name.to_string()));
        self.rounded(cx, r, TILE_RADIUS as f32, alpha(face, (if pressed { 0.95 } else { 0.82 }) * opacity));
        let mosaic = (r.size.y - 20.0).clamp(24.0, 64.0);
        let mx = r.pos.x + 14.0;
        let my = r.pos.y + (r.size.y - mosaic) * 0.5;
        self.rounded(cx, rect(mx - 4.0, my - 4.0, mosaic + 8.0, mosaic + 8.0), 12.0, alpha(ink, 0.08 * opacity));
        let icon = (mosaic - 4.0) * 0.5;
        for (i, app) in members.iter().take(4).enumerate() {
            let ir = rect(mx + (i % 2) as f64 * (icon + 4.0), my + (i / 2) as f64 * (icon + 4.0), icon, icon);
            self.icons.draw(cx, app, style, ir, opacity, alpha(ink, opacity));
        }
        let text_x = mx + mosaic + 18.0;
        let text_w = (r.pos.x + r.size.x - text_x - 10.0).max(10.0);
        let mid = r.pos.y + r.size.y * 0.5;
        self.d.label_elided(cx, rect(text_x, mid - 22.0, text_w, 24.0), true, 15.0, alpha(ink, opacity), HAlign::Left, name);
        let sub = if group.pair && members.len() == 2 { "App pair".to_string() } else { format!("{} app{}", members.len(), if members.len() == 1 { "" } else { "s" }) };
        self.d.label_elided(cx, rect(text_x, mid + 2.0, text_w, 20.0), false, 12.0, alpha(ink, 0.6 * opacity), HAlign::Left, &sub);
        self.hits.push((r, PhoneHit::Group(name.to_string())));
    }

    /// The overlay's part: the split divider (draggable, an exclusion
    /// band) and the split buttons on the Recents cards.
    pub(crate) fn draw_groups_overlay(&mut self, cx: &mut Cx2d, state: &crate::desk::WmState, screen: Rect) {
        let phone = &state.phone;
        let ink = if state.style.dark { rgb(238, 238, 242) } else { rgb(30, 30, 34) };
        if let Some(split) = phone.groups.split.filter(|_| phone.screen == PhoneScreen::App && phone.openness > 0.5 && phone.overview < 0.01) {
            let app = mobile::app_rect(screen);
            let band = split.divider(app);
            let vertical = Split::vertical(app);
            let pill = if vertical { rect(band.pos.x + band.size.x * 0.5 - 28.0, band.pos.y + band.size.y * 0.5 - 2.5, 56.0, 5.0) }
                else { rect(band.pos.x + band.size.x * 0.5 - 2.5, band.pos.y + band.size.y * 0.5 - 28.0, 5.0, 56.0) };
            let pressed = self.pressed_hit() == Some(&PhoneHit::Divider);
            self.rounded(cx, pill, 2.5, alpha(ink, if pressed { 0.9 } else { 0.55 }));
            self.hits.push((band, PhoneHit::Divider));
        }
        if phone.screen == PhoneScreen::Recents && phone.overview > 0.5 {
            let accent = if state.style.target == DesktopStyle::Ios { rgb(0, 122, 255) } else { rgb(103, 80, 164) };
            for (index, client) in phone.order.iter().enumerate() {
                let card = mobile::card_rect(screen, index as f64, phone.page);
                if card.pos.x + card.size.x < screen.pos.x || card.pos.x > screen.pos.x + screen.size.x { continue; }
                let picked = phone.groups.pick == Some(*client);
                let r = rect(card.pos.x + card.size.x - 84.0, card.pos.y - 38.0, 84.0, 30.0);
                self.rounded(cx, r, 15.0, if picked { alpha(accent, 0.95) } else { alpha(rgb(255, 255, 255), 0.22) });
                let label = if picked { "Picked" } else if phone.groups.pick.is_some() { "Pair with" } else { "Split" };
                self.label(cx, r, label, 12.0, true, rgb(255, 255, 255));
                self.hits.push((r, PhoneHit::Split(*client)));
            }
            if phone.groups.pick.is_some() {
                let hint = rect(screen.pos.x, screen.pos.y + screen.size.y - 110.0, screen.size.x, 24.0);
                self.label(cx, hint, "Tap another app to open both", 13.0, false, alpha(rgb(255, 255, 255), 0.85));
            }
        }
    }
}

impl WmDesk {
    /// The open group's sub-window: a frosted panel grown out of its tile,
    /// its members as tiles (their live captures where they run), and for
    /// a pair the "Open both" button. Drawn inside the compositor so the
    /// panel can blur what is under it; tapping outside closes it.
    /// `backdrop`: the still home scene's pyramid when the desk keeps one
    /// for the transition (desk/phone.rs); otherwise the live compositor's.
    pub(crate) fn draw_group_window(&mut self, cx: &mut Cx2d, state: &crate::desk::WmState, screen: Rect, backdrop: Option<makepad_widgets::gauss_view::GaussBlurSnapshot>) {
        let phone = &state.phone;
        let groups = &phone.groups;
        if !groups.window_visible() { return; }
        let Some(group) = groups.shown() else { return };
        let style = state.style.target;
        let dark = state.style.dark;
        let ios = style == DesktopStyle::Ios;
        let catalog = crate::shell::launcher::apps();
        let available: Vec<&str> = catalog.iter().map(|a| a.id.trim_start_matches("apps.")).collect();
        let members = group.present(&available);
        let layout = PhoneSurface::home_layout(style, screen);
        let tile = tile_of(&layout.tiles, &group.name).map(|s| s.rect).or(groups.origin)
            .unwrap_or(Rect { pos: screen.pos + screen.size * 0.5, size: dvec2(1.0, 1.0) });
        let window = group_window(screen, tile, &members, group.pair, groups.openness);
        let t = window.t as f32;
        let face = if dark { rgb(30, 32, 46) } else if ios { rgb(246, 247, 252) } else { rgb(255, 251, 255) };
        let ink = if dark { rgb(240, 240, 248) } else { rgb(28, 27, 36) };
        // The scrim: dims the page and catches the tap that closes. One flat
        // fill, not a full-screen SDF quad.
        self.phone_ui.d.solid(cx, screen, alpha(rgb(0, 0, 0), 0.32 * t));
        if groups.open.is_some() { self.phone_ui.hits.push((screen, PhoneHit::GroupClose)); }
        let backdrop = match backdrop {
            Some(backdrop) => backdrop,
            None => self.compositor.as_mut().unwrap().backdrop(cx, window.panel, 4.0),
        };
        self.phone_ui.group_glass.draw_surface_with_backdrop(cx, window.panel, Some(backdrop), t);
        self.phone_ui.rounded(cx, window.panel, 28.0, alpha(face, 0.55 * t));
        self.phone_content(window.panel);
        let label_a = (t * t).max(0.0);
        self.phone_ui.d.label_elided(cx, window.title, true, 17.0, alpha(ink, label_a), HAlign::Left, &group.name);
        let client_of = |app: &str| state.clients.iter()
            .filter(|(_, s)| s.app == app && !s.warm && !s.pane && !s.is_preview && s.closing.is_none())
            .map(|(c, _)| *c).min();
        for cell in &window.cells {
            let r = cell.rect;
            self.phone_ui.rounded(cx, r, 18.0, alpha(ink, 0.07 * t));
            let client = client_of(&cell.app);
            let mut live = false;
            if let Some(client) = client {
                live = self.present_member_capture(cx, client, r, t, 18.0);
            }
            let size = (r.size.y.min(r.size.x) * 0.36).clamp(20.0, 56.0);
            if !live {
                self.phone_ui.icons.draw(cx, &cell.app, style, rect(r.pos.x + (r.size.x - size) * 0.5, r.pos.y + (r.size.y - size) * 0.5 - 10.0, size, size), t, alpha(ink, t));
            } else {
                // A live member keeps its badge in the corner so the name stays readable.
                let badge = (size * 0.5).max(16.0);
                self.phone_ui.rounded(cx, rect(r.pos.x + 6.0, r.pos.y + r.size.y - badge - 8.0, badge + 4.0, badge + 4.0), 7.0, alpha(face, 0.85 * t));
                self.phone_ui.icons.draw(cx, &cell.app, style, rect(r.pos.x + 8.0, r.pos.y + r.size.y - badge - 6.0, badge, badge), t, alpha(ink, t));
            }
            let name = crate::clients::find_app(&cell.app).map(|a| a.label).unwrap_or_else(|| cell.app.clone());
            let name_r = if live { rect(r.pos.x + size * 0.5 + 16.0, r.pos.y + r.size.y - 26.0, r.size.x - size * 0.5 - 24.0, 20.0) } else { rect(r.pos.x + 6.0, r.pos.y + r.size.y * 0.5 + size * 0.5 - 2.0, r.size.x - 12.0, 22.0) };
            if live { self.phone_ui.rounded(cx, rect(name_r.pos.x - 6.0, name_r.pos.y - 2.0, name_r.size.x + 12.0, name_r.size.y + 4.0), 8.0, alpha(face, 0.85 * t)); }
            self.phone_ui.d.label_elided(cx, name_r, true, 13.0, alpha(ink, label_a), if live { HAlign::Left } else { HAlign::Center }, &name);
            self.phone_content(r);
            if groups.open.is_some() { self.phone_ui.hits.push((r, PhoneHit::GroupApp(group.name.clone(), cell.app.clone()))); }
        }
        if let Some(button) = window.open_both {
            let accent = if ios { rgb(0, 122, 255) } else { rgb(103, 80, 164) };
            let pressed = self.phone_ui.pressed_hit() == Some(&PhoneHit::OpenBoth(group.name.clone()));
            self.phone_ui.rounded(cx, button, (button.size.y * 0.5) as f32, alpha(accent, (if pressed { 0.75 } else { 1.0 }) * t));
            self.phone_ui.d.label_elided(cx, button, true, 15.0, alpha(rgb(255, 255, 255), label_a), HAlign::Center, "Open both");
            self.phone_content(button);
            if groups.open.is_some() { self.phone_ui.hits.push((button, PhoneHit::OpenBoth(group.name.clone()))); }
        }
    }
}

// --------------------------------------------------------------------
// The App side: what a tap does
// --------------------------------------------------------------------

impl crate::App {
    /// A tap on the group tile: the sub-window grows out of it.
    pub(super) fn open_group(&mut self, cx: &mut Cx, name: &str) {
        let state = self.state_mut();
        let screen = state.phone.viewport;
        let tile = tile_of(&PhoneSurface::home_layout(state.style.target, screen).tiles, name).map(|s| s.rect)
            .unwrap_or(Rect { pos: screen.pos + screen.size * 0.5, size: dvec2(1.0, 1.0) });
        state.phone.groups.open_group(name, tile);
        self.animate_phone(cx);
    }
    /// Two clients side by side: both become windows the person opened,
    /// `left` in front, and the split tells each its pane before the
    /// zoom-in, so neither draws a full-screen frame into a half.
    pub(super) fn enter_split(&mut self, cx: &mut Cx, left: ClientId, right: ClientId) {
        if !self.state_mut().phone.groups.enter_split(left, right) { return; }
        self.activate_client(cx, right);
        self.activate_client(cx, left);
        log!("wm: split screen {} | {}", left, right);
    }
    /// The pair's "Open both": every member that runs already joins as it
    /// is; a missing one is launched first. Returns false when the pair is
    /// not ready (the second tap, once both run, enters the split).
    pub(super) fn open_pair(&mut self, cx: &mut Cx, name: &str) -> bool {
        let Some(apps) = self.state_mut().phone.groups.get(name).filter(|g| g.pair).map(|g| g.apps.clone()) else { return false };
        let mut clients = Vec::new();
        for app in &apps {
            match self.client_of_app(app) {
                Some(client) => clients.push(client),
                None => { self.launch_app(cx, app); if let Some(client) = self.client_of_app(app) { clients.push(client); } }
            }
        }
        if let [left, right] = clients[..] {
            self.enter_split(cx, left, right);
            true
        } else {
            log!("wm: pair {} waits for its apps to start ({} of 2 running)", name, clients.len());
            self.state_mut().phone.groups.close();
            false
        }
    }
    /// The running window of `app`, the way the home page finds it.
    pub(super) fn client_of_app(&mut self, app: &str) -> Option<ClientId> {
        self.state_mut().clients.iter()
            .filter(|(_, s)| s.app == app && !s.warm && !s.pane && !s.is_preview && s.closing.is_none())
            .map(|(c, _)| *c).min()
    }
    /// `--test-action phone:<ios|android>` puts the desktop into that phone
    /// shell (a no-op in the standalone shell, which is already the one
    /// phone); `group:<name>` opens a group's window; `split:<a>,<b>`
    /// launches both apps and enters a split.
    pub(super) fn groups_test_action(&mut self, cx: &mut Cx, name: &str) -> bool {
        if let Some(style) = name.strip_prefix("phone:") {
            if crate::MOBILE_ONLY {
                log!("wm: --test-action phone:{} is a no-op in the standalone shell", style);
                return true;
            }
            let style = if style.eq_ignore_ascii_case("ios") { DesktopStyle::Ios } else { DesktopStyle::Android };
            log!("wm: --test-action phone {:?}", style);
            self.set_desktop_style(cx, style);
            return true;
        }
        if let Some(group) = name.strip_prefix("group:") {
            log!("wm: --test-action group {}", group);
            self.open_group(cx, group);
            return true;
        }
        if let Some(pair) = name.strip_prefix("split:") {
            let apps: Vec<String> = pair.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            let mut clients = Vec::new();
            for app in &apps {
                if self.client_of_app(app).is_none() { self.launch_app(cx, app); }
                if let Some(client) = self.client_of_app(app) { clients.push(client); }
            }
            log!("wm: --test-action split {:?} -> clients {:?}", apps, clients);
            if let [left, right] = clients[..] { self.enter_split(cx, left, right); }
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobile_gestures::{Dir, ExclusionZones};

    fn app() -> Rect { Rect { pos: dvec2(0.0, 42.0), size: dvec2(412.0, 826.0) } }

    #[test]
    fn a_group_of_exactly_two_is_a_pair() {
        let mut g = TileGroup::new("Work", &["sheets", "reference"]);
        assert!(g.pair);
        g.add("photos");
        assert!(!g.pair);
        g.remove("photos");
        assert!(g.pair);
        g.add("sheets");
        assert_eq!(g.apps.len(), 2, "no duplicate members");
        g.remove("sheets");
        assert!(!g.pair && g.apps == ["reference"]);
        assert_eq!(g.present(&["reference", "clock"]), ["reference"]);
        let seeded = GroupsState::default();
        assert!(seeded.groups.iter().all(|g| g.pair), "the seed groups are pairs");
        assert_eq!(GroupsState::placed(&["reference", "sheets", "photos", "appcard"]), ["Work", "Media"]);
        assert_eq!(GroupsState::placed(&["clock", "photos", "appcard"]), ["Media"]);
        assert!(GroupsState::placed(&["clock", "photos"]).is_empty(), "one member present is just that app's icon");
    }

    #[test]
    fn the_sub_window_grows_out_of_the_tile_and_back() {
        let mut groups = GroupsState::default();
        let tile = Rect { pos: dvec2(16.0, 300.0), size: dvec2(183.0, 76.0) };
        assert!(!groups.window_visible());
        groups.open_group("Nope", tile);
        assert!(groups.open.is_none(), "an unknown group does not open");
        groups.open_group("Work", tile);
        assert!(groups.is_open("Work") && groups.origin == Some(tile));
        assert!(groups.step(1.0 / 60.0) && groups.window_visible());
        for _ in 0..90 { groups.step(1.0 / 60.0); }
        assert_eq!(groups.openness, 1.0);
        assert!(!groups.step(1.0 / 60.0), "settled");
        groups.close();
        assert!(groups.open.is_none() && groups.shown().is_some_and(|g| g.name == "Work"), "still drawn while it shrinks");
        assert!(groups.step(1.0 / 60.0) && groups.openness < 1.0);
        for _ in 0..120 { groups.step(1.0 / 60.0); }
        assert_eq!(groups.openness, 0.0);
        assert!(groups.shown().is_none() && groups.origin.is_none());
        // The window's geometry follows the animation from the tile.
        let screen = Rect { pos: dvec2(0.0, 0.0), size: dvec2(412.0, 892.0) };
        let start = group_window(screen, tile, &["sheets", "reference"], true, 0.0);
        let end = group_window(screen, tile, &["sheets", "reference"], true, 1.0);
        assert_eq!(start.panel, tile);
        assert!(end.panel.size.x > 300.0 && end.panel.size.y > 200.0);
        assert!(end.panel.pos.x >= screen.pos.x && end.panel.pos.x + end.panel.size.x <= screen.size.x);
        assert_eq!(end.cells.len(), 2);
        assert!(end.open_both.is_some(), "a pair offers Open both");
        assert!(group_window(screen, tile, &["sheets"], false, 1.0).open_both.is_none());
        let [a, b] = [end.cells[0].rect, end.cells[1].rect];
        assert!(a.pos.x + a.size.x <= b.pos.x, "two across");
        for cell in &end.cells {
            assert!(cell.rect.pos.x >= end.panel.pos.x && cell.rect.pos.x + cell.rect.size.x <= end.panel.pos.x + end.panel.size.x + 0.01);
        }
        let button = end.open_both.unwrap();
        assert!(button.pos.y >= a.pos.y + a.size.y && button.pos.y + button.size.y <= end.panel.pos.y + end.panel.size.y + 0.01);
        let mid = group_window(screen, tile, &["sheets", "reference"], true, 0.5);
        assert!(mid.panel.size.x > tile.size.x && mid.panel.size.x < end.panel.size.x);
        // Landscape stays inside a wide screen too.
        let wide = Rect { pos: dvec2(0.0, 0.0), size: dvec2(892.0, 412.0) };
        let w = group_window(wide, tile, &["a", "b", "c"], false, 1.0);
        assert!(w.panel.pos.y + w.panel.size.y <= wide.size.y && w.cells.len() == 3);
    }

    #[test]
    fn split_panes_share_the_app_area_and_the_ratio_is_clamped() {
        let mut split = Split::new(3, 4);
        let app = app();
        let top = split.pane(3, app).unwrap();
        let bottom = split.pane(4, app).unwrap();
        assert!(split.pane(9, app).is_none());
        assert_eq!(top.pos, app.pos);
        assert!((top.size.y + bottom.size.y + SPLIT_GAP - app.size.y).abs() < 0.01, "the two halves and the gap fill the area");
        assert!(top.pos.y + top.size.y <= bottom.pos.y, "portrait: top over bottom");
        assert_eq!(top.size.x, app.size.x);
        split.set_ratio(0.05);
        assert_eq!(split.ratio, SPLIT_MIN_RATIO);
        split.set_ratio(0.99);
        assert_eq!(split.ratio, SPLIT_MAX_RATIO);
        // The divider follows the finger, in app-area coordinates.
        split.drag_to(dvec2(200.0, app.pos.y + app.size.y * 0.4), app);
        assert!((split.ratio - 0.4).abs() < 0.001);
        split.drag_to(dvec2(200.0, app.pos.y + app.size.y * 0.95), app);
        assert_eq!(split.ratio, SPLIT_MAX_RATIO);
        let band = split.divider(app);
        assert!(band.contains(dvec2(100.0, app.pos.y + app.size.y * SPLIT_MAX_RATIO)));
        assert_eq!(band.size, dvec2(app.size.x, DIVIDER_BAND));
        split.swap();
        assert_eq!((split.left, split.right), (4, 3));
        assert!((split.ratio - (1.0 - SPLIT_MAX_RATIO)).abs() < 1e-9);
        // Landscape: side by side.
        let wide = Rect { pos: dvec2(0.0, 24.0), size: dvec2(892.0, 364.0) };
        let split = Split::new(1, 2);
        let l = split.pane(1, wide).unwrap();
        let r = split.pane(2, wide).unwrap();
        assert!(l.pos.x + l.size.x <= r.pos.x && l.size.y == wide.size.y);
        assert_eq!(split.divider(wide).size, dvec2(DIVIDER_BAND, wide.size.y));
        let mut groups = GroupsState::default();
        groups.split = Some(Split::new(1, 2));
        assert_eq!(groups.pane_size(1, dvec2(412.0, 826.0)).y, 826.0 * 0.5 - SPLIT_GAP * 0.5);
        assert_eq!(groups.pane_size(7, dvec2(412.0, 826.0)), dvec2(412.0, 826.0), "outside the split: the whole area");
    }

    #[test]
    fn recents_picks_two_cards_into_a_split_and_home_leaves_it() {
        let mut groups = GroupsState::default();
        assert_eq!(groups.pick_card(10), None);
        assert_eq!(groups.pick, Some(10));
        assert_eq!(groups.pick_card(10), None, "the same card again unpicks");
        assert_eq!(groups.pick, None);
        groups.pick_card(10);
        assert_eq!(groups.pick_card(11), Some((10, 11)));
        assert!(groups.enter_split(10, 11));
        assert!(!groups.enter_split(5, 5), "a client cannot split with itself");
        assert!(groups.in_split(10) && groups.in_split(11) && !groups.in_split(12));
        assert_eq!(groups.pick, None);
        // The split survives the app screen with either member in front...
        groups.follow(PhoneScreen::App, Some(11));
        assert!(groups.split.is_some());
        // ...ends when a third client comes in front, or the app screen is left.
        groups.follow(PhoneScreen::App, Some(12));
        assert!(groups.split.is_none());
        groups.enter_split(10, 11);
        groups.follow(PhoneScreen::Home, Some(10));
        assert!(groups.split.is_none());
        groups.enter_split(10, 11);
        groups.follow(PhoneScreen::Recents, Some(10));
        assert!(groups.split.is_none(), "Recents is for choosing what comes next");
        // A committed HomeUp gesture leaves the split; a progress frame does not.
        groups.enter_split(10, 11);
        groups.on_gesture(&ShellGesture::HomeUp { progress: 0.5, held: false });
        assert!(groups.split.is_some());
        groups.on_gesture(&ShellGesture::Commit(GestureKind::QuickSwitch(Dir::Left)));
        assert!(groups.split.is_some());
        groups.on_gesture(&ShellGesture::Commit(GestureKind::HomeUp));
        assert!(groups.split.is_none());
        // Entering a split from an open pair window closes the window.
        groups.open_group("Work", Rect::default());
        groups.enter_split(10, 11);
        assert!(groups.open.is_none());
    }

    #[test]
    fn the_divider_band_is_an_exclusion_zone_only_while_split() {
        let mut groups = GroupsState::default();
        let mut zones = ExclusionZones::default();
        let app = app();
        groups.add_exclusions(PhoneScreen::App, app, &mut zones);
        assert!(zones.zones.is_empty());
        groups.enter_split(1, 2);
        groups.add_exclusions(PhoneScreen::App, app, &mut zones);
        assert_eq!(zones.zones.len(), 1);
        let mid = dvec2(200.0, app.pos.y + app.size.y * 0.5);
        assert!(zones.excludes(mid, crate::mobile_gestures::Edge::Bottom));
        // The desk clears the frame's zones before every surface adds again.
        groups.drag_divider(dvec2(200.0, app.pos.y + app.size.y * 0.3), app);
        zones.clear();
        groups.add_exclusions(PhoneScreen::App, app, &mut zones);
        assert_eq!(zones.zones.len(), 1);
        assert!(!zones.excludes(mid, crate::mobile_gestures::Edge::Bottom));
        assert!(zones.excludes(dvec2(200.0, app.pos.y + app.size.y * 0.3), crate::mobile_gestures::Edge::Left));
        groups.leave_split();
        zones.clear();
        groups.add_exclusions(PhoneScreen::App, app, &mut zones);
        assert!(zones.zones.is_empty());
        // Off the app screen the band is nobody's, split or not.
        let mut phone = PhoneState::default();
        phone.viewport = Rect { pos: dvec2(0.0, 0.0), size: dvec2(412.0, 892.0) };
        phone.activate(1); phone.activate(2);
        phone.groups.enter_split(2, 1);
        follow(&mut phone);
        phone.groups.add_exclusions(phone.screen, mobile::app_rect(phone.viewport), &mut phone.exclusions);
        assert!(phone.groups.split.is_some() && phone.exclusions.zones.len() == 1);
        phone.navigate(PhoneScreen::Home);
        follow(&mut phone);
        phone.exclusions.clear();
        phone.groups.add_exclusions(phone.screen, mobile::app_rect(phone.viewport), &mut phone.exclusions);
        assert!(phone.groups.split.is_none() && phone.exclusions.zones.is_empty());
    }

    #[test]
    fn group_tiles_sit_in_the_home_grid_and_leave_the_favorites_a_row() {
        use crate::mobile_tiles::{home_layout_for_apps, TileKind};
        let screen = Rect { pos: dvec2(0.0, 78.0), size: dvec2(412.0, 814.0) };
        let top = screen.pos.y + 156.0;
        let dock = PhoneSurface::home_dock(screen);
        let layout = home_layout_for_apps(screen, top, dock, &["reference", "sheets", "photos", "appcard"]);
        let groups: Vec<_> = layout.tiles.iter().filter(|t| matches!(t.kind, TileKind::Group(_))).collect();
        assert_eq!(groups.iter().map(|t| t.app).collect::<Vec<_>>(), ["Work", "Media"]);
        assert!(tile_of(&layout.tiles, "Work").is_some() && tile_of(&layout.tiles, "Nope").is_none());
        let apps: Vec<_> = layout.tiles.iter().filter(|t| !matches!(t.kind, TileKind::Group(_))).collect();
        assert_eq!(apps.len(), 2, "the app tiles are untouched");
        for g in &groups {
            assert_eq!(g.rect.size.y, GROUP_TILE_HEIGHT);
            assert!(apps.iter().all(|a| a.rect.pos.y + a.rect.size.y <= g.rect.pos.y), "groups sit under the app tiles");
            assert!(g.rect.pos.y + g.rect.size.y <= layout.favorites.pos.y);
        }
        assert!(groups[0].rect.pos.x + groups[0].rect.size.x <= groups[1].rect.pos.x, "two groups share a row");
        assert!(layout.capacity >= 4, "a row of favorites still fits: {}", layout.capacity);
        let none = home_layout_for_apps(screen, top, dock, &["clock"]);
        assert!(none.tiles.iter().all(|t| !matches!(t.kind, TileKind::Group(_))));
        let wide = Rect { pos: dvec2(0.0, 26.0), size: dvec2(892.0, 386.0) };
        let landscape = home_layout_for_apps(wide, wide.pos.y + 44.0, PhoneSurface::home_dock(wide), &["reference", "sheets", "photos", "appcard"]);
        let y = landscape.tiles[0].rect.pos.y;
        assert!(landscape.tiles.iter().all(|t| t.rect.pos.y == y), "landscape: one row, groups included");
        assert_eq!(landscape.tiles.len(), 4);
    }

    #[test]
    fn captures_fit_their_cells_by_aspect() {
        let cell = Rect { pos: dvec2(10.0, 10.0), size: dvec2(160.0, 120.0) };
        let wide = fit(cell, dvec2(370.0, 177.0));
        assert!((wide.size.x - 160.0).abs() < 0.01 && wide.size.y < 120.0 && wide.pos.y > cell.pos.y);
        let tall = fit(cell, dvec2(402.0, 782.0));
        assert!((tall.size.y - 120.0).abs() < 0.01 && tall.size.x < 160.0 && tall.pos.x > cell.pos.x);
        assert_eq!(fit(cell, dvec2(0.0, 0.0)), cell);
    }
}
