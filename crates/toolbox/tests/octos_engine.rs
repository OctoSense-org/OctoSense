//! The octos research engine backend (feature `octos-engine`).
//!
//! The browser tests start headless Chrome (no network, no window) and skip
//! when no Chrome is installed or it cannot start here. The live test needs
//! the network:
//!
//! `cargo test -p octosense-toolbox --features octos-engine --test octos_engine -- --include-ignored --nocapture`

#![cfg(feature = "octos-engine")]

mod common;

use octosense_toolbox::fixture::FakeModel;
use octosense_toolbox::research::chrome::{find_binary, process_alive, Chrome, ChromeConfig};
use octosense_toolbox::research::octos::{OctosConfig, OctosResearch};
use octosense_toolbox::research::ResearchHost;
use octosense_toolbox::{run, RunOptions, RunStatus};
use serde_json::json;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

/// A page whose script never yields, so the load never completes.
const HANG: &str = "data:text/html,<p>busy</p><script>while(true){}</script>";

fn chrome(config: ChromeConfig) -> Option<Arc<Chrome>> {
    if find_binary().is_none() {
        eprintln!("no Chrome here; skipping");
        return None;
    }
    Chrome::new(config)
}

/// Processes still in Chrome's process group.
fn group_members(pgid: u32) -> Vec<String> {
    let out = Command::new("pgrep")
        .args(["-g", &pgid.to_string()])
        .output()
        .expect("pgrep");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Starts the browser with one render; `None` (skip) when Chrome cannot
/// start on this machine.
async fn started(chrome: &Arc<Chrome>) -> Option<u32> {
    match chrome.render("data:text/html,<p>hello</p>").await {
        Err(e) if e.contains("start") || e.contains("DevTools port") => {
            eprintln!("Chrome did not start here ({e}); skipping");
            None
        }
        _ => chrome.pid().await,
    }
}

#[tokio::test]
async fn dropping_the_browser_kills_its_whole_process_group() {
    let Some(chrome) = chrome(ChromeConfig::default()) else {
        return;
    };
    let Some(pid) = started(&chrome).await else {
        return;
    };
    assert!(process_alive(pid));
    assert!(!group_members(pid).is_empty());
    drop(chrome);
    // The guard kills synchronously; allow the kernel a moment to reap.
    for _ in 0..20 {
        if group_members(pid).is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(!process_alive(pid), "Chrome {pid} outlived its guard");
    assert!(
        group_members(pid).is_empty(),
        "helpers left: {:?}",
        group_members(pid)
    );
}

#[tokio::test]
async fn a_render_that_hangs_times_out_and_the_browser_stays_usable() {
    let Some(chrome) = chrome(ChromeConfig {
        render_timeout: Duration::from_secs(4),
        ..ChromeConfig::default()
    }) else {
        return;
    };
    let Some(pid) = started(&chrome).await else {
        return;
    };
    let started_at = std::time::Instant::now();
    let error = chrome.render(HANG).await.unwrap_err();
    assert!(error.contains("timed out"), "{error}");
    assert!(started_at.elapsed() < Duration::from_secs(8));
    // The same browser serves the next render; nothing was relaunched.
    let _ = chrome.render("data:text/html,<p>again</p>").await;
    assert_eq!(chrome.pid().await, Some(pid));
    assert_eq!(chrome.launches(), 1);
    chrome.shutdown().await;
    assert!(!process_alive(pid));
}

#[tokio::test]
async fn an_idle_browser_is_closed() {
    let Some(chrome) = chrome(ChromeConfig {
        idle_close: Duration::from_millis(500),
        ..ChromeConfig::default()
    }) else {
        return;
    };
    let Some(pid) = started(&chrome).await else {
        return;
    };
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(chrome.pid().await, None);
    assert!(!process_alive(pid));
    // A later render starts a new one.
    let _ = chrome.render("data:text/html,<p>back</p>").await;
    assert_eq!(chrome.launches(), 2);
}

#[tokio::test]
async fn concurrent_renders_share_one_browser() {
    let Some(chrome) = chrome(ChromeConfig {
        max_tabs: 2,
        ..ChromeConfig::default()
    }) else {
        return;
    };
    if started(&chrome).await.is_none() {
        return;
    }
    let renders = (0..4).map(|i| {
        let chrome = chrome.clone();
        async move {
            chrome
                .render(&format!("data:text/html,<p>page {i}</p>"))
                .await
        }
    });
    futures_util::future::join_all(renders).await;
    assert_eq!(chrome.launches(), 1);
}

#[tokio::test]
async fn the_browser_refuses_private_addresses() {
    let Some(chrome) = chrome(ChromeConfig::default()) else {
        return;
    };
    if started(&chrome).await.is_none() {
        return;
    }
    for url in [
        "http://127.0.0.1:9/",
        "http://169.254.169.254/latest/meta-data/",
        // A page that sends the browser to the metadata address.
        "data:text/html,<script>location.href='http://169.254.169.254/latest/meta-data/'</script>",
    ] {
        let error = chrome.render(url).await.unwrap_err();
        assert!(
            error.contains("ssrf_blocked")
                || error.contains("navigation failed")
                || error.contains("BlockedByClient"),
            "{url}: {error}"
        );
    }
}

#[tokio::test]
#[ignore = "live network"]
async fn news_digest_reads_google_news_links_through_the_browser() {
    let backend = OctosResearch::new(OctosConfig::from_env());
    if !backend.renders() {
        eprintln!("no Chrome here; skipping");
        return;
    }
    let host = ResearchHost::new(
        Arc::new(backend),
        Arc::new(FakeModel::new(Default::default(), Default::default())),
    );
    let folder = common::temp_dir("octos-engine");
    let result = run(
        &common::template("news-digest"),
        &common::app(&folder),
        json!({"topic": "台风", "language": "zh", "search_language": "zh", "limit": 3,
            "max_age_hours": 72}),
        &host,
        RunOptions::default(),
    )
    .await
    .unwrap();
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
    assert_ne!(result.status, RunStatus::Failed, "{:?}", result.diagnostics);
    let sources = result.data["sources"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(!sources.is_empty(), "{:?}", result.diagnostics);
}
