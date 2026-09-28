//! `template.json`: what a template is allowed to do.

use crate::{modules, Error, ErrorKind, Result};
use octoscript_schema::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

/// Largest accepted `template.json`.
pub const MAX_MANIFEST_BYTES: usize = 32 * 1024;
/// Largest accepted `template.octoscript`.
pub const MAX_SOURCE_BYTES: usize = 32 * 1024;

/// The ceiling every template's budget must stay under; an app's own budget
/// and a fork's parent can only narrow it.
pub const BUDGET_CEILING: Budget = Budget {
    max_calls: 64,
    max_model_calls: 8,
    max_reads: 32,
    max_ms: 300_000,
    max_concurrency: 8,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub title: String,
    pub description: String,
    /// JSON Schema (OctoScript's executable subset) of the parameters; the
    /// script sees them as `request`.
    pub params: Value,
    pub modules: Vec<ModuleDecl>,
    pub budget: Budget,
    /// JSON Schema of the `data` the template returns (its result is
    /// `{status: "ready" | "partial", data}`).
    pub output: Value,
    /// Host-kept provenance is attached to the result. Required for every
    /// template that reads external sources.
    pub provenance: bool,
    /// Present on a fork: the template it was copied from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage: Option<Lineage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleDecl {
    pub module: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    /// Host module calls in one run, all methods together.
    pub max_calls: u32,
    /// Calls that run a model (`research.query`, `research.digest`).
    pub max_model_calls: u32,
    /// Articles read (`research.article`), whether or not the read
    /// succeeds. Searches are not charged to it. Not the `crawl` scope's
    /// `max_pages`.
    pub max_reads: u32,
    /// Wall-clock limit of one run.
    pub max_ms: u64,
    /// Host calls in flight at once.
    pub max_concurrency: u32,
}

impl Budget {
    /// The narrower of two budgets, field by field.
    pub fn min(self, other: Budget) -> Budget {
        Budget {
            max_calls: self.max_calls.min(other.max_calls),
            max_model_calls: self.max_model_calls.min(other.max_model_calls),
            max_reads: self.max_reads.min(other.max_reads),
            max_ms: self.max_ms.min(other.max_ms),
            max_concurrency: self.max_concurrency.min(other.max_concurrency),
        }
    }

    /// The fields where `self` exceeds `limit`.
    pub fn exceeds(&self, limit: &Budget) -> Vec<&'static str> {
        let mut over = Vec::new();
        if self.max_calls > limit.max_calls {
            over.push("max_calls");
        }
        if self.max_model_calls > limit.max_model_calls {
            over.push("max_model_calls");
        }
        if self.max_reads > limit.max_reads {
            over.push("max_reads");
        }
        if self.max_ms > limit.max_ms {
            over.push("max_ms");
        }
        if self.max_concurrency > limit.max_concurrency {
            over.push("max_concurrency");
        }
        over
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lineage {
    pub parent_id: String,
    pub parent_version: String,
    pub parent_digest: String,
}

fn manifest_err(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::Manifest, message)
}

/// Template ids: lowercase ASCII letters, digits, `-` and `.`, 1–64 bytes,
/// starting with a letter. They name directories, so nothing else.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
        && !id.contains("..")
}

impl Manifest {
    /// Parses and validates `template.json`.
    pub fn parse(bytes: &[u8]) -> Result<Manifest> {
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(manifest_err(format!(
                "template.json is {} bytes; the limit is {MAX_MANIFEST_BYTES}",
                bytes.len()
            )));
        }
        let manifest: Manifest = serde_json::from_slice(bytes)
            .map_err(|e| manifest_err(format!("template.json: {e}")))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        if !valid_id(&self.id) {
            return Err(manifest_err(format!("invalid template id {:?}", self.id)));
        }
        if self.version.is_empty() || self.version.len() > 32 {
            return Err(manifest_err("version must be 1–32 bytes"));
        }
        if self.title.is_empty() || self.title.len() > 120 {
            return Err(manifest_err("title must be 1–120 bytes"));
        }
        if self.description.len() > 2000 {
            return Err(manifest_err("description is longer than 2000 bytes"));
        }
        for (name, schema) in [("params", &self.params), ("output", &self.output)] {
            if schema.get("type").and_then(Value::as_str) != Some("object") {
                return Err(manifest_err(format!("{name} must be an object schema")));
            }
            JsonSchema::compile(schema.clone())
                .map_err(|e| manifest_err(format!("{name} schema: {e}")))?;
        }
        let required: BTreeSet<&str> = self.params["required"]
            .as_array()
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if let Some(properties) = self.params["properties"].as_object() {
            for (name, property) in properties {
                if !required.contains(name.as_str()) && property.get("default").is_none() {
                    return Err(manifest_err(format!(
                        "params.{name} is optional without a default; a script cannot read an absent field"
                    )));
                }
            }
        }
        if self.modules.is_empty() {
            return Err(manifest_err("a template declares at least one host module"));
        }
        let mut seen = BTreeSet::new();
        for decl in &self.modules {
            let Some(spec) = modules::spec(&decl.module) else {
                return Err(manifest_err(format!(
                    "unknown host module mod.{}",
                    decl.module
                )));
            };
            if !seen.insert(decl.module.as_str()) {
                return Err(manifest_err(format!(
                    "mod.{} is declared twice",
                    decl.module
                )));
            }
            if decl.methods.is_empty() {
                return Err(manifest_err(format!(
                    "mod.{} declares no methods",
                    decl.module
                )));
            }
            let mut methods = BTreeSet::new();
            for method in &decl.methods {
                if spec.method(method).is_none() {
                    return Err(manifest_err(format!(
                        "mod.{} has no method {method}",
                        decl.module
                    )));
                }
                if !methods.insert(method) {
                    return Err(manifest_err(format!(
                        "{}.{method} is declared twice",
                        decl.module
                    )));
                }
            }
            if spec.external_sources && !self.provenance {
                return Err(manifest_err(format!(
                    "mod.{} reads external sources: provenance must be true",
                    decl.module
                )));
            }
        }
        let b = &self.budget;
        if b.max_calls == 0 || b.max_ms == 0 || b.max_concurrency == 0 {
            return Err(manifest_err(
                "budget: max_calls, max_ms and max_concurrency must be positive",
            ));
        }
        let over = b.exceeds(&BUDGET_CEILING);
        if !over.is_empty() {
            return Err(manifest_err(format!(
                "budget exceeds the toolbox ceiling: {}",
                over.join(", ")
            )));
        }
        if let Some(lineage) = &self.lineage {
            if !valid_id(&lineage.parent_id) || lineage.parent_digest.len() != 64 {
                return Err(manifest_err("invalid lineage"));
            }
        }
        Ok(())
    }

    /// Whether this manifest declares `module.method`.
    pub fn declares(&self, module: &str, method: &str) -> bool {
        self.modules
            .iter()
            .any(|d| d.module == module && d.methods.iter().any(|m| m == method))
    }

    /// Every `module.method` this manifest declares.
    pub fn declared_methods(&self) -> BTreeSet<(String, String)> {
        self.modules
            .iter()
            .flat_map(|d| d.methods.iter().map(|m| (d.module.clone(), m.clone())))
            .collect()
    }

    /// The capabilities (App Hub grants) the declared modules require.
    pub fn required_grants(&self) -> BTreeSet<&'static str> {
        self.modules
            .iter()
            .filter_map(|d| modules::spec(&d.module).map(|s| s.capability))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert!(valid_id("news-digest"));
        assert!(valid_id("news-digest.fork"));
        assert!(!valid_id("../x"));
        assert!(!valid_id("News"));
        assert!(!valid_id("a..b"));
        assert!(!valid_id(""));
    }
}
