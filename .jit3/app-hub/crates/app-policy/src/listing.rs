//! What a store shows about an app, as the publisher wrote it.
//!
//! The manifest says what an app may DO and is the security-relevant file.
//! The listing says what an app IS: description, category, screenshots,
//! who publishes it, where it runs. It is reviewed like everything else in
//! the bundle, and the store shows it beside the permissions, never instead
//! of them. What the store says an app may do always comes from the resolved
//! manifest; the listing cannot claim otherwise.
use crate::agent::{Risk, Supervision, ToolSpec};
use crate::manifest::AppManifest;
use serde::{Deserialize, Serialize};

pub const LISTING_FILE: &str = "listing.json";
pub const LISTING_SCHEMA: u32 = 1;

pub const MAX_SUBTITLE: usize = 80;
pub const MAX_DESCRIPTION: usize = 4000;
pub const MAX_KEYWORDS: usize = 10;
pub const MAX_SCREENSHOTS: usize = 8;

/// The categories a store sorts by. Closed, so search and shelves agree.
pub const CATEGORIES: &[&str] = &[
    "productivity", "utilities", "photo-video", "news", "weather", "travel", "finance", "health", "education",
    "entertainment", "games", "social", "shopping", "lifestyle", "developer",
];

/// Where a card app can run: every shell that links the card host. A
/// publisher lists what they tested; the store shows it as-is.
pub const PLATFORMS: &[&str] = &["android", "ios", "macos", "windows", "linux", "openharmony", "web"];

/// Age ratings a listing may declare, coarse on purpose.
pub const AGE_RATINGS: &[&str] = &["all", "12+", "16+", "18+"];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Listing {
    pub schema: u32,
    /// One line under the name.
    #[serde(default)]
    pub subtitle: String,
    /// What the app does, for a person deciding whether to install it.
    pub description: String,
    pub category: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Bundle-relative paths to PNG screenshots, in display order.
    #[serde(default)]
    pub screenshots: Vec<String>,
    /// Bundle-relative path to a square PNG or SVG icon.
    #[serde(default)]
    pub icon: Option<String>,
    pub platforms: Vec<String>,
    pub publisher: Publisher,
    /// What changed in this version, shown as "what's new".
    #[serde(default)]
    pub release_notes: String,
    pub age_rating: String,
    /// An SPDX licence identifier, when the source is open.
    #[serde(default)]
    pub license: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Publisher {
    /// The person or organisation as a person reads it.
    pub name: String,
    /// Where a person gets help: a URL or an email address.
    pub support: String,
    /// Required, as it is on every store: where the app's privacy terms live.
    pub privacy_policy_url: String,
}

impl Listing {
    pub fn parse(json: &str) -> Result<Self, String> {
        let listing: Listing = serde_json::from_str(json).map_err(|e| format!("listing is not valid: {e}"))?;
        if listing.schema != LISTING_SCHEMA {
            return Err(format!("listing schema {} is not {}", listing.schema, LISTING_SCHEMA));
        }
        listing.check()?;
        Ok(listing)
    }

    /// The rules a listing must meet, in words a publisher can act on.
    pub fn check(&self) -> Result<(), String> {
        if self.description.trim().is_empty() {
            return Err("listing description is empty".into());
        }
        if self.description.chars().count() > MAX_DESCRIPTION {
            return Err(format!("listing description is over {MAX_DESCRIPTION} characters"));
        }
        if self.subtitle.chars().count() > MAX_SUBTITLE {
            return Err(format!("listing subtitle is over {MAX_SUBTITLE} characters"));
        }
        if !CATEGORIES.contains(&self.category.as_str()) {
            return Err(format!("listing category {:?} is not one of {:?}", self.category, CATEGORIES));
        }
        if self.keywords.len() > MAX_KEYWORDS {
            return Err(format!("listing has more than {MAX_KEYWORDS} keywords"));
        }
        if self.screenshots.len() > MAX_SCREENSHOTS {
            return Err(format!("listing has more than {MAX_SCREENSHOTS} screenshots"));
        }
        if self.platforms.is_empty() {
            return Err("listing names no platforms".into());
        }
        for platform in &self.platforms {
            if !PLATFORMS.contains(&platform.as_str()) {
                return Err(format!("listing platform {platform:?} is not one of {:?}", PLATFORMS));
            }
        }
        if !AGE_RATINGS.contains(&self.age_rating.as_str()) {
            return Err(format!("listing age rating {:?} is not one of {:?}", self.age_rating, AGE_RATINGS));
        }
        if self.publisher.name.trim().is_empty() {
            return Err("listing publisher name is empty".into());
        }
        if self.publisher.support.trim().is_empty() {
            return Err("listing publisher support (a URL or an email) is empty".into());
        }
        if !self.publisher.privacy_policy_url.starts_with("https://") {
            return Err("listing privacy policy must be an https URL".into());
        }
        for path in self.screenshots.iter().chain(self.icon.iter()) {
            if path.starts_with('/') || path.contains("..") || path.contains("://") {
                return Err(format!("listing asset {path:?} must be a plain bundle-relative path"));
            }
            let lower = path.to_ascii_lowercase();
            if !(lower.ends_with(".png") || lower.ends_with(".svg")) {
                return Err(format!("listing asset {path:?} must be a PNG or SVG"));
            }
        }
        Ok(())
    }
}

/// What an octos kernel tool a contained agent may keep
/// ([`crate::policy::KERNEL_TOOLS`]) lets it do, in the store's plain words
/// (`None`: not such a tool).
pub fn kernel_tool_words(tool: &str) -> Option<&'static str> {
    match tool {
        "ask_user_question" => Some("Ask you questions"),
        _ => None,
    }
}

/// Manifest-only privacy summary. Call [`privacy_summary_with_tools`] when
/// reviewed tools are available: tools can offer host Ask without `agent`.
pub fn privacy_summary(manifest: &AppManifest) -> Vec<String> {
    privacy_summary_with_tools(manifest, &[])
}

/// The store's privacy summary, derived from the admitted manifest and tools,
/// never publisher prose. A tool-bearing app without an agent profile can
/// still offer the host's consent-gated Ask surface; this does not start a
/// peer, grant background execution, or register a service executor.
pub fn privacy_summary_with_tools(manifest: &AppManifest, tools: &[ToolSpec]) -> Vec<String> {
    let has = |c: &str| manifest.capabilities.iter().any(|x| x == c);
    let mut lines = Vec::new();
    lines.push("Runs with its own local storage and quota. Capability and network declarations describe expected use; they are not permission limits.".into());
    if !manifest.network.hosts.is_empty() {
        lines.push(format!("Declared network destinations: {}.", manifest.network.hosts.join(", ")));
    } else {
        lines.push("No network destinations declared; this does not mean the app is offline.".into());
    }
    lines.push("Device access, connected-account scopes, external-write review and sharing with other apps still need the host's authorization.".into());
    lines.push("Available host services can contact external providers after their actual authorization checks, even when a usage declaration is absent.".into());
    let provider_services = manifest.capabilities.iter().any(|cap| {
        matches!(cap.as_str(), "auth" | "github" | "gmail" | "gcalendar" | "device_calendar" | "mail" | "images" | "web"
            | "news" | "youtube" | "model" | "research" | "crawl" | "octos.turn.start")
            || cap.starts_with("matrix.")
            || octosense_app_contract::palpo::SERVICES.contains(&cap.as_str())
    });
    let remote_agent_allowed = manifest.agent.as_ref().map_or(!tools.is_empty(), |agent| {
        !agent.model.as_ref().is_some_and(|model| model.local_only)
    });
    if provider_services || remote_agent_allowed {
        lines.push("Shared host services and assistants may send data to their providers, under the permissions you allow. Network declarations do not restrict those service requests.".to_string());
    }
    if has("ledger.read") {
        lines.push("Reads your shared data.".to_string());
    }
    for (cap, text) in [
        ("location", "Uses your location."),
        ("camera", "Uses the camera."),
        ("clipboard", "Uses the clipboard."),
        ("prompt", "May ask you questions."),
        ("images", "Shows pictures from any website its content links to."),
        ("web", "Opens web pages, which cannot reach back into the app."),
        ("microphone", "Records sound after your permission while the app is active."),
        ("audio", "Plays audio from its own files while the app is active; cannot record sound."),
        ("files", "Imports or exports files and photos you choose, and can open the system sharing chooser."),
        ("library", "Saves photos and videos to your photo library."),
        ("mail", "Reads and sends mail from accounts you add; it never sees your password."),
        ("auth", "Connects its own provider accounts or signs in to its developer's backend through the host; credentials stay with the host."),
        ("runtime", "Inspects the host's available APIs; this grants no access to their data or devices."),
        ("github", "Reads authorized GitHub repositories and requests your review before committing Markdown through the host."),
        ("gmail", "Reads authorized Gmail messages, keeps reply drafts and requests native host review before sending."),
        ("gcalendar", "Reads authorized Google calendars and requests your review before saving event changes through the host."),
        ("device_calendar", "Reads selected device calendars after your permission and requests your review before changing events. The OS calendar account may synchronize those changes with its provider."),
        ("calendar", "Reads and manages local calendar events through the device's Calendar service."),
        ("llm", "Manages the assistant's AI providers; it never sees your API keys."),
        ("news", "Reads news the device collects from its feeds and topics."),
        ("photos", "Reads Photos's own library and publishes photo collections."),
        ("youtube", "Searches YouTube videos and manages music recommendations; playback requires a tap."),
        ("wasm", "Runs its own functions in a sandbox on this device; they share the app's isolation, quotas and host consent checks."),
        ("glance", "Shows short cards on your glance screen; each opens only this app."),
        ("model", "Sends what you give it to the AI provider you configured, for one-off answers within a daily budget; it never sees your API keys."),
    ] {
        if has(cap) {
            lines.push(text.to_string());
        }
    }
    if has("research") || has("crawl") {
        let scope = manifest.shown_research_scope();
        if has("research") {
            lines.push(format!(
                "Searches {}; the device runs each search, within these limits.",
                crate::research::search_words(&scope)
            ));
        }
        if has("crawl") {
            lines.push(format!(
                "Crawls websites, {}: this reaches more of the web than searching.",
                crate::research::crawl_words(&scope)
            ));
        }
    }
    if manifest.capabilities.iter().any(|c| octosense_app_contract::palpo::SERVICES.contains(&c.as_str())) {
        lines.push("Uses your signed-in Matrix identity for the Palpo operations you allow; passwords, access tokens and fleet configuration stay with the host. Server permissions still apply.".into());
    }
    let matrix_reads = manifest
        .capabilities
        .iter()
        .any(|c| c.starts_with("matrix.") && !crate::services::MATRIX_ACTIONS.contains(&c.as_str()));
    let matrix_acts = manifest
        .capabilities
        .iter()
        .any(|c| crate::services::MATRIX_ACTIONS.contains(&c.as_str()));
    if matrix_reads {
        lines.push("Reads from your Matrix account, only in the rooms you allow.".to_string());
    }
    if matrix_acts {
        lines.push("Acts on your Matrix account, only in the rooms you allow.".to_string());
    }
    if has("octos.turn.start") {
        lines.push("Asks the device's assistant to work for it; the assistant's keys stay with the device.".to_string());
    } else if has("octos.session.open") || has("octos.session.history") {
        lines.push("Opens or reads its own conversations with the device's assistant, but cannot ask it to work.".to_string());
    }
    match &manifest.agent {
        Some(agent) => {
            let host_tools: Vec<&str> = agent.tools.iter().map(String::as_str).filter(|t| kernel_tool_words(t).is_none()).collect();
            lines.push(format!(
                "Runs an assistant limited to this app's own data{}.",
                if host_tools.is_empty() { String::new() } else { format!(" with {}", host_tools.join(", ")) }
            ));
            for words in agent.tools.iter().filter_map(|t| kernel_tool_words(t)) {
                lines.push(format!("Its assistant may {}.", words.to_lowercase()));
            }
        }
        None if !tools.is_empty() => {
            lines.push("Offers the host's Ask assistant for its admitted tools, only after you consent. Your conversation and tool results may be sent to your configured AI provider.".to_string());
            lines.push("No app-declared background assistant or automatic triggers.".to_string());
        }
        None => lines.push("Runs no assistant.".to_string()),
    }
    if manifest.agent.as_ref().and_then(|a| a.model.as_ref()).is_some_and(|m| m.local_only) {
        lines.push("Its assistant uses only models that run on your own devices.".to_string());
    }
    lines
}

/// The permission lines for an app's agent and tools, in plain words: a
/// background agent, tools that wait for approval, tools other assistants
/// may be allowed to call. Derived from the manifest and `tools.json`, never
/// from anything the app says about itself.
pub fn agent_permission_lines(manifest: &AppManifest, tools: &[ToolSpec]) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(agent) = &manifest.agent {
        if agent.background {
            let when = match (agent.triggers.schedule.is_empty(), agent.triggers.events.is_empty()) {
                (false, false) => "on a schedule and when new data arrives",
                (false, true) => "on a schedule",
                _ => "when new data arrives",
            };
            lines.push(format!(
                "Its assistant may work while the app is closed, {when}; only if you allow it, and you can turn it off."
            ));
        }
        if agent.model.as_ref().is_some_and(|m| m.local_only) {
            lines.push("Its assistant uses only models that run on your own devices.".to_string());
        }
    }
    let names = |risk: Option<Risk>, pick: &dyn Fn(&ToolSpec) -> bool| -> Vec<&str> {
        tools.iter().filter(|t| risk.is_none_or(|r| t.risk == r) && pick(t)).map(|t| t.name.as_str()).collect()
    };
    let destructive = names(None, &|t| t.supervision() == Supervision::HostApproval);
    if !destructive.is_empty() {
        lines.push(format!(
            "Can ask to {}: nothing of this runs until you approve it.",
            destructive.join(", ")
        ));
    }
    let app_confirmed = names(None, &|t| t.supervision() == Supervision::AppConfirmation);
    if !app_confirmed.is_empty() {
        lines.push(format!(
            "Asks you on its own screen before {}; when you are away, it waits for your approval in the app's conversation.",
            app_confirmed.join(", ")
        ));
    }
    let every_time = names(None, &|t| t.supervision().needs_person() && !t.auto_approvable);
    if !every_time.is_empty() {
        lines.push(format!("You approve every call of {} yourself: no standing rule can.", every_time.join(", ")));
    }
    let shared = names(None, &|t| t.shareable);
    if !shared.is_empty() {
        lines.push(format!("Offers {} to other assistants you allow.", shared.join(", ")));
    }
    let shared_private = names(None, &|t| t.shareable && t.private_data == Some(true));
    if !shared_private.is_empty() {
        lines.push(format!("{} can pass your private data to those assistants.", shared_private.join(", ")));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> String {
        r#"{"schema":1,"subtitle":"The photo-mode screen","description":"A camera viewfinder.","category":"photo-video",
            "keywords":["camera"],"screenshots":["screenshots/01.png"],"platforms":["android","macos"],
            "publisher":{"name":"ymote","support":"https://github.com/ymote/camera-card/issues","privacy_policy_url":"https://example.com/privacy"},
            "release_notes":"First release.","age_rating":"all","license":"Apache-2.0"}"#
            .to_string()
    }

    #[test]
    fn a_complete_listing_parses() {
        let listing = Listing::parse(&good()).unwrap();
        assert_eq!(listing.category, "photo-video");
        assert_eq!(listing.platforms.len(), 2);
    }

    #[test]
    fn the_closed_lists_are_enforced() {
        assert!(Listing::parse(&good().replace("photo-video", "toys")).unwrap_err().contains("category"));
        assert!(Listing::parse(&good().replace("\"android\"", "\"symbian\"")).unwrap_err().contains("platform"));
        assert!(Listing::parse(&good().replace("\"all\"", "\"any\"")).unwrap_err().contains("age rating"));
    }

    #[test]
    fn a_privacy_policy_must_be_https_and_assets_must_stay_in_the_bundle() {
        assert!(Listing::parse(&good().replace("https://example.com/privacy", "http://example.com/privacy")).unwrap_err().contains("https"));
        assert!(Listing::parse(&good().replace("screenshots/01.png", "../01.png")).unwrap_err().contains("bundle-relative"));
        assert!(Listing::parse(&good().replace("screenshots/01.png", "screenshots/01.gif")).unwrap_err().contains("PNG or SVG"));
    }

    #[test]
    fn unknown_fields_are_refused() {
        assert!(Listing::parse(&good().replace("\"license\"", "\"price\":0,\"license\"")).unwrap_err().contains("not valid"));
    }

    #[test]
    fn the_privacy_summary_comes_from_the_manifest_not_the_listing() {
        let m = AppManifest::parse(r#"{"schema":1,"id":"a","version":"1","name":"A","integrity":{"bundle_blake3":"00"},
            "capabilities":["storage","net","camera"],"network":{"hosts":["api.example"]}}"#).unwrap();
        let lines = privacy_summary(&m);
        assert!(lines.iter().any(|l| l.contains("api.example")));
        assert!(lines.iter().any(|l| l.contains("camera")));
        assert!(lines.iter().any(|l| l.contains("no assistant")));
        let quiet = AppManifest::parse(r#"{"schema":1,"id":"a","version":"1","name":"A","integrity":{"bundle_blake3":"00"}}"#).unwrap();
        let lines = privacy_summary(&quiet);
        assert!(lines.iter().any(|line| line.contains("own local storage and quota")));
        assert!(lines.contains(&"No network destinations declared; this does not mean the app is offline.".to_string()));
        assert!(lines.iter().any(|line| line.contains("Available host services can contact external providers")));
    }

    #[test]
    fn a_glance_app_is_told_as_showing_cards_that_open_only_it() {
        let m = AppManifest::parse(r#"{"schema":1,"id":"a","version":"1","name":"A","integrity":{"bundle_blake3":"00"},
            "capabilities":["glance"]}"#).unwrap();
        let lines = privacy_summary(&m);
        assert!(lines.contains(&"Shows short cards on your glance screen; each opens only this app.".to_string()), "{lines:?}");
        assert!(lines.contains(&"No network destinations declared; this does not mean the app is offline.".to_string()), "{lines:?}");
        let quiet = AppManifest::parse(r#"{"schema":1,"id":"a","version":"1","name":"A","integrity":{"bundle_blake3":"00"}}"#).unwrap();
        assert!(!privacy_summary(&quiet).iter().any(|l| l.contains("glance")));
    }

    #[test]
    fn a_research_app_is_told_what_it_searches_and_a_crawl_app_that_it_reaches_more() {
        let m = AppManifest::parse(r#"{"schema":1,"id":"a","version":"1","name":"A","integrity":{"bundle_blake3":"00"},
            "capabilities":["research"],"research":{"langs":["en","zh"],"categories":["news"],"max_age_days":7}}"#).unwrap();
        let lines = privacy_summary(&m);
        assert!(
            lines.contains(&"Searches news in English and Chinese, from the last 7 days; the device runs each search, within these limits.".to_string()),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|l| l.starts_with("Crawls")), "{lines:?}");
        // Host-mediated searches must not be described as offline.
        assert!(lines.contains(&"No network destinations declared; this does not mean the app is offline.".to_string()), "{lines:?}");

        let m = AppManifest::parse(r#"{"schema":1,"id":"a","version":"1","name":"A","integrity":{"bundle_blake3":"00"},
            "capabilities":["crawl"],"research":{"domains_allow":["docs.rs"],"max_depth":2,"max_pages":30}}"#).unwrap();
        let lines = privacy_summary(&m);
        assert!(
            lines.contains(&"Crawls websites, following links up to 2 deep and reading up to 30 pages a crawl, only on docs.rs: this reaches more of the web than searching.".to_string()),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|l| l.starts_with("Searches")), "{lines:?}");
    }

    #[test]
    fn a_model_app_is_told_its_inputs_go_to_your_ai_provider() {
        let m = AppManifest::parse(r#"{"schema":1,"id":"a","version":"1","name":"A","integrity":{"bundle_blake3":"00"},
            "capabilities":["model"]}"#).unwrap();
        let lines = privacy_summary(&m);
        assert!(
            lines.iter().any(|l| l.contains("to the AI provider you configured") && l.contains("never sees your API keys")),
            "{lines:?}"
        );
        // Model calls may leave the device even without a direct-network grant.
        assert!(lines.contains(&"No network destinations declared; this does not mean the app is offline.".to_string()), "{lines:?}");
        let quiet = AppManifest::parse(r#"{"schema":1,"id":"a","version":"1","name":"A","integrity":{"bundle_blake3":"00"}}"#).unwrap();
        assert!(!privacy_summary(&quiet).iter().any(|l| l.contains("AI provider")));
    }

    #[test]
    fn connected_apps_disclose_service_traffic_without_a_direct_network_grant() {
        for capability in ["auth", "github", "gmail", "gcalendar", "model", "research", "crawl", "octos.turn.start"] {
            let m = AppManifest::parse(&serde_json::json!({
                "schema":1,"id":"preview","version":"1","name":"Preview",
                "integrity":{"bundle_blake3":"00"},"capabilities":[capability]
            }).to_string()).unwrap();
            let lines = privacy_summary(&m);
            assert!(lines.iter().any(|line| line == "No network destinations declared; this does not mean the app is offline."), "{capability}: {lines:?}");
            assert!(lines.iter().any(|line| line.contains("may send data to their providers")), "{capability}: {lines:?}");
            assert!(!lines.iter().any(|line| line.contains("Never contacts") || line.starts_with("Contacts only:")));
            if ["auth", "github", "gmail", "gcalendar"].contains(&capability) {
                assert!(lines.iter().any(|line| line.starts_with("Connects its own") || line.starts_with("Reads authorized")));
            }
        }
    }

    #[test]
    fn a_direct_allowlist_does_not_claim_to_limit_shared_provider_requests() {
        let m = AppManifest::parse(r#"{"schema":1,"id":"a","version":"1","name":"A",
            "integrity":{"bundle_blake3":"00"},"capabilities":["net","github"],
            "network":{"hosts":["notes.example"]}}"#).unwrap();
        let lines = privacy_summary(&m);
        assert!(lines.contains(&"Declared network destinations: notes.example.".into()));
        assert!(lines.iter().any(|line| line.contains("Network declarations do not restrict those service requests")));
        assert!(lines.iter().any(|line| line.contains("GitHub repositories")));
    }

    #[test]
    fn assistant_provider_access_is_disclosed_but_local_ui_does_not_gain_it() {
        let mut m = AppManifest::parse(r#"{"schema":1,"id":"a","version":"1","name":"A",
            "integrity":{"bundle_blake3":"00"},"agent":{"profile":"read-only","tools":[]}}"#).unwrap();
        assert!(privacy_summary(&m).iter().any(|line| line.contains("may send data to their providers")));
        m.agent = None;
        m.capabilities = vec!["storage".into(), "glance".into()];
        let lines = privacy_summary(&m);
        assert!(lines.contains(&"No network destinations declared; this does not mean the app is offline.".into()));
        assert!(!lines.iter().any(|line| line.contains("may send data to their providers")));
    }

    #[test]
    fn tool_only_ask_is_distinct_from_no_assistant_and_a_local_only_agent() {
        let mut m = AppManifest::parse(r#"{"schema":1,"id":"a","version":"1","name":"A",
            "integrity":{"bundle_blake3":"00"}}"#).unwrap();
        let tools: Vec<ToolSpec> = serde_json::from_str(r#"[{
            "name":"a.read","description":"Read local data",
            "input_schema":{"type":"object"},"output_schema":{"type":"object"},
            "risk":"read","implemented_by":"app"
        }]"#).unwrap();
        let lines = privacy_summary_with_tools(&m, &tools);
        assert!(lines.iter().any(|line| line.contains("Ask assistant") && line.contains("consent")));
        assert!(lines.iter().any(|line| line.contains("may send data to their providers")));
        assert!(!lines.contains(&"Runs no assistant.".to_string()));
        assert!(privacy_summary_with_tools(&m, &[]).contains(&"Runs no assistant.".to_string()));
        m.agent = serde_json::from_str(r#"{"profile":"read-only","tools":[],"model":{"local_only":true}}"#).unwrap();
        let lines = privacy_summary_with_tools(&m, &tools);
        assert!(lines.iter().any(|line| line.contains("only models that run on your own devices")));
        assert!(!lines.iter().any(|line| line.contains("may send data to their providers") || line.contains("Ask assistant")));
    }
}
