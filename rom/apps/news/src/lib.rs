//! News for OctoSense: headlines from Hacker News, TechMeme, Google News and
//! the person's own RSS or Atom feeds, one tab per source plus All. The same
//! crate is a standalone window (`src/main.rs`), a process-hosted tile, and
//! an in-process `AppModule` (`module.rs`) that phones link automatically.
pub use makepad_widgets;

pub mod ai;
pub mod feed;
pub mod hn;
pub mod model;
pub mod module;
pub mod view;

pub use module::{NewsModule, NEWS_MODULE};
pub use view::NewsView;
