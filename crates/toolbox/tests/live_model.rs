//! Templates with a real model: a live backend for sources and DeepSeek
//! (`deepseek-v4-flash`, OpenAI-compatible chat API) for `query` and
//! `digest`. Needs the network, a key, and the `live` feature (the interim
//! adapter) or the `octos-engine` feature (the octos research engine with
//! headless Chrome; used when both are on, unless `LIVE_BACKEND=interim`).
//! Not run in CI. The key is read only from `DEEPSEEK_API_KEY`, and the
//! tests skip when it is unset:
//!
//! `DEEPSEEK_API_KEY=… cargo test -p octosense-toolbox --features live --test live_model -- --ignored --nocapture --test-threads=1`
//! `DEEPSEEK_API_KEY=… cargo test -p octosense-toolbox --features octos-engine --test live_model -- --ignored --nocapture --test-threads=1`
//!
//! `c_validation_topics` repeats the 27 Sep 2026 validation's toolbox runs
//! (six topics, both research templates, twelve runs); set `LIVE_OUT` to a
//! directory to keep each run's result, the pages read (first 600 bytes of
//! evidence) and the model's token usage as JSON.

#![cfg(any(feature = "live", feature = "octos-engine"))]

mod common;

use octosense_toolbox::host::{CallContext, HostError, HostFuture};
use octosense_toolbox::research::{
    cap_evidence, item_id, Feed, FoundItem, ModelClient, ModelRequest, PageText, ResearchBackend,
    ResearchHost, SearchQuery, SearchResults, MAX_EVIDENCE_BYTES,
};
use octosense_toolbox::{run, RunOptions, RunResult, RunStatus};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const MODEL: &str = "deepseek-v4-flash";

/// A `ModelClient` over DeepSeek's chat completions API. It sets no output
/// cap: the host sets none (`max_output_tokens` is `None`), and a reasoning
/// model spends part of any cap on thinking.
struct DeepSeek {
    http: reqwest::Client,
    key: String,
    calls: AtomicU32,
    /// Prompt tokens (cache hits, misses) and completion tokens so far.
    usage: [AtomicU64; 3],
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
            let usage = &reply["usage"];
            for (slot, field) in self.usage.iter().zip([
                "prompt_cache_hit_tokens",
                "prompt_cache_miss_tokens",
                "completion_tokens",
            ]) {
                slot.fetch_add(usage[field].as_u64().unwrap_or(0), Ordering::Relaxed);
            }
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

fn key() -> Option<String> {
    let key = std::env::var("DEEPSEEK_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty());
    if key.is_none() {
        eprintln!("DEEPSEEK_API_KEY is not set; skipping");
    }
    key
}

/// The publisher feeds the interim adapter uses.
#[cfg_attr(not(feature = "live"), allow(dead_code))]
fn feeds() -> Vec<Feed> {
    let feed = |url: &str, name: &str, language: &str| Feed {
        url: url.into(),
        name: name.into(),
        language: language.into(),
    };
    vec![
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
    ]
}

/// The octos research engine, unless
/// `LIVE_BACKEND=interim` (or only the `live` feature is on).
#[cfg(feature = "octos-engine")]
fn octos_backend() -> Option<Arc<dyn ResearchBackend>> {
    use octosense_toolbox::research::octos::{OctosConfig, OctosResearch};
    if std::env::var("LIVE_BACKEND").as_deref() == Ok("interim") {
        return None;
    }
    // The metasearch's own publisher-feeds engine replaces the feeds the
    // interim adapter is given.
    let backend = OctosResearch::new(OctosConfig::from_env());
    eprintln!(
        "backend: octos research engine (renders: {})",
        backend.renders()
    );
    Some(Arc::new(backend))
}

#[cfg(not(feature = "octos-engine"))]
fn octos_backend() -> Option<Arc<dyn ResearchBackend>> {
    None
}

/// The interim adapter with the feeds these tests use.
#[cfg(feature = "live")]
fn interim_backend() -> Arc<dyn ResearchBackend> {
    use octosense_toolbox::research::live::{InterimResearch, LiveConfig};
    eprintln!("backend: interim adapter");
    Arc::new(
        InterimResearch::new(LiveConfig {
            feeds: feeds(),
            ..LiveConfig::from_env()
        })
        .unwrap(),
    )
}

#[cfg(not(feature = "live"))]
fn interim_backend() -> Arc<dyn ResearchBackend> {
    panic!("LIVE_BACKEND=interim needs the `live` feature")
}

fn backend() -> Arc<dyn ResearchBackend> {
    octos_backend().unwrap_or_else(interim_backend)
}

fn deepseek(key: String) -> DeepSeek {
    DeepSeek {
        http: reqwest::Client::new(),
        key,
        calls: AtomicU32::new(0),
        usage: Default::default(),
    }
}

/// The host, or `None` (the test skips) without `DEEPSEEK_API_KEY`.
fn host() -> Option<ResearchHost> {
    let key = key()?;
    Some(ResearchHost::new(backend(), Arc::new(deepseek(key))))
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

/// Records what the backend read, so a person can judge each source.
struct Recording {
    inner: Arc<dyn ResearchBackend>,
    reads: Mutex<Vec<Value>>,
}

impl ResearchBackend for Recording {
    fn search<'a>(
        &'a self,
        ctx: &'a CallContext,
        query: SearchQuery,
    ) -> HostFuture<'a, Result<SearchResults, HostError>> {
        self.inner.search(ctx, query)
    }

    fn read<'a>(
        &'a self,
        ctx: &'a CallContext,
        item: &'a FoundItem,
    ) -> HostFuture<'a, Result<PageText, HostError>> {
        Box::pin(async move {
            let result = self.inner.read(ctx, item).await;
            let entry = match &result {
                Ok(page) => {
                    let (evidence, _) = cap_evidence(&page.text, MAX_EVIDENCE_BYTES);
                    let mut end = evidence.len().min(600);
                    while !evidence.is_char_boundary(end) {
                        end -= 1;
                    }
                    json!({"id": item_id(&item.url), "url": item.url, "title": item.title,
                        "language": item.language, "evidence": &evidence[..end]})
                }
                Err(e) => json!({"id": item_id(&item.url), "url": item.url, "title": item.title,
                    "error": e.to_string()}),
            };
            self.reads.lock().unwrap().push(entry);
            result
        })
    }
}

#[tokio::test]
#[ignore = "live network and model; needs DEEPSEEK_API_KEY"]
async fn c_validation_topics() {
    let Some(key) = key() else { return };
    let out = std::env::var_os("LIVE_OUT").map(std::path::PathBuf::from);
    let model = Arc::new(deepseek(key));
    // Topic, its language, and the second language topic-brief also searches.
    let topics = [
        ("tb-openai", "OpenAI", "en", Some("zh")),
        ("tb-nvidia", "Nvidia earnings", "en", None),
        ("tb-hormuz", "Strait of Hormuz", "en", Some("zh")),
        ("tb-euaiact", "EU AI Act", "en", None),
        ("tb-typhoon", "台风", "zh", Some("en")),
        ("tb-vucic", "Vucic resignation", "en", None),
    ];
    let mut runs = Vec::new();
    for (label, topic, language, second) in topics {
        let digest = json!({"topic": topic, "language": language, "search_language": language,
            "limit": 5, "max_age_hours": 72});
        let mut languages = vec![json!({"language": language, "translate": false})];
        if let Some(second) = second {
            languages.push(json!({"language": second, "translate": true}));
        }
        let read_top = if second.is_some() { 5 } else { 4 };
        let brief = json!({"topic": topic, "language": language, "languages": languages,
            "per_language": 3, "read_top": read_top, "max_age_hours": 72});
        runs.push((label.replacen("tb-", "nd-", 1), "news-digest", digest));
        runs.push((label.to_owned(), "topic-brief", brief));
    }
    let started_all = Instant::now();
    for (label, template, params) in runs {
        // `ResearchHost` takes an `Arc`; the backend trait asks for neither
        // `Send` nor `Sync`, and the run stays on this thread.
        #[allow(clippy::arc_with_non_send_sync)]
        let backend = Arc::new(Recording {
            inner: backend(),
            reads: Mutex::new(Vec::new()),
        });
        let host = ResearchHost::new(backend.clone(), model.clone());
        let folder = common::temp_dir("live-validation");
        let started = Instant::now();
        let result = run(
            &common::template(template),
            &common::app(&folder),
            params.clone(),
            &host,
            RunOptions::default(),
        )
        .await
        .unwrap();
        let wall = started.elapsed().as_secs_f64();
        let _ = std::fs::remove_dir_all(&folder);
        let sources = result.data["sources"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let languages: std::collections::BTreeSet<String> = sources
            .iter()
            .filter_map(|s| s["language"].as_str().map(str::to_owned))
            .collect();
        println!(
            "\n{label}: {:?} {wall:.1}s kept={} languages={languages:?} off_topic={} queries={}",
            result.status,
            sources.len(),
            result.data["off_topic"],
            result.data["queries"]
        );
        for source in &sources {
            println!("  kept: [{}] {}", source["language"], source["title"]);
        }
        for d in &result.diagnostics {
            println!("  diagnostic: {d}");
        }
        if let Some(out) = &out {
            std::fs::create_dir_all(out).unwrap();
            let record = json!({"label": label, "template": template, "params": params,
                "wall_s": wall, "result": result,
                "reads": *backend.reads.lock().unwrap()});
            std::fs::write(
                out.join(format!("{label}.json")),
                serde_json::to_string_pretty(&record).unwrap(),
            )
            .unwrap();
        }
    }
    let [hit, miss, completion] = &model.usage;
    let (hit, miss, completion) = (
        hit.load(Ordering::Relaxed),
        miss.load(Ordering::Relaxed),
        completion.load(Ordering::Relaxed),
    );
    // DeepSeek's off-peak flash prices per million tokens (27 Sep 2026).
    let usd = (hit as f64 * 0.003 + miss as f64 * 0.15 + completion as f64 * 0.60) / 1e6;
    println!(
        "\nall runs: {:.1}s, {} model calls, tokens cached {hit} / uncached {miss} / out {completion}, about ${usd:.4}",
        started_all.elapsed().as_secs_f64(),
        model.calls.load(Ordering::Relaxed)
    );
}
