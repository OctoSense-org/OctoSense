//! The OctoSense app contract: what an app or a host needs to read, check
//! and run an app package, and nothing else (OctoSense ADR 0005).
//!
//! App Hub, the OctoSense shell and every app that runs other apps (Rinx's
//! mini apps) depend on this crate by version, `octosense-app-contract =
//! "1"`, so Cargo links one `1.x` for the whole build and App Hub can change
//! its store, catalog and runner without forcing an app release.
//!
//! - **Manifest:** [`AppManifest`] and its parts, [`SCHEMA`],
//!   [`MANIFEST_FILE`], [`parse`].
//! - **Policy:** [`policy::resolve`], [`HostLimits`], [`AppPolicy`]: the
//!   capabilities, network hosts, storage and budgets an app gets.
//! - **Integrity:** [`digest_dir`], [`bundle_digest`], [`admit`],
//!   [`admit_digest`], [`SignatureVerifier`], [`RefuseAllSignatures`].
//! - **Running a package:** [`SCRIPT_ENTRY`], [`script_source`],
//!   [`ASSETS_PLACEHOLDER`], [`AssetServer`], [`StaticAssets`],
//!   [`rewrite_assets`].
//!
//! What an app may do is in the contract; how a host sandboxes it is not.
//! Each host turns an [`AppPolicy`] into its own sandbox settings under one
//! rule: **a host may restrict more than the policy says, never less.**
//!
//! The order is always: [`parse`] → [`admit`] (or [`admit_digest`]) →
//! [`policy::resolve`]. Skipping a step is the bug this crate exists to make
//! hard.
//!
//! ```
//! use octosense_app_contract::{admit, bundle_digest, parse, policy, HostLimits, RefuseAllSignatures};
//! let bundle = b"the card bundle bytes";
//! let manifest = format!(
//!     r#"{{"schema":1,"id":"forecast","version":"1.0.0","name":"Forecast",
//!         "integrity":{{"bundle_blake3":"{}"}},
//!         "capabilities":["storage","net"],
//!         "network":{{"hosts":["api.weather.example"]}}}}"#,
//!     bundle_digest(bundle)
//! );
//! let manifest = parse(&manifest).unwrap();
//! admit(&manifest, bundle, &RefuseAllSignatures).unwrap();
//! let limits = HostLimits::default().with_require_signature(false);
//! let policy = policy::resolve(&manifest, &limits).unwrap();
//! assert!(policy.allows_host("api.weather.example"));
//! assert!(!policy.allows_host("example.com"));
//! ```
//!
//! # Stability
//!
//! Within `1.x` the contract only grows (ADR 0005 §2):
//!
//! - **Additive only.** New types, functions, optional manifest fields and
//!   enum variants; nothing is removed or renamed, and no existing field,
//!   default or rule changes meaning. Anything else is `2.0`, decided in an
//!   ADR. CI runs `cargo semver-checks` against the latest published `1.x`
//!   on every change.
//! - **Every public struct and enum is `#[non_exhaustive]`** (the one
//!   exception is the unit marker [`RefuseAllSignatures`]), so adding a
//!   field or a variant is a minor change. Build what you need with
//!   [`HostLimits::default`] / [`HostLimits::system`] and the `with_*`
//!   methods, [`Signature::new`], [`parse`] for manifests and
//!   [`policy::resolve`] for policies; match enums with a `_` arm.
//! - **Unknown manifest fields are classified, not ignored.** Every field
//!   added in `1.x` is either *optional* (a host that does not know it may
//!   run the app without it: it only adds information or asks for less) or
//!   *required* (it restricts or changes what the app gets). A manifest
//!   that uses a required field names its feature in `requires`. Parsing
//!   ([`parse`]) applies, in this order:
//!   1. `schema` must be [`SCHEMA`] (`1` for the whole `1.x` line).
//!   2. Every `requires` entry must be in [`KNOWN_FEATURES`], at every
//!      `schema_minor`; otherwise the app is refused with
//!      `app <id> needs a newer host: <feature>`. `1.0.0` knows none.
//!   3. A manifest whose `schema_minor` (default 0) is at most
//!      [`SCHEMA_MINOR`] (`0` in `1.0.0`, `1` since `1.12.0`, which added
//!      `window`) is read strictly: an unknown field at any level refuses
//!      it, exactly as before `requires` existed.
//!   4. A manifest whose `schema_minor` is above [`SCHEMA_MINOR`] was
//!      written for a newer `1.x`. Its unknown fields, at any level
//!      (top level, `network`, `storage`, `agent`, `agent.model`, …), are
//!      optional by rule 2, so they are ignored and reported by
//!      [`AppManifest::ignored_fields`] for the host to log. Fields this
//!      build knows are still checked as always, and a `research` object in
//!      the old toolbox shape is still refused.
//!
//!   So an older host never runs an app under weaker rules than its author
//!   wrote, and a newer optional field never breaks an older host, as long
//!   as the manifest declares the minor that added the field: `window`, the
//!   first field added at a new minor, is refused below `schema_minor` 1. The
//!   ignored fields stay part of the manifest's signing bytes, so a newer
//!   manifest's signature still verifies; a field added in `1.x` must
//!   therefore be skipped when serialising its default and must serialise
//!   exactly as it is written.
//! - **Both new fields are invisible to older manifests.** `requires` and
//!   `schema_minor` are left out of the canonical signing bytes when empty,
//!   so every manifest signed before them signs exactly as it did.
//! - **Behaviour is pinned by fixtures.** `tests/fixtures/` holds real
//!   manifests and packages with their digest, signing bytes, ignored
//!   fields and resolved [`AppPolicy`]; every `1.x` release must reproduce
//!   all of them, and the corpus is append-only within `1.x`.
pub mod assets;
pub mod backend;
pub mod host_api;
pub mod bundle;
pub mod entry;
mod lenient;
pub mod manifest;
pub mod policy;
pub mod research;
pub mod verify;

pub use assets::{rewrite_assets, AssetServer, StaticAssets};
pub use bundle::{digest_dir, portable_path, MANIFEST_FILE};
pub use entry::{script_source, ASSETS_PLACEHOLDER, SCRIPT_ENTRY};
pub use manifest::{
    check_component_id, check_reserved_id, is_exact_version, parse, short_id, AgentSpec, AgentWorkspace, AppManifest, ComponentDependency,
    Compute, GithubPublisher, Integrity, ModelNeed, ModelSpec, ModelTier, Network, ProfileMode, Signature, Storage, TaskModel, Triggers,
    WindowHint, KNOWN_CAPABILITIES, KNOWN_FEATURES, KNOWN_MODEL_NEEDS, MAX_COMPONENTS, RESERVED_HOST_FILE_IDS, RESERVED_NAMES, SCHEMA, SCHEMA_MINOR,
};
pub use policy::{resolve, AppPolicy, HostLimits, StorageGrant};
pub use research::ResearchScope;
pub use verify::{admit, admit_digest, bundle_digest, verify_manifest, RefuseAllSignatures, SignatureVerifier};

/// Exact Palpo service grants and their consent language.
pub mod palpo;
