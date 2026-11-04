//! The vision-critique request (ADR 0002 §7 step 4, the model's half).
//!
//! `card-studio critique --report r.json --rubric rubric.md` turns a render
//! report into ONE provider-neutral JSON payload; the caller (the octos
//! `card-studio` skill, or an app agent) sends it to whatever vision model its
//! profile allows and keeps the answer with the run. card-studio never calls
//! a model.
//!
//! ```json
//! {
//!   "schema": "card-studio/critique-request@1",
//!   "prompt": "You are reviewing … <rubric> … Answer with JSON only …",
//!   "rubric": "<rubric.md, verbatim>",
//!   "response_schema": { JSON Schema of the expected answer, see below },
//!   "measured": {"pass": false, "errors": 3, "warnings": 1, "infos": 0},
//!   "sizes": [
//!     {
//!       "size": "glance", "kind": "glance", "target": [350, 160], "viewport": [350, 160],
//!       "image": {"path": "/abs/out/glance.png", "media_type": "image/png",
//!                 "data_base64": "…only with --inline…"},
//!       "widgets": [{"id": "beauty_0_1", "type": "Label", "rect": [20, 47, 142, 26],
//!                    "text": "Today in tech", "drawn": true, "parent": "beauty_0"}],
//!       "findings": [ …the measured findings for this size… ],
//!       "metrics": { … }
//!     }
//!   ]
//! }
//! ```
//!
//! To send it: put `prompt` in a user turn, then one image block per entry of
//! `sizes` (from `image.data_base64`, or read `image.path`), each preceded by
//! a text line naming the size and carrying its `widgets` and `findings` as
//! JSON. The answer must match `response_schema`:
//!
//! ```json
//! {"score": 0-10, "verdict": "pass" | "revise",
//!  "sizes": {"glance": {"legibility": 0-5, "hierarchy": 0-5, "balance": 0-5,
//!                       "identity": 0-5, "notes": "…"}},
//!  "issues": [{"size": "glance", "widget": "beauty_0_1_1_0", "severity": "error|warn|info",
//!              "problem": "…", "fix": "…a change to the L0 card…"}]}
//! ```
//!
//! A measured `error` always means `revise`: the model judges what the
//! numbers cannot (legibility, hierarchy, balance, whether it reads as this
//! app's card), it does not overrule them.
use crate::capture::Capture;
use crate::report::Report;
use serde_json::{json, Value};
use std::path::Path;

pub const REQUEST_SCHEMA: &str = "card-studio/critique-request@1";

#[derive(Clone, Debug, Default)]
pub struct CritiqueOptions {
    /// Only these sizes; empty means all in the report.
    pub sizes: Vec<String>,
    /// Embed each PNG as base64.
    pub inline_images: bool,
}

/// The JSON Schema the model's answer must satisfy.
pub fn response_schema() -> Value {
    let score = |max: u32| json!({"type": "number", "minimum": 0, "maximum": max});
    json!({
        "type": "object",
        "required": ["score", "verdict", "issues"],
        "properties": {
            "score": score(10),
            "verdict": {"type": "string", "enum": ["pass", "revise"]},
            "sizes": {"type": "object", "additionalProperties": {
                "type": "object",
                "properties": {
                    "legibility": score(5), "hierarchy": score(5), "balance": score(5), "identity": score(5),
                    "notes": {"type": "string"}
                }
            }},
            "issues": {"type": "array", "items": {
                "type": "object",
                "required": ["severity", "problem"],
                "properties": {
                    "size": {"type": "string"},
                    "widget": {"type": "string"},
                    "severity": {"type": "string", "enum": ["error", "warn", "info"]},
                    "problem": {"type": "string"},
                    "fix": {"type": "string"}
                }
            }}
        }
    })
}

fn prompt(rubric: &str, report: &Report) -> String {
    format!(
        "You are reviewing a card an app agent generated before it is published to the glance screen. \
         The card is an L0 card: it may state nothing it did not get from a declared source. \
         You get one rendered frame per target size, the widget snapshot (ids, types, rects in layout points, text) \
         and the measured findings. Judge what measurement cannot: legibility, visual hierarchy, balance, \
         and whether it reads as this app's card. Do not contradict the measured findings: any measured error \
         means the verdict is \"revise\" ({} measured errors, {} warnings here). For every issue, name the size and, \
         where you can, the widget id, and propose a concrete fix to the card.\n\n\
         ## Rubric\n\n{}\n\n\
         Answer with JSON only, matching this schema:\n{}",
        report.summary.errors,
        report.summary.warnings,
        rubric.trim(),
        serde_json::to_string(&response_schema()).unwrap_or_default()
    )
}

/// Build the request from a report written in `report_dir`.
pub fn payload(
    report: &Report,
    report_dir: &Path,
    rubric: &str,
    opts: &CritiqueOptions,
) -> Result<Value, String> {
    let mut sizes = Vec::new();
    for (name, size) in &report.sizes {
        if !opts.sizes.is_empty() && !opts.sizes.contains(name) {
            continue;
        }
        let read = |rel: &Option<String>| rel.as_ref().map(|r| report_dir.join(r));
        let widgets = match (read(&size.snapshot), read(&size.tree)) {
            (Some(snap), Some(tree)) => {
                let snap: Value = serde_json::from_str(
                    &std::fs::read_to_string(&snap)
                        .map_err(|e| format!("{}: {e}", snap.display()))?,
                )
                .map_err(|e| e.to_string())?;
                let tree = std::fs::read_to_string(&tree)
                    .map_err(|e| format!("{}: {e}", tree.display()))?;
                let capture = Capture {
                    snap,
                    tree,
                    log: Vec::new(),
                };
                capture
                    .card_widgets()
                    .into_iter()
                    .map(|w| {
                        let mut v = json!({"id": w.id, "type": w.ty, "rect": w.rect.as_array(), "drawn": w.drawn});
                        if let Some(t) = w.text {
                            v["text"] = Value::String(t);
                        }
                        if let Some(p) = w.parent {
                            v["parent"] = Value::String(p);
                        }
                        v
                    })
                    .collect::<Vec<_>>()
            }
            _ => Vec::new(),
        };
        let image = match read(&size.png) {
            Some(png) => {
                let abs = std::fs::canonicalize(&png).unwrap_or(png.clone());
                let mut image = json!({"path": abs.to_string_lossy(), "media_type": "image/png"});
                if opts.inline_images {
                    let bytes =
                        std::fs::read(&png).map_err(|e| format!("{}: {e}", png.display()))?;
                    image["data_base64"] = Value::String(base64(&bytes));
                }
                image
            }
            None => Value::Null,
        };
        sizes.push(json!({
            "size": name,
            "kind": size.kind,
            "target": size.target,
            "viewport": size.viewport,
            "image": image,
            "widgets": widgets,
            "findings": size.findings,
            "metrics": size.metrics,
        }));
    }
    if sizes.is_empty() {
        return Err("no size in the report matched".into());
    }
    Ok(json!({
        "schema": REQUEST_SCHEMA,
        "prompt": prompt(rubric, report),
        "rubric": rubric,
        "response_schema": response_schema(),
        "measured": report.summary,
        "sizes": sizes,
    }))
}

/// Standard base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_rfc4648() {
        for (plain, enc) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(super::base64(plain.as_bytes()), enc);
        }
    }
}
