//! The phone's WebView as the octos reader's page renderer (feature
//! `toolbox-peers`).
//!
//! Where there is no Chrome to launch (Android), pages the reader cannot
//! read over plain HTTP, including pages plain HTTP was blocked on, are
//! loaded in a hidden system browser: a real Android WebView on the phone's
//! own network, which sites let through far more often than a headless
//! desktop browser (measured on a OnePlus 6T, 2026-09-29: 0 bot challenges
//! against 7 for desktop headless Chrome on the same 10 sites).
//!
//! How it runs: [`renderer`] is the reader's `Renderer`. Called on the
//! toolbox's async side, it posts a [`WebRenderRequest`] action and waits.
//! [`WebViewRenderHost`], driven by the shell's event loop, serves requests
//! in a small pool of hidden, navigable system browsers (one page each at a
//! time): it loads the URL, polls the document twice a second (the Android
//! WebView reports no "page finished"), and once the page is complete and
//! has settled returns its
//! HTML, final URL and title through the `octos_native` bridge. A check that
//! clears itself in a real browser ("Just a moment…", "正在进行安全检测…") is
//! waited out; nothing is clicked or solved. Each load has a deadline.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use makepad_widgets::makepad_platform::event::{
    NativeSystemBrowserInvoke, NativeSystemBrowserPageError,
};
use makepad_widgets::*;
use octos_research::reader::{RenderFuture, Rendered, Renderer};

/// The hidden browsers' ids: renders run side by side, one per browser.
/// Two keep a toolbox run's parallel reads from queueing behind each other
/// without loading the phone with WebViews.
fn browser_ids() -> [LiveId; 2] {
    [live_id!(octos_page_renderer), live_id!(octos_page_renderer_2)]
}

/// The bridge tool the extraction script calls.
const TOOL: &str = "octos.render";
/// Seconds between document polls.
const POLL: f64 = 0.5;
/// Seconds a complete document must stay complete before it is read.
const SETTLE: f64 = 1.0;
/// Polls a self-clearing check may take before it counts as the page
/// (about 12 s).
const MAX_CHECK_POLLS: u32 = 24;
/// Seconds one render may take. Under a toolbox run's per-read share
/// (`octosense_toolbox::runner::MAX_READ_MS`), so a render that never
/// completes is closed here rather than abandoned by the run.
const DEADLINE: f64 = 20.0;
/// Page-error code the platform reports when the WebView's renderer process
/// is gone (Makepad's Android `onRenderProcessGone`).
const RENDER_PROCESS_GONE: i32 = -1000;
/// Largest HTML taken from the document (the reader's parse limit).
const MAX_HTML_CHARS: usize = 3 * 1024 * 1024;

/// A render request, posted from the toolbox's async side.
#[derive(Clone, Debug)]
pub struct WebRenderRequest {
    pub job: u64,
    pub url: String,
}

type Reply = tokio::sync::oneshot::Sender<Result<Rendered, String>>;

fn replies() -> &'static Mutex<HashMap<u64, Reply>> {
    static R: OnceLock<Mutex<HashMap<u64, Reply>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The reader gave up on this job (its read hit the run's time share):
/// nobody waits for the page, so it is not rendered or no longer.
fn abandoned(job: u64) -> bool {
    let mut replies = replies().lock().unwrap_or_else(|p| p.into_inner());
    let gone = replies.get(&job).is_none_or(|tx| tx.is_closed());
    if gone {
        replies.remove(&job);
    }
    gone
}

fn reply(job: u64, result: Result<Rendered, String>) {
    let tx = replies()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&job);
    if let Some(tx) = tx {
        let _ = tx.send(result);
    }
}

/// The reader's renderer, backed by the shell's hidden WebView. The shell
/// must run a [`WebViewRenderHost`]; without one a render waits for the
/// reader's own timeout.
pub fn renderer() -> Renderer {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    Arc::new(|url: String| {
        let job = NEXT.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = tokio::sync::oneshot::channel();
        replies()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(job, tx);
        Cx::post_action(WebRenderRequest { job, url });
        Box::pin(async move {
            rx.await
                .unwrap_or_else(|_| Err("the WebView renderer stopped".to_string()))
        }) as RenderFuture
    })
}

/// The script run in the page on every poll: the document's state, and on
/// request its HTML. `JOB` and `WANT_HTML` are replaced before running.
const POLL_JS: &str = r#"(function(){var r;try{var d=document;r={job:JOB,url:location.href,ready:d.readyState,title:d.title,text:(d.body&&d.body.innerText||'').slice(0,4000)};if(WANT_HTML){r.html=d.documentElement.outerHTML.slice(0,MAX);}}catch(e){r={job:JOB,error:String(e)};}window.octos_native.invoke(JOB,'octos.render',JSON.stringify(r));})()"#;

fn poll_js(job: u64, want_html: bool) -> String {
    POLL_JS
        .replace("JOB", &job.to_string())
        .replace("WANT_HTML", if want_html { "true" } else { "false" })
        .replace("MAX", &MAX_HTML_CHARS.to_string())
}

struct Active {
    job: u64,
    url: String,
    /// Timer for the next poll.
    poll: Timer,
    deadline: Timer,
    /// When the document was first seen complete (seconds since start).
    complete_since: Option<f64>,
    started: f64,
    /// Seconds it waited in the queue.
    waited: f64,
    check_polls: u32,
    reading: bool,
    navigations: Vec<String>,
}

/// One hidden browser and the page it is rendering.
struct Slot {
    id: LiveId,
    active: Option<Active>,
    spawned: bool,
    /// The page the last render read: until the next load commits, polls
    /// still see it.
    last_page: Option<String>,
}

/// Serves [`WebRenderRequest`]s with the hidden WebViews, one page per
/// browser at a time.
pub struct WebViewRenderHost {
    /// Waiting requests and when each arrived (seconds since app start).
    queue: VecDeque<(WebRenderRequest, f64)>,
    slots: Vec<Slot>,
}

impl Default for WebViewRenderHost {
    fn default() -> Self {
        let slots = browser_ids()
            .into_iter()
            .map(|id| Slot { id, active: None, spawned: false, last_page: None })
            .collect();
        Self { queue: VecDeque::new(), slots }
    }
}

/// A Google News article link: the page only forwards to the publisher by
/// script, so it is not the page until it has left news.google.com.
fn forwarding(url: &str) -> bool {
    url.starts_with("https://news.google.com/rss/articles/")
        || url.starts_with("https://news.google.com/articles/")
}

impl WebViewRenderHost {
    pub fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        match event {
            Event::Actions(actions) => {
                for action in actions {
                    if let Some(req) = action.downcast_ref::<WebRenderRequest>() {
                        self.queue.push_back((req.clone(), cx.seconds_since_app_start()));
                    } else if let Some(inv) = action.downcast_ref::<NativeSystemBrowserInvoke>() {
                        if inv.tool == TOOL {
                            if let Some(i) = self.slot_of(inv.browser_id) {
                                self.on_poll(cx, i, inv.call_id as u64, &inv.args);
                            }
                        }
                    } else if let Some(err) = action.downcast_ref::<NativeSystemBrowserPageError>()
                    {
                        if let Some(i) = self.slot_of(err.browser_id) {
                            // The WebView's renderer process died: the
                            // platform dropped that WebView; spawn a new one.
                            if err.code == RENDER_PROCESS_GONE {
                                self.slots[i].spawned = false;
                                self.slots[i].last_page = None;
                            }
                            if self.slots[i].active.is_some() {
                                let msg = format!(
                                    "render_failed: the page did not load ({} {})",
                                    err.code, err.description
                                );
                                self.finish(cx, i, Err(msg));
                            }
                        }
                    }
                }
            }
            Event::Timer(_) => {
                for i in 0..self.slots.len() {
                    let Some(a) = self.slots[i].active.as_ref() else {
                        continue;
                    };
                    if a.deadline.is_event(event).is_some() {
                        self.finish(cx, i, Err(format!("render_timeout: no page after {DEADLINE}s")));
                    } else if a.poll.is_event(event).is_some() {
                        if abandoned(a.job) {
                            self.finish(cx, i, Err("abandoned: the read gave up".into()));
                            continue;
                        }
                        let (job, want, id) = (a.job, a.reading, self.slots[i].id);
                        cx.system_browser(id).eval_js(&poll_js(job, want));
                        if let Some(a) = self.slots[i].active.as_mut() {
                            a.poll = cx.start_timeout(POLL);
                        }
                    }
                }
            }
            _ => {}
        }
        self.start_waiting(cx);
    }

    fn slot_of(&self, browser_id: u64) -> Option<usize> {
        self.slots.iter().position(|s| s.id.get_value() == browser_id)
    }

    /// Give each idle browser the next request someone still waits for.
    fn start_waiting(&mut self, cx: &mut Cx) {
        let now = cx.seconds_since_app_start();
        while let Some(i) = self.slots.iter().position(|s| s.active.is_none()) {
            let (req, queued) = loop {
                let Some((req, queued)) = self.queue.pop_front() else {
                    return;
                };
                if !abandoned(req.job) {
                    break (req, queued);
                }
                log!("webview render: {} skipped: its read gave up after {:.1}s in the queue", req.url, now - queued);
            };
            let slot = &mut self.slots[i];
            let mut browser = cx.system_browser(slot.id);
            if slot.spawned {
                browser.set_url(&req.url, false);
            } else {
                browser.spawn_navigable(&req.url);
                slot.spawned = true;
            }
            // Hidden: the person never sees the pages read for them.
            browser.update(Area::Empty, false);
            slot.active = Some(Active {
                job: req.job,
                url: req.url,
                poll: cx.start_timeout(POLL),
                deadline: cx.start_timeout(DEADLINE),
                complete_since: None,
                started: now,
                waited: now - queued,
                check_polls: 0,
                reading: false,
                navigations: Vec::new(),
            });
        }
    }

    fn on_poll(&mut self, cx: &mut Cx, i: usize, job: u64, args: &str) {
        let now = cx.seconds_since_app_start();
        let slot = &mut self.slots[i];
        let Some(a) = slot.active.as_mut() else {
            return;
        };
        if a.job != job {
            return;
        }
        let v: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
        let url = v["url"].as_str().unwrap_or("").to_string();
        // Still on the previous page: the load has not committed yet. A
        // Google News link has not reached the publisher yet.
        let committed = !url.is_empty()
            && url != "about:blank"
            && now - a.started > 0.3
            && (slot.last_page.as_deref() != Some(url.as_str()) || url == a.url)
            && !forwarding(&url);
        if !committed || v["ready"].as_str() != Some("complete") {
            a.complete_since = None;
            a.reading = false;
            return;
        }
        if a.navigations.last() != Some(&url) {
            a.navigations.push(url.clone());
        }
        if let Some(html) = v["html"].as_str() {
            let text = v["text"].as_str().unwrap_or("");
            if octos_research::access::interstitial_text(text) && a.check_polls < MAX_CHECK_POLLS {
                // Wait for the check to finish; read again later.
                a.check_polls += 1;
                a.reading = false;
                a.complete_since = None;
                return;
            }
            slot.last_page = Some(url.clone());
            let rendered = Rendered {
                final_url: url,
                html: html.to_string(),
                navigations: a.navigations.clone(),
                status: None,
            };
            self.finish(cx, i, Ok(rendered));
            return;
        }
        let since = *a.complete_since.get_or_insert(now);
        if now - since >= SETTLE {
            // Complete and settled: the next poll brings the HTML.
            a.reading = true;
        }
    }

    fn finish(&mut self, cx: &mut Cx, i: usize, result: Result<Rendered, String>) {
        let Some(a) = self.slots[i].active.take() else {
            return;
        };
        cx.stop_timer(a.poll);
        cx.stop_timer(a.deadline);
        let took = cx.seconds_since_app_start() - a.started;
        match &result {
            Ok(_) => log!("webview render: {} ok in {:.1}s after {:.1}s queued", a.url, took, a.waited),
            Err(e) => log!("webview render: {} failed in {:.1}s after {:.1}s queued: {}", a.url, took, a.waited, e),
        }
        reply(a.job, result);
        self.start_waiting(cx);
    }
}

/// On-device check (the shell's `webview-crawl:<url>,<url>…` test
/// action): a small breadth-first crawl, up to 3 pages per seed on the same
/// site, through octos's reader with this renderer, logging one JSON line
/// per page (`[webview-crawl] {…}`) and a summary. Account links are not
/// followed.
pub fn crawl_test(seeds: Vec<String>) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                log!("[webview-crawl] no runtime: {}", e);
                return;
            }
        };
        rt.block_on(async move {
            use octos_research::reader::{Reader, ReaderConfig};
            let reader = Reader::new(ReaderConfig {
                renderer: Some(renderer()),
                keep_html: true,
                ..ReaderConfig::default()
            });
            let (mut ok, mut failed) = (0usize, std::collections::BTreeMap::<String, usize>::new());
            for seed in seeds {
                let site = octos_research::urls::domain_of(&seed).unwrap_or_default();
                let mut queue: VecDeque<(String, u32)> = VecDeque::from([(seed.clone(), 0)]);
                let mut seen = std::collections::HashSet::from([seed.clone()]);
                let mut done = 0;
                while let Some((url, depth)) = queue.pop_front() {
                    if done >= 3 {
                        break;
                    }
                    done += 1;
                    let started = std::time::Instant::now();
                    match reader.read(&url).await {
                        Ok(page) => {
                            ok += 1;
                            log!(
                                "[webview-crawl] {}",
                                serde_json::json!({"url": url, "ok": true, "rendered": page.rendered,
                                    "text_chars": page.text.chars().count(), "title": page.meta.title,
                                    "ms": started.elapsed().as_millis() as u64})
                            );
                            if depth == 0 {
                                for link in links_of(&page.html, &page.final_url) {
                                    if octos_research::urls::domain_of(&link).as_deref() == Some(site.as_str())
                                        && !octos_research::urls::is_account_link(&link)
                                        && seen.insert(link.clone())
                                    {
                                        queue.push_back((link, 1));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            *failed.entry(e.reason.code().to_string()).or_default() += 1;
                            log!(
                                "[webview-crawl] {}",
                                serde_json::json!({"url": url, "ok": false, "reason": e.reason.code(),
                                    "detail": e.detail, "ms": started.elapsed().as_millis() as u64})
                            );
                        }
                    }
                }
            }
            log!("[webview-crawl] done ok={} failed={:?}", ok, failed);
        });
    });
}

/// Absolute `<a href>` links of a page, in order, without fragments (not
/// `<link href>`: stylesheets, icons and manifests are not pages).
fn links_of(html: &str, base: &str) -> Vec<String> {
    let Ok(base) = url::Url::parse(base) else {
        return Vec::new();
    };
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find("<a ") {
        let tag_start = at + i;
        let Some(tag_len) = lower[tag_start..].find('>') else {
            break;
        };
        let tag = &html[tag_start..tag_start + tag_len];
        at = tag_start + tag_len;
        let Some(h) = tag.to_ascii_lowercase().find("href=") else {
            continue;
        };
        let rest = &tag[h + 5..];
        let (quote, rest) = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => (q, &rest[1..]),
            _ => continue,
        };
        let Some(end) = rest.find(quote) else {
            continue;
        };
        if let Ok(mut u) = base.join(&rest[..end].replace("&amp;", "&")) {
            u.set_fragment(None);
            if matches!(u.scheme(), "http" | "https") {
                out.push(u.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_find_anchor_links_only() {
        let html = r#"<link href="/favicon.ico"><a href="/news/a#top">A</a> <a class="x" href='https://x.org/b?a=1&amp;b=2'>B</a> <a href="mailto:x@y">m</a>"#;
        assert_eq!(
            links_of(html, "https://site.org/"),
            ["https://site.org/news/a", "https://x.org/b?a=1&b=2"]
        );
    }

    #[test]
    fn should_fill_in_the_poll_script() {
        let js = poll_js(42, true);
        assert!(js.contains("invoke(42,'octos.render'"));
        assert!(js.contains("if(true)"));
        assert!(!js.contains("JOB") && !js.contains("WANT_HTML"));
        assert!(poll_js(7, false).contains("if(false)"));
    }
}
