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
//! **Fail closed.** Nothing here guesses in the call's favour: a missing
//! fact fails the condition that needs it, and so does one the reader
//! cannot make out. An argument anywhere in the call (at any depth) whose
//! name looks like the fact under a name the table does not list
//! (`attachment`, `file_path`, `quantity`, `share_with`, `cost`, a nested
//! `cc`), a value in a shape the reader does not know (a recipient object
//! without an address, a count that is not a whole number), or two amounts
//! that disagree: the fact is unreadable ([`Unreadable`]), and the rule
//! does not answer; the person does.

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

const RECIPIENT_KEYS: &[&str] = &["to", "cc", "bcc", "recipients", "invitees", "attendees"];
const ATTACHMENT_KEYS: &[&str] = &["attachments", "files"];
const AMOUNT_KEYS: &[&str] = &["amount", "total", "price"];
/// Words that make an argument name look like one of the facts. A name is
/// split on `_`, `-`, `.` and camel case; one word matching (or starting
/// with the stem, for `attach*`) is enough.
const RECIPIENT_WORDS: &[&str] = &[
    "to", "cc", "bcc", "recipient", "recipients", "invitee", "invitees", "attendee", "attendees", "participant", "participants", "guest", "guests", "member", "members", "email", "emails",
    "address", "addresses", "phone", "phones", "contact", "contacts", "share", "notify", "user", "users", "people", "person",
];
const ATTACHMENT_WORDS: &[&str] = &["attachment", "attachments", "file", "files", "path", "paths", "upload", "uploads", "document", "documents", "media", "image", "images", "photo", "photos", "blob", "blobs"];
const AMOUNT_WORDS: &[&str] = &["amount", "amounts", "total", "totals", "price", "prices", "cost", "costs", "fee", "fees", "sum", "payment", "subtotal", "charge"];
const COUNT_WORDS: &[&str] = &["count", "counts", "quantity", "quantities", "qty", "number", "times", "repeat", "repeats", "copies", "batch", "limit", "max", "n"];
/// Argument names redacted even when the schema does not type them.
const SECRET_NAMES: &[&str] = &["password", "passphrase", "secret", "token", "api_key", "apikey", "pin", "otp", "one_time_code", "private_key", "access_token", "refresh_token", "client_secret"];
pub const REDACTED: &str = "••••••";

/// A fact the call carries in a shape (or under a name) the reader does
/// not know: every condition that needs it fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unreadable;

/// The words of an argument name: `shareWith` → `share`, `with`.
fn words(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in name.chars() {
        if c == '_' || c == '-' || c == '.' || c == ' ' {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        prev_lower = c.is_lowercase() || c.is_ascii_digit();
        cur.extend(c.to_lowercase());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn named_like(name: &str, vocabulary: &[&str]) -> bool {
    words(name).iter().any(|w| vocabulary.contains(&w.as_str()) || (vocabulary == ATTACHMENT_WORDS && w.starts_with("attach")))
}

/// Every (name, value) in the call, at any depth, except the top-level
/// keys in `skip` (the table's own, read by the fact itself) and, unless
/// `into_skipped`, what they hold.
fn walk<'a>(args: &'a Value, skip: &[&str], into_skipped: bool, out: &mut Vec<(&'a str, &'a Value)>) {
    fn inner<'a>(v: &'a Value, out: &mut Vec<(&'a str, &'a Value)>) {
        match v {
            Value::Object(map) => {
                for (k, v) in map {
                    out.push((k.as_str(), v));
                    inner(v, out);
                }
            }
            Value::Array(items) => items.iter().for_each(|i| inner(i, out)),
            _ => {}
        }
    }
    match args {
        Value::Object(map) => {
            for (k, v) in map {
                if skip.contains(&k.as_str()) {
                    if into_skipped {
                        inner(v, out);
                    }
                    continue;
                }
                out.push((k.as_str(), v));
                inner(v, out);
            }
        }
        other => inner(other, out),
    }
}

/// The other arguments named like a fact (not the table's own keys).
fn lookalikes<'a>(args: &'a Value, own: &[&str], into_own: bool, vocabulary: &[&str]) -> Vec<(&'a str, &'a Value)> {
    let mut all = Vec::new();
    walk(args, own, into_own, &mut all);
    all.into_iter().filter(|(k, _)| named_like(k, vocabulary)).collect()
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

/// Every recipient the call names, lower-cased; unreadable when one of
/// them is in a shape the reader does not know, or the call names people
/// under another argument (at any depth).
pub fn recipients_checked(args: &Value) -> Result<Vec<String>, Unreadable> {
    let mut out = Vec::new();
    let Some(map) = args.as_object() else { return if args.is_null() { Ok(out) } else { Err(Unreadable) } };
    for key in RECIPIENT_KEYS {
        if let Some(v) = map.get(*key) {
            addresses(v, &mut out)?;
        }
    }
    if lookalikes(args, RECIPIENT_KEYS, false, RECIPIENT_WORDS).iter().any(|(_, v)| !empty(v)) {
        return Err(Unreadable);
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

/// Whether the call carries attachments: the table's keys, or anything
/// named like an attachment at any depth (`attachment`, `file_path`, a
/// nested `attachments`), that is not empty. Fails closed: whatever the
/// reader cannot rule out counts as an attachment.
pub fn has_attachments(args: &Value) -> bool {
    let own = args.as_object().map(|m| ATTACHMENT_KEYS.iter().filter_map(|k| m.get(*k)).any(|v| !empty(v))).unwrap_or(false);
    own || lookalikes(args, ATTACHMENT_KEYS, false, ATTACHMENT_WORDS).iter().any(|(_, v)| !empty(v))
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

/// The amount the call names: the largest of every amount it carries
/// (`amount`, `total`, `price`, and anything named like one at any depth);
/// `None` when there is none, or one the reader cannot parse.
pub fn amount(args: &Value) -> Option<f64> {
    let own: Vec<&Value> = args.as_object().map(|m: &Map<String, Value>| AMOUNT_KEYS.iter().filter_map(|k| m.get(*k)).collect()).unwrap_or_default();
    let others = lookalikes(args, AMOUNT_KEYS, false, AMOUNT_WORDS);
    let mut max: Option<f64> = None;
    for v in own.into_iter().chain(others.into_iter().map(|(_, v)| v)) {
        let n = number(v).ok()?;
        max = Some(max.map_or(n, |m| m.max(n)));
    }
    max
}

/// How many things the call acts on: `count` (a whole number), else the
/// number of `items`, else the number of recipients (at least one).
/// Unreadable when `count` or `items` is in another shape, the recipients
/// are unreadable, or anything else is named like a count (`quantity`, a
/// nested `repeat`, a `quantity` inside one of the `items`).
pub fn count_checked(args: &Value) -> Result<u64, Unreadable> {
    if !lookalikes(args, &["count", "items"], true, COUNT_WORDS).is_empty() {
        return Err(Unreadable);
    }
    if let Some(v) = args.get("count") {
        return v.as_u64().ok_or(Unreadable);
    }
    if let Some(v) = args.get("items") {
        return v.as_array().map(|a| a.len() as u64).ok_or(Unreadable);
    }
    Ok(recipients_checked(args)?.len().max(1) as u64)
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
        assert_eq!(recipients_checked(&args).unwrap(), vec!["ana@example.org", "bo@example.org", "chen@example.org", "ed@example.org"]);
        assert!(recipients_checked(&json!({"subject": "x"})).unwrap().is_empty());
        assert_eq!(recipients_checked(&json!({"to": [{"name": "Eve"}]})), Err(Unreadable));
        assert_eq!(recipients_checked(&json!({"to": "a@x", "shareWith": "eve@x"})), Err(Unreadable), "camel case splits");
    }

    #[test]
    fn attachments_amounts_counts() {
        assert!(!has_attachments(&json!({"attachments": []})));
        assert!(has_attachments(&json!({"attachments": ["a.pdf"]})));
        assert_eq!(amount(&json!({"amount": "$1,250.50"})), Some(1250.5));
        assert_eq!(amount(&json!({"total": 9})), Some(9.0));
        assert_eq!(amount(&json!({})), None);
        assert_eq!(count_checked(&json!({"count": 7})), Ok(7));
        assert_eq!(count_checked(&json!({"items": [1, 2, 3]})), Ok(3));
        assert_eq!(count_checked(&json!({"to": ["a@x", "b@x"]})), Ok(2));
        assert_eq!(count_checked(&json!({"quantity": 2})), Err(Unreadable));
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
