//! YouTube's grounded business tools and quiet local-time Glance suggestions.
//! Search results are fetched from YouTube's public results page; a model may
//! choose a query, but cannot publish or play an invented video id. No timer
//! plays audio. The person opens a card's in-content Play action explicitly.
//! The foreground host calls `tick`; this is not an Android background job.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const APP: &str = "os.youtube";
const MAX_CACHE: usize = 100;
const CARD: &str = include_str!("../../../apps/youtube/bundle/recommendation.card");
static STORE: Mutex<()> = Mutex::new(());
static CARDS: Mutex<()> = Mutex::new(());
static FOCUS: Mutex<BTreeMap<PathBuf, String>> = Mutex::new(BTreeMap::new());
static JOBS: OnceLock<Mutex<BTreeMap<PathBuf, Job>>> = OnceLock::new();
struct Job {
    running: bool,
    retry_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub length: String,
    pub query: String,
    pub retrieved_at: u64,
}
impl Video {
    fn row(&self) -> Value {
        let mut row = serde_json::to_value(self).unwrap();
        row["thumb"] = json!(format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", self.id));
        // Match the pinned sys.video helper: a top-level iframe /embed URL
        // has no referring page in WebReader and fails with Error 153 on
        // Android. The mobile watch page owns the player's normal context.
        row["embed"] = json!(format!("https://m.youtube.com/watch?v={}", self.id));
        row["url"] = json!(format!("https://www.youtube.com/watch?v={}", self.id));
        row["views"] = json!("");
        row["age"] = json!("");
        row
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Slot {
    pub id: String,
    pub label: String,
    pub hour: u8,
    pub query: String,
}
fn default_slots() -> Vec<Slot> {
    [
        (
            "morning",
            "Morning music",
            7,
            "gentle uplifting morning instrumental music",
        ),
        (
            "noon",
            "Noon break",
            12,
            "light acoustic instrumental lunch break",
        ),
        (
            "afternoon",
            "Afternoon relaxation",
            15,
            "relaxing afternoon lofi instrumental music",
        ),
        (
            "dinner",
            "Dinner music",
            18,
            "soft dinner jazz instrumental",
        ),
        (
            "sleep",
            "Wind down for sleep",
            21,
            "gentle ambient sleep music no vocals",
        ),
    ]
    .into_iter()
    .map(|(id, label, hour, query)| Slot {
        id: id.into(),
        label: label.into(),
        hour,
        query: query.into(),
    })
    .collect()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct State {
    // Quiet checks are an explicit UI/agent preference, never a side effect
    // of another app reading search results.
    enabled: bool,
    slots: Vec<Slot>,
    videos: Vec<Video>,
    published: BTreeMap<String, String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            enabled: false,
            slots: default_slots(),
            videos: Vec::new(),
            published: BTreeMap::new(),
        }
    }
}
fn path(root: &Path) -> PathBuf {
    root.join("youtube").join("recommendations.json")
}
fn load(root: &Path) -> Result<State, String> {
    match std::fs::read(path(root)) {
        Ok(bytes) if bytes.len() <= 256 * 1024 => serde_json::from_slice(&bytes)
            .map_err(|_| "YouTube recommendation data could not be read.".into()),
        Ok(_) => Err("YouTube recommendation data exceeds its limit.".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
        Err(_) => Err("YouTube recommendation data is unavailable.".into()),
    }
}
fn save(root: &Path, state: &State) -> Result<(), String> {
    let file = path(root);
    std::fs::create_dir_all(file.parent().unwrap())
        .map_err(|_| "Could not create YouTube storage.")?;
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(state).unwrap())
        .map_err(|_| "Could not save YouTube recommendations.")?;
    std::fs::rename(tmp, file)
        .map_err(|_| "Could not finish saving YouTube recommendations.".into())
}
fn text<'a>(args: &'a Value, field: &str) -> &'a str {
    args[field].as_str().unwrap_or("").trim()
}
fn valid_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn query(text: &str) -> Result<String, String> {
    let value = text.trim();
    if value.is_empty() || value.chars().count() > 160 || value.chars().any(char::is_control) {
        return Err("Provide a search query of 1–160 characters.".into());
    }
    Ok(value.into())
}
fn search_url(query: &str) -> String {
    let mut encoded = String::new();
    for b in query.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char)
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{b:02X}")),
        }
    }
    format!("https://www.youtube.com/results?search_query={encoded}")
}
fn rendered_text(v: &Value) -> String {
    v["simpleText"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| {
            v["runs"]
                .as_array()
                .map(|runs| runs.iter().filter_map(|r| r["text"].as_str()).collect())
                .unwrap_or_default()
        })
        .chars()
        .take(300)
        .collect()
}
/// Same source and `videoRenderer` boundary as Makepad's `sys.video` helper.
/// Parse each complete renderer as JSON: quotes, Unicode, nesting and adjacent
/// results cannot leak the next video's fields into a missing field.
fn parse_results(bytes: &[u8], query: &str, now: u64) -> Vec<Video> {
    let body = String::from_utf8_lossy(bytes);
    let mut rest = body.as_ref();
    let mut videos = Vec::new();
    while let Some(at) = rest.find("\"videoRenderer\"") {
        rest = &rest[at + "\"videoRenderer\"".len()..];
        let Some(value) = rest.trim_start().strip_prefix(':') else {
            continue;
        };
        let Some(Ok(v)) = serde_json::Deserializer::from_str(value.trim_start())
            .into_iter::<Value>()
            .next()
        else {
            continue;
        };
        let id = v["videoId"].as_str().unwrap_or("");
        let title = rendered_text(&v["title"]);
        if !valid_id(id) || title.is_empty() || videos.iter().any(|video: &Video| video.id == id) {
            continue;
        }
        let length = rendered_text(&v["lengthText"]);
        videos.push(Video {
            id: id.into(),
            title,
            channel: rendered_text(&v["longBylineText"]),
            length: if !length.is_empty() {
                length
            } else if v["badges"].as_array().is_some_and(|badges| {
                badges
                    .iter()
                    .any(|b| b["metadataBadgeRenderer"]["style"] == "BADGE_STYLE_TYPE_LIVE_NOW")
            }) || v["thumbnailOverlays"].as_array().is_some_and(|overlays| {
                overlays
                    .iter()
                    .any(|o| o["thumbnailOverlayTimeStatusRenderer"]["style"] == "LIVE")
            }) {
                "LIVE".into()
            } else {
                "—".into()
            },
            query: query.into(),
            retrieved_at: now,
        });
        if videos.len() >= 20 {
            break;
        }
    }
    videos
}
/// Use the same platform network backend as Splash's `sys.video`. In
/// particular Android uses its registered native networking adapter, while
/// Apple uses NSURLSession. Direct TLS sockets are not a substitute for those
/// platform paths (YouTube resets the legacy socket TLS handshake on macOS).
#[cfg(not(target_arch = "wasm32"))]
fn fetch(query: &str) -> Result<Vec<Video>, String> {
    use makepad_network::{
        HttpMethod, HttpRequest, NetworkConfig, NetworkResponse, NetworkRuntime,
    };
    let runtime = NetworkRuntime::new(NetworkConfig::default());
    let request_id =
        makepad_widgets::LiveId::from_str(&format!("youtube-search-{}", uuid::Uuid::new_v4()));
    let mut request = HttpRequest::new(search_url(query), HttpMethod::GET);
    request.set_header("Accept-Language".into(), "en-US,en;q=0.8".into());
    request.set_header("User-Agent".into(), "Mozilla/5.0 OctoSense/1.0".into());
    request.set_max_response_body_bytes(8 * 1024 * 1024);
    runtime
        .http_start(request_id, request)
        .map_err(|_| "YouTube networking could not start.")?;
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
            let _ = runtime.http_cancel(request_id);
            return Err(
                "YouTube search timed out. Try again when the connection is available.".into(),
            );
        };
        match runtime.recv_timeout(remaining) {
            Some(NetworkResponse::HttpResponse { response, .. }) => {
                if response.status_code != 200 {
                    return Err(format!(
                        "YouTube search returned HTTP {}.",
                        response.status_code
                    ));
                }
                let videos = parse_results(
                    response.body.as_deref().unwrap_or_default(),
                    query,
                    crate::glance::now_ms(),
                );
                if videos.is_empty() {
                    return Err("YouTube returned no readable video results. It may require consent or be temporarily unavailable; no recommendation was invented.".into());
                }
                return Ok(videos);
            }
            Some(NetworkResponse::HttpError { .. }) => {
                return Err(
                    "YouTube search failed. Check the connection or try again later.".into(),
                )
            }
            _ => continue,
        }
    }
}
#[cfg(target_arch = "wasm32")]
fn fetch(_query: &str) -> Result<Vec<Video>, String> {
    Err("YouTube agent search currently requires a native OctoSense host.".into())
}

fn cache(root: &Path, videos: &[Video]) -> Result<(), String> {
    let _lock = STORE.lock().unwrap();
    let mut state = load(root)?;
    for video in videos.iter().rev() {
        state.videos.retain(|v| v.id != video.id);
        state.videos.insert(0, video.clone());
    }
    state.videos.truncate(MAX_CACHE);
    save(root, &state)
}
fn known(root: &Path, id: &str) -> Result<Video, String> {
    if !valid_id(id) {
        return Err("Provide a video id returned by youtube.search or youtube.recommend.".into());
    }
    if let Some(video) = load(root)?.videos.into_iter().find(|v| v.id == id) {
        return Ok(video);
    }
    // A later search may evict a result, but a live card must still open its
    // original identity. Publications retain the exact searched metadata.
    publications(root)?
        .into_iter()
        .find(|p| p.video.id == id)
        .map(|p| p.video)
        .ok_or_else(|| {
            "This video is not in YouTube's searched results. Search again first.".into()
        })
}
#[derive(Clone, Serialize, Deserialize)]
struct Publication {
    video: Video,
    slot: Slot,
    expires_at: u64,
    dismissed: bool,
}
fn publications(root: &Path) -> Result<Vec<Publication>, String> {
    match std::fs::read(root.join("youtube/cards.json")) {
        Ok(bytes) if bytes.len() <= 64 * 1024 => serde_json::from_slice(&bytes)
            .map_err(|_| "YouTube card records are unreadable.".into()),
        Ok(_) => Err("YouTube card records exceed their limit.".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(_) => Err("YouTube card records are unavailable.".into()),
    }
}
fn save_publications(root: &Path, cards: &[Publication]) -> Result<(), String> {
    let path = root.join("youtube/cards.json");
    std::fs::create_dir_all(path.parent().unwrap())
        .map_err(|_| "Could not create YouTube card storage.")?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec(cards).unwrap())
        .map_err(|_| "Could not save YouTube cards.")?;
    std::fs::rename(temp, path).map_err(|_| "Could not finish saving YouTube cards.".into())
}
fn publish_video(root: &Path, video: &Video, slot: &Slot) -> Result<Value, String> {
    let result = crate::glance::publish_for(APP, &card_args(video, slot))?;
    let _lock = CARDS.lock().unwrap();
    let mut cards = publications(root)?;
    cards.retain(|p| p.slot.id != slot.id && p.expires_at > crate::glance::now_ms());
    cards.push(Publication {
        video: video.clone(),
        slot: slot.clone(),
        expires_at: result["expires_at"]
            .as_u64()
            .ok_or("Missing publication expiry")?,
        dismissed: false,
    });
    save_publications(root, &cards)?;
    Ok(result)
}
/// Restore the same searched videos quietly, without extending expiry. Call
/// once after Glance and app grants are initialized. Dismissals survive restart.
pub fn restore(root: &Path) -> Result<usize, String> {
    let cards = {
        let _lock = CARDS.lock().unwrap();
        publications(root)?
    };
    let now = crate::glance::now_ms();
    let mut restored = 0;
    for card in cards {
        let Some(args) = restored_args(&card, now) else {
            continue;
        };
        if crate::glance::card(&format!("{APP}/music-{}", card.slot.id)).is_some() {
            continue;
        }
        crate::glance::publish_for(APP, &args)?;
        restored += 1;
    }
    Ok(restored)
}
fn restored_args(card: &Publication, now: u64) -> Option<Value> {
    if card.dismissed || card.expires_at.saturating_sub(now) < 60_000 {
        return None;
    }
    let mut args = card_args(&card.video, &card.slot);
    args["expires"] = json!((card.expires_at - now) / 1000);
    Some(args)
}
/// The shell calls this for both dismissal and Undo. The daily success ledger
/// remains intact, so the next timer cannot recreate a dismissed slot that day.
pub fn set_dismissed(root: &Path, id: &str, dismissed: bool) -> Result<(), String> {
    let _lock = CARDS.lock().unwrap();
    let mut cards = publications(root)?;
    if let Some(card) = cards
        .iter_mut()
        .find(|p| format!("music-{}", p.slot.id) == id)
    {
        card.dismissed = dismissed;
        save_publications(root, &cards)?;
    }
    Ok(())
}

fn slot<'a>(state: &'a State, id: &str) -> Result<&'a Slot, String> {
    state
        .slots
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| "Choose morning, noon, afternoon, dinner or sleep.".into())
}
fn slot_at(slots: &[Slot], hour: u8) -> Option<&Slot> {
    slots
        .iter()
        .filter(|s| s.hour <= hour)
        .max_by_key(|s| s.hour)
}
fn prefs(state: &State) -> Value {
    json!({"enabled":state.enabled,"agent_allowed":crate::agents::access(APP)==crate::agents::Access::Allowed,"timezone":"device local","slots":state.slots,"background":"while OctoSense host is running; Android may suspend it","autoplay":false})
}
fn update_preferences(state: &mut State, args: &Value) -> Result<(), String> {
    if let Some(enabled) = args.get("enabled") {
        state.enabled = enabled.as_bool().ok_or("enabled must be true or false")?;
    }
    if let Some(updates) = args.get("slots") {
        let updates = updates.as_array().ok_or("slots must be a list")?;
        if updates.len() > 5 {
            return Err("At most five slot updates are allowed.".into());
        }
        for update in updates {
            let item = state
                .slots
                .iter_mut()
                .find(|s| s.id == text(update, "id"))
                .ok_or("Unknown music slot")?;
            if let Some(value) = update.get("query") {
                item.query = query(value.as_str().ok_or("query must be text")?)?;
            }
            if let Some(value) = update.get("hour") {
                item.hour = value
                    .as_u64()
                    .filter(|h| *h < 24)
                    .ok_or("hour must be 0–23")? as u8;
            }
        }
        let mut hours: Vec<_> = state.slots.iter().map(|s| s.hour).collect();
        hours.sort();
        hours.dedup();
        if hours.len() != state.slots.len() {
            return Err("Each music slot needs a distinct hour.".into());
        }
    }
    Ok(())
}
fn card_args(video: &Video, slot: &Slot) -> Value {
    let title = slot.label.clone();
    let summary: String = format!("{} · {}", video.title, video.channel)
        .chars()
        .take(200)
        .collect();
    json!({"card_id":format!("music-{}",slot.id),"title":title,"summary":summary,"source":CARD,
        "data":{"video":{"title":video.title,"subtitle":video.channel,"summary":format!("Found on YouTube for “{}”. Playback starts only after you choose Play; YouTube may ask for an additional tap.",video.query),
            "as_of":slot.label,"pick1_source":format!("https://i.ytimg.com/vi/{}/hqdefault.jpg",video.id),"metric1_label":"Duration","metric1_value":video.length,"url1":format!("app://youtube/video/{}",video.id)}},
        "open":{"app":"youtube","route":format!("video/{}",video.id)},"notify":false,"priority":25,"expires":86400})
}
fn publish(root: &Path, args: &Value) -> Result<Value, String> {
    let video = known(root, text(args, "id"))?;
    if crate::glance::now_ms().saturating_sub(video.retrieved_at) > 24 * 3600 * 1000 {
        return Err(
            "These search results are over a day old. Search again before recommending this video."
                .into(),
        );
    }
    let _lock = STORE.lock().unwrap();
    let mut state = load(root)?;
    let selected = slot(&state, text(args, "slot"))?.clone();
    let result = publish_video(root, &video, &selected)?;
    // An explicit selection wins over an automatic job, including one already
    // fetching. The job rechecks this ledger under STORE before publishing.
    let day = crate::shell::bar::local_time(c"%Y-%m-%d");
    record_choice(&mut state, format!("{day}/{}", selected.id), video.id);
    save(root, &state)?;
    Ok(result)
}
/// Trusted shell route only; the agent has no play tool and cannot start audio.
pub fn focus_video(root: &Path, id: &str) -> Result<(), String> {
    known(root, id)?;
    FOCUS.lock().unwrap().insert(root.to_path_buf(), id.into());
    Ok(())
}
fn handle(root: &Path, method: &str, args: &Value) -> Result<Value, String> {
    match method {
        "search" | "recommend" => {
            let state = load(root)?;
            if method == "recommend" {
                slot(&state, text(args, "slot"))?;
            }
            let chosen_query = if method == "recommend" && text(args, "query").is_empty() {
                slot(&state, text(args, "slot"))?.query.clone()
            } else {
                query(text(args, "query"))?
            };
            let mut videos = fetch(&chosen_query)?;
            videos.truncate(args["limit"].as_u64().unwrap_or(5).clamp(1, 10) as usize);
            cache(root, &videos)?;
            Ok(
                json!({"query":chosen_query,"videos":videos.iter().map(Video::row).collect::<Vec<_>>(),"source":"YouTube public search","autoplay":false}),
            )
        }
        "read" => Ok(json!({"video":known(root,text(args,"id"))?.row()})),
        "publish" => publish(root, args),
        "preferences" => {
            let _lock = STORE.lock().unwrap();
            let mut state = load(root)?;
            if args.get("enabled").is_some() || args.get("slots").is_some() {
                update_preferences(&mut state, args)?;
                save(root, &state)?;
                if let Some(jobs) = JOBS.get() {
                    if let Some(job) = jobs.lock().unwrap().get_mut(root) {
                        job.retry_at = 0;
                    }
                }
            }
            Ok(prefs(&state))
        }
        "view" => {
            let focus = FOCUS.lock().unwrap().remove(root);
            let video = focus.and_then(|id| known(root, &id).ok()).map(|v| v.row());
            Ok(json!({"focus":video,"preferences":prefs(&load(root)?)}))
        }
        "notify" => crate::glance_notice::notify(APP, args),
        _ => Err(format!("YouTube has no method {method:?}")),
    }
}
struct YoutubeService;
impl octosense_appstore::services::HostService for YoutubeService {
    fn family(&self) -> &'static str {
        "youtube"
    }
    fn call(
        &mut self,
        call: octosense_appstore::services::ServiceCall,
        reply: octosense_appstore::services::Replier,
        _host: &mut dyn octosense_appstore::services::ServiceHost,
    ) {
        // Cross-app tools reach here through the relay as the data owner after
        // the caller's explicit grant and shareable declaration were checked.
        if call.app_id != APP {
            return reply.send(Err(
                "YouTube serves its owning app only; other agents need a granted tool.".into(),
            ));
        }
        // Only network work needs a worker. Polling a focused app does not
        // create a new OS thread every second.
        if matches!(call.method(), "search" | "recommend") {
            std::thread::spawn(move || {
                reply.send(handle(&call.host_dir, call.method(), &call.args))
            });
        } else {
            reply.send(handle(&call.host_dir, call.method(), &call.args));
        }
    }
}
pub fn register() {
    octosense_appstore::services::register_host_service(Box::new(YoutubeService));
}

/// Bounded opportunistic timer. One slot per local day; no catch-up burst, no
/// notification, no audio. A successful publication is persisted before a later
/// tick can retry. A failed fetch backs off ten minutes. Consent is rechecked
/// after the network operation. Device timezone changes use the new local day.
pub fn tick(root: &Path) {
    if crate::agents::access(APP) != crate::agents::Access::Allowed {
        return;
    }
    let now = crate::glance::now_ms();
    let root = root.to_path_buf();
    let jobs = JOBS.get_or_init(|| Mutex::new(BTreeMap::new()));
    {
        let mut jobs = jobs.lock().unwrap();
        let job = jobs.entry(root.clone()).or_insert(Job {
            running: false,
            retry_at: 0,
        });
        if job.running || now < job.retry_at {
            return;
        }
        job.running = true;
    }
    std::thread::spawn(move || {
        let _ = scheduled(&root);
        if let Some(job) = JOBS.get().unwrap().lock().unwrap().get_mut(&root) {
            job.running = false;
            job.retry_at = crate::glance::now_ms() + 10 * 60 * 1000;
        }
    });
}
fn scheduled(root: &Path) -> Result<(), String> {
    let state = load(root)?;
    if !state.enabled {
        return Ok(());
    }
    let hour = crate::shell::bar::local_time(c"%H")
        .parse::<u8>()
        .map_err(|_| "Device local time unavailable")?;
    let day = crate::shell::bar::local_time(c"%Y-%m-%d");
    let selected = match slot_at(&state.slots, hour) {
        Some(slot) => slot.clone(),
        None => return Ok(()),
    };
    let key = format!("{day}/{}", selected.id);
    if state.published.contains_key(&key) {
        return Ok(());
    }
    let video = fetch(&selected.query)?
        .into_iter()
        .next()
        .ok_or("No YouTube result")?;
    cache(root, std::slice::from_ref(&video))?;
    let _lock = STORE.lock().unwrap();
    let mut state = load(root)?;
    if !state.enabled
        || slot(&state, &selected.id)? != &selected
        || crate::agents::access(APP) != crate::agents::Access::Allowed
    {
        return Ok(());
    }
    // A search can cross a slot boundary or a device timezone change. Do
    // not publish the stale occasion that was selected before the network.
    let current_day = crate::shell::bar::local_time(c"%Y-%m-%d");
    let current_hour = crate::shell::bar::local_time(c"%H")
        .parse::<u8>()
        .map_err(|_| "Device local time unavailable")?;
    if !same_occasion(&state.slots, &selected.id, &day, &current_day, current_hour)
        || state.published.contains_key(&key)
    {
        return Ok(());
    }
    publish_video(root, &video, &selected)?;
    record_choice(&mut state, key, video.id);
    save(root, &state)
}
fn record_choice(state: &mut State, key: String, id: String) {
    state.published.insert(key, id);
    while state.published.len() > 70 {
        if let Some(key) = state.published.keys().next().cloned() {
            state.published.remove(&key);
        }
    }
}

fn same_occasion(
    slots: &[Slot],
    selected: &str,
    day: &str,
    current_day: &str,
    current_hour: u8,
) -> bool {
    day == current_day && slot_at(slots, current_hour).is_some_and(|s| s.id == selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp() -> PathBuf {
        std::env::temp_dir().join(format!("youtube-test-{}", uuid::Uuid::new_v4()))
    }
    fn video() -> Video {
        Video {
            id: "abcdefghijk".into(),
            title: "Relaxing café \"music\"".into(),
            channel: "Fixture artist".into(),
            length: "30:00".into(),
            query: "piano".into(),
            retrieved_at: 1,
        }
    }
    #[test]
    fn search_parser_preserves_unicode_quotes_and_field_boundaries() {
        let body = br#"<html>{"videoRenderer" : {"videoId":"abcdefghijk","title":{"runs":[{"text":"Caf\u00e9 \"piano\""}]},"lengthText":{"simpleText":"20:10"}}},{"videoRenderer":{"videoId":"123456789ab","title":{"simpleText":"Next"},"longBylineText":{"runs":[{"text":"Artist"}]}}}</html>"#;
        let hits = parse_results(body, "piano", 1);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "Café \"piano\"");
        assert_eq!(hits[0].channel, "");
        assert_eq!(hits[1].channel, "Artist");
        assert_eq!(hits[0].length, "20:10");
    }
    #[test]
    fn duration_badge_distinguishes_live_from_missing_metadata_without_clipping() {
        let body=json!({"videoRenderer":{"videoId":"abcdefghijk","title":{"simpleText":"Live fixture"},"thumbnailOverlays":[{"thumbnailOverlayTimeStatusRenderer":{"style":"LIVE"}}]}}).to_string();
        assert_eq!(parse_results(body.as_bytes(), "q", 1)[0].length, "LIVE");
        let body=json!({"videoRenderer":{"videoId":"abcdefghijk","title":{"simpleText":"Unknown fixture"}}}).to_string();
        assert_eq!(parse_results(body.as_bytes(), "q", 1)[0].length, "—");
    }
    #[test]
    fn search_rejects_invented_urls_invalid_ids_duplicates_and_empty_titles() {
        let one = json!({"videoRenderer":{"videoId":"abcdefghijk","title":{"simpleText":"One"}}})
            .to_string();
        let body = format!(
            "{one}{one}{}{}",
            json!({"videoRenderer":{"videoId":"https://bad","title":{"simpleText":"Bad"}}}),
            json!({"videoRenderer":{"videoId":"123456789ab"}})
        );
        assert_eq!(parse_results(body.as_bytes(), "q", 1).len(), 1);
        assert!(parse_results(b"consent required", "q", 1).is_empty());
        assert!(query("\n").is_err());
        assert!(query(&"x".repeat(161)).is_err());
        assert_eq!(
            search_url("jazz & café"),
            "https://www.youtube.com/results?search_query=jazz+%26+caf%C3%A9"
        );
    }
    #[test]
    fn schedule_chooses_only_current_slot_without_nighttime_catchup() {
        let slots = default_slots();
        assert!(slot_at(&slots, 6).is_none());
        for (hour, id) in [
            (7, "morning"),
            (12, "noon"),
            (16, "afternoon"),
            (20, "dinner"),
            (23, "sleep"),
        ] {
            assert_eq!(slot_at(&slots, hour).unwrap().id, id);
        }
    }
    #[test]
    fn in_flight_search_cannot_publish_yesterdays_or_previous_slots_recommendation() {
        let slots = default_slots();
        assert!(same_occasion(
            &slots,
            "morning",
            "2026-10-06",
            "2026-10-06",
            11
        ));
        assert!(!same_occasion(
            &slots,
            "morning",
            "2026-10-06",
            "2026-10-06",
            12
        ));
        assert!(!same_occasion(
            &slots,
            "sleep",
            "2026-10-06",
            "2026-10-07",
            0
        ));
        assert!(!same_occasion(
            &slots,
            "sleep",
            "2026-10-06",
            "2026-10-06",
            14
        ));
    }
    #[test]
    fn preferences_are_opt_in_and_invalid_updates_are_not_persisted() {
        let root = temp();
        assert_eq!(
            handle(&root, "preferences", &json!({})).unwrap()["enabled"],
            false
        );
        handle(
            &root,
            "preferences",
            &json!({"enabled":true,"slots":[{"id":"sleep","hour":22,"query":"quiet piano"}]}),
        )
        .unwrap();
        assert_eq!(load(&root).unwrap().slots[4].hour, 22);
        assert!(handle(
            &root,
            "preferences",
            &json!({"enabled":false,"slots":[{"id":"sleep","hour":12}]})
        )
        .is_err());
        assert!(load(&root).unwrap().enabled);
        assert!(handle(
            &root,
            "preferences",
            &json!({"slots":[{"id":"sleep","hour":24}]})
        )
        .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn only_cached_videos_can_be_read_focused_or_published() {
        let root = temp();
        assert!(known(&root, "abcdefghijk").is_err());
        assert!(focus_video(&root, "abcdefghijk").is_err());
        cache(&root, &[video()]).unwrap();
        focus_video(&root, "abcdefghijk").unwrap();
        let row = handle(&root, "view", &json!({})).unwrap();
        assert_eq!(row["focus"]["id"], "abcdefghijk");
        assert_eq!(row["focus"]["embed"], "https://m.youtube.com/watch?v=abcdefghijk",
            "WebReader must open the watch page, not an unreferenced iframe embed (Error 153)");
        assert_eq!(
            handle(&root, "view", &json!({})).unwrap()["focus"],
            Value::Null
        );
        assert!(handle(
            &root,
            "publish",
            &json!({"id":"abcdefghijk","slot":"sleep"})
        )
        .unwrap_err()
        .contains("over a day old"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn card_uses_own_route_and_in_content_play_without_autoplay_or_notification() {
        let args = card_args(&video(), &default_slots()[4]);
        assert_eq!(args["open"]["route"], "video/abcdefghijk");
        assert_eq!(
            args["data"]["video"]["url1"],
            "app://youtube/video/abcdefghijk"
        );
        assert_eq!(args["notify"], false);
        assert!(!CARD.contains("autoplay"));
        assert!(CARD.contains("on_tap: play"));
        crate::glance::check_level(CARD).unwrap();
    }
    #[test]
    fn durable_cards_keep_exact_video_after_cache_eviction_and_respect_dismissal() {
        let root = temp();
        let card = Publication {
            video: video(),
            slot: default_slots()[0].clone(),
            expires_at: 180_000,
            dismissed: false,
        };
        save_publications(&root, &[card]).unwrap();
        assert_eq!(known(&root, "abcdefghijk").unwrap().title, video().title);
        assert_eq!(
            restored_args(&publications(&root).unwrap()[0], 60_000).unwrap()["expires"],
            120
        );
        set_dismissed(&root, "music-morning", true).unwrap();
        assert!(restored_args(&publications(&root).unwrap()[0], 60_000).is_none());
        set_dismissed(&root, "music-morning", false).unwrap();
        assert!(restored_args(&publications(&root).unwrap()[0], 170_000).is_none());
        assert_eq!(
            restored_args(&publications(&root).unwrap()[0], 60_000).unwrap()["notify"],
            false
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "Live public YouTube request; run explicitly when network validation is authorized"]
    fn live_public_search_returns_actual_video_results() {
        let videos = fetch("relaxing afternoon instrumental music").unwrap();
        assert!(!videos.is_empty());
        assert!(videos
            .iter()
            .all(|v| valid_id(&v.id) && !v.title.is_empty() && v.retrieved_at > 0));
    }
    #[test]
    fn cache_is_bounded_and_repeated_results_update_in_place() {
        let root = temp();
        let mut first = video();
        cache(&root, &[first.clone()]).unwrap();
        first.title = "Updated".into();
        cache(&root, &[first]).unwrap();
        assert_eq!(load(&root).unwrap().videos.len(), 1);
        assert_eq!(known(&root, "abcdefghijk").unwrap().title, "Updated");
        std::fs::remove_dir_all(root).unwrap();
    }
}
