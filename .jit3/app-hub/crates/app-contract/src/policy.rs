//! Resolving a manifest into what the app actually gets.
//!
//! Validates disclosure names and metadata, admission identity and resource
//! ceilings. The manifest's capability/host sets are retained for disclosure,
//! not execution authorization. Hosts enforce actual consent, account scopes,
//! availability, isolation and inter-app sharing independently. App Hub adds
//! the reviewed agent profile for hosts that run app agents.
use crate::manifest::{AgentWorkspace, AppManifest, KNOWN_CAPABILITIES};
use crate::research::ResearchScope;
use serde::Serialize;
use std::collections::BTreeSet;

/// The host's own ceilings. An app may ask for less and get it; asking for
/// more is clamped, not refused, because a bundle built for a roomier device
/// should still run here — just smaller.
///
/// `max_iterations`, `max_token_budget` and `offered_tools` are ceilings for
/// an app's agent session. [`resolve`] does not read them; a host that runs
/// app agents does, with the same limits value.
///
/// Build one from [`HostLimits::default`] or [`HostLimits::system`] and the
/// `with_*` methods; the struct is `#[non_exhaustive]`, so a later `1.x` can
/// add a ceiling without breaking anyone:
///
/// ```
/// use octosense_app_contract::HostLimits;
/// let limits = HostLimits::default().with_require_signature(false).with_max_storage_bytes(1 << 20);
/// assert!(!limits.require_signature);
/// ```
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct HostLimits {
    pub max_storage_bytes: u64,
    pub max_instruction_budget: u64,
    pub max_memory_bytes: u64,
    pub max_iterations: u32,
    pub max_token_budget: u64,
    /// Tools this host offers to contained apps' agents at all. Shell,
    /// process and arbitrary-path file tools are absent from this list by
    /// design; of the octos kernel's own tools only `ask_user_question` may
    /// ever be here.
    pub offered_tools: Vec<String>,
    /// Whether a bundle must carry a signature to be admitted.
    pub require_signature: bool,
}

impl Default for HostLimits {
    /// Phone-sized defaults: the isolate jail's own ceiling for storage, a
    /// budget that cannot spin the UI thread for a second, and a tool list
    /// holding only what a card app legitimately needs: the host tools, and
    /// the kernel's `ask_user_question`.
    fn default() -> Self {
        HostLimits {
            max_storage_bytes: 16 * 1024 * 1024,
            max_instruction_budget: 20_000_000,
            max_memory_bytes: 64 * 1024 * 1024,
            max_iterations: 8,
            max_token_budget: 200_000,
            offered_tools: [
                "ledger.read",
                "ledger.write",
                "net.fetch",
                "storage.read",
                "storage.write",
                "card.render",
                // The one kernel tool a contained app's agent may keep.
                "ask_user_question",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            require_signature: true,
        }
    }
}

impl HostLimits {
    /// Ceilings for a system app: a bundle that ships inside the build, like
    /// News or Photos, contained like any installed app but living for as
    /// long as the person keeps it open. An installed card's budget is sized
    /// for a card; an app that is used for an hour needs room for an hour.
    /// A system app is part of the signed build, so it is admitted by its
    /// digest alone.
    pub fn system() -> Self {
        HostLimits {
            max_storage_bytes: 64 * 1024 * 1024,
            max_instruction_budget: 4_000_000_000,
            max_memory_bytes: 128 * 1024 * 1024,
            require_signature: false,
            ..HostLimits::default()
        }
    }

    /// Whether a bundle must carry a signature to be admitted.
    pub fn with_require_signature(mut self, require: bool) -> Self {
        self.require_signature = require;
        self
    }

    /// The whole-jail storage ceiling, in bytes.
    pub fn with_max_storage_bytes(mut self, bytes: u64) -> Self {
        self.max_storage_bytes = bytes;
        self
    }

    /// The ceiling on a session's cumulative script instructions.
    pub fn with_max_instruction_budget(mut self, instructions: u64) -> Self {
        self.max_instruction_budget = instructions;
        self
    }

    /// The ceiling on an isolate's heap, in bytes.
    pub fn with_max_memory_bytes(mut self, bytes: u64) -> Self {
        self.max_memory_bytes = bytes;
        self
    }

    /// The ceiling on an app agent's model turns per request.
    pub fn with_max_iterations(mut self, iterations: u32) -> Self {
        self.max_iterations = iterations;
        self
    }

    /// The ceiling on an app agent's tokens per request.
    pub fn with_max_token_budget(mut self, tokens: u64) -> Self {
        self.max_token_budget = tokens;
        self
    }

    /// The tools this host offers contained apps' agents, replacing the list.
    pub fn with_offered_tools<I, T>(mut self, tools: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        self.offered_tools = tools.into_iter().map(Into::into).collect();
        self
    }
}

/// What the app may do. Produced only by [`resolve`].
///
/// Serialises to the JSON the contract's fixture corpus pins.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct AppPolicy {
    pub app_id: String,
    pub version: String,
    pub display_name: String,
    /// Declared capability names for disclosure, sorted and deduplicated.
    /// Not an execution permission or evidence of user consent.
    pub capabilities: BTreeSet<String>,
    /// Declared network destinations for disclosure, not an allowlist.
    pub hosts: BTreeSet<String>,
    /// The app's whole storage jail, in bytes: the `storage` block's
    /// `max_bytes`, clamped to the host's ceiling. The same as
    /// `storage.max_bytes`.
    pub storage_bytes: u64,
    /// The whole `storage` block as granted (OctoSense ADR 0004 §11), so a
    /// host lays out the app's folders without reading the manifest again.
    pub storage: StorageGrant,
    /// Cumulative script instructions for the app's session.
    pub instruction_budget: u64,
    /// Heap ceiling for the app's isolate.
    pub memory_bytes: u64,
    /// The `prompt` capability: may the app ask the person questions of its
    /// own. A service's sheet (Mail's sign-in) does not need it; whether any
    /// sheet may appear is the host surface's call.
    pub may_prompt: bool,
    /// The scope of `research` and `crawl`, validated and normalised as
    /// octos's `Scope::from_grant` does: the grant the host hands the
    /// toolbox. `Some` exactly when the app requests either capability.
    pub research: Option<ResearchScope>,
}

/// The manifest's `storage` block as granted. `external` (a path outside
/// the jail) is for reviewed native apps only and is not part of a script
/// app's manifest, so it is not here either.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct StorageGrant {
    /// The whole-jail ceiling: `max_bytes`, clamped to the host's ceiling.
    pub max_bytes: u64,
    /// Data per account, one agent per account; `false` is one `device`
    /// folder.
    pub accounts: bool,
    /// What the app's agent may read from disk: its account's folder (the
    /// default when the manifest says nothing) or nothing.
    pub agent_workspace: AgentWorkspace,
    /// The ceiling for the jail's `cache/`, as declared (positive; the cache
    /// lives inside the jail, so `max_bytes` bounds it too).
    pub cache_max_bytes: Option<u64>,
}

impl AppPolicy {
    /// Legacy declaration query. This does not authorize or deny execution;
    /// hosts check admission, actual consent, account scope and availability.
    pub fn allows(&self, capability: &str) -> bool {
        self.capabilities.contains(capability)
    }

    /// Legacy declaration query, not a runtime network authorization check.
    pub fn allows_host(&self, host: &str) -> bool {
        self.allows("net") && self.hosts.contains(host)
    }
}

/// Resolve a parsed manifest against this host.
pub fn resolve(manifest: &AppManifest, limits: &HostLimits) -> Result<AppPolicy, String> {
    manifest.check_requires()?;
    check_id(&manifest.id)?;
    if manifest.storage.cache_max_bytes == Some(0) {
        return Err(format!("app {}: storage.cache_max_bytes must be positive", manifest.id));
    }
    if manifest.version.trim().is_empty() {
        return Err("manifest version is empty".into());
    }
    if limits.require_signature && manifest.integrity.signature.is_none() && !manifest.integrity.github.as_ref().is_some_and(|g| g.attestation.is_some()) {
        return Err(format!("app {} is unsigned and this host requires a signature", manifest.id));
    }

    let mut capabilities = BTreeSet::new();
    for capability in &manifest.capabilities {
        if !KNOWN_CAPABILITIES.contains(&capability.as_str()) {
            return Err(format!("app {} requests unknown capability {:?}", manifest.id, capability));
        }
        capabilities.insert(capability.clone());
    }

    let mut hosts = BTreeSet::new();
    for host in &manifest.network.hosts {
        check_host(host)?;
        hosts.insert(host.to_ascii_lowercase());
    }
    let research = resolve_research(&manifest.id, &capabilities, manifest.research.as_ref())?;
    let storage_bytes = clamp(manifest.storage.max_bytes, limits.max_storage_bytes);

    Ok(AppPolicy {
        app_id: manifest.id.clone(),
        version: manifest.version.clone(),
        display_name: manifest.name.clone(),
        may_prompt: capabilities.contains("prompt"),
        capabilities,
        hosts,
        storage_bytes,
        storage: StorageGrant {
            max_bytes: storage_bytes,
            accounts: manifest.storage.accounts,
            agent_workspace: manifest.storage.agent_workspace.unwrap_or_default(),
            cache_max_bytes: manifest.storage.cache_max_bytes,
        },
        instruction_budget: clamp(manifest.compute.instruction_budget, limits.max_instruction_budget),
        memory_bytes: clamp(manifest.compute.memory_bytes, limits.max_memory_bytes),
        research,
    })
}

/// The `research` scope against the `research` and `crawl` capabilities.
/// The scope says what the capabilities reach, so neither goes without the
/// other: a capability without a scope would reach whatever the host
/// defaults to, which the store could not show, and a scope without a
/// capability is a mistake the author should see. Crawl limits need `crawl`,
/// because octos grants crawling from the limits alone.
fn resolve_research(
    app_id: &str,
    capabilities: &BTreeSet<String>,
    scope: Option<&ResearchScope>,
) -> Result<Option<ResearchScope>, String> {
    let research = capabilities.contains("research");
    let crawl = capabilities.contains("crawl");
    let Some(scope) = scope else {
        if research || crawl {
            let cap = if research { "research" } else { "crawl" };
            return Err(format!(
                "app {app_id} requests {cap} but declares no research scope; add a top-level \"research\" object (octos's scope; {{}} means no limits)"
            ));
        }
        return Ok(None);
    };
    let scope = scope.validated().map_err(|e| format!("app {app_id} {e}"))?;
    if crawl && !scope.crawls() {
        return Err(format!(
            "app {app_id} requests crawl, so its research scope needs max_depth and max_pages above 0 (got {} and {})",
            scope.max_depth, scope.max_pages
        ));
    }
    Ok(Some(scope))
}

/// An id is a path component of the app's jail, so it may not be empty, may
/// not navigate, and may not surprise a filesystem.
fn check_id(id: &str) -> Result<(), String> {
    check_id_shape(id, "app id")?;
    crate::manifest::check_reserved_id(id)
}

/// The shape every id has (an app's, a shared component's): `what` names
/// it in the refusal.
pub(crate) fn check_id_shape(id: &str, what: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 64 {
        return Err(format!("{what} {id:?} must be 1 to 64 characters"));
    }
    if !id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.') {
        return Err(format!("{what} {id:?} may hold only lowercase letters, digits, '-' and '.'"));
    }
    if id.starts_with('.') || id.contains("..") {
        return Err(format!("{what} {id:?} may not navigate the filesystem"));
    }
    Ok(())
}

/// A bare host: no scheme, no path, no port, no wildcard. The service adds
/// HTTPS; the app never names a scheme, so it cannot ask for plain HTTP.
fn check_host(host: &str) -> Result<(), String> {
    if host.is_empty() || host.len() > 253 {
        return Err(format!("host {host:?} must be 1 to 253 characters"));
    }
    if host.contains("://") || host.contains('/') {
        return Err(format!("host {host:?} must be a bare host name, with no scheme or path"));
    }
    if host.contains('*') {
        return Err(format!("host {host:?} may not use a wildcard"));
    }
    if host.contains(':') {
        return Err(format!("host {host:?} may not name a port"));
    }
    if !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') {
        return Err(format!("host {host:?} holds a character a host name may not"));
    }
    if host.starts_with('.') || host.ends_with('.') || host.contains("..") {
        return Err(format!("host {host:?} is not a well-formed host name"));
    }
    Ok(())
}

fn clamp(asked: Option<u64>, ceiling: u64) -> u64 {
    asked.unwrap_or(ceiling).min(ceiling)
}
