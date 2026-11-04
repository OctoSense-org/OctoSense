//! What an installed app declares about itself.
//!
//! The manifest is the ONLY thing an app may say about its own limits, and
//! saying it is not the same as getting it: every field is a request that
//! [`crate::policy`] resolves against the host's ceilings. Unknown fields are
//! refused rather than ignored, so a manifest written for a newer host does
//! not silently run with less containment than it asked for.
//!
//! A manifest that needs something this contract added after `1.0` names it
//! in [`AppManifest::requires`]; a host that does not know a listed feature
//! refuses the app ("needs a newer host"). See the crate's Stability section.
use crate::research::ResearchScope;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The manifest schema this build understands. A bundle declaring anything
/// else is refused: an older host must not guess at a newer grammar.
pub const SCHEMA: u32 = 1;

/// The newest `schema_minor` this build knows: which additions to schema 1
/// it understands. `0` is the grammar of `1.0.0`; `1` adds
/// [`AppManifest::window`] (contract `1.12.0`). A manifest may carry a
/// higher `schema_minor` (it records what the manifest uses, and asks for
/// nothing by itself); what a host must understand is listed in
/// [`AppManifest::requires`].
pub const SCHEMA_MINOR: u32 = 1;

/// The first `schema_minor` that knows [`AppManifest::window`].
const WINDOW_SCHEMA_MINOR: u32 = 1;

/// The widths and heights, in points, a [`WindowHint`] may ask for.
const WINDOW_WIDTHS: std::ops::RangeInclusive<u32> = 320..=4096;
const WINDOW_HEIGHTS: std::ops::RangeInclusive<u32> = 240..=4096;

/// The features a manifest may list in [`AppManifest::requires`] and this
/// build honours. Empty in `1.0.0`: every feature added in `1.x` that
/// restricts or changes what an app gets is added here, with the field
/// that carries it, in the same release.
///
/// `wasm-components-v1`: the bundle's `fns/` holds a WebAssembly component
/// (OctoSense ADR 0014), not only core modules. A host whose `wasm` service
/// loads only modules does not know it, so it refuses the app instead of
/// failing when the app first calls its functions. The `wasm` capability describes usage; it is not an admission grant.
///
/// `wasm-shared-components-v1`: the manifest's [`AppManifest::components`]
/// names shared components from App Hub's catalog (App Hub ADR 0003), which
/// the app's functions use. A host that does not know it refuses the app
/// instead of running it without them. The `wasm` capability describes usage; it is not an admission grant.
pub const KNOWN_FEATURES: &[&str] = &[
    "palpo-admin-v1",
    "host-api-v1",
    "backend-api-v1",
    "script-tools-v1",
    "publisher-github-v1",
    "wasm-components-v1",
    "wasm-shared-components-v1",
];

/// The most shared components one manifest may name
/// ([`AppManifest::components`]).
pub const MAX_COMPONENTS: usize = 8;

/// Parse a manifest: [`AppManifest::parse`].
pub fn parse(json: &str) -> Result<AppManifest, String> {
    AppManifest::parse(json)
}

/// Every capability an app may request. The list is closed on purpose — a
/// capability that is not here cannot be granted, so adding one is a change
/// to this file and to the service that enforces it, together.
pub const KNOWN_CAPABILITIES: &[&str] = &[
    // Read implemented API descriptions; this does not grant their capabilities.
    "runtime",
    // Read and write inside the app's own storage jail.
    "storage",
    // Import or export a file the person chooses in the host's native dialog.
    // This grants no arbitrary path access and does not imply storage.
    "files",
    // Make requests, but only to the hosts in `network.hosts`.
    "net",
    // Raise a prompt the person answers (a permission ask, a confirmation).
    "prompt",
    // Read the shared ledger. Writing is always the app's own rows.
    "ledger.read",
    // Location, camera and clipboard reach the person's world; each is a
    // separate consent, never implied by another.
    "location",
    "camera",
    "clipboard",
    // Show pictures from any public https host, not just `network.hosts`:
    // a feed reader's thumbnails come from wherever its stories link.
    "images",
    // Open any public https page in the system WebView, which gets no way
    // back into the app: a reader for the stories it lists.
    "web",
    // Record bounded sound in the foreground, including camera videos.
    "microphone",
    // Play local app audio in the foreground; no recording or storage grant.
    "audio",
    // Offer what it captures to the system photo library, where other apps
    // can see it; without this, captures stay in the app's own storage.
    "library",
    // Read and send mail through the host's mail service, from accounts the
    // person signs in to on the host's own sheet. The app never holds the
    // password or the connection.
    "mail",
    // Host-owned OAuth connection lifecycle. Returns app-bound handles, never tokens.
    "auth",
    // GitHub repository reads and host-reviewed Markdown commits.
    "github",
    // Google Calendar connector; distinct from the local system calendar.
    "gcalendar",
    // Google Gmail API connector; distinct from the IMAP/SMTP mail service.
    "gmail",
    // Calendar's local event store and UI. The service additionally checks
    // owning app identity; this is not Google/Android calendar access.
    "calendar",
    // Public native calendar access, scoped to calendars the person selects.
    // Distinct from os.calendar's store and the Google Calendar connector.
    // The host must implement the declared methods and enforce native consent.
    "device_calendar",
    // See and arrange the assistant's LLM providers through the host's llm
    // service. Keys are typed, shown as a QR and scanned only on the host's
    // own sheets; the app sees masked status, never a key.
    "llm",
    // Read the host's news service: feeds and topic feeds it collects on a
    // schedule into the app's store, and the items' text. The app never
    // fetches arbitrary sites itself through it.
    "news",
    // App-owned media libraries and recommendations; no OS gallery/account grant.
    "photos",
    "youtube",
    // Publish cards to the glance screen through the host's glance service:
    // L0 cards the shell checks, caps, rate-limits and expires, keyed to the
    // app itself. The app sees only its own cards and a card opens only it.
    "glance",
    // Make bounded one-shot model calls through the host's model service
    // (`model.complete`): the app names a model class ("fast" or "strong")
    // and a JSON Schema; the host picks the model from the person's own
    // providers, validates the reply against the schema and keeps a per-app
    // daily budget. No tools, memory or history; the app never sees the
    // provider, model id or key. The app's inputs go to the AI provider the
    // person configured. Not `llm`, which manages providers for os.* apps.
    "model",
    // Search through the system toolbox (`search`, `deep_research`), within
    // the manifest's `research` scope: languages, regions, domains, recency,
    // categories and results per search. The host runs the search and
    // checks every call against the scope; the app never fetches the sites
    // itself. The scope's schema is octos's `Scope` (see [`crate::research`]).
    "research",
    // Crawl a site through the system toolbox (`deep_crawl`): follow links
    // up to the scope's `max_depth` and read up to its `max_pages` a crawl,
    // inside its domain lists. More reach than `research`, which only reads
    // search results; neither implies the other.
    "crawl",
    // Run the app's own WebAssembly functions (its bundle's `fns/*.wasm`)
    // through the host's `wasm` service, in a sandbox with a deadline and a
    // memory cap. A core module reaches no file, network, clock or other
    // app: it gets its input and returns its output. A component
    // (`wasm-components-v1`, OctoSense ADR 0014) may also read the clock and
    // random numbers and, with `storage`, the app's own files; it reaches no
    // network or other app. Shared components from App Hub's catalog
    // (`components`, `wasm-shared-components-v1`) run as the app's own
    // functions, with the app's grants and nothing more.
    "wasm",
    // The craft engines (OctoSense ADR 0013), each behind its host service:
    // spreadsheets, images, documents, slide decks, drawings, raw photos,
    // audio, page layouts, video, motion graphics, vector art and PDFs,
    // processed inside the calling app's own files. The host serves them
    // to system apps only, so declaring one does not let a store app
    // reach it.
    "sheet",
    "photo",
    "word",
    "deck",
    "cad",
    "light",
    "sound",
    "design",
    "film",
    "effect",
    "vector",
    "pdf",
    // Host services reached by exact name (App Hub's `services`). Each is
    // a separate consent: a host adapter checks the exact name, the person's
    // per-instance grant and its own ceilings on every request. A prefix is
    // never a grant: `octos.` or `matrix.` alone is an unknown capability,
    // and history access does not imply starting a turn.
    // Palpo operations are individual grants; they never imply server roles.
    "palpo.intent.new",
    "palpo.session.open",
    "palpo.session.disconnect",
    "palpo.catalog.list",
    "palpo.projects.list",
    "palpo.projects.create",
    "palpo.requests.list",
    "palpo.requests.create",
    "palpo.fleets.list",
    "palpo.fleets.register",
    "palpo.fleets.install",
    "palpo.fleets.set_state",
    "palpo.fleets.migrate",
    "palpo.fleets.queue",
    "palpo.fleets.export",
    "palpo.fleets.connect",
    "palpo.agents.list",
    "palpo.agents.register",
    "palpo.agents.rename",
    "palpo.agents.retire",
    "palpo.activity.list",
    "palpo.accounts.list",
    "palpo.inbox.list",
    "palpo.inbox.submit",
    "palpo.inbox.get",
    "palpo.inbox.decide",
    "palpo.inbox.activate",
    "palpo.inbox.seen",
    "palpo.inbox.snooze",
    "matrix.account_info",
    "matrix.device",
    "matrix.dm_find",
    "matrix.dm_open",
    "matrix.event",
    "matrix.favorite",
    "matrix.ignored_users",
    "matrix.invite",
    "matrix.invite_respond",
    "matrix.invites",
    "matrix.join",
    "matrix.low_priority",
    "matrix.mark_unread",
    "matrix.older_messages",
    "matrix.permalink",
    "matrix.pin",
    "matrix.pinned_events",
    "matrix.power_levels",
    "matrix.profile",
    "matrix.react",
    "matrix.read_messages",
    "matrix.read_receipt",
    "matrix.read_receipts",
    "matrix.reply",
    "matrix.room_info",
    "matrix.room_members",
    "matrix.room_preview",
    "matrix.room_threads",
    "matrix.rooms_info",
    "matrix.rooms_list",
    "matrix.rooms_messages",
    "matrix.rooms_search",
    "matrix.rooms_send",
    "matrix.search_room",
    "matrix.search_rooms",
    "matrix.send_message",
    "matrix.space_info",
    "matrix.space_rooms",
    "matrix.spaces",
    "matrix.successor",
    "matrix.thread_replies",
    "matrix.thread_reply",
    "matrix.typing",
    "matrix.unread",
    "matrix.user_profile",
    "octos.session.open",
    "octos.session.history",
    "octos.turn.start",
    "octos.turn.interrupt",
];

/// The permission profiles an app's agent session may ask for. Full access is
/// absent by construction: it is an operator setting for machines they own,
/// and no manifest may name it (ADR 0002, "the rule at the boundary").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ProfileMode {
    /// No writes at all; every write asks.
    ReadOnly,
    /// Read and write inside the workspace; anything else asks.
    WorkspaceWrite,
    /// Read and write inside the workspace; anything else is refused
    /// outright rather than asked. The right default for an unattended app.
    WorkspaceWriteNeverAsk,
}

impl ProfileMode {
    /// The string the kernel's permission profile uses.
    pub fn as_kernel_mode(self) -> &'static str {
        match self {
            ProfileMode::ReadOnly => "read-only",
            ProfileMode::WorkspaceWrite => "workspace-write",
            ProfileMode::WorkspaceWriteNeverAsk => "workspace-write-never",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct AppManifest {
    /// Must equal [`SCHEMA`].
    pub schema: u32,
    /// Stable identity. Also the name of the app's storage jail, so it is
    /// constrained to the characters a path component may hold.
    pub id: String,
    /// Opaque to the host, but pinned: a different version is a different
    /// bundle and must be admitted again.
    pub version: String,
    /// What a person calls it.
    pub name: String,
    pub integrity: Integrity,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub network: Network,
    #[serde(default)]
    pub storage: Storage,
    #[serde(default)]
    pub compute: Compute,
    /// Absent means the app gets no agent at all, which is the default.
    #[serde(default)]
    pub agent: Option<AgentSpec>,
    /// The scope of the `research` and `crawl` capabilities: octos's
    /// `Scope`, exactly (see [`crate::research`]). Required with either
    /// capability and refused without both. Skipped when absent, so a
    /// manifest written before it existed signs exactly as it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub research: Option<ResearchScope>,
    /// Required and optional API ABI versions, checked again by the running host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_api: Option<crate::host_api::HostApiRequirements>,
    /// Public developer backend registration. Tokens remain host-owned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<crate::backend::BackendRegistration>,
    /// Shared WebAssembly components from App Hub's catalog that the app's
    /// functions use (App Hub ADR 0003), each pinned to one exact version
    /// and digest, so the app's code never changes without an app update.
    /// Requires the `wasm-shared-components-v1` host ABI.
    /// Skipped when empty, so a manifest written before it existed signs
    /// exactly as it did.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<ComponentDependency>,
    /// The size the app prefers its desktop window to open at, in points
    /// ([`WindowHint`]). A hint, not a grant: hosts clamp it to the screen,
    /// phones and other full-screen hosts ignore it, and the person can
    /// still resize the window. An optional field in the growth rule's
    /// sense, so it needs `schema_minor` 1 or later: a host that predates
    /// it then reads the manifest leniently and ignores the hint instead of
    /// refusing an unknown field. Skipped when absent, so a manifest written
    /// before it existed signs exactly as it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowHint>,
    /// Features added in `1.x` that this manifest needs a host to honour,
    /// because they restrict or change what the app gets. A host that does
    /// not know one ([`KNOWN_FEATURES`]) refuses the app rather than run it
    /// under weaker rules than its author wrote. Skipped when empty, so a
    /// manifest written before it existed signs exactly as it did.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    /// Which additions to schema 1 the manifest uses ([`SCHEMA_MINOR`]).
    /// `0`, the default, is the `1.0.0` grammar. Skipped when 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub schema_minor: u32,
    /// Fields of a newer-minor manifest this build did not know and
    /// ignored, with their values ([`AppManifest::ignored_fields`]). Never
    /// part of a manifest this build parses strictly.
    #[serde(skip)]
    ignored: Vec<crate::lenient::Ignored>,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// What the bundle must hash to. The digest covers the bundle bytes as they
/// were signed; a signature, when we have one, signs this manifest.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Integrity {
    /// Lowercase hex blake3 of the bundle.
    pub bundle_blake3: String,
    /// Detached signature over the canonical manifest bytes, if the host
    /// requires signing. Verified by a [`crate::verify::SignatureVerifier`].
    #[serde(default)]
    pub signature: Option<Signature>,
    /// GitHub Actions provenance over the canonical manifest. Requires a
    /// host that understands `publisher-github-v1`; never an unsigned fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GithubPublisher>,
}

/// Immutable GitHub workflow identity for one release. The attestation is
/// excluded from signing bytes; these identity fields are included.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct GithubPublisher {
    /// Exact owner/repository spelling in the authenticated GitHub claims.
    pub repository: String,
    pub repository_id: String,
    pub owner_id: String,
    /// Repository-local workflow, such as `.github/workflows/publish-app.yml`.
    pub workflow: String,
    /// Exactly `v` followed by the manifest version.
    pub tag: String,
    /// Lowercase immutable Git commit SHA (40 hex characters).
    pub commit: String,
    /// Sigstore v0.3 bundle from actions/attest, attached after preparation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation: Option<serde_json::Value>,
}

impl GithubPublisher {
    pub fn validate(&self, version: &str) -> Result<(), String> {
        let parts: Vec<_> = self.repository.split('/').collect();
        if parts.len() != 2 || parts.iter().any(|s| s.is_empty() || s.len() > 100
            || *s == "." || *s == ".." || s.ends_with(".git")
            || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))) {
            return Err("GitHub publisher repository must be owner/repo".into());
        }
        for id in [&self.repository_id, &self.owner_id] {
            if id.is_empty() || id.len() > 20 || id.starts_with('0') || !id.bytes().all(|b| b.is_ascii_digit()) {
                return Err("GitHub publisher requires nonzero numeric repository and owner IDs".into());
            }
        }
        let name = self.workflow.strip_prefix(".github/workflows/").ok_or("publisher workflow must be repository-local")?;
        if name.is_empty() || name.len() > 100 || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            || !(name.ends_with(".yml") || name.ends_with(".yaml")) {
            return Err("publisher workflow must name one YAML file in .github/workflows".into());
        }
        if self.tag != format!("v{version}") || self.tag.len() > 128
            || !self.tag.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.+".contains(&b)) {
            return Err("publisher tag must be v followed by the exact manifest version".into());
        }
        if self.commit.len() != 40 || !self.commit.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err("publisher commit must be a lowercase 40-character Git SHA".into());
        }
        if let Some(proof) = &self.attestation {
            if !proof.is_object() || serde_json::to_vec(proof).map_err(|e| e.to_string())?.len() > 48 * 1024 {
                return Err("publisher attestation must be an object of at most 48 KiB".into());
            }
        }
        Ok(())
    }

    pub fn repository_url(&self) -> String { format!("https://github.com/{}", self.repository) }
    pub fn workflow_uri(&self) -> String { format!("{}/{}@refs/tags/{}", self.repository_url(), self.workflow, self.tag) }
}


#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Signature {
    /// Which key signed it, as the host knows the key.
    pub key_id: String,
    /// Lowercase hex signature bytes.
    pub value: String,
}

impl Signature {
    /// A signature by `key_id` with hex `value`, as a signing tool attaches
    /// it to `integrity.signature`.
    pub fn new(key_id: impl Into<String>, value: impl Into<String>) -> Self {
        Signature { key_id: key_id.into(), value: value.into() }
    }
}

/// One shared component an app uses (App Hub ADR 0003): the catalog's
/// component `id` at exactly `version`, whose WebAssembly file hashes to
/// `blake3`. The app's script calls it by `as`. A host resolves it from its
/// verified catalog and refuses any other bytes, so the app's code never
/// changes without an app update.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ComponentDependency {
    /// The app's own name for it: `[a-z][a-z0-9_]{0,31}`, unique in the
    /// manifest.
    #[serde(rename = "as")]
    pub alias: String,
    /// The component's catalog id. Component ids follow the app id rules
    /// and never start with `os.` ([`check_component_id`]).
    pub id: String,
    /// One exact semantic version, `major.minor.patch` (with an optional
    /// pre-release and build), never a range.
    pub version: String,
    /// Lowercase hex BLAKE3 of the component's `.wasm` file, 64 characters.
    pub blake3: String,
}

impl ComponentDependency {
    /// A dependency on `id` at `version` with digest `blake3`, called `alias`.
    pub fn new(alias: impl Into<String>, id: impl Into<String>, version: impl Into<String>, blake3: impl Into<String>) -> Self {
        ComponentDependency { alias: alias.into(), id: id.into(), version: version.into(), blake3: blake3.into() }
    }

    /// How a person or a log names it: `markdown (org.example.markdown 1.2.0)`.
    pub fn describe(&self) -> String {
        format!("{} ({} {})", self.alias, self.id, self.version)
    }

    fn check(&self) -> Result<(), String> {
        let alias = self.alias.as_bytes();
        if alias.is_empty()
            || alias.len() > 32
            || !alias[0].is_ascii_lowercase()
            || !alias.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
        {
            return Err(format!("component name {:?} must match [a-z][a-z0-9_]{{0,31}}", self.alias));
        }
        check_component_id(&self.id).map_err(|e| format!("component {}: {e}", self.alias))?;
        if !is_exact_version(&self.version) {
            return Err(format!(
                "component {} names version {:?}; a component is pinned to one exact semantic version (major.minor.patch), never a range",
                self.alias, self.version
            ));
        }
        if self.blake3.len() != 64 || !self.blake3.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err(format!("component {}'s blake3 must be 64 lowercase hex characters", self.alias));
        }
        Ok(())
    }
}

/// The rules a shared component's id follows (App Hub ADR 0003): an app
/// id's (1 to 64 of `[a-z0-9.-]`, no navigation, no reserved name or
/// namespace), and never `os.`, which only the system apps a build ships
/// use.
pub fn check_component_id(id: &str) -> Result<(), String> {
    crate::policy::check_id_shape(id, "component id")?;
    if id.starts_with("os.") {
        return Err(format!("component id {id:?} is under os., which only system apps use"));
    }
    if check_reserved_id(id).is_err() {
        return Err(format!(
            "component id {id:?} is reserved: it, or its last segment {:?}, names a native app, the host or a host file",
            short_id(id)
        ));
    }
    Ok(())
}

/// Whether `version` is one exact semantic version (SemVer 2.0):
/// `major.minor.patch`, numbers without leading zeros, then an optional
/// `-pre.release` and `+build`. A range (`^1.2`, `~1`, `>=1.0.0`, `1.x`) is
/// not one.
pub fn is_exact_version(version: &str) -> bool {
    fn numeric(s: &str) -> bool {
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && (s == "0" || !s.starts_with('0'))
    }
    fn identifiers(s: &str, pre: bool) -> bool {
        s.split('.').all(|id| {
            !id.is_empty()
                && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && (!pre || !id.bytes().all(|b| b.is_ascii_digit()) || numeric(id))
        })
    }
    if version.is_empty() || version.len() > 64 {
        return false;
    }
    let (rest, build) = match version.split_once('+') {
        Some((rest, build)) => (rest, Some(build)),
        None => (version, None),
    };
    let (core, pre) = match rest.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (rest, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    let pre_ok = match pre {
        Some(pre) => identifiers(pre, true),
        None => true,
    };
    let build_ok = match build {
        Some(build) => identifiers(build, false),
        None => true,
    };
    parts.len() == 3 && parts.iter().all(|p| numeric(p)) && pre_ok && build_ok
}

/// The size an app prefers its desktop window to open at, in points
/// ([`AppManifest::window`], `schema_minor` 1): `{"width": 1536, "height":
/// 1024}`.
///
/// A hint, not a grant. It asks for no capability, storage or budget, and
/// a host owes the app nothing for it: hosts clamp it to the screen, phones
/// and other full-screen hosts ignore it, and the person can still resize
/// the window. [`parse`] refuses a width outside 320 to 4096 or a height
/// outside 240 to 4096.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct WindowHint {
    /// Points, 320 to 4096.
    pub width: u32,
    /// Points, 240 to 4096.
    pub height: u32,
}

impl WindowHint {
    /// A hint of `width` by `height` points. [`parse`] checks the range; this
    /// does not.
    pub fn new(width: u32, height: u32) -> Self {
        WindowHint { width, height }
    }
}

/// What a derived, `deny_unknown_fields` `Deserialize` does, except that only
/// an object is a window: serde would also read `[1536, 1024]` as one, which
/// then serialises, and signs, as an object rather than as written.
impl<'de> Deserialize<'de> for WindowHint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::{Error, MapAccess, Visitor};
        const FIELDS: &[&str] = &["width", "height"];
        struct Object;
        impl<'de> Visitor<'de> for Object {
            type Value = WindowHint;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(r#"an object {"width": <points>, "height": <points>}"#)
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<WindowHint, A::Error> {
                let (mut width, mut height) = (None, None);
                while let Some(key) = map.next_key::<String>()? {
                    let (slot, name) = match key.as_str() {
                        "width" => (&mut width, "width"),
                        "height" => (&mut height, "height"),
                        other => return Err(A::Error::unknown_field(other, FIELDS)),
                    };
                    if slot.is_some() {
                        return Err(A::Error::duplicate_field(name));
                    }
                    *slot = Some(map.next_value::<u32>()?);
                }
                Ok(WindowHint {
                    width: width.ok_or_else(|| A::Error::missing_field("width"))?,
                    height: height.ok_or_else(|| A::Error::missing_field("height"))?,
                })
            }
        }
        // Named as a struct, so a manifest for a newer 1.x has a later
        // field inside `window` ignored and listed like any other.
        deserializer.deserialize_struct("WindowHint", FIELDS, Object)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Network {
    /// The hosts this app may reach, exactly. No wildcards, no schemes, no
    /// paths: a host and nothing else, and the request is HTTPS by the time
    /// the service makes it.
    #[serde(default)]
    pub hosts: Vec<String>,
}

/// The `storage` block (OctoSense ADR 0004 §11). The host lays out the
/// app's folders from it (OctoSense's `app_storage::spec`, which reads the
/// same fields); App Hub admits it and clamps `max_bytes`. `external`
/// (a path outside the jail) is for reviewed native apps only, so a script
/// manifest that names it is refused like any unknown field. The fields
/// added after `max_bytes` are skipped when unset, so a manifest signed
/// before they existed signs exactly as it did.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Storage {
    /// Whole-jail ceiling the app asks for. Clamped to the host's maximum.
    #[serde(default)]
    pub max_bytes: Option<u64>,
    /// Data per account, one agent per account; `false` (the default) is one
    /// `device` folder.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub accounts: bool,
    /// What the app's agent may read from disk: its account's folder (the
    /// default) or nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_workspace: Option<AgentWorkspace>,
    /// Ceiling for the jail's `cache/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_max_bytes: Option<u64>,
}

/// `storage.agent_workspace`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum AgentWorkspace {
    /// The account's folder.
    #[default]
    Account,
    /// No files: tools only.
    None,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Compute {
    /// Script instructions the app may run per session, cumulative — not the
    /// per-evaluation cap, which only stops one runaway expression.
    #[serde(default)]
    pub instruction_budget: Option<u64>,
    /// Ceiling for the isolate's heap.
    #[serde(default)]
    pub memory_bytes: Option<u64>,
}

/// The agent session an app asks for. Everything here is bounded by the same
/// manifest: its workspace is the app's jail, its network is the app's hosts.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct AgentSpec {
    pub profile: ProfileMode,
    /// The tools the session may call. Closed list, resolved against the
    /// host's own allowlist; shell and arbitrary file tools are never in it.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Model turns per request before the session stops and reports.
    #[serde(default)]
    pub max_iterations: Option<u32>,
    /// Tokens the session may spend per request.
    #[serde(default)]
    pub token_budget: Option<u64>,
    /// What the agent needs from a model, never which model (ADR 0002 §3).
    /// The host picks one from the person's providers that meets it.
    ///
    /// Every field added after the first release of schema 1 is skipped when
    /// it holds its default, so a manifest written before it existed
    /// serialises, and therefore signs, exactly as it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelSpec>,
    /// The agent may run while the app is closed, woken by `triggers`. A
    /// request: the person grants or refuses it per app, and it requires at
    /// least one trigger.
    #[serde(default, skip_serializing_if = "is_false")]
    pub background: bool,
    /// What wakes the agent besides the person (ADR 0002 §2).
    #[serde(default, skip_serializing_if = "Triggers::is_empty")]
    pub triggers: Triggers,
    /// The bundle-relative path of the agent's instructions, conventionally
    /// `AGENT.md`. Named here so the file is declared, not
    /// merely present; the digest pins its bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// The skills the bundle ships under `skills/<name>/`. Installed into
    /// this app's peer workspace only, and data-only for a contained app.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// What a task needs from a model. Closed: a need that is not here cannot be
/// matched by any host, so adding one is a change here and in the host's
/// model selection, together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ModelNeed {
    /// Structured tool calls. An agent with any tools needs this.
    ToolCalling,
    /// Image input, for example to critique a rendered card.
    Vision,
    /// A long context window (the host decides what "long" means today).
    LongContext,
    /// A reasoning (thinking) model.
    Reasoning,
    /// Reliable JSON output against a schema.
    StructuredOutput,
    /// Reads and writes more than one language well.
    Multilingual,
}

/// Every [`ModelNeed`], as the manifest spells it.
pub const KNOWN_MODEL_NEEDS: &[&str] =
    &["tool_calling", "vision", "long_context", "reasoning", "structured_output", "multilingual"];

/// How capable (and costly) a model the task deserves. The host maps a tier
/// to the person's providers; policy may lower it (budget, battery).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ModelTier {
    Fast,
    #[default]
    Standard,
    Strong,
}

/// The model an app's agent needs. Never a provider or a model name: the
/// app cannot know which ones the person configured, and never sees keys.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ModelSpec {
    #[serde(default)]
    pub needs: Vec<ModelNeed>,
    #[serde(default)]
    pub tier: ModelTier,
    /// Only a model that runs on the device (or the person's own machine)
    /// may see this app's data. App-wide: a task cannot relax it.
    #[serde(default)]
    pub local_only: bool,
    /// Different requirements for named tasks, for example a fast model for
    /// `triage` and a strong one for `synthesis`. Task names are the app's
    /// own (`[a-z_]{1,32}`); `AGENT.md` says which task a step is.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub per_task: BTreeMap<String, TaskModel>,
}

/// One task's model requirements. `local_only` is deliberately absent: it
/// holds for the whole app.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct TaskModel {
    #[serde(default)]
    pub needs: Vec<ModelNeed>,
    #[serde(default)]
    pub tier: ModelTier,
}

/// What wakes an app's agent without the person asking.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Triggers {
    /// Five-field cron expressions (minute hour day-of-month month
    /// day-of-week), in the device's local time: `"0 7 * * *"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schedule: Vec<String>,
    /// Events from the app's own host service, in the app's namespace:
    /// `"news.items.new"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
}

impl Triggers {
    pub fn is_empty(&self) -> bool {
        self.schedule.is_empty() && self.events.is_empty()
    }
}

/// The namespace an app's tools and events live in: the last segment of its
/// id (`os.news` and `dev.example.news` are both `news`). Peers are per app,
/// so two apps with the same short id never share a tool registry.
pub fn short_id(app_id: &str) -> &str {
    app_id.rsplit('.').next().unwrap_or(app_id)
}

/// Names no script app may take, as its id or as its namespace
/// ([`short_id`]). A host keys an app's jail, storage folders, tool
/// declarations, executor and consent by its id, and its tools by its
/// namespace, so a store app named `terminal` (or `com.example.terminal`,
/// whose tools would be `terminal.*`) would stand in for the Terminal.
///
/// - The native apps OctoSense ships (its `native-apps.json`; OctoSense
///   checks this list against that file): `apphub`, `appcard`, `reference`,
///   `rinx`, `sheets`, `terminal`, and the Makepad apps it has made native
///   apps: `calculator`, `clock`, `notes`, `reminders`, `weather`, and the
///   desktop-only `browser` and `task`; and `updater`, its host-owned
///   Updates window, which has no tools. Their tools (`notes.search`,
///   `calculator.eval`, `browser.tabs`, …) are named by OctoSense's grants,
///   so no store app may declare them. A Makepad app is reserved here when
///   OctoSense makes it native, not before.
/// - What the shell itself acts as, or owns tools under: `system` (the
///   system agent as a caller), `toolbox` and `workflow` (the system
///   toolbox), `dev` (developer mode's `dev.run`), `agents`
///   (`agents.list`), `octos`, `shell`, and `card` and `os`, the peer and
///   system-app prefixes.
///
/// System apps' own namespaces (`os.news` is `news`) are not in it: a store
/// app may share one, and a host resolves another app's tool by its owner,
/// not by its name.
pub const RESERVED_NAMES: &[&str] = &[
    "agents", "apphub", "appcard", "browser", "calculator", "card", "clock", "dev", "notes", "octos", "octoscode", "os",
    "reference", "reminders", "rinx", "sheets", "shell", "system", "task", "terminal", "toolbox", "updater", "weather",
    "workflow",
];

/// Exact app ids which would collide with host-owned files beneath an install
/// root. These are not namespace suffixes: `org.example.json` remains valid.
/// Compare without ASCII case so the rule also holds on case-insensitive hosts.
pub const RESERVED_HOST_FILE_IDS: &[&str] = &[
    "catalog.json", "catalog.lock", "catalog-v2.json", "catalog-v2.lock",
];

/// Refuse an app id that is, or whose namespace is, a [`RESERVED_NAMES`]
/// entry, or exactly matches a [`RESERVED_HOST_FILE_IDS`] entry ignoring case.
/// Every script app passes through it: [`crate::policy::resolve`],
/// App Hub's agent review (`AgentBundle::load`) and the store's gate.
pub fn check_reserved_id(app_id: &str) -> Result<(), String> {
    if RESERVED_HOST_FILE_IDS.iter().any(|name| app_id.eq_ignore_ascii_case(name)) {
        return Err(format!("app id {app_id:?} is reserved: it names a host catalog or cache lock file"));
    }
    let namespace = short_id(app_id);
    if RESERVED_NAMES.contains(&app_id) {
        return Err(format!("app id {app_id:?} is reserved: it names a native app or the host itself"));
    }
    if RESERVED_NAMES.contains(&namespace) {
        return Err(format!(
            "app id {app_id:?} ends in {namespace:?}, which is reserved: its tools would be {namespace}.*, a native app's or the host's"
        ));
    }
    Ok(())
}

impl AppManifest {
    /// Parse a manifest.
    ///
    /// - `schema` must be [`SCHEMA`], and every `requires` entry must be in
    ///   [`KNOWN_FEATURES`], at every `schema_minor`.
    /// - A manifest whose `schema_minor` is at most [`SCHEMA_MINOR`] is read
    ///   strictly: an unknown field, at any level, refuses it.
    /// - A manifest written for a newer `1.x` (`schema_minor` above
    ///   [`SCHEMA_MINOR`]) is read with its unknown fields ignored, at any
    ///   level: by the growth rule they are optional, because a field that
    ///   restricts or changes what the app gets must be named in `requires`.
    ///   The ignored fields are reported by [`AppManifest::ignored_fields`].
    /// - Known fields are checked as always, at every `schema_minor`
    ///   ([`AppManifest::check_requires`]). A manifest that sets `window`,
    ///   which `schema_minor` 1 added, is refused at `schema_minor` 0: a
    ///   host that predates `window` must read it leniently and ignore the
    ///   hint, never refuse it as an unknown field.
    pub fn parse(json: &str) -> Result<Self, String> {
        let manifest = match serde_json::from_str::<AppManifest>(json) {
            Ok(manifest) => manifest,
            Err(strict) => Self::parse_newer(json).unwrap_or_else(|| {
                // A research scope in the toolbox's old shape gets the fields
                // to rename, not only serde's "unknown field".
                Err(serde_json::from_str::<serde_json::Value>(json)
                    .ok()
                    .and_then(|v| v.get("research").and_then(crate::research::old_shape_advice))
                    .unwrap_or_else(|| format!("manifest is not valid: {strict}")))
            })?,
        };
        if manifest.schema != SCHEMA {
            return Err(format!("manifest schema {} is not {}", manifest.schema, SCHEMA));
        }
        manifest.check_requires()?;
        Ok(manifest)
    }

    /// The lenient read of a manifest written for a newer `1.x`, or `None`
    /// when the manifest is not one (it is then refused as the strict read
    /// refused it).
    fn parse_newer(json: &str) -> Option<Result<Self, String>> {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        let minor = value.get("schema_minor")?.as_u64()?;
        if value.get("schema")?.as_u64()? != u64::from(SCHEMA) || minor <= u64::from(SCHEMA_MINOR) {
            return None;
        }
        // Required features first: a newer field this build would ignore
        // must never be one the author marked as required.
        if let Some(requires) = value.get("requires").and_then(|r| r.as_array()) {
            let id = value.get("id").and_then(|i| i.as_str()).unwrap_or("?");
            let unknown: Vec<&str> =
                requires.iter().filter_map(|f| f.as_str()).filter(|f| !KNOWN_FEATURES.contains(f)).collect();
            if !unknown.is_empty() {
                return Some(Err(format!("app {id} needs a newer host: {}", unknown.join(", "))));
            }
        }
        // A scope in the toolbox's old shape is a mistake, not a newer
        // field: ignoring `languages` would widen the grant.
        if let Some(advice) = value.get("research").and_then(crate::research::old_shape_advice) {
            return Some(Err(advice));
        }
        Some(
            crate::lenient::from_value::<AppManifest>(value)
                .map(|(mut manifest, ignored)| {
                    manifest.ignored = ignored;
                    manifest
                })
                .map_err(|e| format!("manifest is not valid: {e}")),
        )
    }

    /// The fields of a manifest written for a newer `1.x` that this build
    /// does not know and ignored, as dotted paths (`network.proxy`,
    /// `agent.model.per_task.triage.budget`), for a host to log. Always
    /// empty for a manifest at or below [`SCHEMA_MINOR`], which is refused
    /// instead. Sorted.
    pub fn ignored_fields(&self) -> Vec<String> {
        let mut fields: Vec<String> = self.ignored.iter().map(|(path, _)| path.join(".")).collect();
        fields.sort();
        fields
    }

    /// Refuse a manifest that requires a feature this build does not know:
    /// running it would give the app less containment, or something other,
    /// than its author declared. Also refuse a known field its rules refuse:
    /// `integrity.github`, `host_api`, `backend` and `components` without
    /// their feature, or `window` below `schema_minor` 1 or out of range.
    pub fn check_requires(&self) -> Result<(), String> {
        let github_required = self.requires.iter().any(|f| f == "publisher-github-v1");
        if github_required != self.integrity.github.is_some() {
            return Err("GitHub publisher identity and publisher-github-v1 must be declared together".into());
        }
        if let Some(github) = &self.integrity.github {
            if self.integrity.signature.is_some() {return Err("GitHub publisher identity cannot be combined with a legacy signature".into());}
            github.validate(&self.version)?;
        }
        if let Some(api) = &self.host_api {
            if !self.requires.iter().any(|f| f == "host-api-v1") {
                return Err("host_api requires host-api-v1".into());
            }
            api.validate()?;
        }
        if let Some(backend) = &self.backend {
            if !self.requires.iter().any(|f| f == "backend-api-v1") {
                return Err("backend requires backend-api-v1".into());
            }
            if !self.storage.accounts {
                return Err("backend requires storage.accounts".into());
            }
            backend.validate()?;
        }
        self.check_components()?;
        self.check_window()?;
        let unknown: Vec<&str> =
            self.requires.iter().map(String::as_str).filter(|feature| !KNOWN_FEATURES.contains(feature)).collect();
        if unknown.is_empty() {
            return Ok(());
        }
        Err(format!("app {} needs a newer host: {}", self.id, unknown.join(", ")))
    }

    /// The `components` rules (App Hub ADR 0003): the field needs
    /// `wasm-shared-components-v1`, and each of at
    /// most [`MAX_COMPONENTS`] entries has a unique name, a component id, one
    /// exact version and a BLAKE3 digest. Whether each resolves, and whether
    /// its imports are implemented, is the gate's and the host's check.
    fn check_components(&self) -> Result<(), String> {
        let required = self.requires.iter().any(|f| f == "wasm-shared-components-v1");
        if !self.components.is_empty() && !required {
            return Err("components requires wasm-shared-components-v1".into());
        }
        if self.components.len() > MAX_COMPONENTS {
            return Err(format!("an app may name at most {MAX_COMPONENTS} components, not {}", self.components.len()));
        }
        let mut aliases = std::collections::BTreeSet::new();
        for component in &self.components {
            component.check()?;
            if !aliases.insert(component.alias.as_str()) {
                return Err(format!("component name {:?} is used twice", component.alias));
            }
        }
        Ok(())
    }

    /// The `window` rules: the field arrived with `schema_minor` 1, so a
    /// manifest that sets it must say so (an older host then ignores the
    /// hint instead of refusing an unknown field), and each side must be in
    /// range. The hint grants nothing, so nothing else is checked.
    fn check_window(&self) -> Result<(), String> {
        let Some(window) = &self.window else { return Ok(()) };
        if self.schema_minor < WINDOW_SCHEMA_MINOR {
            return Err(format!(
                "window requires \"schema_minor\": {WINDOW_SCHEMA_MINOR} or later, so that hosts which predate window ignore it instead of refusing the app"
            ));
        }
        if !WINDOW_WIDTHS.contains(&window.width) {
            return Err(format!(
                "window width must be {} to {} points, not {}",
                WINDOW_WIDTHS.start(),
                WINDOW_WIDTHS.end(),
                window.width
            ));
        }
        if !WINDOW_HEIGHTS.contains(&window.height) {
            return Err(format!(
                "window height must be {} to {} points, not {}",
                WINDOW_HEIGHTS.start(),
                WINDOW_HEIGHTS.end(),
                window.height
            ));
        }
        Ok(())
    }

    /// Device-side compatibility. Admission validates declarations separately;
    /// installers and runners must supply their actual implemented API versions.
    pub fn check_host_apis(&self, available: &BTreeMap<String, u32>) -> Result<(), String> {
        if let Some(api) = &self.host_api { api.check_available(available)?; }
        for (feature, method) in [("host-api-v1", "app_policy.device_consent"), ("backend-api-v1", "auth.backend.request"), ("script-tools-v1", "app_tools.dispatch")] {
            if self.requires.iter().any(|f| f == feature) && available.get(method) != Some(&1) {
                return Err(format!("app {} needs a host implementing {method}@1", self.id));
            }
        }
        Ok(())
    }

    /// The research scope as the host grants it (normalised), for the words
    /// a store shows. A scope the host would refuse is shown as written; an
    /// absent one as `{}`.
    pub fn shown_research_scope(&self) -> ResearchScope {
        let scope = self.research.clone().unwrap_or_default();
        scope.validated().unwrap_or(scope)
    }

    /// The bytes a signature covers: the manifest without its own signature,
    /// serialised canonically, so the same manifest always signs the same way.
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        let mut bare = self.clone();
        bare.integrity.signature = None;
        if let Some(github) = &mut bare.integrity.github { github.attestation = None; }
        let mut value = serde_json::to_value(&bare).map_err(|e| e.to_string())?;
        // A newer manifest was signed with the fields this build ignored.
        crate::lenient::restore(&mut value, &self.ignored);
        Ok(canonical(&value).into_bytes())
    }
}

/// Canonical JSON: object keys sorted, no insignificant whitespace. Enough
/// for a stable signing input; it is not a general JCS implementation.
fn canonical(value: &serde_json::Value) -> String {
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
        serde_json::Value::Array(items) => {
            let body: Vec<String> = items.iter().map(canonical).collect();
            format!("[{}]", body.join(","))
        }
        other => other.to_string(),
    }
}
