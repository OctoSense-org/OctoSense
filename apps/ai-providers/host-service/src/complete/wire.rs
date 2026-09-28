//! One `model.complete` request on the wire, in the three protocols a
//! provider speaks, and the text and token usage out of its answer.
//!
//! No output-token cap is sent where the protocol allows none (OpenAI chat
//! completions, the Responses API): a reasoning model spends any cap on its
//! thinking first, and the schema plus the host's byte cap bound the reply.
//! Anthropic's Messages API requires `max_tokens`, so it gets the model's
//! own maximum from the catalog (the most the model can produce anyway), or
//! [`ANTHROPIC_UNLISTED_MAX`] for a model the catalog does not list.
use crate::model::{effective_api_type, effective_base_url, effective_model};
use octosense_llm_config::{catalog, registry, ApiType, Provider};
use serde_json::{json, Value};

/// `max_tokens` for an Anthropic-protocol model the catalog does not list.
pub const ANTHROPIC_UNLISTED_MAX: u64 = 32_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prepared {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub api: ApiType,
}

/// The request for `provider`: `system` and one `user` message.
pub fn prepare(provider: &Provider, key: Option<&str>, system: &str, user: &str) -> Result<Prepared, String> {
    let family = registry::lookup(&provider.family).map(|f| f.id).unwrap_or(provider.family.as_str());
    if family == "vertex" {
        return Err("Vertex AI is not supported for direct calls".into());
    }
    let base = effective_base_url(provider).ok_or("no base URL")?;
    let base = base.trim().trim_end_matches('/').to_string();
    if !(base.starts_with("https://") || base.starts_with("http://")) {
        return Err("the base URL must start with https://".into());
    }
    let model = effective_model(provider).ok_or("no model")?;
    let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
    let api = effective_api_type(provider);
    let (url, body) = match api {
        ApiType::Anthropic => {
            if let Some(key) = key {
                headers.push(("x-api-key".into(), key.into()));
            }
            headers.push(("anthropic-version".into(), "2023-06-01".into()));
            let url = if base.ends_with("/v1") { format!("{base}/messages") } else { format!("{base}/v1/messages") };
            let max = catalog::model(family, &model).and_then(|m| m.max_output).unwrap_or(ANTHROPIC_UNLISTED_MAX);
            (url, json!({"model": model, "max_tokens": max, "system": system, "messages": [{"role": "user", "content": user}]}))
        }
        ApiType::OpenAi => {
            if let Some(key) = key {
                headers.push(("Authorization".into(), format!("Bearer {key}")));
            }
            let url = if family == "gemini" && !base.ends_with("/openai") {
                format!("{base}/openai/chat/completions")
            } else {
                format!("{base}/chat/completions")
            };
            (
                url,
                json!({"model": model, "stream": false, "messages": [
                    {"role": "system", "content": system}, {"role": "user", "content": user}
                ]}),
            )
        }
        ApiType::Responses => {
            if let Some(key) = key {
                headers.push(("Authorization".into(), format!("Bearer {key}")));
            }
            (format!("{base}/responses"), json!({"model": model, "instructions": system, "input": user, "store": false}))
        }
    };
    Ok(Prepared { url, headers, body: body.to_string(), api })
}

/// Tokens a provider reports for one request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
}

/// The reply text and usage out of a 2xx answer.
pub fn parse(api: ApiType, body: &[u8]) -> Result<(String, Option<Tokens>), String> {
    let v: Value = serde_json::from_slice(body).map_err(|_| "the provider's answer was not JSON".to_string())?;
    let n = |p: &str| v.pointer(p).and_then(Value::as_u64);
    let (text, usage) = match api {
        ApiType::OpenAi => {
            let text = v.pointer("/choices/0/message/content").and_then(Value::as_str).map(str::to_string);
            let usage = n("/usage/prompt_tokens").map(|i| Tokens { input: i, output: n("/usage/completion_tokens").unwrap_or(0) });
            (text, usage)
        }
        ApiType::Anthropic => {
            let text = v["content"].as_array().map(|blocks| {
                blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("")
            });
            let usage = n("/usage/input_tokens").map(|i| Tokens { input: i, output: n("/usage/output_tokens").unwrap_or(0) });
            (text, usage)
        }
        ApiType::Responses => {
            let text = v["output_text"].as_str().map(str::to_string).or_else(|| {
                v["output"].as_array().map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item["content"].as_array())
                        .flatten()
                        .filter(|c| c["type"] == "output_text")
                        .filter_map(|c| c["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("")
                })
            });
            let usage = n("/usage/input_tokens").map(|i| Tokens { input: i, output: n("/usage/output_tokens").unwrap_or(0) });
            (text, usage)
        }
    };
    match text {
        Some(t) if !t.trim().is_empty() => Ok((t, usage)),
        _ => Err("the provider answered with no text".into()),
    }
}

/// The JSON inside a model's reply: without a leading `<think>` block or a
/// Markdown code fence around it.
pub fn unwrap_json(text: &str) -> &str {
    let mut t = text.trim();
    if t.starts_with("<think>") {
        if let Some(end) = t.find("</think>") {
            t = t[end + "</think>".len()..].trim();
        }
    }
    if let Some(rest) = t.strip_prefix("```") {
        let rest = rest.strip_prefix("json").or_else(|| rest.strip_prefix("JSON")).unwrap_or(rest);
        if let Some(inner) = rest.trim_end().strip_suffix("```") {
            return inner.trim();
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fences_and_thinking_are_removed() {
        assert_eq!(unwrap_json("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(unwrap_json("<think>hmm</think>\n{\"a\":1}"), "{\"a\":1}");
        assert_eq!(unwrap_json(" [1] "), "[1]");
    }

    #[test]
    fn usage_is_read_per_protocol() {
        let (t, u) = parse(ApiType::OpenAi, br#"{"choices":[{"message":{"content":"{}"}}],"usage":{"prompt_tokens":5,"completion_tokens":7}}"#).unwrap();
        assert_eq!((t.as_str(), u), ("{}", Some(Tokens { input: 5, output: 7 })));
        let (t, u) = parse(ApiType::Anthropic, br#"{"content":[{"type":"text","text":"[]"}],"usage":{"input_tokens":3,"output_tokens":2}}"#).unwrap();
        assert_eq!((t.as_str(), u), ("[]", Some(Tokens { input: 3, output: 2 })));
        let (t, _) = parse(ApiType::Responses, br#"{"output":[{"type":"message","content":[{"type":"output_text","text":"1"}]}]}"#).unwrap();
        assert_eq!(t, "1");
        assert!(parse(ApiType::OpenAi, br#"{"choices":[]}"#).is_err());
    }
}
