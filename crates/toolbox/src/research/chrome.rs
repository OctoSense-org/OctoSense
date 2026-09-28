//! Headless Chrome for pages that need JavaScript (feature `octos-engine`).
//!
//! The octos reader ([`octos_research::reader`]) reads a page over plain
//! HTTP first and asks a renderer only when that yields no main text: Google
//! News article links (which reach the publisher through a script) and
//! script-built pages. This is that renderer. It follows octos `deep_crawl`'s
//! reading discipline (octos#2568) in process, over the DevTools protocol:
//!
//! - **headless only** (`--headless=new`), a throwaway profile, no windows,
//!   never an OS screenshot;
//! - **not disguised**: Chrome's own User-Agent with the octos research
//!   product token appended, no automation-hiding switches or scripts, and a
//!   bot challenge ends the attempt ("not bypassed");
//! - **SSRF-safe inside the browser**: every request the tab makes (the page,
//!   each redirect, subresources) is paused and continued only when
//!   [`octos_research::net::check_url`] passes (public address, DNS fail
//!   closed); images, media and fonts are not fetched at all. The final URL
//!   and every main-frame navigation are checked again here, and once more by
//!   the reader ([`octos_research::reader::Reader::accept_rendered`]);
//! - **process hygiene**: one browser per process ([`Chrome::shared`]),
//!   started on the first render, with at most [`ChromeConfig::max_tabs`]
//!   pages at a time. Chrome runs in its own process group; the guard that
//!   owns it kills the whole group and deletes the profile when it is
//!   dropped, which happens when the last backend using it goes away, after
//!   [`ChromeConfig::idle_close`] without a render, on [`Chrome::shutdown`],
//!   or when the browser stops answering. Every render runs under
//!   [`ChromeConfig::render_timeout`], and its tab is closed however the
//!   render ends. (A process killed with SIGKILL runs no destructor, so its
//!   browser is left behind; nothing short of Chrome's pipe transport covers
//!   that.)

use futures_util::{SinkExt, StreamExt};
use octos_research::reader::Rendered;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

/// Appended to Chrome's own User-Agent: who is reading, and where to learn
/// more. The same product token octos `deep_crawl` uses.
pub const UA_TOKEN: &str =
    "octos-research/1.0 (+https://github.com/octos-org/octos) OctoSense-Toolbox/0.1";

/// Rendered HTML handed to the reader, at most.
pub const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;

/// A page with less visible text than this is given more time to settle.
const MIN_TEXT_CHARS: u64 = 200;

/// Environment variables Chrome is not started with (library injection and
/// the like).
const BLOCKED_ENV: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "DYLD_FRAMEWORK_PATH",
    "NODE_OPTIONS",
];

#[derive(Debug, Clone)]
pub struct ChromeConfig {
    /// The Chrome or Chromium binary; `None` looks in the usual places.
    pub binary: Option<PathBuf>,
    /// Pages rendered at the same time (tabs of the one browser).
    pub max_tabs: usize,
    /// The longest one render may take, from asking for a tab to the HTML.
    pub render_timeout: Duration,
    /// The longest Chrome may take to start and open its DevTools port.
    pub launch_timeout: Duration,
    /// Time a loaded page gets for its scripts before it is read.
    pub settle: Duration,
    /// The browser is closed after this long without a render.
    pub idle_close: Duration,
}

impl Default for ChromeConfig {
    fn default() -> Self {
        Self {
            binary: None,
            max_tabs: 2,
            render_timeout: Duration::from_secs(30),
            launch_timeout: Duration::from_secs(15),
            settle: Duration::from_millis(1500),
            idle_close: Duration::from_secs(60),
        }
    }
}

/// The Chrome or Chromium binary on this machine, if any.
pub fn find_binary() -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for name in [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
    ] {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}

/// The Chrome command line. Deliberately without automation-hiding switches
/// (`--disable-blink-features=AutomationControlled`, a spoofed
/// `--user-agent`) and without `--no-sandbox`.
pub fn chrome_args(profile: &Path) -> Vec<String> {
    vec![
        "--headless=new".into(),
        "--remote-debugging-port=0".into(),
        format!("--user-data-dir={}", profile.display()),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-gpu".into(),
        "--disable-dev-shm-usage".into(),
        "--disable-extensions".into(),
        "--disable-background-networking".into(),
        "--disable-sync".into(),
        "--mute-audio".into(),
        "about:blank".into(),
    ]
}

/// One running Chrome. Dropping it kills Chrome's process group and removes
/// its profile.
struct Browser {
    child: Mutex<Child>,
    pid: u32,
    port: u16,
    profile: PathBuf,
    user_agent: String,
}

impl Drop for Browser {
    fn drop(&mut self) {
        kill_group(self.pid);
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&self.profile);
    }
}

/// SIGKILL to the process group Chrome leads (its renderer, GPU and network
/// helpers are in it).
fn kill_group(pid: u32) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(kill_group_args(pid))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// `kill` arguments that signal the whole process group `pgid`. The `--` is
/// required: procps `kill` on Ubuntu 24.04 reads a bare `-<pgid>` as an
/// option and keeps only its first digit, so `-1234` became `kill(-1)`, which
/// signals every process of the user (it took down the CI runner).
fn kill_group_args(pgid: u32) -> [String; 3] {
    ["-KILL".into(), "--".into(), format!("-{pgid}")]
}

/// Whether a process with this id exists (tests and diagnostics).
pub fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// The shared headless browser. See the module documentation.
pub struct Chrome {
    config: ChromeConfig,
    binary: PathBuf,
    browser: tokio::sync::Mutex<Option<Arc<Browser>>>,
    tabs: tokio::sync::Semaphore,
    in_use: AtomicUsize,
    /// Bumped by every render, so an idle timer knows whether it is stale.
    epoch: AtomicU64,
    launches: AtomicU64,
    /// Per `host:port`, whether the in-browser SSRF check let it through.
    verdicts: Mutex<HashMap<String, bool>>,
    /// DevTools HTTP endpoint client (loopback only, no proxy).
    http: reqwest::Client,
}

static PROFILE_SEQ: AtomicU64 = AtomicU64::new(0);

impl Chrome {
    /// A browser of its own, or `None` when no Chrome binary is found.
    pub fn new(config: ChromeConfig) -> Option<Arc<Self>> {
        let binary = config.binary.clone().or_else(find_binary)?;
        let http = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .ok()?;
        Some(Arc::new(Self {
            tabs: tokio::sync::Semaphore::new(config.max_tabs.max(1)),
            config,
            binary,
            browser: tokio::sync::Mutex::new(None),
            in_use: AtomicUsize::new(0),
            epoch: AtomicU64::new(0),
            launches: AtomicU64::new(0),
            verdicts: Mutex::new(HashMap::new()),
            http,
        }))
    }

    /// The process's one browser: the existing one while any backend still
    /// holds it (its configuration wins), else a new one.
    pub fn shared(config: ChromeConfig) -> Option<Arc<Self>> {
        static SHARED: Mutex<Weak<Chrome>> = Mutex::new(Weak::new());
        let mut shared = SHARED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(chrome) = shared.upgrade() {
            return Some(chrome);
        }
        let chrome = Self::new(config)?;
        *shared = Arc::downgrade(&chrome);
        Some(chrome)
    }

    /// Chrome's process id while it runs.
    pub async fn pid(&self) -> Option<u32> {
        self.browser.lock().await.as_ref().map(|b| b.pid)
    }

    /// Times Chrome was started.
    pub fn launches(&self) -> u64 {
        self.launches.load(Ordering::Relaxed)
    }

    /// Closes the browser now (a render in flight keeps it until it ends).
    pub async fn shutdown(&self) {
        self.browser.lock().await.take();
    }

    /// Renders `url` in a tab and returns the document the browser ended on.
    pub async fn render(self: &Arc<Self>, url: &str) -> Result<Rendered, String> {
        let _permit = self
            .tabs
            .acquire()
            .await
            .map_err(|_| "renderer closed".to_string())?;
        self.in_use.fetch_add(1, Ordering::SeqCst);
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let result = tokio::time::timeout(self.config.render_timeout, self.render_once(url))
            .await
            .unwrap_or_else(|_| {
                Err(format!(
                    "render timed out after {} s",
                    self.config.render_timeout.as_secs()
                ))
            });
        if self.in_use.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.close_when_idle();
        }
        result
    }

    /// After [`ChromeConfig::idle_close`] with no render, closes the browser.
    fn close_when_idle(self: &Arc<Self>) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let epoch = self.epoch.load(Ordering::SeqCst);
        let idle = self.config.idle_close;
        let weak = Arc::downgrade(self);
        runtime.spawn(async move {
            tokio::time::sleep(idle).await;
            let Some(chrome) = weak.upgrade() else { return };
            if chrome.in_use.load(Ordering::SeqCst) == 0
                && chrome.epoch.load(Ordering::SeqCst) == epoch
            {
                chrome.shutdown().await;
            }
        });
    }

    /// The running browser, started if need be.
    async fn browser(&self) -> Result<Arc<Browser>, String> {
        let mut slot = self.browser.lock().await;
        if let Some(browser) = slot.as_ref() {
            let exited = browser
                .child
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .try_wait()
                .map(|s| s.is_some())
                .unwrap_or(true);
            if !exited {
                return Ok(browser.clone());
            }
            slot.take();
        }
        let browser = Arc::new(self.launch().await?);
        *slot = Some(browser.clone());
        Ok(browser)
    }

    async fn launch(&self) -> Result<Browser, String> {
        let profile = std::env::temp_dir().join(format!(
            "octosense-toolbox-chrome-{}-{}",
            std::process::id(),
            PROFILE_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&profile).map_err(|e| format!("chrome profile: {e}"))?;
        let mut command = Command::new(&self.binary);
        command
            .args(chrome_args(&profile))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for var in BLOCKED_ENV {
            command.env_remove(var);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // Its own group, so the guard can kill every helper with it.
            command.process_group(0);
        }
        let child = command.spawn().map_err(|e| {
            let _ = std::fs::remove_dir_all(&profile);
            format!("could not start Chrome: {e}")
        })?;
        self.launches.fetch_add(1, Ordering::Relaxed);
        // From here on the guard owns the process: an early return kills it.
        let mut browser = Browser {
            pid: child.id(),
            child: Mutex::new(child),
            port: 0,
            profile,
            user_agent: String::new(),
        };
        let deadline = tokio::time::Instant::now() + self.config.launch_timeout;
        let port_file = browser.profile.join("DevToolsActivePort");
        browser.port = loop {
            if let Some(port) = std::fs::read_to_string(&port_file)
                .ok()
                .and_then(|s| s.lines().next()?.trim().parse::<u16>().ok())
            {
                break port;
            }
            if browser
                .child
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .try_wait()
                .map(|s| s.is_some())
                .unwrap_or(true)
            {
                return Err("Chrome exited while starting".into());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!(
                    "Chrome did not open its DevTools port within {} s",
                    self.config.launch_timeout.as_secs()
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        let version: Value = self
            .http
            .get(format!("http://127.0.0.1:{}/json/version", browser.port))
            .send()
            .await
            .map_err(|e| format!("DevTools: {e}"))?
            .json()
            .await
            .map_err(|e| format!("DevTools: {e}"))?;
        let base = version["User-Agent"].as_str().unwrap_or_default();
        browser.user_agent = if base.is_empty() {
            UA_TOKEN.to_owned()
        } else {
            format!("{base} {UA_TOKEN}")
        };
        Ok(browser)
    }

    async fn render_once(&self, url: &str) -> Result<Rendered, String> {
        let browser = self.browser().await?;
        let target: Value = match self
            .http
            .put(format!(
                "http://127.0.0.1:{}/json/new?about:blank",
                browser.port
            ))
            .send()
            .await
        {
            Ok(response) => response
                .json()
                .await
                .map_err(|e| format!("DevTools: {e}"))?,
            Err(e) => {
                // The browser is not answering: drop it (the guard kills it)
                // so the next render starts a fresh one.
                let mut slot = self.browser.lock().await;
                if slot.as_ref().is_some_and(|b| Arc::ptr_eq(b, &browser)) {
                    slot.take();
                }
                return Err(format!("DevTools: {e}"));
            }
        };
        let (Some(id), Some(ws_url)) = (
            target["id"].as_str(),
            target["webSocketDebuggerUrl"].as_str(),
        ) else {
            return Err("DevTools gave no tab".into());
        };
        let _tab = TabGuard {
            http: self.http.clone(),
            port: browser.port,
            id: id.to_owned(),
        };
        let (ws, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .map_err(|e| format!("DevTools socket: {e}"))?;
        let mut tab = Tab {
            ws,
            chrome: self,
            next_id: 1,
            navigations: Vec::new(),
            blocked_documents: Vec::new(),
            loaded: false,
        };
        tab.render(url, &browser.user_agent, self.config.settle)
            .await
    }

    /// The in-browser SSRF check, cached per host and port.
    async fn request_allowed(&self, url: &str) -> bool {
        let Ok(parsed) = url::Url::parse(url) else {
            return false;
        };
        match parsed.scheme() {
            "data" | "blob" | "about" => return true,
            "http" | "https" => {}
            _ => return false,
        }
        let key = format!(
            "{}:{}",
            parsed.host_str().unwrap_or_default(),
            parsed.port_or_known_default().unwrap_or(0)
        );
        if let Some(verdict) = self
            .verdicts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
        {
            return *verdict;
        }
        let verdict = octos_research::net::check_url(url).await.is_ok();
        self.verdicts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key, verdict);
        verdict
    }
}

/// Closes a tab however its render ended.
struct TabGuard {
    http: reqwest::Client,
    port: u16,
    id: String,
}

impl Drop for TabGuard {
    fn drop(&mut self) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let request = self.http.get(format!(
            "http://127.0.0.1:{}/json/close/{}",
            self.port, self.id
        ));
        runtime.spawn(async move {
            let _ = request.send().await;
        });
    }
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// JS returning the rendered document: its URL, HTML, visible text length
/// and the start of that text (for bot-challenge detection).
const PAGE_JS: &str = r#"JSON.stringify({
  href: location.href,
  html: document.documentElement ? document.documentElement.outerHTML : '',
  text_len: document.body ? (document.body.innerText || '').length : 0,
  head: document.body ? (document.body.innerText || '').slice(0, 2000) : ''
})"#;

struct Tab<'a> {
    ws: Socket,
    chrome: &'a Chrome,
    next_id: u64,
    /// Main-frame document URLs, in order.
    navigations: Vec<String>,
    /// Document requests the SSRF check refused.
    blocked_documents: Vec<String>,
    loaded: bool,
}

impl Tab<'_> {
    async fn render(
        &mut self,
        url: &str,
        user_agent: &str,
        settle: Duration,
    ) -> Result<Rendered, String> {
        self.call("Fetch.enable", json!({"patterns": [{"urlPattern": "*"}]}))
            .await?;
        self.call(
            "Emulation.setUserAgentOverride",
            json!({"userAgent": user_agent}),
        )
        .await?;
        self.call("Page.enable", json!({})).await?;
        let navigated = self.call("Page.navigate", json!({"url": url})).await?;
        if let Some(error) = navigated["errorText"].as_str().filter(|e| !e.is_empty()) {
            return Err(format!("navigation failed: {error}"));
        }
        // The load event, then time for scripts. A Google News link loads
        // an interstitial whose script then goes to the publisher.
        let load_deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        while !self.loaded && tokio::time::Instant::now() < load_deadline {
            self.pump(Duration::from_millis(250)).await?;
        }
        self.pump(settle).await?;
        let mut page = self.page().await?;
        for _ in 0..4 {
            let on_interstitial =
                crate::host::url_host(&page.href).as_deref() == Some("news.google.com");
            if !on_interstitial && page.text_len >= MIN_TEXT_CHARS {
                break;
            }
            self.pump(Duration::from_millis(1500)).await?;
            page = self.page().await?;
        }
        if is_bot_challenge(&page.head) {
            return Err("blocked by a bot challenge (not bypassed)".into());
        }
        if let Some(blocked) = self.blocked_documents.first() {
            return Err(format!(
                "ssrf_blocked: the page tried to load a private or internal address ({blocked})"
            ));
        }
        if page.href.starts_with("chrome-error:") {
            return Err("navigation failed (browser error page)".into());
        }
        for u in self
            .navigations
            .iter()
            .chain(std::iter::once(&page.href))
            .filter(|u| !u.is_empty() && !u.starts_with("about:"))
        {
            octos_research::net::check_url(u)
                .await
                .map_err(|e| format!("ssrf_blocked: the browser went to {u}: {e}"))?;
        }
        let mut html = page.html;
        if html.len() > MAX_HTML_BYTES {
            let mut end = MAX_HTML_BYTES;
            while !html.is_char_boundary(end) {
                end -= 1;
            }
            html.truncate(end);
        }
        Ok(Rendered {
            final_url: page.href,
            html,
            navigations: std::mem::take(&mut self.navigations),
            // Not tracked over CDP: the reader classifies error pages and
            // walls from the document itself.
            status: None,
        })
    }

    async fn page(&mut self) -> Result<Page, String> {
        let result = self
            .call(
                "Runtime.evaluate",
                json!({"expression": PAGE_JS, "returnByValue": true}),
            )
            .await?;
        if let Some(e) = result.get("exceptionDetails") {
            return Err(format!("page script failed: {e}"));
        }
        let value: Value = result["result"]["value"]
            .as_str()
            .and_then(|s| serde_json::from_str(s).ok())
            .ok_or("page script returned nothing")?;
        Ok(Page {
            href: value["href"].as_str().unwrap_or_default().to_owned(),
            html: value["html"].as_str().unwrap_or_default().to_owned(),
            text_len: value["text_len"].as_u64().unwrap_or(0),
            head: value["head"].as_str().unwrap_or_default().to_owned(),
        })
    }

    async fn send(&mut self, method: &str, params: Value) -> Result<u64, String> {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({"id": id, "method": method, "params": params});
        self.ws
            .send(Message::Text(message.to_string()))
            .await
            .map_err(|e| format!("DevTools socket: {e}"))?;
        Ok(id)
    }

    /// Sends a command and waits for its answer, serving events meanwhile.
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.send(method, params).await?;
        loop {
            let message = self.next_message().await?;
            if message["id"].as_u64() == Some(id) {
                if let Some(error) = message.get("error") {
                    return Err(format!("{method}: {error}"));
                }
                return Ok(message["result"].clone());
            }
            self.handle(&message).await?;
        }
    }

    /// Serves events for `period`.
    async fn pump(&mut self, period: Duration) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + period;
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Ok(());
            }
            match tokio::time::timeout(deadline - now, self.next_message()).await {
                Ok(message) => {
                    let message = message?;
                    self.handle(&message).await?;
                }
                Err(_) => return Ok(()),
            }
        }
    }

    async fn next_message(&mut self) -> Result<Value, String> {
        loop {
            match self.ws.next().await {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(value) = serde_json::from_str(&text) {
                        return Ok(value);
                    }
                }
                Some(Ok(Message::Close(_))) | None => {
                    return Err("DevTools socket closed".into());
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => return Err(format!("DevTools socket: {e}")),
            }
        }
    }

    /// Paused requests go on only to public addresses (images, media and
    /// fonts not at all); main-frame navigations are recorded.
    async fn handle(&mut self, message: &Value) -> Result<(), String> {
        match message["method"].as_str().unwrap_or_default() {
            "Fetch.requestPaused" => {
                let params = &message["params"];
                let request_id = params["requestId"].as_str().unwrap_or_default().to_owned();
                let url = params["request"]["url"].as_str().unwrap_or_default();
                let kind = params["resourceType"].as_str().unwrap_or_default();
                let skipped = matches!(kind, "Image" | "Media" | "Font");
                let allowed = !skipped && self.chrome.request_allowed(url).await;
                if !allowed && !skipped && kind == "Document" {
                    self.blocked_documents.push(url.to_owned());
                }
                if allowed {
                    self.send("Fetch.continueRequest", json!({"requestId": request_id}))
                        .await?;
                } else {
                    self.send(
                        "Fetch.failRequest",
                        json!({"requestId": request_id, "errorReason": "BlockedByClient"}),
                    )
                    .await?;
                }
            }
            "Page.frameNavigated" => {
                let frame = &message["params"]["frame"];
                if frame.get("parentId").is_none() {
                    if let Some(u) = frame["url"].as_str() {
                        self.navigations.push(u.to_owned());
                    }
                }
            }
            "Page.loadEventFired" => self.loaded = true,
            _ => {}
        }
        Ok(())
    }
}

struct Page {
    href: String,
    html: String,
    text_len: u64,
    head: String,
}

/// Whether a page's text is a bot challenge (which is never worked around).
pub fn is_bot_challenge(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "performing security verification",
        "press & hold to confirm you are",
        "please verify you are a human",
        "checking your browser",
        "just a moment...",
        "attention required! | cloudflare",
        "enable javascript and cookies to continue",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browser_is_headless_and_not_disguised() {
        let args = chrome_args(Path::new("/tmp/p"));
        assert!(args.contains(&"--headless=new".to_string()));
        for forbidden in [
            "AutomationControlled",
            "--user-agent",
            "--no-sandbox",
            "--disable-infobars",
        ] {
            assert!(
                !args.iter().any(|a| a.contains(forbidden)),
                "{forbidden} in {args:?}"
            );
        }
        assert!(UA_TOKEN.contains("octos-research/1.0"));
        assert!(!UA_TOKEN.contains("Chrome/"));
    }

    #[test]
    fn bot_challenges_are_recognised() {
        assert!(is_bot_challenge("Just a moment...\nChecking"));
        assert!(!is_bot_challenge("The strait was closed on Monday."));
    }

    #[test]
    fn group_kill_passes_the_pgid_after_a_double_dash() {
        assert_eq!(kill_group_args(1234), ["-KILL", "--", "-1234"]);
    }

    #[test]
    fn no_negative_pid_kill_without_a_double_dash() {
        // A bare `-<pid>` right after the signal is parsed as an option by
        // some `kill` builds (see `kill_group_args`).
        let src = include_str!("chrome.rs");
        let needle = ["[\"-KILL\", &format!(\"-{", "pid}\")]"].concat();
        assert!(!src.contains(&needle), "negative-pid kill without `--`");
    }
}
