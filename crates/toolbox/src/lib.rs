//! The OctoSense system toolbox's **workflow template library** (ADR 0002,
//! section 6, "Workflow templates").
//!
//! A template is OctoScript source plus a manifest (`template.json`): its
//! parameters, the host modules and methods it may call, a budget and an
//! output schema. This crate:
//!
//! - loads and pins templates by content digest and statically checks their
//!   source ([`Library`], [`check`]);
//! - runs one in the bounded OctoScript VM with **only** its declared host
//!   modules installed, dispatching every module call to a [`ToolboxHost`]
//!   with the calling app's identity, grants, scope and budget, concurrently up
//!   to the template's `max_concurrency` ([`run`]);
//! - defines `mod.research` v1 ([`research`]) with a fixture backend for tests
//!   and evaluation ([`fixture`]) and, behind the `live` feature, a small
//!   interim adapter over public feeds (`research::live`) and, behind the
//!   `octos-engine` feature, the octos research engine with headless Chrome
//!   (`research::octos`);
//! - forks a template into an app's folder, refusing any widening of its
//!   modules, methods or budget ([`fork`]), and evaluates two templates on the
//!   same recorded inputs ([`evaluate`]);
//! - exposes the `workflow.list`, `workflow.run`, `workflow.fork` and
//!   `workflow.evaluate` tool surface as a Rust API with JSON request and
//!   response types ([`api`]), ready for host routing through octos peer tools.

pub mod api;
pub mod check;
pub mod evaluate;
pub mod fixture;
pub mod fork;
pub mod host;
pub mod json;
pub mod library;
pub mod manifest;
pub mod modules;
pub mod research;
pub mod runner;

pub use api::Toolbox;
pub use evaluate::{evaluate, EvalCase, EvaluationReport, QualityScorer, Verdict, VerdictOutcome};
pub use fork::fork;
pub use host::{
    AppContext, CallContext, HostError, HostReply, Provenance, Scope, ToolboxHost, Usage,
};
pub use library::{Library, Template, TemplateOrigin};
pub use manifest::{Budget, Lineage, Manifest, ModuleDecl};
pub use runner::{run, RunOptions, RunResult, RunStatus};

/// Any error the toolbox reports to its caller: a refused template, a refused
/// fork, invalid parameters, an I/O failure writing a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// The manifest is malformed or out of bounds.
    Manifest,
    /// The source failed the static check (syntax, an undeclared module or
    /// method, size caps).
    Check,
    /// A fork declares more than its parent (modules, methods, budget).
    Widening,
    /// The template's digest does not match its pin.
    Pin,
    /// No such template.
    NotFound,
    /// The calling app lacks a grant the template needs.
    NotGranted,
    /// The parameters do not match the template's schema.
    Params,
    /// The VM could not be set up.
    Runtime,
    /// Reading or writing a template or result.
    Io,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
