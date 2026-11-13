//! What a Mac's approval surfaces are drawn with (`mac_view.rs`,
//! `settings_page.rs`): the host sheets' palette (`crates/oauth-service/src/host.rs`,
//! `DESK_PARTS`), so a shell-drawn sheet and a host service's look like one
//! family, and the few controls those surfaces need. Opaque, and the same in
//! either tree: a decision is read on an even ground.

use makepad_widgets::*;

use crate::desk::WmState;
use crate::desktop::DesktopStyle;
use crate::shell::ui::{contains, rect, HAlign, Ico, ShellDraw};
use crate::shell::{alpha, rgb};

/// A control's height.
pub const CONTROL_H: f64 = 28.0;
/// A small control's height, in a list's row.
pub const SMALL_H: f64 = 22.0;
/// Body text.
pub const TEXT: f64 = 13.0;
/// A note under a control or a row.
pub const NOTE: f64 = 11.0;
pub const TEXT_LINE: f64 = 17.0;
pub const NOTE_LINE: f64 = 15.0;

fn hex(v: u32) -> Vec4f {
    rgb(((v >> 16) & 255) as u8, ((v >> 8) & 255) as u8, (v & 255) as u8)
}

/// The colours, measured at 4.5:1 or better for all text on `card` and `group`.
#[derive(Clone, Copy, Debug)]
pub struct MacInk {
    pub ink: Vec4f,
    pub muted: Vec4f,
    pub line: Vec4f,
    pub card: Vec4f,
    pub group: Vec4f,
    pub control: Vec4f,
    pub control_hover: Vec4f,
    pub control_down: Vec4f,
    pub control_line: Vec4f,
    pub off: Vec4f,
    pub off_ink: Vec4f,
    pub accent: Vec4f,
    pub accent_hover: Vec4f,
    pub accent_down: Vec4f,
    pub dim: Vec4f,
    pub warn: Vec4f,
    pub danger: Vec4f,
    pub amber: Vec4f,
    pub menu: Vec4f,
    pub menu_line: Vec4f,
    pub menu_accent: Vec4f,
    pub thumb: Vec4f,
    pub switch_off: Vec4f,
}

pub fn mac_ink(dark: bool) -> MacInk {
    if dark {
        MacInk {
            ink: hex(0xf5f5f7),
            // 5.1:1 on `group`, where a row's label sits; above the Mac's own
            // secondary label there, which sat at 4.8:1.
            muted: hex(0xaeaeb2),
            line: hex(0x48484a),
            card: hex(0x2c2c2e),
            group: hex(0x3a3a3c),
            control: hex(0x48484a),
            control_hover: hex(0x545458),
            control_down: hex(0x5e5e62),
            control_line: hex(0x545458),
            off: hex(0x3a3a3c),
            off_ink: hex(0xaeaeb2),
            accent: hex(0x0071e3),
            accent_hover: hex(0x006edb),
            accent_down: hex(0x0060c0),
            dim: Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 0.32 },
            warn: hex(0xffb340),
            danger: hex(0xff7b72),
            amber: hex(0xff9f0a),
            menu: hex(0x323234),
            menu_line: hex(0x56565a),
            menu_accent: rgb(10, 96, 208),
            thumb: alpha(hex(0xf5f5f7), 0.38),
            switch_off: hex(0x5a5a5e),
        }
    } else {
        MacInk {
            ink: hex(0x1d1d1f),
            muted: hex(0x636368),
            line: hex(0xe3e3e8),
            card: hex(0xf6f6f8),
            group: hex(0xffffff),
            control: hex(0xffffff),
            control_hover: hex(0xf2f2f4),
            control_down: hex(0xe5e5ea),
            control_line: hex(0xc9c9ce),
            off: hex(0xe3e3e8),
            off_ink: hex(0x636368),
            accent: hex(0x0071e3),
            accent_hover: hex(0x006edb),
            accent_down: hex(0x0060c0),
            dim: Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 0.14 },
            warn: hex(0x8a5a00),
            danger: hex(0xd70015),
            amber: hex(0xff9500),
            menu: hex(0xfbfbfd),
            menu_line: hex(0xd8d8dd),
            menu_accent: rgb(0, 100, 215),
            thumb: alpha(hex(0x1d1d1f), 0.34),
            switch_off: hex(0xd1d1d6),
        }
    }
}

/// The window in front, for a sheet that belongs to it.
#[derive(Clone, Debug, PartialEq)]
pub struct FrontWindow {
    /// As the window's slot names it (`mail`, `os.news`).
    pub app: String,
    pub rect: Rect,
    /// How far below the window's top edge a sheet attaches: under its title
    /// bar, or its unified toolbar.
    pub top: f64,
}

/// What a Mac's surfaces need from the desk.
#[derive(Clone, Debug, PartialEq)]
pub struct MacPlace {
    pub dark: bool,
    pub style: DesktopStyle,
    pub front: Option<FrontWindow>,
    /// The Dock's height above the screen's bottom edge.
    pub dock: f64,
    /// The window in front is full screen and the menu bar is up out of sight:
    /// what the bar's status items say has nowhere to be read.
    pub bar_hidden: bool,
}

/// Read once a frame. `None`: not a Mac, so nothing here applies.
pub fn mac_place(state: &WmState, screen: Rect) -> Option<MacPlace> {
    if state.style.target != DesktopStyle::Macos {
        return None;
    }
    let bottom = screen.pos.y + screen.size.y;
    let dock = if crate::desktop::dock_auto_hides(state) { 0.0 } else { crate::desktop::dock_overlay(&state.style, bottom, bottom) };
    let front = state.layout.focused_client().and_then(|client| {
        let window = state.layout.desktop.get(client).filter(|w| !w.minimized)?;
        let slot = state.clients.get(&client).filter(|s| !s.warm && !s.pane && s.closing.is_none())?;
        let top = if window.fullscreen {
            0.0
        } else if crate::clients::unified_chrome(&slot.app) {
            crate::clients::UNIFIED_TOOLBAR
        } else {
            state.style.title_height()
        };
        Some(FrontWindow { app: slot.app.clone(), rect: rect(window.rect.x, window.rect.y, window.rect.w, window.rect.h), top })
    });
    let bar_hidden = crate::desktop::front_fullscreen(state) && state.top_reveal < 0.01;
    Some(MacPlace { dark: state.style.dark, style: state.style.target, front, dock, bar_hidden })
}

/// The window in front when it is one of `apps`.
pub fn related_window<'a>(place: &'a MacPlace, apps: &[&str]) -> Option<&'a FrontWindow> {
    let bare = |a: &str| a.strip_prefix("os.").unwrap_or(a).to_string();
    place.front.as_ref().filter(|w| apps.iter().any(|a| bare(a) == bare(&w.app)))
}

/// The Mac's card window when it shows a card of one of `apps`: the window a
/// sheet about that card's assistant hangs from, under its Card | Chat control.
pub fn card_window(apps: &[&str]) -> Option<FrontWindow> {
    let (at, top, app) = crate::glance_sheet::mac_window()?;
    let bare = |a: &str| a.strip_prefix("os.").unwrap_or(a).to_string();
    apps.iter().any(|a| bare(a) == bare(&app)).then_some(FrontWindow { app, rect: at, top })
}

/// The icon catalog's name for an app the approvals model calls by its
/// manifest id, the form the Dock draws: `mail` for `os.mail`, a built-in's own
/// id, and `hub:<id>` for an installed app.
pub fn icon_name(app: &str) -> String {
    if let Some(short) = app.strip_prefix("os.") {
        return short.to_string();
    }
    if app.starts_with("hub:") || crate::clients::find_app(app).is_some() {
        return app.to_string();
    }
    crate::apps::installed_launch_id(app)
}

/// A push button's look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// The blue default.
    Default,
    /// Another button.
    Plain,
    /// Pressing it does nothing yet.
    Disabled,
}

/// A surface's card: the shadow of the shell's material, an opaque ground and a hairline.
pub fn draw_card(d: &mut ShellDraw, cx: &mut Cx2d, r: Rect, corner: f64, k: &MacInk) {
    d.glass_pill(cx, r, corner, 1.0);
    d.rounded(cx, r, corner * 0.5, k.card);
    d.focus_ring(cx, r, corner * 0.5, 1.0, k.line);
}

/// An inset group: rows on a lighter ground, with a hairline around them.
pub fn draw_group(d: &mut ShellDraw, cx: &mut Cx2d, r: Rect, k: &MacInk) {
    d.rounded(cx, r, 4.0, k.group);
    d.focus_ring(cx, r, 4.0, 1.0, k.line);
}

/// A window's corner.
const WINDOW_CORNER: f64 = 10.0;

/// The light dim over the window a sheet belongs to: all that shows of it above
/// the Dock, up to the edges of the Assistant's panels floating over it.
pub fn dim_window(d: &mut ShellDraw, cx: &mut Cx2d, window: Rect, k: &MacInk) {
    use crate::system_chat::view::mac::{dim_areas, dim_corners};
    for piece in dim_areas(window) {
        cx.push_clip_rect(piece);
        d.rounded(cx, window, WINDOW_CORNER * 0.5, k.dim);
        cx.pop_clip_rect();
    }
    // A panel's corners are round and the window shows past them: a ring as
    // wide as two corners, its inner edge on the panel's, fills them.
    let round = d.material().corner_radius;
    for (corner, panel) in dim_corners(window, round) {
        let out = round * 2.0;
        cx.push_clip_rect(corner);
        d.focus_ring(cx, rect(panel.pos.x - out, panel.pos.y - out, panel.size.x + out * 2.0, panel.size.y + out * 2.0), round, out, k.dim);
        cx.pop_clip_rect();
    }
}

/// A list row's details button (ⓘ): a ring with an i, grey until the pointer
/// is on it.
pub fn info_button(d: &mut ShellDraw, cx: &mut Cx2d, r: Rect, k: &MacInk, hover: bool, down: bool) {
    let size = 15.0;
    let ring = rect(r.pos.x + ((r.size.x - size) * 0.5).floor(), r.pos.y + ((r.size.y - size) * 0.5).floor(), size, size);
    let color = if down || hover { k.ink } else { k.muted };
    d.focus_ring(cx, ring, size * 0.5, 1.0, color);
    d.label(cx, text_seat(ring), true, 11.0, color, HAlign::Center, "i");
}

/// A button's width for its label.
pub fn button_width(d: &mut ShellDraw, cx: &mut Cx2d, label: &str, min: f64) -> f64 {
    (measure(d, cx, false, TEXT, label) + 28.0).ceil().max(min)
}

/// Width of a label at a size, as `label` draws it.
pub fn measure(d: &mut ShellDraw, cx: &mut Cx2d, bold: bool, px: f64, s: &str) -> f64 {
    let scale = d.text_scale();
    d.measure(cx, bold, px * scale, s)
}

/// Words wrapped to a width, as `label` draws them.
pub fn wrapped(d: &mut ShellDraw, cx: &mut Cx2d, bold: bool, px: f64, s: &str, w: f64) -> Vec<String> {
    let scale = d.text_scale();
    d.wrap_lines(cx, bold, px * scale, s, w, false)
}

/// Words wrapped as `wrapped` does, then evened out: the narrowest width that keeps
/// as few lines, so a centred alert's title never leaves one word alone on its last
/// line, as a Mac sets it.
pub fn balanced(d: &mut ShellDraw, cx: &mut Cx2d, bold: bool, px: f64, s: &str, w: f64) -> Vec<String> {
    let lines = wrapped(d, cx, bold, px, s, w);
    if lines.len() < 2 {
        return lines;
    }
    let (mut lo, mut hi) = (w * 0.4, w);
    for _ in 0..10 {
        let mid = (lo + hi) * 0.5;
        if wrapped(d, cx, bold, px, s, mid).len() <= lines.len() {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    wrapped(d, cx, bold, px, s, hi)
}

/// A value wrapped to a width with every character kept: a word too long for
/// the line breaks inside itself.
pub fn wrapped_exact(d: &mut ShellDraw, cx: &mut Cx2d, code: bool, px: f64, s: &str, w: f64) -> Vec<String> {
    let scale = d.text_scale();
    let mut wrap = |d: &mut ShellDraw| crate::shell::ui::wrap_exact(s, w, |t| d.measure(cx, false, px * scale, t));
    if code {
        d.with_code(|d| wrap(d))
    } else {
        wrap(d)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn push_button(d: &mut ShellDraw, cx: &mut Cx2d, r: Rect, k: &MacInk, face: Face, hover: bool, down: bool, label: &str) {
    let t = text_seat(r);
    match face {
        Face::Default => {
            let fill = if down { k.accent_down } else if hover { k.accent_hover } else { k.accent };
            d.rounded(cx, r, 3.0, fill);
            d.label(cx, t, false, TEXT, rgb(255, 255, 255), HAlign::Center, label);
        }
        Face::Plain => {
            let fill = if down { k.control_down } else if hover { k.control_hover } else { k.control };
            d.rounded(cx, r, 3.0, fill);
            d.focus_ring(cx, r, 3.0, 1.0, k.control_line);
            d.label(cx, t, false, TEXT, k.ink, HAlign::Center, label);
        }
        Face::Disabled => {
            d.rounded(cx, r, 3.0, k.off);
            d.label(cx, t, false, TEXT, k.off_ink, HAlign::Center, label);
        }
    }
}

/// A small push button, in a list's row.
#[allow(clippy::too_many_arguments)]
pub fn small_button(d: &mut ShellDraw, cx: &mut Cx2d, r: Rect, k: &MacInk, face: Face, hover: bool, down: bool, label: &str) {
    let radius = 2.5;
    let t = text_seat(r);
    match face {
        Face::Default => {
            d.rounded(cx, r, radius, if down { k.accent_down } else if hover { k.accent_hover } else { k.accent });
            d.label(cx, t, false, 12.0, rgb(255, 255, 255), HAlign::Center, label);
        }
        Face::Plain => {
            d.rounded(cx, r, radius, if down { k.control_down } else if hover { k.control_hover } else { k.control });
            d.focus_ring(cx, r, radius, 1.0, k.control_line);
            d.label(cx, t, false, 12.0, k.ink, HAlign::Center, label);
        }
        Face::Disabled => {
            d.rounded(cx, r, radius, k.off);
            d.label(cx, t, false, 12.0, k.off_ink, HAlign::Center, label);
        }
    }
}

/// Where a control's one line of text goes: this face's metrics seat it a point
/// low in a rect it is centred in, so a word with no descender looks dropped.
pub fn text_seat(r: Rect) -> Rect {
    rect(r.pos.x, r.pos.y - 1.0, r.size.x, r.size.y)
}

/// What a pop-up button leaves for its mark at the trailing edge.
pub const POPUP_MARK: f64 = 22.0;
/// A pop-up button's title starts this far in, and a menu row's this far into its menu.
const POPUP_TITLE_X: f64 = 10.0;
const MENU_TITLE_X: f64 = 25.0;
const MENU_ROW: f64 = 22.0;
const MENU_PAD: f64 = 5.0;

/// A pop-up button's width for the longest of its titles.
pub fn popup_width(d: &mut ShellDraw, cx: &mut Cx2d, titles: &[String], px: f64, min: f64) -> f64 {
    let widest = titles.iter().map(|t| measure(d, cx, false, px, t)).fold(0.0, f64::max);
    (widest + 10.0 + POPUP_MARK + 4.0).ceil().max(min)
}

/// A menu button's face and title; the mark at its trailing edge goes on top.
#[allow(clippy::too_many_arguments)]
fn menu_button(d: &mut ShellDraw, cx: &mut Cx2d, r: Rect, k: &MacInk, px: f64, hover: bool, down: bool, title: &str) -> Rect {
    let fill = if down { k.control_down } else if hover { k.control_hover } else { k.control };
    let radius = if r.size.y >= CONTROL_H { 3.0 } else { 2.5 };
    d.rounded(cx, r, radius, fill);
    d.focus_ring(cx, r, radius, 1.0, k.control_line);
    d.label_elided(cx, text_seat(rect(r.pos.x + POPUP_TITLE_X, r.pos.y, r.size.x - POPUP_TITLE_X - POPUP_MARK, r.size.y)), false, px, k.ink, HAlign::Left, title);
    rect(r.pos.x + r.size.x - POPUP_MARK + 2.0, r.pos.y + (r.size.y - 16.0) * 0.5, 14.0, 16.0)
}

/// A pop-up button: the choice, and the up and down mark at its trailing edge.
#[allow(clippy::too_many_arguments)]
pub fn popup_button(d: &mut ShellDraw, cx: &mut Cx2d, r: Rect, k: &MacInk, px: f64, hover: bool, down: bool, title: &str) {
    let mark = menu_button(d, cx, r, k, px, hover, down, title);
    d.icon_centered(cx, Ico::ChevronUp, rect(mark.pos.x, mark.pos.y, mark.size.x, 8.0), 8.0, k.muted);
    d.icon_centered(cx, Ico::ChevronDown, rect(mark.pos.x, mark.pos.y + 8.0, mark.size.x, 8.0), 8.0, k.muted);
}

/// A pull-down button: it offers rather than holds, so its title stays what it is
/// and its mark points only down.
#[allow(clippy::too_many_arguments)]
pub fn pull_down_button(d: &mut ShellDraw, cx: &mut Cx2d, r: Rect, k: &MacInk, px: f64, hover: bool, down: bool, title: &str) {
    let mark = menu_button(d, cx, r, k, px, hover, down, title);
    d.icon_centered(cx, Ico::ChevronDown, mark, 8.0, k.muted);
}

/// Where a menu's position is kept inside `limit`; one taller or wider than it uses `screen`.
fn kept_inside(pos: f64, size: f64, limit: (f64, f64), screen: (f64, f64)) -> f64 {
    let (start, room) = if size <= limit.1 { limit } else { screen };
    pos.min(start + room - size).max(start)
}

/// How a pop-up's menu sits, as on a Mac: over its button, the chosen row on the
/// button and that row's title just where the button's is, so that nothing moves
/// under the eye. It stays inside `limit`, the sheet, moving as little as it can;
/// one too big for that uses `screen`. With no row chosen (a pull-down, which
/// offers rather than holds) it opens under the button, or over it where there is
/// no room under it, and never covers the button.
pub fn menu_rect(button: Rect, width: f64, rows: usize, chosen: Option<usize>, limit: Rect, screen: Rect) -> Rect {
    let h = rows as f64 * MENU_ROW + MENU_PAD * 2.0;
    let (x, y, width) = match chosen {
        Some(row) => {
            let x = button.pos.x + POPUP_TITLE_X - MENU_TITLE_X;
            let y = button.pos.y + button.size.y * 0.5 - MENU_PAD - (row as f64 + 0.5) * MENU_ROW;
            // At least to the button's far edge: the menu covers what it stands for.
            (x, y, width.max(button.pos.x + button.size.x - x))
        }
        None => (button.pos.x, button.pos.y + button.size.y + 2.0, width.max(button.size.x)),
    };
    let x = kept_inside(x, width, (limit.pos.x, limit.size.x), (screen.pos.x, screen.size.x));
    let y = match chosen {
        Some(_) => kept_inside(y, h, (limit.pos.y, limit.size.y), (screen.pos.y, screen.size.y)),
        None => {
            let over = button.pos.y - 2.0 - h;
            let fits = |y: f64, area: Rect| y >= area.pos.y && y + h <= area.pos.y + area.size.y;
            [(y, limit), (over, limit), (y, screen), (over, screen)].into_iter().find(|(y, area)| fits(*y, *area)).map_or(y, |(y, _)| y)
        }
    };
    rect(x.floor(), y.floor(), width, h)
}

/// Whether two rects share any area.
pub fn overlap(a: Rect, b: Rect) -> bool {
    clipped(a, b).is_some()
}

/// A pop-up's menu: one row per title, a check on each chosen one, and under the
/// pointer or the arrow keys the row they are on. Returns each row's rect.
#[allow(clippy::too_many_arguments)]
pub fn popup_menu(d: &mut ShellDraw, cx: &mut Cx2d, r: Rect, k: &MacInk, titles: &[String], chosen: &[usize], hover: Option<Vec2d>, cursor: Option<usize>) -> Vec<Rect> {
    popup_menu_lead(d, cx, r, k, titles, chosen, hover, cursor, 0.0)
}

/// An app's icon in a pop-up's menu row, before its name, as a Mac's app
/// chooser lists apps; and how far it moves the name.
pub const MENU_ICON: f64 = 16.0;
pub const MENU_ICON_LEAD: f64 = MENU_ICON + 6.0;

/// Each row's band in a pop-up's menu `r` of `rows` rows, as [`popup_menu`]
/// lays them.
pub fn menu_bands(r: Rect, rows: usize) -> Vec<Rect> {
    (0..rows).map(|i| rect(r.pos.x + MENU_PAD, r.pos.y + MENU_PAD + MENU_ROW * i as f64, r.size.x - MENU_PAD * 2.0, MENU_ROW)).collect()
}

/// Where a row's icon stands in a pop-up's menu `r` ([`MENU_ICON_LEAD`]).
pub fn menu_icon_at(r: Rect, band: Rect) -> Rect {
    rect(r.pos.x + MENU_TITLE_X, band.pos.y + (band.size.y - MENU_ICON) * 0.5, MENU_ICON, MENU_ICON)
}

/// [`popup_menu`] with each title moved `lead` points right, for what the
/// caller draws before it ([`menu_icon_at`]).
#[allow(clippy::too_many_arguments)]
pub fn popup_menu_lead(d: &mut ShellDraw, cx: &mut Cx2d, r: Rect, k: &MacInk, titles: &[String], chosen: &[usize], hover: Option<Vec2d>, cursor: Option<usize>, lead: f64) -> Vec<Rect> {
    d.glass_pill(cx, r, 10.0, 1.0);
    d.rounded(cx, r, 5.0, k.menu);
    d.focus_ring(cx, r, 5.0, 1.0, k.menu_line);
    let mut rows = Vec::with_capacity(titles.len());
    let mut y = r.pos.y + MENU_PAD;
    for (i, title) in titles.iter().enumerate() {
        let band = rect(r.pos.x + MENU_PAD, y, r.size.x - MENU_PAD * 2.0, MENU_ROW);
        let on = cursor == Some(i) || hover.is_some_and(|p| contains(band, p));
        if on {
            d.rounded(cx, band, 2.5, k.menu_accent);
        }
        let color = if on { rgb(255, 255, 255) } else { k.ink };
        if chosen.contains(&i) {
            d.icon_centered(cx, Ico::Check, rect(band.pos.x + 3.0, y, 14.0, MENU_ROW), 11.0, color);
        }
        d.label(cx, text_seat(rect(r.pos.x + MENU_TITLE_X + lead, y, (band.size.x - 28.0 - lead).max(1.0), MENU_ROW)), false, TEXT, color, HAlign::Left, title);
        rows.push(band);
        y += MENU_ROW;
    }
    rows
}

/// Where the arrow keys take a pop-up's cursor among `rows`, from where it is or
/// from the row that is chosen. They stop at the ends, as a Mac's do.
pub fn menu_step(cursor: Option<usize>, chosen: Option<usize>, rows: usize, by: isize) -> Option<usize> {
    let last = rows.checked_sub(1)?;
    Some(match cursor.or(chosen) {
        Some(from) => from.saturating_add_signed(by).min(last),
        None if by > 0 => 0,
        None => last,
    })
}

/// A menu's width for its titles.
pub fn menu_width(d: &mut ShellDraw, cx: &mut Cx2d, titles: &[String], min: f64) -> f64 {
    let widest = titles.iter().map(|t| measure(d, cx, false, TEXT, t)).fold(0.0, f64::max);
    (widest + 20.0 + 5.0 * 2.0 + 18.0).ceil().max(min)
}

/// A switch's size.
pub const SWITCH_W: f64 = 32.0;
pub const SWITCH_H: f64 = 18.0;

/// A Mac's switch.
pub fn switch(d: &mut ShellDraw, cx: &mut Cx2d, r: Rect, k: &MacInk, on: bool, hover: bool) {
    let track = if on { if hover { k.accent_hover } else { k.accent } } else { k.switch_off };
    d.rounded(cx, r, r.size.y * 0.25, track);
    let inset = 1.5;
    let knob = r.size.y - inset * 2.0;
    let x = if on { r.pos.x + r.size.x - knob - inset } else { r.pos.x + inset };
    d.rounded(cx, rect(x, r.pos.y + inset, knob, knob), knob * 0.25, rgb(255, 255, 255));
}

/// A radio button.
pub fn radio(d: &mut ShellDraw, cx: &mut Cx2d, r: Rect, k: &MacInk, on: bool) {
    if on {
        d.rounded(cx, r, r.size.y * 0.25, k.accent);
        let dot = 6.0;
        d.rounded(cx, rect(r.pos.x + (r.size.x - dot) * 0.5, r.pos.y + (r.size.y - dot) * 0.5, dot, dot), dot * 0.25, rgb(255, 255, 255));
    } else {
        d.rounded(cx, r, r.size.y * 0.25, k.control);
        d.focus_ring(cx, r, r.size.y * 0.25, 1.0, k.control_line);
    }
}

/// How long the overlay thumb stays after the content moved or first came up.
const THUMB_SECONDS: f64 = 1.4;
/// The strip at a scroll area's trailing edge where the pointer brings the thumb
/// back: the thumb's own 9 points and a little room, as the assistant's 14.
const THUMB_STRIP: f64 = 14.0;

/// A scroll area: where it is scrolled, how far, and whether its end has been on
/// screen. The wheel, the thumb and the arrow keys move it. The thumb is an
/// overlay, as on a Mac: no track, gone at rest, there while the content moves
/// and while the pointer is on its strip at the trailing edge (not anywhere over
/// the content, which a wheel's pointer always is), as the assistant's is.
#[derive(Clone, Debug, Default)]
pub struct Scroller {
    pub area: Option<Rect>,
    pub offset: f64,
    pub max: f64,
    pub content: f64,
    /// The end of the content has been on screen.
    pub seen_end: bool,
    /// What the content was laid out for: a new one starts from the top.
    key: String,
    drag: Option<(f64, f64)>,
    /// Moved since the last draw: the thumb comes back.
    moved: bool,
    show_until: f64,
    over: bool,
    timer: Timer,
}

impl Scroller {
    /// This frame's area and content. Another `key` (other content, or the same
    /// at another width) starts over from the top, unseen.
    pub fn layout(&mut self, key: &str, area: Rect, content: f64) {
        if self.key != key {
            *self = Scroller { key: key.to_string(), moved: true, ..Scroller::default() };
        }
        self.area = Some(area);
        self.content = content;
        self.max = (content - area.size.y).max(0.0);
        self.offset = self.offset.clamp(0.0, self.max);
        if self.offset >= self.max - 0.5 {
            self.seen_end = true;
        }
    }
    pub fn clear(&mut self) {
        *self = Scroller::default();
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn by(&mut self, delta: f64) {
        self.offset = (self.offset + delta).clamp(0.0, self.max);
        self.moved = true;
    }
    pub fn to(&mut self, offset: f64) {
        self.offset = offset.clamp(0.0, self.max);
        self.moved = true;
    }
    /// The pointer moved. True when the thumb has to appear or go.
    pub fn pointer_at(&mut self, p: Vec2d) -> bool {
        let over = self.area.is_some_and(|a| contains(rect(a.pos.x + a.size.x - THUMB_STRIP, a.pos.y, THUMB_STRIP, a.size.y), p));
        std::mem::replace(&mut self.over, over) != over
    }
    /// The thumb's time is up: draw again without it.
    pub fn faded(&self, event: &Event) -> bool {
        self.timer.is_event(event).is_some()
    }
    pub fn page(&self) -> f64 {
        self.area.map_or(0.0, |a| a.size.y - TEXT_LINE)
    }
    fn thumb(&self) -> Option<Rect> {
        let area = self.area?;
        if self.max <= 0.0 {
            return None;
        }
        let h = (area.size.y * area.size.y / self.content).clamp(24.0, area.size.y);
        let y = area.pos.y + (area.size.y - h) * self.offset / self.max;
        Some(rect(area.pos.x + area.size.x - 9.0, y, 6.0, h))
    }
    /// The thumb's grab, from a press. True when taken.
    pub fn press(&mut self, p: Vec2d) -> bool {
        match self.thumb() {
            Some(t) if contains(rect(t.pos.x - 3.0, t.pos.y, t.size.x + 6.0, t.size.y), p) => {
                self.drag = Some((p.y, self.offset));
                true
            }
            _ => false,
        }
    }
    /// True while the thumb is held.
    pub fn dragged(&mut self, p: Vec2d) -> bool {
        let Some((from, offset)) = self.drag else { return false };
        let Some(area) = self.area else { return false };
        let h = self.thumb().map_or(area.size.y, |t| t.size.y);
        let room = (area.size.y - h).max(1.0);
        self.to(offset + (p.y - from) * self.max / room);
        true
    }
    pub fn release(&mut self) -> bool {
        self.drag.take().is_some()
    }
    pub fn draw_thumb(&mut self, d: &mut ShellDraw, cx: &mut Cx2d, k: &MacInk) {
        let now = cx.seconds_since_app_start();
        if std::mem::take(&mut self.moved) {
            self.show_until = now + THUMB_SECONDS;
            self.timer = cx.start_timeout(THUMB_SECONDS + 0.05);
        }
        if let Some(t) = self.thumb() {
            if self.over || self.drag.is_some() || now < self.show_until {
                d.rounded(cx, t, 1.5, k.thumb);
            }
        }
    }
}

/// `r` cut to `area`, for a press on something that scrolls.
pub fn clipped(r: Rect, area: Rect) -> Option<Rect> {
    let x0 = r.pos.x.max(area.pos.x);
    let y0 = r.pos.y.max(area.pos.y);
    let x1 = (r.pos.x + r.size.x).min(area.pos.x + area.size.x);
    let y1 = (r.pos.y + r.size.y).min(area.pos.y + area.size.y);
    (x1 > x0 && y1 > y0).then(|| rect(x0, y0, x1 - x0, y1 - y0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_thumb_comes_back_for_its_strip_not_for_the_content() {
        let mut s = Scroller::default();
        s.layout("rules", rect(100.0, 100.0, 400.0, 300.0), 900.0);
        assert!(!s.pointer_at(dvec2(300.0, 200.0)), "a pointer over the content leaves it gone");
        assert!(s.pointer_at(dvec2(495.0, 200.0)), "the strip at the trailing edge brings it");
        assert!(s.pointer_at(dvec2(300.0, 200.0)), "and leaving the strip sends it away");
    }
}
