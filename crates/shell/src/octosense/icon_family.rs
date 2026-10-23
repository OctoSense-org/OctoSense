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

fn artwork(style: DesktopStyle) -> impl Iterator<Item = (&'static str, String)> {
    ICONS.iter().map(move |&(name, source, mono)| {
        let svg = if style == DesktopStyle::Omarchy {
            mono.to_string()
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
