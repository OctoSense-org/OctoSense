//! The news model: sources, headline rows, the person's own feeds file, the
//! All interleave, and the per-source fetch state machine (loading, failed,
//! stale-but-kept, cached) both faces render from.
use makepad_widgets::makepad_micro_serde::*;
use makepad_widgets::LiveId;
use std::ops::Range;
use std::time::{Duration, Instant};

pub(crate) const REFRESH_EVERY: Duration = Duration::from_secs(15 * 60);
/// A front page or a feed is tens of kilobytes; anything past this is not one.
pub const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const MAX_ROWS_PER_SOURCE: usize = 30;
pub const MAX_USER_FEEDS: usize = 4;
pub(crate) const TILE_ROWS: usize = 3;
pub(crate) const SUMMARY_CHARS: usize = 280;
/// The one place the built-in source's name is spelled: its tab and its rows.
pub(crate) const HN_LABEL: &str = "Hacker News";
/// The storage key of the person's own feeds: `[{"label": "...", "url": "..."}]`.
/// Read once, when the view starts: an edit shows after a restart.
pub const FEEDS_KEY: &str = "feeds.json";
/// The status of a tab that has nothing yet and nothing wrong.
pub(crate) const NO_HEADLINES: &str = "No headlines yet";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// Algolia's front-page search: one JSON page with points and comments.
    HackerNews,
    /// RSS or Atom. `split_source` takes the trailing " - Source" off Google
    /// News titles into the row's source field.
    Feed { split_source: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDef {
    /// Stable id: the cache key suffix and the tool's `source` argument. A
    /// built-in has a short name (`hn`); a user feed's is derived from its
    /// URL, so a feed replaced by another never inherits its cache.
    pub id: String,
    pub label: String,
    pub url: String,
    pub kind: SourceKind,
}

pub(crate) fn builtin_sources() -> Vec<SourceDef> {
    vec![
        SourceDef {
            id: "hn".into(),
            label: HN_LABEL.into(),
            url: "https://hn.algolia.com/api/v1/search?tags=front_page&hitsPerPage=30".into(),
            kind: SourceKind::HackerNews,
        },
        SourceDef {
            id: "techmeme".into(),
            label: "TechMeme".into(),
            url: "https://www.techmeme.com/feed.xml".into(),
            kind: SourceKind::Feed { split_source: false },
        },
        SourceDef {
            id: "google".into(),
            label: "Google News".into(),
            url: "https://news.google.com/rss?hl=en-US&gl=US&ceid=US:en".into(),
            kind: SourceKind::Feed { split_source: true },
        },
    ]
}

/// One row, whatever the source.
#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct Headline {
    pub title: String,
    pub link: String,
    pub source: String,
    /// Unix seconds, when the feed gives a time.
    pub published: Option<i64>,
    pub points: Option<u32>,
    pub comments: Option<u32>,
    /// The feed's description with tags stripped and entities decoded, capped.
    pub summary: String,
}

/// One entry of the person's own feeds file, one tab each.
#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct UserFeed {
    pub label: String,
    pub url: String,
}

/// Parse `feeds.json`: fields are trimmed and entries without an http(s)
/// url are dropped. One source per URL and the cap are `set_user_feeds`'s,
/// so the file and a caller with a list are treated alike.
pub fn parse_user_feeds(text: &str) -> Result<Vec<UserFeed>, String> {
    let feeds: Vec<UserFeed> = DeJson::deserialize_json_lenient(text).map_err(|e| format!("{e:?}"))?;
    Ok(feeds
        .into_iter()
        .map(|f| UserFeed { label: f.label.trim().to_string(), url: f.url.trim().to_string() })
        .filter(|f| is_http_url(&f.url))
        .collect())
}

/// `http://` or `https://`, whatever the case: the only schemes a row or a
/// feed may carry. Open hands a link to the system browser, and macOS
/// `open` would dispatch any other scheme to whatever claims it.
pub(crate) fn is_http_url(s: &str) -> bool {
    ["http://", "https://"].iter().any(|scheme| s.get(..scheme.len()).is_some_and(|head| head.eq_ignore_ascii_case(scheme)))
}

/// All: the first row of each source, then the second, and so on, so every
/// source's own ranking survives and none floods the list.
pub(crate) fn interleave(sources: &[&[Headline]]) -> Vec<Headline> {
    let max = sources.iter().map(|rows| rows.len()).max().unwrap_or(0);
    (0..max)
        .flat_map(|rank| sources.iter().filter_map(move |rows| rows.get(rank).cloned()))
        .collect()
}

/// The text of a fetched body, or why it is not one: the trust boundary
/// between the network and the parsers. Only a 200 whose body is present,
/// under the cap and UTF-8 gets through.
pub(crate) fn body_text(status_code: u16, body: Option<&[u8]>) -> Result<&str, String> {
    if status_code != 200 {
        // A 301/302 surfaces here as `HTTP 3xx` unless the platform follows
        // redirects.
        return Err(format!("HTTP {status_code}"));
    }
    let body = body.ok_or_else(|| "empty response".to_string())?;
    if body.len() > MAX_BODY_BYTES {
        return Err(format!("response too large ({} bytes)", body.len()));
    }
    std::str::from_utf8(body).map_err(|_| "response is not UTF-8".to_string())
}

/// Parse one source's body into rows, by its kind. Zero rows is an error:
/// the tab keeps what it had rather than going blank.
pub(crate) fn parse_body(kind: SourceKind, body: &str) -> Result<Vec<Headline>, String> {
    let rows = match kind {
        SourceKind::HackerNews => crate::hn::parse(body)?,
        SourceKind::Feed { split_source } => crate::feed::parse(body, split_source)?,
    };
    if rows.is_empty() {
        return Err("no headlines in the response".into());
    }
    Ok(rows.into_iter().take(MAX_ROWS_PER_SOURCE).collect())
}

/// The host of a URL, without scheme, userinfo, port, path or `www.`:
/// `blog.example`. When nothing is left the trimmed input names the feed,
/// so a label is never blank.
pub fn host_of(url: &str) -> String {
    let url = url.trim();
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    // An IPv6 host keeps its brackets; only a port after them goes.
    let host = if host_port.starts_with('[') {
        host_port.find(']').map_or(host_port, |end| &host_port[..=end])
    } else {
        host_port.split_once(':').map_or(host_port, |(host, _)| host)
    };
    let host = match host.get(..4) {
        Some(prefix) if prefix.eq_ignore_ascii_case("www.") => &host[4..],
        _ => host,
    };
    if host.is_empty() { url } else { host }.to_string()
}

pub struct SourceState {
    pub def: SourceDef,
    pub rows: Vec<Headline>,
    /// When `rows` last came from the network (not the cache).
    pub fetched_at: Option<Instant>,
    /// When a fetch last began, whether or not it succeeded.
    pub attempted_at: Option<Instant>,
    pub pending: Option<LiveId>,
    pub error: Option<String>,
    /// `rows` came from the storage jail and no fetch has replaced them.
    pub from_cache: bool,
}

impl SourceState {
    fn new(def: SourceDef) -> Self {
        SourceState { def, rows: Vec::new(), fetched_at: None, attempted_at: None, pending: None, error: None, from_cache: false }
    }

    /// Drop the in-flight request, if any, and the attempt it was.
    fn cancel(&mut self) -> Option<LiveId> {
        let pending = self.pending.take();
        if pending.is_some() {
            self.attempted_at = None;
        }
        pending
    }
}

/// What the faces show, independent of how it is laid out: every source's
/// rows and fetch state, the selected tab and the open row.
pub struct NewsModel {
    pub sources: Vec<SourceState>,
    /// 0 is All; `n` shows `sources[n - 1]`.
    pub tab: usize,
    /// The open row's link: a refresh moves rows, and the story stays open,
    /// not the position.
    pub expanded: Option<String>,
    seq: u64,
}

impl Default for NewsModel {
    fn default() -> Self {
        Self::new()
    }
}

impl NewsModel {
    pub fn new() -> Self {
        NewsModel { sources: builtin_sources().into_iter().map(SourceState::new).collect(), tab: 0, expanded: None, seq: 0 }
    }

    pub fn tab_count(&self) -> usize {
        self.sources.len() + 1
    }

    pub fn tab_label(&self, tab: usize) -> &str {
        if tab == 0 {
            "All"
        } else {
            self.sources.get(tab - 1).map(|s| s.def.label.as_str()).unwrap_or("")
        }
    }

    /// Change tabs; the open row closes. False when nothing changed.
    pub fn select_tab(&mut self, tab: usize) -> bool {
        if tab >= self.tab_count() || tab == self.tab {
            return false;
        }
        self.tab = tab;
        self.expanded = None;
        true
    }

    /// Open a row by its link, or close it when it is the open one.
    pub fn toggle_expanded(&mut self, link: &str) {
        self.expanded = if self.expanded.as_deref() == Some(link) { None } else { Some(link.to_string()) };
    }

    pub fn is_expanded(&self, link: &str) -> bool {
        self.expanded.as_deref() == Some(link)
    }

    /// The source indices a tab shows: every source for All.
    pub fn sources_for_tab(&self, tab: usize) -> Vec<usize> {
        if tab == 0 {
            (0..self.sources.len()).collect()
        } else {
            vec![tab - 1]
        }
    }

    pub fn rows_for_tab(&self, tab: usize) -> Vec<Headline> {
        if tab == 0 {
            let slices: Vec<&[Headline]> = self.sources.iter().map(|s| s.rows.as_slice()).collect();
            interleave(&slices)
        } else {
            self.sources.get(tab - 1).map(|s| s.rows.clone()).unwrap_or_default()
        }
    }

    /// How many rows a tab has, without building them.
    pub fn row_count(&self, tab: usize) -> usize {
        self.sources_for_tab(tab).into_iter().map(|i| self.sources[i].rows.len()).sum()
    }

    /// The home tile: the top of All, one row per built-in source.
    pub fn tile_rows(&self) -> Vec<Headline> {
        self.rows_for_tab(0).into_iter().take(TILE_ROWS).collect()
    }

    /// A fresh request id for a source; an earlier in-flight reply for it is
    /// superseded.
    pub fn begin_fetch(&mut self, source: usize) -> (LiveId, String) {
        self.seq += 1;
        let state = &mut self.sources[source];
        let id = LiveId::from_str(&format!("news_fetch_{}_{}", state.def.id, self.seq));
        state.pending = Some(id);
        state.attempted_at = Some(Instant::now());
        (id, state.def.url.clone())
    }

    /// Which source is waiting for this request, if any.
    pub fn owns(&self, id: LiveId) -> Option<usize> {
        self.sources.iter().position(|s| s.pending == Some(id))
    }

    /// Land a reply: good rows replace the source's rows (and clear its
    /// error); a failure keeps the rows and records the error.
    pub fn complete(&mut self, id: LiveId, result: Result<Vec<Headline>, String>) -> Option<usize> {
        let index = self.owns(id)?;
        let state = &mut self.sources[index];
        state.pending = None;
        match result {
            Ok(rows) => {
                state.rows = rows;
                state.fetched_at = Some(Instant::now());
                state.error = None;
                state.from_cache = false;
            }
            Err(e) => state.error = Some(e),
        }
        Some(index)
    }

    /// Forget every in-flight request; the caller cancels them. A cancelled
    /// attempt was no attempt, so it does not hold the next fetch for the
    /// budget.
    pub fn cancel_all(&mut self) -> Vec<LiveId> {
        self.sources.iter_mut().filter_map(SourceState::cancel).collect()
    }

    /// A source wants a fetch when none is in flight and nothing has been
    /// tried for the refresh budget: rows missing, cached, or old. A failure
    /// waits the same budget before the next try, so a dead source is not
    /// hammered every tick; a manual refresh does not ask.
    pub fn due(&self, source: usize) -> bool {
        self.due_at(source, Instant::now())
    }

    /// `due` as of `now`, so a test moves the clock instead of waiting.
    pub(crate) fn due_at(&self, source: usize, now: Instant) -> bool {
        let state = &self.sources[source];
        if state.pending.is_some() {
            return false;
        }
        match state.fetched_at.max(state.attempted_at) {
            Some(at) => now.saturating_duration_since(at) >= REFRESH_EVERY,
            None => true,
        }
    }

    pub fn loading(&self, tab: usize) -> bool {
        self.sources_for_tab(tab).into_iter().any(|i| self.sources[i].pending.is_some())
    }

    /// The tab's failures, one per line: `TechMeme: HTTP 503`.
    pub fn error_text(&self, tab: usize) -> Option<String> {
        let lines: Vec<String> = self
            .sources_for_tab(tab)
            .into_iter()
            .filter_map(|i| self.sources[i].error.as_ref().map(|e| format!("{}: {e}", self.sources[i].def.label)))
            .collect();
        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    /// One line of status for the bar.
    pub fn status_text(&self, tab: usize) -> String {
        self.status_text_at(tab, Instant::now())
    }

    /// `status_text` as of `now`.
    pub(crate) fn status_text_at(&self, tab: usize, now: Instant) -> String {
        let indices = self.sources_for_tab(tab);
        let newest = indices.iter().filter_map(|i| self.sources[*i].fetched_at).max();
        let loading = self.loading(tab);
        let any_rows = indices.iter().any(|i| !self.sources[*i].rows.is_empty());
        let any_error = indices.iter().any(|i| self.sources[*i].error.is_some());
        let all_cached = any_rows && indices.iter().all(|i| self.sources[*i].rows.is_empty() || self.sources[*i].from_cache);
        match (loading, newest) {
            (true, None) => "Loading".into(),
            (true, Some(at)) => format!("Refreshing · updated {}", age_at(at, now)),
            (false, Some(at)) if any_error => format!("Offline · showing headlines from {}", age_at(at, now)),
            (false, Some(at)) if now.saturating_duration_since(at) >= REFRESH_EVERY => format!("Stale · updated {}", age_at(at, now)),
            (false, Some(at)) => format!("Updated {}", age_at(at, now)),
            (false, None) if all_cached => "Saved headlines".into(),
            (false, None) if any_error => "Unavailable".into(),
            (false, None) => NO_HEADLINES.into(),
        }
    }

    pub fn cache_key(&self, source: usize) -> String {
        format!("cache.{}", self.sources[source].def.id)
    }

    pub fn cache_bytes(&self, source: usize) -> Vec<u8> {
        self.sources[source].rows.serialize_json().into_bytes()
    }

    /// Seed a source from the storage jail; only while it has nothing better.
    /// Rows the faces would not show (no title, no http link) are dropped and
    /// the count is capped, as if the document had just been parsed.
    pub fn load_cache(&mut self, source: usize, bytes: &[u8]) -> bool {
        let state = &mut self.sources[source];
        if state.fetched_at.is_some() || !state.rows.is_empty() {
            return false;
        }
        let Ok(text) = std::str::from_utf8(bytes) else { return false };
        let Ok(rows) = <Vec<Headline> as DeJson>::deserialize_json_lenient(text) else { return false };
        let rows: Vec<Headline> = rows.into_iter().filter(showable).take(MAX_ROWS_PER_SOURCE).collect();
        if rows.is_empty() {
            return false;
        }
        state.rows = rows;
        state.from_cache = true;
        true
    }

    /// Replace the person's feeds (everything after the built-in three).
    /// Two entries with one URL are one source (the first entry's label):
    /// the id and the cache key follow the URL, so a duplicate would be a
    /// second tab reading and writing the same cache. At most
    /// `MAX_USER_FEEDS` distinct feeds are taken, so the tab strip stays
    /// finite. Returns the indices of the new sources and the in-flight
    /// request ids of the old ones, for the caller to cancel.
    pub fn set_user_feeds(&mut self, feeds: Vec<UserFeed>) -> (Range<usize>, Vec<LiveId>) {
        let builtin = builtin_sources().len();
        let cancelled: Vec<LiveId> = self.sources.iter_mut().skip(builtin).filter_map(SourceState::cancel).collect();
        self.sources.truncate(builtin);
        for feed in feeds {
            if self.sources.len() - builtin >= MAX_USER_FEEDS {
                break;
            }
            let url = feed.url.trim().to_string();
            if self.sources[builtin..].iter().any(|s| s.def.url == url) {
                continue;
            }
            let label = if feed.label.trim().is_empty() { host_of(&url) } else { feed.label.trim().to_string() };
            self.sources.push(SourceState::new(SourceDef {
                id: format!("user_{:016x}", LiveId::from_str(&url).0),
                label,
                url,
                kind: SourceKind::Feed { split_source: false },
            }));
        }
        if self.tab > builtin {
            // A user tab's rows just changed under its open row.
            self.expanded = None;
            self.tab = self.tab.min(self.tab_count() - 1);
        }
        (builtin..self.sources.len(), cancelled)
    }

    /// A source by id, else by label, case-insensitively (the tool's
    /// `source`). Ids first, so a feed labelled `google` never shadows the
    /// built-in whatever order the sources are in.
    pub fn source_index(&self, name: &str) -> Option<usize> {
        let name = name.trim().to_lowercase();
        self.sources
            .iter()
            .position(|s| s.def.id.to_lowercase() == name)
            .or_else(|| self.sources.iter().position(|s| s.def.label.to_lowercase() == name))
    }
}

/// A row the faces can show: a title and an http(s) link.
fn showable(row: &Headline) -> bool {
    !row.title.trim().is_empty() && is_http_url(&row.link)
}

/// `just now`, `3 min ago`, `2 h ago`, as of `now`.
pub(crate) fn age_at(at: Instant, now: Instant) -> String {
    let secs = now.saturating_duration_since(at).as_secs();
    match secs {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", secs / 60),
        _ => format!("{} h ago", secs / 3600),
    }
}

/// A row's time relative to `now` (unix seconds): `5m ago`, `3h ago`, `2d ago`.
pub(crate) fn relative_time(published: Option<i64>, now: i64) -> Option<String> {
    let delta = now.saturating_sub(published?);
    Some(match delta {
        d if d < 60 => "just now".into(),
        d if d < 3600 => format!("{}m ago", d / 60),
        d if d < 86_400 => format!("{}h ago", d / 3600),
        d => format!("{}d ago", d / 86_400),
    })
}

/// `1 point`, `2 points`.
pub(crate) fn plural(n: u32, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// One tile line: the title, with its source when the row names one.
pub(crate) fn tile_line(row: &Headline) -> String {
    if row.source.is_empty() {
        row.title.clone()
    } else {
        format!("{} · {}", row.title, row.source)
    }
}

/// The second line of a row: source, time, points and comments, joined by dots.
pub(crate) fn meta_line(row: &Headline, now: i64) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !row.source.is_empty() {
        parts.push(row.source.clone());
    }
    if let Some(t) = relative_time(row.published, now) {
        parts.push(t);
    }
    if let Some(p) = row.points {
        parts.push(plural(p, "point"));
    }
    if let Some(c) = row.comments {
        parts.push(plural(c, "comment"));
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(title: &str) -> Headline {
        Headline { title: title.into(), link: format!("https://example.com/{title}"), ..Default::default() }
    }

    #[test]
    fn builtin_sources_are_hn_techmeme_google_in_that_order() {
        let sources = builtin_sources();
        let ids: Vec<&str> = sources.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["hn", "techmeme", "google"]);
        assert_eq!(builtin_sources()[0].kind, SourceKind::HackerNews);
        assert_eq!(builtin_sources()[2].kind, SourceKind::Feed { split_source: true });
    }

    #[test]
    fn interleave_takes_one_row_per_source_per_rank() {
        let a = vec![row("a1"), row("a2"), row("a3")];
        let b = vec![row("b1")];
        let c: Vec<Headline> = vec![];
        let d = vec![row("d1"), row("d2")];
        let all = interleave(&[&a, &b, &c, &d]);
        let titles: Vec<&str> = all.iter().map(|h| h.title.as_str()).collect();
        assert_eq!(titles, ["a1", "b1", "d1", "a2", "d2", "a3"]);
        assert!(interleave(&[]).is_empty());
    }

    #[test]
    fn user_feeds_parse_leniently_and_drop_bad_urls_and_the_model_caps_the_count() {
        let text = r#"[{"label":"A","url":"https://a.example/feed","extra":1},{"label":"bad","url":"ftp://x"},{"label":"odd","url":"httpx://x"},{"label":"B","url":"http://b.example/rss"},{"label":"C","url":"https://c.example/feed"},{"label":"D","url":"https://d.example/feed"},{"label":"E","url":"https://e.example/feed"}]"#;
        let feeds = parse_user_feeds(text).unwrap();
        let labels: Vec<&str> = feeds.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(labels, ["A", "B", "C", "D", "E"], "bad urls dropped; the parser does not cap");
        let mut m = NewsModel::new();
        let (added, _) = m.set_user_feeds(feeds);
        assert_eq!(added, 3..7, "the model caps at {MAX_USER_FEEDS}");
        assert_eq!(m.tab_label(7), "D");
        assert!(parse_user_feeds("not json").is_err());
        let padded = parse_user_feeds(r#"[{"label":" F ","url":" https://f.example/feed "}]"#).unwrap();
        assert_eq!(padded, vec![UserFeed { label: "F".into(), url: "https://f.example/feed".into() }]);
    }

    #[test]
    fn headline_round_trips_through_json() {
        let h = Headline { title: "T \"q\"".into(), link: "https://x".into(), source: "S".into(), published: Some(1_700_000_000), points: Some(12), comments: None, summary: "s".into() };
        let json = h.serialize_json();
        let back: Headline = DeJson::deserialize_json_lenient(&json).unwrap();
        assert_eq!(back, h);
        let rows: Vec<Headline> = DeJson::deserialize_json_lenient(&vec![h.clone()].serialize_json()).unwrap();
        assert_eq!(rows, vec![h]);
    }

    fn rows(prefix: &str, n: usize) -> Vec<Headline> {
        (1..=n).map(|i| row(&format!("{prefix}{i}"))).collect()
    }

    #[test]
    fn tabs_are_all_then_every_source_and_selecting_resets_the_open_row() {
        let mut m = NewsModel::new();
        assert_eq!(m.tab_count(), 4);
        assert_eq!(m.tab_label(0), "All");
        assert_eq!(m.tab_label(1), "Hacker News");
        assert_eq!(m.sources_for_tab(0), vec![0, 1, 2]);
        assert_eq!(m.sources_for_tab(2), vec![1]);
        m.toggle_expanded("https://x/3");
        assert!(m.is_expanded("https://x/3"));
        assert!(m.select_tab(2));
        assert_eq!(m.expanded, None);
        assert!(!m.select_tab(2), "same tab: nothing changed");
        assert!(!m.select_tab(99), "out of range is ignored");
        m.toggle_expanded("https://x/1");
        m.toggle_expanded("https://x/2");
        assert!(m.is_expanded("https://x/2") && !m.is_expanded("https://x/1"), "opening another row moves the open one");
        m.toggle_expanded("https://x/2");
        assert_eq!(m.expanded, None, "toggling the open row closes it");
    }

    #[test]
    fn a_fetch_is_owned_until_completed_and_a_superseded_reply_is_dropped() {
        let mut m = NewsModel::new();
        let (first, url) = m.begin_fetch(1);
        assert_eq!(url, builtin_sources()[1].url);
        assert_eq!(m.owns(first), Some(1));
        assert!(m.loading(0) && m.loading(2) && !m.loading(1));
        let (second, _) = m.begin_fetch(1);
        assert_ne!(first, second);
        assert_eq!(m.owns(first), None, "the older request is superseded");
        assert_eq!(m.complete(first, Ok(rows("old", 2))), None);
        assert!(m.sources[1].rows.is_empty());
        assert_eq!(m.complete(second, Ok(rows("t", 3))), Some(1));
        assert_eq!(m.sources[1].rows.len(), 3);
        assert!(m.sources[1].fetched_at.is_some());
        assert!(!m.sources[1].from_cache);
        assert!(!m.due(1), "just fetched");
        assert!(m.due(0) && m.due(2), "never fetched");
    }

    #[test]
    fn a_failure_keeps_the_rows_and_records_the_error() {
        let mut m = NewsModel::new();
        let (id, _) = m.begin_fetch(2);
        m.complete(id, Ok(rows("g", 2)));
        let (id, _) = m.begin_fetch(2);
        m.complete(id, Err("HTTP 503".into()));
        assert_eq!(m.sources[2].rows.len(), 2);
        assert_eq!(m.sources[2].error.as_deref(), Some("HTTP 503"));
        assert_eq!(m.error_text(3).as_deref(), Some("Google News: HTTP 503"));
        assert_eq!(m.error_text(0).as_deref(), Some("Google News: HTTP 503"));
        assert_eq!(m.error_text(1), None);
        let (id, _) = m.begin_fetch(2);
        m.complete(id, Ok(rows("g", 1)));
        assert_eq!(m.sources[2].error, None, "a good fetch clears the error");
    }

    #[test]
    fn all_interleaves_and_the_tile_takes_the_top_three() {
        let mut m = NewsModel::new();
        for (i, prefix) in ["h", "t", "g"].iter().enumerate() {
            let (id, _) = m.begin_fetch(i);
            m.complete(id, Ok(rows(prefix, 3)));
        }
        let all: Vec<String> = m.rows_for_tab(0).iter().map(|h| h.title.clone()).collect();
        assert_eq!(all, ["h1", "t1", "g1", "h2", "t2", "g2", "h3", "t3", "g3"]);
        let tile: Vec<String> = m.tile_rows().iter().map(|h| h.title.clone()).collect();
        assert_eq!(tile, ["h1", "t1", "g1"]);
        assert_eq!(m.rows_for_tab(2).len(), 3);
        assert_eq!(m.row_count(0), 9);
        assert_eq!(m.row_count(2), 3);
    }

    #[test]
    fn the_cache_round_trips_and_never_overwrites_fetched_rows() {
        let mut m = NewsModel::new();
        assert_eq!(m.cache_key(0), "cache.hn");
        let (id, _) = m.begin_fetch(0);
        m.complete(id, Ok(rows("h", 2)));
        let bytes = m.cache_bytes(0);
        let mut fresh = NewsModel::new();
        assert!(fresh.load_cache(0, &bytes));
        assert_eq!(fresh.sources[0].rows.len(), 2);
        assert!(fresh.sources[0].from_cache);
        assert!(fresh.due(0), "cached rows still want a fetch");
        assert!(!m.load_cache(0, &bytes), "fetched rows win over the cache");
        assert!(!fresh.load_cache(1, b"garbage"));
    }

    #[test]
    fn user_feeds_append_tabs_and_replace_earlier_user_feeds() {
        let mut m = NewsModel::new();
        let blog = UserFeed { label: "Blog".into(), url: "https://blog.example/feed".into() };
        let only = UserFeed { label: "Only".into(), url: "https://o.example/feed".into() };
        let (added, cancelled) =
            m.set_user_feeds(vec![blog.clone(), UserFeed { label: "".into(), url: "https://b.example/feed".into() }]);
        assert_eq!(added, 3..5);
        assert!(cancelled.is_empty());
        assert_eq!(m.tab_count(), 6);
        assert_eq!(m.tab_label(4), "Blog");
        assert_eq!(m.tab_label(5), "b.example", "an unlabelled feed is named by its host");
        let blog_id = m.sources[3].def.id.clone();
        assert!(blog_id.starts_with("user_"), "{blog_id}");
        assert_ne!(blog_id, m.sources[4].def.id, "a different url gets a different id");
        assert_eq!(m.sources[3].def.kind, SourceKind::Feed { split_source: false });
        m.select_tab(4);
        m.toggle_expanded("https://blog.example/1");
        let (pending, _) = m.begin_fetch(4);
        let (added, cancelled) = m.set_user_feeds(vec![blog.clone(), only.clone()]);
        assert_eq!(added, 3..5);
        assert_eq!(cancelled, vec![pending], "the replaced feed's in-flight request comes back for cancelling");
        assert_eq!(m.sources[3].def.id, blog_id, "the same url gets the same id");
        assert_eq!((m.tab, m.expanded.as_deref()), (4, None), "a user tab's open row closes: its rows changed");
        m.select_tab(5);
        let (added, _) = m.set_user_feeds(vec![only]);
        assert_eq!(added, 3..4);
        assert_eq!(m.tab_count(), 5);
        assert_eq!(m.tab, 4, "the selected tab is clamped into range");
        assert_eq!(m.source_index("only"), Some(3), "labels match case-insensitively");
        assert_eq!(m.source_index("HN"), Some(0), "so do ids");
        assert_eq!(m.source_index("nope"), None);
        m.select_tab(1);
        m.toggle_expanded("https://x/h1");
        m.set_user_feeds(vec![blog]);
        assert_eq!((m.tab, m.expanded.as_deref()), (1, Some("https://x/h1")), "a built-in tab keeps its open row");
    }

    #[test]
    fn user_feed_cache_keys_follow_the_url_not_the_position() {
        let mut m = NewsModel::new();
        let a = UserFeed { label: "A".into(), url: "https://a.example/feed".into() };
        m.set_user_feeds(vec![a.clone()]);
        let (id, _) = m.begin_fetch(3);
        m.complete(id, Ok(rows("a", 2)));
        let a_key = m.cache_key(3);
        assert!(a_key.starts_with("cache.user_"), "{a_key}");
        m.set_user_feeds(vec![UserFeed { label: "B".into(), url: "https://b.example/feed".into() }]);
        assert!(m.sources[3].rows.is_empty(), "a replacement starts empty");
        assert_ne!(m.cache_key(3), a_key, "B reads its own key, so A's saved rows never seed it");
        m.set_user_feeds(vec![a]);
        assert_eq!(m.cache_key(3), a_key, "A's key survives being removed and re-added");
    }

    #[test]
    fn source_index_matches_an_id_before_a_label() {
        let mut m = NewsModel::new();
        m.set_user_feeds(vec![UserFeed { label: "Google".into(), url: "https://g.example/feed".into() }]);
        assert_eq!(m.source_index("google"), Some(2), "the built-in id wins over a user feed's label");
        assert_eq!(m.source_index("Google News"), Some(2));
        let user_id = m.sources[3].def.id.clone();
        assert_eq!(m.source_index(&user_id), Some(3), "the user feed is still reachable by its id");
    }

    #[test]
    fn a_failed_fetch_waits_the_refresh_budget_before_retrying() {
        let mut m = NewsModel::new();
        let (id, _) = m.begin_fetch(1);
        m.complete(id, Err("HTTP 500".into()));
        assert!(!m.due(1), "a failure is not retried on the next tick");
        assert!(m.due(0) && m.due(2), "untouched sources are still due");
        // The clock, moved past the budget: an old failure no longer holds
        // the source, old rows are stale.
        let later = Instant::now() + REFRESH_EVERY + Duration::from_secs(1);
        assert!(m.due_at(1, later), "an old failure no longer holds the source");
        let (id, _) = m.begin_fetch(1);
        m.complete(id, Ok(rows("t", 1)));
        assert!(!m.due(1), "just fetched");
        assert!(m.due_at(1, later), "stale rows are due");
        assert_eq!(m.status_text_at(2, later), "Stale · updated 15 min ago");
        assert_eq!(m.status_text_at(0, later), "Stale · updated 15 min ago", "All reports the newest fetch");
        // A refresh that fails at `later` restarts the wait from the failure,
        // not from the old success.
        m.sources[1].attempted_at = Some(later);
        m.sources[1].error = Some("HTTP 500".into());
        assert!(!m.due_at(1, later + Duration::from_secs(1)), "the failed refresh restarts the wait");
        assert!(m.due_at(1, later + REFRESH_EVERY), "and holds it for one budget only");
    }

    #[test]
    fn a_source_that_failed_before_ever_loading_is_unavailable() {
        let mut m = NewsModel::new();
        let (id, _) = m.begin_fetch(0);
        m.complete(id, Err("HTTP 500".into()));
        assert_eq!(m.status_text(1), "Unavailable");
        assert_eq!(m.status_text(0), "Unavailable", "All with nothing but a failure");
        assert_eq!(m.status_text(2), "No headlines yet", "the failure is HN's alone");
    }

    #[test]
    fn the_cache_drops_rows_it_would_not_show_and_caps_the_count() {
        let good = row("good");
        let blank_title = Headline { title: "  ".into(), link: "https://x/blank".into(), ..Default::default() };
        let odd_link = Headline { title: "Odd link".into(), link: "javascript:alert(1)".into(), ..Default::default() };
        let mut m = NewsModel::new();
        assert!(m.load_cache(0, vec![blank_title.clone(), good.clone(), odd_link.clone()].serialize_json().as_bytes()));
        assert_eq!(m.sources[0].rows, vec![good]);
        assert!(!m.load_cache(1, vec![blank_title, odd_link].serialize_json().as_bytes()), "nothing showable is nothing");
        let many: Vec<Headline> = (0..MAX_ROWS_PER_SOURCE + 10).map(|i| row(&format!("r{i}"))).collect();
        assert!(m.load_cache(2, many.serialize_json().as_bytes()));
        assert_eq!(m.sources[2].rows.len(), MAX_ROWS_PER_SOURCE);
    }

    #[test]
    fn status_text_names_the_state() {
        let mut m = NewsModel::new();
        assert_eq!(m.status_text(1), "No headlines yet");
        let (id, _) = m.begin_fetch(0);
        assert_eq!(m.status_text(1), "Loading");
        m.complete(id, Ok(rows("h", 1)));
        assert_eq!(m.status_text(1), "Updated just now");
        assert_eq!(m.status_text(0), "Updated just now", "All reports the newest fetch");
        let (id, _) = m.begin_fetch(0);
        assert_eq!(m.status_text(1), "Refreshing · updated just now");
        m.complete(id, Err("HTTP 500".into()));
        assert_eq!(m.status_text(1), "Offline · showing headlines from just now");
        let mut cached = NewsModel::new();
        cached.load_cache(0, &m.cache_bytes(0));
        assert_eq!(cached.status_text(1), "Saved headlines");
        assert_eq!(cached.status_text(2), "No headlines yet");
    }

    #[test]
    fn cancel_all_returns_every_pending_request() {
        let mut m = NewsModel::new();
        let (a, _) = m.begin_fetch(0);
        let (b, _) = m.begin_fetch(2);
        let ids = m.cancel_all();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&a) && ids.contains(&b));
        assert!(!m.loading(0));
        assert!(m.due(0) && m.due(2), "a cancelled attempt was no attempt: the next fetch is not held for the budget");
    }

    #[test]
    fn a_duplicate_url_in_the_feeds_file_does_not_take_a_slot() {
        let text = r#"[{"label":"A","url":"https://a.example/feed"},{"label":"A again","url":" https://a.example/feed "},{"label":"B","url":"https://b.example/feed"},{"label":"C","url":"https://c.example/feed"},{"label":"D","url":"https://d.example/feed"}]"#;
        let feeds = parse_user_feeds(text).unwrap();
        assert_eq!(feeds.len(), 5, "the parser keeps the duplicate; the model folds it");
        let mut m = NewsModel::new();
        let (added, _) = m.set_user_feeds(feeds);
        assert_eq!(added, 3..7);
        assert_eq!(m.sources.len(), 7, "four user sources through the real path");
        let labels: Vec<&str> = m.sources[3..].iter().map(|s| s.def.label.as_str()).collect();
        assert_eq!(labels, ["A", "B", "C", "D"], "the duplicate never costs D its slot");
    }

    #[test]
    fn only_http_and_https_are_links() {
        assert!(is_http_url("https://x.example/1"));
        assert!(is_http_url("HTTP://x.example/1"), "the scheme is case-insensitive");
        assert!(!is_http_url("httpx://x.example/1"));
        assert!(!is_http_url("ftp://x.example/1"));
        assert!(!is_http_url("javascript:alert(1)"));
        assert!(!is_http_url("http:/x"));
        assert!(!is_http_url("ü://x"), "a multi-byte prefix never panics");
        assert!(!is_http_url(""));
    }

    #[test]
    fn body_text_lets_only_a_utf8_200_under_the_cap_through() {
        assert_eq!(body_text(200, Some(b"<rss/>".as_slice())), Ok("<rss/>"));
        assert_eq!(body_text(503, Some(b"x".as_slice())), Err("HTTP 503".into()));
        assert_eq!(body_text(302, Some(b"".as_slice())), Err("HTTP 302".into()), "a redirect the platform did not follow");
        assert_eq!(body_text(200, None), Err("empty response".into()));
        let big = vec![b'a'; MAX_BODY_BYTES + 1];
        assert_eq!(body_text(200, Some(&big)), Err(format!("response too large ({} bytes)", MAX_BODY_BYTES + 1)));
        assert_eq!(body_text(200, Some([0xff, 0xfe].as_slice())), Err("response is not UTF-8".into()));
    }

    #[test]
    fn tile_line_is_the_title_with_its_source_when_there_is_one() {
        let mut h = row("Title");
        assert_eq!(tile_line(&h), "Title");
        h.source = "Reuters".into();
        assert_eq!(tile_line(&h), "Title · Reuters");
    }

    #[test]
    fn user_feeds_with_the_same_url_collapse_into_one_source() {
        let mut m = NewsModel::new();
        let a = UserFeed { label: "A".into(), url: "https://a.example/feed".into() };
        let a_again = UserFeed { label: "A again".into(), url: "https://a.example/feed".into() };
        let b = UserFeed { label: "B".into(), url: "https://b.example/feed".into() };
        let (added, _) = m.set_user_feeds(vec![a.clone(), a_again, b.clone()]);
        assert_eq!(added, 3..5, "the duplicate makes no source");
        assert_eq!(m.tab_label(4), "A", "the first entry wins");
        assert_eq!(m.tab_label(5), "B");
        let ids: Vec<&str> = m.sources.iter().map(|s| s.def.id.as_str()).collect();
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(ids.len(), unique.len(), "no two sources share an id or a cache key");
        // A duplicate does not use up one of the slots.
        let c = UserFeed { label: "C".into(), url: "https://c.example/feed".into() };
        let d = UserFeed { label: "D".into(), url: "https://d.example/feed".into() };
        let e = UserFeed { label: "E".into(), url: "https://e.example/feed".into() };
        let (added, _) = m.set_user_feeds(vec![a.clone(), a.clone(), b, c, d, e]);
        assert_eq!(added, 3..7, "{MAX_USER_FEEDS} distinct feeds fit");
        assert_eq!(m.tab_label(7), "D");
    }

    #[test]
    fn meta_line_joins_what_the_row_has() {
        let mut h = row("t");
        assert_eq!(meta_line(&h, 1_000_000), "");
        h.source = "Hacker News".into();
        h.published = Some(1_000_000 - 3 * 3600);
        h.points = Some(10);
        h.comments = Some(2);
        assert_eq!(meta_line(&h, 1_000_000), "Hacker News · 3h ago · 10 points · 2 comments");
        h.points = Some(1);
        h.comments = Some(1);
        assert_eq!(meta_line(&h, 1_000_000), "Hacker News · 3h ago · 1 point · 1 comment");
        assert_eq!(plural(0, "comment"), "0 comments");
        assert_eq!(relative_time(Some(1_000_000 - 30), 1_000_000).as_deref(), Some("just now"));
        assert_eq!(relative_time(Some(1_000_000 - 2 * 86_400), 1_000_000).as_deref(), Some("2d ago"));
        assert_eq!(relative_time(None, 1_000_000), None);
        assert!(relative_time(Some(i64::MIN), 1_000_000).is_some(), "an absurd time never overflows");
    }

    #[test]
    fn host_of_strips_scheme_path_and_www() {
        assert_eq!(host_of("https://www.example.com/a/b?c"), "example.com");
        assert_eq!(host_of("http://blog.example/feed"), "blog.example");
        assert_eq!(host_of("nonsense"), "nonsense");
        assert_eq!(host_of("https://user:pw@host.example/feed"), "host.example");
        assert_eq!(host_of("https://host:8080/x"), "host");
        assert_eq!(host_of("HTTPS://WWW.Example.COM"), "Example.COM");
        assert_eq!(host_of("https://"), "https://", "nothing left: the input names it");
        assert_eq!(host_of("http://[::1]:8080/x"), "[::1]");
        assert_eq!(host_of("  https://sp.example/  "), "sp.example");
    }

    #[test]
    fn parse_body_dispatches_by_kind_caps_rows_and_rejects_empty() {
        let hn = include_str!("../tests/fixtures/hn.json");
        assert_eq!(parse_body(SourceKind::HackerNews, hn).unwrap().len(), 2);
        let google = include_str!("../tests/fixtures/google.xml");
        assert_eq!(parse_body(SourceKind::Feed { split_source: true }, google).unwrap()[0].source, "Reuters");
        assert!(parse_body(SourceKind::Feed { split_source: false }, "<rss><channel></channel></rss>").is_err(), "zero rows is an error");
        assert!(parse_body(SourceKind::HackerNews, "<html>").is_err());
        let many: String = (0..50).map(|i| format!("<item><title>t{i}</title><link>https://x/{i}</link></item>")).collect();
        let feed = format!("<rss><channel>{many}</channel></rss>");
        assert_eq!(parse_body(SourceKind::Feed { split_source: false }, &feed).unwrap().len(), MAX_ROWS_PER_SOURCE);
    }
}
