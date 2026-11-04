//! App Hub's tools for an agent: read only, from the catalog snapshot the
//! view shows (the signed catalog, verified). Installing, updating and
//! removing stay on App Hub's own screens and their consent sheets.
//!
//! - `search` `{query, category?}`: catalog entries matching the words, as
//!   App Hub's own search finds them;
//! - `installed` `{}`: the person's library;
//! - `updates` `{}`: installed apps the catalog has a newer release of.
//!
//! Each answer is a short text for the model plus the rows as JSON `data`.

use crate::catalog::{filter_entries, CatalogSnapshot, Entry, EntryStatus};
use makepad_app_module::makepad_ai_services::wire::{Risk, ServiceCall, ServiceManifest, ToolDef, ToolResult};
use serde_json::{json, Value};

/// The most rows one answer carries.
pub const MAX_ROWS: usize = 25;

pub fn manifest() -> ServiceManifest {
    ServiceManifest::new("apphub", "App Hub", "Discover and install OctoSense apps")
        .with_tool(ToolDef::new(
            "search",
            "Find apps in the signed OctoSense catalog whose name, description, category or publisher match every word of the query. Answers each app's id, name, subtitle, category, publisher, version and status (available, installed, update available, built in or unavailable). Does not install anything.",
            r#"{"type":"object","properties":{"query":{"type":"string","maxLength":200},"category":{"type":"string","maxLength":64}},"required":["query"],"additionalProperties":false}"#,
            Risk::Read,
        ))
        .with_tool(ToolDef::new(
            "installed",
            "List the apps installed from App Hub: id, name, version and whether an update is available.",
            r#"{"type":"object","properties":{},"additionalProperties":false}"#,
            Risk::Read,
        ))
        .with_tool(ToolDef::new(
            "updates",
            "List the installed apps that have a newer release in the catalog: id, name and the catalog's version. Does not update anything.",
            r#"{"type":"object","properties":{},"additionalProperties":false}"#,
            Risk::Read,
        ))
}

/// Answer one App Hub call from `snapshot` (`None`: not loaded yet).
pub fn answer(snapshot: Option<&CatalogSnapshot>, call: &ServiceCall) -> ToolResult {
    let args: Value = match serde_json::from_str(if call.args.trim().is_empty() { "{}" } else { &call.args }) {
        Ok(value @ Value::Object(_)) => value,
        _ => return ToolResult::refused(&call.call_id, "the arguments must be a JSON object"),
    };
    let Some(snapshot) = snapshot else {
        return ToolResult::unavailable(&call.call_id, "App Hub has not loaded the catalog yet");
    };
    match call.tool.as_str() {
        "search" => {
            let Some(query) = args.get("query").and_then(Value::as_str) else {
                return ToolResult::refused(&call.call_id, "search needs a query");
            };
            let category = args.get("category").and_then(Value::as_str);
            let found = filter_entries(&snapshot.entries, query, category);
            rows_result(&call.call_id, &found, &format!("{} app(s) match {query:?}", found.len()))
        }
        "installed" => rows_result(&call.call_id, &snapshot.library, &format!("{} app(s) installed", snapshot.library.len())),
        "updates" => {
            let updates: Vec<Entry> = snapshot.library.iter().filter(|e| matches!(e.status, EntryStatus::UpdateAvailable)).cloned().collect();
            rows_result(&call.call_id, &updates, &format!("{} update(s) available", updates.len()))
        }
        other => ToolResult::refused(&call.call_id, format!("App Hub has no tool {other:?}")),
    }
}

fn rows_result(call_id: &str, entries: &[Entry], note: &str) -> ToolResult {
    let rows: Vec<Value> = entries.iter().take(MAX_ROWS).map(row).collect();
    let mut text = entries
        .iter()
        .take(MAX_ROWS)
        .map(|e| format!("{} ({}) {} — {}", e.name, e.id, e.version, status(&e.status)))
        .collect::<Vec<_>>()
        .join("\n");
    if entries.len() > MAX_ROWS {
        text.push_str(&format!("\n… and {} more", entries.len() - MAX_ROWS));
    }
    if text.is_empty() {
        text = "none".into();
    }
    let mut result = ToolResult::ok(call_id, text, note);
    result.data = json!({ "apps": rows, "total": entries.len() }).to_string();
    result
}

fn row(e: &Entry) -> Value {
    json!({
        "id": e.id,
        "name": e.name,
        "subtitle": e.subtitle,
        "category": e.category,
        "publisher": e.publisher,
        "version": e.version,
        "status": status(&e.status),
    })
}

fn status(status: &EntryStatus) -> &'static str {
    match status {
        EntryStatus::Available => "available",
        EntryStatus::Installed => "installed",
        EntryStatus::UpdateAvailable => "update available",
        EntryStatus::Unavailable(_) => "unavailable",
        EntryStatus::BuiltIn => "built in",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_app_module::makepad_ai_services::wire::ToolOutcome;

    fn entry(id: &str, name: &str, category: &str, status: EntryStatus) -> Entry {
        let mut e = crate::catalog::preview_entries().into_iter().next().expect("a preview entry");
        e.id = id.into();
        e.name = name.into();
        e.category = category.into();
        e.status = status;
        e
    }

    fn snapshot() -> CatalogSnapshot {
        let timer = entry("org.example.timer", "Timer", "Utilities", EntryStatus::Installed);
        let notes = entry("org.example.memo", "Memo", "Productivity", EntryStatus::UpdateAvailable);
        let maps = entry("org.example.trails", "Trails", "Travel", EntryStatus::Available);
        CatalogSnapshot { entries: vec![timer.clone(), notes.clone(), maps], library: vec![timer, notes], ..Default::default() }
    }

    fn call(tool: &str, args: &str) -> ServiceCall {
        ServiceCall { call_id: "c1".into(), tool: tool.into(), args: args.into() }
    }

    fn ids(result: &ToolResult) -> Vec<String> {
        let data: Value = serde_json::from_str(&result.data).unwrap();
        data["apps"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn the_manifest_offers_three_read_tools() {
        let manifest = manifest();
        manifest.validate().expect("a valid manifest");
        assert_eq!(manifest.tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["search", "installed", "updates"]);
        assert!(manifest.tools.iter().all(|t| t.risk == Risk::Read), "App Hub's agent only reads");
    }

    #[test]
    fn search_installed_and_updates_answer_from_the_catalog() {
        let snap = snapshot();
        let found = answer(Some(&snap), &call("search", r#"{"query":"timer"}"#));
        assert_eq!(found.outcome, ToolOutcome::Ok);
        assert_eq!(ids(&found), ["org.example.timer"]);
        let travel = answer(Some(&snap), &call("search", r#"{"query":"","category":"travel"}"#));
        assert_eq!(ids(&travel), ["org.example.trails"]);
        assert_eq!(ids(&answer(Some(&snap), &call("installed", "{}"))), ["org.example.timer", "org.example.memo"]);
        let updates = answer(Some(&snap), &call("updates", ""));
        assert_eq!(ids(&updates), ["org.example.memo"]);
        assert!(updates.text.contains("update available"), "{}", updates.text);
    }

    #[test]
    fn unknown_tools_bad_arguments_and_no_catalog_are_answered_not_guessed() {
        let snap = snapshot();
        assert_eq!(answer(Some(&snap), &call("install", r#"{"id":"org.example.trails"}"#)).outcome, ToolOutcome::Refused);
        assert_eq!(answer(Some(&snap), &call("search", "{}")).outcome, ToolOutcome::Refused);
        assert_eq!(answer(Some(&snap), &call("search", "[1]")).outcome, ToolOutcome::Refused);
        assert_eq!(answer(None, &call("installed", "{}")).outcome, ToolOutcome::Unavailable);
    }
}
