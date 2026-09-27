//! A live smoke test of the interim `mod.research` adapter. Needs the network
//! and the `live` feature; not run in CI:
//!
//! `cargo test -p octosense-toolbox --features live --test live -- --ignored --nocapture`

#![cfg(feature = "live")]

mod common;

use octosense_toolbox::fixture::FakeModel;
use octosense_toolbox::research::live::{Feed, InterimResearch, LiveConfig};
use octosense_toolbox::research::ResearchHost;
use octosense_toolbox::{run, RunOptions, RunStatus};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
#[ignore = "live network"]
async fn news_digest_runs_against_live_sources() {
    // GDELT plus two publisher feeds (the experiment's BBC and Guardian).
    let backend = InterimResearch::new(LiveConfig {
        feeds: vec![
            Feed {
                url: "https://feeds.bbci.co.uk/news/technology/rss.xml".into(),
                name: "BBC News".into(),
                language: "en".into(),
            },
            Feed {
                url: "https://www.theguardian.com/uk/technology/rss".into(),
                name: "The Guardian".into(),
                language: "en".into(),
            },
        ],
        ..LiveConfig::default()
    })
    .unwrap();
    // The extractive stand-in model: this checks search, reading and
    // provenance, not writing.
    let host = ResearchHost::new(
        Arc::new(backend),
        Arc::new(FakeModel::new(Default::default(), Default::default())),
    );
    let folder = common::temp_dir("live");
    let result = run(
        &common::template("news-digest"),
        &common::app(&folder),
        json!({"topic": "OpenAI", "language": "en", "limit": 3, "max_age_hours": 72}),
        &host,
        RunOptions::default(),
    )
    .await
    .unwrap();
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
    assert_ne!(result.status, RunStatus::Failed, "{:?}", result.diagnostics);
    for source in result.data["sources"].as_array().unwrap() {
        assert!(result.provenance.iter().any(|p| p.url == source["url"]));
    }
}
