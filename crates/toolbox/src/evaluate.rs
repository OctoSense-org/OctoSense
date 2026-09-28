//! Evaluating a candidate template (usually a fork) against a baseline (its
//! parent) on the same recorded inputs, and deciding whether to adopt it.

use crate::fixture::{self, FixtureCase};
use crate::library::Template;
use crate::runner::{run, RunOptions, RunResult, RunStatus};
use crate::{json, AppContext, Result};
use octoscript_schema::JsonSchema;
use serde::{Deserialize, Serialize};

/// A recorded case: parameters, the fixture the host replays, and optionally
/// the expected result.
pub type EvalCase = FixtureCase;

/// Scores a run's text output (0.0–1.0, higher is better). Pluggable: a
/// rubric, a model judge, a reference comparison. The default is none.
pub trait QualityScorer {
    fn score(&self, case: &EvalCase, result: &RunResult) -> Option<f64>;
}

/// No quality scoring.
pub struct NoScorer;

impl QualityScorer for NoScorer {
    fn score(&self, _case: &EvalCase, _result: &RunResult) -> Option<f64> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseMetrics {
    pub status: RunStatus,
    /// The run produced `data` valid against its template's output schema.
    pub schema_valid: bool,
    /// Status and data equal the case's expectation, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_match: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difference: Option<String>,
    pub calls: u32,
    pub model_calls: u32,
    pub reads: u32,
    pub elapsed_ms: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<f64>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseComparison {
    pub name: String,
    pub a: CaseMetrics,
    pub b: CaseMetrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictOutcome {
    Better,
    Worse,
    Equal,
}

/// Whether the candidate `b` is better than, worse than or equal to the
/// baseline `a`, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub outcome: VerdictOutcome,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationReport {
    pub a: crate::runner::TemplateRef,
    pub b: crate::runner::TemplateRef,
    pub cases: Vec<CaseComparison>,
    pub verdict: Verdict,
}

fn metrics(
    template: &Template,
    case: &EvalCase,
    result: &RunResult,
    scorer: &dyn QualityScorer,
) -> CaseMetrics {
    let schema_valid = result.status != RunStatus::Failed
        && JsonSchema::compile(template.manifest.output.clone())
            .is_ok_and(|s| s.validate(&result.data).is_ok());
    let (expected_match, difference) = match &case.expected {
        Some(expected) => {
            let same_status = expected.status == result.status;
            let difference = json::first_difference(&expected.data, &result.data, "$");
            let difference = match (same_status, difference) {
                (false, d) => Some(format!(
                    "status {:?}, expected {:?}{}",
                    result.status,
                    expected.status,
                    d.map(|d| format!("; data differs at {d}"))
                        .unwrap_or_default()
                )),
                (true, Some(d)) => Some(format!("data differs at {d}")),
                (true, None) => None,
            };
            (Some(difference.is_none()), difference)
        }
        None => (None, None),
    };
    CaseMetrics {
        status: result.status,
        schema_valid,
        expected_match,
        difference,
        calls: result.stats.calls,
        model_calls: result.stats.model_calls,
        reads: result.stats.reads,
        elapsed_ms: result.stats.elapsed_ms,
        quality: scorer.score(case, result),
        diagnostics: result.diagnostics.clone(),
    }
}

/// Runs `a` and `b` on every case's recorded inputs (a fresh fixture host
/// per run) and compares them. `app` supplies grants and scope; results are
/// not written.
pub async fn evaluate(
    a: &Template,
    b: &Template,
    cases: &[EvalCase],
    app: &AppContext,
    scorer: &dyn QualityScorer,
) -> Result<EvaluationReport> {
    let mut comparisons = Vec::new();
    for case in cases {
        let mut sides = Vec::new();
        for template in [a, b] {
            let host = fixture::host(&case.fixture);
            let result = run(
                template,
                app,
                case.params.clone(),
                &host,
                RunOptions::default(),
            )
            .await?;
            sides.push(metrics(template, case, &result, scorer));
        }
        let b_side = sides.pop().expect("two sides");
        let a_side = sides.pop().expect("two sides");
        comparisons.push(CaseComparison {
            name: case.name.clone(),
            a: a_side,
            b: b_side,
        });
    }
    let verdict = adopt(&comparisons);
    let reference = |t: &Template| crate::runner::TemplateRef {
        id: t.manifest.id.clone(),
        version: t.manifest.version.clone(),
        digest: t.digest.clone(),
    };
    Ok(EvaluationReport {
        a: reference(a),
        b: reference(b),
        cases: comparisons,
        verdict,
    })
}

/// The adoption rule. Criteria in order; the first that differs decides:
///
/// 1. schema-valid outputs, 2. matches with the expected results,
/// 3. `ready` (complete) results, 4. mean quality score (when both sides are
///    scored), 5. model calls, 6. host calls, 7. latency (only a difference
///    above 25% and 20 ms counts; fixture delays make it meaningful).
///
/// Every difference found is listed in `reasons`, deciding or not.
pub fn adopt(cases: &[CaseComparison]) -> Verdict {
    let count = |f: &dyn Fn(&CaseMetrics) -> bool, side: fn(&CaseComparison) -> &CaseMetrics| {
        cases.iter().filter(|c| f(side(c))).count() as f64
    };
    fn a(c: &CaseComparison) -> &CaseMetrics {
        &c.a
    }
    fn b(c: &CaseComparison) -> &CaseMetrics {
        &c.b
    }
    let sum = |f: &dyn Fn(&CaseMetrics) -> f64, side: fn(&CaseComparison) -> &CaseMetrics| {
        cases.iter().map(|c| f(side(c))).sum::<f64>()
    };
    let mean_quality = |side: fn(&CaseComparison) -> &CaseMetrics| {
        let scores: Vec<f64> = cases.iter().filter_map(|c| side(c).quality).collect();
        (scores.len() == cases.len() && !scores.is_empty())
            .then(|| scores.iter().sum::<f64>() / scores.len() as f64)
    };

    // (name, a value, b value, higher is better, minimum difference that counts)
    let mut criteria: Vec<(&str, f64, f64, bool, f64)> = vec![
        (
            "schema-valid outputs",
            count(&|m| m.schema_valid, a),
            count(&|m| m.schema_valid, b),
            true,
            0.0,
        ),
        (
            "expected matches",
            count(&|m| m.expected_match == Some(true), a),
            count(&|m| m.expected_match == Some(true), b),
            true,
            0.0,
        ),
        (
            "ready results",
            count(&|m| m.status == RunStatus::Ready, a),
            count(&|m| m.status == RunStatus::Ready, b),
            true,
            0.0,
        ),
    ];
    if let (Some(qa), Some(qb)) = (mean_quality(a), mean_quality(b)) {
        criteria.push(("mean quality", qa, qb, true, 0.01));
    }
    criteria.push((
        "model calls",
        sum(&|m| m.model_calls as f64, a),
        sum(&|m| m.model_calls as f64, b),
        false,
        0.0,
    ));
    criteria.push((
        "host calls",
        sum(&|m| m.calls as f64, a),
        sum(&|m| m.calls as f64, b),
        false,
        0.0,
    ));
    let (la, lb) = (sum(&|m| m.elapsed_ms, a), sum(&|m| m.elapsed_ms, b));
    let latency_floor = (la.max(lb) * 0.25).max(20.0);
    criteria.push(("latency (ms)", la, lb, false, latency_floor));

    let mut outcome = VerdictOutcome::Equal;
    let mut reasons = Vec::new();
    for (name, va, vb, higher_better, floor) in criteria {
        let diff = vb - va;
        if diff.abs() <= floor || diff == 0.0 {
            continue;
        }
        let b_better = (diff > 0.0) == higher_better;
        reasons.push(format!(
            "{name}: {} {} → {} ({})",
            if b_better { "better" } else { "worse" },
            number(va),
            number(vb),
            if outcome == VerdictOutcome::Equal {
                "decides"
            } else {
                "noted"
            }
        ));
        if outcome == VerdictOutcome::Equal {
            outcome = if b_better {
                VerdictOutcome::Better
            } else {
                VerdictOutcome::Worse
            };
        }
    }
    if reasons.is_empty() {
        reasons.push("no measured difference".into());
    }
    Verdict { outcome, reasons }
}

fn number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}
