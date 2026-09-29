//! Forks (lineage, the same static check, no widening), evaluation verdicts
//! and the JSON tool surface.

mod common;

use common::{app, case, library, temp_dir};
use octosense_toolbox::api::{EvaluateRequest, ForkRequest, RunRequest};
use octosense_toolbox::evaluate::{evaluate, NoScorer};
use octosense_toolbox::fixture;
use octosense_toolbox::fork::{fork, load_fork};
use octosense_toolbox::library::{MANIFEST_FILE, SOURCE_FILE};
use octosense_toolbox::{ErrorKind, EvalCase, QualityScorer, RunResult, Toolbox, VerdictOutcome};
use serde_json::{json, Value};
use std::path::Path;

fn edit_manifest(dir: &Path, f: impl FnOnce(&mut Value)) {
    let path = dir.join(MANIFEST_FILE);
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    f(&mut manifest);
    std::fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
}

fn edit_source(dir: &Path, f: impl FnOnce(String) -> String) {
    let path = dir.join(SOURCE_FILE);
    let source = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, f(source)).unwrap();
}

#[test]
fn a_fork_keeps_lineage_and_loads_from_the_apps_folder() {
    let library = library();
    let folder = temp_dir("fork");
    let app = app(&folder);
    let forked = fork(&library, "news-digest", &app.templates_dir(), None).unwrap();
    let parent = library.get("news-digest").unwrap();
    assert_eq!(forked.manifest.id, "news-digest.fork");
    let lineage = forked.manifest.lineage.as_ref().unwrap();
    assert_eq!(lineage.parent_id, "news-digest");
    assert_eq!(lineage.parent_version, parent.manifest.version);
    assert_eq!(lineage.parent_digest, parent.digest);
    assert_eq!(forked.source, parent.source);
    assert_ne!(
        forked.digest, parent.digest,
        "the lineage is part of the fork's digest"
    );
    // Forking again under the same id, or under a library id, is refused.
    assert!(fork(&library, "news-digest", &app.templates_dir(), None).is_err());
    assert!(fork(
        &library,
        "news-digest",
        &app.templates_dir(),
        Some("briefing")
    )
    .is_err());
    assert_eq!(
        fork(&library, "no-such", &app.templates_dir(), None)
            .unwrap_err()
            .kind,
        ErrorKind::NotFound
    );
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_fork_can_never_widen_its_parent() {
    let library = library();
    let folder = temp_dir("widen");
    let dir = app(&folder).templates_dir();
    fork(&library, "weather-plan", &dir, Some("my-weather")).unwrap();
    let fork_dir = dir.join("my-weather");

    // Narrowing is fine.
    edit_manifest(&fork_dir, |m| m["budget"]["max_ms"] = json!(30000));
    load_fork(&library, &fork_dir).unwrap();

    // A method the parent does not declare (weather-plan has no `query`).
    edit_manifest(
        &fork_dir,
        |m| {
            m["modules"] =
                json!([{"module": "research", "methods": ["query", "search", "article", "digest"]}])
        },
    );
    assert_eq!(
        load_fork(&library, &fork_dir).unwrap_err().kind,
        ErrorKind::Widening
    );
    edit_manifest(
        &fork_dir,
        |m| {
            m["modules"] =
                json!([{"module": "research", "methods": ["search", "article", "digest"]}])
        },
    );

    // A budget above the parent's.
    for field in [
        "max_calls",
        "max_model_calls",
        "max_reads",
        "max_ms",
        "max_concurrency",
    ] {
        let parent = library.get("weather-plan").unwrap().manifest.budget;
        let above = serde_json::to_value(parent).unwrap()[field]
            .as_u64()
            .unwrap()
            + 1;
        edit_manifest(&fork_dir, |m| {
            m["budget"] = serde_json::to_value(parent).unwrap();
            m["budget"][field] = json!(above);
        });
        let err = load_fork(&library, &fork_dir).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Widening, "{field}: {err}");
    }
    edit_manifest(&fork_dir, |m| {
        m["budget"] =
            serde_json::to_value(library.get("weather-plan").unwrap().manifest.budget).unwrap()
    });
    load_fork(&library, &fork_dir).unwrap();

    // An edit calling an undeclared method fails the same static check.
    edit_source(&fork_dir, |s| {
        s.replace(
            "let forecast = forecast_search.await()",
            "let forecast = forecast_search.await()\nlet q = research.query({query: \"x\"}).await()",
        )
    });
    assert_eq!(
        load_fork(&library, &fork_dir).unwrap_err().kind,
        ErrorKind::Check
    );

    // A local template without lineage is not a fork.
    edit_source(&fork_dir, |s| {
        s.replace("\nlet q = research.query({query: \"x\"}).await()", "")
    });
    edit_manifest(&fork_dir, |m| {
        m.as_object_mut().unwrap().remove("lineage");
    });
    assert_eq!(
        load_fork(&library, &fork_dir).unwrap_err().kind,
        ErrorKind::Manifest
    );
    let _ = std::fs::remove_dir_all(folder);
}

fn city_cases() -> Vec<EvalCase> {
    ["city-infrastructure", "blocked-article", "translated-query"]
        .iter()
        .map(|name| case("news-digest", name))
        .collect()
}

#[tokio::test]
async fn evaluate_compares_on_the_same_recorded_inputs() {
    let library = library();
    let folder = temp_dir("evaluate");
    let app = app(&folder);
    let dir = app.templates_dir();
    let parent = library.get("news-digest").unwrap().clone();

    // An unchanged fork is equal.
    let same = fork(&library, "news-digest", &dir, Some("same")).unwrap();
    let report = evaluate(&parent, &same, &city_cases(), &app, &NoScorer)
        .await
        .unwrap();
    assert_eq!(
        report.verdict.outcome,
        VerdictOutcome::Equal,
        "{:?}",
        report.verdict
    );
    assert!(report
        .cases
        .iter()
        .all(|c| c.a.expected_match == Some(true) && c.b.expected_match == Some(true)));

    // A fork that awaits each read before starting the next: the same data
    // and calls, slower. Worse than the parent; the parent is better than it.
    fork(&library, "news-digest", &dir, Some("serial")).unwrap();
    edit_source(&dir.join("serial"), |s| {
        s.replace(
            "    array.push(tasks, research.article({id: item.id}))",
            "    let done = try research.article({id: item.id}).await() catch nil\n    array.push(tasks, done)",
        )
        .replace("    let article = try task.await() catch nil", "    let article = task")
    });
    let serial = load_fork(&library, &dir.join("serial")).unwrap();
    let report = evaluate(&parent, &serial, &city_cases(), &app, &NoScorer)
        .await
        .unwrap();
    assert_eq!(
        report.verdict.outcome,
        VerdictOutcome::Worse,
        "{:?}",
        report.verdict
    );
    assert!(report.verdict.reasons[0].starts_with("latency"));
    assert!(report
        .cases
        .iter()
        .all(|c| c.b.expected_match == Some(true)));
    let report = evaluate(&serial, &parent, &city_cases(), &app, &NoScorer)
        .await
        .unwrap();
    assert_eq!(report.verdict.outcome, VerdictOutcome::Better);

    // A fork that reads fewer articles uses fewer calls but no longer matches
    // the expected results: correctness decides before cost.
    fork(&library, "news-digest", &dir, Some("fewer")).unwrap();
    edit_source(&dir.join("fewer"), |s| {
        s.replace("limit: request.limit", "limit: 1")
    });
    let fewer = load_fork(&library, &dir.join("fewer")).unwrap();
    let report = evaluate(&parent, &fewer, &city_cases(), &app, &NoScorer)
        .await
        .unwrap();
    assert_eq!(report.verdict.outcome, VerdictOutcome::Worse);
    assert!(
        report.verdict.reasons[0].starts_with("expected matches"),
        "{:?}",
        report.verdict
    );
    assert!(report
        .verdict
        .reasons
        .iter()
        .any(|r| r.starts_with("host calls: better")));
    let _ = std::fs::remove_dir_all(folder);
}

/// Scores a run by how many digest points it has (a stand-in for a real
/// rubric or judge).
struct PointCount;

impl QualityScorer for PointCount {
    fn score(&self, _case: &EvalCase, result: &RunResult) -> Option<f64> {
        let points = result.data["digest"]["points"]
            .as_array()
            .map_or(0, Vec::len);
        Some(points as f64 / 10.0)
    }
}

#[tokio::test]
async fn a_quality_scorer_decides_between_equally_correct_templates() {
    let library = library();
    let folder = temp_dir("scorer");
    let app = app(&folder);
    let dir = app.templates_dir();
    let parent = library.get("news-digest").unwrap().clone();
    fork(&library, "news-digest", &dir, Some("fewer")).unwrap();
    edit_source(&dir.join("fewer"), |s| {
        s.replace("limit: request.limit", "limit: 2")
    });
    let fewer = load_fork(&library, &dir.join("fewer")).unwrap();
    // Cases without expectations: only quality and cost can differ.
    let cases: Vec<EvalCase> = city_cases()
        .into_iter()
        .map(|mut c| {
            c.expected = None;
            c
        })
        .collect();
    let report = evaluate(&parent, &fewer, &cases, &app, &PointCount)
        .await
        .unwrap();
    assert_eq!(report.verdict.outcome, VerdictOutcome::Worse);
    assert!(
        report.verdict.reasons[0].starts_with("mean quality"),
        "{:?}",
        report.verdict
    );
    let _ = std::fs::remove_dir_all(folder);
}

#[tokio::test]
async fn the_tool_surface_lists_runs_forks_and_evaluates() {
    let folder = temp_dir("api");
    let app = app(&folder);
    let data = case("news-digest", "city-infrastructure");
    let toolbox = Toolbox::new(library(), fixture::host(&data.fixture));

    let listed = toolbox
        .handle_json(&app, json!({"tool": "workflow.list", "arguments": {}}))
        .await;
    let ids: Vec<&str> = listed["templates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "briefing",
            "compare",
            "dossier",
            "market-brief",
            "news-digest",
            "topic-brief",
            "weather-plan"
        ]
    );
    assert!(listed["templates"]
        .as_array()
        .unwrap()
        .iter()
        .all(|t| t["runnable"] == true));

    let run = toolbox
        .handle_json(
            &app,
            json!({"tool": "workflow.run", "arguments": {"id": "news-digest", "params": data.params, "run_id": "r1"}}),
        )
        .await;
    assert_eq!(run["status"], "ready", "{run}");
    assert_eq!(run["result_path"], "toolbox/runs/news-digest/r1.json");
    assert!(folder.join("toolbox/runs/news-digest/r1.json").is_file());

    let forked = toolbox
        .handle_json(
            &app,
            json!({"tool": "workflow.fork", "arguments": {"id": "news-digest"}}),
        )
        .await;
    assert_eq!(forked["path"], "toolbox/templates/news-digest.fork");
    assert_eq!(forked["template"]["origin"], "fork");
    assert_eq!(forked["template"]["lineage"]["parent_id"], "news-digest");
    assert_eq!(forked["template"]["lineage_current"], true);

    let listed = toolbox.list(&app);
    assert!(listed.templates.iter().any(|t| t.id == "news-digest.fork"));
    // The fork runs through the same surface.
    let result = toolbox
        .run(
            &app,
            RunRequest {
                id: "news-digest.fork".into(),
                params: data.params.clone(),
                run_id: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(result.template.id, "news-digest.fork");

    let report = toolbox
        .evaluate(
            &app,
            EvaluateRequest {
                a: "news-digest".into(),
                b: "news-digest.fork".into(),
                cases: city_cases(),
            },
        )
        .await
        .unwrap();
    assert_eq!(report.verdict.outcome, VerdictOutcome::Equal);

    // A broken fork is reported by list, not dropped.
    std::fs::write(
        app.templates_dir()
            .join("news-digest.fork")
            .join(SOURCE_FILE),
        "use mod.tool\n",
    )
    .unwrap();
    let listed = toolbox.list(&app);
    assert_eq!(listed.refused.len(), 1);
    assert_eq!(listed.refused[0].kind, ErrorKind::Check);

    let missing = toolbox
        .handle_json(
            &app,
            json!({"tool": "workflow.run", "arguments": {"id": "nope"}}),
        )
        .await;
    assert_eq!(missing["error"]["kind"], "not_found");
    let unknown = toolbox
        .handle_json(&app, json!({"tool": "workflow.delete", "arguments": {}}))
        .await;
    assert_eq!(unknown["error"]["kind"], "params");
    assert!(toolbox
        .fork(
            &app,
            ForkRequest {
                id: "nope".into(),
                new_id: None
            }
        )
        .is_err());

    // An app without the research grant sees the templates as not runnable.
    let ungranted = octosense_toolbox::AppContext::new("os.notes", &folder);
    assert!(toolbox
        .list(&ungranted)
        .templates
        .iter()
        .all(|t| !t.runnable));
    assert_eq!(
        octosense_toolbox::api::tool_descriptors()
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn tool_descriptors_use_the_hub_risk_levels() {
    // octos and App Hub's tools.json accept only these three.
    let descriptors = octosense_toolbox::api::tool_descriptors();
    for tool in descriptors.as_array().unwrap() {
        let risk = tool["risk"].as_str().unwrap();
        assert!(
            ["read", "act", "destructive"].contains(&risk),
            "{}: {risk}",
            tool["name"]
        );
    }
    let fork = descriptors
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "workflow.fork")
        .unwrap();
    assert_eq!(fork["risk"], "act");
}
