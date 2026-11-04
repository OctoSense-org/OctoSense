//! What a publisher submits, and what the hub publishes.
//!
//! An index entry is one app version: the manifest that governs it, the hash
//! of the bundle it describes, who published it, where the source lives, and
//! whether it is still offered. A catalog is the signed list of entries a
//! device reads.
//!
//! The manifest is embedded rather than referenced so that what a reviewer
//! read, what the hub signed and what the device enforces are the same bytes.
use octosense_app_policy::{AppManifest, Listing, ToolSpec};
use serde::{Deserialize, Serialize};

/// Whether this version is still offered, and why not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "state", content = "reason")]
pub enum Status {
    /// Offered for install.
    Offered,
    /// No longer offered and, on a device, no longer runnable. The reason is
    /// shown to the person, so it is written for them.
    Withdrawn(String),
}

impl Status {
    pub fn is_offered(&self) -> bool {
        matches!(self, Status::Offered)
    }
}

/// One app version in the index.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The manifest exactly as the publisher signed it.
    pub manifest: AppManifest,
    /// The listing as reviewed: what the store shows about the app. The
    /// permissions the store shows still come from the manifest.
    #[serde(default)]
    pub listing: Option<Listing>,
    /// The app's tool manifest (`tools.json`) as reviewed, so a store can
    /// say which tools wait for approval or may be shared before anything is
    /// installed. A display projection: `entry_for` omits `host_method` so
    /// older stores can still read the catalog. It is never a dispatch table.
    /// Omitted when the app ships no tools, so an entry without them serialises,
    /// and signs, as it did before the field existed. The bundle's exact copy,
    /// including its dispatch bindings, is authoritative: the digest pins it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolSpec>,
    /// Where the hub's own copy of the bundle lives, relative to the catalog.
    pub artifact: String,
    /// The publisher's key identity, as the hub knows it. Update continuity
    /// is checked against this.
    pub publisher: String,
    /// The publisher's public key, hex. Distributed IN the catalog because
    /// the catalog is signed: a device can then check the publisher's
    /// signature itself, and an update signed by a different key is visible
    /// rather than silent. Empty when the app was admitted unsigned.
    #[serde(default)]
    pub publisher_key: String,
    /// Where the source lives, for transparency. Never fetched at install.
    pub source: Source,
    pub status: Status,
    /// When the hub admitted it, as an ISO 8601 date.
    pub admitted: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub repository: String,
    pub commit: String,
}

impl Entry {
    pub fn app_id(&self) -> &str {
        &self.manifest.id
    }

    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    /// What the person is told this app may do, in plain words, derived from
    /// the manifest and reviewed tools rather than app-authored descriptions.
    pub fn permissions_summary(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for capability in &self.manifest.capabilities {
            lines.push(match capability.as_str() {
                "storage" => "Keep its own data on this device".to_string(),
                "files" => "Import or export files and photos you choose, and open the system sharing chooser".to_string(),
                "net" if self.manifest.network.hosts.is_empty() => "Reach the network: nothing listed".to_string(),
                "net" => format!("Reach only: {}", self.manifest.network.hosts.join(", ")),
                "prompt" => "Ask you questions".to_string(),
                "ledger.read" => "Read your shared data".to_string(),
                "location" => "Use your location".to_string(),
                "camera" => "Use the camera".to_string(),
                "clipboard" => "Use the clipboard".to_string(),
                "images" => "Show pictures from any website".to_string(),
                "web" => "Open web pages in a browser view".to_string(),
                "microphone" => "Use the microphone".to_string(),
                "audio" => "Play its own audio files while the app is active".to_string(),
                "library" => "Save to your photo library, where other apps can see it".to_string(),
                "mail" => "Read and send mail from accounts you sign in to on the device".to_string(),
                "calendar" => "Read and manage local events through the device's Calendar service".to_string(),
                "llm" => "Manage the assistant's AI providers, whose keys stay with the device".to_string(),
                "news" => "Read news the device collects from its feeds and topics".to_string(),
                "photos" => "Read Photos's own library and publish collections".to_string(),
                "youtube" => "Search YouTube and manage music recommendations".to_string(),
                "wasm" => "Run its own sandboxed functions on this device; they reach only what the app itself may".to_string(),
                "sheet" => "Use the device's spreadsheet engine on its own files".to_string(),
                "photo" => "Use the device's image-editing engine on its own files".to_string(),
                "word" => "Use the device's document engine on its own files".to_string(),
                "deck" => "Use the device's presentation engine on its own files".to_string(),
                "cad" => "Use the device's CAD engine on its own files".to_string(),
                "light" => "Use the device's raw-photo engine on its own files".to_string(),
                "sound" => "Use the device's audio-editing engine on its own files".to_string(),
                "design" => "Use the device's page-layout engine on its own files".to_string(),
                "film" => "Use the device's video-editing engine on its own files".to_string(),
                "effect" => "Use the device's motion-graphics engine on its own files".to_string(),
                "vector" => "Use the device's vector-drawing engine on its own files".to_string(),
                "pdf" => "Use the device's PDF engine on its own files".to_string(),
                "glance" => "Show cards on your glance screen".to_string(),
                "model" => "Send what you give it to the AI provider you configured, within a daily budget".to_string(),
                "research" => format!("Search {}", octosense_app_policy::search_words(&self.manifest.shown_research_scope())),
                "crawl" => format!(
                    "Crawl websites, {}, which reaches more than searching",
                    octosense_app_policy::crawl_words(&self.manifest.shown_research_scope())
                ),
                other => match octosense_app_policy::service_words(other) {
                    Some(words) => words.to_string(),
                    None => format!("Use {other}"),
                },
            });
        }
        if let Some(agent) = &self.manifest.agent {
            lines.push("Run an assistant for this app, only after you allow it".to_string());
            // The app's own tools come from tools.json. agent.tools adds
            // kernel tools and requests for host or other apps' tools; an empty
            // list does not mean its assistant has no tools.
            if !self.tools.is_empty() {
                let own_tools: Vec<&str> = self.tools.iter().map(|tool| tool.name.as_str()).collect();
                lines.push(format!("Its assistant can use these app tools: {}", own_tools.join(", ")));
            }
            let additional_tools: Vec<&str> =
                agent.tools.iter().map(String::as_str).filter(|t| octosense_app_policy::kernel_tool_words(t).is_none()).collect();
            if !additional_tools.is_empty() {
                lines.push(format!("Its assistant requests these additional tools: {}", additional_tools.join(", ")));
            }
            // The kernel tools it keeps, in plain words ("Ask you
            // questions"), once even when `prompt` says the same.
            for words in agent.tools.iter().filter_map(|t| octosense_app_policy::kernel_tool_words(t)) {
                if !lines.iter().any(|l| l == words) {
                    lines.push(words.to_string());
                }
            }
        }
        lines.extend(octosense_app_policy::agent_permission_lines(&self.manifest, &self.tools));
        // Shared components run as the app's own functions, with its grants
        // and nothing more; the line says whose reviewed code it runs.
        if !self.manifest.components.is_empty() {
            let pinned: Vec<String> = self.manifest.components.iter().map(|c| format!("{} {}", c.id, c.version)).collect();
            lines.push(format!("Runs shared components App Hub reviewed, with only this app's permissions: {}", pinned.join(", ")));
        }
        if lines.is_empty() {
            lines.push("Draw its screens, and nothing else".to_string());
        }
        lines
    }
}

/// The signed list a device reads. `signature` covers [`Catalog::signing_bytes`].
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub schema: u32,
    /// Increases with every publish; a device refuses to go backwards, so a
    /// replayed older catalog cannot un-withdraw an app.
    pub sequence: u64,
    /// When this catalog was signed, ISO 8601. The device's freshness window
    /// is measured from here.
    pub published: String,
    pub entries: Vec<Entry>,
    /// Shared components (App Hub ADR 0003): component versions apps pin,
    /// beside the apps. Skipped when empty, so a catalog without them
    /// serialises, signs and attests exactly as it did before the field
    /// existed. A host that predates the field refuses a catalog that has
    /// one, as it refuses any entry it cannot read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<crate::components::ComponentEntry>,
    /// Hex ed25519 signature by the hub's working key.
    #[serde(default)]
    pub signature: Option<String>,
    /// The working key that signed it, and the anchor's certificate for it.
    #[serde(default)]
    pub key: Option<WorkingKey>,
}

/// The hub's day-to-day signing key, certified by the offline anchor. A
/// device trusts the anchor only; rotating the working key is then a signed
/// statement it already knows how to check, with no shell release.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkingKey {
    /// Hex ed25519 public key.
    pub public: String,
    /// Hex ed25519 signature by the ANCHOR over the working public key bytes.
    pub anchor_certificate: String,
}

pub const CATALOG_SCHEMA: u32 = 1;

impl Catalog {
    pub fn new(sequence: u64, published: &str, entries: Vec<Entry>) -> Self {
        Catalog {
            schema: CATALOG_SCHEMA,
            sequence,
            published: published.to_string(),
            entries,
            components: Vec::new(),
            signature: None,
            key: None,
        }
    }

    /// The bytes the hub signs: the catalog without its signature or key, in
    /// canonical form, so the same catalog always signs the same way.
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        let mut bare = self.clone();
        bare.signature = None;
        bare.key = None;
        let value = serde_json::to_value(&bare).map_err(|e| e.to_string())?;
        Ok(canonical(&value).into_bytes())
    }

    /// The offered entry for an app id, if any.
    pub fn offered(&self, app_id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.app_id() == app_id && e.status.is_offered())
    }

    /// The entry for exactly this component version, offered or not.
    pub fn component(&self, id: &str, version: &str) -> Option<&crate::components::ComponentEntry> {
        crate::components::find(self, id, version)
    }
}

/// Canonical JSON: sorted keys, no insignificant whitespace.
pub(crate) fn canonical(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let body: Vec<String> = keys
                .iter()
                .map(|k| format!("{}:{}", serde_json::Value::String((*k).clone()), canonical(&map[*k])))
                .collect();
            format!("{{{}}}", body.join(","))
        }
        serde_json::Value::Array(items) => format!("[{}]", items.iter().map(canonical).collect::<Vec<_>>().join(",")),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_capability_is_told_in_plain_words() {
        let manifest = AppManifest::parse(
            &serde_json::json!({
                "schema": 1, "id": "dev.example.app", "version": "1", "name": "App",
                "integrity": {"bundle_blake3": ""},
                "capabilities": octosense_app_policy::KNOWN_CAPABILITIES,
                "network": {"hosts": ["api.example.com"]},
                "research": {"langs": ["en", "zh"], "categories": ["news"], "max_age_days": 7, "max_depth": 2, "max_pages": 20}
            })
            .to_string(),
        )
        .unwrap();
        let entry = Entry {
            artifact: String::new(),
            manifest,
            listing: None,
            tools: Vec::new(),
            publisher: String::new(),
            publisher_key: String::new(),
            source: Source { repository: String::new(), commit: String::new() },
            status: Status::Offered,
            admitted: String::new(),
        };
        let lines = entry.permissions_summary();
        assert_eq!(lines.len(), octosense_app_policy::KNOWN_CAPABILITIES.len());
        for (capability, line) in octosense_app_policy::KNOWN_CAPABILITIES.iter().zip(&lines) {
            assert_ne!(line, &format!("Use {capability}"), "{capability} has no plain-words line");
        }
        assert!(lines.contains(&"Search news in English and Chinese, from the last 7 days".to_string()), "{lines:?}");
        assert!(lines.contains(&"Ask you questions".to_string()), "prompt: {lines:?}");
        assert!(
            lines.contains(
                &"Crawl websites, following links up to 2 deep and reading up to 20 pages a crawl, on any site, which reaches more than searching"
                    .to_string()
            ),
            "{lines:?}"
        );
    }

    /// An agent that keeps `ask_user_question` is told as asking you
    /// questions (once, even with `prompt`), not by the tool's name.
    #[test]
    fn an_agent_that_asks_questions_is_told_so() {
        let entry = |capabilities: serde_json::Value| Entry {
            artifact: String::new(),
            manifest: AppManifest::parse(
                &serde_json::json!({
                    "schema": 1, "id": "dev.example.app", "version": "1", "name": "App",
                    "integrity": {"bundle_blake3": ""}, "capabilities": capabilities,
                    "agent": {"profile": "read-only", "tools": ["ask_user_question"]}
                })
                .to_string(),
            )
            .unwrap(),
            listing: None,
            tools: Vec::new(),
            publisher: String::new(),
            publisher_key: String::new(),
            source: Source { repository: String::new(), commit: String::new() },
            status: Status::Offered,
            admitted: String::new(),
        };
        let lines = entry(serde_json::json!([])).permissions_summary();
        assert!(lines.contains(&"Run an assistant for this app, only after you allow it".to_string()), "{lines:?}");
        assert!(!lines.iter().any(|line| line.contains("no tools") || line.contains("these app tools")), "{lines:?}");
        assert!(lines.contains(&"Ask you questions".to_string()), "{lines:?}");
        assert!(!lines.iter().any(|l| l.contains("ask_user_question")), "{lines:?}");
        let lines = entry(serde_json::json!(["prompt"])).permissions_summary();
        assert_eq!(lines.iter().filter(|l| *l == "Ask you questions").count(), 1, "{lines:?}");
    }
}
