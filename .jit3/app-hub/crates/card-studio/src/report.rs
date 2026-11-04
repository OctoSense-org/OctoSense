//! The report `card-studio render` writes, and the severity model.
//!
//! ```json
//! {
//!   "schema": "card-studio/report@1",
//!   "card": {"input": "news.card", "kind": "l0", "data": "digest.json"},
//!   "sizes": {
//!     "glance": {
//!       "target": [350, 160], "viewport": [350, 160], "kind": "glance",
//!       "png": "glance.png", "snapshot": "glance.snap.json",
//!       "tree": "glance.tree.txt", "log": "glance.log.json",
//!       "realize": {"lint": {…}, "realize": {"nodes": 16, "truncated": false, …}, "sources": […]},
//!       "findings": [{"check": "text_hidden", "severity": "error", "widget": "beauty_0_2_5",
//!                     "text": "Grid", "rect": [0,0,0,0], "message": "…"}],
//!       "metrics": {"widgets": 16, "drawn": 11, "text_nodes": 12, "hidden_text": 5,
//!                   "content_height": 253, "fits": false, …},
//!       "pass": false
//!     }
//!   },
//!   "summary": {"pass": false, "errors": 7, "warnings": 1, "infos": 0}
//! }
//! ```
//!
//! Paths are relative to the report's directory, so a report and its files
//! move together.
//!
//! **Severity.** `error`: a person would see something broken — text cut,
//! hidden or overflowing, a failed source, a card that did not lower, lint or
//! realize failures, an error in the log, a card that does not fit the glance
//! tile. Any error fails the size and the report. `warn`: probably wrong and
//! worth a revision, but the card is usable — a pending state or an em dash
//! showing, text overlapping text, a non-text element spilling out of its
//! parent, content taller than a phone or desktop viewport. `info`: measured
//! facts for the critic (overlaps with no text involved, a render without a
//! realize report).
use crate::sizes::SizeKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const REPORT_SCHEMA: &str = "card-studio/report@1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

/// One measured problem.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// Which check: `text_truncated`, `text_hidden`, `text_clipped`,
    /// `overflow`, `outside_viewport`, `overlap`, `missing_value`,
    /// `state_visible`, `source_pending`, `source_failed`, `does_not_fit`,
    /// `lint`, `realize`, `realize_truncated`, `lower_failed`, `log_error`,
    /// `empty_card`, `no_realize_report`.
    pub check: String,
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widget: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rect: Option<[f64; 4]>,
    pub message: String,
}

impl Finding {
    pub fn new(check: &str, severity: Severity, message: impl Into<String>) -> Finding {
        Finding {
            check: check.into(),
            severity,
            widget: None,
            text: None,
            rect: None,
            message: message.into(),
        }
    }
    pub fn at(mut self, widget: &crate::capture::Widget) -> Finding {
        self.widget = Some(widget.id.clone());
        self.text = widget.text.clone();
        self.rect = Some(widget.rect.as_array());
        self
    }
}

/// Measured facts about one render.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    /// The card's widgets, drawn or not.
    pub widgets: usize,
    /// Of those, laid out with a non-empty rect.
    pub drawn: usize,
    /// Widgets carrying text.
    pub text_nodes: usize,
    /// Text widgets that were not drawn: the layout ran out of room.
    pub hidden_text: usize,
    /// Height the card's content needs, in points from the viewport's top:
    /// the lowest drawn leaf's bottom, or, when content was cut, the same
    /// measured in a taller probe render.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_height: Option<f64>,
    /// `content_height` is only a lower bound (content was cut even in the
    /// probe).
    pub content_height_is_lower_bound: bool,
    /// Everything is drawn, inside the viewport, and the content fits.
    pub fits: bool,
    /// From the realize report: nodes realized, and whether a bound was hit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realize_nodes: Option<u64>,
    pub realize_truncated: bool,
    /// Error lines in the log.
    pub log_errors: usize,
}

/// One size's result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SizeReport {
    pub kind: SizeKind,
    pub target: [f64; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewport: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub png: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realize: Option<serde_json::Value>,
    pub findings: Vec<Finding>,
    pub metrics: Metrics,
    pub pass: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub pass: bool,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
}

impl Summary {
    pub fn of<'a>(findings: impl IntoIterator<Item = &'a Finding>) -> Summary {
        let mut s = Summary::default();
        for f in findings {
            match f.severity {
                Severity::Error => s.errors += 1,
                Severity::Warn => s.warnings += 1,
                Severity::Info => s.infos += 1,
            }
        }
        s.pass = s.errors == 0;
        s
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CardInput {
    pub input: String,
    /// `l0` (a card file), `bundle` (a bundle directory) or `script`
    /// (a bundle with `main.splash`).
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub schema: String,
    pub card: CardInput,
    pub sizes: BTreeMap<String, SizeReport>,
    pub summary: Summary,
}

impl Report {
    pub fn new(card: CardInput, sizes: BTreeMap<String, SizeReport>) -> Report {
        let summary = Summary::of(sizes.values().flat_map(|s| s.findings.iter()));
        Report {
            schema: REPORT_SCHEMA.into(),
            card,
            sizes,
            summary,
        }
    }
}
