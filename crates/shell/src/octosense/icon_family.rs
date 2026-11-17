//! Approved OctoSense identities, shared by desktop and Home.
//! Regenerate resource copies with `python3 tools/build_icon_family.py --install`.
use makepad_widgets::{app_icon, desktop_style::DesktopStyle};
use super::icon_frame;

macro_rules! icon {
    ($name:literal) => {
        ($name,
         include_str!(concat!("../../resources/icons/apps/family/", $name, ".svg")),
         include_str!(concat!("../../resources/icons/apps/family/omarchy/", $name, ".svg")))
    };
}

pub(super) const ICONS: &[(&str, &str, &str)] = &[
    icon!("mail"), icon!("calendar"), icon!("calculator"), icon!("clock"),
    icon!("notes"), icon!("reminders"), icon!("weather"), icon!("sheets"),
    icon!("settings"), icon!("updater"), icon!("terminal"), icon!("octoscode"),
    icon!("reference"), icon!("appcard"), icon!("browser"), icon!("files"),
    icon!("task"), icon!("finance"), icon!("route"), icon!("image"),
    icon!("pdf"), icon!("mixer"), icon!("fabric"), icon!("score"),
    icon!("video"), icon!("vj"), icon!("fab"), icon!("studio"),
];

/// The date the Calendar's artwork is drawn with.
const CALENDAR_DATE: &str = "M23 31 V44 M33 31 H41 L33 37 H38 Q43 37 41 42 Q39 46 33 43";

/// The strokes of a day of the month on the Calendar's page, in the artwork's
/// own hand: each digit nine wide, thirteen tall from y 31.
fn date_strokes(day: u32) -> String {
    let digit = |d: u32, x: f64| {
        let at = |dx: f64| x + dx;
        match d {
            0 => format!("M{} 31 Q{} 31 {} 37.5 Q{} 44 {} 44 Q{} 44 {} 37.5 Q{} 31 {} 31 Z", at(4.5), at(0.), at(0.), at(0.), at(4.5), at(9.), at(9.), at(9.), at(4.5)),
            1 => format!("M{} 31 V44", at(4.5)),
            2 => format!("M{} 34 Q{} 31 {} 31 Q{} 31 {} 35 Q{} 38 {} 44 H{}", at(0.), at(1.), at(4.5), at(9.), at(9.), at(9.), at(0.), at(9.)),
            3 => format!("M{} 31 H{} L{} 37 H{} Q{} 37 {} 42 Q{} 46 {} 43", at(0.), at(8.), at(0.), at(5.), at(10.), at(8.), at(6.), at(0.)),
            4 => format!("M{} 44 V31 L{} 40 H{}", at(7.), at(0.), at(9.)),
            5 => format!("M{} 31 H{} L{} 37 H{} Q{} 37 {} 40.5 Q{} 44 {} 44 Q{} 44 {} 42", at(8.), at(1.), at(0.), at(5.), at(9.), at(9.), at(9.), at(4.5), at(1.), at(0.)),
            6 => format!("M{} 31 Q{} 34 {} 39.5 Q{} 44 {} 44 Q{} 44 {} 40 Q{} 36.5 {} 36.5 Q{} 36.5 {} 39.5", at(7.), at(0.), at(0.), at(0.), at(4.5), at(9.), at(9.), at(9.), at(4.5), at(1.), at(0.)),
            7 => format!("M{} 31 H{} L{} 44", at(0.), at(9.), at(3.)),
            8 => format!("M{} 37 Q{} 37 {} 34 Q{} 31 {} 31 Q{} 31 {} 34 Q{} 37 {} 37 Q{} 37 {} 40.5 Q{} 44 {} 44 Q{} 44 {} 40.5 Q{} 37 {} 37 Z", at(4.5), at(0.), at(0.), at(0.), at(4.5), at(9.), at(9.), at(9.), at(4.5), at(0.), at(0.), at(0.), at(4.5), at(9.), at(9.), at(9.), at(4.5)),
            _ => format!("M{} 44 Q{} 41 {} 35.5 Q{} 31 {} 31 Q{} 31 {} 35 Q{} 38.5 {} 38.5 Q{} 38.5 {} 35.5", at(2.), at(9.), at(9.), at(9.), at(4.5), at(0.), at(0.), at(0.), at(4.5), at(8.), at(9.)),
        }
    };
    if day < 10 { digit(day, 27.) } else { format!("{} {}", digit(day / 10 % 10, 20.), digit(day % 10, 34.)) }
}

/// Today's day of the month, where the clock is cheap to read.
pub(super) fn today() -> u32 {
    if cfg!(unix) { crate::shell::bar::local_time(c"%d").trim().parse().unwrap_or(0) } else { 0 }
}

fn artwork(style: DesktopStyle) -> impl Iterator<Item = (&'static str, String)> {
    ICONS.iter().map(move |&(name, source, mono)| {
        let day = if name == "calendar" && style == DesktopStyle::Macos { today() } else { 0 };
        let svg = if style == DesktopStyle::Omarchy {
            mono.to_string()
        } else if day > 0 {
            // A Mac's Calendar icon shows today.
            icon_frame::styled_svg(&source.replace(CALENDAR_DATE, &date_strokes(day)), style)
        } else {
            icon_frame::styled_svg(source, style)
        };
        (name, svg)
    })
}

/// Overlay the explicit local identities onto an otherwise untouched catalog.
/// Upstream can change these images or add new IDs without an asset merge here.
pub(super) fn apply(assets: &mut Vec<app_icon::IconAsset>, style: DesktopStyle) {
    for (name, svg) in artwork(style) {
        match assets.iter_mut().find(|asset| asset.name == name) {
            Some(asset) => asset.svg = svg,
            None => assets.push(app_icon::IconAsset { name: name.into(), svg }),
        }
    }
}

/// Explicit built-in replacements win over system bundle art in every style.
/// Installed `hub:` names and identities outside this list keep their own art.
pub(super) fn owns_svg(name: &str) -> bool {
    ICONS.iter().any(|&(id, _, _)| id == app_icon::canonical_name(name))
}

/// Native identity only: never intercept an installed app's `hub:` namespace.
pub(super) fn png(name: &str) -> Option<&'static [u8]> {
    (makepad_widgets::app_icon::canonical_name(name) == "rinx")
        .then_some(include_bytes!("../../resources/icons/apps/family/rinx.png").as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_calendars_date_is_where_the_artwork_draws_it() {
        let source = ICONS.iter().find(|&&(name, _, _)| name == "calendar").unwrap().1;
        assert_eq!(source.matches(CALENDAR_DATE).count(), 1, "the art changed: move CALENDAR_DATE with it");
        for day in 1..=31 {
            let strokes = date_strokes(day);
            assert!(strokes.starts_with('M') && strokes.split_whitespace().count() > 1, "{strokes}");
            assert_eq!(strokes.matches('M').count(), if day < 10 { 1 } else { 2 });
            // Nothing leaves the page.
            for word in strokes.split_whitespace() {
                let number: f64 = word.trim_start_matches(['M', 'L', 'H', 'V', 'Q', 'Z']).parse().unwrap_or(30.);
                assert!((13.0..=50.0).contains(&number), "{day}: {word}");
            }
        }
    }

    #[test]
    fn upstream_changes_and_new_identities_survive_the_local_overlay() {
        for style in DesktopStyle::ALL {
            let mut assets = vec![
                app_icon::IconAsset { name: "browser".into(), svg: "changed upstream Browser".into() },
                app_icon::IconAsset { name: "photos".into(), svg: "upstream Photos".into() },
                app_icon::IconAsset { name: "future-app".into(), svg: "new upstream identity".into() },
            ];
            apply(&mut assets, style);
            let source = |name| &assets.iter().find(|asset| asset.name == name).unwrap().svg;
            assert_eq!(source("photos"), "upstream Photos");
            assert_eq!(source("future-app"), "new upstream identity");
            assert_eq!(source("browser"), &artwork(style).find(|(name, _)| *name == "browser").unwrap().1);
            assert_eq!(assets.len(), ICONS.len() + 2);
            let original = assets.clone();
            apply(&mut assets, style);
            assert_eq!(assets, original, "reapplying must not duplicate or change identities");
        }
    }
}
