//! What rules and sheets read out of a call's exact arguments.
//!
//! The `tools.json` fields rules can match are still an open question of
//! ADR 0004 (agreed with App Hub later), so the router reads the
//! conventional argument names every system app uses today:
//!
//! | Fact | Arguments |
//! | --- | --- |
//! | recipients | `to`, `cc`, `bcc`, `recipients`, `invitees`, `attendees` (a string, a list of strings, or objects with `address`/`email`/`id`) |
//! | attachments | `attachments`, `files` (a non-empty list, or `true`) |
//! | amount | `amount`, `total`, `price` (a number or a numeric string) |
//! | count | `count`, else the number of `items`, else the number of recipients |
//!
//! Nothing here guesses in the call's favour: a missing fact fails the
//! condition that needs it.

use serde_json::Value;
use sha2::{Digest, Sha256};

const RECIPIENT_KEYS: &[&str] = &["to", "cc", "bcc", "recipients", "invitees", "attendees"];
const ATTACHMENT_KEYS: &[&str] = &["attachments", "files"];
const AMOUNT_KEYS: &[&str] = &["amount", "total", "price"];
/// Argument names redacted even when the schema does not type them.
const SECRET_NAMES: &[&str] = &["password", "passphrase", "secret", "token", "api_key", "apikey", "pin", "otp", "one_time_code", "private_key", "access_token", "refresh_token", "client_secret"];
pub const REDACTED: &str = "••••••";

fn addresses(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => {
            for part in s.split([',', ';']) {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                // "Ana <ana@example.org>" → the address.
                let addr = match (part.rfind('<'), part.rfind('>')) {
                    (Some(a), Some(b)) if a < b => &part[a + 1..b],
                    _ => part,
                };
                out.push(addr.trim().to_lowercase());
            }
        }
        Value::Array(items) => items.iter().for_each(|i| addresses(i, out)),
        Value::Object(map) => {
            if let Some(a) = ["address", "email", "id", "user_id"].iter().find_map(|k| map.get(*k)) {
                addresses(a, out);
            }
        }
        _ => {}
    }
}

/// Every recipient the call names, lower-cased.
pub fn recipients(args: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for key in RECIPIENT_KEYS {
        if let Some(v) = args.get(*key) {
            addresses(v, &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Whether the call carries attachments.
pub fn has_attachments(args: &Value) -> bool {
    ATTACHMENT_KEYS.iter().filter_map(|k| args.get(*k)).any(|v| match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Array(a) => !a.is_empty(),
        Value::String(s) => !s.trim().is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::Number(_) => true,
    })
}

/// The amount the call names, if any.
pub fn amount(args: &Value) -> Option<f64> {
    AMOUNT_KEYS.iter().filter_map(|k| args.get(*k)).find_map(|v| match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().trim_start_matches(['$', '€', '£', '¥']).replace(',', "").parse().ok(),
        Value::Object(o) => o.get("value").and_then(|v| v.as_f64()),
        _ => None,
    })
}

/// How many things the call acts on.
pub fn count(args: &Value) -> u64 {
    if let Some(n) = args.get("count").and_then(|v| v.as_u64()) {
        return n;
    }
    if let Some(items) = args.get("items").and_then(|v| v.as_array()) {
        return items.len() as u64;
    }
    recipients(args).len().max(1) as u64
}

/// Whether an argument name is a secret: declared by the schema, or named
/// like one.
pub fn is_secret(name: &str, declared: &[String]) -> bool {
    let lower = name.to_lowercase();
    declared.iter().any(|d| d.eq_ignore_ascii_case(name)) || SECRET_NAMES.iter().any(|s| lower == *s || lower.ends_with(&format!("_{s}")))
}

/// The arguments with every secret-typed field's value replaced.
pub fn redact(args: &Value, declared: &[String]) -> Value {
    match args {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let v = if is_secret(k, declared) { Value::String(REDACTED.into()) } else { redact(v, declared) };
                    (k.clone(), v)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(|v| redact(v, declared)).collect()),
        v => v.clone(),
    }
}

/// The exact arguments as a sheet shows them: pretty-printed, one field per
/// line, secrets redacted. Key order is the call's own (serde_json keeps
/// insertion order only with `preserve_order`; sorted otherwise, which is
/// stable for the person too).
pub fn pretty(args: &Value, declared: &[String]) -> Vec<String> {
    let redacted = redact(args, declared);
    serde_json::to_string_pretty(&redacted).unwrap_or_default().lines().map(str::to_string).collect()
}

/// The audit's digest of the exact arguments: SHA-256 of their canonical
/// (sorted-key, compact) JSON. The log never holds the arguments.
pub fn digest(args: &Value) -> String {
    fn canonical(v: &Value) -> Value {
        match v {
            Value::Object(map) => {
                let mut keys: Vec<_> = map.keys().collect();
                keys.sort();
                Value::Object(keys.into_iter().map(|k| (k.clone(), canonical(&map[k]))).collect())
            }
            Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
            v => v.clone(),
        }
    }
    let bytes = serde_json::to_vec(&canonical(args)).unwrap_or_default();
    let hash = Sha256::digest(&bytes);
    let mut out = String::from("sha256:");
    for b in hash {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recipients_from_every_shape() {
        let args = json!({"to": "Ana <Ana@Example.org>, bo@example.org", "cc": ["chen@example.org"], "attendees": [{"email": "ed@example.org"}]});
        assert_eq!(recipients(&args), vec!["ana@example.org", "bo@example.org", "chen@example.org", "ed@example.org"]);
        assert!(recipients(&json!({"subject": "x"})).is_empty());
    }

    #[test]
    fn attachments_amounts_counts() {
        assert!(!has_attachments(&json!({"attachments": []})));
        assert!(has_attachments(&json!({"attachments": ["a.pdf"]})));
        assert_eq!(amount(&json!({"amount": "$1,250.50"})), Some(1250.5));
        assert_eq!(amount(&json!({"total": 9})), Some(9.0));
        assert_eq!(amount(&json!({})), None);
        assert_eq!(count(&json!({"count": 7})), 7);
        assert_eq!(count(&json!({"items": [1, 2, 3]})), 3);
        assert_eq!(count(&json!({"to": ["a@x", "b@x"]})), 2);
    }

    #[test]
    fn secrets_are_redacted_by_schema_and_by_name() {
        let args = json!({"user": "ana", "pw": "hunter2", "nested": {"api_key": "sk-1", "note": "hi"}});
        let lines = pretty(&args, &["pw".into()]).join("\n");
        assert!(!lines.contains("hunter2") && !lines.contains("sk-1"), "{lines}");
        assert!(lines.contains("ana") && lines.contains("hi"));
        assert!(is_secret("smtp_password", &[]));
        assert!(!is_secret("subject", &[]));
    }

    #[test]
    fn digest_is_canonical() {
        assert_eq!(digest(&json!({"a": 1, "b": [1, 2]})), digest(&json!({"b": [1, 2], "a": 1})));
        assert_ne!(digest(&json!({"a": 1})), digest(&json!({"a": 2})));
        assert!(digest(&json!({})).starts_with("sha256:"));
    }
}
