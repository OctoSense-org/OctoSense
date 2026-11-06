//! octosense-l0-sys: the `sys` vocabulary of L0 cards, as a host module.
//!
//! The 70-odd `sys.*` methods an L0 card calls (live data, device state, the user's references,
//! formatters) used to live inside the fork makepad's Splash widget. They are AppCard's
//! vocabulary, not a widget's, so this crate owns them and installs them into every Splash
//! isolate through makepad's module hooks; makepad itself stays upstream-clean.
//!
//! A host calls [`ensure_registered`] once before its first Splash evaluates, routes
//! `Event::NetworkResponses` to [`handle_network_responses`], and publishes the user's references
//! through [`set_locale`], [`set_prefs`], [`set_link`] and [`set_collections`].

mod dataset;
pub mod fetch;
pub mod geo;
pub mod l0;
mod news;
pub mod sys;

pub use fetch::{handle_network_responses, live_data_epoch};
pub use sys::{body_binds_live_data, collection_len, install, set_collections, set_link, set_locale, set_prefs};

/// Install the `sys` module into every Splash isolate made from now on, and tell Splash which
/// bodies bind live data. Once per process; later calls do nothing.
pub fn ensure_registered() {
    thread_local! {
        static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if !DONE.with(|d| d.replace(true)) {
        makepad_widgets::widget_async::register_splash_isolate_mod(install);
        makepad_widgets::widget_async::register_live_data_predicate(body_binds_live_data);
    }
}
