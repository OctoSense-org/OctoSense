//! The boundary between the runner and whoever executes module calls: the
//! shell's AI host in production, [`crate::research::ResearchHost`] over a
//! fixture or live backend here.

use crate::manifest::Budget;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

/// A future a host returns. Not `Send`: the VM stays on the runner's thread,
/// and host calls are polled there too.
pub type HostFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// The calling app: who is asking, what it was granted, within what scope,
/// and where its results go.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppContext {
    /// The app's id (for example `os.news`); every call is charged to it.
    pub app_id: String,
    /// App Hub capabilities granted to the app (`research`, `crawl`, …).
    pub grants: BTreeSet<String>,
    /// The research scope granted with those capabilities.
    #[serde(default)]
    pub scope: Scope,
    /// The app's own budget per run, if narrower than the templates'.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<Budget>,
    /// The app's folder: forks live in `toolbox/templates/`, results in
    /// `toolbox/runs/`.
    pub folder: PathBuf,
}

impl AppContext {
    pub fn new(app_id: impl Into<String>, folder: impl Into<PathBuf>) -> Self {
        Self {
            app_id: app_id.into(),
            grants: BTreeSet::new(),
            scope: Scope::default(),
            budget: None,
            folder: folder.into(),
        }
    }

    pub fn grant(mut self, capability: impl Into<String>) -> Self {
        self.grants.insert(capability.into());
        self
    }

    pub fn with_scope(mut self, scope: Scope) -> Self {
        self.scope = scope;
        self
    }

    pub fn with_budget(mut self, budget: Budget) -> Self {
        self.budget = Some(budget);
        self
    }

    pub fn templates_dir(&self) -> PathBuf {
        self.folder.join("toolbox").join("templates")
    }

    pub fn runs_dir(&self) -> PathBuf {
        self.folder.join("toolbox").join("runs")
    }
}

/// The research scope App Hub pinned and the person or store granted (ADR
/// 0002 section 6, "Granted per app, scoped"). Empty lists mean no limit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scope {
    pub languages: Vec<String>,
    pub regions: Vec<String>,
    pub allowed_domains: Vec<String>,
    pub denied_domains: Vec<String>,
    pub max_depth: Option<u32>,
    pub max_pages: Option<u32>,
    /// Oldest item age, in hours.
    pub recency_hours: Option<u32>,
}

impl Scope {
    fn domain_matches(host: &str, domain: &str) -> bool {
        let domain = domain.trim_start_matches('.').to_ascii_lowercase();
        let host = host.to_ascii_lowercase();
        host == domain || host.ends_with(&format!(".{domain}"))
    }

    /// Whether a URL's host is within the allowed and outside the denied
    /// domains.
    pub fn allows_url(&self, url: &str) -> bool {
        let Some(host) = url_host(url) else {
            return false;
        };
        if self
            .denied_domains
            .iter()
            .any(|d| Self::domain_matches(&host, d))
        {
            return false;
        }
        self.allowed_domains.is_empty()
            || self
                .allowed_domains
                .iter()
                .any(|d| Self::domain_matches(&host, d))
    }

    pub fn allows_language(&self, language: &str) -> bool {
        self.languages.is_empty()
            || self
                .languages
                .iter()
                .any(|l| l.eq_ignore_ascii_case(language))
    }

    pub fn allows_region(&self, region: &str) -> bool {
        self.regions.is_empty() || self.regions.iter().any(|r| r.eq_ignore_ascii_case(region))
    }
}

/// The host part of an `http(s)` URL, lowercased, without port or userinfo.
pub fn url_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?.split(':').next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// What is left of the run's budget when a call is dispatched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remaining {
    pub calls: u32,
    pub model_calls: u32,
    /// Pages (article reads) left.
    pub pages: u32,
    pub ms: u64,
}

/// Passed with every module call: the app it is charged to, its grants and
/// scope (in `app`), the template and run, and the budget left.
#[derive(Debug, Clone)]
pub struct CallContext {
    pub app: Arc<AppContext>,
    pub run_id: String,
    pub template_id: String,
    pub template_digest: String,
    /// The run's effective budget: the template's, narrowed by the app's.
    pub budget: Budget,
    pub remaining: Remaining,
    /// 0-based index of this call within the run.
    pub call_index: usize,
}

/// What a host call used, as the host reports it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub model_calls: u32,
    /// For `article`: pages read, charged to `max_pages`. For `search`: the
    /// feeds and API responses the backend fetched, reported in
    /// `RunStats::search_fetches` and not charged to `max_pages`.
    pub pages: u32,
}

/// One source the host read or listed, kept by the host outside the script.
/// Attached to a run's result for every id or URL its data refers to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// The host-assigned item id the script refers to.
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub published_at: String,
    /// When the host listed or read it (RFC 3339).
    pub retrieved_at: String,
    /// SHA-256 of the evidence text the host kept, once read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_sha256: Option<String>,
    /// The provider or method that produced it (`search:gdelt`, `article`).
    pub via: String,
}

/// A successful host call.
#[derive(Debug, Clone, PartialEq)]
pub struct HostReply {
    /// The value the script's `await()` returns; checked against the method's
    /// output contract before the script sees it.
    pub output: Value,
    pub provenance: Vec<Provenance>,
    pub usage: Usage,
    /// Things the host corrected or dropped while still succeeding (a digest
    /// point that failed validation, say). The runner adds each to the run's
    /// diagnostics.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// Refused by policy (scope, grant, unknown id).
    Denied(String),
    Failed(String),
    TimedOut(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Denied(m) => write!(f, "denied: {m}"),
            Self::Failed(m) => write!(f, "failed: {m}"),
            Self::TimedOut(m) => write!(f, "timed out: {m}"),
        }
    }
}

/// Executes module calls for the runner. Implemented by the caller: the shell
/// or AI host supplies the real engines; tests use the fixture backend.
pub trait ToolboxHost {
    /// Runs `module.method(input)` for the app in `ctx`. `input` has already
    /// passed the method's input contract.
    fn call<'a>(
        &'a self,
        ctx: CallContext,
        module: &'a str,
        method: &'a str,
        input: Value,
    ) -> HostFuture<'a, Result<HostReply, HostError>>;

    /// Called once when a run ends, to drop per-run state.
    fn finish_run(&self, _run_id: &str) {}
}

impl<T: ToolboxHost + ?Sized> ToolboxHost for Arc<T> {
    fn call<'a>(
        &'a self,
        ctx: CallContext,
        module: &'a str,
        method: &'a str,
        input: Value,
    ) -> HostFuture<'a, Result<HostReply, HostError>> {
        (**self).call(ctx, module, method, input)
    }

    fn finish_run(&self, run_id: &str) {
        (**self).finish_run(run_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_and_domains() {
        assert_eq!(
            url_host("https://User@News.Example.org:443/a?b").as_deref(),
            Some("news.example.org")
        );
        assert_eq!(url_host("ftp://x"), None);
        let scope = Scope {
            allowed_domains: vec!["example.org".into()],
            denied_domains: vec!["bad.example.org".into()],
            ..Scope::default()
        };
        assert!(scope.allows_url("https://news.example.org/x"));
        assert!(!scope.allows_url("https://bad.example.org/x"));
        assert!(!scope.allows_url("https://example.com/x"));
        assert!(!scope.allows_url("https://notexample.org/x"));
    }
}
