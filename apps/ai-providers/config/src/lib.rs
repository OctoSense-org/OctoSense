//! octosense-llm-config
//!
//! The LLM provider set an OctoSense device runs on, and the three places it
//! lives:
//!
//! - [`registry`] — the octos provider families (key env var, aliases,
//!   default endpoint/model), mirrored from `octos-llm/src/registry/*`.
//! - [`profile`] — the embedded kernel's `profiles/_main.json`: read the
//!   `config.llm` contract, merge a new one in without disturbing anything
//!   else, write it atomically.
//! - [`qr`] — the provisioning QR: octos's own `OCTOS1:` / `OCTOS1E:`
//!   profile-QR format (byte-compatible with `octos-cli/src/profile_qr.rs`)
//!   plus the legacy a2app JSON payload.
//!
//! No UI and no Makepad: the phone Home, the desktop "AI providers" app and
//! the `llm-qr` tool all sit on top of this crate.
//!
//! The flow on a phone is `qr::decode` → [`apply_to_profile`]. Decoding an
//! `OCTOS1E:` code runs Argon2id over 64 MiB — call it off the UI thread.

pub mod profile;
pub mod qr;
pub mod registry;

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Wire protocol of a provider route (octos `LlmRouteConfig.api_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApiType {
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "responses")]
    Responses,
}

impl ApiType {
    /// The octos wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ApiType::OpenAi => "openai",
            ApiType::Anthropic => "anthropic",
            ApiType::Responses => "responses",
        }
    }

    /// Parse the octos wire spelling (exact, lowercase).
    pub fn parse(s: &str) -> Option<ApiType> {
        match s {
            "openai" => Some(ApiType::OpenAi),
            "anthropic" => Some(ApiType::Anthropic),
            "responses" => Some(ApiType::Responses),
            _ => None,
        }
    }
}

impl std::fmt::Display for ApiType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One model selection: an octos `LlmModelSelectionConfig` reduced to what a
/// person provisions. `key_env` is the env var octos reads this provider's
/// key from — normally [`registry::key_env_for`]`(family)`; it is written to
/// the profile as `route.api_key_env` only when it differs from that default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provider {
    pub family: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_type: Option<ApiType>,
    pub key_env: String,
}

impl Provider {
    /// A selection of `family` on its default route and key env var.
    pub fn new(family: impl Into<String>, model: Option<String>) -> Provider {
        let family = family.into();
        let key_env = registry::key_env_for(&family);
        Provider { family, model, base_url: None, api_type: None, key_env }
    }

    /// `family/model`, or just `family` when the model is the family default.
    pub fn label(&self) -> String {
        match &self.model {
            Some(m) => format!("{}/{m}", self.family),
            None => self.family.clone(),
        }
    }
}

/// The primary model plus its ordered fallbacks (octos `LlmProfileConfig`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSet {
    pub primary: Option<Provider>,
    pub fallbacks: Vec<Provider>,
}

impl ProviderSet {
    /// Primary first, then the fallbacks in order.
    pub fn iter(&self) -> impl Iterator<Item = &Provider> {
        self.primary.iter().chain(self.fallbacks.iter())
    }

    pub fn is_empty(&self) -> bool {
        self.primary.is_none() && self.fallbacks.is_empty()
    }
}

/// Everything that can go wrong. `Display` never contains a secret value:
/// messages name fields, env vars and file paths, never their contents.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An `OCTOS1E:` code was given without a PIN.
    #[error("this code is PIN-protected — enter the PIN shown beside it")]
    PinRequired,
    /// The PIN did not open the code. AEAD cannot tell a wrong PIN from a
    /// damaged ciphertext, so this also covers a code altered in transit.
    #[error("wrong PIN (or the code is damaged)")]
    WrongPin,
    /// The code is structurally corrupt: bad encoding, truncated, oversized.
    #[error("the code is damaged or has been altered ({0})")]
    Tampered(&'static str),
    /// A well-formed payload this device does not accept (foreign QR, other
    /// payload kind or version, server configuration).
    #[error("unsupported code: {0}")]
    Unsupported(String),
    /// Refused to put secrets in a plain (`OCTOS1:`) code.
    #[error("the provider set carries keys; use a PIN (OCTOS1E) or explicitly allow plain secrets")]
    SecretsNeedPin,
    /// Malformed input or configuration.
    #[error("invalid: {0}")]
    Invalid(String),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A model selection as a summary line carries it — no key material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryEntry {
    pub family: String,
    pub model: Option<String>,
}

/// What [`apply_to_profile`] wrote, safe to show and log: families, models
/// and the NAMES of the env vars that received keys, never their values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub primary: Option<SummaryEntry>,
    pub fallbacks: Vec<SummaryEntry>,
    pub key_envs: Vec<String>,
}

impl Summary {
    fn of(set: &ProviderSet, key_envs: Vec<String>) -> Summary {
        let entry = |p: &Provider| SummaryEntry { family: p.family.clone(), model: p.model.clone() };
        Summary {
            primary: set.primary.as_ref().map(entry),
            fallbacks: set.fallbacks.iter().map(entry).collect(),
            key_envs,
        }
    }
}

impl std::fmt::Display for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = |e: &SummaryEntry| match &e.model {
            Some(m) => format!("{}/{m}", e.family),
            None => e.family.clone(),
        };
        match &self.primary {
            Some(p) => write!(f, "{}", label(p))?,
            None => write!(f, "(no primary)")?,
        }
        if !self.fallbacks.is_empty() {
            let fb: Vec<String> = self.fallbacks.iter().map(label).collect();
            write!(f, " → {}", fb.join(", "))?;
        }
        if !self.key_envs.is_empty() {
            write!(f, " [keys: {}]", self.key_envs.join(", "))?;
        }
        Ok(())
    }
}

/// Merge a scanned provisioning into the profile at `path`: the provider set
/// replaces `config.llm`, every secret is upserted into `config.env_vars`,
/// everything else is kept. Returns a secret-free summary for the UI.
pub fn apply_to_profile(path: &Path, p: &qr::Provisioning) -> Result<Summary> {
    profile::save_merge(path, &p.set, &p.secrets)?;
    Ok(Summary::of(&p.set, p.secrets.keys().cloned().collect()))
}

/// A legal env var name: `[A-Z_][A-Z0-9_]*`, at most 128 bytes.
pub(crate) fn is_env_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && !s.as_bytes()[0].is_ascii_digit()
        && s.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

/// A `serde_json` error rendered without the offending value: its messages
/// quote string contents ("invalid type: string \"sk-…\""), and a key is a
/// string. Field-name errors are kept verbatim — they only quote field names.
pub(crate) fn json_error(e: &serde_json::Error) -> String {
    let msg = e.to_string();
    if msg.starts_with("unknown field") || msg.starts_with("missing field")
        || msg.starts_with("duplicate field")
    {
        return msg;
    }
    let what = match e.classify() {
        serde_json::error::Category::Syntax => "malformed JSON",
        serde_json::error::Category::Eof => "truncated JSON",
        serde_json::error::Category::Data => "unexpected value",
        serde_json::error::Category::Io => "read error",
    };
    format!("{what} at line {} column {}", e.line(), e.column())
}
