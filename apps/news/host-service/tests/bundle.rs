//! The News bundle and the service agree, and the bundle still works where
//! no shell offers the service (its own fetch, as before).
use octosense_appstore::services::{dispatch, take_replies_for, ServiceCall, ServiceHost};
use octosense_news_service::sources;
use serde_json::{json, Value};

const SCRIPT: &str = include_str!("../../bundle/main.splash");

/// The `sources` table at the top of main.splash: `(id, url)`.
fn script_sources() -> Vec<(String, String)> {
    let table = SCRIPT.split_once("let sources = [").unwrap().1.split_once("\n]").unwrap().0;
    table
        .lines()
        .filter_map(|line| {
            let field = |key: &str| line.split_once(&format!("{key}: \""))?.1.split_once('"').map(|(v, _)| v.to_string());
            Some((field("id")?, field("url")?))
        })
        .collect()
}

#[test]
fn the_service_fetches_the_bundles_own_feeds() {
    let script = script_sources();
    let service: Vec<(String, String)> = sources::app_feeds().into_iter().map(|s| (s.id, s.url)).collect();
    assert_eq!(script, service, "each tab reads the same feed either way");
}

#[test]
fn the_scripts_own_fetch_reaches_only_declared_hosts() {
    let hosts = sources::manifest_hosts();
    for (id, url) in script_sources() {
        assert!(hosts.contains(&sources::host_of(&url).unwrap()), "{id}: {url}");
    }
}

#[test]
fn the_bundle_uses_the_service_only_where_it_is_granted() {
    // Granted: read through the service. Not granted (App Hub does not know
    // `news` yet, or a shell does not register it): the script's own fetch.
    assert!(SCRIPT.contains("use_service = host.has(\"news\")"));
    assert!(SCRIPT.contains("if use_service { refresh_service(); return }"));
    assert!(SCRIPT.contains("host.request(\"news.list\", {feed: s.id current: true limit: 30}"));
    assert!(SCRIPT.contains("net.http_request("), "the fallback fetch is still there");
    // A granted but unanswered family falls back too.
    assert!(SCRIPT.contains("use_service = false"));
    let manifest: Value = serde_json::from_str(sources::MANIFEST).unwrap();
    let capabilities: Vec<&str> = manifest["capabilities"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(capabilities.contains(&"net"), "the fallback fetch needs net");
}

struct NoSheets;
impl ServiceHost for NoSheets {
    fn open_sheet(&mut self, _body: String) {}
    fn close_sheet(&mut self) {}
}

/// In a shell that grants `news` but registers no service, the app hears an
/// error (not silence), which is what sends the script to its own fetch.
/// This test binary registers nothing.
#[test]
fn an_unregistered_service_answers_with_an_error() {
    let call = ServiceCall {
        app_id: "os.news".into(),
        service: "news.list".into(),
        args: json!({"feed": "hn", "current": true}),
        from_sheet: false,
        may_prompt: true,
        host_dir: std::env::temp_dir(),
    };
    dispatch(call, 51_000, 1, &mut NoSheets);
    let answer = take_replies_for(&[51_000]).pop().expect("answered at once").2;
    assert!(answer.unwrap_err().contains("no service answers"));
}
