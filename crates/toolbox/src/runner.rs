//! Running a template: the bounded OctoScript VM with only the declared host
//! modules, every call dispatched to the caller's [`ToolboxHost`] under the
//! app's grants, scope and budget, independent calls concurrently.

use crate::host::{AppContext, CallContext, HostError, HostReply, Provenance, Remaining};
use crate::library::Template;
use crate::manifest::Budget;
use crate::modules::{self, MethodKind};
use crate::{json, Error, ErrorKind, Result, ToolboxHost};
use futures_util::stream::{FuturesUnordered, StreamExt};
use octoscript_capabilities::{
    CapabilityModule, CapabilityRuntime, ExternalToolId, JsonToolContract, ToolError, ToolMetadata,
    ToolPolicy,
};
use octoscript_core::ExecutionLimits;
use octoscript_schema::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Largest parameter object a template accepts.
pub const MAX_PARAMS_BYTES: usize = 16 * 1024;
/// Largest `{status, data}` value a template may return.
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// A caller-chosen run id (`[A-Za-z0-9_-]{1,64}`); generated otherwise.
    pub run_id: Option<String>,
    /// Write the result to `<app folder>/toolbox/runs/<template>/<run>.json`.
    pub write_result: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Everything the template needed arrived.
    Ready,
    /// Usable data with gaps; see `diagnostics`.
    Partial,
    /// No usable data: the script failed or its output was refused.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateRef {
    pub id: String,
    pub version: String,
    pub digest: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunStats {
    /// Host calls dispatched.
    pub calls: u32,
    pub model_calls: u32,
    pub pages: u32,
    /// Calls refused by the budget before dispatch.
    pub denied: u32,
    /// Calls the host failed or that timed out.
    pub failed: u32,
    pub peak_concurrency: u32,
    pub elapsed_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceEvent {
    /// `start`, `complete`, `denied` or `timed_out`.
    pub event: String,
    pub call: usize,
    pub tool: String,
    pub ms: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunResult {
    pub run_id: String,
    pub app_id: String,
    pub template: TemplateRef,
    pub status: RunStatus,
    /// The template's `data`, validated against its output schema; `null`
    /// when the run failed.
    pub data: Value,
    /// Host-kept provenance for every id or URL `data` refers to.
    pub provenance: Vec<Provenance>,
    pub diagnostics: Vec<String>,
    pub stats: RunStats,
    pub trace: Vec<TraceEvent>,
    pub started_at: String,
    /// Where the result was written, relative to the app folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_path: Option<String>,
}

fn valid_run_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn new_run_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ"),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// The budget a run gets: the template's, narrowed by the app's budget and
/// its scope's page limit.
pub fn effective_budget(template: &Budget, app: &AppContext) -> Budget {
    let mut budget = app.budget.map_or(*template, |b| template.min(b));
    if let Some(pages) = app.scope.max_pages {
        budget.max_pages = budget.max_pages.min(pages);
    }
    budget
}

fn runtime_err(e: impl std::fmt::Debug) -> Error {
    Error::new(ErrorKind::Runtime, format!("{e:?}"))
}

/// Builds the VM with exactly the template's declared methods installed.
fn build_runtime(
    template: &Template,
    budget: &Budget,
    params: &Value,
) -> Result<CapabilityRuntime> {
    let mut runtime = CapabilityRuntime::with_limits_and_pending(
        ExecutionLimits::default(),
        budget.max_calls as usize + 1,
    )
    .map_err(runtime_err)?;
    for decl in &template.manifest.modules {
        let spec = modules::spec(&decl.module)
            .ok_or_else(|| Error::new(ErrorKind::Manifest, "unknown module"))?;
        let mut module = CapabilityModule::new(&decl.module, spec.description);
        for method in &decl.methods {
            let m = spec
                .method(method)
                .ok_or_else(|| Error::new(ErrorKind::Manifest, "unknown method"))?;
            let tool = format!("{}.{}", decl.module, method);
            let mut policy = ToolPolicy::json(&tool);
            policy.max_calls = budget.max_calls as usize;
            policy.max_input_bytes = m.max_input_bytes;
            policy.max_output_bytes = m.max_output_bytes;
            runtime
                .register_validated_external_json_tool(
                    policy,
                    ToolMetadata::new(m.description),
                    JsonToolContract::new((m.input)(), (m.output)()).map_err(runtime_err)?,
                )
                .map_err(runtime_err)?;
            module = module.with_deferred_method(method, tool);
        }
        runtime
            .register_capability_module(module)
            .map_err(runtime_err)?;
    }
    runtime
        .set_json_global("request", params, MAX_PARAMS_BYTES, 16)
        .map_err(|e| Error::new(ErrorKind::Params, format!("{e:?}")))?;
    Ok(runtime)
}

type Inflight<'a> = Pin<
    Box<
        dyn Future<
                Output = (
                    ExternalToolId,
                    usize,
                    std::result::Result<HostReply, HostError>,
                ),
            > + 'a,
    >,
>;

struct Charges {
    calls: u32,
    model_calls: u32,
    pages: u32,
}

/// Runs `template` for `app` with `params`, dispatching module calls to
/// `host`.
///
/// Returns `Err` only when the run is refused before it starts (a missing
/// grant, invalid parameters). A script failure, a refused output or an
/// exhausted budget is reported in the result's `status` and `diagnostics`.
///
/// The future is not `Send`: the VM stays on the calling thread.
pub async fn run<H: ToolboxHost + ?Sized>(
    template: &Template,
    app: &AppContext,
    params: Value,
    host: &H,
    options: RunOptions,
) -> Result<RunResult> {
    let manifest = &template.manifest;
    for capability in manifest.required_grants() {
        if !app.grants.contains(capability) {
            return Err(Error::new(
                ErrorKind::NotGranted,
                format!("{} is not granted `{capability}`", app.app_id),
            ));
        }
    }
    let params_schema = JsonSchema::compile(manifest.params.clone())
        .map_err(|e| Error::new(ErrorKind::Manifest, e.to_string()))?;
    let params = with_defaults(&manifest.params, params);
    params_schema
        .validate(&params)
        .map_err(|e| Error::new(ErrorKind::Params, e.to_string()))?;
    let run_id = match options.run_id {
        Some(id) if valid_run_id(&id) => id,
        Some(id) => {
            return Err(Error::new(
                ErrorKind::Params,
                format!("invalid run id {id:?}"),
            ))
        }
        None => new_run_id(),
    };
    let budget = effective_budget(&manifest.budget, app);
    let app = Arc::new(app.clone());
    let mut runtime = build_runtime(template, &budget, &params)?;

    let started_at = chrono::Utc::now().to_rfc3339();
    let started = Instant::now();
    let deadline = tokio::time::Instant::from_std(started + Duration::from_millis(budget.max_ms));
    let ms = || started.elapsed().as_secs_f64() * 1000.0;

    let mut diagnostics = Vec::new();
    let mut trace = Vec::new();
    let mut stats = RunStats::default();
    let mut used = Charges {
        calls: 0,
        model_calls: 0,
        pages: 0,
    };
    let mut provenance: BTreeMap<String, Provenance> = BTreeMap::new();
    let mut provenance_order: Vec<String> = Vec::new();
    let mut inflight: FuturesUnordered<Inflight<'_>> = FuturesUnordered::new();
    let mut inflight_ids: BTreeMap<ExternalToolId, (usize, String)> = BTreeMap::new();
    let mut expired = false;
    let mut failed = false;
    let mut call_index = 0usize;

    let mut evaluation = match runtime.eval(&template.source) {
        Ok(evaluation) => Some(evaluation),
        Err(e) => {
            failed = true;
            diagnostics.push(format!("script: {e:?}"));
            None
        }
    };

    while let Some(current) = evaluation.as_ref() {
        if !current.succeeded() {
            failed = true;
            diagnostics.push(format!("script: {}", current.diagnostics.join("; ")));
            break;
        }
        // Claim every runnable call, up to the concurrency limit.
        while evaluation.as_ref().is_some_and(|e| e.suspended)
            && inflight.len() < budget.max_concurrency as usize
        {
            let Some(invocation) = runtime.claim_next_external_tool() else {
                break;
            };
            let index = call_index;
            call_index += 1;
            let tool = invocation.name.clone();
            let (module, method) = tool
                .split_once('.')
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
                .unwrap_or_default();
            let kind = modules::spec(&module)
                .and_then(|s| s.method(&method))
                .map(|m| m.kind);
            let denial = if expired {
                Some("max_ms")
            } else if used.calls >= budget.max_calls {
                Some("max_calls")
            } else {
                match kind {
                    Some(MethodKind::Model) if used.model_calls >= budget.max_model_calls => {
                        Some("max_model_calls")
                    }
                    Some(MethodKind::Page | MethodKind::Search)
                        if used.pages >= budget.max_pages =>
                    {
                        Some("max_pages")
                    }
                    None => Some("undeclared method"),
                    _ => None,
                }
            };
            if let Some(which) = denial {
                stats.denied += 1;
                let message = format!("budget: {which} exhausted");
                diagnostics.push(format!("{tool} (call {index}) refused: {message}"));
                trace.push(TraceEvent {
                    event: "denied".into(),
                    call: index,
                    tool: tool.clone(),
                    ms: ms(),
                    error: Some(message.clone()),
                });
                match runtime.complete_external_tool(invocation.id, Err(ToolError::Denied(message)))
                {
                    Ok(Some(resumed)) => evaluation = Some(resumed),
                    Ok(None) => {}
                    Err(e) => {
                        failed = true;
                        diagnostics.push(format!("{tool}: {e:?}"));
                        evaluation = None;
                    }
                }
                continue;
            }
            let input: Value = match serde_json::from_str(&invocation.input) {
                Ok(v) => v,
                Err(e) => {
                    failed = true;
                    diagnostics.push(format!("{tool}: input: {e}"));
                    evaluation = None;
                    break;
                }
            };
            let remaining = Remaining {
                calls: budget.max_calls.saturating_sub(used.calls),
                model_calls: budget.max_model_calls.saturating_sub(used.model_calls),
                pages: budget.max_pages.saturating_sub(used.pages),
                ms: budget
                    .max_ms
                    .saturating_sub(started.elapsed().as_millis() as u64),
            };
            used.calls += 1;
            match kind {
                Some(MethodKind::Model) => used.model_calls += 1,
                Some(MethodKind::Page | MethodKind::Search) => used.pages += 1,
                None => {}
            }
            let ctx = CallContext {
                app: app.clone(),
                run_id: run_id.clone(),
                template_id: manifest.id.clone(),
                template_digest: template.digest.clone(),
                budget,
                remaining,
                call_index: index,
            };
            trace.push(TraceEvent {
                event: "start".into(),
                call: index,
                tool: tool.clone(),
                ms: ms(),
                error: None,
            });
            inflight_ids.insert(invocation.id, (index, tool));
            let id = invocation.id;
            inflight.push(Box::pin(async move {
                let result = host.call(ctx, &module, &method, input).await;
                (id, index, result)
            }));
            stats.peak_concurrency = stats.peak_concurrency.max(inflight.len() as u32);
        }
        let Some(current) = evaluation.as_ref() else {
            break;
        };
        if !current.suspended {
            if !inflight.is_empty() {
                diagnostics.push(format!(
                    "{} call(s) were never awaited and were dropped",
                    inflight.len()
                ));
            }
            break;
        }
        if inflight.is_empty() {
            failed = true;
            diagnostics.push("script: suspended with no runnable call".into());
            break;
        }
        tokio::select! {
            Some((id, index, result)) = inflight.next() => {
                let (_, tool) = inflight_ids.remove(&id).unwrap_or((index, String::new()));
                let kind = tool.split_once('.').and_then(|(m, f)| modules::spec(m)?.method(f)).map(|m| m.kind);
                let completion = match result {
                    Ok(reply) => {
                        match kind {
                            Some(MethodKind::Model) => used.model_calls += reply.usage.model_calls.saturating_sub(1),
                            Some(MethodKind::Page | MethodKind::Search) => used.pages += reply.usage.pages.saturating_sub(1),
                            None => {}
                        }
                        for record in reply.provenance {
                            match provenance.get_mut(&record.id) {
                                Some(existing) => {
                                    if record.evidence_sha256.is_some() || existing.evidence_sha256.is_none() {
                                        *existing = record;
                                    }
                                }
                                None => {
                                    provenance_order.push(record.id.clone());
                                    provenance.insert(record.id.clone(), record);
                                }
                            }
                        }
                        trace.push(TraceEvent { event: "complete".into(), call: index, tool: tool.clone(), ms: ms(), error: None });
                        Ok(reply.output.to_string())
                    }
                    Err(error) => {
                        stats.failed += 1;
                        diagnostics.push(format!("{tool} (call {index}): {error}"));
                        trace.push(TraceEvent { event: "complete".into(), call: index, tool: tool.clone(), ms: ms(), error: Some(error.to_string()) });
                        Err(match error {
                            HostError::Denied(m) => ToolError::Denied(m),
                            HostError::Failed(m) => ToolError::Failed(m),
                            HostError::TimedOut(m) => ToolError::TimedOut(m),
                        })
                    }
                };
                match runtime.complete_external_tool(id, completion) {
                    Ok(Some(resumed)) => evaluation = Some(resumed),
                    Ok(None) => {}
                    Err(e) => {
                        // A reply that breaks the method's output contract is
                        // refused here; the script sees a failed call.
                        stats.failed += 1;
                        diagnostics.push(format!("{tool} (call {index}): {e:?}"));
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline), if !expired => {
                expired = true;
                diagnostics.push(format!("budget: max_ms ({} ms) elapsed; in-flight calls cancelled", budget.max_ms));
                inflight = FuturesUnordered::new();
                for (id, (index, tool)) in std::mem::take(&mut inflight_ids) {
                    stats.failed += 1;
                    trace.push(TraceEvent { event: "timed_out".into(), call: index, tool, ms: ms(), error: Some("max_ms".into()) });
                    match runtime.complete_external_tool(id, Err(ToolError::TimedOut("budget max_ms elapsed".into()))) {
                        Ok(Some(resumed)) => evaluation = Some(resumed),
                        Ok(None) => {}
                        Err(e) => diagnostics.push(format!("{e:?}")),
                    }
                }
            }
        }
    }
    drop(inflight);
    host.finish_run(&run_id);

    stats.calls = used.calls;
    stats.model_calls = used.model_calls;
    stats.pages = used.pages;
    stats.elapsed_ms = ms();

    let mut status = RunStatus::Failed;
    let mut data = Value::Null;
    if let (false, Some(done)) = (failed, evaluation) {
        match finish_output(template, &mut runtime, done.value, &provenance) {
            Ok((declared, value)) => {
                status = declared;
                data = value;
            }
            Err(message) => diagnostics.push(message),
        }
    }
    if status == RunStatus::Ready && (stats.denied > 0 || stats.failed > 0 || expired) {
        status = RunStatus::Partial;
        diagnostics
            .push("status: ready downgraded to partial after refused or failed calls".into());
    }

    let referenced: BTreeSet<&str> = json::strings(&data).into_iter().collect();
    let attached = if manifest.provenance {
        provenance_order
            .iter()
            .filter_map(|id| provenance.get(id))
            .filter(|p| referenced.contains(p.id.as_str()) || referenced.contains(p.url.as_str()))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    let mut result = RunResult {
        run_id,
        app_id: app.app_id.clone(),
        template: TemplateRef {
            id: manifest.id.clone(),
            version: manifest.version.clone(),
            digest: template.digest.clone(),
        },
        status,
        data,
        provenance: attached,
        diagnostics,
        stats,
        trace,
        started_at,
        result_path: None,
    };
    if options.write_result {
        write_result(&app, &mut result)?;
    }
    Ok(result)
}

/// Converts, checks and validates the script's return value: `{status, data}`
/// with `data` matching the output schema and every URL in it host-kept.
fn finish_output(
    template: &Template,
    runtime: &mut CapabilityRuntime,
    value: octoscript_core::vm::ScriptValue,
    provenance: &BTreeMap<String, Provenance>,
) -> std::result::Result<(RunStatus, Value), String> {
    let output = runtime
        .script_value_as_json(value, MAX_OUTPUT_BYTES, 32)
        .map_err(|e| format!("output: {e:?}"))?;
    let Some(fields) = output.as_object() else {
        return Err("output: a template returns {status, data}".into());
    };
    if fields.len() != 2 || !fields.contains_key("data") {
        return Err("output: a template returns exactly {status, data}".into());
    }
    let status = match fields.get("status").and_then(Value::as_str) {
        Some("ready") => RunStatus::Ready,
        Some("partial") => RunStatus::Partial,
        other => {
            return Err(format!(
                "output: status must be ready or partial, not {other:?}"
            ))
        }
    };
    let data = fields["data"].clone();
    let schema =
        JsonSchema::compile(template.manifest.output.clone()).map_err(|e| e.to_string())?;
    schema
        .validate(&data)
        .map_err(|e| format!("output schema: {e}"))?;
    let urls: BTreeSet<&str> = provenance.values().map(|p| p.url.as_str()).collect();
    if let Some(bad) = json::strings(&data)
        .into_iter()
        .find(|s| json::contains_url(s) && !urls.contains(s))
    {
        return Err(format!(
            "output: {bad:?} is a URL the host did not retrieve; URLs come only from host provenance"
        ));
    }
    Ok((status, data))
}

fn write_result(app: &AppContext, result: &mut RunResult) -> Result<()> {
    let relative = format!("toolbox/runs/{}/{}.json", result.template.id, result.run_id);
    let dir = app.runs_dir().join(&result.template.id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| Error::new(ErrorKind::Io, format!("{}: {e}", dir.display())))?;
    result.result_path = Some(relative);
    let path = dir.join(format!("{}.json", result.run_id));
    let text = serde_json::to_string_pretty(result)
        .map_err(|e| Error::new(ErrorKind::Io, e.to_string()))?;
    std::fs::write(&path, text)
        .map_err(|e| Error::new(ErrorKind::Io, format!("{}: {e}", path.display())))
}

/// Fills absent top-level parameters from their schema `default`. A script
/// reading an absent field fails, so every optional parameter has one (the
/// manifest check requires it).
pub fn with_defaults(schema: &Value, params: Value) -> Value {
    let Value::Object(mut fields) = params else {
        return params;
    };
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (name, property) in properties {
            if let (false, Some(default)) = (fields.contains_key(name), property.get("default")) {
                fields.insert(name.clone(), default.clone());
            }
        }
    }
    Value::Object(fields)
}
