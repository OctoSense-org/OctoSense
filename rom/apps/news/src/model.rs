//! The news model: sources, headline rows, the person's own feeds file, the
//! All interleave, and the per-source fetch state machine (loading, failed,
//! stale-but-kept, cached) both faces render from.
use makepad_widgets::makepad_micro_serde::*;
use makepad_widgets::{LiveId, ScriptVm, Vec4f};
use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::atomic::{AtomicU8, Ordering};
use std::str::Chars;
use std::time::{Duration, Instant};

pub(crate) const REFRESH_EVERY: Duration = Duration::from_secs(15 * 60);
/// A front page or a feed is tens of kilobytes; anything past this is not one.
pub const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const MAX_ROWS_PER_SOURCE: usize = 30;
pub const MAX_USER_FEEDS: usize = 4;
pub(crate) const TILE_ROWS: usize = 3;
pub(crate) const SUMMARY_CHARS: usize = 280;
/// Stories a Today section shows: the hero and four more.
pub(crate) const SECTION_ROWS: usize = 5;
pub(crate) const MAX_SAVED: usize = 100;
pub(crate) const MAX_SEARCH_ROWS: usize = 50;
/// The storage key of the hidden source ids: a JSON array of ids.
pub const HIDDEN_KEY: &str = "hidden.json";
/// The storage key of the saved stories: a JSON array of rows.
pub const SAVED_KEY: &str = "saved.json";
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
    /// A digest feed (TechMeme): RSS whose titles end in ` (Outlet)`, which
    /// becomes the row's publisher.
    Digest,
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

/// How many sources are built in: the person's own feeds come after them.
pub(crate) fn builtin_count() -> usize {
    3
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
            kind: SourceKind::Digest,
        },
        SourceDef {
            id: "google".into(),
            label: "Google News".into(),
            url: "https://news.google.com/rss?hl=en-US&gl=US&ceid=US:en".into(),
            kind: SourceKind::Feed { split_source: true },
        },
    ]
}

/// One row, whatever the source. `DeJson` is by hand, below: a cache
/// written before a field existed must still load.
#[derive(Clone, Debug, Default, PartialEq, SerJson)]
pub struct Headline {
    pub title: String,
    pub link: String,
    /// What the row shows as its source: the tab's label, or the outlet a
    /// Google News title names.
    pub source: String,
    /// The id of the source the row came from (`hn`, a user feed's
    /// `user_…`), stamped by the model when rows land, so a row keeps its
    /// provenance through the All interleave and the tile. Empty in
    /// caches written before it existed; `load_cache` fills it.
    pub source_id: String,
    /// Unix seconds, when the feed gives a time.
    pub published: Option<i64>,
    pub points: Option<u32>,
    pub comments: Option<u32>,
    /// The feed's description with tags stripped and entities decoded, capped.
    pub summary: String,
    /// Where the story is discussed (a Hacker News item page); the row's
    /// link is the story itself. Absent in caches written before it existed.
    pub discussion: Option<String>,
    /// The feed's picture for the story (an http(s) URL), when it gives one:
    /// a hero on the first card of a section, a thumbnail on the rest.
    /// Absent in caches written before it existed.
    pub image: Option<String>,
}

/// A row as any version of this crate wrote it. micro_serde has no field
/// default and a missing key is an error even when lenient, so the fields
/// that came after the first caches are optional here and `Headline`
/// decodes through this. A field added to `Headline` is added here too:
/// the constructor below is exhaustive.
#[derive(DeJson)]
struct HeadlineJson {
    title: String,
    link: String,
    source: String,
    source_id: Option<String>,
    published: Option<i64>,
    points: Option<u32>,
    comments: Option<u32>,
    summary: String,
    discussion: Option<String>,
    image: Option<String>,
}

impl DeJson for Headline {
    fn de_json(s: &mut DeJsonState, i: &mut Chars) -> Result<Self, DeJsonErr> {
        let h = HeadlineJson::de_json(s, i)?;
        Ok(Headline {
            title: h.title,
            link: h.link,
            source: h.source,
            source_id: h.source_id.unwrap_or_default(),
            published: h.published,
            points: h.points,
            comments: h.comments,
            summary: h.summary,
            discussion: h.discussion,
            image: h.image.filter(|u| is_http_url(u)),
        })
    }
}

/// The colour a source's badge and tile dot carry (RGBA), by the row's
/// `source_id`: Hacker News orange, TechMeme teal, Google News blue; a
/// person's own feed, and a row with no provenance, slate.
pub(crate) fn source_color(id: &str) -> u32 {
    match id {
        "hn" => 0xff6600ff,
        "techmeme" => 0x2bb5a0ff,
        "google" => 0x4285f4ff,
        _ => 0x7d8aa5ff,
    }
}

/// How the view is hosted: its own window, a child process in a host tile,
/// or an in-process module in an isolate. `main.rs` and the module's
/// `create` say which; the opener policy reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hosting {
    #[default]
    Standalone,
    Process,
    Module,
}

/// What the window manager answers when an `Open` or `Launch` named an app
/// its catalog cannot launch, as an `Event::Custom`:
/// `{"wm_unavailable": {"app": "browser", "path": "https://..."}}`. The
/// host's `src/wm_reply.rs` is the same envelope; this crate cannot depend
/// on the host, so the shape is spelled here too.
// `pub`, not `pub(crate)`: the micro_serde derives parse only a plain `pub`.
#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct WmUnavailable {
    pub app: String,
    pub path: String,
}

#[derive(SerJson, DeJson)]
struct WmUnavailableEnvelope {
    wm_unavailable: WmUnavailable,
}

impl WmUnavailable {
    /// The host's side of the envelope, for the tests that play the host.
    #[cfg(test)]
    pub(crate) fn to_json(&self) -> String {
        WmUnavailableEnvelope { wm_unavailable: self.clone() }.serialize_json()
    }

    /// The reply in a Custom event's json; None when it is something else.
    /// Lenient, so a field a later host adds does not break the receiver.
    pub(crate) fn parse(json: &str) -> Option<Self> {
        if !json.contains("\"wm_unavailable\"") {
            return None;
        }
        WmUnavailableEnvelope::deserialize_json_lenient(json).ok().map(|e| e.wm_unavailable)
    }
}

/// One way to open a link, in the order the view tries them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenTier {
    /// The bundled Browser app, through the window manager.
    Browser,
    /// The in-app reader on the platform's native web view.
    Reader,
    /// The system browser.
    System,
    /// Tell the person: nothing here can show the page.
    Notify,
}

/// What a link can be opened with here: the hosting and the platform's
/// answers, so the choice is a pure function the tests can pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OpenPolicy {
    pub hosting: Hosting,
    pub has_webview: bool,
    pub has_system_open: bool,
}

impl OpenPolicy {
    /// The tiers to try, in order. The reader first, wherever the native
    /// web view can appear (the standalone window or a module in the
    /// host's window, never a process child, which has no window of its
    /// own) and the platform has one; the Browser through the host when
    /// hosted; the system browser where the platform can open a URL; a
    /// notification always, last.
    pub(crate) fn tiers(&self) -> Vec<OpenTier> {
        let mut tiers = Vec::with_capacity(4);
        if self.has_webview && matches!(self.hosting, Hosting::Standalone | Hosting::Module) {
            tiers.push(OpenTier::Reader);
        }
        tiers.extend(self.browser_tiers());
        tiers
    }

    /// The tiers after the reader: what `Open in Browser` asks for. The
    /// Browser through the host when hosted, the system browser where the
    /// platform can open a URL, a notification last.
    pub(crate) fn browser_tiers(&self) -> Vec<OpenTier> {
        let mut tiers = Vec::with_capacity(3);
        if matches!(self.hosting, Hosting::Process | Hosting::Module) {
            tiers.push(OpenTier::Browser);
        }
        if self.has_system_open {
            tiers.push(OpenTier::System);
        }
        tiers.push(OpenTier::Notify);
        tiers
    }

    /// The platform's own answers: a native web view on macOS, iOS and
    /// Android; `open_url` on macOS and the web.
    pub(crate) fn for_platform(hosting: Hosting) -> Self {
        OpenPolicy {
            hosting,
            has_webview: cfg!(any(target_os = "macos", target_os = "ios", target_os = "android")),
            has_system_open: cfg!(any(target_os = "macos", target_arch = "wasm32")),
        }
    }
}


/// One Today section: a followed source and its top stories, the first
/// the section's hero.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub source: usize,
    pub rows: Vec<Headline>,
}

/// The caption under a section's name: how the source ranks its stories,
/// or where a person's feed comes from.
pub(crate) fn section_caption(def: &SourceDef) -> String {
    match def.id.as_str() {
        "hn" => "Ranked by points".into(),
        "techmeme" => "Editors' picks".into(),
        "google" => "Top stories".into(),
        _ => host_of(&def.url),
    }
}

/// The monogram a source's circle carries: its label's first letter.
pub(crate) fn initial(label: &str) -> String {
    label.trim().chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "?".into())
}

/// The app's colours: one set of roles in two variants, Apple News' light
/// look and its dark counterpart. `Vec4f`s, so the DSL splices them as
/// values when the crate's `script_mod` runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skin {
    pub light: bool,
    /// The page behind the cards.
    pub ground: Vec4f,
    /// A card, the bottom sheet, a field's card.
    pub card: Vec4f,
    /// Titles and headlines.
    pub ink: Vec4f,
    /// Captions, meta lines, inactive tabs.
    pub secondary: Vec4f,
    /// The line between compact rows.
    pub hairline: Vec4f,
    /// The search field and the inputs.
    pub field: Vec4f,
    /// The selected tab, the `More from` links, `NEWS` on the tile.
    pub accent: Vec4f,
}

impl Skin {
    pub fn for_mode(light: bool) -> Self {
        let c = Vec4f::from_u32;
        if light {
            Skin {
                light,
                ground: c(0xf2f2f7ff),
                card: c(0xffffffff),
                ink: c(0x000000ff),
                secondary: c(0x6e6e73ff),
                hairline: c(0x3c3c432e),
                field: c(0xe5e5eaff),
                accent: c(0xfa2d48ff),
            }
        } else {
            Skin {
                light,
                ground: c(0x000000ff),
                card: c(0x1c1c1eff),
                ink: c(0xffffffff),
                secondary: c(0x98989fff),
                hairline: c(0x54545866),
                field: c(0x2c2c2eff),
                accent: c(0xfa2d48ff),
            }
        }
    }

    /// The skin for this isolate: a forced one (`force_skin`, the
    /// standalone window's `--dark` and `--light`), else the host palette's
    /// light or dark mode, else light. The host re-runs the crate's
    /// `script_mod` on a style change, so the answer is re-read then.
    pub fn for_vm(vm: &mut ScriptVm) -> Self {
        let light = forced_light().or_else(|| makepad_wm_theme::current_for_vm(vm).map(|p| p.light_mode)).unwrap_or(true);
        LAST_SKIN.store(if light { 1 } else { 2 }, Ordering::Relaxed);
        Self::for_mode(light)
    }

    /// The skin the last `for_vm` picked: what the Rust side tints with
    /// (the selected tab) between DSL applies. Light before any.
    pub fn current() -> Self {
        Self::for_mode(LAST_SKIN.load(Ordering::Relaxed) != 2)
    }
}

/// 0: not forced; 1: light; 2: dark.
static FORCED_SKIN: AtomicU8 = AtomicU8::new(0);
/// The last skin `Skin::for_vm` resolved, the same encoding.
static LAST_SKIN: AtomicU8 = AtomicU8::new(0);

/// Force the skin for every isolate in this process, before the crate's
/// `script_mod` runs; `None` follows the palette again.
pub fn force_skin(light: Option<bool>) {
    FORCED_SKIN.store(
        match light {
            None => 0,
            Some(true) => 1,
            Some(false) => 2,
        },
        Ordering::Relaxed,
    );
}

fn forced_light() -> Option<bool> {
    match FORCED_SKIN.load(Ordering::Relaxed) {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// The page the bottom bar picks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Root {
    #[default]
    Today,
    Following,
    Saved,
}

/// A page pushed over the root: a source's own page, or Search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pushed {
    Source(usize),
    Search,
}

/// What the full face shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Today,
    Following,
    Saved,
    Search,
    Source(usize),
}

/// Where the person is: a root page from the bar, with at most one page
/// pushed over it. The reader is not a page; it lies over everything.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Nav {
    pub root: Root,
    pub pushed: Option<Pushed>,
}

impl Nav {
    /// The bar picked a root: a pushed page goes. False when the person
    /// picked the page already showing (the view scrolls it to the top).
    pub fn pick(&mut self, root: Root) -> bool {
        let changed = self.root != root || self.pushed.is_some();
        self.root = root;
        self.pushed = None;
        changed
    }

    pub fn push(&mut self, page: Pushed) {
        self.pushed = Some(page);
    }

    /// Back: the pushed page goes. False when there was none to pop, so
    /// the host may take the press.
    pub fn pop(&mut self) -> bool {
        self.pushed.take().is_some()
    }

    pub fn page(&self) -> Page {
        match (self.pushed, self.root) {
            (Some(Pushed::Source(i)), _) => Page::Source(i),
            (Some(Pushed::Search), _) => Page::Search,
            (None, Root::Today) => Page::Today,
            (None, Root::Following) => Page::Following,
            (None, Root::Saved) => Page::Saved,
        }
    }

    /// The tab the tick refreshes here: a source's own on its page, Today's
    /// followed set everywhere else.
    pub fn fetch_tab(&self) -> usize {
        match self.pushed {
            Some(Pushed::Source(i)) => i + 1,
            _ => 0,
        }
    }
}

const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// `Wednesday, September 16`: the local date of `secs` (unix), in the
/// platform's zone where it has one, else UTC.
pub fn date_line(secs: i64) -> String {
    let (month, day, weekday) = local_date::local(secs).unwrap_or_else(|| civil_date(secs));
    format!("{}, {} {}", WEEKDAYS[weekday], MONTHS[month.saturating_sub(1).min(11)], day)
}

/// The UTC (month, day, weekday) of `secs`: the fallback and the tests'
/// ground truth. Sunday is weekday 0; 1970-01-01 was a Thursday.
pub(crate) fn civil_date(secs: i64) -> (usize, usize, usize) {
    let days = secs.div_euclid(86_400);
    let weekday = (days + 4).rem_euclid(7) as usize;
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as usize;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as usize;
    (month, day, weekday)
}

#[cfg(unix)]
mod local_date {
    use std::os::raw::{c_char, c_int, c_long};

    #[repr(C)]
    struct Tm {
        tm_sec: c_int,
        tm_min: c_int,
        tm_hour: c_int,
        tm_mday: c_int,
        tm_mon: c_int,
        tm_year: c_int,
        tm_wday: c_int,
        tm_yday: c_int,
        tm_isdst: c_int,
        tm_gmtoff: c_long,
        tm_zone: *const c_char,
    }

    extern "C" {
        fn tzset();
        fn localtime_r(time: *const c_long, out: *mut Tm) -> *mut Tm;
    }

    /// The local (month, day, weekday) of `secs`, from the C library.
    pub fn local(secs: i64) -> Option<(usize, usize, usize)> {
        let t: c_long = c_long::try_from(secs).ok()?;
        let mut tm = Tm {
            tm_sec: 0,
            tm_min: 0,
            tm_hour: 0,
            tm_mday: 1,
            tm_mon: 0,
            tm_year: 70,
            tm_wday: 4,
            tm_yday: 0,
            tm_isdst: 0,
            tm_gmtoff: 0,
            tm_zone: std::ptr::null(),
        };
        // SAFETY: `tm` is a properly laid out struct tm the C library fills;
        // `t` outlives the call; localtime_r is the re-entrant form.
        let ok = unsafe {
            tzset();
            !localtime_r(&t, &mut tm).is_null()
        };
        ok.then(|| ((tm.tm_mon + 1).clamp(1, 12) as usize, tm.tm_mday.clamp(1, 31) as usize, tm.tm_wday.clamp(0, 6) as usize))
    }
}

#[cfg(not(unix))]
mod local_date {
    pub fn local(_secs: i64) -> Option<(usize, usize, usize)> {
        None
    }
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

/// A transport error in plain words. The Android backend reports a failed
/// request as the Java exception chain (`java.util.concurrent.
/// CompletionException: java.net.UnknownHostException: Unable to resolve
/// host …`); the person needs "no internet connection", not the chain.
/// Anything unrecognised keeps its message, minus the exception wrappers.
pub(crate) fn plain_error(message: &str) -> String {
    let lower = message.to_lowercase();
    let known = [
        (&["unknownhost", "unable to resolve host", "no address associated", "dns"][..], "no internet connection"),
        (&["sslhandshake", "certificate", "ssl", "tls"][..], "secure connection failed"),
        (&["timed out", "timeout"][..], "timed out"),
        (&["connectexception", "failed to connect", "connection refused", "unreachable", "network is unreachable", "econnreset", "connection reset"][..], "could not connect"),
    ];
    for (needles, words) in known {
        if needles.iter().any(|n| lower.contains(n)) {
            return words.to_string();
        }
    }
    // The last exception in a chain carries the message; drop the wrappers
    // and the package names.
    let last = message.rsplit(": ").find(|part| !part.trim().is_empty()).unwrap_or(message).trim();
    let last = last.rsplit('.').next().unwrap_or(last);
    if last.is_empty() { message.trim().to_string() } else { last.to_string() }
}

/// Parse one source's body into rows, by its kind. Zero rows is an error:
/// the tab keeps what it had rather than going blank.
pub(crate) fn parse_body(kind: SourceKind, body: &str) -> Result<Vec<Headline>, String> {
    let rows = match kind {
        SourceKind::HackerNews => crate::hn::parse(body)?,
        SourceKind::Feed { split_source } => crate::feed::parse(body, split_source)?,
        SourceKind::Digest => crate::feed::parse_digest(body)?,
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
/// rows and fetch state, which sources the person follows, the saved
/// stories, and the tab whose sources the tick refreshes.
pub struct NewsModel {
    pub sources: Vec<SourceState>,
    /// 0 is Today (every followed source); `n` is `sources[n - 1]`'s own
    /// page. The tick refreshes this tab's sources.
    pub tab: usize,
    /// The ids of sources switched off on the Following page: left out of
    /// Today, the tile and Search; still listed, their own page reachable.
    pub hidden: BTreeSet<String>,
    /// The saved stories, newest first, one per link, at most `MAX_SAVED`.
    pub saved: Vec<Headline>,
    seq: u64,
}

impl Default for NewsModel {
    fn default() -> Self {
        Self::new()
    }
}

impl NewsModel {
    pub fn new() -> Self {
        NewsModel {
            sources: builtin_sources().into_iter().map(SourceState::new).collect(),
            tab: 0,
            hidden: BTreeSet::new(),
            saved: Vec::new(),
            seq: 0,
        }
    }

    pub fn tab_count(&self) -> usize {
        self.sources.len() + 1
    }

    pub fn tab_label(&self, tab: usize) -> &str {
        if tab == 0 {
            "Today"
        } else {
            self.sources.get(tab - 1).map(|s| s.def.label.as_str()).unwrap_or("")
        }
    }

    /// Change the tab the tick refreshes. False when nothing changed.
    pub fn select_tab(&mut self, tab: usize) -> bool {
        if tab >= self.tab_count() || tab == self.tab {
            return false;
        }
        self.tab = tab;
        true
    }

    /// The source indices a tab shows: every followed source for Today.
    pub fn sources_for_tab(&self, tab: usize) -> Vec<usize> {
        if tab == 0 {
            self.followed_sources()
        } else {
            vec![tab - 1]
        }
    }

    pub fn rows_for_tab(&self, tab: usize) -> Vec<Headline> {
        if tab == 0 {
            let slices: Vec<&[Headline]> = self.followed_sources().into_iter().map(|i| self.sources[i].rows.as_slice()).collect();
            interleave(&slices)
        } else {
            self.sources.get(tab - 1).map(|s| s.rows.clone()).unwrap_or_default()
        }
    }

    /// The sources the person follows, in tab order: every source not
    /// switched off on the Following page.
    pub fn followed_sources(&self) -> Vec<usize> {
        (0..self.sources.len()).filter(|i| self.is_followed(*i)).collect()
    }

    pub fn is_followed(&self, source: usize) -> bool {
        self.sources.get(source).is_some_and(|s| !self.hidden.contains(&s.def.id))
    }

    /// Follow or hide a source. Returns whether anything changed.
    pub fn set_followed(&mut self, source: usize, followed: bool) -> bool {
        let Some(id) = self.sources.get(source).map(|s| s.def.id.clone()) else { return false };
        if followed {
            self.hidden.remove(&id)
        } else {
            self.hidden.insert(id)
        }
    }

    /// `hidden.json`: the hidden ids as a JSON array.
    pub fn hidden_bytes(&self) -> Vec<u8> {
        self.hidden.iter().cloned().collect::<Vec<String>>().serialize_json().into_bytes()
    }

    /// Seed the hidden set from `hidden.json`; garbage leaves it empty.
    pub fn load_hidden(&mut self, bytes: &[u8]) -> bool {
        let Ok(text) = std::str::from_utf8(bytes) else { return false };
        let Ok(ids) = <Vec<String> as DeJson>::deserialize_json_lenient(text) else { return false };
        self.hidden = ids.into_iter().collect();
        true
    }

    /// Today's sections: one per followed source with rows, each with its
    /// top `SECTION_ROWS` stories (the first is the section's hero).
    pub fn today_sections(&self) -> Vec<Section> {
        self.followed_sources()
            .into_iter()
            .filter(|i| !self.sources[*i].rows.is_empty())
            .map(|i| Section { source: i, rows: self.sources[i].rows.iter().take(SECTION_ROWS).cloned().collect() })
            .collect()
    }

    /// The rows of every followed source whose title, publisher or summary
    /// contains `query` (case-insensitively), in Today's order, at most
    /// `MAX_SEARCH_ROWS`. An empty query matches nothing.
    pub fn search(&self, query: &str) -> Vec<Headline> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        self.rows_for_tab(0)
            .into_iter()
            .filter(|row| {
                [row.title.as_str(), row.source.as_str(), row.summary.as_str()]
                    .iter()
                    .any(|field| field.to_lowercase().contains(&needle))
            })
            .take(MAX_SEARCH_ROWS)
            .collect()
    }

    pub fn is_saved(&self, link: &str) -> bool {
        self.saved.iter().any(|row| row.link == link)
    }

    /// Save a story, or unsave it when it is saved. Returns whether it is
    /// saved afterwards. Newest first, one per link, capped.
    pub fn toggle_saved(&mut self, row: &Headline) -> bool {
        if let Some(pos) = self.saved.iter().position(|r| r.link == row.link) {
            self.saved.remove(pos);
            return false;
        }
        self.saved.insert(0, row.clone());
        self.saved.truncate(MAX_SAVED);
        true
    }

    /// `saved.json`: the saved rows as JSON.
    pub fn saved_bytes(&self) -> Vec<u8> {
        self.saved.serialize_json().into_bytes()
    }

    /// Seed the saved list from `saved.json`; rows the faces would not show
    /// are dropped, as the cache does.
    pub fn load_saved(&mut self, bytes: &[u8]) -> bool {
        let Ok(text) = std::str::from_utf8(bytes) else { return false };
        let Ok(rows) = <Vec<Headline> as DeJson>::deserialize_json_lenient(text) else { return false };
        self.saved = rows.into_iter().filter(showable).take(MAX_SAVED).collect();
        true
    }

    /// The person's own feeds, in order, as the feeds file spells them.
    pub fn user_feeds(&self) -> Vec<UserFeed> {
        self.sources.iter().skip(builtin_count()).map(|s| UserFeed { label: s.def.label.clone(), url: s.def.url.clone() }).collect()
    }

    /// `feeds.json`, for writing back after an add or a remove.
    pub fn feeds_bytes(&self) -> Vec<u8> {
        self.user_feeds().serialize_json().into_bytes()
    }

    /// Add one feed from the Following page: an http(s) URL not already a
    /// source, under the cap. Returns the new source's index.
    pub fn add_user_feed(&mut self, label: &str, url: &str) -> Result<usize, String> {
        let url = url.trim();
        if !is_http_url(url) {
            return Err("Enter a feed address starting with http:// or https://".into());
        }
        if self.sources.iter().any(|s| s.def.url == url) {
            return Err("That feed is already a source".into());
        }
        if self.user_feeds().len() >= MAX_USER_FEEDS {
            return Err(format!("Up to {MAX_USER_FEEDS} feeds; remove one first"));
        }
        let mut feeds = self.user_feeds();
        feeds.push(UserFeed { label: label.trim().to_string(), url: url.to_string() });
        self.set_user_feeds(feeds);
        Ok(self.sources.len() - 1)
    }

    /// Remove one of the person's feeds; a built-in cannot be removed. Its
    /// hidden flag goes with it. Returns the in-flight requests to cancel.
    pub fn remove_user_feed(&mut self, source: usize) -> Result<Vec<LiveId>, String> {
        let builtin = builtin_count();
        if source < builtin || source >= self.sources.len() {
            return Err("Only your own feeds can be removed".into());
        }
        let id = self.sources[source].def.id.clone();
        self.hidden.remove(&id);
        let mut feeds = self.user_feeds();
        feeds.remove(source - builtin);
        let (_, cancelled) = self.set_user_feeds(feeds);
        Ok(cancelled)
    }

    /// The name a row shows as its publisher: the outlet the row names (a
    /// Google News row), else its source's label.
    pub fn publisher(&self, row: &Headline) -> String {
        if !row.source.trim().is_empty() {
            return row.source.trim().to_string();
        }
        self.sources.iter().find(|s| s.def.id == row.source_id).map(|s| s.def.label.clone()).unwrap_or_else(|| "Feed".into())
    }

    /// How many rows a tab has, without building them.
    pub fn row_count(&self, tab: usize) -> usize {
        self.sources_for_tab(tab).into_iter().map(|i| self.sources[i].rows.len()).sum()
    }

    /// The home tile: the top of Today, one row per followed source.
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
            Ok(mut rows) => {
                // Provenance is the model's to stamp: the parsers share
                // one feed reader, and the All interleave mixes sources.
                for row in &mut rows {
                    row.source_id = state.def.id.clone();
                }
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
    /// Rows the faces would not show (no title, no http link) are dropped, a
    /// discussion that is no http link is cleared, and the count is capped,
    /// as if the document had just been parsed. The document is this
    /// source's, so every row gets its id, whatever the row says.
    pub fn load_cache(&mut self, source: usize, bytes: &[u8]) -> bool {
        let state = &mut self.sources[source];
        if state.fetched_at.is_some() || !state.rows.is_empty() {
            return false;
        }
        let Ok(text) = std::str::from_utf8(bytes) else { return false };
        let Ok(rows) = <Vec<Headline> as DeJson>::deserialize_json_lenient(text) else { return false };
        let source_id = state.def.id.as_str();
        let rows: Vec<Headline> = rows
            .into_iter()
            .filter(showable)
            .take(MAX_ROWS_PER_SOURCE)
            .map(|mut row| {
                row.source_id = source_id.to_string();
                row.discussion = row.discussion.filter(|d| is_http_url(d));
                row
            })
            .collect();
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
        let builtin = builtin_count();
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

/// A hero's deck: the feed's summary, unless there is none or it only
/// repeats the headline (a feed whose description is its title again).
pub(crate) fn deck_text(row: &Headline) -> Option<&str> {
    let summary = row.summary.trim();
    if summary.is_empty() {
        return None;
    }
    let title = row.title.trim().trim_end_matches('…').trim();
    let repeats = !title.is_empty() && summary.to_lowercase().starts_with(&title.to_lowercase());
    (!repeats).then_some(summary)
}

/// One tile line: the title, with its source when the row names one.
pub(crate) fn tile_line(row: &Headline) -> String {
    if row.source.is_empty() {
        row.title.clone()
    } else {
        format!("{} · {}", row.title, row.source)
    }
}

/// The second line of a row: time, points and comments, joined by dots.
/// The source is the badge's, not repeated here.
pub(crate) fn meta_line(row: &Headline, now: i64) -> String {
    let mut parts: Vec<String> = Vec::new();
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
        assert_eq!(builtin_sources()[1].kind, SourceKind::Digest);
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
        let mut stamped = vec![row("s1")];
        stamped[0].source_id = "techmeme".into();
        assert_eq!(interleave(&[&stamped, &b])[0].source_id, "techmeme", "a row keeps its provenance");
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
        let h = Headline {
            title: "T \"q\"".into(),
            link: "https://x".into(),
            source: "S".into(),
            source_id: "hn".into(),
            published: Some(1_700_000_000),
            points: Some(12),
            comments: None,
            summary: "s".into(),
            discussion: Some("https://news.ycombinator.com/item?id=1".into()),
            image: Some("https://x/i.jpg".into()),
        };
        let json = h.serialize_json();
        let back: Headline = DeJson::deserialize_json_lenient(&json).unwrap();
        assert_eq!(back, h);
        let rows: Vec<Headline> = DeJson::deserialize_json_lenient(&vec![h.clone()].serialize_json()).unwrap();
        assert_eq!(rows, vec![h]);
    }

    #[test]
    fn a_row_cached_before_discussion_existed_still_loads() {
        let old = r#"{"title":"Old","link":"https://x/old","source":"S","published":null,"points":null,"comments":null,"summary":""}"#;
        let back: Headline = DeJson::deserialize_json_lenient(old).unwrap();
        assert_eq!((back.title.as_str(), back.discussion, back.image), ("Old", None, None));
        assert_eq!(back.source_id, "", "no id in the document");
        let mut m = NewsModel::new();
        assert!(m.load_cache(0, format!("[{old}]").as_bytes()));
        assert_eq!(m.sources[0].rows[0].discussion, None);
        assert_eq!(m.sources[0].rows[0].source_id, "hn", "the cache is per source, so the id is known");
    }

    #[test]
    fn rows_carry_their_source_id_from_the_fetch_and_the_cache_and_through_all() {
        let mut m = NewsModel::new();
        m.set_user_feeds(vec![UserFeed { label: "Blog".into(), url: "https://blog.example/feed".into() }]);
        let user_id = m.sources[3].def.id.clone();
        for (i, prefix) in ["h", "t", "g", "u"].iter().enumerate() {
            let (id, _) = m.begin_fetch(i);
            let mut rows = rows(prefix, 2);
            // A parser leaves the id empty (feeds share one), a row that
            // claims one is corrected: the source the rows land in decides.
            rows[1].source_id = "hn".into();
            if i == 2 {
                rows[0].source = "Reuters".into();
            }
            m.complete(id, Ok(rows));
        }
        for (i, expected) in ["hn", "techmeme", "google", user_id.as_str()].iter().enumerate() {
            assert!(m.sources[i].rows.iter().all(|r| r.source_id == *expected), "source {i} stamps {expected}");
        }
        let all = m.rows_for_tab(0);
        let ids: Vec<&str> = all.iter().map(|r| r.source_id.as_str()).collect();
        assert_eq!(ids, ["hn", "techmeme", "google", &user_id, "hn", "techmeme", "google", &user_id], "All keeps the provenance");
        assert_eq!((all[2].source.as_str(), all[2].source_id.as_str()), ("Reuters", "google"), "the outlet stays the outlet");
        assert!(m.tile_rows().iter().all(|r| !r.source_id.is_empty()));
        // A cache document is this source's, whatever its rows say.
        let mut stamped = rows("c", 1);
        stamped[0].source_id = "hn".into();
        let mut fresh = NewsModel::new();
        assert!(fresh.load_cache(2, stamped.serialize_json().as_bytes()));
        assert_eq!(fresh.sources[2].rows[0].source_id, "google");
    }

    #[test]
    fn hosting_defaults_to_standalone() {
        assert_eq!(Hosting::default(), Hosting::Standalone);
    }

    #[test]
    fn source_colors_are_fixed_per_builtin_and_slate_for_the_rest() {
        assert_eq!(source_color("hn"), 0xff6600ff);
        assert_eq!(source_color("techmeme"), 0x2bb5a0ff);
        assert_eq!(source_color("google"), 0x4285f4ff);
        assert_eq!(source_color("user_0123456789abcdef"), 0x7d8aa5ff, "a user feed is slate");
        assert_eq!(source_color(""), 0x7d8aa5ff, "so is a row with no provenance");
    }

    #[test]
    fn wm_unavailable_parses_the_hosts_envelope_and_nothing_else() {
        let u = WmUnavailable::parse(r#"{"wm_unavailable":{"app":"browser","path":"https://x/a?b=1"}}"#).unwrap();
        assert_eq!((u.app.as_str(), u.path.as_str()), ("browser", "https://x/a?b=1"));
        assert_eq!(WmUnavailable::parse(&u.to_json()), Some(u), "the shape the host sends round-trips");
        let extra = r#"{"wm_unavailable":{"app":"browser","path":"https://x/a","reason":"not installed"}}"#;
        assert_eq!(WmUnavailable::parse(extra).map(|u| u.app), Some("browser".into()), "a field this crate does not know is skipped");
        assert_eq!(WmUnavailable::parse(r#"{"wm":{"CloseRequested":{}}}"#), None);
        assert_eq!(WmUnavailable::parse("garbage"), None);
        assert_eq!(WmUnavailable::parse(r#"{"wm_unavailable":{"app":"browser"}}"#), None, "a missing path is not the envelope");
    }

    #[test]
    fn open_policy_tiers_follow_hosting_and_platform() {
        use OpenTier::*;
        let mac = |hosting| OpenPolicy { hosting, has_webview: true, has_system_open: true };
        assert_eq!(mac(Hosting::Process).tiers(), [Browser, System, Notify], "a process child has no window for a reader");
        assert_eq!(mac(Hosting::Module).tiers(), [Reader, Browser, System, Notify], "the reader first, the Browser behind it");
        assert_eq!(mac(Hosting::Standalone).tiers(), [Reader, System, Notify]);
        assert_eq!(mac(Hosting::Module).browser_tiers(), [Browser, System, Notify], "Open in Browser skips the reader");
        assert_eq!(mac(Hosting::Standalone).browser_tiers(), [System, Notify]);
        let bare = |hosting| OpenPolicy { hosting, has_webview: false, has_system_open: false };
        assert_eq!(bare(Hosting::Module).tiers(), [Browser, Notify]);
        assert_eq!(bare(Hosting::Standalone).tiers(), [Notify]);
        assert_eq!(bare(Hosting::Process).tiers(), [Browser, Notify]);
        let here = OpenPolicy::for_platform(Hosting::Standalone);
        assert_eq!(here.hosting, Hosting::Standalone);
        #[cfg(target_os = "macos")]
        assert!(here.has_webview && here.has_system_open, "this Mac has WKWebView and open");
        #[cfg(target_os = "linux")]
        assert!(!here.has_webview && !here.has_system_open);
    }

    fn rows(prefix: &str, n: usize) -> Vec<Headline> {
        (1..=n).map(|i| row(&format!("{prefix}{i}"))).collect()
    }

    #[test]
    fn tabs_are_today_then_every_source() {
        let mut m = NewsModel::new();
        assert_eq!(m.tab_count(), 4);
        assert_eq!(m.tab_label(0), "Today");
        assert_eq!(m.tab_label(1), "Hacker News");
        assert_eq!(m.sources_for_tab(0), vec![0, 1, 2]);
        assert_eq!(m.sources_for_tab(2), vec![1]);
        assert!(m.select_tab(2));
        assert!(!m.select_tab(2), "same tab: nothing changed");
        assert!(!m.select_tab(99), "out of range is ignored");
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
        let mut fetched = rows("h", 2);
        fetched[0].discussion = Some("https://news.ycombinator.com/item?id=1".into());
        m.complete(id, Ok(fetched));
        let bytes = m.cache_bytes(0);
        let mut fresh = NewsModel::new();
        assert!(fresh.load_cache(0, &bytes));
        assert_eq!(fresh.sources[0].rows.len(), 2);
        assert_eq!(fresh.sources[0].rows[0].discussion.as_deref(), Some("https://news.ycombinator.com/item?id=1"));
        assert_eq!(fresh.sources[0].rows[1].discussion, None);

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
        let (pending, _) = m.begin_fetch(4);
        let (added, cancelled) = m.set_user_feeds(vec![blog.clone(), only.clone()]);
        assert_eq!(added, 3..5);
        assert_eq!(cancelled, vec![pending], "the replaced feed's in-flight request comes back for cancelling");
        assert_eq!(m.sources[3].def.id, blog_id, "the same url gets the same id");
        assert_eq!(m.tab, 4);
        m.select_tab(5);
        let (added, _) = m.set_user_feeds(vec![only]);
        assert_eq!(added, 3..4);
        assert_eq!(m.tab_count(), 5);
        assert_eq!(m.tab, 4, "the selected tab is clamped into range");
        assert_eq!(m.source_index("only"), Some(3), "labels match case-insensitively");
        assert_eq!(m.source_index("HN"), Some(0), "so do ids");
        assert_eq!(m.source_index("nope"), None);
        m.select_tab(1);
        m.set_user_feeds(vec![blog]);
        assert_eq!(m.tab, 1, "a built-in tab stays selected");
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
        let mut stamped = good.clone();
        stamped.source_id = "hn".into();
        assert_eq!(m.sources[0].rows, vec![stamped], "the showable row, stamped with the source it was loaded into");
        assert!(!m.load_cache(1, vec![blank_title, odd_link].serialize_json().as_bytes()), "nothing showable is nothing");
        let many: Vec<Headline> = (0..MAX_ROWS_PER_SOURCE + 10).map(|i| row(&format!("r{i}"))).collect();
        assert!(m.load_cache(2, many.serialize_json().as_bytes()));
        assert_eq!(m.sources[2].rows.len(), MAX_ROWS_PER_SOURCE);
        // A discussion is a link too: one that is no http url is cleared,
        // the row itself is kept.
        let mut scripted = row("scripted");
        scripted.discussion = Some("javascript:alert(2)".into());
        let mut kept = row("kept");
        kept.discussion = Some("https://news.ycombinator.com/item?id=1".into());
        let mut fresh = NewsModel::new();
        assert!(fresh.load_cache(0, vec![scripted, kept].serialize_json().as_bytes()));
        assert_eq!((fresh.sources[0].rows[0].title.as_str(), fresh.sources[0].rows[0].discussion.as_deref()), ("scripted", None));
        assert_eq!(fresh.sources[0].rows[1].discussion.as_deref(), Some("https://news.ycombinator.com/item?id=1"));
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
    fn transport_errors_are_said_in_plain_words() {
        assert_eq!(plain_error("java.util.concurrent.CompletionException: java.net.UnknownHostException: Unable to resolve host \"hn.algolia.com\": No address associated with hostname"), "no internet connection");
        assert_eq!(plain_error("javax.net.ssl.SSLHandshakeException: Chain validation failed"), "secure connection failed");
        assert_eq!(plain_error("java.net.SocketTimeoutException: timeout"), "timed out");
        assert_eq!(plain_error("Failed to connect to www.techmeme.com/1.2.3.4:443"), "could not connect");
        assert_eq!(plain_error("java.util.concurrent.CompletionException: java.io.IOException: unexpected end of stream"), "unexpected end of stream");
        assert_eq!(plain_error("HTTP 503"), "HTTP 503");
        assert_eq!(plain_error("  "), "");
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
    fn the_deck_is_the_summary_unless_it_repeats_the_headline() {
        let mut h = row("A look at AI");
        assert_eq!(deck_text(&h), None, "no summary");
        h.summary = "A look at AI, and more".into();
        assert_eq!(deck_text(&h), None, "the summary opens with the headline");
        h.summary = "Posted by someone".into();
        assert_eq!(deck_text(&h), Some("Posted by someone"));
        h.title = "A look at AI…".into();
        h.summary = "a look at ai in warfare".into();
        assert_eq!(deck_text(&h), None, "case and a cut headline's ellipsis do not matter");
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
        // The badge carries the provenance; the meta line never repeats it.
        h.source = "Hacker News".into();
        h.published = Some(1_000_000 - 3 * 3600);
        h.points = Some(10);
        h.comments = Some(2);
        assert_eq!(meta_line(&h, 1_000_000), "3h ago · 10 points · 2 comments");
        h.points = Some(1);
        h.comments = Some(1);
        assert_eq!(meta_line(&h, 1_000_000), "3h ago · 1 point · 1 comment");
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
        let techmeme = include_str!("../tests/fixtures/techmeme.xml");
        assert_eq!(parse_body(SourceKind::Digest, techmeme).unwrap()[0].source, "", "the fixture's titles name no outlet");
        let many: String = (0..50).map(|i| format!("<item><title>t{i}</title><link>https://x/{i}</link></item>")).collect();
        let feed = format!("<rss><channel>{many}</channel></rss>");
        assert_eq!(parse_body(SourceKind::Feed { split_source: false }, &feed).unwrap().len(), MAX_ROWS_PER_SOURCE);
    }

    #[test]
    fn the_skin_has_two_variants_and_can_be_forced() {
        let light = Skin::for_mode(true);
        let dark = Skin::for_mode(false);
        assert!(light.light && !dark.light);
        assert_eq!(light.ground, Vec4f::from_u32(0xf2f2f7ff));
        assert_eq!(dark.ground, Vec4f::from_u32(0x000000ff));
        assert_eq!(light.accent, dark.accent, "the accent is the same red in both");
        assert_ne!(light.ink, dark.ink);
        force_skin(Some(false));
        assert_eq!(forced_light(), Some(false));
        force_skin(None);
        assert_eq!(forced_light(), None);
    }

    #[test]
    fn nav_picks_roots_pushes_pages_and_pops_back() {
        let mut nav = Nav::default();
        assert_eq!(nav.page(), Page::Today);
        assert!(!nav.pick(Root::Today), "the page already showing: a scroll to the top, not a change");
        assert!(nav.pick(Root::Saved));
        assert_eq!(nav.page(), Page::Saved);
        nav.push(Pushed::Source(1));
        assert_eq!((nav.page(), nav.fetch_tab()), (Page::Source(1), 2));
        assert!(nav.pick(Root::Saved), "picking the root under a pushed page pops it");
        assert_eq!(nav.page(), Page::Saved);
        nav.push(Pushed::Search);
        assert_eq!((nav.page(), nav.fetch_tab()), (Page::Search, 0));
        assert!(nav.pop());
        assert_eq!(nav.page(), Page::Saved);
        assert!(!nav.pop(), "nothing pushed: the host may take the back press");
        assert!(nav.pick(Root::Following));
        assert_eq!(nav.page(), Page::Following);
    }

    #[test]
    fn hidden_sources_leave_today_the_tile_and_search_but_stay_listed() {
        let mut m = NewsModel::new();
        for (i, prefix) in ["h", "t", "g"].iter().enumerate() {
            let (id, _) = m.begin_fetch(i);
            m.complete(id, Ok(rows(prefix, 3)));
        }
        assert!(m.set_followed(1, false));
        assert!(!m.set_followed(1, false), "already hidden");
        assert!(!m.is_followed(1) && m.is_followed(0));
        assert_eq!(m.followed_sources(), vec![0, 2]);
        assert_eq!(m.sources_for_tab(0), vec![0, 2], "the tick skips a hidden source");
        let today: Vec<String> = m.rows_for_tab(0).iter().map(|h| h.title.clone()).collect();
        assert_eq!(today, ["h1", "g1", "h2", "g2", "h3", "g3"]);
        let tile: Vec<String> = m.tile_rows().iter().map(|h| h.title.clone()).collect();
        assert_eq!(tile, ["h1", "g1", "h2"]);
        assert_eq!(m.today_sections().iter().map(|s| s.source).collect::<Vec<_>>(), [0, 2]);
        assert!(m.search("t1").is_empty(), "a hidden source's rows are not searched");
        assert_eq!(m.rows_for_tab(2).len(), 3, "its own page still has them");
        assert_eq!(m.sources.len(), 3, "and it is still a source");
        let bytes = m.hidden_bytes();
        let mut again = NewsModel::new();
        assert!(again.load_hidden(&bytes));
        assert_eq!(again.hidden, m.hidden);
        assert!(!again.load_hidden(b"nonsense"));
        assert!(m.set_followed(1, true));
        assert_eq!(m.followed_sources(), vec![0, 1, 2]);
        assert!(!m.set_followed(99, false), "no such source");
    }

    #[test]
    fn today_sections_take_five_rows_per_followed_source_with_rows() {
        let mut m = NewsModel::new();
        let (id, _) = m.begin_fetch(0);
        m.complete(id, Ok(rows("h", 8)));
        let (id, _) = m.begin_fetch(2);
        m.complete(id, Ok(rows("g", 2)));
        let sections = m.today_sections();
        assert_eq!(sections.len(), 2, "TechMeme has nothing yet: no section");
        assert_eq!(sections[0].source, 0);
        assert_eq!(sections[0].rows.len(), SECTION_ROWS);
        assert_eq!(sections[0].rows[0].title, "h1", "the first row is the hero");
        assert_eq!((sections[1].source, sections[1].rows.len()), (2, 2));
        let def = &m.sources[0].def;
        assert_eq!(section_caption(def), "Ranked by points");
        assert_eq!(section_caption(&m.sources[1].def), "Editors' picks");
        assert_eq!(section_caption(&m.sources[2].def), "Top stories");
        m.set_user_feeds(vec![UserFeed { label: "Blog".into(), url: "https://blog.example/feed".into() }]);
        assert_eq!(section_caption(&m.sources[3].def), "blog.example");
        assert_eq!(initial("hacker news"), "H");
        assert_eq!(initial("  "), "?");
    }

    #[test]
    fn search_matches_title_publisher_and_summary_case_insensitively_and_caps() {
        let mut m = NewsModel::new();
        let mut hn = rows("h", 3);
        hn[1].summary = "A story about Rust".into();
        let (id, _) = m.begin_fetch(0);
        m.complete(id, Ok(hn));
        let mut google = rows("g", 2);
        google[0].source = "Reuters".into();
        let (id, _) = m.begin_fetch(2);
        m.complete(id, Ok(google));
        assert_eq!(m.search("rust").iter().map(|h| h.title.as_str()).collect::<Vec<_>>(), ["h2"]);
        assert_eq!(m.search("REUTERS").iter().map(|h| h.title.as_str()).collect::<Vec<_>>(), ["g1"]);
        assert_eq!(m.search("h").len(), 3);
        assert!(m.search("   ").is_empty(), "an empty query matches nothing");
        let with_summary = |prefix: &str| {
            rows(prefix, 30).into_iter().map(|mut r| {
                r.summary = "shared".into();
                r
            }).collect::<Vec<_>>()
        };
        let (id, _) = m.begin_fetch(1);
        m.complete(id, Ok(with_summary("t")));
        let (id, _) = m.begin_fetch(0);
        m.complete(id, Ok(with_summary("h")));
        // h1, h10..h19, h21 and the same for t: twelve each, plus g1.
        assert_eq!(m.search("1").len(), 25);
        assert_eq!(m.search("shared").len(), MAX_SEARCH_ROWS, "sixty match: capped");
    }

    #[test]
    fn saved_stories_toggle_dedupe_stay_newest_first_cap_and_round_trip() {
        let mut m = NewsModel::new();
        let a = row("a");
        let b = row("b");
        assert!(m.toggle_saved(&a), "saved");
        assert!(m.is_saved(&a.link));
        assert!(m.toggle_saved(&b));
        assert_eq!(m.saved.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), ["b", "a"], "newest first");
        assert!(!m.toggle_saved(&a), "unsaved");
        assert!(!m.is_saved(&a.link));
        assert_eq!(m.saved.len(), 1);
        for i in 0..(MAX_SAVED + 5) {
            m.toggle_saved(&row(&format!("r{i}")));
        }
        assert_eq!(m.saved.len(), MAX_SAVED, "capped");
        assert_eq!(m.saved[0].title, format!("r{}", MAX_SAVED + 4), "the newest survives the cap");
        let bytes = m.saved_bytes();
        let mut again = NewsModel::new();
        assert!(again.load_saved(&bytes));
        assert_eq!(again.saved, m.saved);
        assert!(!again.load_saved(b"{"));
        assert!(again.load_saved(br#"[{"title":"","link":"https://x","source":"","summary":""}]"#));
        assert!(again.saved.is_empty(), "a row without a title is not shown, so not kept");
    }

    #[test]
    fn feeds_are_added_and_removed_from_the_following_page() {
        let mut m = NewsModel::new();
        assert!(m.add_user_feed("", "ftp://x").is_err(), "not http(s)");
        assert!(m.add_user_feed("", "https://www.techmeme.com/feed.xml").is_err(), "already a source");
        assert_eq!(m.add_user_feed(" Blog ", " https://blog.example/feed "), Ok(3));
        assert_eq!(m.tab_label(4), "Blog");
        assert_eq!(m.add_user_feed("", "https://b.example/feed"), Ok(4));
        assert_eq!(m.tab_label(5), "b.example", "no label: the host names it");
        assert!(m.add_user_feed("", "https://blog.example/feed").is_err(), "a duplicate of a feed");
        assert_eq!(m.user_feeds(), vec![
            UserFeed { label: "Blog".into(), url: "https://blog.example/feed".into() },
            UserFeed { label: "b.example".into(), url: "https://b.example/feed".into() },
        ]);
        let json = String::from_utf8(m.feeds_bytes()).unwrap();
        assert_eq!(parse_user_feeds(&json).unwrap(), m.user_feeds(), "the file round-trips through phase 1's parser");
        for i in 0..(MAX_USER_FEEDS - 2) {
            assert!(m.add_user_feed("", &format!("https://{i}.example/feed")).is_ok());
        }
        assert!(m.add_user_feed("", "https://one-more.example/feed").is_err(), "the cap holds");
        assert!(m.remove_user_feed(0).is_err(), "a built-in cannot be removed");
        assert!(m.remove_user_feed(99).is_err());
        m.set_followed(3, false);
        let (pending, _) = m.begin_fetch(3);
        assert_eq!(m.remove_user_feed(3), Ok(vec![pending]), "its in-flight request comes back");
        assert_eq!(m.tab_label(4), "b.example");
        assert!(m.hidden.is_empty(), "the removed feed's hidden flag goes with it");
        assert_eq!(m.user_feeds().len(), MAX_USER_FEEDS - 1);
    }

    #[test]
    fn the_publisher_is_the_rows_outlet_else_its_source() {
        let mut m = NewsModel::new();
        let mut h = row("t");
        h.source_id = "hn".into();
        assert_eq!(m.publisher(&h), "Hacker News");
        h.source = " Reuters ".into();
        assert_eq!(m.publisher(&h), "Reuters");
        h.source.clear();
        h.source_id = "gone".into();
        assert_eq!(m.publisher(&h), "Feed");
        m.set_user_feeds(vec![UserFeed { label: "Blog".into(), url: "https://blog.example/feed".into() }]);
        h.source_id = m.sources[3].def.id.clone();
        assert_eq!(m.publisher(&h), "Blog");
    }

    #[test]
    fn the_date_line_names_the_weekday_month_and_day() {
        // 2026-09-16 (a Wednesday) at noon UTC.
        let secs = crate::feed::days_from_civil(2026, 9, 16) * 86_400 + 12 * 3600;
        assert_eq!(civil_date(secs), (9, 16, 3));
        assert_eq!(civil_date(0), (1, 1, 4), "the epoch was a Thursday");
        assert_eq!(civil_date(crate::feed::days_from_civil(2000, 2, 29) * 86_400), (2, 29, 2));
        let line = date_line(secs);
        assert!(line.starts_with("Wednesday, September 1") || line.starts_with("Thursday, September 1"), "{line}: local zones near the date line may shift a day");
        assert!(WEEKDAYS.iter().any(|w| line.starts_with(w)) && line.contains(", "));
    }
}
