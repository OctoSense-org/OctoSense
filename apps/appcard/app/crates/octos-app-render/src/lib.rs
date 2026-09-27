//! octos-app-render
//!
//! Re-exports of the streaming-markdown pipeline that octos-app uses verbatim
//! from aichat. Kept in a separate crate so the test crate can pull it without
//! pulling Makepad.

pub use streaming_markdown_kit::{
    streaming_display_with_latex_autowrap_remend, wrap_bare_latex, SanitizeOptions,
};
