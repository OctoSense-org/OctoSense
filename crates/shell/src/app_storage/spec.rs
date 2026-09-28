//! The `storage` block every app manifest carries (ADR 0004 §11): App Hub's
//! `manifest.json` (script apps, extending its `storage.max_bytes`) and
//! `native-apps.json` (native apps).
//!
//! ```json
//! "storage": {
//!   "max_bytes": 536870912,
//!   "accounts": true,
//!   "agent_workspace": "account",
//!   "cache_max_bytes": 1073741824,
//!   "external": []
//! }
//! ```
//!
//! Every field is optional: no block, or `{}`, is one `device` folder, the
//! agent's workspace in it, no byte limits beyond the host's, nothing
//! outside the jail. Unknown fields are refused (as App Hub refuses them),
//! so a manifest written for a newer host does not run with less
//! containment than it asked for.

use serde_json::Value;

/// Which manifest the block came from: only a native app may reach outside
/// its jail (`external`), and only after review in `native-apps.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppKind {
    Script,
    Native,
}

/// What the app's agent may read from disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentWorkspace {
    /// The account's folder (the default).
    #[default]
    Account,
    /// No files: tools only.
    None,
}

/// A path outside the jail a native app needs (`home:rw`), part of its OS
/// sandbox and never of an agent's workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct External {
    /// A well-known place ([`PLACES`]), optionally with a relative sub-path:
    /// `home`, `documents/Invoices`.
    pub place: String,
    pub writable: bool,
}

/// The places `external` may name.
pub const PLACES: &[&str] = &["home", "documents", "downloads", "desktop", "pictures", "music", "videos"];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StorageSpec {
    /// Whole-jail ceiling (App Hub clamps it to the host's maximum).
    pub max_bytes: Option<u64>,
    /// Data per account, one agent per account; `false`: one `device` folder.
    pub accounts: bool,
    pub agent_workspace: AgentWorkspace,
    /// Ceiling for `cache/`.
    pub cache_max_bytes: Option<u64>,
    pub external: Vec<External>,
}

const FIELDS: &[&str] = &["max_bytes", "accounts", "agent_workspace", "cache_max_bytes", "external"];

fn bytes(block: &serde_json::Map<String, Value>, key: &str) -> Result<Option<u64>, String> {
    match block.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_u64() {
            Some(0) => Err(format!("storage.{key} must be positive")),
            Some(n) => Ok(Some(n)),
            None => Err(format!("storage.{key} must be a whole number of bytes")),
        },
    }
}

fn external(entry: &Value) -> Result<External, String> {
    let text = entry.as_str().ok_or("storage.external entries are strings like \"home:rw\"")?;
    let (place, mode) = text
        .rsplit_once(':')
        .ok_or_else(|| format!("storage.external {text:?}: expected <place>:ro or <place>:rw"))?;
    let writable = match mode {
        "rw" => true,
        "ro" => false,
        _ => return Err(format!("storage.external {text:?}: the mode is ro or rw")),
    };
    let mut parts = place.split('/');
    let base = parts.next().unwrap_or("");
    if !PLACES.contains(&base) {
        return Err(format!("storage.external {text:?}: unknown place {base:?} (one of {})", PLACES.join(", ")));
    }
    for part in parts {
        if part.is_empty() || part == "." || part == ".." || part.contains('\\') {
            return Err(format!("storage.external {text:?}: the sub-path must be plain and relative"));
        }
    }
    Ok(External { place: place.to_owned(), writable })
}

impl StorageSpec {
    /// Parse and validate a `storage` value (`None` or `null`: no block).
    pub fn parse(value: Option<&Value>, kind: AppKind) -> Result<Self, String> {
        let block = match value {
            None | Some(Value::Null) => return Ok(Self::default()),
            Some(Value::Object(block)) => block,
            Some(_) => return Err("storage must be an object".into()),
        };
        if let Some(unknown) = block.keys().find(|k| !FIELDS.contains(&k.as_str())) {
            return Err(format!("storage.{unknown} is not a storage field"));
        }
        let accounts = match block.get("accounts") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err("storage.accounts must be true or false".into()),
        };
        let agent_workspace = match block.get("agent_workspace") {
            None | Some(Value::Null) => AgentWorkspace::Account,
            Some(Value::String(s)) if s == "account" => AgentWorkspace::Account,
            Some(Value::String(s)) if s == "none" => AgentWorkspace::None,
            Some(_) => return Err("storage.agent_workspace must be \"account\" or \"none\"".into()),
        };
        let external = match block.get("external") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(entries)) => entries.iter().map(external).collect::<Result<_, _>>()?,
            Some(_) => return Err("storage.external must be a list".into()),
        };
        if kind == AppKind::Script && !external.is_empty() {
            return Err("storage.external is for reviewed native apps only".into());
        }
        let spec = Self {
            max_bytes: bytes(block, "max_bytes")?,
            accounts,
            agent_workspace,
            cache_max_bytes: bytes(block, "cache_max_bytes")?,
            external,
        };
        Ok(spec)
    }

    /// The `storage` block of a whole manifest (`manifest.json` or one
    /// `native-apps.json` entry).
    pub fn from_manifest(manifest: &Value, kind: AppKind) -> Result<Self, String> {
        Self::parse(manifest.get("storage"), kind)
    }
}
