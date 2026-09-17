//! News on the AI bus: one read tool over the headlines on screen — the
//! same tool the standalone service answers over its port and the module
//! executor answers on the root at call time.

use crate::model::{plural, Headline, NewsModel, NO_HEADLINES};
use crate::view::NewsView;
use makepad_ai_services::wire::{Risk, ServiceCall, ServiceManifest, ToolDef, ToolResult};
use makepad_widgets::makepad_micro_serde::*;
use makepad_widgets::WidgetRef;

pub(crate) const DEFAULT_LIMIT: usize = 10;
pub(crate) const MAX_LIMIT: usize = 30;

/// The manifest shared by the standalone service and the module executor.
pub fn manifest() -> ServiceManifest {
    ServiceManifest::new(
        "news",
        "News",
        "Headlines on screen from Hacker News, TechMeme, Google News and the person's own feeds.",
    )
    .with_tool(ToolDef::new(
        "headlines",
        "The headlines on screen. `source` picks one tab by id or label (hn, techmeme, google, or a feed's label); omitted means every source interleaved by rank. `limit` caps the rows (default 10, max 30).",
        r#"{"type":"object","properties":{"source":{"type":"string","description":"A source id or label"},"limit":{"type":"integer","description":"Rows to return, 1-30"}}}"#,
        Risk::Read,
    ))
}

/// The tool's arguments; a model may send the limit as `10` or `10.0`.
#[derive(Clone, Debug, Default, DeJson)]
struct Args {
    source: Option<String>,
    limit: Option<f64>,
}

/// Answer one call against the model. Every branch answers: an unknown
/// tool or source, or arguments that do not parse, are refused with what
/// would have been accepted.
pub fn answer(model: &NewsModel, call: &ServiceCall) -> ToolResult {
    match call.tool.as_str() {
        "headlines" => {
            // No arguments means the defaults; malformed ones are refused
            // rather than quietly answered with everything.
            let args = if call.args.trim().is_empty() {
                Args::default()
            } else {
                match Args::deserialize_json_lenient(&call.args) {
                    Ok(args) => args,
                    Err(e) => return ToolResult::refused(&call.call_id, format!("bad arguments: {e:?}")),
                }
            };
            let limit = args.limit.map(|l| l.max(1.0) as usize);
            match headlines_text(model, args.source.as_deref(), limit) {
                Ok(text) => ToolResult::ok(&call.call_id, text, ""),
                Err(e) => ToolResult::refused(&call.call_id, e),
            }
        }
        other => ToolResult::refused(&call.call_id, format!("unknown tool `{other}`; this app only has `headlines`")),
    }
}

/// Answer one call on a `NewsView` root: what the module executor and the
/// standalone port both do. A root that is not (or no longer) a `NewsView`
/// answers Unavailable rather than panicking in the caller's event loop.
pub fn answer_root(root: &WidgetRef, call: &ServiceCall) -> ToolResult {
    root.borrow::<NewsView>()
        .map(|view| answer(view.model(), call))
        .unwrap_or_else(|| ToolResult::unavailable(&call.call_id, "the news view is gone"))
}

/// The rows of one tab as numbered text with links; an unknown source is
/// the error. An empty tab answers with its status.
pub(crate) fn headlines_text(model: &NewsModel, source: Option<&str>, limit: Option<usize>) -> Result<String, String> {
    let tab = match source {
        None => 0,
        Some(name) => model.source_index(name).map(|i| i + 1).ok_or_else(|| {
            let known: Vec<String> = model.sources.iter().map(|s| format!("{} ({})", s.def.id, s.def.label)).collect();
            format!("unknown source `{name}`; sources: {}", known.join(", "))
        })?,
    };
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let rows = model.rows_for_tab(tab);
    if rows.is_empty() {
        let status = model.status_text(tab);
        return Ok(if status == NO_HEADLINES { status } else { format!("No headlines: {status}") });
    }
    Ok(rows.iter().take(limit).enumerate().map(|(i, h)| row_text(i + 1, h)).collect::<Vec<_>>().join("\n"))
}

/// `3. Title — Source (12 points, 4 comments)` over its link.
fn row_text(number: usize, row: &Headline) -> String {
    let mut line = format!("{number}. {}", row.title);
    if !row.source.is_empty() {
        line.push_str(&format!(" — {}", row.source));
    }
    let mut extras = Vec::new();
    if let Some(p) = row.points {
        extras.push(plural(p, "point"));
    }
    if let Some(c) = row.comments {
        extras.push(plural(c, "comment"));
    }
    if !extras.is_empty() {
        line.push_str(&format!(" ({})", extras.join(", ")));
    }
    format!("{line}\n   {}", row.link)
}

/// One line of volatile context for the assistant: the tab and its status.
pub(crate) fn context_line(model: &NewsModel) -> String {
    format!("News: {} tab, {} rows, {}", model.tab_label(model.tab), model.row_count(model.tab), model.status_text(model.tab))
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_ai_services::wire::ToolOutcome;

    fn model_with_rows() -> NewsModel {
        let mut m = NewsModel::new();
        for (i, prefix) in ["h", "t", "g"].iter().enumerate() {
            let rows = (1..=3)
                .map(|n| Headline {
                    title: format!("{prefix}{n}"),
                    link: format!("https://x/{prefix}{n}"),
                    source: "S".into(),
                    points: Some(n),
                    ..Default::default()
                })
                .collect();
            let (id, _) = m.begin_fetch(i);
            m.complete(id, Ok(rows));
        }
        m
    }

    fn call(args: &str) -> ServiceCall {
        ServiceCall { call_id: "c1".into(), tool: "headlines".into(), args: args.into() }
    }

    #[test]
    fn the_manifest_declares_one_read_tool() {
        let m = manifest();
        assert_eq!(m.id, "news");
        assert!(m.validate().is_ok());
        assert_eq!(m.tools.len(), 1);
        assert_eq!(m.tools[0].name, "headlines");
        assert_eq!(m.tools[0].risk, Risk::Read);
    }

    #[test]
    fn headlines_lists_all_interleaved_by_default_and_one_source_on_request() {
        let m = model_with_rows();
        let text = headlines_text(&m, None, None).unwrap();
        assert!(text.starts_with("1. h1 — S (1 point)\n   https://x/h1\n2. t1"), "{text}");
        assert_eq!(text.lines().count(), 18, "9 rows, two lines each");
        let text = headlines_text(&m, Some("techmeme"), Some(2)).unwrap();
        assert_eq!(text.lines().count(), 4);
        assert!(text.contains("t1") && text.contains("t2") && !text.contains("t3"));
        let err = headlines_text(&m, Some("nope"), None).unwrap_err();
        assert!(err.contains("hn") && err.contains("techmeme") && err.contains("google"), "{err}");
        assert_eq!(headlines_text(&NewsModel::new(), None, None).unwrap(), "No headlines yet");
        let mut failed = NewsModel::new();
        let (id, _) = failed.begin_fetch(0);
        failed.complete(id, Err("HTTP 500".into()));
        assert_eq!(headlines_text(&failed, Some("hn"), None).unwrap(), "No headlines: Unavailable");
    }

    #[test]
    fn answer_parses_arguments_and_refuses_unknown_tools() {
        let m = model_with_rows();
        let ok = answer(&m, &call(r#"{"source": "hn", "limit": 1}"#));
        assert_eq!(ok.outcome, ToolOutcome::Ok);
        assert!(ok.text.contains("h1") && !ok.text.contains("h2"), "{}", ok.text);
        let ok = answer(&m, &call(""));
        assert!(ok.text.contains("g3"), "empty args mean everything: {}", ok.text);
        let ok = answer(&m, &call("  \n"));
        assert!(ok.text.contains("g3"), "so do blank args: {}", ok.text);
        let bad = answer(&m, &call(r#"{"source":"hn","limit":"5"}"#));
        assert_eq!(bad.outcome, ToolOutcome::Refused, "a malformed limit is not silently every source");
        assert!(bad.text.contains("bad arguments"), "{}", bad.text);
        let bad = answer(&m, &call(r#"{"source":"nope"}"#));
        assert_eq!(bad.outcome, ToolOutcome::Refused);
        assert!(bad.text.contains("unknown source"), "{}", bad.text);
        let bad = answer(&m, &ServiceCall { call_id: "c2".into(), tool: "weather".into(), args: String::new() });
        assert_eq!(bad.outcome, ToolOutcome::Refused);
        assert!(bad.text.contains("headlines"), "the refusal names the one tool that exists: {}", bad.text);
    }

    #[test]
    fn answer_root_reports_a_missing_view_as_unavailable() {
        let result = answer_root(&WidgetRef::empty(), &call(""));
        assert_eq!(result.outcome, ToolOutcome::Unavailable);
        assert!(result.text.contains("gone"), "{}", result.text);
    }

    #[test]
    fn the_context_line_names_the_tab_its_rows_and_status() {
        let mut m = model_with_rows();
        assert_eq!(context_line(&m), "News: Today tab, 9 rows, Updated just now");
        m.select_tab(2);
        assert_eq!(context_line(&m), "News: TechMeme tab, 3 rows, Updated just now");
    }
}
