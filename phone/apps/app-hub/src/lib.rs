//! Native App Hub, backed by the signed OctoSense catalog.
pub mod catalog;
pub mod icons;
pub use octosense_appstore::{data_root, data_root_if_set, installed_apps, set_data_root};
pub use octosense_appstore::system::SystemApp;

mod system_packs {
    include!(concat!(env!("OUT_DIR"), "/system_apps.rs"));
}

/// A system app's own launcher art, by its short id (`camera`), when its
/// bundle ships an `icon.svg` or `icon.png`.
pub fn system_icon(short_id: &str) -> Option<icons::IconData> {
    let (_, svg, bytes) = system_packs::SYSTEM_ICONS.iter().find(|(id, _, _)| *id == short_id)?;
    Some(if *svg { icons::IconData::Svg(String::from_utf8(bytes.to_vec()).ok()?) } else { icons::IconData::Png(bytes.to_vec()) })
}

/// The system apps this build ships (ADR 0004), registered with the Card
/// runner on first use.
pub fn system_apps() -> Vec<SystemApp> {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(system_packs::register_system_apps);
    octosense_appstore::system::system_apps()
}
mod card_host;
pub use card_host::CARD_MODULE;

pub mod module;
pub mod view;
pub use module::APP_HUB_MODULE;
pub use view::{take_completed_installs, AppHubAction};
include!(concat!(env!("OUT_DIR"), "/app_icon.rs"));
