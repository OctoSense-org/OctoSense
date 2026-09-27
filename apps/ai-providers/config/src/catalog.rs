//! octos's model catalog (`model_catalog.json`, vendored in `data/`), as the
//! pickers show it: each provider family with its models, and each model with
//! its context window, price, tier and the routes (endpoints) it is served on.
//!
//! The catalog names a model `family/model` (split on the first `/`: model
//! ids may carry further slashes, `nvidia/meta/llama-3.3-70b-instruct`). It
//! has no display names, so [`model_label`] makes one. Families are
//! canonicalised through [`registry`]; a family the registry does not know is
//! skipped (octos would not construct it either).
//!
//! Routes: a model may list `endpoints` (`{id, label, base_url?,
//! api_key_env?}`). The one whose id is the family's own id (or `official`)
//! is the family's official API; octos addresses a selection with no
//! `route_id` as the synthetic `"official"` route (octos-cli
//! `same_llm_selection_address`), so the official route is saved as no route
//! at all. Every other endpoint is saved with its `route_id`, `label`,
//! `base_url` and `api_key_env` (octos `LlmRouteConfig`). A model with no
//! endpoints is served on the official route only.

use crate::registry::{self, Family};
use serde::Deserialize;
use std::sync::OnceLock;

/// The vendored catalog, byte for byte as octos ships it.
pub const CATALOG_JSON: &str = include_str!("../data/model_catalog.json");

/// The id octos gives the default route of a family.
pub const OFFICIAL: &str = "official";

/// Strong models reason; fast ones are cheap and quick (octos `type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Strong,
    Fast,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Strong => "strong",
            Tier::Fast => "fast",
        }
    }
}

/// One endpoint a model is served on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// The catalog's endpoint id; [`OFFICIAL`] for the family's own API.
    pub id: String,
    pub label: String,
    /// `None`: the family's default endpoint.
    pub base_url: Option<String>,
    /// `None`: the family's own key env var.
    pub api_key_env: Option<String>,
}

impl Route {
    /// The family's own API: saved without a route.
    pub fn is_official(&self) -> bool {
        self.id == OFFICIAL
    }

    fn official(label: &str) -> Route {
        Route { id: OFFICIAL.into(), label: label.into(), base_url: None, api_key_env: None }
    }
}

/// One catalog model.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// Canonical registry family id.
    pub family: &'static str,
    /// The model id octos sends (`deepseek-v4-flash`).
    pub id: String,
    /// A display name (`DeepSeek V4 Flash`).
    pub label: String,
    pub tier: Tier,
    /// The family's default model.
    pub default: bool,
    pub context_window: Option<u64>,
    pub max_output: Option<u64>,
    /// USD per 1M input / output tokens.
    pub cost_in: f64,
    pub cost_out: f64,
    /// The catalog's endpoints, official first; empty when it lists none.
    pub endpoints: Vec<Route>,
}

impl Model {
    /// The routes to offer: the catalog's endpoints (official first), or the
    /// official route alone.
    pub fn routes(&self) -> Vec<Route> {
        if self.endpoints.is_empty() {
            let label = family(self.family).map(|f| f.official_label()).unwrap_or("Official API");
            vec![Route::official(label)]
        } else {
            self.endpoints.clone()
        }
    }

    /// `1M context`, or empty when the catalog does not say.
    pub fn context_text(&self) -> String {
        self.context_window.map(|n| format!("{} context", format_tokens(n))).unwrap_or_default()
    }

    /// `$0.14 / $0.28 per 1M` (input / output), or what a zero price means.
    pub fn price_text(&self) -> String {
        price_text(self.family, self.cost_in, self.cost_out)
    }
}

/// One provider family with its catalog models.
#[derive(Debug, Clone)]
pub struct CatalogFamily {
    pub family: &'static Family,
    /// Catalog order.
    pub models: Vec<Model>,
}

impl CatalogFamily {
    pub fn id(&self) -> &'static str {
        self.family.id
    }

    pub fn label(&self) -> &'static str {
        self.family.label
    }

    /// The catalog's default, else the registry's, else the first model.
    pub fn default_model(&self) -> Option<&str> {
        self.models
            .iter()
            .find(|m| m.default)
            .map(|m| m.id.as_str())
            .or(self.family.default_model)
            .or_else(|| self.models.first().map(|m| m.id.as_str()))
    }

    pub fn model(&self, id: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.id == id)
    }

    /// Every route any of the family's models is served on, official first
    /// (what a model the catalog does not list is offered).
    pub fn routes(&self) -> Vec<Route> {
        let mut out = vec![Route::official(self.official_label())];
        for m in &self.models {
            for r in &m.endpoints {
                if !out.iter().any(|o| o.id == r.id) {
                    out.push(r.clone());
                }
            }
        }
        out
    }

    /// The official route's name: the catalog's, else "Official API" ("Local
    /// server" for a keyless family, "Your endpoint" for one with no default).
    pub fn official_label(&self) -> &str {
        let fallback = if self.family.default_base_url.is_none() {
            "Your endpoint"
        } else if !self.family.key_required {
            "Local server"
        } else {
            "Official API"
        };
        self.models.iter().flat_map(|m| &m.endpoints).find(|r| r.is_official()).map(|r| r.label.as_str()).unwrap_or(fallback)
    }

    /// The routes `model` is offered on: its own, or the family's for a model
    /// the catalog does not list.
    pub fn routes_for(&self, model: &str) -> Vec<Route> {
        match self.model(model) {
            Some(m) if !m.endpoints.is_empty() => m.routes(),
            Some(_) => vec![Route::official(self.official_label())],
            None => self.routes(),
        }
    }
}

#[derive(Deserialize)]
struct RawCatalog {
    models: Vec<RawModel>,
}

#[derive(Deserialize)]
struct RawModel {
    provider: String,
    #[serde(rename = "type")]
    tier: Option<String>,
    #[serde(default)]
    default: bool,
    #[serde(default)]
    cost_in: f64,
    #[serde(default)]
    cost_out: f64,
    context_window: Option<u64>,
    max_output: Option<u64>,
    #[serde(default)]
    endpoints: Vec<RawEndpoint>,
}

#[derive(Deserialize)]
struct RawEndpoint {
    id: String,
    label: String,
    base_url: Option<String>,
    api_key_env: Option<String>,
}

/// Parse a catalog in octos's shape. Families the registry does not know
/// are skipped; the result follows the registry's family order.
pub fn parse(json: &str) -> Result<Vec<CatalogFamily>, String> {
    let raw: RawCatalog = serde_json::from_str(json).map_err(|e| crate::json_error(&e))?;
    let mut out: Vec<CatalogFamily> =
        registry::all().iter().map(|family| CatalogFamily { family, models: Vec::new() }).collect();
    for m in raw.models {
        let Some((family, id)) = m.provider.split_once('/') else { continue };
        let Some(fam) = registry::lookup(family) else { continue };
        let slot = out.iter_mut().find(|f| f.family.id == fam.id).expect("every registry family has a slot");
        let mut endpoints: Vec<Route> = m
            .endpoints
            .into_iter()
            .map(|e| {
                let official = e.id == OFFICIAL || registry::lookup(&e.id).is_some_and(|f| f.id == fam.id);
                Route {
                    id: if official { OFFICIAL.into() } else { e.id },
                    label: e.label,
                    // The official route is the family's own endpoint and key.
                    base_url: if official { None } else { e.base_url.filter(|u| !u.trim().is_empty()) },
                    api_key_env: if official { None } else { e.api_key_env.filter(|k| crate::is_env_name(k)) },
                }
            })
            .collect();
        endpoints.sort_by_key(|r| !r.is_official());
        slot.models.push(Model {
            family: fam.id,
            label: model_label(id),
            id: id.to_string(),
            tier: if m.tier.as_deref() == Some("fast") { Tier::Fast } else { Tier::Strong },
            default: m.default,
            context_window: m.context_window.filter(|n| *n > 0),
            max_output: m.max_output.filter(|n| *n > 0),
            cost_in: m.cost_in.max(0.0),
            cost_out: m.cost_out.max(0.0),
            endpoints,
        });
    }
    Ok(out)
}

/// The vendored catalog, parsed once. Every registry family, in registry
/// order (a family the catalog lists no model for has none).
pub fn families() -> &'static [CatalogFamily] {
    static CATALOG: OnceLock<Vec<CatalogFamily>> = OnceLock::new();
    CATALOG.get_or_init(|| parse(CATALOG_JSON).expect("the vendored model catalog parses"))
}

/// `id_or_alias`'s catalog family.
pub fn family(id_or_alias: &str) -> Option<&'static CatalogFamily> {
    let id = registry::lookup(id_or_alias)?.id;
    families().iter().find(|f| f.family.id == id)
}

/// `family`'s catalog entry for `model`.
pub fn model(family_id: &str, model: &str) -> Option<&'static Model> {
    family(family_id)?.model(model)
}

/// A route's display name for a saved selection: its own label, the
/// catalog's name for its id, "Custom endpoint" for a bare base URL, else
/// the family's official route.
pub fn route_label(family_id: &str, route_id: Option<&str>, label: Option<&str>, base_url: Option<&str>) -> String {
    if let Some(l) = label.filter(|l| !l.trim().is_empty()) {
        return l.to_string();
    }
    let fam = family(family_id);
    if let Some(id) = route_id.filter(|id| *id != OFFICIAL && registry::lookup(id).map(|f| f.id) != fam.map(|f| f.id())) {
        if let Some(r) = fam.and_then(|f| f.routes().into_iter().find(|r| r.id == id)) {
            return r.label;
        }
        return title_case(id);
    }
    if base_url.is_some_and(|u| !u.trim().is_empty()) {
        let default = fam.and_then(|f| f.family.default_base_url);
        if base_url.map(|u| u.trim().trim_end_matches('/')) != default.map(|u| u.trim_end_matches('/')) {
            return "Custom endpoint".into();
        }
    }
    fam.map(|f| f.official_label().to_string()).unwrap_or_else(|| "Official API".into())
}

/// Tokens as people say them: `1M`, `10M`, `128K`, `200K`, `4K`.
pub fn format_tokens(n: u64) -> String {
    const MI: u64 = 1024 * 1024;
    let trim = |v: f64| {
        let s = if v >= 10.0 { format!("{v:.0}") } else { format!("{v:.1}") };
        s.trim_end_matches(".0").to_string()
    };
    if n >= 1_000_000 {
        let v = if n.is_multiple_of(MI) { (n / MI) as f64 } else { n as f64 / 1_000_000.0 };
        format!("{}M", trim(v))
    } else if n >= 1000 {
        // Decimal when the number is round (`128000`), else binary (`131072`).
        let v = if !n.is_multiple_of(1000) && n.is_multiple_of(1024) { (n / 1024) as f64 } else { n as f64 / 1000.0 };
        format!("{}K", trim(v).trim_end_matches(".0"))
    } else {
        n.to_string()
    }
}

/// `$0.14 / $0.28 per 1M`; a zero price reads as what it means for the
/// family (a coding plan's flat fee, a local server, a free tier).
pub fn price_text(family_id: &str, cost_in: f64, cost_out: f64) -> String {
    if cost_in <= 0.0 && cost_out <= 0.0 {
        let fam = registry::lookup(family_id);
        return match fam {
            Some(f) if f.id.ends_with("-coding") => "Included in plan".into(),
            Some(f) if !f.key_required => "Runs locally".into(),
            _ => "Free".into(),
        };
    }
    format!("{} / {} per 1M", dollars(cost_in), dollars(cost_out))
}

fn dollars(v: f64) -> String {
    if v > 0.0 && v < 0.01 {
        format!("${v:.3}")
    } else {
        format!("${v:.2}")
    }
}

fn title_case(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|p| !p.is_empty())
        .map(capitalize)
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Names that are not simply capitalised.
fn word(token: &str) -> Option<&'static str> {
    Some(match token {
        "deepseek" => "DeepSeek",
        "minimax" => "MiniMax",
        "glm" => "GLM",
        "gpt" => "GPT",
        "it" => "IT",
        "for" => "for",
        "codellama" => "Code Llama",
        "llama3.1" => "Llama 3.1",
        "llama3.2" => "Llama 3.2",
        "phi3" => "Phi-3",
        "gemma2" => "Gemma 2",
        _ => return None,
    })
}

/// A readable name for a model id (the catalog has none): the vendor prefix
/// and a date suffix dropped, words capitalised, version numbers joined
/// (`claude-sonnet-4-5-20250929` → `Claude Sonnet 4.5`), known spellings
/// kept (`DeepSeek`, `GLM-5.3`, `GPT-4o`, `o3-mini`).
pub fn model_label(id: &str) -> String {
    match id {
        "local-default" => return "Server default".into(),
        "k3" => return "Kimi K3".into(),
        _ => {}
    }
    let name = id.rsplit('/').next().unwrap_or(id);
    let lower = name.to_ascii_lowercase();
    // OpenAI's reasoning models keep their spelling.
    if lower.len() >= 2 && lower.starts_with('o') && lower.as_bytes()[1].is_ascii_digit() {
        return lower;
    }
    let mut tokens: Vec<&str> = name.split('-').filter(|t| !t.is_empty()).collect();
    // A release date (`20250514`) is not part of the name.
    if tokens.len() > 1 && tokens.last().is_some_and(|t| t.len() == 8 && t.bytes().all(|b| b.is_ascii_digit())) {
        tokens.pop();
    }
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i];
        let tl = t.to_ascii_lowercase();
        // `glm-5.3`, `gpt-4o`: the family and its version are one word.
        if (tl == "glm" || tl == "gpt") && i + 1 < tokens.len() {
            out.push(format!("{}-{}", tl.to_ascii_uppercase(), tokens[i + 1]));
            i += 2;
            continue;
        }
        // `4-5` → `4.5`: consecutive small numbers are one version.
        if t.len() <= 2 && t.bytes().all(|b| b.is_ascii_digit()) {
            let mut v = t.to_string();
            while i + 1 < tokens.len() && tokens[i + 1].len() <= 2 && tokens[i + 1].bytes().all(|b| b.is_ascii_digit()) {
                v.push('.');
                v.push_str(tokens[i + 1]);
                i += 1;
            }
            out.push(v);
            i += 1;
            continue;
        }
        if let Some(w) = word(&tl) {
            out.push(w.to_string());
        } else if is_code(&tl) {
            out.push(tl.to_ascii_uppercase());
        } else {
            out.push(capitalize(t));
        }
        i += 1;
    }
    out.join(" ")
}

/// A size or version code (`70b`, `a22b`, `v4`, `k2.5`, `m2.5`, `16e`):
/// at most one letter before the digits, at most one after.
fn is_code(t: &str) -> bool {
    let b = t.as_bytes();
    if !b.iter().any(u8::is_ascii_digit) || !b.iter().any(u8::is_ascii_alphabetic) {
        return false;
    }
    let lead = b.iter().take_while(|c| c.is_ascii_alphabetic()).count();
    let tail = b.iter().rev().take_while(|c| c.is_ascii_alphabetic()).count();
    let middle = &b[lead..b.len() - tail];
    lead <= 1 && tail <= 1 && !middle.is_empty() && middle.iter().all(|c| c.is_ascii_digit() || *c == b'.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vendored_catalog_parses_and_every_family_is_the_registrys() {
        let raw: serde_json::Value = serde_json::from_str(CATALOG_JSON).unwrap();
        let rows = raw["models"].as_array().unwrap();
        let mut seen = std::collections::BTreeSet::new();
        for m in rows {
            let provider = m["provider"].as_str().unwrap();
            let (family, _) = provider.split_once('/').expect("family/model");
            assert!(registry::lookup(family).is_some(), "{provider}: family unknown to the registry");
            seen.insert(family.to_string());
        }
        let parsed = families_count(super::families());
        assert_eq!(parsed, rows.len(), "every row is kept");
        assert_eq!(seen.len(), 20);
        for f in super::families() {
            assert!(!f.models.is_empty(), "{} has catalog models", f.id());
            assert!(f.models.iter().filter(|m| m.default).count() <= 1, "{}: one default", f.id());
            if let Some(d) = f.family.default_model {
                assert!(f.model(d).is_some() || f.models.iter().any(|m| m.default), "{}: default {d}", f.id());
            }
            for m in &f.models {
                assert!(!m.label.is_empty() && m.routes()[0].is_official(), "{}/{}", f.id(), m.id);
            }
        }
    }

    fn families_count(f: &[CatalogFamily]) -> usize {
        f.iter().map(|f| f.models.len()).sum()
    }

    #[test]
    fn pinned_entries() {
        let ds = family("deepseek").unwrap();
        assert_eq!(ds.label(), "DeepSeek");
        assert_eq!(ds.default_model(), Some("deepseek-v4-flash"));
        let flash = ds.model("deepseek-v4-flash").unwrap();
        assert!(flash.default);
        assert_eq!(flash.tier, Tier::Fast);
        assert_eq!(flash.label, "DeepSeek V4 Flash");
        assert_eq!(flash.context_text(), "1M context");
        assert_eq!(flash.price_text(), "$0.14 / $0.28 per 1M");
        let routes = flash.routes();
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0], Route { id: "official".into(), label: "Official API".into(), base_url: None, api_key_env: None });
        assert_eq!(
            routes[1],
            Route {
                id: "autodl".into(),
                label: "AutoDL".into(),
                base_url: Some("https://www.autodl.art/api/v1".into()),
                api_key_env: Some("AUTODL_API_KEY".into()),
            }
        );
        assert_eq!(ds.model("deepseek-v4-pro").unwrap().label, "DeepSeek V4 Pro");

        // No endpoints: the official route alone.
        let zai = family("z.ai").unwrap();
        assert_eq!(zai.id(), "zai");
        assert_eq!(zai.default_model(), Some("glm-5-turbo"));
        assert_eq!(zai.model("glm-5.3").unwrap().routes(), vec![Route::official("Official API")]);
        // A coding plan's one endpoint is its own API, saved as no route.
        let plan = family("zai-coding").unwrap();
        assert_eq!(plan.model("glm-5.3").unwrap().routes()[0].id, OFFICIAL);
        assert_eq!(plan.official_label(), "GLM Coding Plan");
        assert_eq!(plan.model("glm-5.3").unwrap().price_text(), "Included in plan");
        assert_eq!(family("ollama").unwrap().models[0].price_text(), "Runs locally");
        assert_eq!(family("vllm").unwrap().default_model(), Some("Qwen/Qwen3-32B"));
        assert!(family("nope").is_none());
    }

    #[test]
    fn labels_read_like_names() {
        for (id, label) in [
            ("claude-sonnet-4-5-20250929", "Claude Sonnet 4.5"),
            ("claude-3-5-haiku-20241022", "Claude 3.5 Haiku"),
            ("claude-opus-4-6", "Claude Opus 4.6"),
            ("gpt-4o-mini", "GPT-4o Mini"),
            ("gpt-5.1-codex", "GPT-5.1 Codex"),
            ("o3-mini", "o3-mini"),
            ("glm-5.3-flash", "GLM-5.3 Flash"),
            ("glm-5-turbo", "GLM-5 Turbo"),
            ("kimi-k2.5", "Kimi K2.5"),
            ("MiniMax-M2.5-highspeed", "MiniMax M2.5 Highspeed"),
            ("qwen3-235b-a22b", "Qwen3 235B A22B"),
            ("meta/llama-3.3-70b-instruct", "Llama 3.3 70B Instruct"),
            ("gemini-2.5-flash-lite", "Gemini 2.5 Flash Lite"),
            ("llama3.2", "Llama 3.2"),
            ("kimi-for-coding-highspeed", "Kimi for Coding Highspeed"),
            ("deepseek-ai/deepseek-v3.2", "DeepSeek V3.2"),
        ] {
            assert_eq!(model_label(id), label, "{id}");
        }
    }

    #[test]
    fn tokens_and_prices() {
        for (n, text) in [(1_048_576, "1M"), (1_000_000, "1M"), (10_485_760, "10M"), (1_047_576, "1M"), (131_072, "128K"),
            (200_000, "200K"), (204_800, "200K"), (163_840, "160K"), (8192, "8K"), (128_000, "128K"), (400_000, "400K")]
        {
            assert_eq!(format_tokens(n), text, "{n}");
        }
        assert_eq!(price_text("openai", 2.5, 10.0), "$2.50 / $10.00 per 1M");
        assert_eq!(price_text("nvidia", 0.0, 0.0), "Free");
    }

    #[test]
    fn route_labels() {
        assert_eq!(route_label("deepseek", None, None, None), "Official API");
        assert_eq!(route_label("deepseek", Some("official"), None, None), "Official API");
        assert_eq!(route_label("deepseek", Some("deepseek"), None, None), "Official API");
        assert_eq!(route_label("deepseek", Some("autodl"), None, Some("https://www.autodl.art/api/v1")), "AutoDL");
        assert_eq!(route_label("deepseek", Some("autodl"), Some("AutoDL (mine)"), None), "AutoDL (mine)");
        assert_eq!(route_label("openai", None, None, Some("http://127.0.0.1:8080/v1")), "Custom endpoint");
        assert_eq!(route_label("openai", None, None, Some("https://api.openai.com/v1/")), "Official API");
        assert_eq!(route_label("zai-coding", None, None, None), "GLM Coding Plan");
        assert_eq!(route_label("ollama", None, None, None), "Local server");
        assert_eq!(route_label("vllm", None, None, Some("http://gpu:8000/v1")), "Custom endpoint");
        assert_eq!(family("vllm").unwrap().models[0].routes()[0].label, "Your endpoint");
    }
}
