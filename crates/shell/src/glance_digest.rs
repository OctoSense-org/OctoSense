//! `sys.digest(app:, id:)` for glance cards (ADR 0002 §7.1, M4): a card binds
//! to a digest the host holds for its own app, with provenance, instead of
//! carrying the findings as text.
//!
//! **Where digests live.** A digest is a system-toolbox run result
//! (`crates/toolbox`, `RunOptions::write_result`) at
//! `<app folder>/toolbox/runs/<template>/<run>.json`. The app folder the
//! shell gives the toolbox is host-owned: `<apps root>/.host/toolbox/<app id>`
//! ([`digest_root`]), outside the app's jail (`<apps root>/<app id>`). A file
//! the app could write itself would let it forge a digest, citations and all,
//! so the resolver never reads from the jail. The shell reads the file; it
//! takes no crate dependency on the toolbox, only on the minimal shape below.
//!
//! **Binding.** `app` must name the PUBLISHER, which is the caller
//! (`glance::Caller`), never an argument: the app's id or its launcher id
//! (`news` for `os.news`). A card naming another app is refused at publish.
//! `id` is a run id, `[A-Za-z0-9_-]{1,64}`, looked up under each of the app's
//! template folders (the newest file wins when two templates share an id).
//! The file's own `app_id` and `run_id` must agree. An agent that republishes
//! the same card after each run uses a fixed caller-chosen run id (`glance`),
//! so the file is replaced in place and the card's source does not change.
//!
//! **The value.** One shape, whatever template produced the run
//! (news-digest's `data.digest`, topic-brief's `data.brief`, …):
//!
//! ```text
//! {status, topic, language, summary, retrieved_at, count,
//!  points:  [{id, text, label, cite, citations: [source index]}],
//!  sources: [{id, n, title, source, url, published_at}]}
//! ```
//!
//! `status` is the run's (`ready`, `partial`, `failed`) or the resolver's
//! (`missing`, `expired`); the card's `$state` is `.ready` for a ready or
//! partial run and `.failed` otherwise, and a missing, expired or unreadable
//! digest is an empty record, never an error that fails the card.
//!
//! **Provenance.** A source is kept only when the run's host-kept
//! `provenance` lists its id with the same URL, and only an `http(s)` URL;
//! a point is kept only when it still cites a kept source, and its
//! `citations` become indexes into `sources` (`cite` is the same list,
//! 1-based, as text: a card has no arithmetic). Model text (`summary`,
//! `text`, `label`) never carries a URL: a point that does is dropped, a
//! summary that does is emptied. Links come only from `sources[].url`.
//!
//! **Caps.** The file is read only up to [`FILE_MAX`]; [`POINTS_MAX`] points
//! of [`TEXT_MAX`] characters, [`SOURCES_MAX`] sources, a [`SUMMARY_MAX`]
//! summary; longer text is cut at a character boundary.
//!
//! **Expiry.** A digest is current for [`MAX_AGE_MS`] from the run's
//! `started_at`; older, it resolves `expired`. A card bound to a current
//! digest expires no later than the digest does.
//!
//! **Retention** ([`retain`]): after a card that binds digests is published,
//! each of the app's template folders keeps its [`KEEP_PER_TEMPLATE`] newest
//! results plus every run a live glance card of the app binds; the rest are
//! deleted. Nothing else in the shell deletes run results.
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// The largest run-result file read.
pub const FILE_MAX: u64 = 512 * 1024;
pub const SUMMARY_MAX: usize = 800;
pub const POINTS_MAX: usize = 8;
pub const TEXT_MAX: usize = 400;
pub const LABEL_MAX: usize = 40;
pub const SOURCES_MAX: usize = 8;
pub const TITLE_MAX: usize = 200;
pub const NAME_MAX: usize = 80;
pub const URL_MAX: usize = 2048;
pub const CITATIONS_MAX: usize = 8;
/// Template folders scanned for one id.
pub const TEMPLATES_MAX: usize = 64;
/// How long a digest is current, from the run's start.
pub const MAX_AGE_MS: u64 = 48 * 3600 * 1000;
/// Results kept per template folder, besides those a live card binds.
pub const KEEP_PER_TEMPLATE: usize = 8;

/// A run id, as the toolbox validates it.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

/// An app id that is safe as one path segment.
fn valid_app(app: &str) -> bool {
    !app.is_empty() && app.len() <= 64 && !app.starts_with('.') && app.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Where the host keeps every app's toolbox folder: `<apps root>/.host/toolbox`
/// (`octosense_ai_host::toolbox_root`), the folder the system toolbox writes
/// its run results under (host_tools/toolbox.rs). None until the host's
/// storage knows its apps root.
pub fn digest_root() -> Option<PathBuf> {
    crate::app_storage::host().map(|storage| octosense_ai_host::toolbox_root(storage.layout().apps_root()))
}

/// `<root>/<app>/toolbox/runs`.
pub fn runs_dir(root: &Path, app: &str) -> Option<PathBuf> {
    valid_app(app).then(|| root.join(app).join("toolbox").join("runs"))
}

/// A resolved digest: the value the card binds, and its lifecycle.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub value: Value,
    /// The `$status` token: `ready` or `failed`.
    pub state: &'static str,
    /// When the digest stops being current (ready digests only).
    pub expires_ms: Option<u64>,
}

fn empty(status: &str) -> Resolved {
    Resolved {
        value: json!({"status": status, "topic": "", "language": "", "summary": "", "retrieved_at": "", "count": 0, "points": [], "sources": []}),
        state: "failed",
        expires_ms: None,
    }
}

/// Resolve `app`'s digest `id` under `root` at `now_ms`. Never fails: a
/// digest that is missing, expired, oversized or malformed is an empty
/// record whose `status` says which.
pub fn resolve(root: Option<&Path>, app: &str, id: &str, now_ms: u64) -> Resolved {
    let Some(runs) = root.and_then(|r| runs_dir(r, app)) else {
        return empty("missing");
    };
    if !valid_id(id) {
        return empty("missing");
    }
    let Some(path) = find(&runs, id) else {
        return empty("missing");
    };
    let Some(result) = read(&path) else {
        return empty("failed");
    };
    if result.get("app_id").and_then(Value::as_str) != Some(app) || result.get("run_id").and_then(Value::as_str) != Some(id) {
        return empty("failed");
    }
    let started = result.get("started_at").and_then(Value::as_str).and_then(parse_rfc3339_ms);
    let Some(started) = started else {
        return empty("failed");
    };
    let expires = started.saturating_add(MAX_AGE_MS);
    if expires <= now_ms {
        return empty("expired");
    }
    normalize(&result, expires)
}

/// The newest `<runs>/<template>/<id>.json`.
fn find(runs: &Path, id: &str) -> Option<PathBuf> {
    let name = format!("{id}.json");
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(runs).ok()?.flatten().take(TEMPLATES_MAX) {
        let path = entry.path().join(&name);
        // No symlinks: the file must be the host's own, where it says it is.
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if !meta.is_file() || !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        if best.as_ref().is_none_or(|(t, _)| modified > *t) {
            best = Some((modified, path));
        }
    }
    best.map(|(_, p)| p)
}

fn read(path: &Path) -> Option<Value> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > FILE_MAX {
        return None;
    }
    let mut text = String::new();
    file.take(FILE_MAX + 1).read_to_string(&mut text).ok()?;
    serde_json::from_str(&text).ok()
}

fn contains_url(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("http://") || lower.contains("https://") || lower.contains("www.")
}

fn cut(text: &str, max: usize) -> String {
    let text = text.trim();
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", text[..at].trim_end()),
        None => text.to_string(),
    }
}

fn string(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// The template-independent record from one run result.
fn normalize(result: &Value, expires_ms: u64) -> Resolved {
    let status = match result.get("status").and_then(Value::as_str) {
        Some(s @ ("ready" | "partial" | "failed")) => s,
        _ => return empty("failed"),
    };
    let data = result.get("data").cloned().unwrap_or(Value::Null);
    // The template's findings: news-digest's `digest`, topic-brief's
    // `brief`, or a template whose data is the findings itself.
    let findings = ["digest", "brief"].iter().find_map(|k| data.get(*k).filter(|v| v.is_object())).cloned().unwrap_or_else(|| if data.get("points").is_some() { data.clone() } else { Value::Null });

    // Host-kept provenance: id -> (url, retrieved_at).
    let provenance: Vec<(String, String, String)> = result
        .get("provenance")
        .and_then(Value::as_array)
        .map(|p| p.iter().map(|e| (string(e, "id"), string(e, "url"), string(e, "retrieved_at"))).collect())
        .unwrap_or_default();

    let mut sources: Vec<Value> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    let mut retrieved: Option<(u64, String)> = None;
    for s in data.get("sources").and_then(Value::as_array).into_iter().flatten() {
        if sources.len() >= SOURCES_MAX {
            break;
        }
        let (id, url) = (string(s, "id"), string(s, "url"));
        let lower = url.to_ascii_lowercase();
        if id.is_empty() || url.len() > URL_MAX || !(lower.starts_with("https://") || lower.starts_with("http://")) || ids.contains(&id) {
            continue;
        }
        let Some((_, _, at)) = provenance.iter().find(|(pid, purl, _)| *pid == id && *purl == url) else { continue };
        if let Some(ms) = parse_rfc3339_ms(at) {
            if retrieved.as_ref().is_none_or(|(best, _)| ms > *best) {
                retrieved = Some((ms, at.clone()));
            }
        }
        ids.push(id.clone());
        sources.push(json!({
            "id": id,
            "n": sources.len().saturating_add(1).to_string(),
            "title": cut(&string(s, "title"), TITLE_MAX),
            "source": cut(&string(s, "source"), NAME_MAX),
            "url": url,
            "published_at": cut(&string(s, "published_at"), 40),
        }));
    }

    let mut points: Vec<Value> = Vec::new();
    for p in findings.get("points").and_then(Value::as_array).into_iter().flatten() {
        if points.len() >= POINTS_MAX {
            break;
        }
        let text = string(p, "text");
        let label = string(p, "label");
        if text.trim().is_empty() || contains_url(&text) || contains_url(&label) {
            continue;
        }
        let mut citations: Vec<usize> = Vec::new();
        for c in p.get("citations").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            if let Some(i) = ids.iter().position(|id| id == c) {
                if !citations.contains(&i) && citations.len() < CITATIONS_MAX {
                    citations.push(i);
                }
            }
        }
        if citations.is_empty() {
            continue; // an uncited point states something no source backs
        }
        let cite = citations.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(", ");
        points.push(json!({
            "id": format!("p{}", points.len() + 1),
            "text": cut(&text, TEXT_MAX),
            "label": cut(&label, LABEL_MAX),
            "cite": cite,
            "citations": citations,
        }));
    }

    let summary = string(&findings, "summary");
    let summary = if contains_url(&summary) { String::new() } else { cut(&summary, SUMMARY_MAX) };
    let language = Some(string(&findings, "language")).filter(|l| !l.is_empty()).unwrap_or_else(|| string(&data, "language"));
    let retrieved_at = retrieved.map(|(_, at)| at).unwrap_or_else(|| string(result, "started_at"));
    let topic = string(&data, "topic");
    let mut value = Map::new();
    value.insert("status".into(), json!(status));
    value.insert("topic".into(), json!(if contains_url(&topic) { String::new() } else { cut(&topic, TITLE_MAX) }));
    value.insert("language".into(), json!(cut(&language, 16)));
    value.insert("summary".into(), json!(summary));
    value.insert("retrieved_at".into(), json!(retrieved_at));
    value.insert("count".into(), json!(points.len()));
    value.insert("points".into(), Value::Array(points));
    value.insert("sources".into(), Value::Array(sources));
    Resolved {
        value: Value::Object(value),
        state: if status == "failed" { "failed" } else { "ready" },
        expires_ms: (status != "failed").then_some(expires_ms),
    }
}

/// Keep each of `app`'s template folders to its [`KEEP_PER_TEMPLATE`]
/// newest results plus the `pinned` run ids; delete the rest. Returns how
/// many files went.
pub fn retain(root: &Path, app: &str, pinned: &[String]) -> usize {
    let Some(runs) = runs_dir(root, app) else { return 0 };
    let Ok(templates) = std::fs::read_dir(&runs) else { return 0 };
    let mut removed = 0;
    for template in templates.flatten().take(TEMPLATES_MAX) {
        if !template.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Ok(files) = std::fs::read_dir(template.path()) else { continue };
        let mut results: Vec<(std::time::SystemTime, String, PathBuf)> = files
            .flatten()
            .filter_map(|f| {
                let path = f.path();
                let id = path.file_name()?.to_str()?.strip_suffix(".json")?.to_string();
                let meta = std::fs::symlink_metadata(&path).ok()?;
                (meta.is_file() && valid_id(&id)).then(|| (meta.modified().unwrap_or(std::time::UNIX_EPOCH), id, path))
            })
            .collect();
        results.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
        for (_, id, path) in results.into_iter().skip(KEEP_PER_TEMPLATE) {
            if !pinned.contains(&id) && std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

/// Milliseconds since the epoch for an RFC 3339 time (`Z` or an offset;
/// fractional seconds kept to the millisecond). None when malformed.
pub fn parse_rfc3339_ms(text: &str) -> Option<u64> {
    let b = text.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> { text.get(from..to)?.parse::<i64>().ok() };
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let mut rest = &text[19..];
    let mut millis = 0i64;
    if let Some(frac) = rest.strip_prefix('.') {
        let digits = frac.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        let ms = format!("{:0<3}", &frac[..digits.min(3)]);
        millis = ms.parse().ok()?;
        rest = &frac[digits..];
    }
    let offset_s = match rest {
        "Z" | "z" => 0,
        _ if rest.len() == 6 && matches!(rest.as_bytes()[0], b'+' | b'-') && rest.as_bytes()[3] == b':' => {
            let sign = if rest.starts_with('-') { -1 } else { 1 };
            sign * (rest.get(1..3)?.parse::<i64>().ok()? * 3600 + rest.get(4..6)?.parse::<i64>().ok()? * 60)
        }
        _ => return None,
    };
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + hour * 3600 + minute * 60 + second - offset_s;
    u64::try_from(secs * 1000 + millis).ok()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const NOW: &str = "2026-09-20T09:00:00Z";

    /// A news-digest run result as the toolbox writes it (fixtures below).
    pub fn news_digest_result() -> Value {
        serde_json::from_str(include_str!("../resources/glance/fixtures/news-digest-run.json")).unwrap()
    }

    pub fn now() -> u64 {
        parse_rfc3339_ms(NOW).unwrap()
    }

    /// A fresh digest root under the target dir, with `result` written as
    /// `<app>/toolbox/runs/<template>/<run>.json`.
    pub fn root_with(name: &str, results: &[(&str, &str, Value)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("octosense-digest-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (app, template, result) in results {
            let dir = root.join(app).join("toolbox/runs").join(template);
            std::fs::create_dir_all(&dir).unwrap();
            let run = result["run_id"].as_str().unwrap();
            std::fs::write(dir.join(format!("{run}.json")), serde_json::to_string_pretty(result).unwrap()).unwrap();
        }
        root
    }

    #[test]
    fn a_news_digest_run_resolves_to_the_stable_shape() {
        let root = root_with("shape", &[("os.news", "news-digest", news_digest_result())]);
        let r = resolve(Some(&root), "os.news", "glance", now());
        assert_eq!(r.state, "ready");
        let v = &r.value;
        assert_eq!(v["status"], "ready");
        assert_eq!(v["topic"], "city infrastructure");
        assert_eq!(v["language"], "en");
        assert_eq!(v["count"], 3);
        assert_eq!(v["retrieved_at"], "2026-09-20T08:00:03Z");
        assert_eq!(v["sources"].as_array().unwrap().len(), 3);
        assert_eq!(v["sources"][1], json!({"id": "s39327104dbcc", "n": "2", "title": "Ocho bibliotecas amplían su horario durante un piloto", "source": "Diario de Puerto Claro", "url": "https://example.invalid/news/libraries", "published_at": "2026-09-19T08:30:00Z"}));
        assert_eq!(v["points"][0]["citations"], json!([0]));
        assert_eq!(v["points"][2]["cite"], "3");
        assert_eq!(v["points"][0]["id"], "p1");
        // No evidence hash, provenance or trace reaches the card.
        assert!(!v.to_string().contains("evidence_sha256") && !v.to_string().contains("trace"));
        assert_eq!(r.expires_ms, Some(parse_rfc3339_ms("2026-09-20T08:00:00Z").unwrap() + MAX_AGE_MS));
    }

    #[test]
    fn a_topic_brief_resolves_to_the_same_shape() {
        let mut result = news_digest_result();
        let digest = result["data"]["digest"].take();
        let data = result["data"].as_object_mut().unwrap();
        data.remove("digest");
        data.insert("brief".into(), digest);
        data.insert("queries".into(), json!([{"terms": "city infrastructure", "language": "en", "found": 3, "read": 3, "unreadable": 0}]));
        result["template"]["id"] = json!("topic-brief");
        let root = root_with("brief", &[("os.news", "topic-brief", result)]);
        let r = resolve(Some(&root), "os.news", "glance", now());
        assert_eq!((r.state, r.value["count"].as_u64()), ("ready", Some(3)));
        let keys: Vec<&String> = r.value.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["count", "language", "points", "retrieved_at", "sources", "status", "summary", "topic"]);
    }

    #[test]
    fn missing_expired_and_malformed_digests_are_empty_records() {
        let root = root_with("absent", &[("os.news", "news-digest", news_digest_result())]);
        for (app, id, status) in [("os.news", "nope", "missing"), ("os.mail", "glance", "missing"), ("os.news", "../x", "missing"), ("..", "glance", "missing"), ("os.news", "", "missing")] {
            let r = resolve(Some(&root), app, id, now());
            assert_eq!((r.state, r.value["status"].as_str()), ("failed", Some(status)), "{app} {id}");
            assert_eq!(r.value["points"], json!([]));
        }
        assert_eq!(resolve(None, "os.news", "glance", now()).value["status"], "missing");
        let late = parse_rfc3339_ms("2026-09-22T08:00:00Z").unwrap();
        assert_eq!(resolve(Some(&root), "os.news", "glance", late).value["status"], "expired");
        // A file that is not JSON, names another app or run, or is too big.
        let mut other = news_digest_result();
        other["app_id"] = json!("os.mail");
        let root = root_with("malformed", &[("os.news", "news-digest", other)]);
        assert_eq!(resolve(Some(&root), "os.news", "glance", now()).value["status"], "failed");
        let dir = root.join("os.news/toolbox/runs/news-digest");
        std::fs::write(dir.join("junk.json"), "{not json").unwrap();
        assert_eq!(resolve(Some(&root), "os.news", "junk", now()).value["status"], "failed");
        std::fs::write(dir.join("big.json"), " ".repeat(FILE_MAX as usize + 1)).unwrap();
        assert_eq!(resolve(Some(&root), "os.news", "big", now()).value["status"], "failed");
    }

    #[test]
    fn links_come_only_from_host_provenance_and_text_carries_none() {
        let mut result = news_digest_result();
        // A source whose URL the host never retrieved, one with a script URL.
        result["data"]["sources"][0]["url"] = json!("https://evil.invalid/x");
        result["data"]["sources"][2]["url"] = json!("javascript:alert(1)");
        result["provenance"][2]["url"] = json!("javascript:alert(1)");
        result["data"]["digest"]["points"][1]["text"] = json!("Read more at www.example.invalid");
        result["data"]["digest"]["summary"] = json!("See https://example.invalid for more");
        let root = root_with("links", &[("os.news", "news-digest", result)]);
        let v = resolve(Some(&root), "os.news", "glance", now()).value;
        let sources = v["sources"].as_array().unwrap();
        assert_eq!(sources.len(), 1, "{v}");
        assert_eq!((sources[0]["id"].as_str(), sources[0]["n"].as_str()), (Some("s39327104dbcc"), Some("1")));
        // Point 1 cited a dropped source, point 2 carried a URL; neither stays.
        assert_eq!(v["points"], json!([]));
        assert_eq!(v["summary"], "");
    }

    #[test]
    fn text_is_capped() {
        let mut result = news_digest_result();
        result["data"]["digest"]["summary"] = json!("s".repeat(SUMMARY_MAX * 2));
        let point = result["data"]["digest"]["points"][0].clone();
        let points: Vec<Value> = (0..POINTS_MAX + 4).map(|i| { let mut p = point.clone(); p["text"] = json!(format!("{i} {}", "é".repeat(TEXT_MAX))); p }).collect();
        result["data"]["digest"]["points"] = json!(points);
        let root = root_with("caps", &[("os.news", "news-digest", result)]);
        let v = resolve(Some(&root), "os.news", "glance", now()).value;
        assert_eq!(v["summary"].as_str().unwrap().chars().count(), SUMMARY_MAX + 1);
        assert_eq!(v["points"].as_array().unwrap().len(), POINTS_MAX);
        assert_eq!(v["points"][0]["text"].as_str().unwrap().chars().count(), TEXT_MAX + 1);
    }

    #[test]
    fn a_failed_run_is_a_failed_state_with_what_it_has() {
        let mut result = news_digest_result();
        result["status"] = json!("failed");
        result["data"] = Value::Null;
        let root = root_with("failedrun", &[("os.news", "news-digest", result)]);
        let r = resolve(Some(&root), "os.news", "glance", now());
        assert_eq!((r.state, r.value["status"].as_str(), r.expires_ms), ("failed", Some("failed"), None));
    }

    #[test]
    fn retention_keeps_the_newest_and_the_pinned() {
        let root = root_with("retain", &[]);
        let dir = root.join("os.news/toolbox/runs/news-digest");
        std::fs::create_dir_all(&dir).unwrap();
        let total = KEEP_PER_TEMPLATE + 4;
        for i in 0..total {
            let path = dir.join(format!("r{i:02}.json"));
            std::fs::write(&path, "{}").unwrap();
            let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000 + i as u64);
            std::fs::File::options().write(true).open(&path).unwrap().set_modified(t).unwrap();
        }
        std::fs::write(dir.join("notes.txt"), "kept: not a result").unwrap();
        assert_eq!(retain(&root, "os.news", &["r00".into()]), 3);
        let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left.len(), KEEP_PER_TEMPLATE + 2);
        assert!(left.contains(&"r00.json".into()) && left.contains(&"notes.txt".into()) && !left.contains(&"r01.json".into()));
        assert_eq!(retain(&root, "../etc", &[]), 0);
    }

    #[test]
    fn rfc3339_times_parse() {
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339_ms("2026-09-20T08:00:00.250Z"), Some(1_789_891_200_250));
        assert_eq!(parse_rfc3339_ms("2026-09-20T10:00:00+02:00"), parse_rfc3339_ms("2026-09-20T08:00:00Z"));
        assert_eq!(parse_rfc3339_ms("2026-02-30"), None);
        assert_eq!(parse_rfc3339_ms("yesterday"), None);
    }
}
