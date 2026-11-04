//! ADR 0002 phase 1: one declaration, two containers.
//!
//! The manifest, the app-facing policy, the integrity checks and running a
//! package are the app contract, `octosense-app-contract` (OctoSense ADR
//! 0005), re-exported here as [`contract`] and item by item, so existing
//! paths keep working. This crate adds what only App Hub and the shell
//! need: the app's agent, the two containers, listings, research words and
//! the host service registry.
//!
//! An app that arrives after the build ships a bundle and a manifest. This
//! crate is the only thing that turns that manifest into what the app gets:
//! the settings for its splash isolate, and the profile for its octos agent
//! session. Both come from the same resolved [`AppPolicy`], so the app and
//! its agent can never have different reach.
//!
//! The order is always: [`AppManifest::parse`] → [`verify::admit`] →
//! [`policy::resolve`] → [`AppPolicy::isolate_settings`] and
//! [`AppPolicy::session_profile`]. Skipping a step is the bug this crate
//! exists to make hard.
//!
//! ```
//! use octosense_app_policy::{admit_and_resolve, HostLimits, RefuseAllSignatures};
//! let bundle = b"the card bundle bytes";
//! let manifest = format!(
//!     r#"{{"schema":1,"id":"forecast","version":"1.0.0","name":"Forecast",
//!         "integrity":{{"bundle_blake3":"{}"}},
//!         "capabilities":["storage","net"],
//!         "network":{{"hosts":["api.weather.example"]}}}}"#,
//!     octosense_app_policy::bundle_digest(bundle)
//! );
//! let limits = HostLimits::default().with_require_signature(false);
//! let policy = admit_and_resolve(&manifest, bundle, &limits, &RefuseAllSignatures).unwrap();
//! assert!(policy.allows_host("api.weather.example"));
//! assert!(!policy.allows_host("example.com"));
//! assert!(policy.agent.is_none(), "an app gets no agent unless it asks");
//! ```
pub use octosense_app_contract as contract;

pub mod agent;
pub mod assets;
pub mod bundle;
pub mod containers;
pub mod entry;
pub mod listing;
#[cfg(feature = "splash")]
pub mod splash_adapter;
pub mod manifest;
pub mod policy;
pub mod research;
pub mod services;
pub mod verify;

pub use agent::{
    AgentBundle, Confirm, ImplementedBy, Risk, Skill, SkillManifest, Supervision, ToolHost, ToolManifest, ToolSpec, AGENT_FILE,
    SKILLS_DIR, TOOLS_FILE,
};
pub use assets::{rewrite_assets, AssetServer, StaticAssets};
pub use bundle::{digest_dir, portable_path, MANIFEST_FILE};
pub use containers::{IsolateSettings, Provenance, SessionProfile};
pub use entry::{script_source, ASSETS_PLACEHOLDER, SCRIPT_ENTRY};
pub use listing::{agent_permission_lines, kernel_tool_words, privacy_summary, privacy_summary_with_tools, Listing, Publisher, LISTING_FILE};
pub use manifest::{
    check_reserved_id, short_id, AgentSpec, AgentWorkspace, AppManifest, ComponentDependency, ModelNeed, ModelSpec, ModelTier, ProfileMode, TaskModel,
    Triggers, KNOWN_CAPABILITIES, KNOWN_FEATURES, KNOWN_MODEL_NEEDS, RESERVED_NAMES, SCHEMA, SCHEMA_MINOR,
};
pub use policy::{is_kernel_tool_name, AgentPolicy, AppPolicy, HostLimits, KERNEL_TOOLS};
pub use research::{crawl_words, search_words, ResearchScope, RESEARCH_CATEGORIES};
pub use services::{is_host_service, service_words, MATRIX_ACTIONS, OCTOS_SERVICES};
pub use verify::{admit, admit_digest, bundle_digest, RefuseAllSignatures, SignatureVerifier};

/// The whole admission path for a bundle that is a directory: parse, check
/// the digest the caller computed over that directory, resolve.
pub fn admit_and_resolve_dir(
    manifest_json: &str,
    bundle_digest_hex: &str,
    limits: &HostLimits,
    verifier: &dyn SignatureVerifier,
) -> Result<AppPolicy, String> {
    let manifest = AppManifest::parse(manifest_json)?;
    verify::admit_digest(&manifest, bundle_digest_hex, verifier)?;
    policy::resolve(&manifest, limits)
}

/// The whole admission path in one call: parse, admit, resolve. A host that
/// uses this cannot forget the digest check.
pub fn admit_and_resolve(
    manifest_json: &str,
    bundle: &[u8],
    limits: &HostLimits,
    verifier: &dyn SignatureVerifier,
) -> Result<AppPolicy, String> {
    let manifest = AppManifest::parse(manifest_json)?;
    verify::admit(&manifest, bundle, verifier)?;
    policy::resolve(&manifest, limits)
}
