//! The news service end to end over fixtures: fetch runs, the ledger, the
//! tools, and App Hub's real dispatch path. No network.
use octosense_appstore::services::{dispatch, take_replies_for, ServiceCall, ServiceHost};
use octosense_news_service::{register_with, sources, ArticleReader, FetchReport, Fetcher, News, Options, Query, Request, Response, Retention};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const T0: i64 = 1_789_570_000; // 16 Sep 2026, after every fixture's date

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// Answers by URL prefix, remembers every request, and honours ETags.
#[derive(Default)]
struct Fixtures {
    routes: Mutex<Vec<(String, Response)>>,
    log: Mutex<Vec<Request>>,
}

impl Fixtures {
    fn standard() -> Arc<Fixtures> {
        let f = Arc::new(Fixtures::default());
        f.route("https://hn.algolia.com/", 200, &fixture("hn.json"), Some("\"hn-1\""));
        f.route("https://www.techmeme.com/", 200, &fixture("techmeme.xml"), None);
        f.route("https://news.google.com/rss?", 200, &fixture("google.xml"), None);
        f.route("https://news.google.com/rss/search", 200, &fixture("google-topic.xml"), None);
        f.route("https://api.gdeltproject.org/", 200, &fixture("gdelt.json"), None);
        f.route("https://feeds.bbci.co.uk/", 200, &fixture("bbc.xml"), None);
        f
    }

    fn route(&self, prefix: &str, status: u16, body: &str, etag: Option<&str>) {
        let mut routes = self.routes.lock().unwrap();
        routes.retain(|(p, _)| p != prefix);
        routes.push((prefix.into(), Response { status, body: body.into(), etag: etag.map(str::to_string), ..Response::default() }));
    }

    fn requests_to(&self, prefix: &str) -> Vec<Request> {
        self.log.lock().unwrap().iter().filter(|r| r.url.starts_with(prefix)).cloned().collect()
    }
}

impl Fetcher for Fixtures {
    fn get(&self, request: &Request) -> Result<Response, String> {
        self.log.lock().unwrap().push(request.clone());
        let routes = self.routes.lock().unwrap();
        let Some((_, response)) = routes.iter().filter(|(p, _)| request.url.starts_with(p.as_str())).max_by_key(|(p, _)| p.len()) else {
            return Err("connection refused".into());
        };
        if response.etag.is_some() && response.etag == request.etag {
            return Ok(Response { status: 304, ..Response::default() });
        }
        Ok(response.clone())
    }
}

struct Rig {
    dir: PathBuf,
    clock: Arc<AtomicI64>,
    fixtures: Arc<Fixtures>,
    reports: Arc<Mutex<Vec<FetchReport>>>,
    news: News,
}

impl Rig {
    fn new(tag: &str) -> Rig {
        Rig::with(tag, |o| o)
    }

    fn with(tag: &str, extra: impl FnOnce(Options) -> Options) -> Rig {
        let dir = std::env::temp_dir().join(format!("news-service-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let clock = Arc::new(AtomicI64::new(T0));
        let fixtures = Fixtures::standard();
        let reports = Arc::new(Mutex::new(Vec::new()));
        let news = News::new(extra(Rig::options(&dir, &clock, &fixtures, &reports)));
        Rig { dir, clock, fixtures, reports, news }
    }

    fn options(dir: &std::path::Path, clock: &Arc<AtomicI64>, fixtures: &Arc<Fixtures>, reports: &Arc<Mutex<Vec<FetchReport>>>) -> Options {
        let (clock, reports) = (clock.clone(), reports.clone());
        Options::default()
            .host_dir(dir)
            .timer(false)
            .fetcher(fixtures.clone())
            .spacing(Duration::ZERO, Duration::ZERO)
            .clock(move || clock.load(Ordering::SeqCst))
            .on_fetch(move |r| reports.lock().unwrap().push(r.clone()))
    }

    /// The same folder, as a shell would find it after a restart.
    fn reopen(&self) -> News {
        News::new(Rig::options(&self.dir, &self.clock, &self.fixtures, &self.reports))
    }

    fn advance(&self, secs: i64) {
        self.clock.fetch_add(secs, Ordering::SeqCst);
    }

    fn refresh(&self) -> FetchReport {
        self.news.refresh().unwrap().expect("not already running")
    }

    fn list(&self, args: Value) -> Value {
        self.news.list(&octosense_news_service::query_from(&args)).unwrap()
    }
}

fn titles(list: &Value) -> Vec<String> {
    list["items"].as_array().unwrap().iter().map(|i| i["title"].as_str().unwrap().to_string()).collect()
}

#[test]
fn a_run_files_every_source_and_reports_it() {
    let rig = Rig::new("run");
    let report = rig.refresh();
    let status: Vec<(&str, &str)> = report.sources.iter().map(|s| (s.id.as_str(), s.status)).collect();
    assert!(status.contains(&("hn", "ok")) && status.contains(&("techmeme", "ok")) && status.contains(&("google", "ok")), "{status:?}");
    assert!(status.contains(&("npr", "error")), "a source with no answer fails alone: {status:?}");
    // hn 2 + techmeme 2 + google 2 + bbc 3, less Google's and BBC's shared story.
    assert_eq!(report.new, 8, "{report:?}");
    assert_eq!(report.total, 8);
    assert_eq!(rig.reports.lock().unwrap().len(), 1, "the hook heard the run");
    assert!(report.clusters.iter().any(|c| c.topic == "tech" && c.count == 4), "{:?}", report.clusters);

    // Each tab reads exactly its source's latest fetch, in the source's order.
    let hn = rig.list(json!({"feed": "hn", "current": true}));
    assert_eq!(titles(&hn), vec!["Show HN: A tiny news reader", "Ask HN: What are you reading?"]);
    let first = &hn["items"][0];
    assert_eq!(first["points"], 312);
    assert_eq!(first["discussion"], "https://news.ycombinator.com/item?id=44000001");
    assert_eq!(first["url"], "https://example.com/reader");
    assert_eq!(first["lang"], "en");
    assert_eq!(first["fetched"], T0);
    assert_eq!(first["id"].as_str().unwrap().len(), 16);
    assert_eq!(hn["feed_error"], Value::Null);
    assert_eq!(hn["updated"], T0);
    let techmeme = rig.list(json!({"feed": "techmeme", "current": true}));
    assert_eq!(techmeme["items"][0]["source"], "TechMeme", "an item without an outlet is named after its feed");
    let npr = rig.list(json!({"feed": "npr", "current": true}));
    assert_eq!(npr["feed_error"], "connection refused");

    // Stored on disk, and read back after a restart.
    for file in ["items.json", "ledger.json", "sources.json"] {
        assert!(rig.dir.join("news").join(file).exists(), "{file}");
    }
    let again = rig.reopen().list(&Query { feed: Some("hn".into()), current: true, limit: 30, ..Query::default() }).unwrap();
    assert_eq!(titles(&again), titles(&hn), "News opens from the cache");
}

#[test]
fn the_ledger_keeps_stories_from_coming_back() {
    let rig = Rig::new("ledger");
    rig.refresh();
    // Same answers later: nothing new, and HN's ETag gets a 304.
    rig.advance(700);
    let report = rig.refresh();
    assert_eq!(report.new, 0, "{report:?}");
    let hn = report.sources.iter().find(|s| s.id == "hn").unwrap();
    assert_eq!(hn.status, "not_modified");
    assert_eq!(rig.fixtures.requests_to("https://hn.algolia.com/").last().unwrap().etag.as_deref(), Some("\"hn-1\""));
    // The 304 keeps the tab's list.
    assert_eq!(titles(&rig.list(json!({"feed": "hn", "current": true}))).len(), 2);

    // A story dropped by retention is still remembered as seen.
    let rig = Rig::with("ledger-retention", |o| o.retention(Retention { max_items: 1, ..Retention::default() }));
    rig.refresh();
    assert_eq!(rig.list(json!({}))["total"], 1);
    rig.fixtures.route("https://hn.algolia.com/", 200, &fixture("hn.json"), None);
    rig.advance(700);
    assert_eq!(rig.refresh().new, 0, "dropped stories are not new again");
}

#[test]
fn near_duplicates_merge_across_feeds() {
    let rig = Rig::new("dup");
    rig.refresh();
    let all = rig.list(json!({"limit": 100}));
    let markets: Vec<&Value> = all["items"].as_array().unwrap().iter().filter(|i| i["title"] == "Markets rally as rates hold").collect();
    assert_eq!(markets.len(), 1, "Google's and BBC's copies are one story");
    assert_eq!(markets[0]["feed"], "google");
    assert_eq!(markets[0]["also"], json!(["bbc-world"]));
    // The BBC tab still lists it (its latest fetch carried it).
    let bbc = rig.list(json!({"feed": "bbc-world", "current": true}));
    assert!(titles(&bbc).contains(&"Markets rally as rates hold".to_string()));
    // Tracking parameters do not make a second story.
    rig.fixtures.route(
        "https://feeds.bbci.co.uk/",
        200,
        &fixture("bbc.xml").replace("articles/c2</link>", "articles/c2?utm_source=rss</link>").replace("Storm season", "STORM SEASON"),
        None,
    );
    rig.advance(700);
    assert_eq!(rig.refresh().new, 0);
}

#[test]
fn list_filters() {
    let rig = Rig::new("filters");
    rig.refresh();
    let all = rig.list(json!({"limit": 100}));
    assert_eq!(all["total"], 8);
    let whens: Vec<i64> = all["items"].as_array().unwrap().iter().map(|i| i["published"].as_i64().unwrap()).collect();
    assert!(whens.windows(2).all(|w| w[0] >= w[1]), "newest first: {whens:?}");

    let page = rig.list(json!({"limit": 3, "offset": 2}));
    assert_eq!(page["total"], 8);
    assert_eq!(titles(&page), titles(&all)[2..5].to_vec());

    let tech = rig.list(json!({"topic": "TECH", "limit": 100}));
    assert_eq!(tech["total"], 4, "HN and TechMeme are tech");
    let world = rig.list(json!({"topic": "world", "limit": 100}));
    assert_eq!(world["total"], 3, "BBC's, including the shared story");

    let since = rig.list(json!({"since": 1_789_556_000, "limit": 100}));
    assert!(since["items"].as_array().unwrap().iter().all(|i| i["published"].as_i64().unwrap() >= 1_789_556_000));
    assert!(since["total"].as_u64().unwrap() < 8);

    assert_eq!(rig.list(json!({"lang": "de"}))["total"], 0);
    assert_eq!(rig.list(json!({"feed": "bbc-world"}))["total"], 3, "a feed's stories, shared ones too");
    assert_eq!(rig.list(json!({"limit": 100000}))["items"].as_array().unwrap().len(), 8, "limit is capped, not refused");
}

#[test]
fn topics_are_checked_stored_and_fetched() {
    let rig = Rig::new("topics");
    assert_eq!(rig.news.topics().unwrap(), vec![]);
    let set = rig.news.set_topics(&[json!({"query": "Elektroautos", "lang": "de", "region": "de"}), json!({"query": "Elektroautos", "lang": "de", "region": "DE"})]).unwrap();
    assert_eq!(set.len(), 1, "a repeated topic is kept once");
    assert_eq!(set[0].region, "DE");
    assert!(rig.news.set_topics(&[json!({"query": "", "lang": "de", "region": "DE"})]).is_err());
    assert!(rig.news.set_topics(&vec![json!({"query": "x"}); 21]).is_err());
    assert_eq!(rig.reopen().topics().unwrap(), set, "topics persist");

    let report = rig.refresh();
    let google = report.sources.iter().find(|s| s.id.ends_with("-google") && s.id.starts_with("topic-")).expect("the topic's Google News search");
    let gdelt = report.sources.iter().find(|s| s.id.ends_with("-gdelt")).expect("the topic's GDELT query");
    assert_eq!((google.status, gdelt.status), ("ok", "ok"));
    let asked = rig.fixtures.requests_to("https://news.google.com/rss/search");
    assert_eq!(asked[0].url, "https://news.google.com/rss/search?q=Elektroautos&hl=de-DE&gl=DE&ceid=DE:de");
    assert!(rig.fixtures.requests_to("https://api.gdeltproject.org/")[0].url.contains("sourcelang%3Agerman"));

    let topic = rig.list(json!({"topic": "Elektroautos", "limit": 100}));
    // Google's two, GDELT's two, less the story both carry.
    assert_eq!(topic["total"], 3, "{topic}");
    let german = rig.list(json!({"lang": "de", "limit": 100}));
    assert_eq!(german["total"], 2, "GDELT's French article keeps its own language");
    assert!(report.clusters.iter().any(|c| c.topic == "Elektroautos" && c.count == 3), "{:?}", report.clusters);
}

#[test]
fn only_declared_hosts_are_reached() {
    let rig = Rig::new("hosts");
    let imported = rig.news.import_opml(&fixture("feeds.opml")).unwrap();
    assert_eq!(imported["added"].as_array().unwrap().len(), 1, "{imported}");
    let skipped = imported["skipped"].as_array().unwrap();
    assert!(skipped.iter().any(|s| s["url"] == "https://blog.unlisted.example/feed" && s["reason"].as_str().unwrap().contains("network.hosts")));
    assert!(skipped.iter().any(|s| s["reason"] == "already followed"));
    let science = rig.news.sources().unwrap().into_iter().find(|s| s.url == "https://feeds.npr.org/1007/rss.xml").unwrap();
    assert_eq!(science.topics, vec!["science"]);

    // A redirect to an undeclared host is not followed.
    rig.fixtures.routes.lock().unwrap().push((
        "https://feeds.arstechnica.com/".into(),
        Response { status: 301, location: Some("https://evil.example/feed".into()), ..Response::default() },
    ));
    rig.refresh();
    assert!(rig.fixtures.requests_to("https://evil.example").is_empty());
    let status = rig.news.source_status().unwrap();
    let ars = status["sources"].as_array().unwrap().iter().find(|s| s["id"] == "ars").unwrap().clone();
    assert!(ars["last_error"].as_str().unwrap().contains("evil.example"), "{ars}");
    // Nothing but declared hosts was asked.
    let hosts = sources::manifest_hosts();
    for request in rig.fixtures.log.lock().unwrap().iter() {
        assert!(hosts.contains(&sources::host_of(&request.url).unwrap()), "{}", request.url);
    }
}

#[test]
fn a_failing_source_backs_off_on_the_timer() {
    let rig = Rig::new("backoff");
    rig.fixtures.route("https://www.techmeme.com/", 503, "", None);
    rig.refresh();
    let asked = || rig.fixtures.requests_to("https://www.techmeme.com/").len();
    assert_eq!(asked(), 1, "one request, no retry storm");
    // The timer retries after 30 s, then 1 min, 2 min, …: never sooner.
    rig.advance(20);
    rig.news.refresh_due().unwrap();
    assert_eq!(asked(), 1, "backed off");
    rig.advance(15);
    rig.news.refresh_due().unwrap();
    assert_eq!(asked(), 2, "the first retry, 30 s on");
    rig.advance(45);
    rig.news.refresh_due().unwrap();
    assert_eq!(asked(), 2, "the second waits a minute");
    rig.advance(20);
    rig.news.refresh_due().unwrap();
    assert_eq!(asked(), 3);
    // However long it failed, the timer tries again within the hour.
    for _ in 0..12 {
        rig.advance(3600);
        rig.news.refresh_due().unwrap();
    }
    assert_eq!(asked(), 15);
    let status = rig.news.source_status().unwrap();
    let techmeme = status["sources"].as_array().unwrap().iter().find(|s| s["id"] == "techmeme").unwrap().clone();
    assert!(techmeme["next_due"].as_i64().unwrap() - rig.clock.load(Ordering::SeqCst) <= 3600, "{techmeme}");
    rig.fixtures.route("https://www.techmeme.com/", 200, &fixture("techmeme.xml"), None);
    rig.advance(3600);
    let report = rig.news.refresh_due().unwrap().unwrap();
    assert_eq!(report.sources.iter().find(|s| s.id == "techmeme").unwrap().status, "ok");
    // And a source fetched a moment ago is not fetched again on a refresh.
    rig.advance(30);
    let report = rig.refresh();
    assert_eq!(report.sources.iter().find(|s| s.id == "techmeme").unwrap().status, "skipped");
    assert_eq!(asked(), 16);
}

/// The device: News's first fetch ran before the network was up (every
/// source failed), and neither opening News nor Refresh fetched again
/// until the back-off ended. A Refresh (which News also sends when it
/// opens) fetches a failed source again at once, at most every 10 s.
#[test]
fn refresh_retries_a_failed_source_at_once() {
    let rig = Rig::new("refresh-retry");
    for url in ["https://www.techmeme.com/", "https://hn.algolia.com/"] {
        rig.fixtures.route(url, 503, "", None);
    }
    rig.refresh();
    let asked = || rig.fixtures.requests_to("https://www.techmeme.com/").len();
    assert_eq!(asked(), 1);
    rig.advance(2);
    let report = rig.refresh();
    assert_eq!(asked(), 1, "not twice within 10 s");
    assert_eq!(report.sources.iter().find(|s| s.id == "techmeme").unwrap().status, "skipped");
    // The network came up.
    rig.fixtures.route("https://www.techmeme.com/", 200, &fixture("techmeme.xml"), None);
    rig.advance(10);
    let report = rig.refresh();
    assert_eq!(asked(), 2, "the person's Refresh fetched it");
    assert_eq!(report.sources.iter().find(|s| s.id == "techmeme").unwrap().status, "ok");
    assert!(rig.reports.lock().unwrap().len() >= 2, "each run that fetched is reported (and logged by the shell)");
}

struct Reader;
impl ArticleReader for Reader {
    fn read(&self, url: &str) -> Result<String, String> {
        Ok(format!("The full text of {url}."))
    }
}

#[test]
fn read_gives_the_summary_or_a_granted_readers_text() {
    let rig = Rig::new("read");
    rig.refresh();
    let id = rig.list(json!({"feed": "bbc-world", "current": true}))["items"][1]["id"].as_str().unwrap().to_string();
    let read = rig.news.read(&id, true).unwrap();
    assert_eq!(read["full_text"], false, "no reader was granted");
    assert_eq!(read["text"], "Forecasters warn of an active season.");
    assert!(rig.news.read("nope", false).is_err());

    let rig = Rig::with("read-full", |o| o.reader(Arc::new(Reader)));
    rig.refresh();
    let read = rig.news.read(&id, true).unwrap();
    assert_eq!(read["full_text"], true);
    assert_eq!(read["text"], "The full text of https://www.bbc.com/news/articles/c2.");
}

// --- Through App Hub's dispatch, as the Card runner calls it.

static SERIAL: Mutex<()> = Mutex::new(());
static NEXT: AtomicUsize = AtomicUsize::new(41_000);

struct NoSheets;
impl ServiceHost for NoSheets {
    fn open_sheet(&mut self, _body: String) {}
    fn close_sheet(&mut self) {}
}

fn ask(app: &str, dir: &std::path::Path, service: &str, args: Value) -> Result<Value, String> {
    let heap = NEXT.fetch_add(1, Ordering::SeqCst);
    let call = ServiceCall { app_id: app.into(), service: service.into(), args, from_sheet: false, may_prompt: true, host_dir: dir.to_path_buf() };
    dispatch(call, heap, 1, &mut NoSheets);
    for _ in 0..500 {
        if let Some((_, _, answer)) = take_replies_for(&[heap]).pop() {
            return answer.map(|json| serde_json::from_str(&json).unwrap());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("{service} never answered");
}

#[test]
fn the_card_runner_reaches_the_tools() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("news-service-dispatch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let clock = Arc::new(AtomicI64::new(T0));
    let fixtures = Fixtures::standard();
    let reports = Arc::new(Mutex::new(Vec::new()));
    // No host_dir: the service takes the runner's at the first request.
    let tick = clock.clone();
    register_with(
        Options::default()
            .timer(false)
            .fetcher(fixtures.clone())
            .spacing(Duration::ZERO, Duration::ZERO)
            .clock(move || tick.load(Ordering::SeqCst))
            .on_fetch(move |r| reports.lock().unwrap().push(r.clone())),
    );

    let empty = ask("os.news", &dir, "news.list", json!({"feed": "hn", "current": true})).unwrap();
    assert_eq!(empty["items"], json!([]), "nothing fetched yet");
    let report = ask("os.news", &dir, "news.refresh", json!({})).unwrap();
    assert_eq!(report["new"], 8);
    let hn = ask("os.news", &dir, "news.list", json!({"feed": "hn", "current": true, "limit": 30})).unwrap();
    assert_eq!(hn["items"].as_array().unwrap().len(), 2);
    let id = hn["items"][0]["id"].clone();
    let read = ask("os.news", &dir, "news.read", json!({"id": id})).unwrap();
    assert_eq!(read["item"]["title"], "Show HN: A tiny news reader");
    let topics = ask("os.news", &dir, "news.topics.set", json!({"topics": [{"query": "fusion energy", "lang": "en", "region": "GB"}]})).unwrap();
    assert_eq!(topics["topics"][0]["region"], "GB");
    assert_eq!(ask("os.news", &dir, "news.topics.get", json!({})).unwrap(), topics);
    let sources = ask("os.news", &dir, "news.sources", json!({})).unwrap();
    assert!(sources["sources"].as_array().unwrap().iter().any(|s| s["kind"] == "gdelt"));
    assert!(ask("os.news", &dir, "news.nope", json!({})).unwrap_err().contains("no method"));
    assert!(ask("com.example.app", &dir, "news.list", json!({})).unwrap_err().contains("system apps only"));
    assert!(dir.join("news/items.json").exists(), "the service keeps its files under <host_dir>/news");
}

/// The real feeds, once: `cargo test -p octosense-news-service -- --ignored live`.
#[test]
#[ignore]
fn live_smoke_test() {
    let dir = std::env::temp_dir().join(format!("news-service-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let news = News::new(Options::default().host_dir(&dir).timer(false));
    news.set_topics(&[json!({"query": "climate", "lang": "en", "region": "US"})]).unwrap();
    let report = news.refresh().unwrap().unwrap();
    for s in &report.sources {
        println!("{:<28} {:<13} {:>3} {}", s.id, s.status, s.new, s.error.clone().unwrap_or_default());
    }
    assert!(report.new > 0, "{report:?}");
    for feed in ["hn", "techmeme", "google"] {
        let tab = news.list(&Query { feed: Some(feed.into()), current: true, limit: 30, ..Query::default() }).unwrap();
        let items = tab["items"].as_array().unwrap();
        assert!(!items.is_empty(), "{feed}");
        for item in items.iter().take(2) {
            println!("{feed}: [{}] {} | {}", item["source"].as_str().unwrap(), item["title"].as_str().unwrap(), item["summary"].as_str().unwrap());
        }
    }
}
