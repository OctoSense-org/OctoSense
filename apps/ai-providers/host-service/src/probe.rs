//! `llm.test`: the smallest request a provider answers, and what its answer
//! means. Runs on a worker; nothing it returns carries the key.
use crate::model::{effective_api_type, effective_base_url, effective_model};
use octosense_llm_config::{registry, ApiType, Provider};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// The request for `provider` with `key` (none for keyless local servers).
pub fn probe_request(provider: &Provider, key: Option<&str>) -> Result<ProbeRequest, String> {
    let family = registry::lookup(&provider.family).map(|f| f.id).unwrap_or(provider.family.as_str());
    if family == "vertex" {
        return Err("Test is not available for Vertex AI".into());
    }
    let base = effective_base_url(provider).ok_or("no base URL")?;
    let base = base.trim().trim_end_matches('/').to_string();
    if !(base.starts_with("https://") || base.starts_with("http://")) {
        return Err("the base URL must start with https://".into());
    }
    let model = effective_model(provider).ok_or("no model")?;
    let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
    let ping = json!([{"role": "user", "content": "ping"}]);
    let (url, body) = match effective_api_type(provider) {
        ApiType::Anthropic => {
            if let Some(key) = key {
                headers.push(("x-api-key".into(), key.into()));
            }
            headers.push(("anthropic-version".into(), "2023-06-01".into()));
            let url = if base.ends_with("/v1") { format!("{base}/messages") } else { format!("{base}/v1/messages") };
            (url, json!({"model": model, "max_tokens": 1, "messages": ping}))
        }
        ApiType::OpenAi => {
            if let Some(key) = key {
                headers.push(("Authorization".into(), format!("Bearer {key}")));
            }
            // Gemini's OpenAI-compatible surface sits under /openai.
            let url = if family == "gemini" && !base.ends_with("/openai") {
                format!("{base}/openai/chat/completions")
            } else {
                format!("{base}/chat/completions")
            };
            (url, json!({"model": model, "max_tokens": 1, "messages": ping}))
        }
        ApiType::Responses => {
            if let Some(key) = key {
                headers.push(("Authorization".into(), format!("Bearer {key}")));
            }
            // The Responses API's smallest allowed output.
            (format!("{base}/responses"), json!({"model": model, "input": "ping", "max_output_tokens": 16}))
        }
    };
    Ok(ProbeRequest { url, headers, body: body.to_string() })
}

/// The endpoint's model list (`GET …/models`), with the same headers as the
/// test (Anthropic's `GET /v1/models` takes `x-api-key`).
pub fn models_request(provider: &Provider, key: Option<&str>) -> Result<ProbeRequest, String> {
    let family = registry::lookup(&provider.family).map(|f| f.id).unwrap_or(provider.family.as_str());
    if family == "vertex" {
        return Err("Vertex AI does not list its models here.".into());
    }
    let base = effective_base_url(provider).ok_or("This route needs a base URL.")?;
    let base = base.trim().trim_end_matches('/').to_string();
    if !(base.starts_with("https://") || base.starts_with("http://")) {
        return Err("the base URL must start with https://".into());
    }
    let mut headers = Vec::new();
    let url = match effective_api_type(provider) {
        ApiType::Anthropic => {
            if let Some(key) = key {
                headers.push(("x-api-key".to_string(), key.to_string()));
            }
            headers.push(("anthropic-version".into(), "2023-06-01".into()));
            if base.ends_with("/v1") { format!("{base}/models") } else { format!("{base}/v1/models") }
        }
        ApiType::OpenAi | ApiType::Responses => {
            if let Some(key) = key {
                headers.push(("Authorization".into(), format!("Bearer {key}")));
            }
            if family == "gemini" && !base.ends_with("/openai") { format!("{base}/openai/models") } else { format!("{base}/models") }
        }
    };
    Ok(ProbeRequest { url, headers, body: String::new() })
}

/// The model ids in a `/models` answer (`{data: [{id}]}`, OpenAI and
/// Anthropic alike; `{models: [{name}]}` as some servers answer), sorted,
/// without duplicates or anything that is not a plausible id.
pub fn parse_models(body: &[u8]) -> Vec<String> {
    let Ok(v) = serde_json::from_slice::<Value>(body) else { return Vec::new() };
    let rows = v.get("data").or_else(|| v.get("models")).and_then(Value::as_array).cloned().unwrap_or_default();
    let mut ids: Vec<String> = rows
        .iter()
        .filter_map(|m| m.get("id").or_else(|| m.get("name")).and_then(Value::as_str))
        .map(|id| id.trim().trim_start_matches("models/").to_string())
        .filter(|id| !id.is_empty() && id.len() <= 200 && !id.chars().any(|c| c.is_control() || c == '"' || c == '\\'))
        .collect();
    ids.sort();
    ids.dedup();
    ids.truncate(500);
    ids
}

/// Ask the endpoint which models it serves: the ids, or why not (never
/// carrying the key).
pub fn fetch_models(provider: &Provider, key: Option<&str>) -> Result<Vec<String>, String> {
    let request = models_request(provider, key)?;
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).redirects(0).build();
    let mut call = agent.get(&request.url);
    for (name, value) in &request.headers {
        call = call.set(name, value);
    }
    let (status, body) = match call.call() {
        Ok(response) => (response.status(), read_limited(response, 4 * 1024 * 1024)),
        Err(ureq::Error::Status(status, response)) => (status, read(response)),
        Err(ureq::Error::Transport(t)) => return Err(describe_error(&t.to_string(), key)),
    };
    if let Some(error) = describe_response(status, &body, key) {
        return Err(error);
    }
    let ids = parse_models(&body);
    if ids.is_empty() {
        return Err("The endpoint listed no models.".into());
    }
    Ok(ids)
}

/// Send the test request: `{ok, ms, error, network, reason}`: `error` is
/// null on success; `network` says the provider was not reached (DNS, TLS,
/// refused, timed out) rather than answering with an error; `reason` says
/// why in a few words (`HTTP 401 · invalid key`, `unreachable`, `timeout`).
pub fn run(provider: &Provider, key: Option<&str>) -> Value {
    run_within(provider, key, Duration::from_secs(20))
}

/// [`run`] with its own timeout (the check before a save waits less).
pub fn run_within(provider: &Provider, key: Option<&str>, timeout: Duration) -> Value {
    let request = match probe_request(provider, key) {
        Ok(r) => r,
        Err(e) => return json!({"ok": false, "ms": 0, "error": e, "network": false, "reason": "not testable"}),
    };
    let agent = ureq::AgentBuilder::new().timeout(timeout).redirects(0).build();
    let mut call = agent.post(&request.url);
    for (name, value) in &request.headers {
        call = call.set(name, value);
    }
    let started = Instant::now();
    let answer = call.send_string(&request.body);
    let ms = started.elapsed().as_millis() as u64;
    let (status, body) = match answer {
        Ok(response) => (response.status(), read(response)),
        Err(ureq::Error::Status(status, response)) => (status, read(response)),
        // The provider was not reached: `network` lets a sheet save anyway.
        Err(ureq::Error::Transport(t)) => {
            let text = t.to_string();
            return json!({"ok": false, "ms": ms, "error": describe_error(&text, key), "network": true, "reason": transport_reason(&text)});
        }
    };
    match describe_response(status, &body, key) {
        None => json!({"ok": true, "ms": ms, "error": null, "network": false, "reason": ""}),
        Some(error) => json!({"ok": false, "ms": ms, "error": error, "network": false, "reason": status_reason(status)}),
    }
}

fn read(response: ureq::Response) -> Vec<u8> {
    read_limited(response, 64 * 1024)
}

fn read_limited(response: ureq::Response, limit: u64) -> Vec<u8> {
    use std::io::Read as _;
    let mut body = Vec::new();
    let _ = response.into_reader().take(limit).read_to_end(&mut body);
    body
}

/// What a finished test means: `None` on success, else the HTTP status and
/// the provider's short message, with anything resembling the key removed.
pub fn describe_response(status: u16, body: &[u8], key: Option<&str>) -> Option<String> {
    if (200..300).contains(&status) {
        return None;
    }
    let text = String::from_utf8_lossy(body);
    let message = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| {
            let pick = |v: &Value| v.as_str().map(str::to_string);
            v.pointer("/error/message")
                .and_then(pick)
                .or_else(|| v.pointer("/message").and_then(pick))
                .or_else(|| v.pointer("/error").and_then(pick))
                .or_else(|| v.pointer("/detail").and_then(pick))
        })
        .unwrap_or_default();
    let message = short(&redact(&message, key), 120);
    Some(if message.is_empty() { format!("HTTP {status}") } else { format!("HTTP {status}: {message}") })
}

/// What an HTTP error status means, in a few words: `HTTP 401 · invalid key`.
pub fn status_reason(status: u16) -> String {
    let what = match status {
        401 | 403 => "invalid key",
        404 => "model or route not found",
        400 | 422 => "request refused",
        402 => "billing or quota",
        429 => "rate limited",
        500..=599 => "provider error",
        _ => "",
    };
    if what.is_empty() { format!("HTTP {status}") } else { format!("HTTP {status} · {what}") }
}

/// What a transport failure means: `timeout` or `unreachable`.
pub fn transport_reason(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") { "timeout" } else { "unreachable" }
}

/// A transport failure (DNS, TLS, refused).
pub fn describe_error(message: &str, key: Option<&str>) -> String {
    let message = short(&redact(message, key), 120);
    if message.is_empty() { "request failed".into() } else { format!("failed: {message}") }
}

/// Drop every word that carries the key or a visible piece of it (providers
/// quote a masked key back: "Incorrect API key provided: sk-proj-****abcd").
pub fn redact(text: &str, key: Option<&str>) -> String {
    let key = key.map(str::trim).unwrap_or("");
    let head: String = key.chars().take(8).collect();
    let tail: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    text.split(' ')
        .map(|word| {
            let keyish = word.starts_with("sk-")
                || (!key.is_empty() && (word.contains(key) || (head.len() >= 8 && word.contains(&head))))
                || (tail.len() == 4 && word.contains('*') && word.contains(&tail));
            if keyish { "[key]" } else { word }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn short(text: &str, max: usize) -> String {
    let text = text.trim().replace('\n', " ");
    if text.chars().count() <= max {
        return text;
    }
    let cut: String = text.chars().take(max).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_per_protocol() {
        let r = probe_request(&Provider::new("openai", Some("gpt-test".into())), Some("sk-test-1")).unwrap();
        assert_eq!(r.url, "https://api.openai.com/v1/chat/completions");
        assert!(r.headers.contains(&("Authorization".into(), "Bearer sk-test-1".into())));
        let body: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(body["max_tokens"], 1);
        assert_eq!(body["model"], "gpt-test");

        let r = probe_request(&Provider::new("anthropic", None), Some("sk-test-2")).unwrap();
        assert_eq!(r.url, "https://api.anthropic.com/v1/messages");
        assert!(r.headers.contains(&("x-api-key".into(), "sk-test-2".into())));
        assert!(!r.body.contains("sk-test"));

        let r = probe_request(&Provider::new("zai-coding", None), Some("k")).unwrap();
        assert_eq!(r.url, "https://api.z.ai/api/anthropic/v1/messages");

        let r = probe_request(&Provider::new("ollama", None), None).unwrap();
        assert_eq!(r.url, "http://localhost:11434/v1/chat/completions");
        assert!(!r.headers.iter().any(|(h, _)| h == "Authorization"));

        let mut responses = Provider::new("openai", None);
        responses.api_type = Some(ApiType::Responses);
        assert_eq!(probe_request(&responses, Some("k")).unwrap().url, "https://api.openai.com/v1/responses");
        assert!(probe_request(&Provider::new("vllm", None), None).is_err());
    }

    #[test]
    fn model_lists_per_protocol() {
        let r = models_request(&Provider::new("deepseek", None), Some("sk-test-1")).unwrap();
        assert_eq!(r.url, "https://api.deepseek.com/v1/models");
        assert!(r.headers.contains(&("Authorization".into(), "Bearer sk-test-1".into())));
        let r = models_request(&Provider::new("anthropic", None), Some("sk-test-2")).unwrap();
        assert_eq!(r.url, "https://api.anthropic.com/v1/models");
        assert!(r.headers.contains(&("x-api-key".into(), "sk-test-2".into())));
        let r = models_request(&Provider::new("zai", None), Some("k")).unwrap();
        assert_eq!(r.url, "https://api.z.ai/api/anthropic/v1/models");
        assert_eq!(
            parse_models(br#"{"data":[{"id":"b"},{"id":"a"},{"id":"a"},{"id":"bad\"id"},{"x":1}]}"#),
            ["a", "b"]
        );
        assert_eq!(parse_models(br#"{"models":[{"name":"models/gemini-2.5-flash"}]}"#), ["gemini-2.5-flash"]);
        assert!(parse_models(b"<html>").is_empty());
    }

    #[test]
    fn reasons_are_short() {
        assert_eq!(status_reason(401), "HTTP 401 · invalid key");
        assert_eq!(status_reason(429), "HTTP 429 · rate limited");
        assert_eq!(status_reason(418), "HTTP 418");
        assert_eq!(transport_reason("Network Error: timed out reading response"), "timeout");
        assert_eq!(transport_reason("Connection Failed: Connect error: Connection refused"), "unreachable");
    }

    #[test]
    fn errors_never_echo_the_key() {
        let key = "sk-proj-test-abcdefgh9876";
        let body = br#"{"error":{"message":"Incorrect API key provided: sk-proj-****9876. You can find your API key at https://platform.openai.com."}}"#;
        let text = describe_response(401, body, Some(key)).unwrap();
        assert!(text.starts_with("HTTP 401: Incorrect API key provided: [key]"), "{text}");
        assert!(!text.contains("9876"));
        assert_eq!(describe_response(200, b"{}", Some(key)), None);
        assert_eq!(describe_response(503, b"<html>", None).unwrap(), "HTTP 503");
        assert!(!describe_error(&format!("bad key {key} here"), Some(key)).contains("abcdefgh"));
    }
}
