//! The octos provider families, as data.
//!
//! Mirrors `octos-llm/src/registry/*.rs` (`ProviderEntry`: name, aliases,
//! api_key_env, key_env_aliases, default_base_url, requires_api_key) at the
//! octos commit AppCard pins, plus each family's catalog default model
//! (`model_catalog.json` rows flagged `"default": true`). octos looks a key
//! up by ITS env var name, so these must match exactly; the drift test in
//! `tests/registry_drift.rs` re-reads the octos sources from the checkout
//! `OCTOS_SRC` names.
//!
//! Order matters the way it does in octos's `ALL`: coding-plan and region
//! variants come before their base family so an alias resolves to the most
//! specific family.

/// One provider family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Family {
    /// Canonical octos `family_id`.
    pub id: &'static str,
    /// Human-readable name for pickers.
    pub label: &'static str,
    /// Other names octos resolves to this family (case-insensitive).
    pub aliases: &'static [&'static str],
    /// The env var octos reads the key from; `None` for keyless families.
    pub key_env: Option<&'static str>,
    /// Further env var names octos also accepts for this family's key.
    pub key_env_aliases: &'static [&'static str],
    /// Endpoint used when the route has no `base_url`; `None` = must be given.
    pub default_base_url: Option<&'static str>,
    /// Model used when the selection has no `model_id`; `None` = must be given.
    pub default_model: Option<&'static str>,
    /// Whether octos refuses to construct the provider without a key.
    pub key_required: bool,
}

#[allow(clippy::too_many_arguments)]
const fn fam(
    id: &'static str,
    label: &'static str,
    aliases: &'static [&'static str],
    key_env: Option<&'static str>,
    key_env_aliases: &'static [&'static str],
    default_base_url: Option<&'static str>,
    default_model: Option<&'static str>,
    key_required: bool,
) -> Family {
    Family { id, label, aliases, key_env, key_env_aliases, default_base_url, default_model, key_required }
}

static ALL: &[Family] = &[
    fam("anthropic", "Anthropic", &[], Some("ANTHROPIC_API_KEY"), &[],
        Some("https://api.anthropic.com"), Some("claude-sonnet-4-20250514"), true),
    fam("openai", "OpenAI", &[], Some("OPENAI_API_KEY"), &[],
        Some("https://api.openai.com/v1"), Some("gpt-4o"), true),
    fam("gemini", "Google Gemini", &["google"], Some("GEMINI_API_KEY"), &[],
        Some("https://generativelanguage.googleapis.com/v1beta"), Some("gemini-2.5-flash"), true),
    // The "key" is a service-account JSON; octos keeps it in the OS keychain
    // (`keychain:` marker) when it saves the profile itself.
    fam("vertex", "Google Vertex AI", &["vertex-ai", "vertexai"], Some("VERTEX_SA_JSON"), &[],
        None, Some("gemini-2.5-flash"), true),
    fam("r9s", "r9s.ai", &["r9s.ai"], Some("R9S_API_KEY"), &[],
        Some("https://api.r9s.ai/v1"), Some("claude-sonnet-4-6"), true),
    fam("openrouter", "OpenRouter", &[], Some("OPENROUTER_API_KEY"), &[],
        Some("https://openrouter.ai/api/v1"), Some("anthropic/claude-sonnet-4-6"), true),
    fam("deepseek", "DeepSeek", &[], Some("DEEPSEEK_API_KEY"), &[],
        Some("https://api.deepseek.com/v1"), Some("deepseek-v4-flash"), true),
    fam("groq", "Groq", &[], Some("GROQ_API_KEY"), &[],
        Some("https://api.groq.com/openai/v1"), Some("llama-3.3-70b-versatile"), true),
    fam("moonshot-coding", "Kimi Coding Plan", &["kimi-coding"], Some("KIMI_CODING_API_KEY"),
        &["KIMI_API_KEY", "MOONSHOT_API_KEY"], Some("https://api.kimi.com/coding/v1"), Some("k3"), true),
    fam("moonshot", "Moonshot (Kimi)", &["kimi"], Some("MOONSHOT_API_KEY"), &["KIMI_API_KEY"],
        Some("https://api.moonshot.ai/v1"), Some("kimi-k2.5"), true),
    fam("dashscope", "Alibaba DashScope (Qwen)", &["qwen"], Some("DASHSCOPE_API_KEY"), &[],
        Some("https://dashscope.aliyuncs.com/compatible-mode/v1"), Some("qwen-max"), true),
    fam("minimax-cn", "MiniMax (China)", &["minimaxi"], Some("MINIMAX_CN_API_KEY"), &["MINIMAX_API_KEY"],
        Some("https://api.minimaxi.com/v1"), Some("MiniMax-M3"), true),
    fam("minimax", "MiniMax", &[], Some("MINIMAX_API_KEY"), &[],
        Some("https://api.minimax.io/v1"), Some("MiniMax-M3"), true),
    fam("zai-coding", "Z.ai GLM Coding Plan", &["z.ai-coding", "glm-coding"], Some("ZAI_CODING_API_KEY"),
        &["ZAI_API_KEY"], Some("https://api.z.ai/api/anthropic"), Some("glm-5.3"), true),
    fam("zhipu", "Zhipu GLM (China)", &["glm"], Some("ZHIPU_API_KEY"), &[],
        Some("https://open.bigmodel.cn/api/paas/v4"), Some("glm-4-plus"), true),
    fam("zai", "Z.ai", &["z.ai"], Some("ZAI_API_KEY"), &[],
        Some("https://api.z.ai/api/anthropic"), Some("glm-5-turbo"), true),
    fam("nvidia", "NVIDIA NIM", &["nim"], Some("NVIDIA_API_KEY"), &[],
        Some("https://integrate.api.nvidia.com/v1"), Some("meta/llama-3.3-70b-instruct"), true),
    fam("ollama", "Ollama", &[], None, &[],
        Some("http://localhost:11434/v1"), Some("llama3.2"), false),
    // Needs a base URL and a model; the key is optional (`--api-key` servers).
    fam("vllm", "vLLM", &[], Some("VLLM_API_KEY"), &[], None, None, false),
    fam("local", "Local (llama.cpp / LM Studio)",
        &["llamacpp", "llama.cpp", "llama-server", "llama_server", "lmstudio", "lm-studio", "openai-compatible"],
        None, &[], Some("http://127.0.0.1:8080/v1"), Some("local-default"), false),
];

/// Every family, in octos's resolution order.
pub fn all() -> &'static [Family] {
    ALL
}

/// Resolve a canonical id or alias, case-insensitively (octos `lookup`).
pub fn lookup(id_or_alias: &str) -> Option<&'static Family> {
    let lower = id_or_alias.trim().to_ascii_lowercase();
    ALL.iter()
        .find(|f| f.id == lower || f.aliases.iter().any(|a| a.eq_ignore_ascii_case(&lower)))
}

/// The env var octos reads `family`'s key from.
///
/// A registry family answers with its `key_env`; anything else (a keyless
/// family, a family this build does not know) gets `<FAMILY>_API_KEY` with
/// every character outside `[A-Z0-9_]` turned into `_` — a hyphen or a dot
/// cannot appear in an env var name.
pub fn key_env_for(family: &str) -> String {
    let found = lookup(family);
    if let Some(env) = found.and_then(|f| f.key_env) {
        return env.to_string();
    }
    let base = found.map(|f| f.id).unwrap_or(family.trim());
    let mut name: String = base
        .to_ascii_uppercase()
        .chars()
        .map(|c| if c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' { c } else { '_' })
        .collect();
    if name.is_empty() || name.as_bytes()[0].is_ascii_digit() {
        name.insert(0, '_');
    }
    name.push_str("_API_KEY");
    name
}

/// Whether `name` is a key env var some registry family reads (primary name
/// or alias).
pub fn is_known_key_env(name: &str) -> bool {
    ALL.iter()
        .any(|f| f.key_env == Some(name) || f.key_env_aliases.contains(&name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_resolve_to_the_right_key_env() {
        for (family, env) in [
            ("vertex", "VERTEX_SA_JSON"),
            ("vertex-ai", "VERTEX_SA_JSON"),
            ("z.ai", "ZAI_API_KEY"),
            ("zai", "ZAI_API_KEY"),
            ("glm", "ZHIPU_API_KEY"),
            ("qwen", "DASHSCOPE_API_KEY"),
            ("google", "GEMINI_API_KEY"),
            ("nim", "NVIDIA_API_KEY"),
            ("kimi", "MOONSHOT_API_KEY"),
            ("kimi-coding", "KIMI_CODING_API_KEY"),
            ("moonshot-coding", "KIMI_CODING_API_KEY"),
            ("glm-coding", "ZAI_CODING_API_KEY"),
            ("minimaxi", "MINIMAX_CN_API_KEY"),
            ("r9s.ai", "R9S_API_KEY"),
            ("DeepSeek", "DEEPSEEK_API_KEY"),
        ] {
            assert_eq!(key_env_for(family), env, "{family}");
        }
    }

    #[test]
    fn unknown_and_keyless_families_get_a_legal_fallback() {
        assert_eq!(key_env_for("some-new.vendor"), "SOME_NEW_VENDOR_API_KEY");
        assert_eq!(key_env_for("ollama"), "OLLAMA_API_KEY");
        assert_eq!(key_env_for("llama.cpp"), "LOCAL_API_KEY");
        assert_eq!(key_env_for("9x"), "_9X_API_KEY");
        for f in all() {
            assert!(crate::is_env_name(&key_env_for(f.id)), "{}", f.id);
        }
    }

    #[test]
    fn lookup_prefers_the_specific_family() {
        assert_eq!(lookup("z.ai-coding").unwrap().id, "zai-coding");
        assert_eq!(lookup("Z.AI").unwrap().id, "zai");
        assert_eq!(lookup("minimaxi").unwrap().id, "minimax-cn");
        assert!(lookup("nope").is_none());
    }

    #[test]
    fn ids_and_aliases_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for f in all() {
            assert!(seen.insert(f.id), "{}", f.id);
            for a in f.aliases {
                assert!(seen.insert(a), "{a}");
            }
        }
    }
}
