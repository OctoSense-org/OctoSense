//! Templates with a real model: the interim live adapter for sources and
//! DeepSeek (`deepseek-v4-flash`, OpenAI-compatible chat API) for `query`
//! and `digest`. Needs the network, the `live` feature and a key; not run in
//! CI. The key is read only from `DEEPSEEK_API_KEY`, and the tests skip
//! when it is unset:
//!
//! `DEEPSEEK_API_KEY=… cargo test -p octosense-toolbox --features live --test live_model -- --ignored --nocapture --test-threads=1`

#![cfg(feature = "live")]

mod common;

use octosense_toolbox::host::{CallContext, HostError, HostFuture};
use octosense_toolbox::research::live::{Feed, InterimResearch, LiveConfig};
use octosense_toolbox::research::{ModelClient, ModelRequest, ResearchHost};
use octosense_toolbox::{run, RunOptions, RunResult, RunStatus};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

const MODEL: &str = "deepseek-v4-flash";

/// A `ModelClient` over DeepSeek's chat completions API. It sets no output
/// cap: the host sets none (`max_output_tokens` is `None`), and a reasoning
/// model spends part of any cap on thinking.
struct DeepSeek {
    http: reqwest::Client,
    key: String,
    calls: AtomicU32,
}

impl ModelClient for DeepSeek {
    fn complete<'a>(
        &'a self,
        _ctx: &'a CallContext,
        request: ModelRequest,
    ) -> HostFuture<'a, Result<String, HostError>> {
        Box::pin(async move {
            let n = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
            let started = Instant::now();
            let system = format!(
                "{}\n\nReply with one JSON object matching this JSON Schema, and nothing else:\n{}",
                request.system, request.output_schema
            );
            let mut body = json!({
                "model": MODEL,
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": request.user}
                ],
                "response_format": {"type": "json_object"},
                "temperature": 0.2
            });
            if let Some(max) = request.max_output_tokens {
                body["max_tokens"] = json!(max);
            }
            let response = self
                .http
                .post("https://api.deepseek.com/chat/completions")
                .bearer_auth(&self.key)
                .json(&body)
                .send()
                .await
                .map_err(|e| HostError::Failed(format!("deepseek: {e}")))?;
            let status = response.status();
            let reply: Value = response
                .json()
                .await
                .map_err(|e| HostError::Failed(format!("deepseek: {e}")))?;
            if !status.is_success() {
                return Err(HostError::Failed(format!(
                    "deepseek: HTTP {status}: {}",
                    reply["error"]["message"]
                )));
            }
            let text = reply["choices"][0]["message"]["content"]
                .as_str()
                .ok_or_else(|| HostError::Failed("deepseek: no content".into()))?
                .to_owned();
            eprintln!(
                "[model call {n}] {:?} {:.1}s in={} out={} reasoning={} finish={}",
                request.task,
                started.elapsed().as_secs_f64(),
                reply["usage"]["prompt_tokens"],
                reply["usage"]["completion_tokens"],
                reply["usage"]["completion_tokens_details"]["reasoning_tokens"],
                reply["choices"][0]["finish_reason"],
            );
            Ok(text)
        })
    }
}

/// The host, or `None` (the test skips) without `DEEPSEEK_API_KEY`.
fn host() -> Option<ResearchHost> {
    let key = std::env::var("DEEPSEEK_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty());
    let Some(key) = key else {
        eprintln!("DEEPSEEK_API_KEY is not set; skipping");
        return None;
    };
    let feed = |url: &str, name: &str, language: &str| Feed {
        url: url.into(),
        name: name.into(),
        language: language.into(),
    };
    let backend = InterimResearch::new(LiveConfig {
        feeds: vec![
            feed(
                "https://feeds.bbci.co.uk/news/technology/rss.xml",
                "BBC News",
                "en",
            ),
            feed(
                "https://www.theguardian.com/uk/technology/rss",
                "The Guardian",
                "en",
            ),
            feed(
                "https://feeds.bbci.co.uk/zhongwen/simp/rss.xml",
                "BBC 中文",
                "zh",
            ),
        ],
        ..LiveConfig::from_env()
    })
    .unwrap();
    let model = DeepSeek {
        http: reqwest::Client::new(),
        key,
        calls: AtomicU32::new(0),
    };
    Some(ResearchHost::new(Arc::new(backend), Arc::new(model)))
}

fn topic() -> String {
    std::env::var("LIVE_TOPIC").unwrap_or_else(|_| "OpenAI".into())
}

async fn go(host: &ResearchHost, template: &str, params: Value) -> RunResult {
    let folder = common::temp_dir("live-model");
    let started = Instant::now();
    let result = run(
        &common::template(template),
        &common::app(&folder),
        params,
        host,
        RunOptions::default(),
    )
    .await
    .unwrap();
    println!(
        "\n===== {template} ({:.1}s wall) =====",
        started.elapsed().as_secs_f64()
    );
    println!("status: {:?}", result.status);
    println!("stats: {}", serde_json::to_string(&result.stats).unwrap());
    println!("diagnostics: {:#?}", result.diagnostics);
    println!("{}", serde_json::to_string_pretty(&result.data).unwrap());
    assert_ne!(result.status, RunStatus::Failed, "{:?}", result.diagnostics);
    // Every citation is an article the host read in this run.
    let read: Vec<&str> = result
        .provenance
        .iter()
        .filter(|p| p.evidence_sha256.is_some())
        .map(|p| p.id.as_str())
        .collect();
    for digest in [&result.data["digest"], &result.data["brief"]] {
        for point in digest["points"].as_array().into_iter().flatten() {
            for citation in point["citations"].as_array().unwrap() {
                assert!(read.contains(&citation.as_str().unwrap()), "{citation}");
            }
        }
    }
    result
}

#[tokio::test]
#[ignore = "live network and model; needs DEEPSEEK_API_KEY"]
async fn a_news_digest_en() {
    let Some(host) = host() else { return };
    go(
        &host,
        "news-digest",
        json!({"topic": topic(), "language": "en", "limit": 3, "max_age_hours": 72}),
    )
    .await;
}

#[tokio::test]
#[ignore = "live network and model; needs DEEPSEEK_API_KEY"]
async fn b_topic_brief_en_zh() {
    let Some(host) = host() else { return };
    let result = go(
        &host,
        "topic-brief",
        json!({
            "topic": topic(),
            "language": "en",
            "languages": [{"language": "en", "translate": false}, {"language": "zh", "translate": true}],
            "per_language": 3, "read_top": 4, "max_age_hours": 72
        }),
    )
    .await;
    assert_eq!(result.stats.denied, 0, "{:?}", result.diagnostics);
    for query in result.data["queries"].as_array().unwrap() {
        println!(
            "language {}: terms {:?}, found {}, unreadable {}, read {}",
            query["language"], query["terms"], query["found"], query["unreadable"], query["read"]
        );
    }
}
