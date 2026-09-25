//! Native App Hub, backed by the signed OctoSense catalog.
pub mod catalog;
pub mod icons;
pub use octosense_appstore::{data_root, data_root_if_set, installed_apps, set_data_root};
mod card_host;
pub use card_host::CARD_MODULE;

pub mod module;
pub mod view;
pub use module::APP_HUB_MODULE;
pub use view::{take_completed_installs, AppHubAction};
include!(concat!(env!("OUT_DIR"), "/app_icon.rs"));
