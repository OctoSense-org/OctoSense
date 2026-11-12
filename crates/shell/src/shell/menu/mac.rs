//! What `DesktopStyle::Macos` draws in place of the shell's own menu: the menu
//! bar's pull-down menus, Spotlight over the whole tree, Launchpad over the
//! apps, and the About panel. The rows of the pull-down menus come from the shell
//! (`App::mac_menus`, what they do from `App::shell_menu_activate`); Spotlight and
//! Launchpad show the same `MenuModel` every other style's menu does.

use super::*;
use crate::shell::rgb;
use std::time::Instant;

const ROW: f64 = 22.0;
const SEPARATOR: f64 = 11.0;
const SLIDER_ROW: f64 = 30.0;
const SLIDER_H: f64 = 20.0;
const SLIDER_WIDTH: f64 = 260.0;
const PAD_Y: f64 = 5.0;
const PAD_X: f64 = 5.0;
const TEXT: f64 = 13.0;
/// A pull-down menu is no narrower than this.
const MIN_WIDTH: f64 = 150.0;
/// Help's search field: its row, the menu's width while it shows, and how many
/// of the bar's rows it lists.
const SEARCH_ROW: f64 = 30.0;
const SEARCH_WIDTH: f64 = 280.0;
const SEARCH_FOUND: usize = 12;
/// Control Center: its width, the room around its modules and between them, a
/// module's room above and below its rows, and a tile's height.
const CC_WIDTH: f64 = 300.0;
const CC_PAD: f64 = 10.0;
const MODULE_GAP: f64 = 10.0;
const MODULE_PAD: f64 = 6.0;
const TILE_ROW: f64 = 44.0;
/// A Sound menu's output: its row, and the round button its device's glyph sits in.
const DEVICE_ROW: f64 = 32.0;
const DEVICE: f64 = 26.0;
/// A release this long after the press that opened a menu chooses the row under
/// it: press on a title, drag to a row, let go.
const DRAG_RELEASE: f64 = 0.3;

const SPOTLIGHT_WIDTH: f64 = 640.0;
const SPOTLIGHT_FIELD: f64 = 52.0;
const SPOTLIGHT_ROW: f64 = 36.0;
const SPOTLIGHT_ROW_KEYS: f64 = 28.0;
const SPOTLIGHT_PREVIEW_MIN_WIDTH: f64 = 560.0;
/// Room for the preview's icon, name and kind, and an app's facts under them.
const SPOTLIGHT_PREVIEW_MIN_HEIGHT: f64 = 216.0;
/// A row with a note under its name: the name's line, where the note starts,
/// a line of it, and what is left below.
const SPOTLIGHT_NAME_LINE: f64 = 30.0;
const SPOTLIGHT_NOTE_TOP: f64 = 27.0;
const SPOTLIGHT_NOTE_LINE: f64 = 15.0;
const SPOTLIGHT_NOTE_BOTTOM: f64 = 9.0;
/// A group's name over its first result.
const SPOTLIGHT_CAPTION: f64 = 24.0;

const PAD_ICON: f64 = 80.0;
const PAD_LABEL: f64 = 18.0;
/// An icon's cell is at least this wide, and between these tall.
const PAD_CELL_W: f64 = 132.0;
const PAD_CELL_H: f64 = 112.0;
const PAD_CELL_H_MOST: f64 = 144.0;
const PAD_FIELD_W: f64 = 240.0;
const PAD_FIELD_H: f64 = 28.0;
const PAD_FIELD_TOP: f64 = 28.0;
/// Under the grid, above the Dock: where the page dots go.
const PAD_DOTS: f64 = 30.0;
/// Apps that are not something a person opens from Launchpad: Software Update
/// sits in the system menu, and Reference is the developers' sample of an app.
/// Spotlight still finds them.
const PAD_HIDDEN: [&str; 2] = ["apps.updater", "apps.reference"];

/// An app's name on a Mac's surfaces: the launcher's, in Title Case where it
/// is not, and Software Update as the system menu calls it. A Mac's windows say
/// assistant where the shell's own menu says agent.
pub fn app_label(target: &str, label: &str) -> String {
    match target {
        "apps.updater" => "Software Update".to_string(),
        ASK_APP_ROW => "Ask This App's Assistant".to_string(),
        _ => title_case(label),
    }
}

/// Title Case for a row: words in lower case take a capital, unless they are
/// small words inside the name ("At a Glance", but "Sign In"); a word with its
/// own capitals keeps them.
pub fn title_case(label: &str) -> String {
    const SMALL: [&str; 11] = ["a", "an", "and", "at", "for", "in", "of", "on", "or", "the", "to"];
    let last = label.split(' ').count() - 1;
    label
        .split(' ')
        .enumerate()
        .map(|(i, word)| {
            if word.chars().any(char::is_uppercase) || (i > 0 && i < last && SMALL.contains(&word)) {
                return word.to_string();
            }
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a name answers a search as a Mac's does: each word typed starts a
/// word of the name, or the name run together ("apphub" for App Hub). The
/// shell's own search also looks in an app's package name and in the middle of
/// words, so "ma" would find Files (`makepad-files`) and Image Viewer.
pub fn name_matches(label: &str, needle: &str) -> bool {
    let label = label.to_lowercase();
    let words: Vec<&str> = label.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let joined = words.concat();
    needle.split_whitespace().all(|term| {
        words.iter().any(|w| w.starts_with(term)) || joined.starts_with(term) || (!term.chars().all(char::is_alphanumeric) && label.contains(term))
    })
}

/// Launchpad's rows: only what a person opens, the Dock's apps first and the
/// rest by name while browsing, and by how well they match while searching
/// (`needle`, in lower case).
pub fn launchpad_rows(rows: &mut Vec<MenuRow>, needle: &str) {
    rows.retain(|row| row.target.starts_with("apps.") && !PAD_HIDDEN.contains(&row.target.as_str()));
    for row in rows.iter_mut() {
        row.label = app_label(&row.target, &row.label);
    }
    rows.retain(|row| name_matches(&row.label, needle));
    if needle.is_empty() {
        let pinned = |row: &MenuRow| {
            crate::desktop::MAC_DOCK_PINNED
                .iter()
                .position(|id| row.target.strip_prefix("apps.") == Some(*id))
                .unwrap_or(usize::MAX)
        };
        rows.sort_by_cached_key(|row| (pinned(row), row.label.to_lowercase()));
    }
}

/// What a row of Spotlight is, in the words its second column shows.
fn spotlight_kind(target: &str) -> &'static str {
    // These two open a conversation; there is nothing to set in them.
    if target == SYSTEM_CHAT_ROW || target == ASK_APP_ROW {
        return "Command";
    }
    let top = target.split('.').next().unwrap_or(target);
    match top {
        "apps" => "Application",
        GLANCE_ROW => "Notifications",
        "desktop" => "Desktop Style",
        "style" => "Appearance",
        "learn" => "Help",
        "setup" | "settings" => "System Settings",
        "system" => "System",
        _ => "Command",
    }
}

/// What a Mac's Spotlight lists under an app's name in its preview, as far as
/// OctoSense knows it: its kind, its version and where it came from. A built-in
/// ships inside OctoSense, at its version, as its About says; an App Hub app's
/// bundle isn't read here, so it says where it came from alone.
fn spotlight_app_facts(id: &str) -> Vec<(String, String)> {
    let installed = crate::clients::find_app(id)
        .is_some_and(|app| crate::apps::card_manifest_id(&app).is_some_and(|manifest| !manifest.starts_with("os.")));
    let mut facts = vec![("Kind".to_string(), "Application".to_string())];
    if installed {
        facts.push(("Where".to_string(), "Installed from App Hub".to_string()));
    } else {
        facts.push(("Version".to_string(), env!("CARGO_PKG_VERSION").to_string()));
        facts.push(("Where".to_string(), "Comes with OctoSense".to_string()));
    }
    facts
}

/// The glyph of a Spotlight result that is no app and has none of its own.
fn spotlight_glyph(target: &str) -> Ico {
    // The assistant's own mark, as on the menu bar; Approvals is what it asks.
    match target {
        "setup.assistant" | SYSTEM_CHAT_ROW | ASK_APP_ROW => return Ico::Sparkle,
        APPROVALS_ROW => return Ico::Check,
        _ => {}
    }
    match target.split('.').next().unwrap_or(target) {
        GLANCE_ROW => Ico::Bell,
        "desktop" => Ico::Monitor,
        "style" => Ico::Shirt,
        "learn" => Ico::Keyboard,
        "setup" | "settings" => Ico::Cpu,
        "system" => Ico::Power,
        _ => Ico::Sparkle,
    }
}

/// A Spotlight result's tile: a settings pane's in the colour System Settings'
/// sidebar gives it (apps/settings `pane_tint`), grey for the rest.
fn spotlight_tint(target: &str, dark: bool) -> Vec4f {
    match target {
        "settings.appearance" if dark => rgb(72, 72, 74),
        "settings.appearance" => rgb(28, 28, 30),
        "settings.assistant" => rgb(138, 92, 245),
        "settings.general" | "settings.keyboard" => rgb(142, 142, 147),
        _ => rgb(112, 118, 130),
    }
}

/// System Settings' panes, as Spotlight finds them, under the words a person
/// would look for in them, each with the glyph its sidebar row has.
pub fn settings_items() -> Vec<MenuItem> {
    vec![
        MenuItem::new("settings.general", "General", MenuKind::Action)
            .icon(Ico::Gear)
            .aliases(&["about", "version", "software update", "update", "app hub"]),
        MenuItem::new("settings.appearance", "Appearance", MenuKind::Action)
            .icon(Ico::Sun)
            .aliases(&["dark mode", "light mode", "dark", "light", "auto", "desktop style", "style", "theme", "desktop picture", "wallpaper", "background"]),
        MenuItem::new("settings.assistant", "Assistant", MenuKind::Action)
            .icon(Ico::Sparkle)
            .aliases(&["run commands", "commands", "command execution", "agents", "rules", "consent", "ai providers", "models"]),
        MenuItem::new("settings.keyboard", "Keyboard", MenuKind::Action)
            .icon(Ico::Key)
            .aliases(&["keyboard shortcuts", "shortcuts", "keybindings", "keys"]),
    ]
}

/// A setting is changed in System Settings. Spotlight opens its pane, and does
/// not list the rows the shell's other styles keep in their menus.
fn left_to_system_settings(target: &str) -> bool {
    if target == APPROVALS_ROW || target == SYSTEM_CHAT_ROW || target == ASK_APP_ROW || target.starts_with("setup.developer") {
        return false;
    }
    let top = target.split('.').next().unwrap_or(target);
    // A phone's looks are for phones.
    let phone_style = || crate::desktop::DesktopStyle::ALL.iter().any(|style| style.mobile() && target.strip_prefix("desktop.") == Some(style.id()));
    matches!(top, "setup" | "style" | "learn") || target == "desktop" || (top == "desktop" && target.ends_with("-dark")) || phone_style()
}

/// The order of Spotlight's groups, and what each is called over its rows.
const SPOTLIGHT_GROUPS: [(&str, &str); 7] = [
    ("Application", "Applications"),
    ("System Settings", "System Settings"),
    ("Command", "Commands"),
    ("Notifications", "Notifications"),
    ("Desktop Style", "Desktop Styles"),
    ("Help", "Help"),
    ("System", "System"),
];

fn spotlight_group(kind: &str) -> usize {
    SPOTLIGHT_GROUPS.iter().position(|(name, _)| *name == kind).unwrap_or(SPOTLIGHT_GROUPS.len())
}

/// What a result's group is called, over the first row of each, as a Mac's
/// Spotlight heads every group: the first result is the Top Hit, even alone.
fn spotlight_caption(rows: &[MenuRow], i: usize) -> Option<&'static str> {
    if i == 0 {
        return Some("Top Hit");
    }
    let group = SPOTLIGHT_GROUPS.iter().find(|(name, _)| *name == rows[i].detail).map_or("Other", |(_, title)| *title);
    (i == 1 || rows[i].detail != rows[i - 1].detail).then_some(group)
}

/// Spotlight's search results: each row says what it is, in place of the
/// path it was found under; the group of all apps is no result of its own.
/// An app answers to its name; a setting also to its other names (`needle`, in
/// lower case), once they have typed enough to mean one. The best name comes
/// first, then each kind of thing together.
pub fn spotlight_rows(rows: &mut Vec<MenuRow>, needle: &str) {
    // A note about a setting is no result: it is read in the setting's own list.
    rows.retain(|row| row.target != "apps" && !(row.kind == MenuKind::Inert && row.target.starts_with("setup.")) && !left_to_system_settings(&row.target));
    for row in rows.iter_mut() {
        row.detail = spotlight_kind(&row.target).to_string();
        row.label = app_label(&row.target, &row.label);
        row.divider = false;
    }
    rows.retain(|row| name_matches(&row.label, needle) || (!row.target.starts_with("apps.") && needle.chars().count() >= 3));
    let starts = |row: &MenuRow| row.label.to_lowercase().starts_with(needle);
    rows.sort_by_key(|row| (spotlight_group(&row.detail), !starts(row)));
    if let Some(best) = rows.iter().position(|row| starts(row)) {
        let top = rows.remove(best);
        rows.insert(0, top);
    }
}

/// A list a result opened: the Mac's case, and the word its windows use for the
/// assistant. Keyboard Shortcuts' rows are sentences, and stay so.
pub fn spotlight_list(rows: &mut [MenuRow], path: &str) {
    if path == "learn.keybindings" {
        return;
    }
    for row in rows.iter_mut() {
        row.label = app_label(&row.target, &row.label);
        match row.target.as_str() {
            APPROVALS_ROW => row.detail = "Standing rules and recent automatic approvals".to_string(),
            SYSTEM_CHAT_ROW => row.detail = "Talk to the assistant (F8)".to_string(),
            ASK_APP_ROW => row.detail = "Talk to the assistant of the app in front (\u{21e7}F8)".to_string(),
            _ => {}
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MacRow {
    pub label: String,
    /// In the Mac's glyphs: `⇧⌘Q`.
    pub shortcut: String,
    /// What `ShellMenuAction::Activate` carries.
    pub target: String,
    pub enabled: bool,
    pub checked: bool,
    /// A row that can carry a tick, on or off: its menu keeps the room for one.
    pub ticks: bool,
    pub separator: bool,
    /// Words to read, not to press: a status item's menu saying how things stand.
    pub info: bool,
    /// Bold words to read: what a status item's menu is about.
    pub head: bool,
    /// A recent application's identity, drawn in the active shell's icon style.
    pub app_icon: Option<String>,
    /// A level to drag, from 0 to 1. What it moves is its target.
    pub slider: Option<f64>,
    /// Help's search field: what is typed finds the bar's rows.
    pub search: bool,
    pub sub: Vec<MacRow>,
    /// A Control Center tile: its glyph in a circle, lit while on, and how it
    /// stands, under its name.
    pub tile: Option<(Ico, bool, String)>,
    /// A Sound menu's output, as a Mac's: its device's glyph in a round
    /// button, filled in the accent colour while the sound goes there.
    pub device: Option<(Ico, bool)>,
    /// The Control Center module a row sits in, from 1: one module's rows share
    /// its rounded box.
    pub module: u8,
}

impl MacRow {
    pub fn item(label: &str, target: &str) -> Self {
        Self { label: label.into(), target: target.into(), enabled: true, ..Default::default() }
    }
    pub fn sep() -> Self {
        Self { separator: true, ..Default::default() }
    }
    pub fn info(label: &str) -> Self {
        Self { label: label.into(), info: true, ..Default::default() }
    }
    pub fn head(label: &str) -> Self {
        Self { label: label.into(), head: true, ..Default::default() }
    }
    pub fn app_icon(mut self, app: &str) -> Self {
        self.app_icon = Some(app.to_string());
        self
    }
    /// Help's search field, as a Mac's Help menu opens with one.
    pub fn search() -> Self {
        Self { label: "Search".into(), search: true, ..Default::default() }
    }
    pub fn slider(label: &str, target: &str, level: f64) -> Self {
        Self { label: label.into(), target: target.into(), enabled: true, slider: Some(level), ..Default::default() }
    }
    pub fn tile(label: &str, target: &str, ico: Ico, on: bool, state: &str) -> Self {
        Self { label: label.into(), target: target.into(), enabled: true, tile: Some((ico, on, state.into())), ..Default::default() }
    }
    /// An output in a Mac's Sound menu: no tick, the one in use filled.
    pub fn device(label: &str, target: &str, ico: Ico, on: bool) -> Self {
        Self { label: label.into(), target: target.into(), enabled: true, device: Some((ico, on)), ..Default::default() }
    }
    pub fn module(mut self, module: u8) -> Self {
        self.module = module;
        self
    }
    pub fn key(mut self, shortcut: &str) -> Self {
        self.shortcut = shortcut.into();
        self
    }
    pub fn when(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = on;
        self.ticks = true;
        self
    }
    pub fn sub(mut self, rows: Vec<MacRow>) -> Self {
        self.sub = rows;
        self
    }
    fn takes_cursor(&self) -> bool {
        !self.separator && self.enabled && self.slider.is_none()
    }
}

#[derive(Clone, Debug, Default)]
pub struct MacMenu {
    pub rows: Vec<MacRow>,
}

/// The open pull-down menu, and where each of its parts was last drawn.
pub struct MacMenus {
    pub menus: Vec<MacMenu>,
    /// The bar's rect of each menu's title.
    pub titles: Vec<Rect>,
    pub open: usize,
    cursor: Option<usize>,
    sub_open: Option<usize>,
    sub_cursor: Option<usize>,
    card: Rect,
    rows: Vec<Rect>,
    sub_card: Rect,
    sub_rows: Vec<Rect>,
    /// While the press that opened the menu may still be a drag to a row.
    opened: Option<Instant>,
    /// The slider row a press is dragging.
    sliding: Option<usize>,
    /// A menu with no title to move to, and where it opens.
    place: Option<MacPlace>,
    /// What is typed in Help's search field.
    query: String,
    /// The open menu's own rows while its search lists what it found.
    base: Option<Vec<MacRow>>,
}

#[derive(Clone, Copy)]
enum MacPlace {
    /// A Dock icon's menu: up from its column.
    UpFrom(Rect),
    /// The desktop's menu: from the pointer.
    At(Vec2d),
    /// A status item's menu: down from the item, its title in the bar.
    Status,
}

/// The About panel: an app's name and a line or two under its icon, closed by
/// its red light like the real one (or Escape, or a press outside).
#[derive(Clone, Debug, Default)]
pub struct MacAbout {
    pub app: String,
    pub name: String,
    pub lines: Vec<String>,
    /// The © line every Mac About panel ends with; empty for none.
    pub foot: String,
    card: Rect,
    close: Rect,
    over_close: bool,
    /// OctoSense's own panel: More Info…, as About This Mac has.
    more: Rect,
    over_more: bool,
}

impl MacAbout {
    pub fn new(app: &str, name: &str, lines: Vec<String>) -> Self {
        Self { app: app.into(), name: name.into(), lines, ..Default::default() }
    }

    /// The © line under everything else.
    pub fn foot(mut self, foot: &str) -> Self {
        self.foot = foot.into();
        self
    }
}

/// Force Quit Applications: every app with a window, one row each, and Force
/// Quit for the one chosen, which asks first, as a Mac's does.
#[derive(Clone, Debug, Default)]
pub struct MacForceQuit {
    /// Each app's id and name, in the order its first window opened.
    pub apps: Vec<(String, String)>,
    pub chosen: Option<usize>,
    /// Asking whether to force the chosen app to quit, on a sheet.
    pub asking: bool,
    card: Rect,
    close: Rect,
    over_close: bool,
    rows: Vec<Rect>,
    button: Rect,
    over_button: bool,
    cancel: Rect,
    over_cancel: bool,
    confirm: Rect,
    over_confirm: bool,
}

impl MacForceQuit {
    /// The app in front comes chosen, as on a Mac.
    pub fn new(apps: Vec<(String, String)>, chosen: Option<usize>) -> Self {
        Self { chosen: chosen.filter(|i| *i < apps.len()), apps, ..Default::default() }
    }

    /// The row an arrow moves to: the first or the last when none is chosen,
    /// and no further than the list's ends.
    fn step(&self, by: i64) -> Option<usize> {
        let last = self.apps.len().checked_sub(1)? as i64;
        Some(match self.chosen {
            Some(i) => (i as i64 + by).clamp(0, last) as usize,
            None if by > 0 => 0,
            None => last as usize,
        })
    }

    /// What the chosen app's Force Quit activates.
    fn target(&self) -> Option<String> {
        self.chosen.and_then(|i| self.apps.get(i)).map(|(app, _)| format!("mac.force-quit-app.{app}"))
    }
}

/// A Force Quit list's row.
const FORCE_QUIT_ROW: f64 = 24.0;

/// Help's search: the rows of the bar's other menus whose words hold what is
/// typed, a submenu's as "Appearance ▸ Dark", as a Mac's Help menu finds menu
/// items; its own rows again when the field is empty. The first row found is
/// chosen, so Return picks it.
fn help_search(m: &mut MacMenus) {
    let open = m.open;
    let Some(menu) = m.menus.get(open) else { return };
    let base = m.base.get_or_insert_with(|| menu.rows.clone()).clone();
    let query = m.query.trim().to_lowercase();
    let rows = if query.is_empty() {
        base
    } else {
        let found = help_found(&m.menus, open, &query);
        let mut rows: Vec<MacRow> = base.into_iter().take(1).collect();
        rows.push(MacRow::info(if found.is_empty() { "No Menu Items" } else { "Menu Items" }));
        rows.extend(found);
        rows
    };
    m.cursor = if query.is_empty() { None } else { step(&rows, None, 1) };
    if let Some(menu) = m.menus.get_mut(open) {
        menu.rows = rows;
    }
    m.sub_open = None;
    m.sub_cursor = None;
}

/// The rows of every menu but `open` whose label holds `query` (lower case),
/// in the bar's order and no more than a menu's worth. A row found lists its
/// name and keys without its menu's tick column, so the field stays put. Words
/// two menus share are listed once, the first menu's: two lines alike can't be
/// told apart (About OctoSense, with no app in front).
fn help_found(menus: &[MacMenu], open: usize, query: &str) -> Vec<MacRow> {
    let holds = |row: &MacRow| !row.separator && !row.search && row.slider.is_none() && !row.target.is_empty() && row.label.to_lowercase().contains(query);
    let plain = |row: &MacRow, label: String| MacRow { label, ticks: false, checked: false, app_icon: None, ..row.clone() };
    let mut found = Vec::new();
    for (_, menu) in menus.iter().enumerate().filter(|(i, _)| *i != open) {
        for row in &menu.rows {
            if row.sub.is_empty() {
                if holds(row) && !found.iter().any(|f: &MacRow| f.label == row.label) {
                    found.push(plain(row, row.label.clone()));
                }
                continue;
            }
            for sub in row.sub.iter().filter(|sub| holds(sub)) {
                let label = format!("{} \u{25b8} {}", row.label, sub.label);
                if !found.iter().any(|f| f.label == label) {
                    found.push(plain(sub, label));
                }
            }
        }
    }
    found.truncate(SEARCH_FOUND);
    found
}

/// Where a row Help's search found sits: its menu, its row there, and its row in
/// that row's submenu when it is in one.
fn help_place(menus: &[MacMenu], open: usize, found: &MacRow) -> Option<(usize, usize, Option<usize>)> {
    if found.target.is_empty() {
        return None;
    }
    for (mi, menu) in menus.iter().enumerate().filter(|(i, _)| *i != open) {
        for (ri, row) in menu.rows.iter().enumerate() {
            if row.sub.is_empty() {
                if row.target == found.target && row.label == found.label {
                    return Some((mi, ri, None));
                }
                continue;
            }
            for (si, sub) in row.sub.iter().enumerate() {
                if sub.target == found.target && format!("{} \u{25b8} {}", row.label, sub.label) == found.label {
                    return Some((mi, ri, Some(si)));
                }
            }
        }
    }
    None
}

/// A menu taller than the `room` it has keeps every row in reach: the last row it can hold
/// is More, with the others in it. Says whether it had to.
fn fold_to_room(rows: &mut Vec<MacRow>, room: f64) -> bool {
    let height = |row: &MacRow| if row.separator { SEPARATOR } else if row.slider.is_some() { SLIDER_ROW } else if row.device.is_some() { DEVICE_ROW } else { ROW };
    if PAD_Y * 2.0 + rows.iter().map(height).sum::<f64>() <= room {
        return false;
    }
    let mut used = PAD_Y * 2.0 + ROW;
    let held = rows.iter().take_while(|row| { used += height(row); used <= room }).count();
    if held == 0 {
        return false;
    }
    let mut rest = rows.split_off(held);
    while rows.last().is_some_and(|row| row.separator) {
        rows.pop();
    }
    while rest.first().is_some_and(|row| row.separator) {
        rest.remove(0);
    }
    rows.push(MacRow::item("More", "").sub(rest));
    true
}

/// Control Center's rows from the top of its card: where each row starts and how
/// tall it is, the card's height, and each module's box (top, height).
fn cc_layout(rows: &[MacRow]) -> (Vec<(f64, f64)>, f64, Vec<(f64, f64)>) {
    let mut placed = Vec::with_capacity(rows.len());
    let mut boxes: Vec<(f64, f64)> = Vec::new();
    let mut y = CC_PAD;
    let mut module = 0;
    for row in rows {
        if row.module != module {
            if module != 0 {
                y += MODULE_PAD + MODULE_GAP;
            }
            module = row.module;
            boxes.push((y, 0.0));
            y += MODULE_PAD;
        }
        let h = if row.tile.is_some() { TILE_ROW } else if row.slider.is_some() { SLIDER_ROW } else if row.separator { SEPARATOR } else { ROW };
        placed.push((y, h));
        y += h;
        if let Some(last) = boxes.last_mut() {
            last.1 = y + MODULE_PAD - last.0;
        }
    }
    (placed, y + MODULE_PAD + CC_PAD, boxes)
}

fn slider_track(band: Rect) -> Rect {
    rect(band.pos.x + 9.0, band.pos.y + (band.size.y - SLIDER_H) * 0.5, band.size.x - 18.0, SLIDER_H)
}

/// The level a slider row's knob stands at under the pointer.
fn slider_level(band: Rect, x: f64) -> f64 {
    let track = slider_track(band);
    ((x - track.pos.x - SLIDER_H * 0.5) / (track.size.x - SLIDER_H).max(1.0)).clamp(0.0, 1.0)
}

fn row_at(rows: &[Rect], p: Vec2d) -> Option<usize> {
    rows.iter().position(|r| contains(*r, p))
}

fn step(rows: &[MacRow], from: Option<usize>, by: i64) -> Option<usize> {
    let n = rows.len() as i64;
    if n == 0 {
        return None;
    }
    let mut i = match from {
        Some(i) => i as i64 + by,
        None if by > 0 => 0,
        None => n - 1,
    };
    while (0..n).contains(&i) {
        if rows[i as usize].takes_cursor() {
            return Some(i as usize);
        }
        i += by;
    }
    from
}

impl ShellMenu {
    /// A Mac's own surfaces stand in for the menu in this style.
    pub(super) fn mac_surface(&self) -> bool {
        self.desktop_style == DesktopStyle::Macos && self.anchor.is_none()
    }

    pub(super) fn launchpad(&self) -> bool {
        self.mac_surface() && self.mac.is_none() && self.about.is_none() && self.force_quit.is_none() && self.model.skin == MenuSkin::Launcher
    }

    pub(super) fn spotlight(&self) -> bool {
        self.mac_surface() && self.mac.is_none() && self.about.is_none() && self.force_quit.is_none() && self.model.skin == MenuSkin::Menu
    }

    /// Open the menu bar's menu at `index`: `titles` are the places of its titles.
    pub fn open_mac(&mut self, cx: &mut Cx, menus: Vec<MacMenu>, titles: Vec<Rect>, index: usize) {
        self.anchor = None;
        self.touch_press = None;
        self.about = None;
        self.force_quit = None;
        self.model.open = true;
        crate::desktop::set_mac_launchpad(false);
        self.model.skin = MenuSkin::Menu;
        self.model.rows.clear();
        self.mac = Some(MacMenus {
            menus,
            titles,
            open: index,
            cursor: None,
            sub_open: None,
            sub_cursor: None,
            card: Rect::default(),
            rows: Vec::new(),
            sub_card: Rect::default(),
            sub_rows: Vec::new(),
            opened: Some(Instant::now()),
            sliding: None,
            place: None,
            query: String::new(),
            base: None,
        });
        self.redraw(cx);
    }

    /// Open a Dock icon's menu, up from `icon`.
    pub fn open_mac_above(&mut self, cx: &mut Cx, menu: MacMenu, icon: Rect) {
        self.open_mac(cx, vec![menu], Vec::new(), 0);
        if let Some(mac) = self.mac.as_mut() {
            mac.place = Some(MacPlace::UpFrom(icon));
        }
    }

    /// Open the desktop's menu at the pointer.
    pub fn open_mac_at(&mut self, cx: &mut Cx, menu: MacMenu, at: Vec2d) {
        self.open_mac(cx, vec![menu], Vec::new(), 0);
        if let Some(mac) = self.mac.as_mut() {
            mac.place = Some(MacPlace::At(at));
        }
    }

    /// Open a status item's menu, down from it. `items` hold every item's place, so the
    /// others are a move away, as the bar's menus are.
    pub fn open_mac_status(&mut self, cx: &mut Cx, menus: Vec<MacMenu>, items: Vec<Rect>, index: usize) {
        self.open_mac(cx, menus, items, index);
        if let Some(mac) = self.mac.as_mut() {
            mac.place = Some(MacPlace::Status);
        }
    }

    /// Which status item's menu is open, for the item to show it.
    pub fn mac_status_open(&self) -> Option<usize> {
        self.mac.as_ref().filter(|m| self.model.open && matches!(m.place, Some(MacPlace::Status))).map(|m| m.open)
    }

    /// The open pull-down menu, for the bar's title to show it.
    pub fn mac_open(&self) -> Option<usize> {
        self.mac.as_ref().filter(|m| self.model.open && m.place.is_none()).map(|m| m.open)
    }

    /// The open menu's rows where they were last drawn, for a test to press one.
    pub fn mac_drawn_rows(&self) -> Vec<(String, Rect)> {
        // Force Quit Applications: its apps and its button, or its sheet's two.
        if let Some(panel) = self.force_quit.as_ref().filter(|_| self.model.open) {
            if panel.asking {
                return vec![("Cancel".to_string(), panel.cancel), ("Force Quit".to_string(), panel.confirm)];
            }
            let mut drawn: Vec<_> = panel.apps.iter().zip(&panel.rows).map(|((_, name), at)| (name.clone(), *at)).collect();
            if panel.chosen.is_some() {
                drawn.push(("Force Quit".to_string(), panel.button));
            }
            return drawn;
        }
        // An About panel's button, by its words, as a menu's rows are.
        if let Some(about) = self.about.as_ref().filter(|about| self.model.open && about.more.size.x > 0.0) {
            return vec![("More Info\u{2026}".to_string(), about.more)];
        }
        let Some(mac) = self.mac.as_ref().filter(|_| self.model.open) else { return Vec::new() };
        let rows = mac.menus.get(mac.open).map(|m| m.rows.as_slice()).unwrap_or_default();
        let mut drawn: Vec<_> = rows.iter().zip(&mac.rows).filter(|(row, _)| !row.separator).map(|(row, at)| (row.label.clone(), *at)).collect();
        if let Some(sub) = mac.sub_open.and_then(|i| rows.get(i)) {
            drawn.extend(sub.sub.iter().zip(&mac.sub_rows).filter(|(row, _)| !row.separator).map(|(row, at)| (row.label.clone(), *at)));
        }
        drawn
    }

    pub fn open_about(&mut self, cx: &mut Cx, about: MacAbout) {
        self.anchor = None;
        self.mac = None;
        self.force_quit = None;
        self.model.open = true;
        crate::desktop::set_mac_launchpad(false);
        self.model.skin = MenuSkin::Menu;
        self.model.rows.clear();
        self.about = Some(about);
        self.redraw(cx);
    }

    pub fn open_force_quit(&mut self, cx: &mut Cx, panel: MacForceQuit) {
        self.anchor = None;
        self.mac = None;
        self.about = None;
        self.model.open = true;
        crate::desktop::set_mac_launchpad(false);
        self.model.skin = MenuSkin::Menu;
        self.model.rows.clear();
        self.force_quit = Some(panel);
        self.redraw(cx);
    }

    fn mac_ink(&self) -> (Vec4f, Vec4f) {
        if self.dark {
            (rgb(245, 245, 247), rgb(10, 96, 200))
        } else {
            (rgb(28, 28, 30), rgb(0, 100, 210))
        }
    }

    /// A menu's width and height, and where its labels start.
    fn mac_card_size(&mut self, cx: &mut Cx2d, rows: &[MacRow]) -> (f64, f64, f64) {
        if rows.iter().any(|r| r.module > 0) {
            return (CC_WIDTH, cc_layout(rows).1, CC_PAD + 12.0);
        }
        let label_x = PAD_X + 9.0
            + if rows.iter().any(|r| r.ticks) { 10.0 } else { 0.0 }
            + if rows.iter().any(|r| r.app_icon.is_some()) { 22.0 } else { 0.0 };
        let mut width = MIN_WIDTH;
        let mut height = PAD_Y * 2.0;
        for row in rows {
            if row.separator {
                height += SEPARATOR;
                continue;
            }
            if row.slider.is_some() {
                height += SLIDER_ROW;
                width = width.max(SLIDER_WIDTH);
                continue;
            }
            if row.search {
                height += SEARCH_ROW;
                width = width.max(SEARCH_WIDTH);
                continue;
            }
            if row.device.is_some() {
                height += DEVICE_ROW;
                let label = self.d.measure(cx, false, TEXT, &row.label);
                width = width.max(PAD_X + 9.0 + DEVICE + 8.0 + label + PAD_X + 14.0);
                continue;
            }
            height += ROW;
            let label = self.d.measure(cx, row.head, TEXT, &row.label);
            let trailing = if !row.sub.is_empty() {
                34.0
            } else if !row.shortcut.is_empty() {
                self.d.measure(cx, false, TEXT, &row.shortcut) + 30.0
            } else {
                0.0
            };
            width = width.max(label_x + label + trailing + PAD_X + 9.0);
        }
        (width.ceil(), height, label_x)
    }

    fn draw_mac_card(&mut self, cx: &mut Cx2d, card: Rect, rows: &[MacRow], cursor: Option<usize>, label_x: f64, query: &str) -> Vec<Rect> {
        let (ink, accent) = self.mac_ink();
        let surface = self.skin().surface;
        // A menu of one row would be a capsule.
        self.d.card_no_rounder(cx, card, &surface, card.size.y * 0.25);
        if rows.iter().any(|r| r.module > 0) {
            return self.draw_mac_cc(cx, card, rows, cursor);
        }
        let mut hits = Vec::with_capacity(rows.len());
        let mut y = card.pos.y + PAD_Y;
        for (i, row) in rows.iter().enumerate() {
            if row.separator {
                self.d.solid(cx, rect(card.pos.x + 14.0, (y + SEPARATOR * 0.5).floor(), card.size.x - 28.0, 1.0), fade(ink, 0.14));
                hits.push(Rect::default());
                y += SEPARATOR;
                continue;
            }
            if row.search {
                // "Search" and its field, which has the keyboard: a Mac's blue ring.
                let band = rect(card.pos.x + PAD_X, y, card.size.x - PAD_X * 2.0, SEARCH_ROW);
                hits.push(band);
                let left = card.pos.x + label_x;
                let label_w = self.d.measure(cx, false, TEXT, &row.label).ceil();
                self.d.label(cx, rect(left, y, label_w, SEARCH_ROW), false, TEXT, ink, super::super::ui::HAlign::Left, &row.label);
                let field_x = left + label_w + 8.0;
                let field = rect(field_x, y + 4.0, (card.pos.x + card.size.x - PAD_X - 9.0 - field_x).max(60.0), SEARCH_ROW - 8.0);
                self.d.rounded(cx, field, 5.0, if self.dark { rgb(58, 58, 60) } else { rgb(255, 255, 255) });
                self.d.focus_ring(cx, field, 5.0, 2.0, fade(accent, 0.55));
                self.d.icon_centered(cx, Ico::Search, rect(field.pos.x + 3.0, field.pos.y, 16.0, field.size.y), 11.0, fade(ink, 0.55));
                let text = rect(field.pos.x + 21.0, field.pos.y, field.size.x - 25.0, field.size.y);
                self.d.label_elided(cx, text, false, TEXT, ink, super::super::ui::HAlign::Left, query);
                let caret = (self.d.measure(cx, false, TEXT, query) + 1.0).min(text.size.x);
                self.d.solid(cx, rect(text.pos.x + caret, field.pos.y + 4.0, 1.0, field.size.y - 8.0), ink);
                y += SEARCH_ROW;
                continue;
            }
            if let Some(level) = row.slider {
                let band = rect(card.pos.x + PAD_X, y, card.size.x - PAD_X * 2.0, SLIDER_ROW);
                hits.push(band);
                self.draw_mac_slider(cx, band, level, ink);
                y += SLIDER_ROW;
                continue;
            }
            if let Some((ico, current)) = row.device {
                // A Sound menu's output, as a Mac's: a grey wash under the
                // pointer, the device's glyph in its round button, the one in
                // use filled in the accent colour, and no tick.
                let band = rect(card.pos.x + PAD_X, y, card.size.x - PAD_X * 2.0, DEVICE_ROW);
                hits.push(band);
                if cursor == Some(i) && row.enabled {
                    self.d.rounded(cx, band, 3.0, fade(ink, 0.1));
                }
                let circle = rect(band.pos.x + 9.0, y + ((DEVICE_ROW - DEVICE) * 0.5).round(), DEVICE, DEVICE);
                self.d.rounded(cx, circle, DEVICE * 0.5, if current { accent } else { fade(ink, 0.1) });
                self.d.icon_centered(cx, ico, circle, 13.0, if current { rgb(255, 255, 255) } else { ink });
                let text_x = circle.pos.x + DEVICE + 8.0;
                self.d.label_elided(cx, rect(text_x, y, (band.pos.x + band.size.x - text_x - 6.0).max(1.0), DEVICE_ROW), false, TEXT, ink, super::super::ui::HAlign::Left, &row.label);
                y += DEVICE_ROW;
                continue;
            }
            let band = rect(card.pos.x + PAD_X, y, card.size.x - PAD_X * 2.0, ROW);
            hits.push(band);
            let on = cursor == Some(i) && row.enabled;
            if on {
                self.d.rounded(cx, band, 2.5, accent);
            }
            let color = if on { rgb(255, 255, 255) } else if row.enabled || row.head { ink } else if row.info { fade(ink, 0.72) } else { fade(ink, 0.36) };
            if row.checked {
                self.d.icon_centered(cx, Ico::Check, rect(card.pos.x + PAD_X + 3.0, y, 14.0, ROW), 11.0, color);
            }
            let right = card.pos.x + card.size.x - PAD_X - 9.0;
            let left = card.pos.x + label_x;
            if let Some(app) = &row.app_icon {
                self.app_icons.draw(cx, app, self.desktop_style,
                    rect(left - 20.0, y + (ROW - 16.0) * 0.5, 16.0, 16.0), 1.0, color);
            }
            let shortcut_w = if row.shortcut.is_empty() { 0.0 } else { self.d.measure(cx, false, TEXT, &row.shortcut) };
            self.d.label_elided(cx, rect(left, y, (right - left - shortcut_w).max(1.0), ROW), row.head, TEXT, color, super::super::ui::HAlign::Left, &row.label);
            if !row.sub.is_empty() {
                self.d.icon_centered(cx, Ico::ChevronRight, rect(right - 12.0, y, 12.0, ROW), 10.0, color);
            } else if !row.shortcut.is_empty() {
                let keys = if on { color } else if row.enabled || row.head { fade(ink, 0.62) } else { fade(ink, 0.3) };
                self.d.label(cx, rect(right - shortcut_w, y, shortcut_w, ROW), false, TEXT, keys, super::super::ui::HAlign::Right, &row.shortcut);
            }
            y += ROW;
        }
        hits
    }

    /// Control Center: its modules' boxes, and in them the tiles, a module's name and
    /// the sound slider. One hit rect per row, as a menu's.
    fn draw_mac_cc(&mut self, cx: &mut Cx2d, card: Rect, rows: &[MacRow], cursor: Option<usize>) -> Vec<Rect> {
        let (ink, accent) = self.mac_ink();
        let (placed, _, boxes) = cc_layout(rows);
        let fill = if self.dark { fade(rgb(255, 255, 255), 0.09) } else { fade(rgb(255, 255, 255), 0.62) };
        let left = card.pos.x + CC_PAD;
        let width = card.size.x - CC_PAD * 2.0;
        for (top, h) in &boxes {
            self.d.rounded(cx, rect(left, card.pos.y + top, width, *h), 12.0, fill);
        }
        let mut hits = Vec::with_capacity(rows.len());
        for (i, (row, (top, h))) in rows.iter().zip(placed).enumerate() {
            let y = card.pos.y + top;
            let band = rect(left + 4.0, y, width - 8.0, h);
            hits.push(band);
            if let Some(level) = row.slider {
                self.draw_mac_slider(cx, band, level, ink);
                continue;
            }
            if let Some((ico, on, state)) = &row.tile {
                if cursor == Some(i) {
                    self.d.rounded(cx, band, 8.0, fade(ink, 0.08));
                }
                let circle = rect(band.pos.x + 6.0, y + ((h - 28.0) * 0.5).round(), 28.0, 28.0);
                self.d.rounded(cx, circle, 14.0, if *on { accent } else { fade(ink, 0.12) });
                self.d.icon_centered(cx, *ico, circle, 14.0, if *on { rgb(255, 255, 255) } else { ink });
                let text_x = circle.pos.x + circle.size.x + 9.0;
                let text_w = (band.pos.x + band.size.x - text_x - 6.0).max(1.0);
                self.d.label_elided(cx, rect(text_x, y + 6.0, text_w, 17.0), true, TEXT, ink, super::super::ui::HAlign::Left, &row.label);
                self.d.label_elided(cx, rect(text_x, y + 23.0, text_w, 14.0), false, 11.0, fade(ink, 0.6), super::super::ui::HAlign::Left, state);
                continue;
            }
            if row.separator {
                self.d.solid(cx, rect(band.pos.x + 8.0, (y + h * 0.5).floor(), band.size.x - 16.0, 1.0), fade(ink, 0.12));
                continue;
            }
            // A module's name, words to read in it, or a row to choose (its settings).
            let chosen = cursor == Some(i) && row.takes_cursor();
            if chosen {
                self.d.rounded(cx, band, 6.0, fade(ink, 0.08));
            }
            let color = if row.head || row.takes_cursor() { ink } else { fade(ink, 0.6) };
            self.d.label_elided(cx, rect(band.pos.x + 8.0, y, band.size.x - 16.0, h), row.head, TEXT, color, super::super::ui::HAlign::Left, &row.label);
        }
        hits
    }

    /// Help's blue arrow, white edged, pointing at the row `at` from beside its menu
    /// `card`: from the left, or from the right where the left has no room.
    fn draw_help_arrow(&mut self, cx: &mut Cx2d, at: Rect, card: Rect, screen: Rect) {
        const SIZE: f64 = 30.0;
        let (_, accent) = self.mac_ink();
        let mid = at.pos.y + at.size.y * 0.5;
        let (ico, x) = if card.pos.x - SIZE - 6.0 >= screen.pos.x + 2.0 {
            (Ico::PointRight, card.pos.x - SIZE - 2.0)
        } else {
            (Ico::PointLeft, card.pos.x + card.size.x + 2.0)
        };
        let place = rect(x, mid - SIZE * 0.5, SIZE, SIZE);
        self.d.icon_centered(cx, ico, place, SIZE + 5.0, rgb(255, 255, 255));
        self.d.icon_centered(cx, ico, place, SIZE, accent);
    }

    /// A Mac's sound slider: a capsule filled up to its knob, the speaker on the fill.
    fn draw_mac_slider(&mut self, cx: &mut Cx2d, band: Rect, level: f64, ink: Vec4f) {
        let track = slider_track(band);
        self.d.rounded(cx, track, SLIDER_H * 0.5, fade(ink, 0.16));
        let knob = track.pos.x + (track.size.x - SLIDER_H) * level;
        self.d.rounded(cx, rect(track.pos.x, track.pos.y, knob + SLIDER_H - track.pos.x, SLIDER_H), SLIDER_H * 0.5, rgb(255, 255, 255));
        self.d.focus_ring(cx, rect(knob, track.pos.y, SLIDER_H, SLIDER_H), SLIDER_H * 0.5, 0.5, fade(rgb(0, 0, 0), 0.18));
        self.d.icon_centered(cx, Ico::Speaker, rect(track.pos.x + 3.0, track.pos.y, 15.0, SLIDER_H), 11.0, fade(rgb(28, 28, 30), 0.72));
    }

    pub(super) fn draw_mac_menus(&mut self, cx: &mut Cx2d, screen: Rect) {
        let Some(mut menus) = self.mac.take() else { return };
        self.screen = screen;
        let mut rows = menus.menus.get(menus.open).map(|m| m.rows.clone()).unwrap_or_default();
        let title = menus.titles.get(menus.open).copied().unwrap_or_default();
        let room = match menus.place {
            Some(MacPlace::UpFrom(icon)) => icon.pos.y - screen.pos.y - 10.0,
            Some(MacPlace::At(_)) => screen.size.y - 8.0,
            Some(MacPlace::Status) | None => screen.pos.y + screen.size.y - title.pos.y - title.size.y - 5.0,
        };
        if fold_to_room(&mut rows, room) {
            if let Some(menu) = menus.menus.get_mut(menus.open) {
                menu.rows = rows.clone();
            }
        }
        let (width, height, label_x) = self.mac_card_size(cx, &rows);
        let right = screen.pos.x + screen.size.x - 4.0;
        menus.card = match menus.place {
            Some(MacPlace::UpFrom(icon)) => {
                let left = (icon.pos.x + (icon.size.x - width) * 0.5).min(right - width).max(screen.pos.x + 4.0);
                rect(left, (icon.pos.y - height - 6.0).max(screen.pos.y + 4.0), width, height)
            }
            Some(MacPlace::At(at)) => {
                let bottom = screen.pos.y + screen.size.y - 4.0;
                rect(at.x.min(right - width).max(screen.pos.x + 4.0), at.y.min(bottom - height).max(screen.pos.y + 4.0), width, height)
            }
            Some(MacPlace::Status) | None => {
                let left = title.pos.x.min(right - width).max(screen.pos.x + 4.0);
                rect(left, title.pos.y + title.size.y + 1.0, width, height)
            }
        };
        // Help's search: the row chosen among those it found shows where it is, in its
        // own menu at its title (and its submenu), with a Mac's blue arrow at it.
        let pointed = if menus.base.is_some() { menus.cursor.and_then(|i| rows.get(i)).and_then(|found| help_place(&menus.menus, menus.open, found)) } else { None };
        if let Some((mi, ri, si)) = pointed {
            let held = menus.menus[mi].rows.clone();
            let held_title = menus.titles.get(mi).copied().unwrap_or_default();
            let (w, h, x) = self.mac_card_size(cx, &held);
            let card = rect(held_title.pos.x.min(right - w).max(screen.pos.x + 4.0), held_title.pos.y + held_title.size.y + 1.0, w, h);
            let drawn = self.draw_mac_card(cx, card, &held, Some(ri), x, "");
            let mut at = drawn.get(ri).copied().unwrap_or_default();
            let mut beside = card;
            if let Some(si) = si {
                let sub = held[ri].sub.clone();
                let (sw, sh, sx) = self.mac_card_size(cx, &sub);
                let next = card.pos.x + card.size.x - 3.0;
                let sub_x = if next + sw > right { card.pos.x - sw + 3.0 } else { next };
                let sub_y = (at.pos.y - PAD_Y).min(screen.pos.y + screen.size.y - 4.0 - sh).max(screen.pos.y + 4.0);
                beside = rect(sub_x, sub_y, sw, sh);
                let sub_drawn = self.draw_mac_card(cx, beside, &sub, Some(si), sx, "");
                at = sub_drawn.get(si).copied().unwrap_or(at);
            }
            self.draw_help_arrow(cx, at, beside, screen);
        }
        menus.rows = self.draw_mac_card(cx, menus.card, &rows, menus.cursor, label_x, &menus.query);
        let sub = menus.sub_open.and_then(|i| rows.get(i).map(|row| (i, row.sub.clone()))).filter(|(_, s)| !s.is_empty());
        if let Some((i, sub_rows)) = sub {
            let (sub_w, sub_h, sub_x) = self.mac_card_size(cx, &sub_rows);
            let beside = menus.card.pos.x + menus.card.size.x - 3.0;
            let x = if beside + sub_w > right { menus.card.pos.x - sub_w + 3.0 } else { beside };
            let bottom = screen.pos.y + screen.size.y - 4.0;
            let y = (menus.rows[i].pos.y - PAD_Y).min(bottom - sub_h).max(screen.pos.y + 4.0);
            menus.sub_card = rect(x, y, sub_w, sub_h);
            menus.sub_rows = self.draw_mac_card(cx, menus.sub_card, &sub_rows, menus.sub_cursor, sub_x, "");
        } else {
            menus.sub_card = Rect::default();
            menus.sub_rows.clear();
        }
        self.mac = Some(menus);
    }

    fn mac_finish(&mut self, cx: &mut Cx, activate: Option<String>) {
        self.model.close();
        self.mac = None;
        self.redraw(cx);
        cx.widget_action(self.uid, match activate {
            Some(target) => ShellMenuAction::Activate(target),
            None => ShellMenuAction::Cancel,
        });
    }

    pub(super) fn mac_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let Some(mut m) = self.mac.take() else { return false };
        let rows = m.menus.get(m.open).map(|menu| menu.rows.clone()).unwrap_or_default();
        let sub_rows = m.sub_open.and_then(|i| rows.get(i)).map(|row| row.sub.clone()).unwrap_or_default();
        let mut finish: Option<Option<String>> = None;
        let mut switched = false;
        let mut slide: Option<(usize, f64)> = None;
        match event {
            Event::MouseMove(e) => {
                if let Some(i) = m.sliding {
                    slide = Some((i, slider_level(m.rows[i], e.abs.x)));
                } else if let Some(i) = row_at(&m.titles, e.abs) {
                    if i != m.open {
                        m.open = i;
                        m.cursor = None;
                        m.sub_open = None;
                        m.sub_cursor = None;
                        switched = true;
                    }
                } else if contains(m.sub_card, e.abs) {
                    m.sub_cursor = row_at(&m.sub_rows, e.abs).filter(|i| sub_rows.get(*i).is_some_and(MacRow::takes_cursor));
                } else if contains(m.card, e.abs) {
                    let under = row_at(&m.rows, e.abs);
                    m.cursor = under.filter(|i| rows.get(*i).is_some_and(MacRow::takes_cursor));
                    m.sub_open = m.cursor.filter(|i| !rows[*i].sub.is_empty());
                    m.sub_cursor = None;
                } else if m.sub_open.is_none() {
                    m.cursor = None;
                }
            }
            Event::MouseDown(e) => {
                m.opened = None;
                if let Some(i) = row_at(&m.titles, e.abs) {
                    if i == m.open {
                        finish = Some(None);
                    } else {
                        m.open = i;
                        m.cursor = None;
                        m.sub_open = None;
                        m.sub_cursor = None;
                        switched = true;
                    }
                } else if contains(m.sub_card, e.abs) {
                    if let Some(row) = row_at(&m.sub_rows, e.abs).and_then(|i| sub_rows.get(i)).filter(|r| r.takes_cursor()) {
                        finish = Some(Some(row.target.clone()));
                    }
                } else if contains(m.card, e.abs) {
                    if let Some(i) = row_at(&m.rows, e.abs).filter(|i| rows.get(*i).is_some_and(|r| r.slider.is_some())) {
                        m.sliding = Some(i);
                        slide = Some((i, slider_level(m.rows[i], e.abs.x)));
                    } else if let Some(i) = row_at(&m.rows, e.abs).filter(|i| rows.get(*i).is_some_and(MacRow::takes_cursor)) {
                        if rows[i].sub.is_empty() {
                            finish = Some(Some(rows[i].target.clone()));
                        } else {
                            m.cursor = Some(i);
                            m.sub_open = Some(i);
                        }
                    }
                } else {
                    finish = Some(None);
                }
            }
            Event::MouseUp(e) => {
                m.sliding = None;
                if let Some(opened) = m.opened.take() {
                    if opened.elapsed().as_secs_f64() > DRAG_RELEASE {
                        if contains(m.sub_card, e.abs) {
                            if let Some(row) = row_at(&m.sub_rows, e.abs).and_then(|i| sub_rows.get(i)).filter(|r| r.takes_cursor()) {
                                finish = Some(Some(row.target.clone()));
                            }
                        } else if let Some(i) = row_at(&m.rows, e.abs).filter(|i| rows.get(*i).is_some_and(|r| r.takes_cursor() && r.sub.is_empty())) {
                            finish = Some(Some(rows[i].target.clone()));
                        }
                    }
                }
            }
            Event::Scroll(_) => {}
            _ => {
                self.mac = Some(m);
                return false;
            }
        }
        if let Some((i, level)) = slide {
            if let Some(row) = m.menus.get_mut(m.open).and_then(|menu| menu.rows.get_mut(i)) {
                row.slider = Some(level);
                cx.widget_action(self.uid, ShellMenuAction::MacSlide(row.target.clone(), level));
            }
        }
        self.mac = Some(m);
        match finish {
            Some(activate) => self.mac_finish(cx, activate),
            None => {
                if switched {
                    cx.widget_action(self.uid, ShellMenuAction::MacSwitched);
                }
                self.redraw(cx);
            }
        }
        true
    }

    pub(super) fn mac_key(&mut self, cx: &mut Cx, e: &KeyEvent) -> bool {
        let Some(mut m) = self.mac.take() else { return false };
        let rows = m.menus.get(m.open).map(|menu| menu.rows.clone()).unwrap_or_default();
        let sub_rows = m.sub_open.and_then(|i| rows.get(i)).map(|row| row.sub.clone()).unwrap_or_default();
        let count = m.menus.len().max(1);
        let in_sub = m.sub_cursor.is_some();
        // Help's search field has the keyboard: what is typed finds the bar's rows.
        if rows.first().is_some_and(|row| row.search) && !in_sub && !e.modifiers.logo && !e.modifiers.control && !e.modifiers.alt {
            let typed = match e.key_code {
                KeyCode::Backspace => m.query.pop().is_some(),
                KeyCode::Escape if !m.query.is_empty() => {
                    m.query.clear();
                    true
                }
                code => match code.to_char(e.modifiers.shift).filter(|ch| *ch as u32 >= 32 && *ch as u32 != 127) {
                    Some(ch) => {
                        m.query.push(ch);
                        true
                    }
                    None => false,
                },
            };
            if typed {
                help_search(&mut m);
                self.mac = Some(m);
                self.redraw(cx);
                return true;
            }
        }
        let mut finish: Option<Option<String>> = None;
        let mut switched = false;
        match e.key_code {
            KeyCode::Escape => {
                if m.sub_open.is_some() {
                    m.sub_open = None;
                    m.sub_cursor = None;
                } else {
                    finish = Some(None);
                }
            }
            KeyCode::ArrowDown if in_sub => m.sub_cursor = step(&sub_rows, m.sub_cursor, 1),
            KeyCode::ArrowUp if in_sub => m.sub_cursor = step(&sub_rows, m.sub_cursor, -1),
            KeyCode::ArrowDown => {
                m.cursor = step(&rows, m.cursor, 1);
                m.sub_open = None;
            }
            KeyCode::ArrowUp => {
                m.cursor = step(&rows, m.cursor, -1);
                m.sub_open = None;
            }
            KeyCode::ArrowRight => {
                if let Some(i) = m.cursor.filter(|i| !rows[*i].sub.is_empty() && !in_sub) {
                    m.sub_open = Some(i);
                    m.sub_cursor = step(&rows[i].sub, None, 1);
                } else {
                    m.open = (m.open + 1) % count;
                    m.cursor = None;
                    m.sub_open = None;
                    m.sub_cursor = None;
                    switched = true;
                }
            }
            KeyCode::ArrowLeft => {
                if in_sub {
                    m.sub_open = None;
                    m.sub_cursor = None;
                } else {
                    m.open = (m.open + count - 1) % count;
                    m.cursor = None;
                    m.sub_open = None;
                    switched = true;
                }
            }
            KeyCode::ReturnKey | KeyCode::NumpadEnter | KeyCode::Space => {
                if let Some(row) = m.sub_cursor.and_then(|i| sub_rows.get(i)) {
                    finish = Some(Some(row.target.clone()));
                } else if let Some(i) = m.cursor {
                    if rows[i].sub.is_empty() {
                        finish = Some(Some(rows[i].target.clone()));
                    } else {
                        m.sub_open = Some(i);
                        m.sub_cursor = step(&rows[i].sub, None, 1);
                    }
                }
            }
            _ => {}
        }
        self.mac = Some(m);
        match finish {
            Some(activate) => self.mac_finish(cx, activate),
            None => {
                if switched {
                    cx.widget_action(self.uid, ShellMenuAction::MacSwitched);
                }
                self.redraw(cx);
            }
        }
        true
    }

    // ---- the About panel

    pub(super) fn draw_about(&mut self, cx: &mut Cx2d, screen: Rect) {
        let Some(mut about) = self.about.take() else { return };
        self.screen = screen;
        let (ink, _) = self.mac_ink();
        let width: f64 = 300.0;
        // Its words keep well in from the panel's edges.
        let lines: Vec<String> = about.lines.iter().flat_map(|line| self.d.wrap_lines(cx, false, 11.0, line, width - 56.0, false)).collect();
        let more = about.app.is_empty();
        let foot = !about.foot.is_empty();
        let height = 34.0
            + 72.0
            + 14.0
            + 22.0
            + 18.0 * lines.len() as f64
            + 22.0
            + if more { 8.0 + crate::approvals::mac_kit::CONTROL_H } else { 0.0 }
            + if foot { 8.0 + 16.0 } else { 0.0 };
        let card = rect(
            (screen.pos.x + (screen.size.x - width) * 0.5).floor(),
            (screen.pos.y + (screen.size.y - height) * 0.4).floor(),
            width,
            height,
        );
        about.card = card;
        let surface = self.skin().surface;
        self.d.card(cx, card, &surface);
        about.close = self.d.mac_panel_lights(
            cx,
            card.pos.x + 9.0,
            card.pos.y + 9.0,
            super::super::ui::PanelLights { dark: self.dark, front: true, over: about.over_close, down: false },
        );
        let mut y = card.pos.y + 34.0;
        let icon = rect(card.pos.x + (width - 72.0) * 0.5, y, 72.0, 72.0);
        if about.app.is_empty() {
            // OctoSense's own icon: its mark on its dark green tile.
            self.d.rounded(cx, icon, 16.0, rgb(36, 63, 48));
            self.d.icon_centered(cx, Ico::Octopus, icon, 50.0, rgb(212, 237, 184));
        } else {
            self.app_icons.draw(cx, &about.app, self.desktop_style, icon, 1.0, ink);
        }
        y += 72.0 + 14.0;
        self.d.label(cx, rect(card.pos.x, y, width, 20.0), true, 14.0, ink, super::super::ui::HAlign::Center, &about.name);
        y += 22.0;
        for line in &lines {
            self.d.label(cx, rect(card.pos.x + 16.0, y, width - 32.0, 18.0), false, 11.0, fade(ink, 0.7), super::super::ui::HAlign::Center, line);
            y += 18.0;
        }
        about.more = Rect::default();
        if more {
            // As About This Mac: More Info… opens Settings' General, where the
            // version and its updates are.
            use crate::approvals::mac_kit::{mac_ink, push_button, Face, CONTROL_H};
            let label = "More Info\u{2026}";
            let w = (self.d.measure(cx, false, 13.0, label) + 28.0).max(96.0).ceil();
            let r = rect((card.pos.x + (width - w) * 0.5).floor(), y + 8.0, w, CONTROL_H);
            push_button(&mut self.d, cx, r, &mac_ink(self.dark), Face::Plain, about.over_more, false, label);
            about.more = r;
            y = r.pos.y + r.size.y;
        }
        if foot {
            // Under everything else, smaller and fainter, as a Mac's is.
            self.d.label(cx, rect(card.pos.x + 16.0, y + 8.0, width - 32.0, 16.0), false, 10.0, fade(ink, 0.55), super::super::ui::HAlign::Center, &about.foot);
        }
        self.about = Some(about);
    }

    pub(super) fn about_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let Some(about) = self.about.as_mut() else { return false };
        match event {
            Event::MouseDown(e) => {
                let target = about.close;
                let hit = rect(target.pos.x - 3.0, target.pos.y - 3.0, target.size.x + 6.0, target.size.y + 6.0);
                if about.more.size.x > 0.0 && contains(about.more, e.abs) {
                    self.about = None;
                    self.mac_finish(cx, Some("mac.about-more".into()));
                } else if contains(hit, e.abs) || !contains(about.card, e.abs) {
                    self.about = None;
                    self.mac_finish(cx, None);
                }
                true
            }
            Event::MouseMove(e) => {
                let over = contains(about.close, e.abs);
                let over_more = about.more.size.x > 0.0 && contains(about.more, e.abs);
                if over != about.over_close || over_more != about.over_more {
                    about.over_close = over;
                    about.over_more = over_more;
                    self.redraw(cx);
                }
                true
            }
            Event::MouseUp(_) | Event::Scroll(_) => true,
            _ => false,
        }
    }

    pub(super) fn about_key(&mut self, cx: &mut Cx, e: &KeyEvent) -> bool {
        if matches!(e.key_code, KeyCode::Escape | KeyCode::ReturnKey | KeyCode::NumpadEnter) {
            self.about = None;
            self.mac_finish(cx, None);
        }
        true
    }

    // ---- Force Quit Applications

    /// A Mac's Force Quit window: what it is for, the apps, how it opens, and
    /// Force Quit, which asks on a sheet with the app's icon before it acts.
    pub(super) fn draw_force_quit(&mut self, cx: &mut Cx2d, screen: Rect) {
        use crate::approvals::mac_kit::{balanced, button_width, draw_card, draw_group, mac_ink, push_button, wrapped, Face, CONTROL_H, NOTE, NOTE_LINE, TEXT_LINE};
        use super::super::ui::{HAlign, PanelLights};
        let Some(mut panel) = self.force_quit.take() else { return };
        self.screen = screen;
        let k = mac_ink(self.dark);
        let width: f64 = 400.0;
        let inner = width - 40.0;
        let prompt = wrapped(&mut self.d, cx, false, TEXT, "If an app doesn\u{2019}t respond for a while, select its name and click Force Quit.", inner);
        let button_w = button_width(&mut self.d, cx, "Force Quit", 96.0);
        let hint = wrapped(&mut self.d, cx, false, NOTE, "You can open this window by pressing Command-Option-Escape.", inner - button_w - 16.0);
        // Room for every app on the screen, and never less than a Mac's list shows.
        let room = ((screen.size.y - 260.0) / FORCE_QUIT_ROW).floor().max(1.0) as usize;
        let shown = panel.apps.len().clamp(6, room.max(6));
        let list_h = shown as f64 * FORCE_QUIT_ROW + 2.0;
        let foot = (hint.len() as f64 * NOTE_LINE).max(CONTROL_H);
        let height = 28.0 + 10.0 + prompt.len() as f64 * TEXT_LINE + 10.0 + list_h + 14.0 + foot + 18.0;
        let card = rect(
            (screen.pos.x + (screen.size.x - width) * 0.5).floor(),
            (screen.pos.y + (screen.size.y - height) * 0.35).floor(),
            width,
            height,
        );
        panel.card = card;
        let surface = self.skin().surface;
        self.d.card(cx, card, &surface);
        panel.close = self.d.mac_panel_lights(
            cx,
            card.pos.x + 9.0,
            card.pos.y + 9.0,
            PanelLights { dark: self.dark, front: true, over: panel.over_close, down: false },
        );
        self.d.label(cx, rect(card.pos.x, card.pos.y + 6.0, width, 16.0), true, TEXT, k.ink, HAlign::Center, "Force Quit Applications");
        let mut y = card.pos.y + 28.0 + 10.0;
        for line in &prompt {
            self.d.label(cx, rect(card.pos.x + 20.0, y, inner, TEXT_LINE), false, TEXT, k.ink, HAlign::Left, line);
            y += TEXT_LINE;
        }
        y += 10.0;
        let list = rect(card.pos.x + 20.0, y, inner, list_h);
        draw_group(&mut self.d, cx, list, &k);
        panel.rows.clear();
        for (i, (app, name)) in panel.apps.iter().enumerate().take(shown) {
            let row = rect(list.pos.x + 1.0, list.pos.y + 1.0 + i as f64 * FORCE_QUIT_ROW, list.size.x - 2.0, FORCE_QUIT_ROW);
            let chosen = panel.chosen == Some(i);
            if chosen {
                self.d.rounded(cx, rect(row.pos.x + 4.0, row.pos.y + 1.0, row.size.x - 8.0, row.size.y - 2.0), 4.0, k.accent);
            }
            let icon = rect(row.pos.x + 10.0, row.pos.y + (FORCE_QUIT_ROW - 18.0) * 0.5, 18.0, 18.0);
            self.app_icons.draw(cx, app, self.desktop_style, icon, 1.0, k.ink);
            let ink = if chosen { rgb(255, 255, 255) } else { k.ink };
            self.d.label(cx, rect(icon.pos.x + 26.0, row.pos.y + 4.0, row.size.x - 46.0, 16.0), false, TEXT, ink, HAlign::Left, name);
            panel.rows.push(row);
        }
        y += list_h + 14.0;
        let face = if panel.chosen.is_none() { Face::Disabled } else if panel.asking { Face::Plain } else { Face::Default };
        panel.button = rect(list.pos.x + list.size.x - button_w, y, button_w, CONTROL_H);
        push_button(&mut self.d, cx, panel.button, &k, face, panel.over_button && face == Face::Default, false, "Force Quit");
        let mut note_y = y + ((CONTROL_H - hint.len() as f64 * NOTE_LINE) * 0.5).max(0.0);
        for line in &hint {
            self.d.label(cx, rect(list.pos.x, note_y, inner - button_w - 16.0, NOTE_LINE), false, NOTE, k.muted, HAlign::Left, line);
            note_y += NOTE_LINE;
        }
        panel.cancel = Rect::default();
        panel.confirm = Rect::default();
        if let Some((app, name)) = panel.chosen.and_then(|i| panel.apps.get(i)).cloned().filter(|_| panel.asking) {
            // The window dims behind its sheet, as the shell's sheets do.
            self.d.rounded(cx, card, 10.0, k.dim);
            let sheet_w = 300.0;
            let title = balanced(&mut self.d, cx, true, TEXT, &format!("Do you want to force \u{201c}{name}\u{201d} to quit?"), sheet_w - 40.0);
            let note = wrapped(&mut self.d, cx, false, NOTE, "You will lose any unsaved changes.", sheet_w - 40.0);
            let sheet_h = 20.0 + 56.0 + 12.0 + title.len() as f64 * TEXT_LINE + 4.0 + note.len() as f64 * NOTE_LINE + 18.0 + CONTROL_H + 18.0;
            let sheet = rect((card.pos.x + (width - sheet_w) * 0.5).floor(), card.pos.y + 28.0, sheet_w, sheet_h);
            draw_card(&mut self.d, cx, sheet, 10.0, &k);
            let icon = rect(sheet.pos.x + (sheet_w - 56.0) * 0.5, sheet.pos.y + 20.0, 56.0, 56.0);
            self.app_icons.draw(cx, &app, self.desktop_style, icon, 1.0, k.ink);
            let mut y = icon.pos.y + 56.0 + 12.0;
            for line in &title {
                self.d.label(cx, rect(sheet.pos.x + 20.0, y, sheet_w - 40.0, TEXT_LINE), true, TEXT, k.ink, HAlign::Center, line);
                y += TEXT_LINE;
            }
            y += 4.0;
            for line in &note {
                self.d.label(cx, rect(sheet.pos.x + 20.0, y, sheet_w - 40.0, NOTE_LINE), false, NOTE, k.ink, HAlign::Center, line);
                y += NOTE_LINE;
            }
            y += 18.0;
            // Cancel on the left, the action on the right and blue, as a Mac's alert.
            let half = ((sheet_w - 40.0 - 8.0) * 0.5).floor();
            panel.cancel = rect(sheet.pos.x + 20.0, y, half, CONTROL_H);
            panel.confirm = rect(sheet.pos.x + sheet_w - 20.0 - half, y, half, CONTROL_H);
            push_button(&mut self.d, cx, panel.cancel, &k, Face::Plain, panel.over_cancel, false, "Cancel");
            push_button(&mut self.d, cx, panel.confirm, &k, Face::Default, panel.over_confirm, false, "Force Quit");
        }
        self.force_quit = Some(panel);
    }

    /// Ends the panel, with the chosen app's Force Quit or with nothing.
    fn force_quit_done(&mut self, cx: &mut Cx, act: bool) {
        let target = self.force_quit.take().filter(|_| act).and_then(|panel| panel.target());
        self.mac_finish(cx, target);
    }

    pub(super) fn force_quit_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let Some(panel) = self.force_quit.as_mut() else { return false };
        match event {
            Event::MouseDown(e) => {
                let p = e.abs;
                if panel.asking {
                    // A sheet keeps its window's presses to itself.
                    if contains(panel.confirm, p) {
                        self.force_quit_done(cx, true);
                    } else if contains(panel.cancel, p) {
                        panel.asking = false;
                        self.redraw(cx);
                    }
                    return true;
                }
                let close = panel.close;
                let hit = rect(close.pos.x - 3.0, close.pos.y - 3.0, close.size.x + 6.0, close.size.y + 6.0);
                if contains(hit, p) || !contains(panel.card, p) {
                    self.force_quit_done(cx, false);
                } else if let Some(i) = row_at(&panel.rows, p) {
                    panel.chosen = Some(i);
                    self.redraw(cx);
                } else if panel.chosen.is_some() && contains(panel.button, p) {
                    panel.asking = true;
                    self.redraw(cx);
                }
                true
            }
            Event::MouseMove(e) => {
                let p = e.abs;
                let over = (contains(panel.close, p), contains(panel.button, p), contains(panel.cancel, p), contains(panel.confirm, p));
                if over != (panel.over_close, panel.over_button, panel.over_cancel, panel.over_confirm) {
                    (panel.over_close, panel.over_button, panel.over_cancel, panel.over_confirm) = over;
                    self.redraw(cx);
                }
                true
            }
            Event::MouseUp(_) | Event::Scroll(_) => true,
            _ => false,
        }
    }

    pub(super) fn force_quit_key(&mut self, cx: &mut Cx, e: &KeyEvent) -> bool {
        let Some(panel) = self.force_quit.as_mut() else { return false };
        match e.key_code {
            KeyCode::Escape if panel.asking => {
                panel.asking = false;
                self.redraw(cx);
            }
            KeyCode::Escape => self.force_quit_done(cx, false),
            KeyCode::KeyW if e.modifiers.logo && !panel.asking => self.force_quit_done(cx, false),
            KeyCode::ReturnKey | KeyCode::NumpadEnter if panel.asking => self.force_quit_done(cx, true),
            KeyCode::ReturnKey | KeyCode::NumpadEnter if panel.chosen.is_some() => {
                panel.asking = true;
                self.redraw(cx);
            }
            KeyCode::ArrowUp | KeyCode::ArrowDown if !panel.asking => {
                panel.chosen = panel.step(if e.key_code == KeyCode::ArrowUp { -1 } else { 1 });
                self.redraw(cx);
            }
            _ => {}
        }
        true
    }

    // ---- Spotlight

    fn spotlight_keys(&self) -> bool {
        self.model.path == "learn.keybindings"
    }

    /// A search result can show its own icon and kind beside the list when the
    /// desktop has room. Nested lists and narrow windows keep the whole width.
    fn spotlight_has_preview(&self, screen: Rect, width: f64) -> bool {
        width >= SPOTLIGHT_PREVIEW_MIN_WIDTH && screen.size.y >= 420.0
            && self.model.path.is_empty() && !self.model.filter.is_empty() && !self.model.rows.is_empty()
    }

    /// Nothing typed and no list open: Spotlight shows no rows to choose.
    pub(super) fn spotlight_blank(&self) -> bool {
        self.spotlight() && self.model.path.is_empty() && self.model.filter.is_empty()
    }

    /// Spotlight's arrows: the first one lights the first row, and a note about
    /// a setting is no stop.
    pub(super) fn spotlight_arrow(&mut self, by: i64) {
        if self.spotlight_blank() {
            return;
        }
        let keys = self.spotlight_keys();
        let len = self.model.rows.len() as i64;
        let (mut next, step) = if self.mac_idle { (0, 1) } else { (self.model.sel as i64 + by, by) };
        while next >= 0 && next < len {
            let row = &self.model.rows[next as usize];
            if !row.disabled && (keys || row.kind != MenuKind::Inert) {
                self.model.sel = next as usize;
                return;
            }
            next += step;
        }
    }

    /// The lines of each row's note that does not fit beside the row's name: it
    /// goes under the name, as System Settings writes one. Search results say
    /// only what they are, and Keyboard Shortcuts' rows are their keys.
    fn spotlight_note_lines(&mut self, cx: &mut Cx2d, screen: Rect) -> Vec<Vec<String>> {
        if self.model.path.is_empty() || self.spotlight_keys() {
            return Vec::new();
        }
        // The width between a row's tile and its right edge.
        let room = (SPOTLIGHT_WIDTH.min(screen.size.x - 24.0) - 66.0).max(1.0);
        let rows = self.model.rows.clone();
        rows.iter()
            .map(|row| {
                if row.detail.is_empty() {
                    return Vec::new();
                }
                let name = self.d.measure(cx, false, 13.0, row.label.trim_end_matches(" \u{2713}"));
                let note = self.d.measure(cx, false, 12.0, &row.detail);
                if name + 16.0 + note <= room {
                    Vec::new()
                } else {
                    self.d.wrap(cx, false, 12.0, &row.detail, room, 6)
                }
            })
            .collect()
    }

    /// A row of Spotlight's list: `plain`, or what its note needs.
    fn spotlight_row_h(&self, i: usize, plain: f64) -> f64 {
        match self.spotlight_notes.get(i).map_or(0, Vec::len) {
            0 => plain,
            lines => SPOTLIGHT_NOTE_TOP + lines as f64 * SPOTLIGHT_NOTE_LINE + SPOTLIGHT_NOTE_BOTTOM,
        }
    }

    /// A result's group name over it, in a search of everything.
    fn spotlight_group_title(&self, i: usize) -> Option<&'static str> {
        (self.model.path.is_empty() && !self.model.filter.is_empty()).then(|| spotlight_caption(&self.model.rows, i)).flatten()
    }

    /// What a result takes in the list: its group's name, and itself.
    fn spotlight_row_room(&self, i: usize, plain: f64) -> f64 {
        self.spotlight_row_h(i, plain) + if self.spotlight_group_title(i).is_some() { SPOTLIGHT_CAPTION } else { 0.0 }
    }

    /// Spotlight's card under its field: nothing until there is something to
    /// show, a page of rows after that.
    pub(super) fn spotlight_layout(&self, screen: Rect) -> (Rect, usize, f64) {
        let width = SPOTLIGHT_WIDTH.min(screen.size.x - 24.0).max(1.0);
        let row = if self.spotlight_keys() { SPOTLIGHT_ROW_KEYS } else { SPOTLIGHT_ROW };
        let top = (screen.pos.y + screen.size.y * 0.16).floor();
        let room = (screen.size.y - (top - screen.pos.y) - SPOTLIGHT_FIELD - 96.0).max(row);
        let most = ((room / row).floor() as usize).clamp(1, if self.spotlight_keys() { 18 } else { 9 });
        let browsing = !self.model.path.is_empty() || !self.model.filter.is_empty();
        let (visible, height) = if !browsing {
            (0, 0.0)
        } else if self.spotlight_notes.iter().any(|notes| !notes.is_empty()) || self.spotlight_group_title(0).is_some() {
            // As many as fit from the first one showing: a note or a group's name makes its row taller.
            let first = self.model.scroll.min(self.model.rows.len().saturating_sub(1));
            let (mut count, mut used) = (0, 0.0);
            for i in first..self.model.rows.len() {
                let h = self.spotlight_row_room(i, row);
                if count > 0 && used + h > room {
                    break;
                }
                count += 1;
                used += h;
            }
            (count, used)
        } else {
            let count = self.model.rows.len().min(most);
            (count, count as f64 * row)
        };
        let list = if visible > 0 {
            let height = height + 12.0;
            if self.spotlight_has_preview(screen, width) { height.max(SPOTLIGHT_PREVIEW_MIN_HEIGHT.min(room + 12.0)) } else { height }
        } else if browsing { 112.0 } else { 0.0 };
        let x = (screen.pos.x + (screen.size.x - width) * 0.5).floor();
        (rect(x, top, width, SPOTLIGHT_FIELD + list), visible, row)
    }

    pub(super) fn draw_spotlight(&mut self, cx: &mut Cx2d, screen: Rect) {
        self.screen = screen;
        let (ink, accent) = self.mac_ink();
        self.spotlight_notes = self.spotlight_note_lines(cx, screen);
        let (card, visible, row_h) = self.spotlight_layout(screen);
        self.card = card;
        let surface = self.skin().surface;
        self.d.card(cx, card, &surface);
        // The widget's place in a test's snapshot (`text`); nothing to see.
        self.draw_bg.color = Vec4f::default();
        self.draw_bg.draw_abs(cx, card);
        let (header, placeholder) = if self.model.filter.is_empty() {
            let title = match self.model.path.as_str() {
                "" => "Spotlight Search".to_string(),
                "learn.keybindings" => "Keyboard Shortcuts".to_string(),
                _ => title_case(self.model.header().0.trim_end_matches('\u{2026}')),
            };
            (title, true)
        } else {
            (self.model.filter.clone(), false)
        };
        let field = rect(card.pos.x, card.pos.y, card.size.x, SPOTLIGHT_FIELD);
        self.d.icon_centered(cx, Ico::Search, rect(field.pos.x + 12.0, field.pos.y, 28.0, field.size.y), 20.0, fade(ink, 0.66));
        let text = rect(field.pos.x + 48.0, field.pos.y, field.size.x - 64.0, field.size.y);
        self.d.label_elided(cx, text, false, 20.0, if placeholder { fade(ink, 0.66) } else { ink }, super::super::ui::HAlign::Left, &header);
        if !placeholder {
            let at = text.pos.x + self.d.measure(cx, false, 20.0, &header) + 1.0;
            self.d.solid(cx, rect(at.min(text.pos.x + text.size.x), field.pos.y + 14.0, 2.0, 24.0), accent);
        }
        self.row_rects.clear();
        if card.size.y <= SPOTLIGHT_FIELD {
            return;
        }
        self.d.solid(cx, rect(card.pos.x, card.pos.y + SPOTLIGHT_FIELD, card.size.x, 1.0), fade(ink, 0.12));
        if self.model.rows.is_empty() {
            // Centred in its pane, saying what to do.
            let y = card.pos.y + SPOTLIGHT_FIELD;
            self.d.icon_centered(cx, Ico::Search, rect(card.pos.x, y + 18.0, card.size.x, 30.0), 26.0, fade(ink, 0.5));
            let title = format!("No Results for \u{201c}{}\u{201d}", self.model.filter);
            self.d.label_elided(cx, rect(card.pos.x + 24.0, y + 54.0, card.size.x - 48.0, 20.0), true, 13.0, ink, super::super::ui::HAlign::Center, &title);
            self.d.label(cx, rect(card.pos.x, y + 74.0, card.size.x, 18.0), false, 12.0, fade(ink, 0.66), super::super::ui::HAlign::Center, "Check the spelling, or try a different name.");
            return;
        }
        let preview = self.spotlight_has_preview(screen, card.size.x);
        let list_width = if preview { (card.size.x * 0.52).floor() } else { card.size.x };
        let first = self.model.scroll.min(self.model.rows.len().saturating_sub(1));
        let mut y = card.pos.y + SPOTLIGHT_FIELD + 6.0;
        let list = rect(card.pos.x, y, list_width, (card.size.y - SPOTLIGHT_FIELD - 12.0).max(0.0));
        cx.push_clip_rect(list);
        let keys = self.spotlight_keys();
        for (i, row) in self.model.rows.clone().iter().enumerate().skip(first).take(visible) {
            let notes = self.spotlight_notes.get(i).cloned().unwrap_or_default();
            let row_h = self.spotlight_row_h(i, row_h);
            if let Some(title) = self.spotlight_group_title(i) {
                self.d.label(cx, rect(card.pos.x + 16.0, y + 4.0, list_width - 32.0, SPOTLIGHT_CAPTION - 4.0), true, 11.0, fade(ink, 0.66), super::super::ui::HAlign::Left, title);
                y += SPOTLIGHT_CAPTION;
            }
            // The name's line: all there is of a plain row.
            let line_h = if notes.is_empty() { row_h } else { SPOTLIGHT_NAME_LINE };
            let band = rect(card.pos.x + 6.0, y, list_width - 12.0, row_h);
            self.row_rects.push(band);
            // A note about a setting is there to read, not to choose.
            let note_row = row.kind == MenuKind::Inert && !keys;
            let on = !self.mac_idle && i == self.model.sel && !row.disabled && !note_row;
            if on {
                self.d.rounded(cx, band, 3.0, accent);
            }
            let color = if on { rgb(255, 255, 255) } else if row.disabled { fade(ink, 0.36) } else { ink };
            let mut left = band.pos.x + 10.0;
            if !keys {
                if let Some(name) = row.target.strip_prefix("apps.") {
                    self.app_icons.draw(cx, name, self.desktop_style, rect(left, y + (line_h - 24.0) * 0.5, 24.0, 24.0), 1.0, color);
                } else if !note_row {
                    // A setting's tile, as a Mac's Spotlight shows a settings pane.
                    let tile = rect(left + 1.0, y + (line_h - 22.0) * 0.5, 22.0, 22.0);
                    let glyph = row.icon.filter(|ico| *ico != Ico::Check).unwrap_or_else(|| spotlight_glyph(&row.target));
                    self.d.rounded(cx, tile, 2.5, spotlight_tint(&row.target, self.dark));
                    self.d.icon_centered(cx, glyph, tile, 14.0, rgb(255, 255, 255));
                }
                left += 34.0;
            }
            let detail = if row.detail.is_empty() || !notes.is_empty() || (i > 0 && self.spotlight_group_title(0).is_some()) { 0.0 } else { self.d.measure(cx, false, if keys { 13.0 } else { 12.0 }, &row.detail) };
            let right = band.pos.x + band.size.x - 10.0;
            let label = row.label.trim_end_matches(" \u{2713}");
            self.d.label_elided(cx, rect(left, y, (right - left - detail - 16.0).max(1.0), line_h), false, 13.0, color, super::super::ui::HAlign::Left, label);
            // A setting that is on says so beside what it is. The check glyph some
            // rows carry says nothing about that.
            if !keys && row.label.ends_with(" \u{2713}") {
                self.d.icon_centered(cx, Ico::Check, rect(right - detail - 26.0, y, 20.0, line_h), 12.0, color);
            }
            if detail > 0.0 {
                let shade = if on { color } else { fade(ink, 0.66) };
                let px = if keys { 13.0 } else { 12.0 };
                self.d.label(cx, rect(right - detail, y, detail, row_h), false, px, shade, super::super::ui::HAlign::Right, &row.detail);
            }
            for (line, text) in notes.iter().enumerate() {
                let shade = if on { color } else { fade(ink, 0.66) };
                let at = y + SPOTLIGHT_NOTE_TOP + line as f64 * SPOTLIGHT_NOTE_LINE;
                self.d.label(cx, rect(left, at, right - left, SPOTLIGHT_NOTE_LINE), false, 12.0, shade, super::super::ui::HAlign::Left, text);
            }
            y += row_h;
        }
        cx.pop_clip_rect();
        if preview { self.draw_spotlight_preview(cx, card, list_width, ink); }
    }

    /// The selected result's own available information, without inventing
    /// filesystem metadata for apps, settings or shell commands.
    fn draw_spotlight_preview(&mut self, cx: &mut Cx2d, card: Rect, list_width: f64, ink: Vec4f) {
        let Some(row) = self.model.rows.get(self.model.sel).cloned() else { return };
        let pane = rect(card.pos.x + list_width, card.pos.y + SPOTLIGHT_FIELD,
                        card.size.x - list_width, card.size.y - SPOTLIGHT_FIELD);
        self.d.solid(cx, rect(pane.pos.x, pane.pos.y, 1.0, pane.size.y), fade(ink, 0.12));
        cx.push_clip_rect(pane);
        let icon = rect(pane.pos.x + (pane.size.x - 64.0) * 0.5, pane.pos.y + 20.0, 64.0, 64.0);
        if let Some(name) = row.target.strip_prefix("apps.") {
            self.app_icons.draw(cx, name, self.desktop_style, icon, 1.0, ink);
        } else {
            let glyph = row.icon.filter(|ico| *ico != Ico::Check).unwrap_or_else(|| spotlight_glyph(&row.target));
            self.d.rounded(cx, icon, 10.0, spotlight_tint(&row.target, self.dark));
            self.d.icon_centered(cx, glyph, icon, 34.0, rgb(255, 255, 255));
        }
        let title = row.label.trim_end_matches(" \u{2713}");
        self.d.label_elided(cx, rect(pane.pos.x + 16.0, pane.pos.y + 94.0, pane.size.x - 32.0, 22.0),
                            true, 14.0, ink, super::super::ui::HAlign::Center, title);
        self.d.label_elided(cx, rect(pane.pos.x + 16.0, pane.pos.y + 119.0, pane.size.x - 32.0, 18.0),
                            false, 12.0, fade(ink, 0.66), super::super::ui::HAlign::Center, &row.detail);
        // An app's facts under a rule, as a Mac's: the labels right-aligned against the values,
        // in the subtitle's grey, which reads at 4.5:1 or better on the light pane too.
        if let Some(name) = row.target.strip_prefix("apps.") {
            if self.spotlight_facts.as_ref().map(|(target, _)| target.as_str()) != Some(row.target.as_str()) {
                let facts = spotlight_app_facts(name);
                log!("spotlight preview: {} {}", row.target, facts.iter().map(|(label, value)| format!("{label}={value}")).collect::<Vec<_>>().join("; "));
                self.spotlight_facts = Some((row.target.clone(), facts));
            }
            let facts = self.spotlight_facts.as_ref().map(|(_, facts)| facts.clone()).unwrap_or_default();
            let mut y = pane.pos.y + 150.0;
            self.d.solid(cx, rect(pane.pos.x + 20.0, y - 7.0, pane.size.x - 40.0, 1.0), fade(ink, 0.1));
            let mid = pane.pos.x + (pane.size.x * 0.4).round();
            for (label, value) in &facts {
                self.d.label_elided(cx, rect(pane.pos.x + 12.0, y, mid - pane.pos.x - 18.0, 16.0),
                                    false, 11.0, fade(ink, 0.66), super::super::ui::HAlign::Right, label);
                self.d.label_elided(cx, rect(mid, y, pane.pos.x + pane.size.x - mid - 14.0, 16.0),
                                    false, 11.0, fade(ink, 0.85), super::super::ui::HAlign::Left, value);
                y += 18.0;
            }
        }
        cx.pop_clip_rect();
    }

    // ---- Launchpad

    /// Columns, rows, the cell's size and the grid's top-left corner.
    pub(super) fn launchpad_grid(&self, screen: Rect) -> (usize, usize, f64, f64, f64, f64) {
        let side = (screen.size.x * 0.1).floor().max(24.0);
        let top = PAD_FIELD_TOP + PAD_FIELD_H + 20.0;
        let bottom = crate::desktop::MacDockLayout::ROOM + PAD_DOTS;
        let columns = (((screen.size.x - side * 2.0) / PAD_CELL_W).floor() as usize).clamp(3, 7);
        let room = (screen.size.y - top - bottom).max(PAD_CELL_H);
        let rows = ((room / PAD_CELL_H).floor() as usize).clamp(1, 5);
        let cell_w = ((screen.size.x - side * 2.0) / columns as f64).floor();
        let cell_h = (room / rows as f64).floor().min(PAD_CELL_H_MOST);
        let y0 = screen.pos.y + top + ((room - cell_h * rows as f64) * 0.5).floor();
        (columns, rows, cell_w, cell_h, screen.pos.x + side + ((screen.size.x - side * 2.0 - cell_w * columns as f64) * 0.5).floor(), y0)
    }

    /// The page dots, when the apps need more than one page.
    fn launchpad_dots(&self, screen: Rect, pages: usize) -> Vec<Rect> {
        if pages < 2 {
            return Vec::new();
        }
        const PITCH: f64 = 16.0;
        const DOT: f64 = 7.0;
        let y = screen.pos.y + screen.size.y - crate::desktop::MacDockLayout::ROOM - PAD_DOTS * 0.5 - DOT * 0.5;
        let x = screen.pos.x + (screen.size.x - pages as f64 * PITCH) * 0.5;
        (0..pages).map(|p| rect(x + p as f64 * PITCH + (PITCH - DOT) * 0.5, y, DOT, DOT)).collect()
    }

    pub(super) fn draw_launchpad(&mut self, cx: &mut Cx2d, screen: Rect) {
        self.screen = screen;
        self.card = screen;
        let white = rgb(255, 255, 255);
        // The frost is the Dock's neighbour under it (`DesktopShelf`); this is
        // the widget's place in a test's snapshot (`text`), nothing to see.
        self.draw_bg.color = Vec4f::default();
        self.draw_bg.draw_abs(cx, screen);
        // The search field, centred at the top.
        let field = rect((screen.pos.x + (screen.size.x - PAD_FIELD_W) * 0.5).floor(), screen.pos.y + PAD_FIELD_TOP, PAD_FIELD_W, PAD_FIELD_H);
        self.d.rounded(cx, field, 3.5, fade(white, 0.12));
        self.d.focus_ring(cx, field, 3.5, 1.0, fade(white, 0.14));
        let searching = !self.model.filter.is_empty();
        let (text, shade) = if searching { (self.model.filter.clone(), 1.0) } else { ("Search".to_string(), 0.9) };
        let width = self.d.measure(cx, false, 13.0, &text);
        let left = if searching { field.pos.x + 28.0 } else { field.pos.x + (field.size.x - width + 18.0) * 0.5 };
        self.d.icon_centered(cx, Ico::Search, rect(left - 22.0, field.pos.y, 16.0, field.size.y), 13.0, fade(white, 0.9));
        self.d.label_elided(cx, rect(left, field.pos.y, (field.pos.x + field.size.x - left - 8.0).max(1.0), field.size.y), false, 13.0, fade(white, shade), super::super::ui::HAlign::Left, &text);
        if searching {
            self.d.solid(cx, rect((left + width + 1.0).min(field.pos.x + field.size.x - 8.0), field.pos.y + 6.0, 1.5, field.size.y - 12.0), white);
        }
        self.row_rects.clear();
        let (columns, rows, cell_w, cell_h, x0, y0) = self.launchpad_grid(screen);
        let page = (columns * rows).max(1);
        if self.model.rows.is_empty() {
            let y = y0 + (cell_h * rows as f64 - 64.0) * 0.4;
            self.d.label(cx, rect(screen.pos.x, y, screen.size.x, 32.0), false, 22.0, white, super::super::ui::HAlign::Center, "No Results");
            self.d.label(cx, rect(screen.pos.x, y + 34.0, screen.size.x, 20.0), false, 13.0, fade(white, 0.92), super::super::ui::HAlign::Center, "Check the spelling, or try a different name.");
            return;
        }
        let first = (self.model.scroll / page) * page;
        for (i, row) in self.model.rows.clone().iter().enumerate().skip(first).take(page) {
            let at = i - first;
            let cell = rect(x0 + (at % columns) as f64 * cell_w, y0 + (at / columns) as f64 * cell_h, cell_w, cell_h);
            self.row_rects.push(cell);
            let content = PAD_ICON + 6.0 + PAD_LABEL;
            let top = cell.pos.y + ((cell.size.y - content) * 0.5).floor();
            if !self.mac_idle && i == self.model.sel {
                let tile = rect(cell.pos.x + (cell.size.x - 116.0) * 0.5, top - 6.0, 116.0, content + 12.0);
                self.d.rounded(cx, tile, 6.0, fade(white, 0.18));
            }
            let icon = rect(cell.pos.x + (cell.size.x - PAD_ICON) * 0.5, top, PAD_ICON, PAD_ICON);
            if let Some(name) = row.target.strip_prefix("apps.") {
                self.app_icons.draw(cx, name, self.desktop_style, icon, 1.0, white);
            }
            let label = rect(cell.pos.x + 4.0, top + PAD_ICON + 6.0, cell.size.x - 8.0, PAD_LABEL);
            self.d.label_elided(cx, label, false, 13.0, white, super::super::ui::HAlign::Center, &row.label);
        }
        // Page dots, when the apps need more than one page.
        let current = first / page;
        for (p, dot) in self.launchpad_dots(screen, self.model.rows.len().div_ceil(page)).into_iter().enumerate() {
            self.d.rounded(cx, dot, 1.75, fade(white, if p == current { 0.95 } else { 0.35 }));
        }
    }

    pub(super) fn launchpad_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        // The Dock stays live over Launchpad: over it, the pointer is the Dock's.
        let dock = crate::desktop::dock_drawn_bounds();
        let at = match event {
            Event::MouseDown(e) => Some(e.abs),
            Event::MouseMove(e) => Some(e.abs),
            Event::MouseUp(e) => Some(e.abs),
            _ => None,
        };
        if at.is_some_and(|p| dock.size.x > 0.0 && dock.contains(p)) {
            return false;
        }
        match event {
            Event::MouseDown(e) => {
                let (columns, rows, ..) = self.launchpad_grid(self.screen);
                let page = (columns * rows).max(1);
                let first = (self.model.scroll / page) * page;
                // A press on a page dot turns to that page.
                let pages = self.model.rows.len().div_ceil(page);
                let dots = self.launchpad_dots(self.screen, pages);
                if let Some(p) = dots.iter().position(|r| contains(rect(r.pos.x - 4.0, r.pos.y - 8.0, r.size.x + 8.0, r.size.y + 16.0), e.abs)) {
                    self.model.scroll = p * page;
                    self.redraw(cx);
                    return true;
                }
                match self.row_rects.iter().position(|r| contains(*r, e.abs)).map(|i| i + first) {
                    Some(i) if i < self.model.rows.len() => {
                        self.model.sel = i;
                        self.mac_idle = false;
                        self.activate_selected(cx);
                    }
                    _ => {
                        self.model.close();
                        cx.widget_action(self.uid, ShellMenuAction::Cancel);
                    }
                }
                self.redraw(cx);
                true
            }
            Event::Scroll(e) => {
                if e.scroll.y.abs() > 0.5 {
                    let (columns, rows, ..) = self.launchpad_grid(self.screen);
                    let page = (columns * rows).max(1);
                    let last = self.model.rows.len().saturating_sub(1) / page * page;
                    let at = self.model.scroll / page * page;
                    self.model.scroll = if e.scroll.y > 0.0 { (at + page).min(last) } else { at.saturating_sub(page) };
                    self.redraw(cx);
                }
                true
            }
            Event::MouseMove(_) | Event::MouseUp(_) => true,
            _ => false,
        }
    }

    /// Launchpad's arrows move through its grid. Everything else is the menu's.
    pub(super) fn launchpad_key(&mut self, cx: &mut Cx, e: &KeyEvent) -> bool {
        let (columns, rows, ..) = self.launchpad_grid(self.screen);
        let by = match e.key_code {
            KeyCode::ArrowLeft => -1,
            KeyCode::ArrowRight => 1,
            KeyCode::ArrowUp => -(columns as i64),
            KeyCode::ArrowDown => columns as i64,
            _ => return false,
        };
        if self.mac_idle {
            self.mac_idle = false;
        } else {
            self.model.move_sel(by);
        }
        let page = (columns * rows).max(1);
        self.model.scroll = self.model.sel / page * page;
        self.redraw(cx);
        true
    }
}

#[cfg(test)]
mod launchpad_tests {
    use super::*;

    fn row(target: &str, label: &str) -> MenuRow {
        MenuRow { label: label.into(), detail: "a path".into(), icon: None, target: target.into(), kind: MenuKind::App, disabled: false, has_children: false, divider: true }
    }

    /// A Mac's names are in Title Case; the small words inside them stay small
    /// and a word with its own capitals keeps them.
    #[test]
    fn a_menu_too_tall_for_its_room_keeps_every_row_in_reach() {
        let row = |i: i32| MacRow::item(&format!("Row {i}"), "t");
        let mut rows: Vec<MacRow> = (0..6).map(row).collect();
        rows.insert(3, MacRow::sep());
        let short = rows.clone();
        let mut same = rows.clone();
        assert!(!fold_to_room(&mut same, 1000.0));
        assert_eq!(same.len(), short.len());
        // Room for three rows and More: the rule before More goes, and none after it.
        assert!(fold_to_room(&mut rows, PAD_Y * 2.0 + ROW * 4.0 + SEPARATOR));
        let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["Row 0", "Row 1", "Row 2", "More"]);
        let rest: Vec<&str> = rows[3].sub.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(rest, ["Row 3", "Row 4", "Row 5"]);
        // Folded, it fits, and folding again changes nothing.
        assert!(!fold_to_room(&mut rows, PAD_Y * 2.0 + ROW * 4.0 + SEPARATOR));
    }

    #[test]
    fn a_rows_name_takes_title_case_and_keeps_its_own_capitals() {
        assert_eq!(title_case("ai providers"), "Ai Providers");
        assert_eq!(title_case("AI Providers"), "AI Providers");
        assert_eq!(title_case("app hub"), "App Hub");
        assert_eq!(title_case("at a glance"), "At a Glance");
        assert_eq!(title_case("Sign in"), "Sign In");
        assert_eq!(title_case("YouTube"), "YouTube");
        assert_eq!(app_label("apps.updater", "OctoSense Updates"), "Software Update");
        assert_eq!(app_label("apps.notes", "Notes"), "Notes");
    }

    /// A search finds the start of a word, as a Mac's does, or the name run
    /// together; not the middle of a word.
    #[test]
    fn a_search_finds_the_start_of_a_word_of_the_name() {
        assert!(name_matches("Task Manager", "ma"));
        assert!(name_matches("Maps", "ma"));
        assert!(name_matches("macOS \u{00b7} Dark", "macos dark"));
        assert!(name_matches("macOS \u{00b7} Dark", "\u{00b7}"));
        assert!(name_matches("App Hub", "apphub"));
        assert!(name_matches("App Hub", "hub"));
        assert!(name_matches("Anything", ""), "no search: everything");
        assert!(!name_matches("Image Viewer", "ma"));
        assert!(!name_matches("Omarchy", "ma"));
        assert!(!name_matches("Files", "makepad"));
    }

    /// Launchpad lists what a person opens: not the commands, nor the system's
    /// updater, nor the developers' sample. The Dock's apps come first, in the
    /// Dock's order, and the rest by name; a search keeps its order of match.
    #[test]
    fn launchpad_holds_apps_only_the_docks_first_then_by_name() {
        let listed = || vec![
            row("apps.zeta", "Zeta"), row("apps.reference", "Reference"), row("apps.mail", "Mail"), row("system.lock", "Lock Screen"),
            row("apps.updater", "OctoSense Updates"), row("apps.alpha", "Alpha"), row("apps.browser", "Browser"), row("apps", "Apps"),
        ];
        let mut rows = listed();
        launchpad_rows(&mut rows, "");
        let names: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(names, ["Browser", "Mail", "Alpha", "Zeta"]);
        let mut rows = vec![row("apps.maps", "Maps"), row("apps.task", "Task Manager"), row("apps.mail", "Mail"), row("apps.image", "Image Viewer"), row("system.mute", "Mute")];
        launchpad_rows(&mut rows, "ma");
        let names: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(names, ["Maps", "Task Manager", "Mail"], "a search keeps how well they matched, and finds only apps");
    }

    /// A Spotlight result says what it is, and the group of all apps is none.
    #[test]
    fn a_spotlight_row_says_what_it_is_and_the_apps_group_is_no_result() {
        let mut rows = vec![
            row("apps", "Apps"), row("apps.mail", "Mail"), row("apps.updater", "OctoSense Updates"), row("apps.reference", "Reference"),
            row("desktop.macos", "macOS"), row("desktop.macos-dark", "macOS Dark"), row("style.dark", "Dark"), row("setup.wifi", "Wi-Fi"), row("learn.keys", "Keyboard Shortcuts"),
            row("settings.keyboard", "Keyboard"), row("other.thing", "Thing"),
        ];
        spotlight_rows(&mut rows, "");
        let says: Vec<(&str, &str)> = rows.iter().map(|r| (r.label.as_str(), r.detail.as_str())).collect();
        assert_eq!(says, [
            ("Mail", "Application"), ("Software Update", "Application"), ("Reference", "Application"),
            ("Keyboard", "System Settings"), ("Thing", "Command"), ("macOS", "Desktop Style"),
        ], "what System Settings holds is found under its panes");
        assert!(rows.iter().all(|r| !r.divider), "one list");
    }

    /// An app answers Spotlight to its name; a setting found by another of its
    /// names stays once enough has been typed to mean it.
    #[test]
    fn spotlight_finds_apps_by_name_and_settings_by_their_other_names() {
        let found = || vec![row("apps.files", "Files"), row("apps.mail", "Mail"), row("settings.assistant", "Assistant")];
        let mut rows = found();
        spotlight_rows(&mut rows, "ma");
        assert_eq!(rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["Mail"]);
        let mut rows = found();
        spotlight_rows(&mut rows, "agents");
        assert_eq!(rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["Assistant"], "the app's package name finds nothing");
    }

    /// A list opened from a result has the Mac's case and words; Keyboard
    /// Shortcuts' rows are sentences.
    #[test]
    fn a_macs_spotlight_list_has_the_macs_words() {
        let mut rows = vec![row(ASK_APP_ROW, "Ask this app's agent"), row("setup.assistant.commands", "Command execution")];
        spotlight_list(&mut rows, "setup.assistant");
        assert_eq!(rows[0].label, "Ask This App's Assistant");
        assert!(!rows[0].detail.contains("agent"), "{}", rows[0].detail);
        assert_eq!(rows[1].label, "Command Execution");
        let mut keys = vec![row("learn.keybindings.0", "Open the launcher")];
        spotlight_list(&mut keys, "learn.keybindings");
        assert_eq!(keys[0].label, "Open the launcher");
    }

    /// A note about a setting is no search result: nothing happens on Return.
    /// Nor is a shortcut's row: System Settings' Keyboard pane lists them.
    #[test]
    fn a_macs_spotlight_leaves_out_notes_about_a_setting() {
        let mut note = row("setup.assistant.commands.applies", "Applies when the assistant starts");
        note.kind = MenuKind::Inert;
        let mut shortcut = row("learn.keybindings.0", "Open the launcher");
        shortcut.kind = MenuKind::Inert;
        let mut rows = vec![note, shortcut];
        spotlight_rows(&mut rows, "the");
        assert!(rows.is_empty(), "{:?}", rows.iter().map(|r| r.target.as_str()).collect::<Vec<_>>());
    }

    /// A row says what it does in the words the Mac's windows use.
    #[test]
    fn a_macs_spotlight_says_assistant_and_what_opens_a_conversation() {
        let mut rows = vec![row(ASK_APP_ROW, "Ask this app's agent"), row(SYSTEM_CHAT_ROW, "Assistant chat"), row(APPROVALS_ROW, "Approvals")];
        spotlight_rows(&mut rows, "a");
        assert_eq!(rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["Approvals", "Ask This App's Assistant", "Assistant Chat"]);
        assert_eq!(rows.iter().map(|r| r.detail.as_str()).collect::<Vec<_>>(), ["System Settings", "Command", "Command"]);
    }

    /// Results stand in groups, under the best name.
    #[test]
    fn a_macs_spotlight_groups_its_results_under_a_top_hit() {
        let mut rows = vec![row("desktop.macos", "macOS"), row("apps.task", "Task Manager"), row("apps.mail", "Mail"), row("apps.maps", "Maps"), row("settings.keyboard", "Keyboard")];
        spotlight_rows(&mut rows, "ma");
        assert_eq!(rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["Mail", "Maps", "Task Manager", "macOS"]);
        let titles: Vec<Option<&str>> = (0..rows.len()).map(|i| spotlight_caption(&rows, i)).collect();
        assert_eq!(titles, [Some("Top Hit"), Some("Applications"), None, Some("Desktop Styles")]);
        let mut only_apps = vec![row("apps.mail", "Mail"), row("apps.maps", "Maps")];
        spotlight_rows(&mut only_apps, "ma");
        let titles: Vec<Option<&str>> = (0..only_apps.len()).map(|i| spotlight_caption(&only_apps, i)).collect();
        assert_eq!(titles, [Some("Top Hit"), Some("Applications")], "one kind is headed too");
        let mut alone = vec![row("settings.appearance", "Appearance")];
        spotlight_rows(&mut alone, "dark");
        assert_eq!(spotlight_caption(&alone, 0), Some("Top Hit"), "a lone result is the Top Hit");
    }
}

#[cfg(test)]
mod help_search_tests {
    use super::*;

    fn menus(open: usize) -> MacMenus {
        let app = MacMenu { rows: vec![MacRow::item("About News", "mac.about"), MacRow::sep(), MacRow::item("Hide News", "mac.hide").key("\u{2318}H"), MacRow::item("Hide Others", "mac.hide-others")] };
        let mut appearance = MacRow::item("Appearance", "");
        appearance.sub = vec![MacRow::item("Light", "mac.light"), MacRow::item("Dark", "mac.dark")];
        let mut sidebar = MacRow::item("Hide Sidebar", "mac.sidebar");
        sidebar.ticks = true;
        sidebar.checked = true;
        let view = MacMenu { rows: vec![appearance, MacRow::item("Hidden Files", "").when(false), sidebar] };
        let help = MacMenu { rows: vec![MacRow::search(), MacRow::item("Keyboard Shortcuts", "mac.keys")] };
        MacMenus {
            menus: vec![app, view, help],
            titles: Vec::new(),
            open,
            cursor: None,
            sub_open: None,
            sub_cursor: None,
            card: Rect::default(),
            rows: Vec::new(),
            sub_card: Rect::default(),
            sub_rows: Vec::new(),
            opened: None,
            sliding: None,
            place: None,
            query: String::new(),
            base: None,
        }
    }

    fn labels(m: &MacMenus) -> Vec<String> {
        m.menus[m.open].rows.iter().map(|row| row.label.clone()).collect()
    }

    #[test]
    fn help_finds_the_bars_rows_and_an_empty_field_brings_its_own_back() {
        let mut m = menus(2);
        m.query = "HID".into();
        help_search(&mut m);
        assert_eq!(labels(&m), ["Search", "Menu Items", "Hide News", "Hide Others", "Hide Sidebar"], "a row with no target is not found");
        assert!(m.menus[2].rows.iter().all(|row| !row.ticks && !row.checked), "no tick column moves the field");
        assert_eq!(m.cursor, Some(2), "Return picks the first row found");
        assert_eq!(m.menus[2].rows[2].target, "mac.hide");
        m.query = "dark".into();
        help_search(&mut m);
        assert_eq!(labels(&m), ["Search", "Menu Items", "Appearance \u{25b8} Dark"]);
        assert_eq!(m.menus[2].rows[2].target, "mac.dark");
        m.query = "qxz".into();
        help_search(&mut m);
        assert_eq!(labels(&m), ["Search", "No Menu Items"]);
        assert_eq!(m.cursor, None);
        m.query.clear();
        help_search(&mut m);
        assert_eq!(labels(&m), ["Search", "Keyboard Shortcuts"]);
        assert_eq!(m.cursor, None);
    }

    #[test]
    fn help_lists_words_two_menus_share_once() {
        let mut m = menus(2);
        m.menus[0] = MacMenu { rows: vec![MacRow::item("About OctoSense", "mac.about")] };
        m.menus[1] = MacMenu { rows: vec![MacRow::item("About OctoSense", "mac.about-app"), MacRow::item("About Sharing", "mac.sharing")] };
        m.query = "about".into();
        help_search(&mut m);
        assert_eq!(labels(&m), ["Search", "Menu Items", "About OctoSense", "About Sharing"]);
        assert_eq!(m.menus[2].rows[2].target, "mac.about", "the first menu's row");
        assert_eq!(help_place(&m.menus, 2, &m.menus[2].rows[2]).map(|(mi, ri, _)| (mi, ri)), Some((0, 0)));
    }

    #[test]
    fn help_never_finds_its_own_rows() {
        let m = menus(2);
        assert!(help_found(&m.menus, 2, "keyboard").is_empty());
        assert_eq!(help_found(&m.menus, 0, "keyboard").len(), 1, "from another menu it would");
    }

    #[test]
    fn a_recent_app_result_does_not_move_helps_search_field() {
        let mut m = menus(2);
        m.menus[0].rows.push(MacRow::item("Recent Items", "").sub(vec![
            MacRow::item("News", "apps.news").app_icon("news"),
        ]));
        m.query = "news".into();
        help_search(&mut m);
        let found = m.menus[2].rows.iter().find(|row| row.target == "apps.news").unwrap();
        assert_eq!(found.label, "Recent Items ▸ News");
        assert!(m.menus[2].rows.iter().all(|row| row.app_icon.is_none()), "the results keep the search field in place");
    }
}

#[cfg(test)]
mod force_quit_tests {
    use super::*;

    #[test]
    fn the_arrows_choose_an_app_and_force_quit_names_the_one_chosen() {
        let apps = vec![("notes".to_string(), "Notes".to_string()), ("terminal".to_string(), "Terminal".to_string())];
        let mut panel = MacForceQuit::new(apps.clone(), None);
        assert_eq!(panel.target(), None, "nothing chosen, nothing to force");
        assert_eq!(panel.step(1), Some(0));
        assert_eq!(panel.step(-1), Some(1));
        panel.chosen = Some(1);
        assert_eq!(panel.step(1), Some(1), "the list's end stays");
        assert_eq!(panel.step(-1), Some(0));
        assert_eq!(panel.target().as_deref(), Some("mac.force-quit-app.terminal"));
        assert_eq!(MacForceQuit::new(apps, Some(5)).chosen, None, "a front app not listed chooses nothing");
        assert_eq!(MacForceQuit::new(Vec::new(), None).step(1), None);
    }
}
