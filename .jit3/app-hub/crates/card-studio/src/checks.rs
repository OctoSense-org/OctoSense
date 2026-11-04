//! The measured checks (ADR 0002 §7 step 4): what can be decided from
//! rectangles, text and the log, before any model looks at the picture.
//!
//! All of it reads a [`Capture`]; nothing here needs a window, so the phone
//! can keep these checks when the vision critique runs elsewhere.
//!
//! | check | severity | what it measures |
//! |---|---|---|
//! | `text_hidden` | error | a text widget was not laid out at all: the card ran out of room (zero rect in `/snap?all=1`) |
//! | `hidden` | warn | the same for a non-text leaf (an image, an icon) |
//! | `text_clipped` | error | a text widget's rect crosses the viewport's edge |
//! | `outside_viewport` | warn | a non-text widget's rect crosses the viewport's edge |
//! | `overflow` | error (text) / warn | a widget's rect sticks out of its parent's by more than [`SLACK`] |
//! | `text_truncated` | error | a text widget's rect is too small for its text (see [`truncation_ratio`]) |
//! | `overlap` | warn (text on text) / info | two sibling rects intersect by more than 10% of the smaller |
//! | `missing_value` | warn | a binding rendered as an em dash: it resolved to nothing |
//! | `state_visible` | warn | visible text reads as a loading, empty or failed state |
//! | `source_pending` / `source_failed` | warn / error | the realize report says a source is not ready |
//! | `does_not_fit` | error (glance) / warn | the content is taller than the viewport |
//! | `lint`, `realize`, `realize_truncated`, `lower_failed` | error | the card is not a clean L0 card, or did not lower |
//! | `log_error` | error | an `[E]` line in the log (Splash errors, refusals) |
//! | `empty_card` | error | nothing drawn |
//! | `no_realize_report` | info | card-host logged no realize report (a script app, or an old card-host) |
use crate::capture::{Capture, Rect, Widget};
use crate::report::{Finding, Metrics, Severity};
use crate::sizes::{SizeKind, TargetSize};
use std::collections::{HashMap, HashSet};

/// Rounding slack, in points, before an edge counts as crossed.
pub const SLACK: f64 = 1.0;

/// The result of checking one render.
#[derive(Clone, Debug, Default)]
pub struct Checked {
    pub findings: Vec<Finding>,
    pub metrics: Metrics,
    pub viewport: Option<Rect>,
}

impl Checked {
    pub fn pass(&self) -> bool {
        !self.findings.iter().any(|f| f.severity == Severity::Error)
    }
}

fn has_text(w: &Widget) -> bool {
    w.text.as_deref().is_some_and(|t| !t.trim().is_empty())
}

/// How many times too small a text widget's rect is for its text; above 1.0
/// the text cannot be all there.
///
/// Makepad reports a label's laid-out rect, not the text's extent, so a label
/// squeezed by its row keeps its full string in `/snap` but draws a sliver of
/// it. The estimate is deliberately generous to the card: a glyph is taken as
/// 0.25 of the line height wide (measured Roboto runs 0.42–0.54), and the line
/// height as the rect's height capped at 20pt so a wrapped, multi-line label
/// is judged by area. Only a gross mismatch fires.
pub fn truncation_ratio(text: &str, rect: &Rect) -> f64 {
    if rect.is_empty() {
        return f64::INFINITY;
    }
    let chars = text.trim().chars().count() as f64;
    let line = rect.h.min(20.0);
    let need = chars * 0.25 * line * line;
    need / rect.area()
}

/// Visible text that reads as a state rather than content.
fn state_text(text: &str) -> Option<&'static str> {
    let t = text.trim().to_lowercase();
    const PATTERNS: &[(&str, &str)] = &[
        ("loading", "loading"),
        ("not available", "unavailable"),
        ("unavailable", "unavailable"),
        ("could not", "failed"),
        ("couldn't", "failed"),
        ("failed", "failed"),
        ("no data", "empty"),
        ("nothing yet", "empty"),
        ("try again", "failed"),
        ("error", "failed"),
        ("加载", "loading"),
        ("失败", "failed"),
    ];
    PATTERNS
        .iter()
        .find(|(p, _)| t.contains(p))
        .map(|(_, kind)| *kind)
}

/// Run every check over `capture` rendered at `size`. `probe` is an optional
/// second render of the same card at the same width and a taller height,
/// used only to measure how tall the content wants to be when it was cut.
pub fn check(capture: &Capture, size: &TargetSize, probe: Option<&Capture>) -> Checked {
    let mut out = Checked {
        viewport: capture.viewport(),
        ..Default::default()
    };
    let widgets = capture.card_widgets();
    let viewport = out
        .viewport
        .unwrap_or(Rect::new(0.0, 0.0, size.width, size.height));

    let by_id: HashMap<&str, &Widget> = widgets.iter().map(|w| (w.id.as_str(), w)).collect();
    let parents: HashSet<&str> = widgets.iter().filter_map(|w| w.parent.as_deref()).collect();
    let is_leaf = |w: &Widget| !parents.contains(w.id.as_str());
    // Ids whose subtree carries text, for the overlap severity.
    let mut texty: HashSet<String> = HashSet::new();
    for w in widgets.iter().filter(|w| has_text(w)) {
        let mut at = Some(w.id.clone());
        while let Some(id) = at {
            if !texty.insert(id.clone()) {
                break;
            }
            at = by_id.get(id.as_str()).and_then(|p| p.parent.clone());
        }
    }

    let m = &mut out.metrics;
    m.widgets = widgets.len();
    m.drawn = widgets.iter().filter(|w| w.drawn).count();
    m.text_nodes = widgets.iter().filter(|w| has_text(w)).count();
    let f = &mut out.findings;

    for w in &widgets {
        let text = has_text(w);
        if !w.drawn {
            if text {
                m.hidden_text += 1;
                f.push(
                    Finding::new(
                        "text_hidden",
                        Severity::Error,
                        format!(
                            "{:?} was not laid out: the card ran out of room before it",
                            w.text.as_deref().unwrap_or("")
                        ),
                    )
                    .at(w),
                );
            } else if is_leaf(w) {
                f.push(
                    Finding::new(
                        "hidden",
                        Severity::Warn,
                        format!("{} {} was not laid out", w.ty, w.id),
                    )
                    .at(w),
                );
            }
            continue;
        }
        // Against the viewport.
        let [l, t, r, b] = w.rect.overhang(&viewport);
        if l.max(t).max(r).max(b) > SLACK {
            let (check, sev) = if text {
                ("text_clipped", Severity::Error)
            } else {
                ("outside_viewport", Severity::Warn)
            };
            f.push(
                Finding::new(
                    check,
                    sev,
                    format!(
                        "{} {} crosses the viewport's edge by {} (left, top, right, bottom)",
                        w.ty,
                        w.id,
                        fmt_sides([l, t, r, b])
                    ),
                )
                .at(w),
            );
        }
        // Against its parent.
        if let Some(parent) = w
            .parent
            .as_deref()
            .and_then(|p| by_id.get(p))
            .filter(|p| p.drawn)
        {
            let [l, t, r, b] = w.rect.overhang(&parent.rect);
            if l.max(t).max(r).max(b) > SLACK {
                let sev = if text {
                    Severity::Error
                } else {
                    Severity::Warn
                };
                f.push(
                    Finding::new(
                        "overflow",
                        sev,
                        format!(
                            "{} {} sticks out of {} by {} (left, top, right, bottom)",
                            w.ty,
                            w.id,
                            parent.id,
                            fmt_sides([l, t, r, b])
                        ),
                    )
                    .at(w),
                );
            }
        }
        if text && is_leaf(w) {
            let t = w.text.as_deref().unwrap_or("");
            let ratio = truncation_ratio(t, &w.rect);
            if ratio > 1.0 {
                f.push(Finding::new("text_truncated", Severity::Error, format!(
                    "{:?} gets {:.0}x{:.0}pt, about {:.1}x too small for {} characters: it is cut or squeezed",
                    t, w.rect.w, w.rect.h, ratio, t.trim().chars().count())).at(w));
            }
            if t.trim() == "—" {
                f.push(
                    Finding::new(
                        "missing_value",
                        Severity::Warn,
                        "a binding rendered as an em dash: it resolved to nothing",
                    )
                    .at(w),
                );
            } else if let Some(kind) = state_text(t) {
                f.push(
                    Finding::new(
                        "state_visible",
                        Severity::Warn,
                        format!("visible text reads as a {kind} state: {t:?}"),
                    )
                    .at(w),
                );
            }
        }
    }

    // Overlapping siblings.
    let mut children: HashMap<&str, Vec<&Widget>> = HashMap::new();
    for w in widgets.iter().filter(|w| w.drawn) {
        children
            .entry(w.parent.as_deref().unwrap_or(""))
            .or_default()
            .push(w);
    }
    for (_, kids) in children {
        for (i, a) in kids.iter().enumerate() {
            for b in &kids[i + 1..] {
                let inter = a.rect.intersection(&b.rect).area();
                let smaller = a.rect.area().min(b.rect.area());
                if inter > 1.0 && smaller > 0.0 && inter / smaller > 0.10 {
                    let both_text = texty.contains(&a.id) && texty.contains(&b.id);
                    let sev = if both_text {
                        Severity::Warn
                    } else {
                        Severity::Info
                    };
                    f.push(
                        Finding::new(
                            "overlap",
                            sev,
                            format!(
                                "{} and {} overlap by {:.0}% of the smaller{}",
                                a.id,
                                b.id,
                                100.0 * inter / smaller,
                                if both_text { ", text on text" } else { "" }
                            ),
                        )
                        .at(a),
                    );
                }
            }
        }
    }

    // The realize report and the log.
    match capture.realize_report() {
        Some(report) => realize_findings(&report, m, f),
        None => f.push(Finding::new(
            "no_realize_report",
            Severity::Info,
            "card-host logged no realize report (a script app, or a card-host without it)",
        )),
    }
    let errors = capture.log_errors();
    m.log_errors = errors.len();
    for line in errors.iter().take(10) {
        f.push(Finding::new("log_error", Severity::Error, line.clone()));
    }

    if m.drawn == 0 {
        f.push(Finding::new(
            "empty_card",
            Severity::Error,
            "nothing of the card was drawn",
        ));
    }

    // Content height and fit.
    let clipped = f.iter().any(|x| x.check == "text_clipped");
    let (height, lower_bound) = match (
        content_bottom(&widgets, &viewport),
        m.hidden_text > 0 || clipped,
        probe,
    ) {
        (h, true, Some(probe)) => {
            let pw = probe.card_widgets();
            let pv = probe.viewport().unwrap_or(viewport);
            let still_cut = pw.iter().any(|w| has_text(w) && !w.drawn);
            (content_bottom(&pw, &pv).or(h), still_cut)
        }
        (h, cut, _) => (h, cut),
    };
    m.content_height = height;
    m.content_height_is_lower_bound = lower_bound;
    let too_tall = height.is_some_and(|h| h > viewport.h + SLACK);
    m.fits = !too_tall && m.hidden_text == 0 && !clipped;
    if too_tall || (m.hidden_text > 0 && height.is_some()) {
        let sev = if size.kind == SizeKind::Glance {
            Severity::Error
        } else {
            Severity::Warn
        };
        f.push(Finding::new(
            "does_not_fit",
            sev,
            format!(
                "the content needs {}{:.0}pt; the {} viewport is {:.0}pt tall",
                if lower_bound { "at least " } else { "" },
                height.unwrap_or(0.0),
                size.name,
                viewport.h
            ),
        ));
    }
    out
}

/// The lowest drawn leaf's bottom, from the viewport's top. Containers are
/// left out: a `Fill` surface is always exactly as tall as the viewport.
fn content_bottom(widgets: &[Widget], viewport: &Rect) -> Option<f64> {
    let parents: HashSet<&str> = widgets.iter().filter_map(|w| w.parent.as_deref()).collect();
    widgets
        .iter()
        .filter(|w| w.drawn && !parents.contains(w.id.as_str()))
        .filter(|w| has_text(w) || !matches!(w.ty.as_str(), "View" | "RoundedView" | "SolidView"))
        .map(|w| w.rect.bottom() - viewport.y)
        .fold(None, |acc: Option<f64>, b| {
            Some(acc.map_or(b, |a| a.max(b)))
        })
}

fn realize_findings(report: &serde_json::Value, m: &mut Metrics, f: &mut Vec<Finding>) {
    let diags = |v: &serde_json::Value| -> Vec<String> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .map(|d| {
                        format!(
                            "{}:{}: {}",
                            d["line"],
                            d["column"],
                            d["message"].as_str().unwrap_or("")
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let lint = &report["lint"];
    if lint["valid"] == serde_json::Value::Bool(false) {
        f.push(Finding::new(
            "lint",
            Severity::Error,
            format!(
                "the card is not valid {}: {}",
                lint["level"].as_str().unwrap_or("L0"),
                diags(&lint["diagnostics"]).join("; ")
            ),
        ));
    } else if lint["level"]
        .as_str()
        .is_some_and(|l| l != "L0" && l != "L1")
    {
        f.push(Finding::new(
            "lint",
            Severity::Error,
            format!("the card is {}, not L0", lint["level"]),
        ));
    }
    let realize = &report["realize"];
    m.realize_nodes = realize["nodes"].as_u64();
    m.realize_truncated = realize["truncated"].as_bool().unwrap_or(false);
    if m.realize_truncated {
        f.push(Finding::new(
            "realize_truncated",
            Severity::Error,
            "realization hit a bound: the tree is partial",
        ));
    }
    let rd = diags(&realize["diagnostics"]);
    if !rd.is_empty() {
        f.push(Finding::new("realize", Severity::Error, rd.join("; ")));
    }
    if let Some(e) = report["lower_error"].as_str() {
        f.push(Finding::new(
            "lower_failed",
            Severity::Error,
            format!("the card did not lower: {e}"),
        ));
    }
    for source in report["sources"].as_array().into_iter().flatten() {
        let name = source["name"].as_str().unwrap_or("?");
        match source["state"].as_str() {
            Some("failed") => f.push(Finding::new(
                "source_failed",
                Severity::Error,
                format!(
                    "source {name} ({}) failed: the card shows its failed state",
                    source["helper"].as_str().unwrap_or("")
                ),
            )),
            Some("pending") => f.push(Finding::new(
                "source_pending",
                Severity::Warn,
                format!(
                    "source {name} ({}) is pending: no data was given for it",
                    source["helper"].as_str().unwrap_or("")
                ),
            )),
            _ => {}
        }
    }
}

fn fmt_sides(s: [f64; 4]) -> String {
    format!("{:.0}/{:.0}/{:.0}/{:.0}pt", s[0], s[1], s[2], s[3])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_ratio_flags_only_gross_squeezes() {
        // Measured on card-host: a squeezed chip label and healthy labels.
        assert!(
            truncation_ratio(
                "Artificial intelligence regulation",
                &Rect::new(318.0, 67.0, 12.0, 14.0)
            ) > 5.0
        );
        assert!(truncation_ratio("Open News", &Rect::new(30.0, 135.0, 68.0, 14.0)) < 1.0);
        assert!(truncation_ratio("Today in tech", &Rect::new(20.0, 47.0, 142.0, 26.0)) < 1.0);
        // A wrapped three-line label is judged by area.
        let long = "word ".repeat(20);
        assert!(truncation_ratio(&long, &Rect::new(0.0, 0.0, 310.0, 51.0)) < 1.0);
    }

    #[test]
    fn state_words() {
        assert_eq!(state_text("Loading the digest…"), Some("loading"));
        assert_eq!(state_text("The digest could not be loaded"), Some("failed"));
        assert_eq!(state_text("Chip makers race to 1nm"), None);
    }
}
