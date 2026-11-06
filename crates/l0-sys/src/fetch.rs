//! Live data for the `sys` methods: one HTTP fetch per URL, answered from a cache once it lands.
//!
//! A `sys.*` method runs inside a card's evaluation and cannot wait: it asks here, gets the bytes
//! when they are cached, or fires the request once (deduped by URL) and answers a placeholder.
//! When the response arrives ([`handle_network_responses`], called by the host app from its
//! event handler) the cache fills and the Splash live-data epoch bumps, so every body that binds
//! live data re-evaluates once and reads the value. Failures retry lazily with exponential
//! back-off, up to [`MAX_RETRIES`]; a permanent status (4xx) or a spent budget is terminal and
//! shows as "n/a" instead of the loading glyph.
//!
//! Ported from the fork makepad's `platform/src/script/res.rs` and `script/std.rs`.

use makepad_widgets::makepad_platform::script::vm::ScriptVmCx;
use makepad_widgets::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// State of one URL's fetch.
#[derive(Clone)]
pub enum DataFetch {
    /// Request in flight, under this request id.
    Loading(LiveId),
    Loaded(Rc<Vec<u8>>),
    Error,
}

/// Retries per URL before the Error state sticks (the first attempt not counted).
pub const MAX_RETRIES: u8 = 4;

thread_local! {
    static FETCHES: RefCell<HashMap<String, DataFetch>> = RefCell::new(HashMap::new());
    /// (attempts used, earliest next retry)
    static RETRIES: RefCell<HashMap<String, (u8, std::time::Instant)>> = RefCell::new(HashMap::new());
}

/// User-Agent by host: Yahoo answers 429 without a browser-ish one; Overpass and Nominatim
/// refuse the bare browser signature and want an identifying agent.
fn user_agent(url: &str) -> &'static str {
    if url.contains("overpass") || url.contains("nominatim") {
        "octoscript-appcard/1.0 (+https://github.com/OctoSense-org/Octoscript-AppCard; live card data binding)"
    } else {
        "Mozilla/5.0"
    }
}

/// The cached state of `url`, if any fetch was ever issued for it.
pub fn get(url: &str) -> Option<DataFetch> {
    FETCHES.with(|f| f.borrow().get(url).cloned())
}

/// Drop `url` from the cache: the next ask fetches again.
pub fn forget(url: &str) {
    FETCHES.with(|f| f.borrow_mut().remove(url));
    RETRIES.with(|r| r.borrow_mut().remove(url));
}

/// The live-data epoch (`widget_async::live_data_epoch`): bumped when a fetch lands or fails for good.
pub fn live_data_epoch() -> u64 {
    makepad_widgets::widget_async::live_data_epoch()
}

/// A `sys` method's fetch, on behalf of the isolate running `vm`.
pub fn fetch_vm(vm: &mut ScriptVm, url: &str) -> Option<Rc<Vec<u8>>> {
    let heap_key = vm.heap_key();
    fetch_cx(vm.host.cx_mut(), heap_key, url)
}

/// Get-or-fetch `url` for the isolate `heap_key`: the bytes once loaded, else the request is
/// issued once (deduped by URL) and `None` says "still loading".
pub fn fetch_cx(cx: &mut Cx, heap_key: usize, url: &str) -> Option<Rc<Vec<u8>>> {
    if !makepad_widgets::splash_policy::url_allowed(heap_key, url) {
        log!("sys: data fetch refused by the host's allowlist: {url}");
        return None;
    }
    match get(url) {
        Some(DataFetch::Loaded(bytes)) => return Some(bytes),
        Some(DataFetch::Loading(_)) => return None,
        Some(DataFetch::Error) => {
            if !take_retry(url) {
                return None;
            }
            log!("sys: data fetch retrying: {url}");
        }
        None => {}
    }
    // Not issued here: a `sys` method runs inside an isolate's evaluation, where the platform
    // refuses raw HTTP (host-IO-only isolates). The request waits for the host's [`pump`].
    let request_id = LiveId::unique();
    FETCHES.with(|f| f.borrow_mut().insert(url.to_string(), DataFetch::Loading(request_id)));
    PENDING.with(|p| p.borrow_mut().push((request_id, url.to_string())));
    // An event cycle follows a redraw; the host pumps in it.
    cx.redraw_all();
    None
}

thread_local! {
    /// Fetches a `sys` method asked for and the host has not issued yet: (request id, URL).
    static PENDING: RefCell<Vec<(LiveId, String)>> = RefCell::new(Vec::new());
}

/// Issue the fetches the cards asked for. The HOST calls this from its own event handler,
/// outside any isolate (the shell's app, or AppCard standalone): that is the one place the
/// platform lets raw HTTP out, and the place a host applies its network policy. Cheap when
/// nothing waits.
pub fn pump(cx: &mut Cx) {
    let pending = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
    for (request_id, url) in pending {
        log!("sys: data fetch issuing {url}");
        let mut req = HttpRequest::new(url.clone(), HttpMethod::GET);
        req.set_header("User-Agent".to_string(), user_agent(&url).to_string());
        cx.http_request(request_id, req);
    }
}

/// What a method answers while `url` is unresolved: the loading glyph while it may still
/// arrive, "n/a" once the retry budget is spent.
pub fn placeholder(url: &str) -> String {
    if failed_terminally(url) { "n/a".to_string() } else { "\u{2014}".to_string() }
}

/// True once `url` is in Error with no retry budget left.
pub fn failed_terminally(url: &str) -> bool {
    if !matches!(get(url), Some(DataFetch::Error)) {
        return false;
    }
    RETRIES.with(|r| r.borrow().get(url).is_some_and(|(used, _)| *used >= MAX_RETRIES))
}

/// Lazy-retry bookkeeping: budget left and back-off elapsed, consuming one unit when so.
fn take_retry(url: &str) -> bool {
    let now = std::time::Instant::now();
    RETRIES.with(|r| {
        let mut r = r.borrow_mut();
        let entry = r.entry(url.to_string()).or_insert((0, now));
        if entry.0 >= MAX_RETRIES || now < entry.1 {
            return false;
        }
        entry.0 += 1;
        entry.1 = now + std::time::Duration::from_secs(1u64 << (entry.0.min(6) - 1));
        true
    })
}

fn url_of(request_id: LiveId) -> Option<String> {
    FETCHES.with(|f| f.borrow().iter().find_map(|(url, fetch)| matches!(fetch, DataFetch::Loading(id) if *id == request_id).then(|| url.clone())))
}

fn loaded(request_id: LiveId, data: Vec<u8>) -> bool {
    let Some(url) = url_of(request_id) else { return false };
    FETCHES.with(|f| f.borrow_mut().insert(url.clone(), DataFetch::Loaded(Rc::new(data))));
    RETRIES.with(|r| r.borrow_mut().remove(&url));
    makepad_widgets::widget_async::bump_live_data_epoch();
    true
}

/// A transport error or transient status: the next evaluation re-issues the request while
/// budget remains; once spent, the fetch is terminal and the epoch bumps so bound cards settle
/// on "n/a".
fn failed(request_id: LiveId) {
    let Some(url) = url_of(request_id) else { return };
    FETCHES.with(|f| f.borrow_mut().insert(url, DataFetch::Error));
    makepad_widgets::widget_async::bump_live_data_epoch();
}

/// A permanent failure (404, 403...): the same request can only fail the same way.
fn failed_terminally_by_id(request_id: LiveId) {
    let Some(url) = url_of(request_id) else { return };
    FETCHES.with(|f| f.borrow_mut().insert(url.clone(), DataFetch::Error));
    RETRIES.with(|r| r.borrow_mut().insert(url, (MAX_RETRIES, std::time::Instant::now())));
    makepad_widgets::widget_async::bump_live_data_epoch();
}

/// Route the responses of this module's fetches: the host app calls it with every
/// `Event::NetworkResponses`. Responses to other requests are left alone.
pub fn handle_network_responses(cx: &mut Cx, responses: &[NetworkResponse]) {
    pump(cx);
    let mut touched = false;
    for response in responses {
        // The request id rides on the variant (`metadata_id` is the request's own tag).
        let request_id = match response {
            NetworkResponse::HttpResponse { request_id, .. } | NetworkResponse::HttpError { request_id, .. } => *request_id,
            _ => continue,
        };
        if url_of(request_id).is_none() {
            continue;
        }
        touched = true;
        match response {
            NetworkResponse::HttpResponse { response: res, .. } => {
                // Classify by status first; a 2xx counts only with a non-empty body, or an
                // empty answer would be cached as loaded forever.
                let status = res.status_code;
                let body = res.get_body().filter(|b| !b.is_empty());
                if (200..300).contains(&status) {
                    match body {
                        Some(body) => {
                            log!("sys: data fetch loaded {} bytes (status {status}) {}", body.len(), url_of(request_id).unwrap_or_default());
                            loaded(request_id, body.to_vec());
                        }
                        None => {
                            log!("sys: data fetch {status} with an empty body; retrying {}", url_of(request_id).unwrap_or_default());
                            failed(request_id);
                        }
                    }
                } else {
                    log!("sys: data fetch failed: status={status} url={}", url_of(request_id).unwrap_or_default());
                    let transient = matches!(status, 408 | 429) || status >= 500;
                    if transient { failed(request_id) } else { failed_terminally_by_id(request_id) }
                }
            }
            NetworkResponse::HttpError { error, .. } => {
                log!("sys: data fetch request error: {}", error.message);
                failed(request_id);
            }
            _ => {}
        }
    }
    if touched {
        cx.redraw_all();
    }
}

/// Put `bytes` in the cache as the loaded answer for `url` (a test's seed, or a host that
/// already has the document): the next ask returns them without a request.
pub fn seed_loaded(url: &str, bytes: &[u8]) {
    FETCHES.with(|f| f.borrow_mut().insert(url.to_string(), DataFetch::Loaded(Rc::new(bytes.to_vec()))));
    RETRIES.with(|r| r.borrow_mut().remove(url));
}

/// How many URLs the cache knows (loaded, loading or failed).
pub fn cached_count() -> usize {
    FETCHES.with(|f| f.borrow().len())
}
