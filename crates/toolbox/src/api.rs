//! The toolbox tool surface: `workflow.list`, `workflow.run`, `workflow.fork`
//! and `workflow.evaluate`, as a Rust API and as JSON requests and responses
//! a host can route from octos peer tools (octos#2567). Every call names the
//! calling app ([`AppContext`]); the host fills it from the peer's identity
//! and the app's App Hub grants, never from the model.

use crate::evaluate::{evaluate, EvalCase, EvaluationReport, NoScorer, QualityScorer};
use crate::fork::{self, lineage_current, list_forks};
use crate::library::{Library, Template, TemplateOrigin};
use crate::manifest::{Budget, Lineage, ModuleDecl};
use crate::runner::{run, RunOptions, RunResult};
use crate::{AppContext, Error, ErrorKind, Result, ToolboxHost};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One template as `workflow.list` shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateSummary {
    pub id: String,
    pub version: String,
    pub title: String,
    pub description: String,
    pub digest: String,
    /// `library` or `fork`.
    pub origin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage: Option<Lineage>,
    /// For a fork: whether its parent still has the digest it was copied from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage_current: Option<bool>,
    pub params: Value,
    pub output: Value,
    pub modules: Vec<ModuleDecl>,
    pub budget: Budget,
    /// The app holds every grant the template needs.
    pub runnable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListResponse {
    pub templates: Vec<TemplateSummary>,
    /// Forks in the app's folder that failed to load, with the reason.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refused: Vec<Refusal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Refusal {
    pub id: String,
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRequest {
    pub id: String,
    #[serde(default = "empty_object")]
    pub params: Value,
    #[serde(default)]
    pub run_id: Option<String>,
}

fn empty_object() -> Value {
    json!({})
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForkRequest {
    pub id: String,
    #[serde(default)]
    pub new_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForkResponse {
    pub template: TemplateSummary,
    /// Where the fork lives, relative to the app folder.
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluateRequest {
    /// The baseline (usually the parent).
    pub a: String,
    /// The candidate (usually the fork).
    pub b: String,
    pub cases: Vec<EvalCase>,
}

/// A JSON tool call, tagged by tool name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tool", content = "arguments")]
pub enum WorkflowRequest {
    #[serde(rename = "workflow.list")]
    List {},
    #[serde(rename = "workflow.run")]
    Run(RunRequest),
    #[serde(rename = "workflow.fork")]
    Fork(ForkRequest),
    #[serde(rename = "workflow.evaluate")]
    Evaluate(EvaluateRequest),
}

/// Most cases one `workflow.evaluate` call may carry.
pub const MAX_EVAL_CASES: usize = 32;

/// The toolbox for one host: the library plus the engine that executes
/// module calls.
pub struct Toolbox<H> {
    library: Library,
    host: H,
    scorer: Box<dyn QualityScorer>,
}

impl<H: ToolboxHost> Toolbox<H> {
    pub fn new(library: Library, host: H) -> Self {
        Self {
            library,
            host,
            scorer: Box::new(NoScorer),
        }
    }

    pub fn with_scorer(mut self, scorer: impl QualityScorer + 'static) -> Self {
        self.scorer = Box::new(scorer);
        self
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    fn summary(&self, app: &AppContext, template: &Template) -> TemplateSummary {
        let m = &template.manifest;
        TemplateSummary {
            id: m.id.clone(),
            version: m.version.clone(),
            title: m.title.clone(),
            description: m.description.clone(),
            digest: template.digest.clone(),
            origin: match template.origin {
                TemplateOrigin::Library => "library".into(),
                TemplateOrigin::Fork { .. } => "fork".into(),
            },
            lineage: m.lineage.clone(),
            lineage_current: lineage_current(&self.library, template),
            params: m.params.clone(),
            output: m.output.clone(),
            modules: m.modules.clone(),
            budget: crate::runner::effective_budget(&m.budget, app),
            runnable: m.required_grants().iter().all(|g| app.grants.contains(*g)),
        }
    }

    /// Resolves an id: the app's forks first, then the library.
    pub fn resolve(&self, app: &AppContext, id: &str) -> Result<Template> {
        if !crate::manifest::valid_id(id) {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("no template {id:?}"),
            ));
        }
        let dir = app.templates_dir().join(id);
        if dir.is_dir() {
            return fork::load_fork(&self.library, &dir);
        }
        self.library
            .get(id)
            .cloned()
            .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("no template {id}")))
    }

    /// `workflow.list(app)`: the library and the app's forks.
    pub fn list(&self, app: &AppContext) -> ListResponse {
        let mut templates: Vec<TemplateSummary> = self
            .library
            .templates()
            .map(|t| self.summary(app, t))
            .collect();
        let mut refused = Vec::new();
        for (name, loaded) in list_forks(&self.library, &app.templates_dir()) {
            match loaded {
                Ok(t) => templates.push(self.summary(app, &t)),
                Err(e) => refused.push(Refusal {
                    id: name,
                    kind: e.kind,
                    message: e.message,
                }),
            }
        }
        ListResponse { templates, refused }
    }

    /// `workflow.run(app, id, params)`: runs and writes the result into the
    /// app's folder.
    pub async fn run(&self, app: &AppContext, request: RunRequest) -> Result<RunResult> {
        let template = self.resolve(app, &request.id)?;
        run(
            &template,
            app,
            request.params,
            &self.host,
            RunOptions {
                run_id: request.run_id,
                write_result: true,
            },
        )
        .await
    }

    /// `workflow.fork(app, id)`: copies a library template into the app's
    /// folder with lineage.
    pub fn fork(&self, app: &AppContext, request: ForkRequest) -> Result<ForkResponse> {
        let template = fork::fork(
            &self.library,
            &request.id,
            &app.templates_dir(),
            request.new_id.as_deref(),
        )?;
        Ok(ForkResponse {
            path: format!("toolbox/templates/{}", template.manifest.id),
            template: self.summary(app, &template),
        })
    }

    /// `workflow.evaluate(app, a, b, cases)`: both templates on the same
    /// recorded inputs; a verdict for adopting `b`.
    pub async fn evaluate(
        &self,
        app: &AppContext,
        request: EvaluateRequest,
    ) -> Result<EvaluationReport> {
        if request.cases.is_empty() || request.cases.len() > MAX_EVAL_CASES {
            return Err(Error::new(
                ErrorKind::Params,
                format!("1–{MAX_EVAL_CASES} cases"),
            ));
        }
        let a = self.resolve(app, &request.a)?;
        let b = self.resolve(app, &request.b)?;
        evaluate(&a, &b, &request.cases, app, self.scorer.as_ref()).await
    }

    /// Routes one JSON tool call. Errors come back as
    /// `{"error": {"kind", "message"}}`.
    pub async fn handle_json(&self, app: &AppContext, request: Value) -> Value {
        let request: WorkflowRequest = match serde_json::from_value(request) {
            Ok(r) => r,
            Err(e) => return error_json(&Error::new(ErrorKind::Params, e.to_string())),
        };
        let result = match request {
            WorkflowRequest::List {} => serde_json::to_value(self.list(app)).map_err(to_err),
            WorkflowRequest::Run(r) => self
                .run(app, r)
                .await
                .and_then(|v| serde_json::to_value(v).map_err(to_err)),
            WorkflowRequest::Fork(r) => self
                .fork(app, r)
                .and_then(|v| serde_json::to_value(v).map_err(to_err)),
            WorkflowRequest::Evaluate(r) => self
                .evaluate(app, r)
                .await
                .and_then(|v| serde_json::to_value(v).map_err(to_err)),
        };
        result.unwrap_or_else(|e| error_json(&e))
    }
}

fn to_err(e: serde_json::Error) -> Error {
    Error::new(ErrorKind::Io, e.to_string())
}

fn error_json(e: &Error) -> Value {
    json!({"error": {"kind": e.kind, "message": e.message}})
}

/// The four tools' descriptors (name, description, input schema) for host
/// registration with a peer's tool set.
pub fn tool_descriptors() -> Value {
    json!([
        {"name": "workflow.list", "risk": "read",
         "description": "List the toolbox workflow templates and this app's forks, with parameters, outputs and budgets.",
         "input_schema": {"type": "object", "properties": {}, "additionalProperties": false}},
        {"name": "workflow.run", "risk": "read",
         "description": "Run a workflow template with parameters; the result (with sources) is written to this app's folder.",
         "input_schema": {"type": "object", "required": ["id"], "additionalProperties": false,
            "properties": {"id": {"type": "string"}, "params": {"type": "object"}, "run_id": {"type": "string"}}}},
        {"name": "workflow.fork", "risk": "act_in_app",
         "description": "Copy a library template into this app's folder to edit it; it keeps lineage to its parent.",
         "input_schema": {"type": "object", "required": ["id"], "additionalProperties": false,
            "properties": {"id": {"type": "string"}, "new_id": {"type": "string"}}}},
        {"name": "workflow.evaluate", "risk": "read",
         "description": "Run two templates on the same recorded inputs and say whether the second is better, worse or equal.",
         "input_schema": {"type": "object", "required": ["a", "b", "cases"], "additionalProperties": false,
            "properties": {"a": {"type": "string"}, "b": {"type": "string"}, "cases": {"type": "array"}}}}
    ])
}
