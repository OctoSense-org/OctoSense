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
//! | amount | `amount`, `total`, `price` (a number or a numeric string; the largest) |
//! | count | `count`, else the number of `items`, else the number of recipients |
//!
//! **Only declared fields, and fail closed** (ADR 0004 §8; review of #215).
//! A condition reads only what the tool's own `input_schema` declares
//! ([`readable`]): a call that carries any key the schema does not declare,
//! at any depth (`dest`, `mailto`, `payload`, a key inside one of the
//! `items`), or a tool with no declared schema, is unreadable for rules
//! ([`Unreadable`]), and so is a fact in a shape the reader does not know (a
//! recipient object without an address, a count that is not a whole
//! number, an amount it cannot parse). A missing fact fails the condition
//! that needs it too. Then the rule does not answer; the person does.

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

const RECIPIENT_KEYS: &[&str] = &["to", "cc", "bcc", "recipients", "invitees", "attendees"];
const ATTACHMENT_KEYS: &[&str] = &["attachments", "files"];
const AMOUNT_KEYS: &[&str] = &["amount", "total", "price"];
/// Argument names redacted even when the schema does not type them.
const SECRET_NAMES: &[&str] = &["password", "passphrase", "secret", "token", "api_key", "apikey", "pin", "otp", "one_time_code", "private_key", "access_token", "refresh_token", "client_secret"];
pub const REDACTED: &str = "••••••";

/// A call the rules cannot read: a key its tool's schema does not declare,
/// no schema, or a fact in a shape the reader does not know.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unreadable;

/// Whether every key of `value` is declared by `schema` (its `properties`,
/// and theirs, through arrays' `items`), skipping the top-level fact keys
/// whose values the fact readers check themselves. An object value whose
/// schema declares no properties may carry nothing.
fn declared(value: &Value, schema: &Value) -> bool {
    match value {
        Value::Object(map) => {
            if map.is_empty() {
                return true;
            }
            let Some(props) = schema.get("properties").and_then(Value::as_object) else { return false };
            map.iter().all(|(k, v)| props.get(k).is_some_and(|s| declared(v, s)))
        }
        Value::Array(items) => {
            let item = schema.get("items").unwrap_or(&Value::Null);
            items.iter().all(|i| !i.is_object() && !i.is_array() || declared(i, item))
        }
        _ => true,
    }
}

/// The call's arguments, if the rules may read them: an object whose every
/// key `schema` declares (the fact keys' own values are checked by their
/// readers). `None` schema: nothing is readable.
pub fn readable<'a>(args: &'a Value, schema: Option<&Value>) -> Result<&'a Map<String, Value>, Unreadable> {
    let schema = schema.ok_or(Unreadable)?;
    let map = args.as_object().ok_or(Unreadable)?;
    let props = schema.get("properties").and_then(Value::as_object);
    for (k, v) in map {
        let Some(s) = props.and_then(|p| p.get(k)) else { return Err(Unreadable) };
        if RECIPIENT_KEYS.contains(&k.as_str()) {
            continue;
        }
        if !declared(v, s) {
            return Err(Unreadable);
        }
    }
    Ok(map)
}

fn addresses(v: &Value, out: &mut Vec<String>) -> Result<(), Unreadable> {
    match v {
        Value::Null => Ok(()),
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
            Ok(())
        }
        Value::Array(items) => items.iter().try_for_each(|i| addresses(i, out)),
        Value::Object(map) => match ["address", "email", "id", "user_id"].iter().find_map(|k| map.get(*k)) {
            Some(a @ Value::String(_)) => addresses(a, out),
            _ => Err(Unreadable),
        },
        Value::Bool(_) | Value::Number(_) => Err(Unreadable),
    }
}

/// Every recipient the call names, lower-cased.
pub fn recipients_checked(args: &Value, schema: Option<&Value>) -> Result<Vec<String>, Unreadable> {
    let map = readable(args, schema)?;
    let mut out = Vec::new();
    for key in RECIPIENT_KEYS {
        if let Some(v) = map.get(*key) {
            addresses(v, &mut out)?;
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Whether a value carries nothing.
fn empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(b) => !*b,
        Value::Array(a) => a.is_empty(),
        Value::String(s) => s.trim().is_empty(),
        Value::Object(o) => o.is_empty(),
        Value::Number(_) => false,
    }
}

/// Whether the call carries attachments. Fails closed: a call the rules
/// cannot read counts as carrying them.
pub fn has_attachments(args: &Value, schema: Option<&Value>) -> bool {
    match readable(args, schema) {
        Ok(map) => ATTACHMENT_KEYS.iter().filter_map(|k| map.get(*k)).any(|v| !empty(v)),
        Err(_) => true,
    }
}

fn number(v: &Value) -> Result<f64, Unreadable> {
    let n = match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().trim_start_matches(['$', '€', '£', '¥']).replace(',', "").parse().ok(),
        Value::Object(o) => o.get("value").and_then(Value::as_f64),
        _ => None,
    };
    n.filter(|n| n.is_finite()).ok_or(Unreadable)
}

/// The amount the call names: the largest of `amount`, `total` and `price`;
/// `None` when it names none, one the reader cannot parse, or the call is
/// unreadable.
pub fn amount(args: &Value, schema: Option<&Value>) -> Option<f64> {
    let map = readable(args, schema).ok()?;
    let mut max: Option<f64> = None;
    for v in AMOUNT_KEYS.iter().filter_map(|k| map.get(*k)) {
        let n = number(v).ok()?;
        max = Some(max.map_or(n, |m| m.max(n)));
    }
    max
}

/// How many things the call acts on: `count` (a whole number), else the
/// number of `items` (a list), else the number of recipients (at least one).
pub fn count_checked(args: &Value, schema: Option<&Value>) -> Result<u64, Unreadable> {
    let map = readable(args, schema)?;
    if let Some(v) = map.get("count") {
        return v.as_u64().ok_or(Unreadable);
    }
    if let Some(v) = map.get("items") {
        return v.as_array().map(|a| a.len() as u64).ok_or(Unreadable);
    }
    Ok(recipients_checked(args, schema)?.len().max(1) as u64)
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Relative to the OctoSense home: the audit's per-home digest key
/// (32 bytes, owner-only), made on first use.
pub const AUDIT_KEY_FILE: &str = "approvals/audit.key";

static AUDIT_KEY: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();

/// The home's audit key, read or made (owner-only).
pub fn home_key(home: &std::path::Path) -> std::io::Result<[u8; 32]> {
    let path = home.join(AUDIT_KEY_FILE);
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(key) = <[u8; 32]>::try_from(bytes.as_slice()) {
            return Ok(key);
        }
    }
    // 244 random bits from two v4 UUIDs (the OS's generator), hashed to 32 bytes.
    let mut seed = Vec::with_capacity(32);
    seed.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    seed.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    let key: [u8; 32] = Sha256::digest(&seed).into();
    if let Some(dir) = path.parent() {
        super::create_private_dir(dir)?;
    }
    super::write_private(&path, &key)?;
    Ok(key)
}

/// Use `home`'s key for every audit digest from now on (at startup).
pub fn use_home_key(home: &std::path::Path) {
    match home_key(home) {
        Ok(key) => {
            let _ = AUDIT_KEY.set(key);
        }
        Err(e) => eprintln!("approvals: no audit key ({e}); digests are unkeyed"),
    }
}

/// HMAC-SHA256 (RFC 2104) of `msg` under `key`.
fn hmac(key: &[u8; 32], msg: &[u8]) -> [u8; 32] {
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for (i, k) in key.iter().enumerate() {
        ipad[i] ^= k;
        opad[i] ^= k;
    }
    let inner = Sha256::new().chain_update(ipad).chain_update(msg).finalize();
    Sha256::new().chain_update(opad).chain_update(inner).finalize().into()
}

/// The arguments' digest under `key`: HMAC-SHA256 of their canonical
/// (sorted-key, compact) JSON.
pub fn keyed_digest(key: &[u8; 32], args: &Value) -> String {
    let bytes = serde_json::to_vec(&canonical(args)).unwrap_or_default();
    format!("hmac-sha256:{}", hex(&hmac(key, &bytes)))
}

/// The audit's digest of the exact arguments (the log never holds the
/// arguments): keyed by the home's [`AUDIT_KEY_FILE`] once the shell set it
/// up ([`use_home_key`]), so a short secret (a PIN, a one-time code) cannot
/// be found by hashing guesses; a plain SHA-256 before that (tests).
pub fn digest(args: &Value) -> String {
    if let Some(key) = AUDIT_KEY.get() {
        return keyed_digest(key, args);
    }
    let bytes = serde_json::to_vec(&canonical(args)).unwrap_or_default();
    format!("sha256:{}", hex(&Sha256::digest(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({"type": "object", "properties": {"to": {}, "cc": {}, "attendees": {}, "subject": {}, "attachments": {}, "amount": {}, "total": {}, "count": {}, "items": {"type": "array"}}})
    }

    #[test]
    fn recipients_from_every_shape() {
        let s = schema();
        let s = Some(&s);
        let args = json!({"to": "Ana <Ana@Example.org>, bo@example.org", "cc": ["chen@example.org"], "attendees": [{"email": "ed@example.org"}]});
        assert_eq!(recipients_checked(&args, s).unwrap(), vec!["ana@example.org", "bo@example.org", "chen@example.org", "ed@example.org"]);
        assert!(recipients_checked(&json!({"subject": "x"}), s).unwrap().is_empty());
        assert_eq!(recipients_checked(&json!({"to": [{"name": "Eve"}]}), s), Err(Unreadable));
        assert_eq!(recipients_checked(&json!({"to": "a@x", "shareWith": "eve@x"}), s), Err(Unreadable), "not declared");
        assert_eq!(recipients_checked(&json!({"to": "a@x"}), None), Err(Unreadable), "no schema");
    }

    #[test]
    fn attachments_amounts_counts() {
        let s = schema();
        let s = Some(&s);
        assert!(!has_attachments(&json!({"attachments": []}), s));
        assert!(has_attachments(&json!({"attachments": ["a.pdf"]}), s));
        assert!(has_attachments(&json!({"enclosure": ["a.pdf"]}), s), "undeclared: counted as attachments");
        assert!(has_attachments(&json!({}), None), "no schema: counted as attachments");
        assert_eq!(amount(&json!({"amount": "$1,250.50"}), s), Some(1250.5));
        assert_eq!(amount(&json!({"total": 9}), s), Some(9.0));
        assert_eq!(amount(&json!({"amount": 1, "total": 9}), s), Some(9.0), "the largest");
        assert_eq!(amount(&json!({}), s), None);
        assert_eq!(count_checked(&json!({"count": 7}), s), Ok(7));
        assert_eq!(count_checked(&json!({"items": [1, 2, 3]}), s), Ok(3));
        assert_eq!(count_checked(&json!({"to": ["a@x", "b@x"]}), s), Ok(2));
        assert_eq!(count_checked(&json!({"quantity": 2}), s), Err(Unreadable));
        assert_eq!(count_checked(&json!({"items": [{"quantity": 2}]}), s), Err(Unreadable), "an undeclared key inside a list");
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

    /// Review of #222: the audit's digest is an HMAC under a per-home key,
    /// so a short secret (a PIN) cannot be found by hashing guesses.
    #[test]
    fn the_audit_digest_is_keyed_per_home() {
        let a = keyed_digest(&[1u8; 32], &json!({"pin": "1234"}));
        let b = keyed_digest(&[2u8; 32], &json!({"pin": "1234"}));
        assert!(a.starts_with("hmac-sha256:") && a != b);
        assert_eq!(a, keyed_digest(&[1u8; 32], &json!({"pin": "1234"})));
        assert_ne!(a, digest(&json!({"pin": "1234"})));
        let home = std::env::temp_dir().join(format!("octosense-auditkey-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let k1 = home_key(&home).unwrap();
        assert_eq!(k1, home_key(&home).unwrap(), "stable");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(home.join(AUDIT_KEY_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(home);
    }
}
