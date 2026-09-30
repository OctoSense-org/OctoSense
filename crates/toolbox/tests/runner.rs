//! The runner: grants, parameters, budget, concurrency, partial results and
//! the status rules, provenance, output validation and writing results to
//! the app's folder.

mod common;

use common::{app, case, probe, temp_dir, template};
use octosense_toolbox::fixture::{self, FakeModel, FakeModelConfig, FixtureData};
use octosense_toolbox::host::{CallContext, HostError, HostFuture, HostReply, Usage};
use octosense_toolbox::research::{
    FoundItem, PageText, ResearchBackend, ResearchHost, SearchQuery, SearchResults,
};
use octosense_toolbox::{
    run, scope, AppContext, Budget, ErrorKind, RunOptions, RunStatus, ToolboxHost,
};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

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
    assert_eq!(
        result.status_reasons,
        [
            "the digest failed; the sources are listed without a digest",
            "the budget refused 1 call(s); see diagnostics"
        ]
    );
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
    // The digest of two articles is complete, but the budget cut the run
    // short: the runner says so.
    assert_eq!(result.status, RunStatus::Partial);
    assert_eq!(
        result.status_reasons,
        ["the budget refused 1 call(s); see diagnostics"]
    );
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
    // No model calls: no time is kept back from the reads (see
    // `a_slow_read_leaves_time_for_the_digest`), so the run's end cuts them.
    let quick = app(&folder).with_budget(Budget {
        max_ms: 200,
        max_model_calls: 0,
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
    // Every read was cancelled: nothing usable came back.
    assert_eq!(result.status, RunStatus::Failed);
    assert!(result.data.is_null());
    assert_eq!(
        result.status_reasons,
        [
            "no article on the topic was read (3 failed, 0 unreadable, 0 off topic)",
            "the time budget (200 ms) ran out; in-flight calls were cancelled"
        ]
    );
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
async fn a_slow_read_leaves_time_for_the_digest() {
    // The phone run of 29 Sep 2026: two reads waited 45-60 s on a WebView
    // render, the 90 s run ran out and the digest never ran. Reads now stop
    // when the time kept for the model begins.
    let template = template("topic-brief");
    let mut case = case("topic-brief", "failed-read-fallback");
    for page in case.fixture.pages.values_mut() {
        page.delay_ms = 60_000;
    }
    let folder = temp_dir("read-window");
    let budget = Budget {
        max_ms: 2_000,
        ..template.manifest.budget
    };
    assert_eq!(octosense_toolbox::runner::read_window_ms(&budget), 1_500);
    let started = std::time::Instant::now();
    let result = run(
        &template,
        &app(&folder).with_budget(budget),
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert!(started.elapsed().as_millis() < 2_000);
    // Every read was given up at the window's close, then the run ended on
    // its own (the template has nothing to digest), not by the time budget.
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.contains("read given up after")));
    assert!(!result
        .status_reasons
        .iter()
        .any(|r| r.contains("time budget")));
    assert_eq!(result.trace.iter().filter(|e| e.event == "timed_out").count(), 0);

    // One slow page: it is given up and the digest runs on the others.
    let mut case = common::case("topic-brief", "failed-read-fallback");
    let slow = case.fixture.pages.keys().next().unwrap().clone();
    case.fixture.pages.get_mut(&slow).unwrap().delay_ms = 60_000;
    let result = run(
        &template,
        &app(&folder).with_budget(budget),
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert!(result.data["brief"].is_object(), "{:?}", result.diagnostics);
    assert_eq!(result.stats.model_calls, 2, "{:?}", result.diagnostics);
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
    assert_eq!(
        result.status_reasons,
        ["the template's script failed; see diagnostics"]
    );
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
        // The digest is kept, but it lost points: partial, and why.
        let expected = if model.malformed_points { 3 } else { 1 };
        assert_eq!(
            result.status,
            RunStatus::Partial,
            "{:?}",
            result.diagnostics
        );
        assert_eq!(
            result.status_reasons,
            [format!("the digest dropped {expected} invalid point(s)")]
        );
        assert_eq!(result.data["digest"]["dropped_points"], expected);
        // Exactly the valid points survive.
        assert_eq!(
            result.data["digest"]["points"], good.data["digest"]["points"],
            "{model:?}"
        );
        let dropped: Vec<&String> = result
            .diagnostics
            .iter()
            .filter(|d| d.starts_with("research.digest") && d.contains("dropped"))
            .collect();
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
    // Skipping it is not a gap: the digest of the rest is complete.
    assert_eq!(
        result.status,
        RunStatus::Ready,
        "{:?}",
        result.status_reasons
    );
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
    // Ready: a read failed, but the fallback recovered it. The failure is
    // in the diagnostics and the counts.
    assert_eq!(
        result.status,
        RunStatus::Ready,
        "{:?}",
        result.status_reasons
    );
    assert!(result.status_reasons.is_empty());
    assert!(result.diagnostics.iter().any(|d| d.contains("HTTP 403")));
    assert_eq!(result.data["queries"][1]["failed"], 1);
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

    // With no readable result in a language, it goes unread; there was
    // nothing to read, so the brief is still complete.
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
    assert_eq!(
        result.status,
        RunStatus::Ready,
        "{:?}",
        result.status_reasons
    );
    assert_eq!(result.stats.failed, 0);
    assert_eq!(result.data["queries"][1]["read"], 0);
    assert_eq!(result.data["queries"][1]["unreadable"], 3);
    assert_eq!(result.data["queries"][0]["read"], 2);
}

#[tokio::test]
async fn topic_brief_replaces_early_failures_up_to_read_top() {
    // Validation run 3 (28 Sep 2026): 4 of the typhoon brief's 7 reads
    // failed and the old cap (`read_top` plus one per language) stopped it
    // at 3 of 5 with candidates left. Now a failed publisher's other
    // candidates wait until the rest are tried, and attempts go up to twice
    // `read_top` plus one per language.
    let template = template("topic-brief");
    let case = case("topic-brief", "early-reads-fail");
    let folder = temp_dir("early-fail");
    let result = run(
        &template,
        &app(&folder),
        case.params.clone(),
        &fixture::host(&case.fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        result.status,
        RunStatus::Ready,
        "{:?}",
        result.status_reasons
    );
    assert_eq!(result.data["sources"].as_array().unwrap().len(), 4);
    assert_eq!(result.data["missing"], 3);
    assert_eq!(result.stats.reads, 7);
    // The second Harbor Wire candidate (its publisher answered with a bot
    // challenge) was left for last and never needed.
    assert_eq!(result.data["queries"][0]["left"], 1);
    assert!(result
        .diagnostics
        .iter()
        .all(|d| !d.contains("host_skipped")));
}

#[tokio::test]
async fn a_publisher_that_refused_a_read_is_not_read_again() {
    // One language, two reads wanted. Harbor Wire answers the first read
    // with a bot challenge; the only candidates left are its own (by source
    // name, and by the host its first read ended on), so the host refuses
    // them without reading. A page that merely had no main text does not
    // mark its publisher.
    let fixture: FixtureData = serde_json::from_value(json!({
        "searches": [{"topic": "Typhoon Surigae", "language": "en", "items": [
            {"url": "https://news.google.com/rss/articles/a", "title": "Typhoon Surigae 1", "source": "Harbor Wire", "language": "en", "published_at": ""},
            {"url": "https://news.google.com/rss/articles/b", "title": "Typhoon Surigae 2", "source": "Coast Daily", "language": "en", "published_at": ""},
            {"url": "https://news.google.com/rss/articles/c", "title": "Typhoon Surigae 3", "source": "Harbor Wire", "language": "en", "published_at": ""},
            {"url": "https://harborwire.invalid/storm/2", "title": "Typhoon Surigae 4", "source": "HW Online", "language": "en", "published_at": ""},
            {"url": "https://news.google.com/rss/articles/e", "title": "Typhoon Surigae 5", "source": "Coast Daily", "language": "en", "published_at": ""}
        ]}],
        "pages": {
            "https://news.google.com/rss/articles/a": {"text": "", "error": "bot_challenge: a DataDome challenge (not bypassed) (final URL: https://www.harborwire.invalid/storm/1)"},
            "https://news.google.com/rss/articles/b": {"text": "", "error": "no_main_text: rendered (final URL: https://coastdaily.invalid/1)"},
            "https://news.google.com/rss/articles/c": {"text": "Typhoon Surigae neared Okinawa."},
            "https://harborwire.invalid/storm/2": {"text": "Typhoon Surigae weakened."},
            "https://news.google.com/rss/articles/e": {"text": "Typhoon Surigae shifted east."}
        }
    }))
    .unwrap();
    let params = json!({"topic": "Typhoon Surigae", "language": "en",
        "languages": [{"language": "en", "translate": false}],
        "per_language": 5, "read_top": 2});
    let result = run(
        &template("topic-brief"),
        &app(&temp_dir("host-skip")),
        params,
        &fixture::host(&fixture),
        RunOptions::default(),
    )
    .await
    .unwrap();
    // Round 1 reads a (bot challenge) and b (no main text); both mark
    // their publisher in the template. Round 2 takes d first (HW Online has
    // no failed read), then c: the host refuses both without reading, d by
    // the host a's read ended on, c by its source name. The attempt cap
    // (2 x 2 + 1) leaves one more: e, from Coast Daily, whose no-main-text
    // failure did not make the host refuse it.
    let mut skipped: Vec<&str> = result
        .diagnostics
        .iter()
        .filter_map(|d| d.split_once("host_skipped: ").map(|(_, m)| m))
        .collect();
    skipped.sort();
    assert_eq!(
        skipped,
        [
            "host harborwire.invalid refused an earlier read in this run (bot_challenge); not read",
            "source \"Harbor Wire\" refused an earlier read in this run (bot_challenge); not read",
        ],
        "{:?}",
        result.diagnostics
    );
    assert_eq!(result.data["queries"][0]["failed"], 4);
    let urls: Vec<&str> = result.data["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["url"].as_str().unwrap())
        .collect();
    assert_eq!(urls, ["https://news.google.com/rss/articles/e"]);
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
    assert_eq!(result.status, RunStatus::Failed);
    assert_eq!(result.status_reasons, ["the search found nothing"]);
    assert!(result.data.is_null());
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
    assert_eq!(result.status_reasons, ["the search failed"]);
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

#[tokio::test]
async fn a_suspended_provider_is_a_diagnostic_not_a_gap() {
    // Validation run 2: GDELT answered 429, octos suspended it, and every
    // search said `partial`, so every run was partial. Other providers
    // answered; the run is complete.
    let (case, template) = city();
    let mut data = case.fixture.clone();
    data.searches[0].partial = true;
    data.searches[0].notes =
        vec!["engine gdelt (en) suspended after earlier errors: suspended for 86s".into()];
    let result = run(
        &template,
        &app(&temp_dir("suspended")),
        case.params.clone(),
        &fixture::host(&data),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        result.status,
        RunStatus::Ready,
        "{:?}",
        result.status_reasons
    );
    assert!(result.status_reasons.is_empty());
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.contains("gdelt (en) suspended")));
}

#[tokio::test]
async fn topic_brief_is_partial_when_a_language_or_the_read_count_falls_short() {
    // Every English read fails: all three English candidates are tried
    // (the attempt cap, 2 x 2 + 2, leaves room), so English has no read and
    // no candidate left.
    let template = template("topic-brief");
    let case = case("topic-brief", "failed-read-fallback");
    let mut data = case.fixture.clone();
    for item in &mut data.searches[0].items {
        item.readable = true;
        data.pages.insert(
            item.url.clone(),
            fixture::RecordedPage {
                error: Some("no main text".into()),
                ..Default::default()
            },
        );
    }
    let result = run(
        &template,
        &app(&temp_dir("short")),
        case.params.clone(),
        &fixture::host(&data),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Partial);
    assert_eq!(
        result.status_reasons,
        ["language en: no article read (3 failed, 0 readable left untried)"]
    );
    let en = &result.data["queries"][0];
    assert_eq!(
        (&en["read"], &en["failed"], &en["left"]),
        (&json!(0), &json!(3), &json!(0))
    );
    assert!(result.data["brief"].is_object());

    // Every read in every language fails: nothing usable, failed.
    for page in data.pages.values_mut() {
        page.error = Some("no main text".into());
    }
    let result = run(
        &template,
        &app(&temp_dir("short")),
        case.params.clone(),
        &fixture::host(&data),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Failed);
    assert!(result.data.is_null());
    assert_eq!(
        result.status_reasons.last().unwrap(),
        "no article on the topic was read (5 reads failed)"
    );
    assert!(result
        .status_reasons
        .iter()
        .any(|r| r.starts_with("language zh: no article read")));

    // The attempts run out with candidates untried: five English reads
    // fail, one Chinese read succeeds, and the cap (2 x 2 + 2 = 6) stops
    // the run with one of the two articles asked for.
    let items = |language: &str, source: &str| -> Vec<Value> {
        (1..=5)
            .map(|i| {
                json!({"url": format!("https://example.invalid/{language}/{i}"),
                "title": if language == "zh" { format!("台风舒力基逼近冲绳 {i}") } else { format!("Typhoon Surigae {i}") },
                "source": format!("{source} {i}"), "language": language, "published_at": ""})
            })
            .collect()
    };
    let mut pages = serde_json::Map::new();
    for i in 1..=5 {
        pages.insert(
            format!("https://example.invalid/en/{i}"),
            json!({"text": "", "error": "no_main_text: rendered"}),
        );
        pages.insert(
            format!("https://example.invalid/zh/{i}"),
            json!({"text": "台风舒力基逼近冲绳。"}),
        );
    }
    let data: FixtureData = serde_json::from_value(json!({
        "searches": [
            {"topic": "Typhoon Surigae", "language": "en", "items": items("en", "Wire")},
            {"topic": "台风舒力基", "language": "zh", "items": items("zh", "报")}
        ],
        "pages": pages,
        "translations": {"Typhoon Surigae": "台风舒力基"}
    }))
    .unwrap();
    let params = json!({"topic": "Typhoon Surigae", "language": "en",
        "languages": [{"language": "en", "translate": false}, {"language": "zh", "translate": true}],
        "per_language": 5, "read_top": 2});
    let result = run(
        &template,
        &app(&temp_dir("short")),
        params,
        &fixture::host(&data),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Partial);
    assert_eq!(result.stats.reads, 6, "{:?}", result.diagnostics);
    assert_eq!(
        result.status_reasons,
        [
            "language en: no article read (5 failed, 0 readable left untried)",
            "read 1 of 2 articles while 4 readable candidates remained"
        ]
    );
}

#[tokio::test]
async fn a_summary_sentence_no_point_backs_is_dropped() {
    // Validation run 2: the points were clean, the summary added facts. The
    // host drops a summary sentence no point backs and says so; the digest
    // is otherwise complete.
    let (case, template) = city();
    let mut data = case.fixture.clone();
    data.model.summary_drift = true;
    let result = run(
        &template,
        &app(&temp_dir("drift")),
        case.params.clone(),
        &fixture::host(&data),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        result.status,
        RunStatus::Ready,
        "{:?}",
        result.status_reasons
    );
    let digest = &result.data["digest"];
    assert!(!digest["summary"].as_str().unwrap().contains("2031"));
    assert_eq!(
        digest["summary_check"],
        json!({"sentences": 2, "dropped": 1, "flagged": 0})
    );
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.ends_with("digest summary: sentence 2 of 2 dropped: no point backs it")));
}

#[tokio::test]
async fn a_template_states_its_status_and_reasons() {
    let folder = temp_dir("reasons");
    let host = fixture::host(&FixtureData::default());
    let status = |source: &str| {
        let template = probe(&["search"], source).unwrap();
        let app = app(&folder);
        let host = &host;
        async move {
            run(&template, &app, json!({}), host, RunOptions::default())
                .await
                .unwrap()
        }
    };
    // Reasons are passed through.
    let result = status(
        "use mod.research
{status: \"partial\", reasons: [\"language zh: no article read\"], data: {n: 1}}\n",
    )
    .await;
    assert_eq!(result.status, RunStatus::Partial);
    assert_eq!(result.status_reasons, ["language zh: no article read"]);
    assert_eq!(result.data["n"], 1);
    // A template may declare `failed`: its data is not published.
    let result = status(
        "use mod.research
{status: \"failed\", reasons: [\"nothing found\"], data: {n: 0}}\n",
    )
    .await;
    assert_eq!(result.status, RunStatus::Failed);
    assert_eq!(result.status_reasons, ["nothing found"]);
    assert!(result.data.is_null());
    // Partial without a reason still says so; a ready run has none.
    let result = status(
        "use mod.research
{status: \"partial\", data: {}}\n",
    )
    .await;
    assert_eq!(result.status_reasons, ["the template gave no reason"]);
    let result = status(
        "use mod.research
{status: \"ready\", reasons: [\"stale\"], data: {}}\n",
    )
    .await;
    assert!(result.status_reasons.is_empty());
    // A reason with a URL is withheld; a malformed one refuses the output.
    let result = status(
        "use mod.research
{status: \"partial\", reasons: [\"see https://x.invalid\"], data: {}}\n",
    )
    .await;
    assert_eq!(
        result.status_reasons,
        ["a reason was withheld: it contained a URL"]
    );
    let result = status(
        "use mod.research
{status: \"partial\", reasons: [3], data: {}}\n",
    )
    .await;
    assert_eq!(result.status, RunStatus::Failed);
    assert_eq!(
        result.status_reasons,
        ["the template's output was refused; see diagnostics"]
    );
    let result = status(
        "use mod.research
{status: \"ready\", data: {}, extra: 1}\n",
    )
    .await;
    assert_eq!(result.status, RunStatus::Failed);
}

/// A backend that records the queries it was asked and serves one Google
/// News item whose read ends on the publisher's page.
struct Probe {
    queries: Mutex<Vec<SearchQuery>>,
}

impl Probe {
    fn host(self: &Arc<Self>) -> ResearchHost {
        ResearchHost::new(
            Arc::clone(self) as Arc<dyn ResearchBackend>,
            Arc::new(FakeModel::new(BTreeMap::new(), FakeModelConfig::default())),
        )
    }
}

impl ResearchBackend for Probe {
    fn search<'a>(
        &'a self,
        _ctx: &'a CallContext,
        query: SearchQuery,
    ) -> HostFuture<'a, Result<SearchResults, HostError>> {
        Box::pin(async move {
            self.queries.lock().unwrap().push(query);
            Ok(SearchResults {
                items: vec![FoundItem {
                    url: "https://news.google.com/rss/articles/CBMiZmlzaW9u?oc=5".into(),
                    title: "Fusion energy milestone".into(),
                    source: "Example Post".into(),
                    language: "en".into(),
                    published_at: "2026-09-28T00:00:00Z".into(),
                    via: "fixture".into(),
                    readable: true,
                    snippet: "Fusion energy progress".into(),
                }],
                ..SearchResults::default()
            })
        })
    }
    fn read<'a>(
        &'a self,
        _ctx: &'a CallContext,
        _item: &'a FoundItem,
    ) -> HostFuture<'a, Result<PageText, HostError>> {
        Box::pin(async {
            Ok(PageText {
                text: "Fusion energy researchers report a milestone.".into(),
                title: Some("Fusion energy milestone".into()),
                final_url: Some("https://examplepost.com/fusion-energy".into()),
            })
        })
    }
}

#[tokio::test]
async fn a_search_in_an_ungranted_category_is_denied() {
    let template = probe(
        &["search"],
        "use mod.research\nlet found = try research.search({topic: \"fusion energy\", category: \"science\"}).await() catch nil\n{status: \"partial\", data: {searched: found != nil}}\n",
    )
    .unwrap();
    let folder = temp_dir("category-denied");
    let scoped = app(&folder).with_scope(scope::parse(&json!({"categories": ["news"]})).unwrap());
    let backend = Arc::new(Probe {
        queries: Mutex::new(Vec::new()),
    });
    let result = run(
        &template,
        &scoped,
        json!({}),
        &backend.host(),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Partial);
    assert_eq!(result.data["searched"], false);
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.contains("category science is not in this app's research grant")));
    assert!(backend.queries.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_granted_category_reaches_the_backend() {
    let template = probe(
        &["search"],
        "use mod.research\nlet found = research.search({topic: \"fusion energy\", category: \"science\"}).await()\n{status: \"ready\", data: {n: found.items}}\n",
    )
    .unwrap();
    let folder = temp_dir("category-granted");
    let scoped =
        app(&folder).with_scope(scope::parse(&json!({"categories": ["science"]})).unwrap());
    let backend = Arc::new(Probe {
        queries: Mutex::new(Vec::new()),
    });
    let result = run(
        &template,
        &scoped,
        json!({}),
        &backend.host(),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Ready);
    let queries = backend.queries.lock().unwrap();
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0].category.as_deref(), Some("science"));
}

#[tokio::test]
async fn a_redirected_read_reports_its_resolved_url_and_publisher() {
    let template = probe(
        &["search", "article"],
        "use mod.research\nlet found = research.search({topic: \"fusion energy\", category: \"science\"}).await()\nlet a = research.article({id: found.items[0].id}).await()\n{status: \"ready\", data: {resolved: a.resolved_url, publisher: a.publisher_host}}\n",
    )
    .unwrap();
    let folder = temp_dir("resolved-url");
    let backend = Arc::new(Probe {
        queries: Mutex::new(Vec::new()),
    });
    let result = run(
        &template,
        &app(&folder),
        json!({}),
        &backend.host(),
        RunOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, RunStatus::Ready);
    assert_eq!(
        result.data["resolved"], "https://examplepost.com/fusion-energy",
        "{:?}",
        result.diagnostics
    );
    assert_eq!(result.data["publisher"], "examplepost.com");
    assert!(result.provenance.iter().any(|p| {
        p.resolved_url.as_deref() == Some("https://examplepost.com/fusion-energy")
            && p.publisher_host.as_deref() == Some("examplepost.com")
    }));
}
