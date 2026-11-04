//! card-studio: render a card, measure it, and prepare its critique.
//!
//! ADR 0002 §7 step 3–4: "A render tool loads the card in a hidden
//! `card-host --remote` at the target sizes (glance tile, phone, desktop) and
//! returns the frame (`/g`), the widget snapshot (`/snap`), the log, the
//! realize report (truncation) and the app's lint result", then the card is
//! critiqued: measured checks plus a vision model's judgement.
//!
//! The pieces:
//!
//! - [`capture`]: what one render at one size produced (`/snap`, `/d`, `/log`)
//!   and how to read it. Pure; the fixtures in `fixtures/captures` are real
//!   captures, so the checks are tested without a GPU.
//! - [`checks`]: the measured checks — clipped, truncated or hidden text,
//!   overflow, elements outside the viewport, overlapping siblings, empty or
//!   failed data states, fits-the-tile, node count and truncation, errors in
//!   the log — producing [`report::Finding`]s and [`report::Metrics`].
//! - [`render`]: launching card-host hidden, one process per size, and
//!   collecting a [`capture::Capture`] from its remote instrument.
//! - [`critique`]: the vision-model request payload (PNG + snapshot +
//!   rubric). The model call is the caller's.
//! - [`report`]: the JSON report and the severity model.
pub mod capture;
pub mod checks;
pub mod critique;
pub mod remote;
pub mod render;
pub mod report;
pub mod sizes;
