//! `toolbox.deep_crawl` through `PeerToolbox` over a scripted site: the
//! crawl stays on the site, under the path prefix and inside the grant's
//! domains and limits.

mod common;

use octosense_toolbox::fixture::{FakeModel, FakeModelConfig};
use octosense_toolbox::host::{CallContext, HostError, HostFuture};
use octosense_toolbox::peer::{PeerToolbox, DEEP_CRAWL};
use octosense_toolbox::research::{
    FoundItem, LinkedPage, PageText, ResearchBackend, SearchQuery, SearchResults,
};
use octosense_toolbox::{scope, AppContext, Library};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// A site: each page's links.
struct Site {
    links: BTreeMap<&'static str, Vec<&'static str>>,
    read: Mutex<Vec<String>>,
}

impl ResearchBackend for Site {
    fn search<'a>(
        &'a self,
        _: &'a CallContext,
        _: SearchQuery,
    ) -> HostFuture<'a, Result<SearchResults, HostError>> {
        Box::pin(async { Ok(SearchResults::default()) })
    }
    fn read<'a>(
        &'a self,
        _: &'a CallContext,
        _: &'a FoundItem,
    ) -> HostFuture<'a, Result<PageText, HostError>> {
        Box::pin(async { Err(HostError::Failed("not used".into())) })
    }
    fn read_links<'a>(
        &'a self,
        ctx: &'a CallContext,
        url: &'a str,
    ) -> HostFuture<'a, Result<LinkedPage, HostError>> {
        Box::pin(async move {
            ctx.app.scope.check_domain(url).map_err(HostError::Denied)?;
            self.read.lock().unwrap().push(url.to_owned());
            let links = self
                .links
                .get(url)
                .ok_or_else(|| HostError::Failed(format!("404 {url}")))?;
            Ok(LinkedPage {
                final_url: url.to_owned(),
                page: PageText {
                    text: format!("The page at {url}."),
                    title: Some(url.to_owned()),
                },
                links: links.iter().map(|l| l.to_string()).collect(),
            })
        })
    }
}

fn app(folder: &std::path::Path, scope: Value) -> AppContext {
    AppContext::new("os.news", folder)
        .grant("crawl")
        .with_scope(scope::parse(&scope).unwrap())
}

#[tokio::test]
async fn a_crawl_stays_on_the_site_and_within_its_limits() {
    let site = Arc::new(Site {
        links: BTreeMap::from([
            (
                "https://example.org/news/",
                vec![
                    "https://example.org/news/a",
                    "https://example.org/about",
                    "https://other.example/news/x",
                    "https://bad.example.org/news/y",
                    "https://www.example.org/news/b",
                ],
            ),
            (
                "https://example.org/news/a",
                vec!["https://example.org/news/c"],
            ),
            ("https://www.example.org/news/b", vec![]),
            ("https://example.org/news/c", vec![]),
        ]),
        read: Mutex::new(Vec::new()),
    });
    let model = Arc::new(FakeModel::new(BTreeMap::new(), FakeModelConfig::default()));
    let toolbox = PeerToolbox::new(Library::builtin().unwrap(), site.clone(), model);
    let dir = common::temp_dir("crawl");
    let app = app(
        &dir,
        json!({"max_depth": 1, "max_pages": 10, "domains_deny": ["bad.example.org"]}),
    );
    let out = toolbox
        .call(
            &app,
            DEEP_CRAWL,
            json!({"url": "https://example.org/news/", "max_depth": 5, "path_prefix": "/news"}),
        )
        .await
        .unwrap();
    // Depth 1 (the grant's): the start and its on-site links under /news,
    // never another site or a denied subdomain; not `c` (depth 2).
    assert_eq!(
        *site.read.lock().unwrap(),
        [
            "https://example.org/news/",
            "https://example.org/news/a",
            "https://www.example.org/news/b"
        ]
    );
    assert_eq!(out["pages"].as_array().unwrap().len(), 3);
    let file = out["file"].as_str().unwrap();
    assert!(file.starts_with("research/crawl-"), "{file}");
    let saved: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join(file)).unwrap()).unwrap();
    assert_eq!(saved["max_depth"], 1);
    assert_eq!(
        saved["pages"][0]["text"],
        "The page at https://example.org/news/."
    );

    // The page limit holds.
    site.read.lock().unwrap().clear();
    let app = self::app(&dir, json!({"max_depth": 3, "max_pages": 2}));
    toolbox
        .call(
            &app,
            DEEP_CRAWL,
            json!({"url": "https://example.org/news/", "max_pages": 50}),
        )
        .await
        .unwrap();
    assert_eq!(site.read.lock().unwrap().len(), 2);

    // Without crawl limits, or outside the domains: refused before reading.
    site.read.lock().unwrap().clear();
    let none = AppContext::new("os.news", &dir).grant("crawl");
    let refused = toolbox
        .call(
            &none,
            DEEP_CRAWL,
            json!({"url": "https://example.org/news/"}),
        )
        .await
        .unwrap_err();
    assert_eq!(refused.kind, "not_granted");
    let narrow = self::app(
        &dir,
        json!({"max_depth": 1, "max_pages": 2, "domains_allow": ["example.org"]}),
    );
    let refused = toolbox
        .call(
            &narrow,
            DEEP_CRAWL,
            json!({"url": "https://other.example/"}),
        )
        .await
        .unwrap_err();
    assert_eq!(refused.kind, "denied");
    assert!(site.read.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
