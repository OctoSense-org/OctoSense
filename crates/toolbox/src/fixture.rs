//! Recorded inputs for `mod.research`: a backend that replays recorded
//! searches and pages (with their delays) and a deterministic extractive
//! model. Tests, the shipped templates' fixtures and [`crate::evaluate`] run
//! on these, so no network or model is needed and both sides of an
//! evaluation see exactly the same inputs.

use crate::host::{CallContext, HostError, HostFuture};
use crate::research::{
    FoundItem, ModelClient, ModelRequest, ModelTask, PageText, ResearchBackend, ResearchHost,
    SearchQuery, SearchResults,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// Everything a fixture run can see.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureData {
    /// The host clock during the run (RFC 3339).
    #[serde(default = "default_now")]
    pub now: String,
    #[serde(default)]
    pub searches: Vec<RecordedSearch>,
    /// Pages by URL.
    #[serde(default)]
    pub pages: BTreeMap<String, RecordedPage>,
    /// Query translations the fake model returns.
    #[serde(default)]
    pub translations: BTreeMap<String, String>,
    #[serde(default)]
    pub model: FakeModelConfig,
}

fn default_now() -> String {
    "2026-09-20T08:00:00Z".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedSearch {
    /// Matched case-insensitively against the query's topic.
    pub topic: String,
    /// When present, the query's language must match.
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub delay_ms: u64,
    #[serde(default)]
    pub partial: bool,
    #[serde(default = "fixture_providers")]
    pub providers: Vec<String>,
    #[serde(default)]
    pub items: Vec<FoundItem>,
    /// Replay a provider failure instead of results.
    #[serde(default)]
    pub error: Option<String>,
}

fn fixture_providers() -> Vec<String> {
    vec!["fixture".into()]
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedPage {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub delay_ms: u64,
    /// Replay a failed read (blocked, no main text, …).
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FakeModelConfig {
    pub delay_ms: u64,
    /// Fail every model call.
    pub fail: bool,
    /// Put a URL into the digest summary (the host must refuse it).
    pub inject_url: bool,
    /// Cite an article id the digest was not given (the host must refuse it).
    pub cite_unknown: bool,
}

/// Replays recorded searches and pages.
pub struct FixtureBackend {
    data: FixtureData,
}

impl FixtureBackend {
    pub fn new(data: FixtureData) -> Self {
        Self { data }
    }
}

impl ResearchBackend for FixtureBackend {
    fn search<'a>(
        &'a self,
        _ctx: &'a CallContext,
        query: SearchQuery,
    ) -> HostFuture<'a, Result<SearchResults, HostError>> {
        Box::pin(async move {
            let recorded = self.data.searches.iter().find(|s| {
                s.topic.eq_ignore_ascii_case(query.topic.trim())
                    && s.language
                        .as_ref()
                        .is_none_or(|l| query.language.as_deref() == Some(l.as_str()))
            });
            let Some(recorded) = recorded else {
                return Err(HostError::Failed(format!(
                    "no recorded search for {:?} ({})",
                    query.topic,
                    query.language.as_deref().unwrap_or("any language")
                )));
            };
            tokio::time::sleep(Duration::from_millis(recorded.delay_ms)).await;
            if let Some(error) = &recorded.error {
                return Err(HostError::Failed(error.clone()));
            }
            Ok(SearchResults {
                items: recorded
                    .items
                    .iter()
                    .map(|i| FoundItem {
                        via: if i.via.is_empty() {
                            "fixture".into()
                        } else {
                            i.via.clone()
                        },
                        ..i.clone()
                    })
                    .collect(),
                providers: recorded.providers.clone(),
                partial: recorded.partial,
                pages: 1,
            })
        })
    }

    fn read<'a>(
        &'a self,
        _ctx: &'a CallContext,
        item: &'a FoundItem,
    ) -> HostFuture<'a, Result<PageText, HostError>> {
        Box::pin(async move {
            let Some(page) = self.data.pages.get(&item.url) else {
                return Err(HostError::Failed(format!(
                    "no recorded page for {}",
                    item.url
                )));
            };
            tokio::time::sleep(Duration::from_millis(page.delay_ms)).await;
            if let Some(error) = &page.error {
                return Err(HostError::Failed(error.clone()));
            }
            Ok(PageText {
                text: page.text.clone(),
                title: page.title.clone(),
            })
        })
    }
}

/// A deterministic, extractive stand-in for the host's model: translations
/// from the fixture, and digests built from each article's first sentence
/// with a citation to it. It measures the workflow, not writing quality.
pub struct FakeModel {
    translations: BTreeMap<String, String>,
    config: FakeModelConfig,
}

impl FakeModel {
    pub fn new(translations: BTreeMap<String, String>, config: FakeModelConfig) -> Self {
        Self {
            translations,
            config,
        }
    }
}

/// The first sentence of `text`, at most 300 bytes.
pub fn first_sentence(text: &str) -> String {
    let text = text.trim();
    let end = text
        .char_indices()
        .find(|&(i, c)| {
            matches!(c, '。' | '！' | '？')
                || (matches!(c, '.' | '!' | '?')
                    && text[i + c.len_utf8()..].starts_with(char::is_whitespace))
        })
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(text.len());
    let mut end = end.min(300);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].trim().to_owned()
}

impl ModelClient for FakeModel {
    fn complete<'a>(
        &'a self,
        _ctx: &'a CallContext,
        request: ModelRequest,
    ) -> HostFuture<'a, Result<String, HostError>> {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(self.config.delay_ms)).await;
            if self.config.fail {
                return Err(HostError::Failed("fake model failure".into()));
            }
            let input: Value = serde_json::from_str(&request.user)
                .map_err(|e| HostError::Failed(e.to_string()))?;
            match request.task {
                ModelTask::TranslateQuery => {
                    let query = input["query"].as_str().unwrap_or_default();
                    let translated = self
                        .translations
                        .get(query)
                        .cloned()
                        .unwrap_or_else(|| query.to_owned());
                    Ok(json!({"query": translated}).to_string())
                }
                ModelTask::Digest => {
                    let articles = input["articles"].as_array().cloned().unwrap_or_default();
                    let titles: Vec<&str> = articles
                        .iter()
                        .filter_map(|a| a["title"].as_str())
                        .collect();
                    let mut summary = format!("Sources: {}", titles.join("; "));
                    if summary.len() > 1100 {
                        summary = first_sentence(&summary);
                    }
                    if self.config.inject_url {
                        summary.push_str(" More at https://model.invalid/story");
                    }
                    let mut points: Vec<Value> = articles
                        .iter()
                        .map(|a| {
                            json!({"text": first_sentence(a["text"].as_str().unwrap_or_default()),
                                "citations": [a["id"].clone()]})
                        })
                        .collect();
                    if self.config.cite_unknown {
                        points.push(json!({"text": "Invented.", "citations": ["s000000000000"]}));
                    }
                    Ok(json!({"summary": summary, "points": points}).to_string())
                }
            }
        })
    }
}

/// A `mod.research` host over recorded inputs.
pub fn host(data: &FixtureData) -> ResearchHost {
    let now = data.now.clone();
    ResearchHost::new(
        Arc::new(FixtureBackend::new(data.clone())),
        Arc::new(FakeModel::new(
            data.translations.clone(),
            data.model.clone(),
        )),
    )
    .with_clock(move || now.clone())
}

/// One recorded case of a template: parameters, recorded inputs and, where
/// known, the expected `status` and `data`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureCase {
    pub name: String,
    pub params: Value,
    pub fixture: FixtureData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<Expected>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expected {
    pub status: crate::RunStatus,
    pub data: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentences() {
        assert_eq!(first_sentence("One. Two."), "One.");
        assert_eq!(first_sentence("v1.2 is out. More"), "v1.2 is out.");
        assert_eq!(first_sentence("第一句。第二句。"), "第一句。");
        assert_eq!(first_sentence("no stop"), "no stop");
    }
}
