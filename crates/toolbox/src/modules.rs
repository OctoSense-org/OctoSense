//! The host modules a template may declare, with each method's executable
//! input and output contract. The runner registers exactly these contracts in
//! the VM for the methods a template declares, so a call with a malformed
//! input never reaches the host and a malformed host reply never reaches the
//! script.

use serde_json::{json, Value};

/// How a method is charged against the budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    /// Runs a model: one `max_model_calls` each.
    Model,
    /// Reads one article: one `max_reads` each, so `max_reads` is the number
    /// of articles a run may try to read.
    Read,
    /// Queries sources: one call. The feeds and API responses the backend
    /// fetched are reported (`RunStats::search_fetches`) but not charged to
    /// `max_reads`: how many providers a backend queries is its own
    /// configuration (one for the fixture, four for the interim adapter), and
    /// charging it would make the same template read fewer articles on a
    /// broader backend. The fan-out is capped per search by
    /// `research::MAX_SEARCH_FETCHES`.
    Search,
}

pub struct MethodSpec {
    pub name: &'static str,
    pub kind: MethodKind,
    pub description: &'static str,
    pub input: fn() -> Value,
    pub output: fn() -> Value,
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
}

pub struct ModuleSpec {
    pub name: &'static str,
    /// The App Hub capability an app must be granted to run a template that
    /// declares this module.
    pub capability: &'static str,
    pub description: &'static str,
    /// Reads external sources: its results carry host-kept provenance.
    pub external_sources: bool,
    pub methods: &'static [MethodSpec],
}

impl ModuleSpec {
    pub fn method(&self, name: &str) -> Option<&'static MethodSpec> {
        self.methods.iter().find(|m| m.name == name)
    }
}

/// Every host module the toolbox knows. v1: `mod.research`.
pub static MODULES: &[ModuleSpec] = &[RESEARCH];

pub fn spec(name: &str) -> Option<&'static ModuleSpec> {
    MODULES.iter().find(|m| m.name == name)
}

/// Standard-library modules a template may import (`use mod.std.<name>`).
/// They are pure and part of the OctoScript profile, not host modules.
pub const STD_MODULES: &[&str] = &["array", "assert", "json", "math", "object", "text"];

/// Digest tasks: the host owns each task's prompt; a template picks one.
pub const DIGEST_TASKS: &[&str] = &["digest", "brief", "plan", "compare"];

fn string(min: usize, max: usize) -> Value {
    json!({"type": "string", "minLength": min, "maxLength": max})
}

fn search_item() -> Value {
    json!({"type": "object", "additionalProperties": false,
    "required": ["id", "title", "url", "source", "language", "published_at", "readable"],
    "properties": {
        "id": string(1, 64),
        "title": string(0, 400),
        "url": string(1, 2048),
        "source": string(0, 200),
        "language": string(0, 16),
        "published_at": string(0, 40),
        "readable": {"type": "boolean"}
    }})
}

/// `mod.research` v1: search, read and digest, executed by the host with the
/// calling app's scope. Article ids come only from this run's search results;
/// digests take only articles read in this run; URLs and evidence hashes come
/// only from the host.
pub const RESEARCH: ModuleSpec = ModuleSpec {
    name: "research",
    capability: "research",
    description: "Search, read and digest external sources under the calling app's research scope",
    external_sources: true,
    methods: &[
        MethodSpec {
            name: "query",
            kind: MethodKind::Model,
            description: "Translate a query into search terms in `language` (one model call)",
            input: || {
                json!({"type": "object", "additionalProperties": false, "required": ["query"],
                    "properties": {"query": string(1, 160), "language": string(2, 16)}})
            },
            output: || {
                json!({"type": "object", "additionalProperties": false, "required": ["query", "language"],
                    "properties": {"query": string(1, 160), "language": string(2, 16)}})
            },
            max_input_bytes: 1024,
            max_output_bytes: 1024,
        },
        MethodSpec {
            name: "search",
            kind: MethodKind::Search,
            description: "One query through the provider chain (the metasearch `category`, `news` by default, must be in the app's grant; `max_age_hours` is clamped to its `max_age_days`); structured items with host-assigned ids, readable items first (`readable: false` marks one the backend cannot read)",
            input: || {
                json!({"type": "object", "additionalProperties": false, "required": ["topic"],
                    "properties": {
                        "topic": string(1, 160),
                        "language": string(2, 16),
                        "region": string(2, 8),
                        "limit": {"type": "integer", "minimum": 1, "maximum": 10},
                        "max_age_hours": {"type": "integer", "minimum": 1, "maximum": 36000},
                        "category": {"type": "string", "enum": ["news", "general", "it", "social", "science"]}
                    }})
            },
            output: || {
                json!({"type": "object", "additionalProperties": false, "required": ["items", "source"],
                    "properties": {
                        "items": {"type": "array", "maxItems": 10, "items": search_item()},
                        "source": {"type": "object", "additionalProperties": false,
                            "required": ["partial", "providers", "queried_at"],
                            "properties": {
                                "partial": {"type": "boolean"},
                                "providers": {"type": "array", "maxItems": 8, "items": string(1, 64)},
                                "queried_at": string(1, 40)
                            }}
                    }})
            },
            max_input_bytes: 1024,
            max_output_bytes: 16 * 1024,
        },
        MethodSpec {
            name: "article",
            kind: MethodKind::Read,
            description: "Read one search result's page and keep its main text as evidence; `resolved_url` is where the read ended (a Google News link's publisher page, null while unresolved) and `publisher_host` its host; `on_topic: false` when the page does not mention the terms of the search that found it (the host will not digest it)",
            input: || {
                json!({"type": "object", "additionalProperties": false, "required": ["id"],
                    "properties": {"id": string(1, 64)}})
            },
            output: || {
                json!({"type": "object", "additionalProperties": false,
                    "required": ["id", "title", "url", "resolved_url", "publisher_host", "source", "language",
                        "published_at", "excerpt", "chars", "truncated", "evidence_sha256", "on_topic"],
                    "properties": {
                        "id": string(1, 64),
                        "title": string(0, 400),
                        "url": string(1, 2048),
                        "resolved_url": {"description": "Where the read ended after redirects (a Google News link's publisher page); null when unknown or still on the aggregator"},
                        "publisher_host": {"description": "The publisher's host (resolved_url's, else url's unless an aggregator's), or null"},
                        "source": string(0, 200),
                        "language": string(0, 16),
                        "published_at": string(0, 40),
                        "excerpt": string(0, 600),
                        "chars": {"type": "integer", "minimum": 0},
                        "truncated": {"type": "boolean"},
                        "evidence_sha256": string(64, 64),
                        "on_topic": {"type": "boolean"}
                    }})
            },
            max_input_bytes: 256,
            max_output_bytes: 8 * 1024,
        },
        MethodSpec {
            name: "digest",
            kind: MethodKind::Model,
            description: "Summarize articles read in this run, in `language`, with citations by article id (one model call). Off-topic articles are left out; for the `digest` task with a `focus`, the model also lists in `off_topic` the articles it found not about the focus, and nothing cites them. Invalid points are dropped (`dropped_points`); summary sentences no kept point backs are dropped and those carrying a number, name or phrase no point carries are flagged (`summary_check`)",
            input: || {
                json!({"type": "object", "additionalProperties": false,
                    "required": ["task", "language", "article_ids"],
                    "properties": {
                        "task": {"type": "string", "enum": DIGEST_TASKS},
                        "language": string(2, 16),
                        "article_ids": {"type": "array", "minItems": 1, "maxItems": 8, "items": string(1, 64)},
                        "focus": string(0, 280)
                    }})
            },
            output: || {
                json!({"type": "object", "additionalProperties": false,
                    "required": ["task", "language", "summary", "points", "off_topic", "dropped_points", "summary_check"],
                    "properties": {
                        "task": {"type": "string", "enum": DIGEST_TASKS},
                        "language": string(2, 16),
                        "summary": string(1, 1200),
                        "points": {"type": "array", "minItems": 1, "maxItems": 12, "items": {
                            "type": "object", "additionalProperties": false,
                            "required": ["text", "citations"],
                            "properties": {
                                "text": string(1, 400),
                                "citations": {"type": "array", "minItems": 1, "maxItems": 8, "items": string(1, 64)},
                                "label": string(0, 40)
                            }}},
                        "off_topic": {"type": "array", "maxItems": 8, "items": string(1, 64)},
                        "dropped_points": {"type": "integer", "minimum": 0},
                        "summary_check": {"type": "object", "additionalProperties": false,
                            "required": ["sentences", "dropped", "flagged"],
                            "properties": {
                                "sentences": {"type": "integer", "minimum": 0},
                                "dropped": {"type": "integer", "minimum": 0},
                                "flagged": {"type": "integer", "minimum": 0}
                            }}
                    }})
            },
            max_input_bytes: 2048,
            max_output_bytes: 32 * 1024,
        },
    ],
};

#[cfg(test)]
mod tests {
    use super::*;
    use octoscript_schema::JsonSchema;

    #[test]
    fn contracts_compile() {
        for module in MODULES {
            for method in module.methods {
                JsonSchema::compile((method.input)()).unwrap();
                JsonSchema::compile((method.output)()).unwrap();
            }
        }
    }
}
