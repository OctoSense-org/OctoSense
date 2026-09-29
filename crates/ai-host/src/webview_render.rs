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
//! one at a time: it loads the URL in a hidden, navigable system browser,
//! polls the document every second (the Android WebView reports no "page
//! finished"), and once the page is complete and has settled returns its
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

/// The hidden browser's id.
fn browser_id() -> LiveId {
    live_id!(octos_page_renderer)
}

/// The bridge tool the extraction script calls.
const TOOL: &str = "octos.render";
/// Seconds between document polls.
const POLL: f64 = 1.0;
/// Seconds a complete document must stay complete before it is read.
const SETTLE: f64 = 1.5;
/// Polls a self-clearing check may take before it counts as the page.
const MAX_CHECK_POLLS: u32 = 12;
/// Seconds one render may take.
const DEADLINE: f64 = 45.0;
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

/// Wording of checks that clear themselves in a real browser within
/// seconds. (octos#2637 adds `access::is_interstitial`; switch to it once
/// OctoSense pins an octos that has it.)
const SELF_CLEARING: &[&str] = &[
    "just a moment",
    "checking your browser",
    "performing security verification",
    "enable javascript and cookies to continue",
    "正在进行安全检测",
    "正在检测当前网络环境",
];

/// Wording of checks that ask a person (never done for them).
const ASKS_PERSON: &[&str] = &[
    "captcha",
    "人机验证",
    "滑动验证",
    "拖动滑块",
    "请完成安全验证",
];

/// Whether a page's visible text is a check that clears itself.
fn self_clearing(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.chars().filter(|c| !c.is_whitespace()).count() < 600
        && !ASKS_PERSON.iter().any(|p| lower.contains(p))
        && SELF_CLEARING.iter().any(|p| lower.contains(p))
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
    check_polls: u32,
    reading: bool,
    navigations: Vec<String>,
}

/// Serves [`WebRenderRequest`]s with the hidden WebView, one at a time.
#[derive(Default)]
pub struct WebViewRenderHost {
    queue: VecDeque<WebRenderRequest>,
    active: Option<Active>,
    spawned: bool,
    /// The page the last render read: until the next load commits, polls
    /// still see it.
    last_page: Option<String>,
}

impl WebViewRenderHost {
    pub fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        match event {
            Event::Actions(actions) => {
                for action in actions {
                    if let Some(req) = action.downcast_ref::<WebRenderRequest>() {
                        self.queue.push_back(req.clone());
                    } else if let Some(inv) = action.downcast_ref::<NativeSystemBrowserInvoke>() {
                        if inv.browser_id == browser_id().get_value() && inv.tool == TOOL {
                            self.on_poll(cx, inv.call_id as u64, &inv.args);
                        }
                    } else if let Some(err) = action.downcast_ref::<NativeSystemBrowserPageError>()
                    {
                        if err.browser_id == browser_id().get_value() {
                            // The WebView's renderer process died: the
                            // platform dropped that WebView; spawn a new one.
                            if err.code == RENDER_PROCESS_GONE {
                                self.spawned = false;
                                self.last_page = None;
                            }
                            if let Some(a) = self.active.as_ref() {
                                let job = a.job;
                                let msg = format!(
                                    "render_failed: the page did not load ({} {})",
                                    err.code, err.description
                                );
                                self.finish(cx, job, Err(msg));
                            }
                        }
                    }
                }
            }
            Event::Timer(_) => {
                if let Some(a) = self.active.as_ref() {
                    if a.deadline.is_event(event).is_some() {
                        let job = a.job;
                        self.finish(
                            cx,
                            job,
                            Err(format!("render_timeout: no page after {DEADLINE}s")),
                        );
                    } else if a.poll.is_event(event).is_some() {
                        let (job, want) = (a.job, a.reading);
                        cx.system_browser(browser_id()).eval_js(&poll_js(job, want));
                        if let Some(a) = self.active.as_mut() {
                            a.poll = cx.start_timeout(POLL);
                        }
                    }
                }
            }
            _ => {}
        }
        if self.active.is_none() {
            self.start_next(cx);
        }
    }

    fn start_next(&mut self, cx: &mut Cx) {
        let Some(req) = self.queue.pop_front() else {
            return;
        };
        let mut browser = cx.system_browser(browser_id());
        if self.spawned {
            browser.set_url(&req.url, false);
        } else {
            browser.spawn_navigable(&req.url);
            self.spawned = true;
        }
        // Hidden: the person never sees the pages read for them.
        browser.update(Area::Empty, false);
        let now = cx.seconds_since_app_start();
        self.active = Some(Active {
            job: req.job,
            url: req.url,
            poll: cx.start_timeout(POLL),
            deadline: cx.start_timeout(DEADLINE),
            complete_since: None,
            started: now,
            check_polls: 0,
            reading: false,
            navigations: Vec::new(),
        });
    }

    fn on_poll(&mut self, cx: &mut Cx, job: u64, args: &str) {
        let now = cx.seconds_since_app_start();
        let Some(a) = self.active.as_mut() else {
            return;
        };
        if a.job != job {
            return;
        }
        let v: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
        let url = v["url"].as_str().unwrap_or("").to_string();
        // Still on the previous page: the load has not committed yet.
        let committed = !url.is_empty()
            && url != "about:blank"
            && now - a.started > 0.5
            && (self.last_page.as_deref() != Some(url.as_str()) || url == a.url);
        if !committed || v["ready"].as_str() != Some("complete") {
            a.complete_since = None;
            return;
        }
        if a.navigations.last() != Some(&url) {
            a.navigations.push(url.clone());
        }
        if let Some(html) = v["html"].as_str() {
            let text = v["text"].as_str().unwrap_or("");
            if self_clearing(text) && a.check_polls < MAX_CHECK_POLLS {
                // Wait for the check to finish; read again later.
                a.check_polls += 1;
                a.reading = false;
                a.complete_since = None;
                return;
            }
            self.last_page = Some(url.clone());
            let rendered = Rendered {
                final_url: url,
                html: html.to_string(),
                navigations: a.navigations.clone(),
                status: None,
            };
            self.finish(cx, job, Ok(rendered));
            return;
        }
        let since = *a.complete_since.get_or_insert(now);
        if now - since >= SETTLE {
            // Complete and settled: the next poll brings the HTML.
            a.reading = true;
        }
    }

    fn finish(&mut self, cx: &mut Cx, job: u64, result: Result<Rendered, String>) {
        if let Some(a) = self.active.take() {
            cx.stop_timer(a.poll);
            cx.stop_timer(a.deadline);
            if let Err(e) = &result {
                log!("webview render: {} failed: {}", a.url, e);
            }
        }
        reply(job, result);
        self.start_next(cx);
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
                                        && !account_link(&link)
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

/// Sign-in, sign-up and account pages (not followed). octos#2637 adds
/// `urls::is_account_link`; switch to it once OctoSense pins it.
fn account_link(raw: &str) -> bool {
    const SEGMENTS: &[&str] = &[
        "login",
        "signin",
        "sign-in",
        "signup",
        "sign-up",
        "register",
        "auth",
        "oauth",
        "sso",
        "account",
        "accounts",
        "usercenter",
        "passport",
        "logout",
        "password",
        "cart",
    ];
    url::Url::parse(raw).is_ok_and(|u| {
        u.path_segments()
            .is_some_and(|mut s| s.any(|seg| SEGMENTS.contains(&seg.to_ascii_lowercase().as_str())))
    })
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
    fn should_wait_only_for_checks_that_clear_themselves() {
        assert!(self_clearing(
            "Just a moment... Checking your browser before accessing"
        ));
        assert!(self_clearing(
            "火山引擎 正在进行安全检测... 为保障您的访问安全，系统正在检测当前网络环境"
        ));
        assert!(
            !self_clearing("请完成安全验证 拖动滑块完成拼图"),
            "asks a person"
        );
        assert!(!self_clearing("Please complete the CAPTCHA to continue"));
        let article = "Just a moment of reflection: ".to_string() + &"words ".repeat(300);
        assert!(!self_clearing(&article), "a long page is content");
    }

    #[test]
    fn should_find_links_and_skip_account_pages() {
        let html = r#"<link href="/favicon.ico"><a href="/news/a#top">A</a> <a class="x" href='https://x.org/b?a=1&amp;b=2'>B</a> <a href="mailto:x@y">m</a>"#;
        assert_eq!(
            links_of(html, "https://site.org/"),
            ["https://site.org/news/a", "https://x.org/b?a=1&b=2"]
        );
        assert!(account_link("https://medium.com/m/signin?op=login"));
        assert!(account_link("https://36kr.com/usercenter/basicinfo"));
        assert!(!account_link("https://www.bbc.com/news/articles/c1"));
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
