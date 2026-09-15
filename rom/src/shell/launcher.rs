//! The `apps` provider — omarchy's launcher.
//!
//! In omarchy-shell the launcher is not a separate surface: `Super+Space`
//! opens the menu at its root and `Super+Alt+Space` opens it on the `apps`
//! provider, which lists desktop entries alphabetically
//! (`services/AppLibrary.qml` + `AppSearch.js`) minus the ids in
//! `default/omarchy/launcher.hides` and anything marked Hidden/NoDisplay.
//! Ours lists the app registry (`clients.rs`) — the same contract, with our
//! binaries in place of `.desktop` files: an entry appears only when its
//! binary is actually next to us, which is our version of NoDisplay.
//!
//! The launcher's own token section (`[launcher]` in shell.toml.tpl: a card
//! at α 0.95 over a scrim at α 0.5) is what the brief asks the launcher to
//! wear, so the menu surface takes its colors from `shell.launcher` when it
//! is opened on this provider and from `shell.menu` otherwise.

use crate::clients;

use super::menu::{MenuItem, MenuKind};
use super::ui::Ico;

/// Ids hidden from the launcher, one per line, matched EXACTLY and
/// case-sensitively against the entry id — omarchy's `launcher.hides`
/// semantics (no globs, no substring).
pub fn hides() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let path = crate::theme::makepad_home().join("wm/launcher.hides");
    if let Ok(text) = std::fs::read_to_string(path) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            out.push(line.to_string());
        }
    }
    out
}

/// True when this id is hidden (exact, case-sensitive).
pub fn is_hidden(id: &str, hides: &[String]) -> bool {
    hides.iter().any(|h| h == id)
}

/// An icon for every app id. The user's rule: EVERY row carries one, so
/// the label column starts at the same x on every line — our icon set is
/// small, so a few are stand-ins rather than literal pictograms.
fn icon_for(id: &str) -> Option<Ico> {
    Some(match id {
        "terminal" => Ico::Keyboard,
        "browser" => Ico::Search,
        "files" => Ico::Menu,
        "task" => Ico::Pulse,
        "sheets" => Ico::Calendar,
        "score" => Ico::Bell,
        "image" => Ico::Monitor,
        "video" => Ico::Play,
        "pdf" => Ico::Check,
        "route" => Ico::Globe,
        "mixer" => Ico::Speaker,
        "vj" => Ico::Headphone,
        "photos" => Ico::Photo,
        "clock" => Ico::Calendar,
        "weather" => Ico::Brightness,
        "fabric" => Ico::Shirt,
        "fab" => Ico::Refresh,
        "studio" => Ico::Moon,
        // Never None: an app row without an icon would shift its label.
        _ => Ico::Dot,
    })
}

/// The `apps` provider rows: every registry app whose binary exists, not
/// hidden, in the CURATED registry order (the user's: browser/files/
/// terminal first, then by rarity — a deliberate deviation from omarchy's
/// alphabetical provider). The live filter never reorders.
pub fn apps() -> Vec<MenuItem> {
    thread_local! {
        static MEMO: std::cell::RefCell<Option<(std::time::Instant, Vec<MenuItem>)>> = const { std::cell::RefCell::new(None) };
    }
    if let Some(items) = MEMO.with(|m| m.borrow().as_ref().filter(|(at, _)| at.elapsed().as_secs_f64() < APPS_MEMO_S).map(|(_, items)| items.clone())) {
        return items;
    }
    let items = apps_uncached();
    MEMO.with(|m| *m.borrow_mut() = Some((std::time::Instant::now(), items.clone())));
    items
}

/// How long `apps()` answers from memory. The list depends on two settings
/// files (`wm/launcher.hides`, `wm/apps.splash`) and on which app binaries
/// exist, all read afresh per call before: the phone shell asked several
/// times per frame, so a home page burnt milliseconds of file I/O per
/// frame. A change to those files shows within a second.
pub const APPS_MEMO_S: f64 = 1.0;

/// The list, read from the settings and the file system right now.
pub fn apps_uncached() -> Vec<MenuItem> {
    let hides = hides();
    let items: Vec<MenuItem> = clients::registry()
        .iter()
        // Launchable: a package this checkout can run, or a module this
        // build links (the only kind the web build has).
        .filter(|app| crate::apps::is_launchable(app))
        .filter(|app| !is_hidden(&app.id, &hides))
        .map(|app| MenuItem {
            id: format!("apps.{}", app.id),
            label: app.label.clone(),
            icon: icon_for(&app.id),
            kind: MenuKind::App,
            checked: false,
            disabled: false,
            description: String::new(),
            aliases: vec![app.id.clone(), app.package.clone()],
        })
        .collect();
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_match_exactly_and_case_sensitively() {
        let hides = vec!["btop".to_string(), "libreoffice-base".to_string()];
        assert!(is_hidden("btop", &hides));
        // No substring, no case folding, no globs.
        assert!(!is_hidden("btopper", &hides));
        assert!(!is_hidden("BTOP", &hides));
        assert!(!is_hidden("libreoffice", &hides));
    }

    #[test]
    fn the_apps_list_is_answered_from_memory_within_a_second() {
        let first = apps();
        let again = apps();
        assert_eq!(first.len(), again.len());
        assert!(first.iter().zip(&again).all(|(a, b)| a.id == b.id && a.label == b.label));
        // The memo is the uncached answer, not a stale or reordered one.
        let fresh = apps_uncached();
        assert!(first.iter().zip(&fresh).all(|(a, b)| a.id == b.id) && first.len() == fresh.len());
    }

    #[test]
    fn the_apps_provider_keeps_the_curated_order() {
        let items = apps();
        // Rows appear in registry order (available subset preserves it).
        let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        let registry_order: Vec<String> = clients::registry()
            .iter()
            .filter(|a| labels.contains(&a.label))
            .map(|a| a.label.clone())
            .collect();
        assert_eq!(labels, registry_order);
        // Every row is an app row under the `apps` parent.
        assert!(items
            .iter()
            .all(|i| i.kind == MenuKind::App && i.id.starts_with("apps.")));
    }
}
