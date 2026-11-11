//! The rows of a Mac's menu bar (`DesktopStyle::Macos`), and what they do. The
//! surface that draws them is `shell/menu/mac.rs`.

use super::*;
use crate::clients::LaunchPolicy;
use crate::shell::menu::mac::{MacAbout, MacForceQuit, MacMenu, MacRow};
use makepad_platform::event::TextClipboardEvent;
use std::cell::RefCell;
use std::rc::Rc;

/// What the menus already are; any other name in a row is a menu of the app's
/// own, between View and Window.
const STANDARD_MENUS: [&str; 6] = ["App", "File", "Edit", "View", "Window", "Help"];
/// How much of an app's answer the bar reads.
const MAX_ROWS: usize = 64;
const MAX_ROWS_JSON: usize = 32 * 1024;
const MAX_TITLE_CHARS: usize = 60;
/// How many apps Recent Items keeps, as a Mac does by default.
const RECENT_APPS: usize = 10;
/// The © line of OctoSense's About panels, as the repository's NOTICE has it.
const COPYRIGHT: &str = "Copyright \u{a9} 2026 The OctoSense Authors";

/// A row an app adds to the menu bar while it is in front: a script app's
/// `app_menus()`, a native app's [`native_rows`]. No title makes it a rule.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
#[serde(default)]
pub(super) struct AppRow {
    menu: String,
    title: String,
    /// `shift+cmd+R`; shown in a Mac's glyphs, and a `cmd` one is the row's own
    /// wherever the keyboard is in the window.
    key: String,
    /// What `on_menu_command` gets; `key:<key>` sends the app that key instead.
    command: String,
    enabled: Option<bool>,
    checked: bool,
    /// The submenu of `menu` the row goes in ("Update Frequency"), or none.
    submenu: String,
}

fn row_takes_close_key(row: &AppRow) -> bool {
    row.enabled != Some(false) && row.key == "cmd+W"
}

/// What a script app's `app_menus()` answered: the rows the bar will show.
fn parse_rows(json: &str) -> Vec<AppRow> {
    let mut rows: Vec<AppRow> = serde_json::from_str(json).unwrap_or_default();
    rows.truncate(MAX_ROWS);
    rows.retain(|row| !row.menu.is_empty());
    for row in &mut rows {
        if row.title.chars().count() > MAX_TITLE_CHARS {
            row.title = row.title.chars().take(MAX_TITLE_CHARS).collect();
        }
    }
    rows
}

/// A row of a native app: its own key, sent to it as if pressed.
fn keyed(menu: &str, title: &str, key: &str) -> AppRow {
    AppRow { menu: menu.into(), title: title.into(), key: key.into(), command: format!("key:{key}"), ..Default::default() }
}

/// The process apps that read the Edit menu's commands with no text field in
/// front: a grid's cells, a page's selection, a terminal's.
const EDITS_WITHOUT_A_FIELD: &[&str] = &["sheets", "browser", "terminal", "pdf"];

/// The commands of the apps that have no script to ask: only keys they answer to.
fn native_rows(app: &str) -> Vec<AppRow> {
    match app {
        "notes" => vec![
            keyed("File", "New Note", "cmd+N"),
            keyed("File", "New Folder\u{2026}", "shift+cmd+N"),
            keyed("Edit", "Find\u{2026}", "cmd+F"),
            keyed("View", "Sidebar", "ctrl+cmd+S"),
            keyed("Format", "Bold", "cmd+B"),
            keyed("Format", "Italic", "cmd+I"),
            keyed("Format", "Underline", "cmd+U"),
            keyed("Format", "Strikethrough", "shift+cmd+X"),
        ],
        "reminders" => vec![
            keyed("File", "New Reminder", "cmd+N"),
            keyed("File", "New List\u{2026}", "shift+cmd+N"),
            keyed("Edit", "Find\u{2026}", "cmd+F"),
            keyed("View", "Sidebar", "ctrl+cmd+S"),
        ],
        "clock" => vec![
            keyed("File", "New Alarm\u{2026}", "cmd+N"),
            keyed("File", "Add City\u{2026}", "shift+cmd+N"),
            keyed("View", "World Clock", "cmd+1"),
            keyed("View", "Alarms", "cmd+2"),
            keyed("View", "Stopwatch", "cmd+3"),
            keyed("View", "Timer", "cmd+4"),
        ],
        "weather" => vec![
            keyed("File", "Add Location\u{2026}", "cmd+N"),
            keyed("View", "Sidebar", "ctrl+cmd+S"),
            keyed("View", "Refresh", "cmd+R"),
        ],
        "calculator" => vec![
            keyed("View", "Basic", "cmd+1"),
            keyed("View", "Scientific", "cmd+2"),
            keyed("View", "History", "cmd+T"),
        ],
        "terminal" => vec![
            keyed("App", "Settings\u{2026}", "cmd+,"),
            keyed("File", "New Tab", "cmd+T"),
            keyed("Edit", "Find\u{2026}", "cmd+F"),
            keyed("Edit", "Find Next", "cmd+G"),
            keyed("Edit", "Find Previous", "shift+cmd+G"),
            keyed("View", "Bigger", "cmd+="),
            keyed("View", "Smaller", "cmd+-"),
            keyed("View", "Actual Size", "cmd+0"),
        ],
        "task" => vec![keyed("Edit", "Find\u{2026}", "cmd+F")],
        "octoscode" => vec![
            keyed("App", "Settings\u{2026}", "cmd+,"),
            keyed("File", "New Chat", "cmd+N"),
            keyed("File", "Add Workspace\u{2026}", "cmd+O"),
            keyed("Edit", "Find\u{2026}", "cmd+F"),
            keyed("View", "Sidebar", "ctrl+cmd+S"),
        ],
        "sheets" => vec![
            keyed("File", "New", "cmd+N"),
            keyed("File", "Open\u{2026}", "cmd+O"),
            keyed("File", "Save", "cmd+S"),
            keyed("File", "Save As\u{2026}", "shift+cmd+S"),
            keyed("Format", "Bold", "cmd+B"),
            keyed("Format", "Italic", "cmd+I"),
        ],
        "files" => vec![
            keyed("File", "New Tab", "cmd+T"),
            keyed("File", "New Folder", "shift+cmd+N"),
            keyed("File", "Get Info", "cmd+I"),
            // What these do to the files that are selected.
            keyed("Edit", "Undo", "cmd+Z"),
            keyed("Edit", "Cut", "cmd+X"),
            keyed("Edit", "Copy", "cmd+C"),
            keyed("Edit", "Paste", "cmd+V"),
            keyed("Edit", "Select All", "cmd+A"),
            keyed("Edit", "Find\u{2026}", "cmd+F"),
            // Finder's views; where Finder has its gallery, the disk map.
            keyed("View", "as Icons", "cmd+1"),
            keyed("View", "as List", "cmd+2"),
            keyed("View", "as Columns", "cmd+3"),
            keyed("View", "as Disk Map", "cmd+4"),
            AppRow { menu: "View".into(), ..Default::default() },
            keyed("View", "Bigger", "cmd+="),
            keyed("View", "Smaller", "cmd+-"),
            keyed("Go", "Go to Folder\u{2026}", "cmd+L"),
        ],
        "finance" => vec![
            keyed("File", "New Transaction\u{2026}", "cmd+N"),
            keyed("File", "New Account\u{2026}", "shift+cmd+N"),
            keyed("Edit", "Find\u{2026}", "cmd+F"),
            keyed("View", "Sidebar", "ctrl+cmd+S"),
            keyed("View", "Overview", "cmd+1"),
            keyed("View", "Ledger", "cmd+2"),
            keyed("View", "Budget", "cmd+3"),
            keyed("View", "Reports", "cmd+4"),
        ],
        "route" => vec![
            keyed("Edit", "Find\u{2026}", "cmd+F"),
            keyed("View", "Sidebar", "ctrl+cmd+S"),
            keyed("View", "Zoom In", "cmd+="),
            keyed("View", "Zoom Out", "cmd+-"),
            keyed("Go", "Current Location", "cmd+L"),
            keyed("Go", "Directions", "cmd+R"),
        ],
        "image" => vec![
            keyed("View", "Thumbnails", "shift+cmd+T"),
            keyed("View", "Zoom In", "cmd+="),
            keyed("View", "Zoom Out", "cmd+-"),
            keyed("View", "Actual Size", "cmd+0"),
            keyed("View", "Zoom to Fit", "cmd+9"),
        ],
        "pdf" => vec![
            keyed("Edit", "Find\u{2026}", "cmd+F"),
            keyed("Edit", "Find Next", "cmd+G"),
            keyed("Edit", "Find Previous", "shift+cmd+G"),
            keyed("View", "Thumbnails", "shift+cmd+T"),
            keyed("View", "Zoom In", "cmd+="),
            keyed("View", "Zoom Out", "cmd+-"),
            keyed("View", "Actual Size", "cmd+0"),
            keyed("View", "Zoom to Fit", "cmd+9"),
            keyed("Go", "Go to Page\u{2026}", "alt+cmd+G"),
        ],
        "aichat" => vec![keyed("File", "New Chat", "cmd+N"), keyed("View", "Sidebar", "ctrl+cmd+S")],
        "browser" => vec![
            keyed("File", "New Tab", "cmd+T"),
            keyed("File", "Open Location\u{2026}", "cmd+L"),
            keyed("Edit", "Find\u{2026}", "cmd+F"),
            keyed("View", "Reload", "cmd+R"),
            keyed("View", "Stop", "cmd+."),
            keyed("History", "Back", "cmd+["),
            keyed("History", "Forward", "cmd+]"),
            keyed("History", "Show History", "cmd+Y"),
            keyed("History", "Clear History\u{2026}", "shift+cmd+delete"),
            keyed("Bookmarks", "Add Bookmark\u{2026}", "cmd+D"),
            keyed("Bookmarks", "Show Bookmarks", "alt+cmd+B"),
        ],
        _ => Vec::new(),
    }
}

/// The key a row names: its code and modifiers.
fn parse_key(spec: &str) -> Option<(KeyCode, KeyModifiers)> {
    let mut modifiers = KeyModifiers::default();
    // The last part is the key.
    let mut parts: Vec<&str> = spec.split('+').collect();
    let key = parts.pop()?;
    for part in parts {
        match part {
            "cmd" => modifiers.logo = true,
            "shift" => modifiers.shift = true,
            "alt" => modifiers.alt = true,
            "ctrl" => modifiers.control = true,
            _ => return None,
        }
    }
    let code = match key {
        "," => KeyCode::Comma,
        "." => KeyCode::Period,
        "/" => KeyCode::Slash,
        "-" => KeyCode::Minus,
        "=" => KeyCode::Equals,
        "[" => KeyCode::LBracket,
        "]" => KeyCode::RBracket,
        "'" => KeyCode::Quote,
        "delete" => KeyCode::Backspace,
        "return" => KeyCode::ReturnKey,
        "space" => KeyCode::Space,
        "left" => KeyCode::ArrowLeft,
        "right" => KeyCode::ArrowRight,
        "up" => KeyCode::ArrowUp,
        "down" => KeyCode::ArrowDown,
        one => {
            let mut chars = one.chars();
            let (first, rest) = (chars.next()?, chars.next());
            if rest.is_some() {
                return None;
            }
            match first.to_ascii_uppercase() {
                'A' => KeyCode::KeyA, 'B' => KeyCode::KeyB, 'C' => KeyCode::KeyC, 'D' => KeyCode::KeyD, 'E' => KeyCode::KeyE,
                'F' => KeyCode::KeyF, 'G' => KeyCode::KeyG, 'H' => KeyCode::KeyH, 'I' => KeyCode::KeyI, 'J' => KeyCode::KeyJ,
                'K' => KeyCode::KeyK, 'L' => KeyCode::KeyL, 'M' => KeyCode::KeyM, 'N' => KeyCode::KeyN, 'O' => KeyCode::KeyO,
                'P' => KeyCode::KeyP, 'Q' => KeyCode::KeyQ, 'R' => KeyCode::KeyR, 'S' => KeyCode::KeyS, 'T' => KeyCode::KeyT,
                'U' => KeyCode::KeyU, 'V' => KeyCode::KeyV, 'W' => KeyCode::KeyW, 'X' => KeyCode::KeyX, 'Y' => KeyCode::KeyY,
                'Z' => KeyCode::KeyZ, '0' => KeyCode::Key0, '1' => KeyCode::Key1, '2' => KeyCode::Key2, '3' => KeyCode::Key3,
                '4' => KeyCode::Key4, '5' => KeyCode::Key5, '6' => KeyCode::Key6, '7' => KeyCode::Key7, '8' => KeyCode::Key8,
                '9' => KeyCode::Key9,
                _ => return None,
            }
        }
    };
    Some((code, modifiers))
}

/// A row's key as a Mac's menu shows it: `shift+cmd+R` is ⇧⌘R.
fn key_glyphs(spec: &str) -> String {
    if spec.is_empty() {
        return String::new();
    }
    let (mut parts, mut out): (Vec<&str>, String) = (spec.split('+').collect(), String::new());
    let key = parts.pop().unwrap_or_default();
    for (name, glyph) in [("ctrl", '\u{2303}'), ("alt", '\u{2325}'), ("shift", '\u{21e7}'), ("cmd", '\u{2318}')] {
        if parts.contains(&name) {
            out.push(glyph);
        }
    }
    match key {
        "delete" => out.push('\u{232b}'),
        "return" => out.push('\u{21a9}'),
        "space" => out.push_str("Space"),
        "left" => out.push('\u{2190}'),
        "right" => out.push('\u{2192}'),
        "up" => out.push('\u{2191}'),
        "down" => out.push('\u{2193}'),
        other => out.push_str(&other.to_uppercase()),
    }
    out
}

fn key_matches(spec: &str, e: &KeyEvent) -> bool {
    parse_key(spec).is_some_and(|(code, m)| {
        code == e.key_code && m.logo == e.modifiers.logo && m.shift == e.modifiers.shift && m.alt == e.modifiers.alt && m.control == e.modifiers.control
    })
}

/// A row of the app's own, in the menu it joins.
/// The Mac's sound, set off the UI thread and only the latest asked for: a drag asks far
/// faster than `osascript` answers. A hidden test shell leaves the machine's sound alone.
fn ask_host_sound(level: u32, muted: bool) {
    log!("wm: host sound {level} muted {muted}");
    if crate::launch_input::hidden_windows() {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        use std::sync::Mutex;
        static ASKED: Mutex<(Option<(u32, bool)>, bool)> = Mutex::new((None, false));
        let Ok(mut asked) = ASKED.lock() else { return };
        asked.0 = Some((level, muted));
        if std::mem::replace(&mut asked.1, true) {
            return;
        }
        std::thread::spawn(|| loop {
            let next = ASKED.lock().ok().and_then(|mut asked| {
                let next = asked.0.take();
                asked.1 = next.is_some();
                next
            });
            let Some((level, muted)) = next else { return };
            let with = if muted { "with" } else { "without" };
            let _ = std::process::Command::new("osascript").args(["-e", &format!("set volume output volume {level} {with} output muted")]).output();
        });
    }
}

/// The Mac's own System Settings at a pane. A hidden test shell only says which.
pub(crate) fn open_host_settings(pane: &str) {
    let id = match pane {
        "network" => "com.apple.Network-Settings.extension",
        "bluetooth" => "com.apple.BluetoothSettings",
        "sound" => "com.apple.Sound-Settings.extension",
        "battery" => "com.apple.Battery-Settings.extension",
        "location" => "com.apple.preference.security?Privacy_LocationServices",
        _ => return,
    };
    let url = format!("x-apple.systempreferences:{id}");
    log!("wm: host settings {url}");
    if crate::launch_input::hidden_windows() {
        return;
    }
    #[cfg(target_os = "macos")]
    std::thread::spawn(move || {
        let _ = std::process::Command::new("open").arg(url).status();
    });
}

/// The Mac's Character Viewer, over the window: what is picked in it is typed into
/// the field with the keyboard, as from the Mac's own keyboard.
#[cfg(target_os = "macos")]
fn mac_character_palette() {
    use makepad_widgets::makepad_platform::{makepad_objc_sys::{class, msg_send, sel, sel_impl}, os::apple::apple_sys::*};
    unsafe {
        let app: ObjcId = msg_send![class!(NSApplication), sharedApplication];
        let () = msg_send![app, orderFrontCharacterPalette: nil];
    }
}

#[cfg(not(target_os = "macos"))]
fn mac_character_palette() {}

/// Whether this Mac's AppKit answers Start Dictation…; its row is left out where it doesn't.
#[cfg(target_os = "macos")]
fn mac_can_dictate() -> bool {
    use makepad_widgets::makepad_platform::{makepad_objc_sys::{class, msg_send, sel, sel_impl}, os::apple::apple_sys::*};
    unsafe {
        let app: ObjcId = msg_send![class!(NSApplication), sharedApplication];
        let answers: BOOL = msg_send![app, respondsToSelector: sel!(startDictation:)];
        answers == YES
    }
}

#[cfg(not(target_os = "macos"))]
fn mac_can_dictate() -> bool {
    false
}

/// Start Dictation…: AppKit's own, to the window's text input.
#[cfg(target_os = "macos")]
fn mac_start_dictation() {
    use makepad_widgets::makepad_platform::{makepad_objc_sys::{class, msg_send, sel, sel_impl}, os::apple::apple_sys::*};
    if !mac_can_dictate() {
        return;
    }
    unsafe {
        let app: ObjcId = msg_send![class!(NSApplication), sharedApplication];
        let () = msg_send![app, startDictation: nil];
    }
}

#[cfg(not(target_os = "macos"))]
fn mac_start_dictation() {}

fn app_row(row: &AppRow) -> MacRow {
    if row.title.is_empty() {
        return MacRow::sep();
    }
    MacRow::item(&row.title, &format!("mac.app.{}", row.command)).key(&key_glyphs(&row.key)).when(row.enabled.unwrap_or(true)).checked(row.checked)
}

/// The submenu of a Mac's Edit menu a row of an app's own goes in: Find and its
/// kin, the case of the selection.
fn edit_submenu(title: &str) -> Option<&'static str> {
    match title {
        "Find" | "Find\u{2026}" | "Find and Replace\u{2026}" | "Find Next" | "Find Previous" | "Use Selection for Find" | "Jump to Selection" => Some("Find"),
        "Make Upper Case" | "Make Lower Case" | "Capitalize" => Some("Transformations"),
        _ => None,
    }
}

/// A Mac's Find submenu, in its order, with each row's key.
const FIND_ROWS: &[(&str, &str)] = &[
    ("Find\u{2026}", "cmd+F"),
    ("Find and Replace\u{2026}", "alt+cmd+F"),
    ("Find Next", "cmd+G"),
    ("Find Previous", "shift+cmd+G"),
    ("Use Selection for Find", "cmd+E"),
    ("Jump to Selection", "cmd+J"),
];

/// An app's Find rows in a Mac's order: what it doesn't answer is there all the
/// same, greyed, as a Mac's Find submenu always holds its six; any row of its own
/// after them.
fn find_rows(own: Vec<MacRow>) -> Vec<MacRow> {
    let standard = |row: &MacRow| FIND_ROWS.iter().any(|(title, _)| row.label == *title);
    let mut out: Vec<MacRow> = FIND_ROWS
        .iter()
        .map(|(title, key)| {
            own.iter()
                .find(|row| row.label == *title)
                .cloned()
                .unwrap_or_else(|| MacRow::item(title, "").key(&key_glyphs(key)).when(false))
        })
        .collect();
    out.extend(own.into_iter().filter(|row| !standard(row)));
    out
}

/// An app's own Edit rows, as a Mac's Edit menu holds them: Find and its kin in
/// a Find submenu, the case of the selection in Transformations, each where its
/// first row was. A lone "Find" opens the find field, so it reads "Find…" there.
fn edit_rows<'a>(rows: impl Iterator<Item = &'a AppRow>) -> Vec<MacRow> {
    let rows: Vec<&AppRow> = rows.collect();
    // A row that names its submenu goes there; Find's kin and the case rows go in theirs.
    let sub_of = |row: &AppRow| if row.submenu.is_empty() { edit_submenu(&row.title).map(String::from) } else { Some(row.submenu.clone()) };
    let mut out: Vec<MacRow> = Vec::new();
    let mut placed: Vec<String> = Vec::new();
    for row in &rows {
        let Some(sub) = sub_of(row) else {
            out.push(app_row(row));
            continue;
        };
        if placed.contains(&sub) {
            continue;
        }
        placed.push(sub.clone());
        let inner = rows
            .iter()
            .filter(|other| sub_of(other).as_ref() == Some(&sub))
            .map(|other| {
                let mut other = (*other).clone();
                if other.title == "Find" {
                    other.title = "Find\u{2026}".into();
                }
                app_row(&other)
            })
            .collect();
        let inner = if sub == "Find" { find_rows(inner) } else { inner };
        out.push(MacRow::item(&sub, "").sub(inner));
    }
    // Rules that the moved rows left side by side, or at an end.
    let mut kept: Vec<MacRow> = Vec::new();
    for row in out {
        if row.separator && kept.last().is_none_or(|last| last.separator) {
            continue;
        }
        kept.push(row);
    }
    if kept.last().is_some_and(|last| last.separator) {
        kept.pop();
    }
    kept
}

/// The app's rows for `menu`; rows that name a submenu go in it, where its first
/// row was (Task's View > Update Frequency).
fn rows_of(rows: &[AppRow], menu: &str) -> Vec<MacRow> {
    let rows: Vec<&AppRow> = rows.iter().filter(|row| row.menu == menu).collect();
    let mut out = Vec::new();
    let mut placed: Vec<&str> = Vec::new();
    for row in &rows {
        if row.submenu.is_empty() {
            out.push(app_row(row));
            continue;
        }
        if placed.contains(&row.submenu.as_str()) {
            continue;
        }
        placed.push(&row.submenu);
        let inner = rows.iter().filter(|other| other.submenu == row.submenu).map(|other| app_row(other)).collect();
        out.push(MacRow::item(&row.submenu, "").sub(inner));
    }
    out
}

/// The menus an app has of its own, in the order it names them.
fn own_menus(rows: &[AppRow]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for row in rows {
        if !STANDARD_MENUS.contains(&row.menu.as_str()) && !names.contains(&row.menu) {
            names.push(row.menu.clone());
        }
    }
    names
}

impl App {
    /// Whether the window in front shows a sheet that owns the keyboard. Close waits for
    /// it too, as on a Mac: nothing closes the window, or a tab, from under its question.
    pub(super) fn focused_sheet_up(&mut self, cx: &mut Cx) -> bool {
        let Some(client) = self.state_mut().layout.focused_client() else { return false };
        self.desk(cx)
            .borrow_mut::<crate::desk::WmDesk>()
            .and_then(|mut desk| desk.with_tile(cx, client, |_, tile| tile.sheet_owns_keyboard()))
            .unwrap_or(false)
    }

    /// What the app in front adds to the menu bar: asked each time, so its rows
    /// say what is true now.
    fn mac_app_rows(&mut self, cx: &mut Cx) -> Vec<AppRow> {
        let state = self.state_mut();
        let Some(client) = state.layout.focused_client() else { return Vec::new() };
        let Some(app) = state.clients.get(&client).map(|s| s.app.clone()) else { return Vec::new() };
        let answer = self
            .module_host
            .dispatch(cx, client, "its menu rows", |cx, root| {
                // A host's sheet over the window has it all to itself.
                if root.splash(cx, ids!(sheet)).borrow().is_some_and(|sheet| sheet.view.visible) {
                    return Some("[]".to_string());
                }
                root.splash(cx, ids!(card)).ask_script_fn(cx, id!(app_menus), &[], MAX_ROWS_JSON)
            })
            .flatten()
            // A native module says what its rows are now, a process app what it last
            // sent (`WmRequest::MenuRows`), or the bar keeps its own.
            .or_else(|| self.module_host.menu_rows(cx, client))
            .or_else(|| self.client_menu_rows.get(&client).cloned());
        let mut rows = match answer {
            Some(json) => parse_rows(&json),
            None => native_rows(&app),
        };
        // A sheet that owns the keyboard is all the window has to say: its rows wait.
        if self.focused_sheet_up(cx) {
            for row in &mut rows {
                row.enabled = Some(false);
            }
        }
        rows
    }

    /// The titles after the app's name, for the window in front. An app's own
    /// menus are read when it comes to the front.
    pub(super) fn mac_titles(&mut self, cx: &mut Cx) -> Vec<String> {
        let focus = self.state_mut().layout.focused_client();
        if self.mac_titles_of != Some(focus) {
            let rows = self.mac_app_rows(cx);
            self.mac_own_menus = own_menus(&rows);
            // Until a script has run it has nothing to say: ask again.
            if !rows.is_empty() || focus.is_none() {
                self.mac_titles_of = Some(focus);
            }
        }
        // Terminal's first menu is its Shell, as a Mac's Terminal has it.
        let terminal = focus.and_then(|c| self.state_mut().clients.get(&c).map(|slot| slot.app == "terminal")).unwrap_or(false);
        let first = if terminal { "Shell" } else { "File" };
        let mut titles: Vec<String> = [first, "Edit", "View"].map(String::from).to_vec();
        titles.extend(self.mac_own_menus.iter().cloned());
        titles.extend(["Window", "Help"].map(String::from));
        titles
    }

    /// Run an app's row, with the keyboard back where it was.
    fn mac_app_command(&mut self, cx: &mut Cx, command: &str) {
        let Some(client) = self.state_mut().layout.focused_client() else { return };
        // A choice from a menu is the person's own act in that window.
        self.presenter.press(client, host::now());
        if let Some((key_code, modifiers)) = command.strip_prefix("key:").and_then(parse_key) {
            self.mac_press(cx, KeyEvent { key_code, modifiers, ..Default::default() });
            return;
        }
        // A native module's row with no key: a submenu's folder.
        if self.module_host.menu_command(cx, client, command) {
            return;
        }
        // A process app's own row goes back to it.
        if self.client_menu_rows.contains_key(&client) && !self.module_host.is_module(client) {
            self.send_wm_event(client, WmEvent::MenuCommand { command: command.to_string() });
            return;
        }
        let called = self.module_host.dispatch(cx, client, "its menu command", |cx, root| root.splash(cx, ids!(card)).call_script_fn_with_strings(cx, id!(on_menu_command), &[command]));
        if called == Some(true) {
            cx.with_vm_and_async(|_| {});
        }
    }

    /// A key on one of the rows of the app in front belongs to that row, as a
    /// Mac's menus take their keys first. Not while something else holds the
    /// keyboard; a native app hears its own keys anyway.
    pub(super) fn mac_app_key(&mut self, cx: &mut Cx, e: &KeyEvent) -> bool {
        if !e.modifiers.logo || self.chat_holds_keyboard(cx) || self.ai_pane_is_open(cx) || approvals::with(|a| a.router.front_sheet().is_some() || a.settings_open).unwrap_or(false) {
            return false;
        }
        // Paste and Match Style, from the keyboard: the app's own row where it has one, as
        // the menu has it (Writer's pastes the characters, where its editor reads a paste
        // as Markdown), else Edit > Paste's text. A grey row of its own does nothing.
        let plain_paste = e.key_code == KeyCode::KeyV && e.modifiers.shift && e.modifiers.alt && !e.modifiers.control;
        if plain_paste && cfg!(target_os = "macos") && self.mac_can_edit(cx) && !self.mac_focus_pressed(cx) {
            let own = self.mac_app_rows(cx).into_iter().find(|row| row.menu == "Edit" && row.title == "Paste and Match Style");
            match own {
                Some(row) if row.enabled == Some(false) => {}
                Some(row) => {
                    self.mac_app_command(cx, &row.command);
                    self.update_bar(cx);
                    self.redraw_all(cx);
                }
                None => self.mac_edit(cx, "paste"),
            }
            return true;
        }
        // A text field with the keyboard keeps the keys the Edit menu is for. A view a
        // press gave it to has no text: the app's own rows have them first, as a Mac's do.
        let edit_key = matches!(
            e.key_code,
            KeyCode::KeyA | KeyCode::KeyC | KeyCode::KeyV | KeyCode::KeyX | KeyCode::KeyZ | KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::Backspace
        );
        if edit_key && self.mac_can_edit(cx) && !self.mac_focus_pressed(cx) {
            return false;
        }
        let rows = self.mac_app_rows(cx);
        let Some(row) = rows.iter().find(|row| row.enabled != Some(false) && !row.command.starts_with("key:") && !row.title.is_empty() && key_matches(&row.key, e)) else {
            return false;
        };
        let command = row.command.clone();
        self.mac_app_command(cx, &command);
        self.update_bar(cx);
        self.redraw_all(cx);
        true
    }

    /// Whether a chat pane has the keyboard now.
    fn chat_holds_keyboard(&mut self, cx: &mut Cx) -> bool {
        [ids!(shell_app_chat), ids!(shell_system_chat)].into_iter().any(|path| {
            self.ui.widget(cx, path).borrow::<system_chat::view::ShellSystemChat>().is_some_and(|pane| pane.has_keyboard(cx))
        })
    }

    /// Whether the window in front has text for the Edit menu to act on. A
    /// process app holds its keyboard in its own process, and says when one of
    /// its fields has it. A pane over the window can hold the keyboard instead.
    fn mac_can_edit(&mut self, cx: &mut Cx) -> bool {
        let Some(client) = self.state_mut().layout.focused_client() else { return true };
        if !self.module_host.is_module(client) {
            let ai_pane = self.ui.widget(cx, ids!(shell_ai_pane)).borrow::<shell::ai_pane::ShellAiPane>().is_some_and(|pane| pane.is_open());
            if ai_pane || self.chat_holds_keyboard(cx) {
                return true;
            }
            let state = self.state_mut();
            return state.clients.get(&client).is_some_and(|slot| EDITS_WITHOUT_A_FIELD.contains(&slot.app.as_str()))
                || state.phone.ime.get(&client).is_some_and(|ime| ime.visible);
        }
        // A button or a checkbox with the keyboard has no text: the app's own Undo acts.
        if self.mac_pressed {
            self.mac_press_typing
        } else {
            !cx.key_focus().is_empty() && !cx.key_focus_kept() && !CxDraw::key_focus_is_control(cx)
        }
    }

    /// Whether the keyboard in the window in front is with a view only because it was
    /// pressed: a canvas, a list, a segment. The app's own Undo, Copy or Select All acts
    /// there, as no field is there to take it; a key with no row of the app's still goes in.
    fn mac_focus_pressed(&mut self, cx: &mut Cx) -> bool {
        let Some(client) = self.state_mut().layout.focused_client() else { return false };
        if !self.module_host.is_module(client) || self.chat_holds_keyboard(cx) {
            return false;
        }
        if self.mac_pressed { self.mac_press_view } else { cx.key_focus_pressed() }
    }

    /// What the field with the keyboard can do, by its own account. A process app's
    /// is in its own process, unless a pane over its window has the keyboard.
    fn mac_edit_field(&mut self, cx: &mut Cx) -> makepad_platform::ime::HostedEdit {
        let Some(client) = self.state_mut().layout.focused_client() else { return Default::default() };
        if !self.module_host.is_module(client) && !self.chat_holds_keyboard(cx) {
            let ai_pane = self.ui.widget(cx, ids!(shell_ai_pane)).borrow::<shell::ai_pane::ShellAiPane>().is_some_and(|pane| pane.is_open());
            return if ai_pane { Default::default() } else { self.client_edits.get(&client).copied().unwrap_or_default() };
        }
        if self.mac_pressed { self.mac_press_edit } else { cx.hosted_edit() }
    }

    /// The menu bar's menu at `index`: the mark, the app, then File and the rest.
    pub(super) fn toggle_mac_menu(&mut self, cx: &mut Cx, index: usize) {
        let menu = self.ui.widget(cx, ids!(shell_menu));
        if menu.borrow::<ShellMenu>().is_some_and(|m| m.is_open()) {
            self.close_shell_menu(cx);
            self.mac_restore_focus(cx);
            return;
        }
        let rows = self.mac_app_rows(cx);
        let can_edit = self.mac_can_edit(cx);
        let pressed = self.mac_focus_pressed(cx);
        let field = self.mac_edit_field(cx);
        self.mac_own_menus = own_menus(&rows);
        self.update_bar(cx);
        self.mac_menu_takes_keyboard();
        let sheet_up = self.focused_sheet_up(cx);
        let menus = self.mac_menus(&rows, can_edit, pressed, field, sheet_up);
        let titles = self.shell_bar(cx).borrow::<shell::bar::ShellBar>().map(|bar| bar.mac_title_rects()).unwrap_or_default();
        if let Some(mut menu) = menu.borrow_mut::<ShellMenu>() {
            menu.open_mac(cx, menus, titles, index);
        }
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    /// A Dock icon's menu, as on a Mac: the app's windows, then what the app can do.
    pub(super) fn open_dock_menu(&mut self, cx: &mut Cx, hit: desktop::ShelfHit, column: Rect) {
        let rows = match hit {
            desktop::ShelfHit::App(app) => {
                let of_app = self.mac_windows_of(&app);
                let focus = self.state_mut().layout.focused_client();
                let mut rows: Vec<MacRow> = self
                    .mac_windows()
                    .into_iter()
                    .filter(|(client, ..)| of_app.contains(client))
                    .map(|(client, title, _)| MacRow::item(&title, &format!("mac.window.{client}")).checked(focus == Some(client)))
                    .collect();
                let running = !rows.is_empty();
                // Options: whether the app stays in the Dock when it isn't
                // running. Files always does, as a Mac's Finder.
                let kept = self.state_mut().dock_pins.iter().any(|id| *id == app);
                let at_login = self.login_apps.iter().any(|id| *id == app);
                let current = self.state_mut().layout.active;
                let options = (app != "files" && (running || kept)).then(|| {
                    let keep = format!("mac.dock-keep.{app}");
                    let row = if running { MacRow::item("Keep in Dock", &keep).checked(kept) } else { MacRow::item("Remove from Dock", &keep) };
                    // The shell opens it when it starts (`App::open_login_apps`).
                    let login = MacRow::item("Open at Login", &format!("mac.dock-login.{app}")).checked(at_login);
                    let mut sub = vec![row, login, MacRow::sep()];
                    sub.extend(assign_rows(&app, assigned(&app), current));
                    MacRow::item("Options", "").sub(sub)
                });
                if !running {
                    if let Some(options) = options {
                        rows.push(options);
                        rows.push(MacRow::sep());
                    }
                    rows.push(MacRow::item("Open", &format!("mac.dock-open.{app}")));
                } else {
                    rows.push(MacRow::sep());
                    if clients::find_app(&app).is_some_and(|a| a.policy == LaunchPolicy::AlwaysNew) {
                        rows.push(MacRow::item("New Window", &format!("mac.dock-open.{app}")));
                        rows.push(MacRow::sep());
                    }
                    if let Some(options) = options {
                        rows.push(options);
                        rows.push(MacRow::sep());
                    }
                    // App Exposé: its windows on this desktop, side by side, those in the Dock too.
                    let (out, minimized) = self.expose_windows(&app);
                    rows.push(MacRow::item("Show All Windows", &format!("mac.dock-expose.{app}")).when(!out.is_empty() || !minimized.is_empty()));
                    rows.push(MacRow::item("Hide", &format!("mac.dock-hide.{app}")));
                    rows.push(MacRow::item("Quit", &format!("mac.dock-quit.{app}")));
                }
                rows
            }
            desktop::ShelfHit::Window(client) => vec![
                MacRow::item("Open", &format!("mac.window.{client}")),
                MacRow::item("Close", &format!("mac.dock-close.{client}")),
            ],
            desktop::ShelfHit::Assistant => {
                vec![MacRow::item(if crate::system_chat::is_open() { "Close" } else { "Open" }, "mac.dock-assistant")]
            }
            desktop::ShelfHit::Trash => {
                let full = self.state_mut().trash_full;
                vec![MacRow::item("Open", "mac.dock-trash"), MacRow::item("Empty Trash", "mac.dock-empty-trash").when(full)]
            }
            desktop::ShelfHit::Launcher | desktop::ShelfHit::ShowDesktop => return,
        };
        self.close_focused_menu(cx);
        self.mac_menu_takes_keyboard();
        // The icon's name makes way for its menu.
        if let Some(mut shelf) = self.ui.widget(cx, ids!(desktop_shelf)).borrow_mut::<desktop::DesktopShelf>() {
            shelf.hover_at(cx, dvec2(-1.0, -1.0));
        }
        if let Some(mut menu) = self.ui.widget(cx, ids!(shell_menu)).borrow_mut::<ShellMenu>() {
            menu.open_mac_above(cx, MacMenu { rows }, column);
        }
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    /// A Dock menu's Options ▸ Assign To: the app's windows go to the
    /// desktop in front (This Desktop), stay on every desktop from it (All
    /// Desktops), or open wherever the person is (None), as a Mac's do. The
    /// choice is kept (`dock-desktops`).
    fn mac_assign_desktop(&mut self, app: &str, how: &str) {
        let current = self.state_mut().layout.active;
        let assign = match how {
            "all" => Some(Assign::All),
            "this" => Some(Assign::Desktop(current)),
            "none" => None,
            n => match n.parse::<usize>() {
                Ok(n) if n < layout::WORKSPACES => Some(Assign::Desktop(n)),
                _ => return,
            },
        };
        set_assigned(app, assign);
        // Its windows go where it belongs now; with None they stay put.
        let to = match assign {
            Some(Assign::All) => Some(current),
            Some(Assign::Desktop(n)) => Some(n),
            None => None,
        };
        if let Some(to) = to {
            for client in self.mac_windows_of(app) {
                self.state_mut().layout.move_client_to(client, to);
            }
        }
        self.mac_sync_every_desktop();
        log!("wm: {app} is assigned to {}", match assign {
            Some(Assign::All) => "all desktops".to_string(),
            Some(Assign::Desktop(n)) => format!("desktop {}", n + 1),
            None => "no desktop".to_string(),
        });
    }

    /// The windows a Mac shows on every desktop (Assign To ▸ All Desktops),
    /// for the layout to carry along (`WmLayout::every_desktop`); none in
    /// another style.
    pub(super) fn mac_sync_every_desktop(&mut self) {
        let Some(state) = self.state.as_mut() else { return };
        if state.style.target != desktop::DesktopStyle::Macos {
            state.layout.every_desktop.clear();
            return;
        }
        let every: Vec<ClientId> = state
            .clients
            .iter()
            .filter(|(client, slot)| state.layout.desktop.get(**client).is_some() && assigned(&slot.app) == Some(Assign::All))
            .map(|(client, _)| *client)
            .collect();
        state.layout.every_desktop = every;
    }

    /// A Mac opens an app assigned to a desktop there: the shell goes to it
    /// first, as a Mac goes to the app's Space.
    #[cfg_attr(mobile_only, allow(dead_code))]
    pub(super) fn mac_open_on_assigned_desktop(&mut self, cx: &mut Cx, app: &str) {
        let Some(state) = self.state.as_ref() else { return };
        if state.style.target != desktop::DesktopStyle::Macos {
            return;
        }
        if let Some(Assign::Desktop(n)) = assigned(app) {
            if n != state.layout.active && n < layout::WORKSPACES {
                log!("wm: {app} opens on desktop {}, where it is assigned", n + 1);
                self.do_action(cx, WmAction::Workspace(n));
            }
        }
    }

    /// The bare desktop's menu, at the pointer: Finder's, less the rows that make,
    /// inspect and arrange the files a Mac's desktop holds, as OctoSense's holds none.
    /// The appearance, like the picture, is chosen in System Settings; the widgets
    /// live in Notification Center's column, where Edit Widgets… takes them.
    pub(super) fn open_desktop_menu(&mut self, cx: &mut Cx, at: Vec2d) {
        let rows = vec![MacRow::item("Change Wallpaper…", "mac.wallpaper"), MacRow::item("Edit Widgets…", "mac.edit-widgets")];
        self.mac_menu_takes_keyboard();
        if let Some(mut menu) = self.ui.widget(cx, ids!(shell_menu)).borrow_mut::<ShellMenu>() {
            menu.open_mac_at(cx, MacMenu { rows }, at);
        }
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    /// A status item's rows: how things stand, then the Mac's own settings for it. The Mac
    /// owns these, so a row changes something only where the shell really can.
    fn status_rows(&self, module: BarModule) -> Vec<MacRow> {
        let sample = &self.bar_sample;
        let (mut rows, settings) = match module {
            BarModule::TimeBox => {
                let boxes = approvals::time_boxes();
                let several = boxes.len() > 1;
                let mut rows = Vec::new();
                for (id, app, left) in boxes {
                    rows.push(MacRow::info(&format!("Approving everything {app} asks")));
                    rows.push(MacRow::info(&if left == 1 { "1 minute left".to_string() } else { format!("{left} minutes left") }));
                    rows.push(MacRow::item(&if several { format!("Stop {app}") } else { "Stop".to_string() }, &format!("mac.stop-time-box.{}", id.0)));
                    rows.push(MacRow::sep());
                }
                rows.push(MacRow::item("Approvals\u{2026}", shell::menu::APPROVALS_ROW));
                return rows;
            }
            BarModule::Desktops => {
                let current = self.state.as_ref().map_or(0, |state| state.layout.active);
                let mut rows = vec![MacRow::head("Desktops")];
                for &desktop in &self.bar_workspaces {
                    let row = MacRow::item(&format!("Desktop {}", desktop + 1), &format!("mac.desktop.{desktop}")).checked(desktop == current);
                    // The keymap reaches the first nine.
                    rows.push(if desktop < 9 { row.key(&format!("\u{2303}{}", desktop + 1)) } else { row });
                }
                return rows;
            }
            // Control Center: the network and Bluetooth in one module, Dark Mode, the
            // sound. A tile opens its module's details in place, or does what it says.
            BarModule::ControlCenter => {
                use shell::ui::Ico;
                fn words<'a>(on: Option<bool>, yes: &'a str, no: &'a str) -> &'a str {
                    match on { Some(true) => yes, Some(false) => no, None => "Unavailable" }
                }
                let (network, bluetooth) = (sample.network, sample.bluetooth);
                let mut rows = vec![
                    MacRow::tile("Network", "mac.cc.network", if network == Some(true) { Ico::WifiArcs } else { Ico::WifiArcsOff }, network == Some(true), words(network, "Connected", "Not Connected")).module(1),
                    MacRow::tile("Bluetooth", "mac.cc.bluetooth", if bluetooth == Some(true) { Ico::Bluetooth } else { Ico::BluetoothOff }, bluetooth == Some(true), words(bluetooth, "On", "Off")).module(1),
                ];
                if cfg!(not(mobile_only)) {
                    let dark = self.state.as_ref().is_some_and(|state| state.style.dark);
                    rows.push(MacRow::tile("Dark Mode", "mac.cc.dark", Ico::Moon, dark, if dark { "On" } else { "Off" }).module(2));
                }
                if let Some(volume) = sample.volume {
                    let level = if sample.muted { 0.0 } else { volume as f64 / 100.0 };
                    rows.push(MacRow::head("Sound").module(3));
                    rows.push(MacRow::slider("Volume", "mac.volume", level).module(3));
                }
                return rows;
            }
            BarModule::Network => {
                let words = match sample.network { Some(true) => "Connected", Some(false) => "Not Connected", None => "Status Unavailable" };
                (vec![MacRow::head("Network"), MacRow::info(words)], ("Network Settings\u{2026}", "network"))
            }
            BarModule::Bluetooth => {
                let words = match sample.bluetooth { Some(true) => "On", Some(false) => "Off", None => "Status Unavailable" };
                (vec![MacRow::head("Bluetooth"), MacRow::info(words)], ("Bluetooth Settings\u{2026}", "bluetooth"))
            }
            BarModule::Audio => {
                let level = if sample.muted { 0.0 } else { sample.volume.unwrap_or(0) as f64 / 100.0 };
                // A Mac's Sound menu: the slider, then where the sound goes, each device's
                // glyph in a round button, the one in use filled in the accent colour.
                let mut rows = vec![MacRow::head("Sound"), MacRow::slider("Volume", "mac.volume", level)];
                let outputs = mac_sound::outputs();
                if !outputs.is_empty() {
                    log!("wm: sound outputs: {}", outputs.iter().map(|o| if o.current { format!("{}*", o.name) } else { o.name.clone() }).collect::<Vec<_>>().join(", "));
                    rows.push(MacRow::sep());
                    // A section's name in the secondary ink, not a disabled row's.
                    rows.push(MacRow::info("Output"));
                    for output in outputs {
                        let glyph = match output.kind {
                            mac_sound::Kind::Speakers => shell::ui::Ico::Speaker,
                            mac_sound::Kind::Headphones => shell::ui::Ico::Headphone,
                            mac_sound::Kind::Display => shell::ui::Ico::Monitor,
                        };
                        rows.push(MacRow::device(&output.name, &format!("mac.sound-output.{}", output.id), glyph, output.current));
                    }
                }
                (rows, ("Sound Settings\u{2026}", "sound"))
            }
            BarModule::Power => {
                let Some(battery) = sample.battery else { return Vec::new() };
                let source = if battery.charging { "Power Source: Power Adapter" } else { "Power Source: Battery" };
                (vec![MacRow::head("Battery").key(&format!("{}%", battery.percent)), MacRow::info(source)], ("Battery Settings\u{2026}", "battery"))
            }
            _ => return Vec::new(),
        };
        if cfg!(target_os = "macos") {
            rows.push(MacRow::sep());
            rows.push(MacRow::item(settings.0, &format!("mac.host-settings.{}", settings.1)));
        }
        rows
    }

    /// A status item's menu, down from it; the bar's other status items are a move away.
    pub(super) fn open_status_menu(&mut self, cx: &mut Cx, module: BarModule) {
        let bar = self.shell_bar(cx);
        let mut items = Vec::new();
        for item in [BarModule::Desktops, BarModule::TimeBox, BarModule::Bluetooth, BarModule::Network, BarModule::Audio, BarModule::Power, BarModule::ControlCenter] {
            if let Some(place) = bar.borrow::<shell::bar::ShellBar>().and_then(|bar| bar.module_rect(item)) {
                items.push((item, place));
            }
        }
        let Some(index) = items.iter().position(|(item, _)| *item == module) else { return };
        let menus = items.iter().map(|(item, _)| MacMenu { rows: self.status_rows(*item) }).collect();
        let places = items.iter().map(|(_, place)| *place).collect();
        self.mac_status_items = items.into_iter().map(|(item, _)| item).collect();
        self.mac_menu_takes_keyboard();
        if let Some(mut menu) = self.ui.widget(cx, ids!(shell_menu)).borrow_mut::<ShellMenu>() {
            menu.open_mac_status(cx, menus, places, index);
        }
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    /// A Control Center module's details, where Control Center was: the network's or
    /// Bluetooth's menu. The Left arrow, or a press on the item, goes back to it.
    pub(super) fn open_control_center_detail(&mut self, cx: &mut Cx, module: BarModule) {
        let bar = self.shell_bar(cx);
        let Some(place) = bar.borrow::<shell::bar::ShellBar>().and_then(|bar| bar.module_rect(BarModule::ControlCenter)) else { return };
        // The module's rows, in Control Center's own box and width.
        let detail = self.status_rows(module).into_iter().map(|row| row.module(1)).collect();
        let menus = vec![MacMenu { rows: self.status_rows(BarModule::ControlCenter) }, MacMenu { rows: detail }];
        self.mac_status_items = vec![BarModule::ControlCenter, BarModule::ControlCenter];
        self.mac_menu_takes_keyboard();
        if let Some(mut menu) = self.ui.widget(cx, ids!(shell_menu)).borrow_mut::<ShellMenu>() {
            menu.open_mac_status(cx, menus, vec![place, place], 1);
        }
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    /// A status item's slider moved.
    pub(super) fn mac_menu_slide(&mut self, cx: &mut Cx, target: &str, level: f64) {
        if target == "mac.volume" {
            let level = (level * 100.0).round() as u32;
            self.hold_host_sound();
            self.bar_sample.volume = Some(level);
            self.bar_sample.muted = false;
            ask_host_sound(level, false);
            self.update_bar(cx);
        }
    }

    /// The windows on the desktop, oldest first: client, title, minimized.
    fn mac_windows(&mut self) -> Vec<(ClientId, String, bool)> {
        let state = self.state_mut();
        state
            .layout
            .desktop
            .windows
            .iter()
            .filter_map(|w| state.clients.get(&w.client).filter(|s| !s.warm && !s.pane && s.closing.is_none()).map(|s| (w.client, s.display_title().to_string(), w.minimized)))
            .collect()
    }

    /// Whether the app in front closes its own tabs with ⌘W.
    pub(super) fn focused_app_closes_tabs(&mut self) -> bool {
        let state = self.state_mut();
        let focus = state.layout.focused_client();
        focus.and_then(|c| state.clients.get(&c)).is_some_and(|s| clients::closes_own_tabs(&s.app))
    }

    /// Whether ⌘W is a row of the app's own now: a script app's Close Tab.
    pub(super) fn app_row_takes_close_key(&mut self, cx: &mut Cx) -> bool {
        self.mac_app_rows(cx).iter().any(row_takes_close_key)
    }

    fn mac_menus(&mut self, rows: &[AppRow], can_edit: bool, pressed: bool, field: makepad_platform::ime::HostedEdit, sheet_up: bool) -> Vec<MacMenu> {
        let windows = self.mac_windows();
        let recent = self.mac_recent_rows();
        let ask = self.focused_agent_app().map(|a| a.name);
        let state = self.state_mut();
        let focus = state.layout.focused_client();
        let slot = focus.and_then(|c| state.clients.get(&c));
        let app = slot.map(|s| s.app_name().to_string());
        let new_window = slot.and_then(|s| clients::find_app(&s.app)).is_some_and(|a| a.policy == LaunchPolicy::AlwaysNew);
        let tabs = slot.is_some_and(|s| clients::closes_own_tabs(&s.app));
        let have = focus.is_some();
        let fullscreen = focus.is_some_and(|c| state.layout.is_fullscreen(c));
        let tiled = focus.and_then(|c| state.layout.desktop.get(c)).is_some_and(|w| w.maximized || w.snap.is_some());
        let name = app.clone().unwrap_or_else(|| "OctoSense".to_string());
        let front_app = slot.map(|s| s.app.clone());
        let others_out = windows.iter().any(|w| !w.2 && state.clients.get(&w.0).map(|s| &s.app) != front_app.as_ref());
        let any_hidden = windows.iter().any(|w| state.layout.desktop.hidden(w.0));
        let available = |id: &str| clients::find_app(id).is_some();
        let launch = |label: &str, id: &str| MacRow::item(label, &format!("apps.{id}"));

        let mut system = vec![MacRow::item("About OctoSense", "mac.about"), MacRow::sep()];
        system.push(MacRow::item("System Settings…", "mac.settings"));
        if available("apphub") {
            system.push(launch("App Hub…", "apphub"));
        }
        if available("updater") {
            system.push(launch("Software Update…", "updater"));
        }
        system.push(MacRow::sep());
        system.push(MacRow::item("Recent Items", "").sub(recent));
        system.push(MacRow::sep());
        system.push(MacRow::item("Force Quit\u{2026}", "mac.force-quit").key("\u{2325}\u{2318}\u{238b}"));
        system.push(MacRow::sep());
        // The rest is the system's own, as a Mac's Apple menu is. The appearance
        // and the desktop style are in System Settings' Appearance (and the
        // desktop's menu), the assistant in Help and on F8, AI Providers and the
        // approvals in System Settings' Assistant.
        system.push(MacRow::item("Quit OctoSense…", "system.quit").key("\u{21e7}\u{2318}Q"));

        let mut app_menu = vec![MacRow::item(&format!("About {name}"), "mac.about-app"), MacRow::sep()];
        let own = rows_of(rows, "App");
        if !own.is_empty() {
            app_menu.extend(own);
            app_menu.push(MacRow::sep());
        }
        // A Mac's app menu always has Services; no OctoSense app offers one yet.
        app_menu.push(MacRow::item("Services", "").sub(vec![MacRow::item("No Services Apply", "").when(false)]));
        app_menu.push(MacRow::sep());
        if let Some(agent) = &ask {
            app_menu.push(MacRow::item(&format!("Ask {agent}…"), shell::menu::ASK_APP_ROW).key("\u{21e7}F8"));
            app_menu.push(MacRow::sep());
        }
        app_menu.push(MacRow::item(&format!("Hide {name}"), "mac.hide").key("\u{2318}H").when(have));
        app_menu.push(MacRow::item("Hide Others", "mac.hide-others").key("\u{2325}\u{2318}H").when(others_out));
        app_menu.push(MacRow::item("Show All", "mac.show-all").when(any_hidden));
        app_menu.push(MacRow::sep());
        if have {
            app_menu.push(MacRow::item(&format!("Quit {name}"), "mac.quit-app").key("\u{2318}Q"));
        } else {
            app_menu.push(MacRow::item("Quit OctoSense…", "system.quit").key("\u{21e7}\u{2318}Q"));
        }

        let mut file = rows_of(rows, "File");
        if new_window {
            // Terminal answers Command-N itself.
            let row = MacRow::item("New Window", "mac.new-window");
            file.push(if front_app.as_deref() == Some("terminal") { row.key("\u{2318}N") } else { row });
        }
        if !file.is_empty() {
            file.push(MacRow::sep());
        }
        if tabs {
            file.push(MacRow::item("Close Tab", "mac.edit.close-tab").key("\u{2318}W"));
            file.push(MacRow::item("Close Window", "mac.close").key("\u{21e7}\u{2318}W"));
        } else if rows.iter().any(row_takes_close_key) || (sheet_up && rows.iter().any(|row| row.key == "cmd+W")) {
            file.push(MacRow::item("Close Window", "mac.close").key("\u{21e7}\u{2318}W").when(!sheet_up));
        } else {
            file.push(MacRow::item("Close Window", "mac.close").key("\u{2318}W").when(have && !sheet_up));
        }
        let edits = have && can_edit;
        let can_paste = edits && cfg!(target_os = "macos");
        // A terminal's screen is its program's: it can be copied and pasted to, not undone or cut.
        let changes = edits && front_app.as_deref() != Some("terminal");
        // An app's own Undo, Cut or Select All stands in while there is no text to act on,
        // as over a view a press gave the keyboard to.
        let stands_in = |title: &str| rows.iter().find(|row| row.menu == "Edit" && row.title == title).filter(|_| !edits || pressed).map(app_row);
        // A field says what it can do now. An editor of an app's own says nothing, and its rows stay live.
        let says = |can: bool| can || !field.field;
        // A Mac's Paste and Match Style and Delete: an app's own row stands in for
        // either whenever it has one (Writer's).
        let own_row = |title: &str| rows.iter().find(|row| row.menu == "Edit" && row.title == title).map(app_row);
        let mut edit = vec![
            stands_in("Undo").unwrap_or_else(|| MacRow::item("Undo", "mac.edit.undo").key("\u{2318}Z").when(changes && says(field.can_undo))),
            stands_in("Redo").unwrap_or_else(|| MacRow::item("Redo", "mac.edit.redo").key("\u{21e7}\u{2318}Z").when(changes && says(field.can_redo))),
            MacRow::sep(),
            stands_in("Cut").unwrap_or_else(|| MacRow::item("Cut", "mac.edit.cut").key("\u{2318}X").when(changes && says(field.selected && field.writable))),
            stands_in("Copy").unwrap_or_else(|| MacRow::item("Copy", "mac.edit.copy").key("\u{2318}C").when(edits && says(field.selected))),
            stands_in("Paste").unwrap_or_else(|| MacRow::item("Paste", "mac.edit.paste").key("\u{2318}V").when(can_paste && says(field.writable))),
            // Pasted text is as plain as the field's own, which every field here is.
            own_row("Paste and Match Style").unwrap_or_else(|| {
                MacRow::item("Paste and Match Style", "mac.edit.paste").key("\u{2325}\u{21e7}\u{2318}V").when(can_paste && says(field.writable))
            }),
            own_row("Delete").unwrap_or_else(|| MacRow::item("Delete", "mac.edit.delete").when(changes && says(field.selected && field.writable))),
            stands_in("Select All").unwrap_or_else(|| MacRow::item("Select All", "mac.edit.select-all").key("\u{2318}A").when(edits && says(field.has_text))),
        ];
        let standard = ["Undo", "Redo", "Cut", "Copy", "Paste", "Paste and Match Style", "Delete", "Select All"];
        let own = edit_rows(rows.iter().filter(|row| row.menu == "Edit" && !standard.contains(&row.title.as_str())));
        if !own.is_empty() {
            edit.push(MacRow::sep());
            edit.extend(own);
        }
        // What a Mac adds to every Edit menu, where this Mac can do it: dictation, and
        // the Character Viewer, which types into the field as its keyboard does.
        if cfg!(target_os = "macos") {
            edit.push(MacRow::sep());
            if mac_can_dictate() {
                edit.push(MacRow::item("Start Dictation\u{2026}", "mac.dictation").when(can_paste && says(field.writable)));
            }
            edit.push(MacRow::item("Emoji & Symbols", "mac.emoji").key("\u{2303}\u{2318}Space"));
        }
        let mut view = rows_of(rows, "View");
        if !view.is_empty() {
            view.push(MacRow::sep());
        }
        let zooms = have && !front_app.as_deref().is_some_and(crate::apps::mac_window_fixed);
        view.push(MacRow::item(if fullscreen { "Exit Full Screen" } else { "Enter Full Screen" }, "mac.fullscreen").key("\u{2303}\u{2318}F").when(zooms));
        // macOS 15's tiling: Fill, and Move & Resize to a half or a quarter of the room above the Dock.
        let tile = |label: &str, zone: &str| MacRow::item(label, &format!("mac.tile.{zone}")).when(zooms);
        let mut window = vec![
            MacRow::item("Minimize", "mac.minimize").key("\u{2318}M").when(have),
            // A window that only widens zooms its width; it can't fill the screen or tile.
            MacRow::item("Zoom", "mac.zoom").when(zooms || (have && front_app.as_deref().is_some_and(crate::apps::mac_window_widens))),
            tile("Fill", "fill"),
            MacRow::item("Center", "mac.tile.center").when(have && !fullscreen),
            MacRow::item("Move & Resize", "")
                .sub(vec![
                    MacRow::head("Halves"),
                    tile("Left", "left"),
                    tile("Right", "right"),
                    tile("Top", "top"),
                    tile("Bottom", "bottom"),
                    MacRow::sep(),
                    MacRow::head("Quarters"),
                    tile("Top Left", "top-left"),
                    tile("Top Right", "top-right"),
                    tile("Bottom Left", "bottom-left"),
                    tile("Bottom Right", "bottom-right"),
                    MacRow::sep(),
                    MacRow::item("Return to Previous Size", "mac.tile.previous").when(zooms && tiled),
                ])
                .when(zooms),
            MacRow::sep(),
            MacRow::item("Next Window", "mac.next-window").key("\u{2318}`").when(windows.len() > 1),
            MacRow::item("Previous Window", "mac.previous-window").key("\u{21e7}\u{2318}`").when(windows.len() > 1),
            MacRow::sep(),
            MacRow::item("Bring All to Front", "mac.front").when(!windows.is_empty()),
        ];
        if !windows.is_empty() {
            window.push(MacRow::sep());
        }
        for (client, title, _) in &windows {
            window.push(MacRow::item(title, &format!("mac.window.{client}")).checked(Some(*client) == focus));
        }
        // A Mac's Help menu opens with a search field that finds the bar's rows.
        let mut help = vec![MacRow::search()];
        help.extend(rows_of(rows, "Help"));
        help.push(MacRow::item("Keyboard Shortcuts", "mac.keys").key("\u{2318}/"));
        help.push(MacRow::item("Ask OctoSense…", shell::menu::SYSTEM_CHAT_ROW).key("F8"));
        let mut menus = vec![system, app_menu, file, edit, view];
        menus.extend(own_menus(rows).iter().map(|menu| rows_of(rows, menu)));
        menus.push(window);
        menus.push(help);
        menus.into_iter().map(|rows| MacMenu { rows }).collect()
    }

    /// A Mac menu row or shortcut (`mac.*`). True when the target was one.
    pub(super) fn mac_menu_activate(&mut self, cx: &mut Cx, target: &str) -> bool {
        let Some(what) = target.strip_prefix("mac.") else { return false };
        self.close_shell_menu(cx);
        let focus = self.state_mut().layout.focused_client();
        let focused_app = focus.and_then(|c| self.state_mut().clients.get(&c).map(|s| s.app.clone()));
        match what {
            "about" => {
                let lines = vec![
                    format!("Version {}", env!("CARGO_PKG_VERSION")),
                    "Your apps and agents, on your desktop and your phone.".to_string(),
                ];
                self.mac_open_about(cx, MacAbout::new("", "OctoSense", lines).foot(COPYRIGHT));
            }
            "about-app" => match focus.and_then(|c| self.state_mut().clients.get(&c).map(|s| (s.app.clone(), s.app_name().to_string()))) {
                Some((app, name)) => {
                    let installed = clients::find_app(&app).is_some_and(|a| apps::card_manifest_id(&a).is_some_and(|id| !id.starts_with("os.")));
                    let lines = if installed {
                        vec!["Installed from App Hub.".to_string()]
                    } else {
                        // It ships inside the shell, at the shell's version.
                        vec![format!("Version {}", env!("CARGO_PKG_VERSION")), "Comes with OctoSense.".to_string()]
                    };
                    // An App Hub app's holder isn't known here: no © line rather than a wrong one.
                    let about = MacAbout::new(&app, &name, lines);
                    self.mac_open_about(cx, if installed { about } else { about.foot(COPYRIGHT) });
                }
                None => return self.mac_menu_activate(cx, "mac.about"),
            },
            "close" => {
                if let Some(client) = focus {
                    self.request_close(cx, client);
                }
            }
            "quit-app" => {
                if let Some(app) = focused_app {
                    for client in self.mac_windows_of(&app) {
                        self.send_wm_event(client, WmEvent::Quitting);
                        self.request_close(cx, client);
                    }
                }
            }
            "new-window" => {
                if let Some(app) = focused_app {
                    self.launch_app(cx, &app);
                }
            }
            "minimize" => {
                if let Some(client) = focus {
                    self.minimize_desktop(cx, client);
                }
            }
            "zoom" => {
                if let Some(client) = focus {
                    self.maximize_desktop(cx, client);
                }
            }
            "fullscreen" => {
                if let Some(client) = focus {
                    self.fullscreen_desktop(cx, client);
                }
            }
            // Center: the window as it is, in the middle of the room above the Dock. A
            // window that doesn't zoom moves too; a tiled one keeps its own size.
            "tile.center" => {
                if let Some(client) = focus {
                    let area = self.desk_area(cx);
                    let room = crate::desktop_layout::room(area, self.dock_overlay(cx, area));
                    if let Some(w) = self.state_mut().layout.desktop.get_mut(client) {
                        w.maximized = false;
                        w.snap = None;
                        w.rect = room.centered(w.rect.w, w.rect.h);
                    }
                    self.focus_client(cx, client);
                    self.redraw_all(cx);
                }
            }
            tile if tile.starts_with("tile.") => {
                use crate::snap::Zone;
                let fixed = focused_app.as_deref().is_some_and(crate::apps::mac_window_fixed);
                let Some(client) = focus.filter(|_| !fixed) else { return true };
                let zone = match &tile["tile.".len()..] {
                    "fill" => Zone::Maximize,
                    "left" => Zone::Left,
                    "right" => Zone::Right,
                    "top" => Zone::Top,
                    "bottom" => Zone::Bottom,
                    "top-left" => Zone::TopLeft,
                    "top-right" => Zone::TopRight,
                    "bottom-left" => Zone::BottomLeft,
                    "bottom-right" => Zone::BottomRight,
                    // Return to Previous Size: the window's own rect, kept under the tile.
                    _ => {
                        if let Some(w) = self.state_mut().layout.desktop.get_mut(client) {
                            w.maximized = false;
                            w.snap = None;
                        }
                        self.focus_client(cx, client);
                        self.redraw_all(cx);
                        return true;
                    }
                };
                self.apply_snap(cx, client, zone);
            }
            "hide" => {
                if let Some(app) = focused_app {
                    for client in self.mac_windows_of(&app) {
                        self.hide_desktop(cx, client);
                    }
                }
            }
            "hide-others" => {
                let keep = focused_app.as_deref().map(|app| self.mac_windows_of(app)).unwrap_or_default();
                for (client, _, _) in self.mac_windows() {
                    if !keep.contains(&client) {
                        self.hide_desktop(cx, client);
                    }
                }
                if let Some(client) = focus {
                    self.focus_client(cx, client);
                }
            }
            "show-all" => {
                for (client, _, _) in self.mac_windows() {
                    if self.state_mut().layout.desktop.hidden(client) {
                        self.restore_desktop(cx, client);
                    }
                }
                if let Some(client) = focus {
                    self.restore_desktop(cx, client);
                }
            }
            "front" => {
                for (client, _, minimized) in self.mac_windows() {
                    if !minimized {
                        self.state_mut().layout.raise_float(client);
                    }
                }
                if let Some(client) = focus {
                    self.restore_desktop(cx, client);
                }
            }
            "next-window" => self.do_action(cx, WmAction::CycleFocus(true)),
            "previous-window" => self.do_action(cx, WmAction::CycleFocus(false)),
            "desktop-next" => self.do_action(cx, WmAction::WorkspaceNext),
            "desktop-previous" => self.do_action(cx, WmAction::WorkspacePrev),
            "spotlight" => self.open_shell_menu(cx, "", MenuSkin::Menu),
            "emoji" => mac_character_palette(),
            "dictation" => mac_start_dictation(),
            "cc.network" => self.open_control_center_detail(cx, BarModule::Network),
            "cc.bluetooth" => self.open_control_center_detail(cx, BarModule::Bluetooth),
            // Dark Mode's tile turns the appearance over and leaves Control Center up,
            // as a Mac's does.
            #[cfg(not(mobile_only))]
            "cc.dark" => {
                let choice = if self.state_mut().style.dark { mac_appearance::Choice::Light } else { mac_appearance::Choice::Dark };
                self.choose_mac_appearance(cx, choice);
                self.open_status_menu(cx, BarModule::ControlCenter);
            }
            "launchpad" => self.toggle_launcher(cx),
            "keys" => self.open_settings_pane(cx, "keyboard"),
            "wallpaper" => self.open_settings_pane(cx, "appearance"),
            "edit-widgets" => {
                self.set_glance_open(cx, true);
                if let Some(mut panel) = self.ui.widget(cx, ids!(shell_glance)).borrow_mut::<glance_panel::ShellGlancePanel>() {
                    panel.edit_widgets(cx);
                }
            }
            "about-more" => self.open_settings_pane(cx, "general"),
            "settings" => self.open_settings_pane(cx, ""),
            #[cfg(not(mobile_only))]
            "appearance.flip" => {
                let choice = if self.state_mut().style.dark { mac_appearance::Choice::Light } else { mac_appearance::Choice::Dark };
                self.choose_mac_appearance(cx, choice);
            }
            #[cfg(not(mobile_only))]
            "appearance.auto" => self.choose_mac_appearance(cx, mac_appearance::Choice::Auto),
            #[cfg(not(mobile_only))]
            "appearance.light" => self.choose_mac_appearance(cx, mac_appearance::Choice::Light),
            #[cfg(not(mobile_only))]
            "appearance.dark" => self.choose_mac_appearance(cx, mac_appearance::Choice::Dark),
            "force-quit" => self.mac_open_force_quit(cx),
            forced if forced.starts_with("force-quit-app.") => {
                let app = forced["force-quit-app.".len()..].to_string();
                self.mac_force_quit(cx, &app);
            }
            "recent-clear" => {
                self.recent_apps.clear();
                save_recent_apps(&self.recent_apps);
                self.recent_docs.clear();
                save_recent(&recent_docs_path(), &self.recent_docs);
            }
            doc if doc.starts_with("recent-doc.") => {
                let path = doc["recent-doc.".len()..].parse::<usize>().ok().and_then(|n| self.recent_docs.get(n).cloned());
                if let Some(open) = path.and_then(|path| crate::preview::OpenRequest::from_request(&WmRequest::Open { app: None, path })) {
                    self.open_request(cx, None, open);
                }
            }
            #[cfg(not(mobile_only))]
            picture if picture.starts_with("picture.") => {
                if let Some(choice) = mac_appearance::Picture::parse(&picture["picture.".len()..]) {
                    self.choose_mac_picture(cx, choice);
                }
            }
            other => {
                if let Some(n) = other.strip_prefix("desktop.").and_then(|n| n.parse::<usize>().ok()) {
                    self.do_action(cx, WmAction::Workspace(n));
                } else if let Some(client) = other.strip_prefix("window.").and_then(|n| n.parse::<ClientId>().ok()) {
                    self.restore_desktop(cx, client);
                } else if let Some(command) = other.strip_prefix("edit.") {
                    self.mac_edit(cx, command);
                } else if other.starts_with("app.") {
                    self.mac_edit(cx, other);
                } else if let Some(app) = other.strip_prefix("dock-open.") {
                    self.launch_app(cx, app);
                } else if let Some(app) = other.strip_prefix("dock-keep.") {
                    let pins = &mut self.state_mut().dock_pins;
                    if !keep_in_dock(pins, app) {
                        pins.retain(|id| id != app);
                    }
                    log!("wm: dock keeps {}", pins.join(","));
                    save_dock_pins(pins);
                } else if let Some(app) = other.strip_prefix("dock-expose.") {
                    self.open_expose(cx, app);
                } else if let Some(app) = other.strip_prefix("dock-login.") {
                    if !keep_in_dock(&mut self.login_apps, app) {
                        self.login_apps.retain(|id| id != app);
                    }
                    log!("wm: opens at login: {}", self.login_apps.join(","));
                    save_login_apps(&self.login_apps);
                } else if let Some(app) = other.strip_prefix("dock-hide.") {
                    for client in self.mac_windows_of(app) {
                        self.minimize_desktop(cx, client);
                    }
                } else if let Some(app) = other.strip_prefix("dock-quit.") {
                    for client in self.mac_windows_of(app) {
                        self.request_close(cx, client);
                    }
                } else if let Some(client) = other.strip_prefix("dock-close.").and_then(|n| n.parse::<ClientId>().ok()) {
                    self.request_close(cx, client);
                } else if other == "dock-assistant" {
                    self.activate_shelf(cx, desktop::ShelfHit::Assistant);
                } else if let Some((how, app)) = other.strip_prefix("dock-assign.").and_then(|rest| rest.split_once('.')) {
                    self.mac_assign_desktop(app, how);
                } else if other == "dock-trash" {
                    self.activate_shelf(cx, desktop::ShelfHit::Trash);
                } else if other == "dock-empty-trash" {
                    self.empty_trash(cx);
                } else if let Some(id) = other.strip_prefix("stop-time-box.") {
                    approvals::stop_time_box(id);
                    self.approvals_changed(cx);
                } else if other == "mute" {
                    self.hold_host_sound();
                    self.bar_sample.muted = !self.bar_sample.muted;
                    ask_host_sound(self.bar_sample.volume.unwrap_or(0), self.bar_sample.muted);
                    self.update_bar(cx);
                } else if let Some(id) = other.strip_prefix("sound-output.").and_then(|id| id.parse::<u32>().ok()) {
                    let chosen = mac_sound::choose_output(id);
                    log!("wm: sound output {id} chosen: {chosen}");
                } else if let Some(pane) = other.strip_prefix("host-settings.") {
                    open_host_settings(pane);
                } else {
                    log!("wm: no Mac menu row '{other}'");
                }
            }
        }
        self.update_bar(cx);
        self.redraw_all(cx);
        true
    }

    fn mac_open_about(&mut self, cx: &mut Cx, about: MacAbout) {
        if let Some(mut menu) = self.ui.widget(cx, ids!(shell_menu)).borrow_mut::<ShellMenu>() {
            menu.open_about(cx, about);
        }
    }

    fn mac_windows_of(&mut self, app: &str) -> Vec<ClientId> {
        let state = self.state_mut();
        state
            .layout
            .desktop
            .windows
            .iter()
            .filter(|w| state.clients.get(&w.client).is_some_and(|s| s.app == app && !s.warm && !s.pane && s.closing.is_none()))
            .map(|w| w.client)
            .collect()
    }

    /// Force Quit Applications: each app with a window, in the order its first
    /// one opened, with the app in front chosen.
    fn mac_open_force_quit(&mut self, cx: &mut Cx) {
        let windows = self.mac_windows();
        let state = self.state_mut();
        let front = state.layout.focused_client().and_then(|c| state.clients.get(&c)).map(|s| s.app.clone());
        let mut apps: Vec<(String, String)> = Vec::new();
        for (client, _, _) in windows {
            if let Some(slot) = state.clients.get(&client).filter(|s| !apps.iter().any(|(id, _)| *id == s.app)) {
                apps.push((slot.app.clone(), slot.app_name().to_string()));
            }
        }
        let chosen = front.and_then(|front| apps.iter().position(|(id, _)| *id == front));
        if let Some(mut menu) = self.ui.widget(cx, ids!(shell_menu)).borrow_mut::<ShellMenu>() {
            menu.open_force_quit(cx, MacForceQuit::new(apps, chosen));
        }
    }

    /// Force Quit: every window of `app` ends now, whatever it would ask first.
    /// A process app's process is ended; a module's instance, one of its extra
    /// windows or a stopped app's tile goes as a closed one does.
    fn mac_force_quit(&mut self, cx: &mut Cx, app: &str) {
        log!("wm: force quitting {app}");
        for client in self.mac_windows_of(app) {
            let stopped = self.state_mut().clients.get(&client).is_some_and(|s| s.stopped);
            if stopped || self.module_host.is_module(client) || self.module_windows.contains_key(&client) {
                self.remove_client(cx, client);
            } else {
                self.end_process_client(cx, client);
            }
        }
    }

    /// Recent Items: the apps opened last, newest first, as a Mac lists them
    /// under Applications, then the documents under Documents, each with the icon
    /// of the app that opens it, then Servers (the shell connects to none), and
    /// Clear Menu.
    fn mac_recent_rows(&self) -> Vec<MacRow> {
        let mut rows = vec![MacRow::info("Applications")];
        rows.extend(self.recent_apps.iter().filter_map(|id| clients::find_app(id)
            .map(|app| MacRow::item(&app.label, &format!("apps.{id}")).app_icon(id))));
        rows.push(MacRow::sep());
        rows.push(MacRow::info("Documents"));
        rows.extend(self.recent_docs.iter().enumerate().map(|(n, path)| {
            let path = std::path::Path::new(path);
            let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            MacRow::item(&name, &format!("mac.recent-doc.{n}")).app_icon(makepad_wm_api::viewer_for(path))
        }));
        rows.push(MacRow::sep());
        rows.push(MacRow::info("Servers"));
        rows.push(MacRow::sep());
        let any = !self.recent_apps.is_empty() || !self.recent_docs.is_empty();
        rows.push(MacRow::item("Clear Menu", "mac.recent-clear").when(any));
        rows
    }

    /// A person opened the document at `path`: Recent Items lists it first.
    #[cfg_attr(mobile_only, allow(dead_code))]
    pub(super) fn note_recent_doc(&mut self, path: &std::path::Path) {
        let Some(path) = path.to_str().filter(|path| !path.is_empty() && !path.contains('\n')) else {
            return;
        };
        if remember_app(&mut self.recent_docs, path) {
            save_recent(&recent_docs_path(), &self.recent_docs);
        }
    }

    /// A person opened `id`: Recent Items lists it first.
    #[cfg_attr(mobile_only, allow(dead_code))]
    pub(super) fn note_recent_app(&mut self, id: &str) {
        if remember_app(&mut self.recent_apps, id) {
            save_recent_apps(&self.recent_apps);
        }
    }

    /// The menu holds the keyboard until it closes, so the press that opened it
    /// doesn't hand it back to the window on its release.
    fn mac_menu_takes_keyboard(&mut self) {
        self.mac_menu_focus = self.mac_press_focus;
        self.mac_focus_after_up = false;
        self.mac_refocus = None;
    }

    /// Hand the keyboard back to what held it when the menu opened.
    pub(super) fn mac_restore_focus(&mut self, cx: &mut Cx) {
        if !std::mem::take(&mut self.mac_menu_focus) {
            return;
        }
        if self.mac_pressed {
            self.mac_focus_after_up = true;
        } else {
            self.mac_refocus = Some(cx.start_timeout(0.001));
        }
    }

    /// The key focus changes between events, so a command from the menu runs
    /// once the keyboard is back.
    fn mac_edit(&mut self, cx: &mut Cx, command: &str) {
        self.mac_edit_pending = Some(command.to_string());
        // A row chosen with the pointer gives the keyboard back on the release.
        if self.mac_refocus.is_none() && !self.mac_focus_after_up {
            self.mac_edit_later = Some(cx.start_timeout(0.001));
        }
    }

    /// The Edit menu: what the focused window would get from the same key.
    pub(super) fn mac_edit_now(&mut self, cx: &mut Cx, command: &str) {
        let shortcut = |key_code: KeyCode, shift: bool| {
            let modifiers = KeyModifiers { logo: cfg!(target_os = "macos"), control: !cfg!(target_os = "macos"), shift, alt: false };
            KeyEvent { key_code, modifiers, ..Default::default() }
        };
        if let Some(own) = command.strip_prefix("app.") {
            self.mac_app_command(cx, own);
            return;
        }
        match command {
            "undo" => self.mac_press(cx, shortcut(KeyCode::KeyZ, false)),
            "redo" => self.mac_press(cx, shortcut(KeyCode::KeyZ, true)),
            "select-all" => self.mac_press(cx, shortcut(KeyCode::KeyA, false)),
            // What the Delete key does: the selected text goes, or the app's selection.
            "delete" => self.mac_press(cx, KeyEvent { key_code: KeyCode::Backspace, ..Default::default() }),
            "close-tab" => self.mac_press(cx, shortcut(KeyCode::KeyW, false)),
            "copy" | "cut" => {
                let response = Rc::new(RefCell::new(None));
                let clip = TextClipboardEvent { response: response.clone() };
                let event = if command == "copy" { Event::TextCopy(clip) } else { Event::TextCut(clip) };
                if !self.chat_clipboard(cx, &event) {
                    self.mac_dispatch(cx, &event);
                }
                let text = response.borrow().clone();
                if let Some(text) = text {
                    cx.copy_to_clipboard(&text);
                }
            }
            "paste" => {
                #[cfg(target_os = "macos")]
                if let Ok(output) = std::process::Command::new("/usr/bin/pbpaste").output() {
                    let input = String::from_utf8_lossy(&output.stdout).into_owned();
                    if !input.is_empty() {
                        self.mac_dispatch(cx, &Event::TextInput(TextInputEvent { input, was_paste: true, replace_last: false, ..Default::default() }));
                    }
                }
            }
            other => log!("wm: no Edit command '{other}'"),
        }
    }

    /// A key from a menu row, down and up: a process app with only the press would go on
    /// holding its modifiers, and read the next letters as commands.
    fn mac_press(&mut self, cx: &mut Cx, key: KeyEvent) {
        self.mac_dispatch(cx, &Event::KeyDown(key));
        self.mac_dispatch(cx, &Event::KeyUp(key));
    }

    fn mac_dispatch(&mut self, cx: &mut Cx, event: &Event) {
        if let Some(state) = self.state.as_mut() {
            let mut scope = Scope::with_data(state);
            self.ui.handle_event(cx, event, &mut scope);
        }
    }
}

/// `id` opened: it leads the list, once, and the list keeps the newest
/// [`RECENT_APPS`]. False when it already led.
fn remember_app(list: &mut Vec<String>, id: &str) -> bool {
    if list.first().is_some_and(|first| first == id) {
        return false;
    }
    list.retain(|other| other != id);
    list.insert(0, id.to_string());
    list.truncate(RECENT_APPS);
    true
}

fn recent_apps_path() -> std::path::PathBuf {
    octosense::paths::home().join("recent-apps")
}

/// Keep `id` in the Dock, after the apps already kept there. False when it
/// already was.
fn keep_in_dock(pins: &mut Vec<String>, id: &str) -> bool {
    if pins.iter().any(|kept| kept == id) {
        return false;
    }
    pins.push(id.to_string());
    true
}

fn dock_pins_path() -> std::path::PathBuf {
    octosense::paths::home().join("dock-apps")
}

/// The apps the person keeps in a Mac's Dock, an app id a line, in order;
/// a new Mac's until they keep or remove one.
pub(super) fn load_dock_pins() -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(dock_pins_path()) else {
        return desktop::MAC_DOCK_PINNED.iter().map(|id| id.to_string()).collect();
    };
    let mut pins = Vec::new();
    for id in text.lines().map(str::trim).filter(|id| !id.is_empty()) {
        keep_in_dock(&mut pins, id);
    }
    pins
}

fn save_dock_pins(pins: &[String]) {
    if let Err(error) = std::fs::write(dock_pins_path(), pins.join("\n") + "\n") {
        log!("wm: could not keep the Dock: {error}");
    }
}

fn login_apps_path() -> std::path::PathBuf {
    octosense::paths::home().join("login-apps")
}

/// The apps the shell opens when it starts (a Dock menu's Options ▸ Open at
/// Login), an app id a line, in the order they were chosen; none at first.
#[cfg_attr(mobile_only, allow(dead_code))]
pub(super) fn load_login_apps() -> Vec<String> {
    let text = std::fs::read_to_string(login_apps_path()).unwrap_or_default();
    let mut apps = Vec::new();
    for id in text.lines().map(str::trim).filter(|id| !id.is_empty()) {
        // Each once, in the order first written.
        keep_in_dock(&mut apps, id);
    }
    apps
}

fn save_login_apps(apps: &[String]) {
    let text = if apps.is_empty() { String::new() } else { apps.join("\n") + "\n" };
    if let Err(error) = std::fs::write(login_apps_path(), text) {
        log!("wm: could not keep the apps that open at login: {error}");
    }
}

/// Where a Mac's Dock ▸ Options ▸ Assign To puts an app's windows: on every
/// desktop, or on one (from 0, as the layout counts them). An app with
/// neither opens on the desktop in front.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Assign {
    All,
    Desktop(usize),
}

/// The Assign To section of a Dock menu's Options, as a Mac's: its title,
/// then All Desktops, This Desktop and None, one ticked. An app assigned to
/// another desktop shows that one ticked, above This Desktop.
fn assign_rows(app: &str, assigned: Option<Assign>, current: usize) -> Vec<MacRow> {
    let row = |label: &str, how: &str, on: bool| MacRow::item(label, &format!("mac.dock-assign.{how}.{app}")).checked(on);
    let mut rows = vec![MacRow::item("Assign To", "").when(false), row("All Desktops", "all", assigned == Some(Assign::All))];
    if let Some(Assign::Desktop(n)) = assigned.filter(|a| *a != Assign::Desktop(current) && *a != Assign::All) {
        rows.push(row(&format!("Desktop {}", n + 1), &n.to_string(), true));
    }
    rows.push(row("This Desktop", "this", assigned == Some(Assign::Desktop(current))));
    rows.push(row("None", "none", assigned.is_none()));
    rows
}

fn dock_desktops_path() -> std::path::PathBuf {
    octosense::paths::home().join("dock-desktops")
}

/// The apps assigned a desktop, a line each: the app's id, a tab, then
/// `all` or the desktop's number from 1. Lines that say neither are dropped.
fn parse_assignments(text: &str) -> Vec<(String, Assign)> {
    let mut out: Vec<(String, Assign)> = Vec::new();
    for line in text.lines() {
        let Some((app, how)) = line.split_once('\t') else { continue };
        let assign = match how.trim() {
            "all" => Assign::All,
            n => match n.parse::<usize>() {
                Ok(n) if (1..=layout::WORKSPACES).contains(&n) => Assign::Desktop(n - 1),
                _ => continue,
            },
        };
        let app = app.trim();
        if !app.is_empty() {
            out.retain(|(a, _)| a != app);
            out.push((app.to_string(), assign));
        }
    }
    out
}

fn format_assignments(list: &[(String, Assign)]) -> String {
    list.iter()
        .map(|(app, assign)| match assign {
            Assign::All => format!("{app}\tall\n"),
            Assign::Desktop(n) => format!("{app}\t{}\n", n + 1),
        })
        .collect()
}

thread_local! {
    static ASSIGNED: RefCell<Option<Vec<(String, Assign)>>> = const { RefCell::new(None) };
}

fn with_assignments<R>(f: impl FnOnce(&mut Vec<(String, Assign)>) -> R) -> R {
    ASSIGNED.with(|cell| {
        let mut cell = cell.borrow_mut();
        let list = cell.get_or_insert_with(|| parse_assignments(&std::fs::read_to_string(dock_desktops_path()).unwrap_or_default()));
        f(list)
    })
}

/// Where `app` is assigned, if anywhere.
pub(super) fn assigned(app: &str) -> Option<Assign> {
    with_assignments(|list| list.iter().find(|(a, _)| a == app).map(|(_, assign)| *assign))
}

fn set_assigned(app: &str, assign: Option<Assign>) {
    let text = with_assignments(|list| {
        list.retain(|(a, _)| a != app);
        if let Some(assign) = assign {
            list.push((app.to_string(), assign));
        }
        format_assignments(list)
    });
    if let Err(error) = std::fs::write(dock_desktops_path(), text) {
        log!("wm: could not keep where apps are assigned: {error}");
    }
}

/// Recent Items as the person left them: an app id a line, newest first.
#[cfg_attr(mobile_only, allow(dead_code))]
pub(super) fn load_recent_apps() -> Vec<String> {
    let text = std::fs::read_to_string(recent_apps_path()).unwrap_or_default();
    let mut list = Vec::new();
    for id in text.lines().map(str::trim).filter(|id| !id.is_empty()).rev() {
        remember_app(&mut list, id);
    }
    list
}

fn save_recent_apps(list: &[String]) {
    save_recent(&recent_apps_path(), list);
}

/// Recent Items' documents as the person left them: a path a line, newest first.
#[cfg_attr(mobile_only, allow(dead_code))]
pub(super) fn load_recent_docs() -> Vec<String> {
    let text = std::fs::read_to_string(recent_docs_path()).unwrap_or_default();
    let mut list = Vec::new();
    for path in text.lines().filter(|path| !path.trim().is_empty()).rev() {
        remember_app(&mut list, path);
    }
    list
}

fn save_recent(file: &std::path::Path, list: &[String]) {
    if let Err(error) = std::fs::write(file, list.join("\n") + "\n") {
        log!("wm: could not keep Recent Items: {error}");
    }
}

fn recent_docs_path() -> std::path::PathBuf {
    octosense::paths::home().join("recent-documents")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_items_keep_the_newest_ten_once_each() {
        let mut list = Vec::new();
        assert!(remember_app(&mut list, "mail"));
        assert!(remember_app(&mut list, "notes"));
        assert!(!remember_app(&mut list, "notes"));
        assert!(remember_app(&mut list, "mail"));
        assert_eq!(list, ["mail", "notes"]);
        for i in 0..12 {
            remember_app(&mut list, &format!("app{i}"));
        }
        assert_eq!(list.len(), RECENT_APPS);
        assert_eq!(list[0], "app11");
        assert!(!list.iter().any(|id| id == "mail"));
    }

    #[test]
    fn assign_to_reads_and_writes_a_line_an_app_and_ticks_one_row() {
        let list = parse_assignments("notes\tall\ncalendar\t2\nbad\t0\nworse\nmail\t99\nnotes\t1\n");
        assert_eq!(list, [("calendar".to_string(), Assign::Desktop(1)), ("notes".to_string(), Assign::Desktop(0))], "the last line for an app wins; nonsense is dropped");
        assert_eq!(parse_assignments(&format_assignments(&list)), list);
        let labels = |rows: Vec<MacRow>| rows.iter().map(|r| format!("{}{}", if r.checked { "+" } else { "" }, r.label)).collect::<Vec<_>>();
        assert_eq!(labels(assign_rows("notes", None, 0)), ["Assign To", "All Desktops", "This Desktop", "+None"]);
        assert_eq!(labels(assign_rows("notes", Some(Assign::Desktop(0)), 0)), ["Assign To", "All Desktops", "+This Desktop", "None"]);
        assert_eq!(labels(assign_rows("notes", Some(Assign::Desktop(1)), 0)), ["Assign To", "All Desktops", "+Desktop 2", "This Desktop", "None"]);
        assert_eq!(labels(assign_rows("notes", Some(Assign::All), 1)), ["Assign To", "+All Desktops", "This Desktop", "None"]);
        let rows = assign_rows("os.news", Some(Assign::Desktop(1)), 0);
        assert!(!rows[0].enabled, "the section's title is not a row to press");
        assert_eq!(rows[2].target, "mac.dock-assign.1.os.news");
    }

    #[test]
    fn the_dock_keeps_an_app_once_after_the_others() {
        let mut pins = vec!["files".to_string(), "notes".to_string()];
        assert!(keep_in_dock(&mut pins, "calculator"));
        assert!(!keep_in_dock(&mut pins, "notes"));
        assert_eq!(pins, ["files", "notes", "calculator"]);
    }

    #[test]
    fn a_rows_key_reads_in_a_macs_glyphs_and_order() {
        assert_eq!(key_glyphs("cmd+N"), "\u{2318}N");
        assert_eq!(key_glyphs("shift+cmd+R"), "\u{21e7}\u{2318}R");
        assert_eq!(key_glyphs("cmd+shift+ctrl+f"), "\u{2303}\u{21e7}\u{2318}F");
        assert_eq!(key_glyphs("cmd+,"), "\u{2318},");
        // Mail's Format > Quote Level, as a Mac's Mail has it.
        assert_eq!(key_glyphs("cmd+'"), "\u{2318}'");
        assert_eq!(key_glyphs("alt+cmd+'"), "\u{2325}\u{2318}'");
        assert_eq!(key_glyphs("delete"), "\u{232b}");
        assert_eq!(key_glyphs(""), "");
    }

    #[test]
    fn a_rows_key_is_only_that_key() {
        let press = |key_code, logo, shift| KeyEvent { key_code, modifiers: KeyModifiers { logo, shift, ..Default::default() }, ..Default::default() };
        assert!(key_matches("cmd+N", &press(KeyCode::KeyN, true, false)));
        assert!(!key_matches("cmd+N", &press(KeyCode::KeyN, true, true)));
        assert!(!key_matches("cmd+N", &press(KeyCode::KeyN, false, false)));
        assert!(key_matches("shift+cmd+R", &press(KeyCode::KeyR, true, true)));
        assert!(key_matches("cmd+,", &press(KeyCode::Comma, true, false)));
        assert!(key_matches("cmd+'", &press(KeyCode::Quote, true, false)));
        let alt_quote = KeyEvent { key_code: KeyCode::Quote, modifiers: KeyModifiers { logo: true, alt: true, ..Default::default() }, ..Default::default() };
        assert!(key_matches("alt+cmd+'", &alt_quote));
        assert!(!key_matches("cmd+'", &alt_quote));
        assert!(!key_matches("bad+N", &press(KeyCode::KeyN, true, false)));
        assert!(!key_matches("", &press(KeyCode::KeyN, true, false)));
    }

    #[test]
    fn an_apps_answer_is_read_with_bounds() {
        let rows = parse_rows(r#"[{"menu":"File","title":"New Event","key":"cmd+N","command":"new"},{"menu":"File"},{"title":"No menu"},{"menu":"Message","title":"Reply","enabled":false}]"#);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].command, "new");
        assert!(app_row(&rows[1]).separator);
        assert_eq!(rows[2].enabled, Some(false));
        assert_eq!(own_menus(&rows), ["Message"]);
        assert!(parse_rows("not json").is_empty());
        let long = format!(r#"[{{"menu":"File","title":"{}"}}]"#, "x".repeat(200));
        assert_eq!(parse_rows(&long)[0].title.chars().count(), MAX_TITLE_CHARS);
        let many = format!("[{}]", vec![r#"{"menu":"File","title":"x"}"#; 200].join(","));
        assert_eq!(parse_rows(&many).len(), MAX_ROWS);
    }

    /// Find and its kin go in a Find submenu, the case rows in Transformations,
    /// each where its first row was, with no rule left doubled or at an end.
    #[test]
    fn edit_rows_nest_find_and_transformations() {
        let rows = parse_rows(
            r#"[{"menu":"Edit","title":"Delete"},{"menu":"Edit"},{"menu":"Edit","title":"Find…","key":"cmd+F"},{"menu":"Edit","title":"Find Next","key":"cmd+G"},
            {"menu":"Edit"},{"menu":"Edit","title":"Make Upper Case"},{"menu":"Edit","title":"Capitalize"},{"menu":"Edit"}]"#,
        );
        let edit = edit_rows(rows.iter());
        let labels: Vec<&str> = edit.iter().map(|row| if row.separator { "-" } else { row.label.as_str() }).collect();
        assert_eq!(labels, ["Delete", "-", "Find", "-", "Transformations"]);
        let find: Vec<&str> = edit[2].sub.iter().map(|row| row.label.as_str()).collect();
        assert_eq!(find, ["Find\u{2026}", "Find and Replace\u{2026}", "Find Next", "Find Previous", "Use Selection for Find", "Jump to Selection"]);
        let live: Vec<bool> = edit[2].sub.iter().map(|row| row.enabled).collect();
        assert_eq!(live, [true, false, true, false, false, false], "what the app doesn't answer is greyed");
        assert_eq!(edit[2].sub[0].shortcut, "\u{2318}F", "its key goes with it");
        assert_eq!(edit[2].sub[3].shortcut, key_glyphs("shift+cmd+G"), "a grey row shows its key");
        assert_eq!(edit[4].sub.len(), 2);
        let settings = parse_rows(r#"[{"menu":"Edit","title":"Find","key":"cmd+F","command":"find"}]"#);
        let edit = edit_rows(settings.iter());
        assert_eq!(edit.len(), 1);
        assert_eq!(edit[0].sub[0].label, "Find\u{2026}", "a lone Find opens the find field");
        let terminal = native_rows("terminal");
        assert!(edit_rows(terminal.iter().filter(|row| row.menu == "Edit"))
            .iter()
            .any(|row| row.label == "Find" && row.sub.len() == 6 && row.sub.iter().filter(|row| row.enabled).count() == 3));
    }

    /// A process app's rows that name a submenu go in it, where the first was.
    #[test]
    fn rows_naming_a_submenu_nest_in_it() {
        let rows = parse_rows(
            r#"[{"menu":"View","title":"Columns"},{"menu":"View","submenu":"Update Frequency","title":"Very Often (1 sec)","command":"rate:3","checked":true},
            {"menu":"View","submenu":"Update Frequency","title":"Often (2 sec)","command":"rate:4"},{"menu":"View","title":"Filter"}]"#,
        );
        let view = rows_of(&rows, "View");
        let labels: Vec<&str> = view.iter().map(|row| row.label.as_str()).collect();
        assert_eq!(labels, ["Columns", "Update Frequency", "Filter"]);
        let rates: Vec<(&str, bool)> = view[1].sub.iter().map(|row| (row.label.as_str(), row.checked)).collect();
        assert_eq!(rates, [("Very Often (1 sec)", true), ("Often (2 sec)", false)]);
        assert_eq!(view[1].sub[1].target, "mac.app.rate:4");
    }

    #[test]
    fn a_native_modules_rows_read_as_a_script_apps() {
        use makepad_app_module::{menu_rows_json, MenuRow};
        let rows = parse_rows(&menu_rows_json(&[
            MenuRow::keyed("View", "Hide Sidebar", "ctrl+cmd+S").enabled(false),
            MenuRow::keyed("View", "Scientific", "cmd+2").checked(true),
            MenuRow::keyed("File", "Say \"hi\"\u{2026}", "cmd+N"),
            MenuRow::command("File", "Quick Notes", "move-to:3").within("Move To"),
        ]));
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0], AppRow { enabled: Some(false), ..keyed("View", "Hide Sidebar", "ctrl+cmd+S") });
        assert_eq!(rows[1], AppRow { enabled: Some(true), checked: true, ..keyed("View", "Scientific", "cmd+2") });
        assert_eq!(rows[2].title, "Say \"hi\"\u{2026}");
        assert_eq!(rows[2].command, "key:cmd+N");
        // A row with its own command has no key to send.
        assert_eq!(rows[3], AppRow { menu: "File".into(), title: "Quick Notes".into(), command: "move-to:3".into(), enabled: Some(true), submenu: "Move To".into(), ..Default::default() });
    }
}
