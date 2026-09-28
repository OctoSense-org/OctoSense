//! The runner: grants, parameters, budget, concurrency, partial results,
//! provenance, output validation and writing results to the app's folder.

mod common;

use common::{app, case, probe, temp_dir, template};
use octosense_toolbox::fixture::{self, FixtureData};
use octosense_toolbox::host::{CallContext, HostError, HostFuture, HostReply, Usage};
use octosense_toolbox::{
    run, scope, AppContext, Budget, ErrorKind, RunOptions, RunStatus, ToolboxHost,
};
use serde_json::{json, Value};
use std::cell::RefCell;

fn city() -> (
    octosense_toolbox::fixture::FixtureCase,
    octosense_toolbox::Template,
) {
    (
        case("news-digest", "city-infrastructure"),
        template("news-digest"),
    )
}

#[tokio::test]
async fn refuses_an_app_without_the_grant_and_invalid_params() {
    let (case, template) = city();
    let host = fixture::host(&case.fixture);
    let folder = temp_dir("grant");
    let ungranted = octosense_toolbox::AppContext::new("os.news", &folder);
    let err = run(
        &template,
        &ungranted,
        case.params.clone(),
        &host,
        RunOptions::default(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotGranted);

    let err = run(
        &template,
        &app(&folder),
        json!({"topic": 7}),
        &host,
        RunOptions::default(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Params);

    let err = run(
        &template,
        &app(&folder),
        case.params.clone(),
        &host,
        RunOptions {
            run_id: Some("../x".into()),
            write_result: false,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Params);
}

#[tokio::test]
async fn defaults_fill_optional_params() {
    let (case, template) = city();
    let host = fixture::host(&case.fixture);
    let folder = temp_dir("defaults");
    let result = run(
        &template,
        &app(&folder),
        json!({"topic": "city infrastructure", "language": "en"}),
        &host,
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Ready, "{:?}", result.diagnostics);
}

#[tokio::test]
async fn independent_calls_overlap_up_to_max_concurrency() {
    let (case, template) = city();
    let folder = temp_dir("concurrency");
    let parallel = run(
        &template,
        &app(&folder),
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(parallel.stats.peak_concurrency, 3);
    // Every article read starts after the search completes and before any
    // read completes.
    let t = &parallel.trace;
    let search_done = t
        .iter()
        .find(|e| e.event == "complete" && e.tool == "research.search")
        .unwrap()
        .ms;
    let starts: Vec<f64> = t
        .iter()
        .filter(|e| e.event == "start" && e.tool == "research.article")
        .map(|e| e.ms)
        .collect();
    let first_read_done = t
        .iter()
        .filter(|e| e.event == "complete" && e.tool == "research.article")
        .map(|e| e.ms)
        .fold(f64::MAX, f64::min);
    assert_eq!(starts.len(), 3);
    assert!(starts
        .iter()
        .all(|&s| s >= search_done && s < first_read_done));
    // The digest starts after every read completed.
    let digest_start = t
        .iter()
        .find(|e| e.event == "start" && e.tool == "research.digest")
        .unwrap()
        .ms;
    assert!(t
        .iter()
        .filter(|e| e.event == "complete" && e.tool == "research.article")
        .all(|e| e.ms <= digest_start));

    // The same run with the app's budget limiting concurrency to one: the
    // same data, sequential reads, slower.
    let serial_app = app(&folder).with_budget(Budget {
        max_concurrency: 1,
        ..template.manifest.budget
    });
    let serial = run(
        &template,
        &serial_app,
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(serial.stats.peak_concurrency, 1);
    assert_eq!(serial.data, parallel.data);
    // Reads of 120 + 60 + 90 ms: serial ≥ 270 ms of reads, parallel ≈ 120.
    assert!(serial.stats.elapsed_ms > parallel.stats.elapsed_ms + 100.0);
}

#[tokio::test]
async fn the_model_budget_is_enforced_and_leaves_a_partial_result() {
    let (case, template) = city();
    let folder = temp_dir("model-budget");
    let no_model = app(&folder).with_budget(Budget {
        max_model_calls: 0,
        ..template.manifest.budget
    });
    let result = run(
        &template,
        &no_model,
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Partial);
    assert!(result.data["digest"].is_null());
    assert_eq!(result.data["sources"].as_array().unwrap().len(), 3);
    assert_eq!(result.stats.model_calls, 0);
    assert_eq!(result.stats.denied, 1);
    assert!(result
        .trace
        .iter()
        .any(|e| e.event == "denied" && e.tool == "research.digest"));
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.contains("max_model_calls")));
}

#[tokio::test]
async fn the_page_and_call_budgets_are_enforced() {
    let (case, template) = city();
    let folder = temp_dir("page-budget");
    // The app's own read budget: two of the three articles. The search is
    // not charged to it, however many feeds it fetched.
    let scoped = app(&folder).with_budget(Budget {
        max_reads: 2,
        ..template.manifest.budget
    });
    let mut data = case.fixture.clone();
    data.searches[0].fetches = 5;
    let result = run(
        &template,
        &scoped,
        case.params.clone(),
        &fixture::host(&data),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Partial);
    assert_eq!(result.stats.reads, 2);
    assert_eq!(result.stats.search_fetches, 5);
    assert_eq!(result.stats.denied, 1);
    assert_eq!(result.data["missing"], 1);
    assert_eq!(result.data["sources"].as_array().unwrap().len(), 2);

    // The call limit: the search and two articles, then nothing.
    let limited = app(&folder).with_budget(Budget {
        max_calls: 3,
        ..template.manifest.budget
    });
    let result = run(
        &template,
        &limited,
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Partial);
    assert_eq!(result.stats.calls, 3);
    assert!(result.data["digest"].is_null());
    assert_eq!(result.data["missing"], 1);
}

#[tokio::test]
async fn the_time_budget_cancels_in_flight_calls() {
    let (mut case, template) = city();
    for page in case.fixture.pages.values_mut() {
        page.delay_ms = 10_000;
    }
    let folder = temp_dir("time-budget");
    let quick = app(&folder).with_budget(Budget {
        max_ms: 200,
        ..template.manifest.budget
    });
    let started = std::time::Instant::now();
    let result = run(
        &template,
        &quick,
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert!(started.elapsed().as_millis() < 2000);
    assert_eq!(result.status, RunStatus::Partial);
    assert_eq!(result.data["missing"], 3);
    assert_eq!(
        result
            .trace
            .iter()
            .filter(|e| e.event == "timed_out")
            .count(),
        3
    );
    assert!(result.diagnostics.iter().any(|d| d.contains("max_ms")));
}

#[tokio::test]
async fn a_required_failure_publishes_no_data() {
    let template = template("weather-plan");
    let case = case("weather-plan", "no-forecast");
    let folder = temp_dir("required");
    let result = run(
        &template,
        &app(&folder),
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Failed);
    assert!(result.data.is_null());
    assert!(result.provenance.is_empty());
}

#[tokio::test]
async fn provenance_is_host_kept_and_model_urls_are_refused() {
    let (case, template) = city();
    let folder = temp_dir("provenance");
    let result = run(
        &template,
        &app(&folder),
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.provenance.len(), 3);
    for (source, record) in result.data["sources"]
        .as_array()
        .unwrap()
        .iter()
        .zip(&result.provenance)
    {
        assert_eq!(source["id"], record.id.as_str());
        assert_eq!(source["url"], record.url.as_str());
        assert_eq!(
            source["evidence_sha256"],
            record.evidence_sha256.as_deref().unwrap()
        );
        assert_eq!(record.retrieved_at, "2026-09-20T08:00:00Z");
        assert_eq!(record.via, "article");
    }
    // Every citation names a read article.
    let ids: Vec<&str> = result.provenance.iter().map(|p| p.id.as_str()).collect();
    for point in result.data["digest"]["points"].as_array().unwrap() {
        for citation in point["citations"].as_array().unwrap() {
            assert!(ids.contains(&citation.as_str().unwrap()));
        }
    }

    // A summary carrying a URL is refused by the host: the digest is
    // dropped, the sources stay.
    let mut data = case.fixture.clone();
    data.model.inject_url = true;
    let result = run(
        &template,
        &app(&folder),
        case.params.clone(),
        &fixture::host(&data),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Partial);
    assert!(result.data["digest"].is_null());
    assert_eq!(result.data["sources"].as_array().unwrap().len(), 3);
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.contains("model output rejected: the summary contains a URL")));
    assert!(!serde_json::to_string(&result.data)
        .unwrap()
        .contains("model.invalid"));
}

#[tokio::test]
async fn one_bad_point_does_not_sink_the_digest() {
    // The live failure of 27 Sep 2026: one point without text used to
    // refuse the whole digest. Now invalid points are dropped with a
    // diagnostic, and the valid ones are kept.
    let (case, template) = city();
    let folder = temp_dir("bad-point");
    let good = run(
        &template,
        &app(&folder),
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    for model in [
        fixture::FakeModelConfig {
            malformed_points: true,
            ..Default::default()
        },
        fixture::FakeModelConfig {
            cite_unknown: true,
            ..Default::default()
        },
    ] {
        let mut data = case.fixture.clone();
        data.model = model.clone();
        let result = run(
            &template,
            &app(&folder),
            case.params.clone(),
            &fixture::host(&data),
            RunOptions::default(),
        )
        .await
        .unwrap();
        assert_eq!(result.status, RunStatus::Ready, "{:?}", result.diagnostics);
        // Exactly the valid points survive.
        assert_eq!(result.data["digest"], good.data["digest"], "{model:?}");
        let dropped: Vec<&String> = result
            .diagnostics
            .iter()
            .filter(|d| d.starts_with("research.digest") && d.contains("dropped"))
            .collect();
        let expected = if model.malformed_points { 3 } else { 1 };
        assert_eq!(dropped.len(), expected, "{:?}", result.diagnostics);
        let text = serde_json::to_string(&result).unwrap();
        assert!(!text.contains("model.invalid"));
        assert!(!text.contains("Invented."));
        // Every citation is an article read in this run.
        let read: Vec<&str> = result.provenance.iter().map(|p| p.id.as_str()).collect();
        for point in result.data["digest"]["points"].as_array().unwrap() {
            for citation in point["citations"].as_array().unwrap() {
                assert!(read.contains(&citation.as_str().unwrap()));
            }
        }
    }
}

#[tokio::test]
async fn news_digest_skips_results_the_backend_cannot_read() {
    // The live run of 27 Sep 2026 on "China" spent all three reads on
    // Google News links. The host lists readable results first; the
    // template counts the rest and does not read them.
    let (case, template) = city();
    let mut data = case.fixture.clone();
    data.searches[0].items[0].readable = false;
    let folder = temp_dir("unreadable");
    let result = run(
        &template,
        &app(&folder),
        case.params.clone(),
        &fixture::host(&data),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Partial);
    assert_eq!(result.data["unreadable"], 1);
    assert_eq!(result.data["missing"], 0);
    assert_eq!((result.stats.reads, result.stats.failed), (2, 0));
    assert_eq!(result.data["sources"].as_array().unwrap().len(), 2);
    assert!(result.data["digest"].is_object());
}

#[tokio::test]
async fn topic_brief_reads_every_language_that_has_a_readable_article() {
    // The live failure of 27 Sep 2026: two searches fetching four feeds
    // each used up the page budget, the zh results were all Google News links
    // (unreadable without a browser), and the brief rested on one en
    // article. Searches are no longer charged to the read budget, unreadable
    // results are skipped, and each language gets a read.
    let template = template("topic-brief");
    let case = case("topic-brief", "unreadable-skipped");
    let folder = temp_dir("languages");
    let result = run(
        &template,
        &app(&folder),
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Ready, "{:?}", result.diagnostics);
    assert_eq!(result.stats.search_fetches, 8);
    assert_eq!(result.stats.reads, 4);
    assert_eq!((result.stats.denied, result.stats.failed), (0, 0));
    for query in result.data["queries"].as_array().unwrap() {
        assert_eq!(query["read"], 2, "{query}");
        assert_eq!(query["unreadable"], 1, "{query}");
    }
    let languages: Vec<&str> = result.data["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["language"].as_str().unwrap())
        .collect();
    assert_eq!(languages, ["en", "zh", "en", "zh"]);

    // A failed read is replaced by that language's next candidate.
    let case = common::case("topic-brief", "failed-read-fallback");
    let result = run(
        &template,
        &app(&folder),
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    // Partial: a read failed, even though the fallback recovered.
    assert_eq!(result.status, RunStatus::Partial);
    assert_eq!(result.data["missing"], 1);
    assert_eq!(result.stats.reads, 3);
    let reads: Vec<u64> = result.data["queries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["read"].as_u64().unwrap())
        .collect();
    assert_eq!(reads, [1, 1]);
    assert!(result.data["brief"].is_object());

    // With no readable result in a language, it goes unread and the run
    // says so.
    let mut data = case.fixture.clone();
    for item in &mut data.searches[1].items {
        item.readable = false;
    }
    let result = run(
        &template,
        &app(&folder),
        case.params.clone(),
        &fixture::host(&data),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Partial);
    assert_eq!(result.stats.failed, 0);
    assert_eq!(result.data["queries"][1]["read"], 0);
    assert_eq!(result.data["queries"][1]["unreadable"], 3);
    assert_eq!(result.data["queries"][0]["read"], 2);
}

#[tokio::test]
async fn a_template_cannot_emit_a_url_the_host_did_not_retrieve() {
    let template = probe(
        &["search"],
        "use mod.research\nlet found = research.search({topic: \"city infrastructure\", language: \"en\"}).await()\n{status: \"ready\", data: {link: \"https://invented.example/story\", n: found.items}}\n",
    )
    .unwrap();
    let case = case("news-digest", "city-infrastructure");
    let folder = temp_dir("invented-url");
    let result = run(
        &template,
        &app(&folder),
        json!({}),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Failed);
    assert!(result.data.is_null());
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.contains("did not retrieve")));
}

#[tokio::test]
async fn articles_are_read_only_from_this_runs_search_results() {
    let template = probe(
        &["article"],
        "use mod.research\nlet a = try research.article({id: \"s0123456789ab\"}).await() catch nil\n{status: \"partial\", data: {read: a != nil}}\n",
    )
    .unwrap();
    let folder = temp_dir("ids");
    let result = run(
        &template,
        &app(&folder),
        json!({}),
        &fixture::host(&FixtureData::default()),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.data["read"], false);
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.contains("not a result of this run's searches")));
}

#[tokio::test]
async fn output_is_validated_against_the_schema() {
    let (case, _) = city();
    let mut manifest = common::manifest(&["search"]);
    manifest["output"] = json!({"type": "object", "additionalProperties": false,
        "required": ["count"], "properties": {"count": {"type": "integer"}}});
    let bad = octosense_toolbox::library::Template::from_parts(
        &manifest.to_string(),
        "use mod.research\n{status: \"ready\", data: {count: \"three\"}}\n",
        octosense_toolbox::TemplateOrigin::Library,
    )
    .unwrap();
    let folder = temp_dir("schema");
    let host = fixture::host(&case.fixture);
    let result = run(&bad, &app(&folder), json!({}), &host, RunOptions::default())
        .await
        .unwrap();
    assert_eq!(result.status, RunStatus::Failed);
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.contains("output schema")));

    let wrong_envelope = common::probe(
        &["search"],
        "use mod.research\n{status: \"done\", data: {}}\n",
    )
    .unwrap();
    let result = run(
        &wrong_envelope,
        &app(&folder),
        json!({}),
        &host,
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Failed);
}

#[tokio::test]
async fn the_apps_scope_filters_and_refuses() {
    let (case, template) = city();
    let folder = temp_dir("scope");
    // The grant is octos's `Scope`, parsed by `Scope::from_grant`.
    let scoped = app(&folder)
        .with_scope(scope::parse(&json!({"domains_deny": ["example.invalid"]})).unwrap());
    let result = run(
        &template,
        &scoped,
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    // Every result was on a denied domain: nothing found, nothing read.
    assert_eq!(result.status, RunStatus::Partial);
    assert!(result.data["sources"].as_array().unwrap().is_empty());
    assert!(result.provenance.is_empty());

    let spanish_only = app(&folder).with_scope(scope::parse(&json!({"langs": ["es"]})).unwrap());
    let result = run(
        &template,
        &spanish_only,
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    // An English search is outside the scope: the required search fails.
    assert_eq!(result.status, RunStatus::Failed);
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.contains("language en is not in this app's research grant")));

    // `max_results` caps what one search gives the app, with a note.
    let two_results = app(&folder).with_scope(scope::parse(&json!({"max_results": 2})).unwrap());
    let result = run(
        &template,
        &two_results,
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.data["sources"].as_array().unwrap().len(), 2);
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.contains("count clamped to this app's limit of 2")));
}

#[test]
fn an_app_context_carries_the_grant_in_octos_shape_only() {
    let context: AppContext = serde_json::from_value(json!({
        "app_id": "os.news", "grants": ["research"], "folder": "/tmp/os.news",
        "scope": {"langs": ["en", "zh-hant"], "max_age_days": 3, "max_depth": 0, "max_pages": 0}
    }))
    .unwrap();
    assert_eq!(context.scope.langs, ["en", "zh-Hant"]);
    assert_eq!(context.scope.max_results, 20);
    // No scope: unrestricted, with octos's default results per search.
    let context: AppContext = serde_json::from_value(
        json!({"app_id": "os.news", "grants": [], "folder": "/tmp/os.news"}),
    )
    .unwrap();
    assert_eq!(context.scope, scope::unrestricted());
    // The old toolbox shape is refused, naming the new fields.
    let error = serde_json::from_value::<AppContext>(json!({
        "app_id": "os.news", "grants": ["research"], "folder": "/tmp/os.news",
        "scope": {"languages": ["en"], "allowed_domains": ["example.org"], "recency_hours": 24}
    }))
    .unwrap_err()
    .to_string();
    assert!(error.contains("old toolbox shape"), "{error}");
    assert!(
        error.contains("`allowed_domains` is now `domains_allow`"),
        "{error}"
    );
    // So is any field octos's scope does not have.
    assert!(serde_json::from_value::<AppContext>(json!({
        "app_id": "os.news", "grants": [], "folder": "/tmp/os.news", "scope": {"depth": 1}
    }))
    .is_err());
}

/// Records the context every call carries.
struct Recorder {
    seen: RefCell<Vec<(String, String, u32, u32)>>,
}

impl ToolboxHost for Recorder {
    fn call<'a>(
        &'a self,
        ctx: CallContext,
        module: &'a str,
        method: &'a str,
        _input: Value,
    ) -> HostFuture<'a, Result<HostReply, HostError>> {
        self.seen.borrow_mut().push((
            ctx.app.app_id.clone(),
            format!("{module}.{method}"),
            ctx.remaining.calls,
            ctx.budget.max_calls,
        ));
        Box::pin(async move {
            Ok(HostReply {
                output: json!({"items": [], "source": {"partial": false, "providers": ["rec"], "queried_at": "t"}}),
                provenance: Vec::new(),
                usage: Usage {
                    model_calls: 0,
                    fetches: 1,
                },
                notes: Vec::new(),
            })
        })
    }
}

#[tokio::test]
async fn every_call_carries_the_apps_identity_and_budget() {
    let template = probe(
        &["search"],
        "use mod.research\nlet a = research.search({topic: \"a\"})\nlet b = research.search({topic: \"b\"})\nlet x = a.await()\nlet y = b.await()\n{status: \"ready\", data: {}}\n",
    )
    .unwrap();
    let host = Recorder {
        seen: RefCell::new(Vec::new()),
    };
    let folder = temp_dir("identity");
    let result = run(
        &template,
        &app(&folder),
        json!({}),
        &host,
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Ready);
    let seen = host.seen.into_inner();
    assert_eq!(
        seen,
        vec![
            ("os.news".into(), "research.search".into(), 8, 8),
            ("os.news".into(), "research.search".into(), 7, 8),
        ]
    );
}

#[tokio::test]
async fn results_are_written_to_the_apps_folder() {
    let (case, template) = city();
    let folder = temp_dir("write");
    let result = run(
        &template,
        &app(&folder),
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions {
            run_id: Some("morning".into()),
            write_result: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        result.result_path.as_deref(),
        Some("toolbox/runs/news-digest/morning.json")
    );
    let written: Value = serde_json::from_str(
        &std::fs::read_to_string(folder.join("toolbox/runs/news-digest/morning.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(written["status"], "ready");
    assert_eq!(written["data"], result.data);
    assert_eq!(written["provenance"].as_array().unwrap().len(), 3);
    let _ = std::fs::remove_dir_all(folder);
}

#[tokio::test]
async fn provider_failures_and_off_topic_reads_reach_the_diagnostics() {
    let case = case("news-digest", "off-topic-dropped");
    let host = fixture::host(&case.fixture);
    let folder = temp_dir("diagnostics");
    let result = run(
        &template("news-digest"),
        &app(&folder),
        case.params.clone(),
        &host,
        RunOptions::default(),
    )
    .await
    .unwrap();
    let diagnostics = result.diagnostics.join("\n");
    // The failing provider is named, with the reason.
    assert!(
        diagnostics.contains(
            "research.search (call 0): gdelt rate-limited (429); not queried again in this process; results from other sources"
        ),
        "{diagnostics}"
    );
    // Each page the relevance gate dropped, and the model's own check.
    assert_eq!(
        result
            .diagnostics
            .iter()
            .filter(|d| d.contains("off topic, the page does not mention strait + hormuz"))
            .count(),
        2,
        "{diagnostics}"
    );
    assert!(
        diagnostics.contains("digest: the model marked 1 of 2 articles off topic"),
        "{diagnostics}"
    );
    assert!(
        diagnostics.contains("dropped: it cites only articles marked off topic"),
        "{diagnostics}"
    );
    assert_eq!(result.data["off_topic"], 3);
}

#[tokio::test]
async fn the_host_leaves_off_topic_articles_out_of_any_digest() {
    // A template that ignores `on_topic` still cannot digest an off-topic page.
    let case = case("news-digest", "off-topic-dropped");
    let host = fixture::host(&case.fixture);
    let folder = temp_dir("gate");
    let source = r#"
use mod.research
let found = research.search({topic: "Strait of Hormuz", language: "en", limit: 4}).await()
let ids = []
let flags = []
for item in found.items {
    let article = research.article({id: item.id}).await()
    array.push(ids, article.id)
    array.push(flags, article.on_topic)
}
let only_off = []
for item in found.items {
    if item.title == "Pope Leo warns of a 'paradise of machines'" {
        array.push(only_off, item.id)
    }
}
let refused = try research.digest({task: "brief", language: "en", article_ids: only_off}).await() catch nil
let digest = research.digest({task: "brief", language: "en", article_ids: ids}).await()
{status: "ready", data: {flags: flags, refused: refused == nil, cited: digest.points}}
"#;
    let source = format!("use mod.std.array\n{source}");
    let template = probe(&["search", "article", "digest"], &source).unwrap();
    let result = run(
        &template,
        &app(&folder),
        json!({}),
        &host,
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.data["refused"], true, "{:?}", result.diagnostics);
    let flags: Vec<bool> = result.data["flags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_bool().unwrap())
        .collect();
    assert_eq!(flags.iter().filter(|f| !**f).count(), 2, "{flags:?}");
    // Only the two pages that mention the topic are cited.
    let cited: Vec<&str> = result.data["cited"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["citations"].as_array().unwrap())
        .map(|c| c.as_str().unwrap())
        .collect();
    assert_eq!(cited.len(), 2, "{cited:?}");
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.contains("no article given is about the topic")));
}
