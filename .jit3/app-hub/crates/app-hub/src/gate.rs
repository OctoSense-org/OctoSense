//! Stage one of admission: the checks a model never makes (ADR 0003 §5).
//!
//! Everything here is a fact about the bundle, decided by code, reported the
//! same way whether it runs on a developer's machine before submitting or in
//! the hub's job after. A finding is either a refusal or a warning; an agent
//! scan runs afterwards on what passes, and never overrides a refusal.
use crate::index::{Catalog, Entry};
use octosense_app_policy::{digest_dir, policy, AppManifest, AppPolicy, HostLimits, Listing, SignatureVerifier, LISTING_FILE};
use std::path::Path;

/// Everything a bundle may hold besides its manifest, by extension. A bundle
/// is cards, data and artwork; anything else is a refusal, so a publisher
/// cannot smuggle a payload the checks do not understand. The one exception
/// is the app's own functions, `fns/*.wasm` (below).
const ALLOWED_EXTENSIONS: &[&str] = &["card", "json", "l0", "octoscript", "splash", "svg", "png", "jpg", "jpeg", "webp", "ttf", "otf", "txt", "md"];

/// The most WebAssembly modules (`fns/*.wasm`) a bundle may carry.
pub const MAX_FUNCTION_MODULES: usize = 8;

/// Ceiling for a whole bundle. Cards are text and artwork; a bundle bigger
/// than this is either shipping something it should not, or should be split.
pub const MAX_BUNDLE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// The bundle is not admitted.
    Refusal,
    /// Admitted, but the publisher and a reviewer should see it.
    Warning,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub check: &'static str,
    pub detail: String,
    /// Where in the bundle, when one file or property is at fault.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Finding {
    fn refuse(check: &'static str, detail: impl Into<String>) -> Self {
        Finding { severity: Severity::Refusal, check, detail: detail.into(), path: None }
    }
    fn warn(check: &'static str, detail: impl Into<String>) -> Self {
        Finding { severity: Severity::Warning, check, detail: detail.into(), path: None }
    }
    pub(crate) fn at(check: &'static str, path: impl Into<String>, detail: impl Into<String>) -> Self {
        Finding { severity: Severity::Refusal, check, detail: detail.into(), path: Some(path.into()) }
    }
    fn warn_at(check: &'static str, path: impl Into<String>, detail: impl Into<String>) -> Self {
        Finding { severity: Severity::Warning, check, detail: detail.into(), path: Some(path.into()) }
    }
}

#[derive(Debug)]
pub struct GateReport {
    pub app_id: String,
    pub version: String,
    pub digest: String,
    pub findings: Vec<Finding>,
    /// What the app would actually get, when the gate passed.
    pub policy: Option<AppPolicy>,
    /// What the bundle's data and artwork reference, by JSON pointer.
    pub resources: Vec<crate::admission::ResourceReference>,
    /// The manifest this report checked, so an entry is made from exactly it.
    pub(crate) admitted_manifest: Vec<u8>,
}

impl GateReport {
    /// The report as `hub check --json` prints it.
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": 1, "stage": "structural", "passed": self.passed(),
            "app_id": self.app_id, "version": self.version, "digest": self.digest,
            "findings": self.findings, "resources": self.resources,
        })
    }

    pub fn passed(&self) -> bool {
        !self.findings.iter().any(|f| f.severity == Severity::Refusal)
    }

    /// One line per finding, for a pull-request comment or a terminal.
    pub fn render(&self) -> String {
        let mut out = format!("{} {} — {}\n", self.app_id, self.version, if self.passed() { "PASSED" } else { "REFUSED" });
        for finding in &self.findings {
            let mark = match finding.severity {
                Severity::Refusal => "refused",
                Severity::Warning => "warning",
            };
            let path = finding.path.as_ref().map(|p| format!(" ({p})")).unwrap_or_default();
            out.push_str(&format!("  [{mark}] {}{path}: {}\n", finding.check, finding.detail));
        }
        if let Some(policy) = &self.policy {
            let storage = format!("{} bytes", policy.storage_bytes);
            out.push_str(&format!(
                "  declarations: capabilities {:?}, hosts {:?}, storage {storage}, agent {}\n",
                policy.capabilities,
                policy.hosts,
                policy.agent.as_ref().map(|a| a.profile.as_kernel_mode()).unwrap_or("none")
            ));
            if let Some(scope) = &policy.research {
                out.push_str(&format!(
                    "  research scope (octos Scope): {}\n",
                    serde_json::to_string(scope).unwrap_or_default()
                ));
            }
        }
        out
    }
}

/// Run the deterministic gate over a bundle directory.
///
/// `previous` is the catalog the hub already published, used for the identity
/// rules: a version may not be republished, and once a publisher key is on
/// record every later version, and every new app of that publisher, must be
/// signed by it ([`crate::publishers`]). The caller authenticates `previous`
/// before passing it as trusted history.
pub fn check_bundle(
    bundle: &Path,
    limits: &HostLimits,
    verifier: &dyn SignatureVerifier,
    previous: Option<&Catalog>,
) -> Result<GateReport, String> {
    check_bundle_for(bundle, limits, verifier, previous, false, None)
}

/// Check a shipped system app during development, using the same resource
/// ceilings as the system runner. This report cannot become a store entry.
/// Store admission always uses [`check_bundle`], including with system limits.
pub fn check_system_bundle(
    bundle: &Path,
    verifier: &dyn SignatureVerifier,
) -> Result<GateReport, String> {
    check_bundle_for(bundle, &HostLimits::system(), verifier, None, true, None)
}

/// Check against an opaque authenticated publication history, without losing the
/// authenticated publisher bindings.
pub fn check_authenticated_bundle(bundle:&Path,limits:&HostLimits,verifier:&dyn SignatureVerifier,
    base:&crate::github_catalog::AuthenticatedCatalog)->Result<GateReport,String>{
    check_bundle_with_registry(bundle,limits,verifier,base.catalog(),base.publishers())
}

pub(crate) fn check_bundle_with_registry(bundle:&Path, limits:&HostLimits, verifier:&dyn SignatureVerifier,
    previous:&Catalog, registry:&dyn crate::PublisherRegistry)->Result<GateReport,String> {
    check_bundle_for(bundle,limits,verifier,Some(previous),false,Some(registry))
}

fn check_bundle_for(
    bundle: &Path,
    limits: &HostLimits,
    verifier: &dyn SignatureVerifier,
    previous: Option<&Catalog>,
    system_development: bool,
    registry: Option<&dyn crate::PublisherRegistry>,
) -> Result<GateReport, String> {
    // Metadata first: nothing is read or hashed beyond the limits.
    let files = crate::admission::inventory(bundle)?;
    let manifest_path = bundle.join(octosense_app_policy::MANIFEST_FILE);
    let manifest_json = crate::admission::read_text(&manifest_path, crate::admission::MAX_MANIFEST_BYTES)?;
    let manifest = AppManifest::parse(&manifest_json)?;
    let digest = digest_dir(bundle)?;
    let (mut findings, resources) = crate::admission::validate(bundle, &files);

    // ---- integrity ------------------------------------------------------
    if !digest.eq_ignore_ascii_case(&manifest.integrity.bundle_blake3) {
        findings.push(Finding::refuse(
            "digest",
            format!("the bundle hashes to {digest}, the manifest claims {}", manifest.integrity.bundle_blake3),
        ));
    }
    if let Err(e) = octosense_app_policy::verify::verify_manifest(&manifest, verifier) {
        findings.push(Finding::refuse("publisher-signature", e));
    }
    if manifest.integrity.signature.is_none() && manifest.integrity.github.is_none() {
        if limits.require_signature {
            findings.push(Finding::refuse("publisher-signature", "this hub requires a signed manifest"));
        } else {
            findings.push(Finding::warn("publisher-signature", "unsigned: accountability rests on the hub alone"));
        }
    }

    // ---- identity ------------------------------------------------------
    // Ids under `os.` are the system apps' (appstore::system): every device
    // refuses to install one from a store, so the hub refuses to offer one.
    if manifest.id.starts_with("os.") && !system_development {
        findings.push(Finding::refuse(
            "identity",
            format!("{} is under os., which is reserved for system apps that ship with the device", manifest.id),
        ));
    }
    if system_development && !manifest.id.starts_with("os.") {
        findings.push(Finding::refuse(
            "identity",
            "--system-app checks only shipped os.* apps; use ordinary check/scan for store apps",
        ));
    }
    // Native apps' ids and the host's own names (app-policy's
    // RESERVED_NAMES) are refused by the agent review below (as "identity")
    // and by the policy.

    // ---- contents -------------------------------------------------------
    let mut total = 0u64;
    let mut function_files = Vec::new();
    // A system app's shared components, `components/<blake3>.wasm` (App Hub
    // ADR 0003): checked with the manifest's `components` below.
    let mut bundled_components = Vec::new();
    for file in list_files(bundle)? {
        let path = bundle.join(&file);
        let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
        total += size;
        let extension = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if extension == "wasm" && file.starts_with(crate::components::BUNDLED_DIR) {
            bundled_components.push(file.clone());
        } else if extension == "wasm" {
            function_files.push(file.clone());
        } else if !ALLOWED_EXTENSIONS.contains(&extension.as_str()) {
            findings.push(Finding::refuse(
                "contents",
                format!("{} has extension {extension:?}, which a bundle may not hold", file.display()),
            ));
        }
    }
    if total > MAX_BUNDLE_BYTES {
        findings.push(Finding::refuse("size", format!("the bundle is {total} bytes, over the {MAX_BUNDLE_BYTES} ceiling")));
    }

    // ---- the app's own functions ----------------------------------------
    // A WebAssembly module is the one executable a bundle may carry: the
    // app's own functions in `fns/` (admission checks each name and header),
    // run by the host's `wasm` service in a sandbox, under the `wasm`
    // capability a store shows the person.
    let declares_wasm = manifest.capabilities.iter().any(|c| c == "wasm");
    let modules = function_files.len();
    if modules > 0 && !declares_wasm {
        findings.push(Finding::warn(
            "functions",
            format!("the bundle carries {modules} WebAssembly module(s); add wasm to its usage disclosures"),
        ));
    }
    if modules > MAX_FUNCTION_MODULES {
        findings.push(Finding::refuse(
            "functions",
            format!("the bundle carries {modules} WebAssembly modules, over the {MAX_FUNCTION_MODULES} it may"),
        ));
    }
    if declares_wasm && modules == 0 && manifest.components.is_empty() {
        findings.push(Finding::warn("functions", "the bundle declares the wasm capability but carries no fns/*.wasm"));
    }
    // A component also reaches supported WASI interfaces. Admission validated
    // its imports; required ABIs keep unsupported hosts from installing it.
    // Reviewers see its reach even when usage disclosures are omitted.
    let requires_components = manifest.requires.iter().any(|f| f == crate::functions::COMPONENTS_FEATURE);
    let declares_storage = manifest.capabilities.iter().any(|c| c == "storage");
    let declares_net = manifest.capabilities.iter().any(|c| c == "net");
    let mut components = 0usize;
    for file in &function_files {
        let name = octosense_app_policy::portable_path(file).unwrap_or_else(|| file.to_string_lossy().replace('\\', "/"));
        let Ok(bytes) = crate::admission::read_bounded(&bundle.join(file), MAX_BUNDLE_BYTES) else { continue };
        if !crate::functions::is_component(&bytes) {
            continue;
        }
        components += 1;
        // Admission reported a component that does not validate or imports
        // what no host gives it.
        let Ok(info) = crate::functions::inspect_component(&bytes) else { continue };
        if !info.refused_imports().is_empty() {
            continue;
        }
        if !requires_components {
            findings.push(Finding::refuse(
                "functions",
                format!("{name} is a WebAssembly component; the manifest must require {}", crate::functions::COMPONENTS_FEATURE),
            ));
        }
        if let Some(missing) = info.missing_host_api(&manifest) {
            findings.push(Finding::refuse("functions", format!("{name} {missing}")));
        }
        if info.uses_sockets() && !declares_net {
            findings.push(Finding::warn("functions", format!("{name} imports wasi:sockets; disclose net usage")));
        }
        if info.uses_files() && !declares_storage {
            findings.push(Finding::warn(
                "functions",
                format!("{name} imports wasi:filesystem; disclose storage usage (the app jail and quota still apply)"),
            ));
        }
        if info.uses_http() && !declares_net {
            findings.push(Finding::warn(
                "functions",
                format!("{name} imports wasi:http; disclose net usage"),
            ));
        }
        findings.push(Finding::warn_at("functions", name.clone(), format!("{name} is a component that reaches {}", info.reach())));
        // What it says it is built from, for reviewers and for
        // `hub check --advisory-db` (the gate cannot check it against the code).
        findings.push(Finding::warn_at("functions", name.clone(), match &info.crates {
            Some(crates) => format!("{name} is built from {}", crate::functions::crate_line(crates)),
            None => format!(
                "{name} does not list the crates it is built from (its {} section); tools/octo wasm build and App Flow's release workflow add it",
                crate::functions::CRATES_SECTION
            ),
        }));
    }
    if requires_components && components == 0 {
        findings.push(Finding::warn(
            "functions",
            format!("the manifest requires {} but fns/ holds no component; hosts without it refuse the app", crate::functions::COMPONENTS_FEATURE),
        ));
    }

    // ---- shared components (App Hub ADR 0003) -----------------------------
    // Each component the manifest names must resolve: from the catalog for a
    // store app (the exact version, offered, hashing to the pinned digest),
    // from the bundle's components/<blake3>.wasm for a system app. The app
    // must require each imported ABI, as for its own components, and
    // reviewers see what each reaches.
    shared_component_findings(bundle, &manifest, &bundled_components, previous, system_development, &mut findings);

    // ---- assets are local ----------------------------------------------
    // ADR 0002's prototype found a card with no network grant fetching nine
    // images over HTTP, because the resource loader is not the network module
    // the grant gates. Until that is closed in the runtime, the gate is what
    // keeps a bundle from reaching outside itself.
    for reference in external_references(bundle, &manifest)? {
        findings.push(Finding::refuse(
            "assets",
            format!("{reference} points outside the bundle; ship the asset with the app"),
        ));
    }

    // ---- secrets are the host's ----------------------------------------
    // A contained app never collects a password, a PIN or a code: the host's
    // sheet does, for the service that needs it (appstore::services). The
    // runtime makes such a field inert; the gate refuses the bundle, so a
    // publisher learns it before a person meets a dead field.
    for field in secret_fields(bundle)? {
        findings.push(Finding::refuse(
            "secrets",
            format!("{field}: apps may not ask for passwords or codes; a host service collects them on its own sheet"),
        ));
    }

    // ---- storage is asked for ------------------------------------------
    // An app gets its storage only when it declares `storage`; without it,
    // every fs call errors and the camera saves nothing. Warned, not
    // refused: the publisher may handle the error, and a camera preview
    // needs no storage.
    for warning in storage_warnings(bundle, &manifest)? {
        findings.push(Finding::warn("storage", warning));
    }

    // ---- the listing ----------------------------------------------------
    // A store shows nothing it has not reviewed: the listing ships in the
    // bundle, under the same digest, and its assets must be there too.
    match std::fs::read_to_string(bundle.join(LISTING_FILE)) {
        Err(_) => findings.push(Finding::refuse("listing", format!("no {LISTING_FILE}: a store needs a description, a category, screenshots, a publisher and the platforms it runs on"))),
        Ok(text) => match Listing::parse(&text) {
            Err(e) => findings.push(Finding::refuse("listing", e)),
            Ok(listing) => {
                // A screenshot is the one listing claim a reviewer can check
                // against the rendered card, and the icon is what the
                // launcher shows once installed: both are required, as on
                // every store people know.
                if listing.screenshots.is_empty() {
                    findings.push(Finding::refuse("listing", "no screenshots: at least one PNG in the bundle, named in the listing, is required"));
                }
                if listing.icon.is_none() {
                    findings.push(Finding::refuse("listing", "no icon: a square PNG or SVG in the bundle, named in the listing, is required"));
                }
                for asset in listing.screenshots.iter().chain(listing.icon.iter()) {
                    if !bundle.join(asset).is_file() {
                        findings.push(Finding::refuse("listing", format!("{asset} is named by the listing but is not in the bundle")));
                    }
                }
            }
        },
    }

    // ---- the app's agent and tools (ADR 0002 §3, §4) --------------------
    // tools.json, AGENT.md and skills are checked here and pinned by the
    // digest above like every other file. A native module's tools.json gets
    // the same checks from the app-peers broker (ToolManifest::load).
    let agent_review = octosense_app_policy::agent::review(bundle, &manifest);
    for issue in &agent_review.issues {
        findings.push(if issue.refusal {
            Finding::refuse(issue.check, issue.detail.clone())
        } else {
            Finding::warn(issue.check, issue.detail.clone())
        });
    }
    if let Some(agent) = &agent_review.bundle {
        for tool in agent.tools.iter().filter(|t| t.supervision().needs_person()) {
            // Recorded, not refused: a destructive or outward tool is
            // allowed, and the host always asks before it runs, whoever
            // calls it.
            let who = match tool.confirm {
                octosense_app_policy::Confirm::Host => "the host's approval",
                octosense_app_policy::Confirm::App => "the app's own confirmation sheet (an approval request when the person is away)",
            };
            findings.push(Finding::warn(
                "tools",
                format!(
                    "{} is {}: every call waits for {who}",
                    tool.name,
                    if tool.risk == octosense_app_policy::Risk::Destructive { "destructive" } else { "outward" }
                ),
            ));
        }
    }

    // ---- what it would get ----------------------------------------------
    let policy = match policy::resolve(&manifest, limits) {
        Ok(policy) => Some(policy),
        Err(e) => {
            findings.push(Finding::refuse("policy", e));
            None
        }
    };

    // ---- identity against what is already published ----------------------
    if let Some(catalog) = previous {
        for entry in &catalog.entries {
            if entry.app_id() == manifest.id && entry.version() == manifest.version {
                findings.push(Finding::refuse(
                    "version",
                    format!("version {} of {} is already published; publish a new version", manifest.version, manifest.id),
                ));
            }
        }
        let continuity = match registry {
            Some(registry) => crate::publishers::verify_continuity(&manifest, registry),
            None => crate::publishers::CatalogPublishers::from_catalog(catalog)
                .and_then(|registry| crate::publishers::verify_continuity(&manifest, &registry)),
        };
        if let Err(e) = continuity {
            findings.push(Finding::refuse("continuity", e));
        }
    }

    let admitted_manifest = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
    Ok(GateReport { app_id: manifest.id, version: manifest.version, digest, findings, policy, resources, admitted_manifest })
}

/// The `components` findings for an app's shared components (App Hub ADR
/// 0003): each resolved from `previous` (a store app) or from the bundle's
/// `components/` (a system app), checked against required ABIs, and told to
/// reviewers.
fn shared_component_findings(
    bundle: &Path,
    manifest: &AppManifest,
    bundled: &[std::path::PathBuf],
    previous: Option<&Catalog>,
    system_development: bool,
    findings: &mut Vec<Finding>,
) {
    use crate::functions::SHARED_COMPONENTS_FEATURE;
    if manifest.requires.iter().any(|f| f == SHARED_COMPONENTS_FEATURE) && manifest.components.is_empty() {
        findings.push(Finding::warn(
            "components",
            format!("the manifest requires {SHARED_COMPONENTS_FEATURE} but names no components; hosts without it refuse the app"),
        ));
    }
    let named = |file: &std::path::Path| {
        manifest.components.iter().any(|c| file == std::path::Path::new(crate::components::BUNDLED_DIR).join(format!("{}.wasm", c.blake3)))
    };
    for file in bundled {
        let name = octosense_app_policy::portable_path(file).unwrap_or_else(|| file.to_string_lossy().replace('\\', "/"));
        if !system_development {
            findings.push(Finding::at(
                "components",
                name,
                "components/ ships only in a system app's bundle; a store app's components come from the catalog",
            ));
        } else if !named(file) {
            findings.push(Finding::at("components", name, "this file is not one of the manifest's components"));
        }
    }
    for dependency in &manifest.components {
        let info = if system_development {
            let relative = format!("{}/{}.wasm", crate::components::BUNDLED_DIR, dependency.blake3);
            let path = bundle.join(&relative);
            if std::fs::symlink_metadata(&path).is_err() {
                findings.push(Finding::refuse(
                    "components",
                    format!("component {} is not in the bundle: a system app ships it as {relative}", dependency.describe()),
                ));
                continue;
            }
            // Admission validated the file, its imports and its name.
            match crate::admission::read_bounded(&path, MAX_BUNDLE_BYTES).and_then(|bytes| crate::functions::admissible_component(&bytes)) {
                Ok(info) => info,
                Err(_) => continue,
            }
        } else if let Some(catalog) = previous {
            match crate::components::resolve(catalog, dependency) {
                Ok(entry) => crate::components::info_of(&entry.component),
                Err(e) => {
                    findings.push(Finding::refuse("components", e));
                    continue;
                }
            }
        } else {
            findings.push(Finding::warn(
                "components",
                format!(
                    "component {} is not resolved: check with --catalog to see that it is offered, matches its digest and what it reaches",
                    dependency.describe()
                ),
            ));
            continue;
        };
        if let Some(missing) = info.missing_host_api(manifest) {
            findings.push(Finding::refuse("components", format!("component {} {missing}", dependency.describe())));
        }
        if info.uses_sockets() && !manifest.capabilities.iter().any(|capability| capability == "net") {
            findings.push(Finding::warn("components", format!("component {} imports wasi:sockets; disclose net usage", dependency.describe())));
        }
        findings.push(Finding::warn("components", format!("component {} reaches {}", dependency.describe(), info.reach())));
    }
}

/// The gate's report on a shared component release (App Hub ADR 0003).
#[derive(Debug)]
pub struct ComponentReport {
    pub id: String,
    pub version: String,
    /// BLAKE3 of the file that was checked.
    pub digest: String,
    pub findings: Vec<Finding>,
    /// The release this report checked, so an entry is made from exactly it.
    pub(crate) admitted_release: Vec<u8>,
}

impl ComponentReport {
    pub fn passed(&self) -> bool {
        !self.findings.iter().any(|f| f.severity == Severity::Refusal)
    }

    /// One line per finding, as [`GateReport::render`] writes them.
    pub fn render(&self) -> String {
        let mut out = format!(
            "{} {} (component {}) — {}\n",
            self.id,
            self.version,
            self.digest,
            if self.passed() { "PASSED" } else { "REFUSED" }
        );
        for finding in &self.findings {
            let mark = match finding.severity {
                Severity::Refusal => "refused",
                Severity::Warning => "warning",
            };
            let path = finding.path.as_ref().map(|p| format!(" ({p})")).unwrap_or_default();
            out.push_str(&format!("  [{mark}] {}{path}: {}\n", finding.check, finding.detail));
        }
        out
    }

    /// The report as `hub component-check --json` prints it.
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": 1, "stage": "structural", "kind": "component", "passed": self.passed(),
            "id": self.id, "version": self.version, "digest": self.digest, "findings": self.findings,
        })
    }
}

/// Run the gate over a shared component release and its `.wasm` file.
///
/// The file is validated exactly as an app's own component is (allowed
/// imports only) and must be the one the release describes: its BLAKE3
/// digest, size, imports and exports. The release's own fields follow the
/// component rules ([`crate::components::ComponentRelease::problems`]).
/// `require_provenance` refuses a release without GitHub provenance, as
/// App Hub does; a development check may turn it off. With `previous`, an
/// authenticated catalog, the version must be new, the id no app's, and
/// the publisher the one on record. Reviewers see what the component
/// reaches in an app.
pub fn check_component(
    release: &crate::components::ComponentRelease,
    wasm: &[u8],
    require_provenance: bool,
    previous: Option<&Catalog>,
) -> Result<ComponentReport, String> {
    let c = &release.component;
    let digest = blake3::hash(wasm).to_hex().to_string();
    let mut findings = Vec::new();
    for problem in release.problems() {
        findings.push(Finding::refuse("component", problem));
    }
    let file = format!("{}-{}.wasm", c.id, c.version);
    if wasm.len() as u64 > crate::components::MAX_COMPONENT_BYTES {
        findings.push(Finding::refuse(
            "size",
            format!("the component is {} bytes, over the {} ceiling", wasm.len(), crate::components::MAX_COMPONENT_BYTES),
        ));
    }
    if digest != c.wasm_blake3 {
        findings.push(Finding::refuse("digest", format!("the file hashes to {digest}, the release says {}", c.wasm_blake3)));
    }
    if wasm.len() as u64 != c.bytes {
        findings.push(Finding::refuse("digest", format!("the file is {} bytes, the release says {}", wasm.len(), c.bytes)));
    }
    let admissible = if crate::functions::is_component(wasm) {
        crate::functions::admissible_component(wasm)
    } else {
        Err("not a WebAssembly component".into())
    };
    match &admissible {
        Err(e) => findings.push(Finding::at("contents-invalid", file.clone(), e.clone())),
        Ok(info) => {
            if info.imports != c.imports || info.exports != c.exports {
                findings.push(Finding::refuse(
                    "component",
                    "the release's imports or exports are not the file's; prepare the release again from this file",
                ));
            }
        }
    }
    match &c.integrity.github {
        Some(github) => {
            if let Err(e) = release.subject_bytes().and_then(|bytes| crate::github_publisher::verify_component(github, &bytes)) {
                findings.push(Finding::refuse("publisher-signature", e));
            }
        }
        None if require_provenance => {
            findings.push(Finding::refuse("publisher-signature", "App Hub accepts only GitHub-attested component releases"));
        }
        None => findings.push(Finding::warn("publisher-signature", "unsigned: accountability rests on the hub alone")),
    }
    if let Some(catalog) = previous {
        if catalog.entries.iter().any(|e| e.app_id() == c.id) {
            findings.push(Finding::refuse("identity", format!("{} is an app's id in the catalog; a component needs an id of its own", c.id)));
        }
        let continuity = crate::components::ComponentPublishers::from_catalog(catalog).and_then(|registry| registry.authorize(release));
        if let Err(e) = continuity {
            let check = if e.contains("is already published") { "version" } else { "continuity" };
            findings.push(Finding::refuse(check, e));
        }
    }
    if let Ok(info) = &admissible {
        // The gate checks each app that names this component against that
        // app's own grants (`net` for wasi:http, `storage` for files).
        findings.push(Finding::warn_at("functions", file, format!("{} {} is a component that reaches {}", c.id, c.version, info.reach())));
    }
    Ok(ComponentReport {
        id: c.id.clone(),
        version: c.version.clone(),
        digest,
        findings,
        admitted_release: serde_json::to_vec(release).map_err(|e| e.to_string())?,
    })
}

/// Build the catalog entry for a component release the gate passed.
#[allow(clippy::too_many_arguments)]
pub fn component_entry_for(
    release: &crate::components::ComponentRelease,
    wasm: &[u8],
    report: &ComponentReport,
    publisher: &str,
    repository: &str,
    commit: &str,
    admitted: &str,
) -> Result<crate::components::ComponentEntry, String> {
    if !report.passed() {
        return Err("cannot make an entry from a refused gate report".into());
    }
    if serde_json::to_vec(release).map_err(|e| e.to_string())? != report.admitted_release
        || blake3::hash(wasm).to_hex().as_str() != report.digest
    {
        return Err("the release or its file changed after the gate; check it again".into());
    }
    let c = &release.component;
    match &c.integrity.github {
        Some(github) => {
            if publisher != format!("github:{}", github.repository_id) {
                return Err("GitHub publisher must be github: followed by its authenticated repository id".into());
            }
            if repository != github.repository_url() || commit != github.commit {
                return Err("entry source does not match authenticated publisher repository and commit".into());
            }
        }
        None if publisher.is_empty() => return Err("an entry needs a publisher".into()),
        None => {}
    }
    Ok(crate::components::ComponentEntry {
        component: c.clone(),
        listing: release.listing.clone(),
        artifact: crate::components::artifact_path(&c.id, &c.version),
        publisher: publisher.to_string(),
        source: crate::index::Source { repository: repository.to_string(), commit: commit.to_string() },
        status: crate::index::Status::Offered,
        admitted: admitted.to_string(),
    })
}

/// Every file in the bundle except the manifest, relative to its root.
fn list_files(root: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err(format!("{}: a bundle may not hold a symlink", path.display()));
            }
            if kind.is_dir() {
                walk(root, &path, out)?;
            } else {
                let relative = path.strip_prefix(root).map_err(|e| e.to_string())?.to_path_buf();
                if relative != Path::new(octosense_app_policy::MANIFEST_FILE) {
                    out.push(relative);
                }
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

/// Anything in the bundle's text that reaches outside it: an absolute URL, or
/// a path that climbs out. Cards name their artwork in text, so this is a
/// textual check by necessity; it is a gate, not the runtime's enforcement.
fn external_references(root: &Path, manifest: &AppManifest) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    for file in list_files(root)? {
        // The two metadata files carry URLs on purpose (a support page, a
        // privacy policy); nothing loads them. Their own asset paths are
        // checked by the listing rules.
        if file == Path::new(LISTING_FILE) || file == Path::new(octosense_app_policy::MANIFEST_FILE) {
            continue;
        }
        let extension = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let text = match std::fs::read_to_string(root.join(&file)) {
            Ok(text) => text,
            Err(_) => continue, // not valid text: the extension check already covers it
        };
        // A script, and the agent's own files (instructions, skills, the
        // tool manifest), may name the hosts the app declares: the agent's
        // network is the app's network. Nothing else may reach out.
        if extension == "splash" || octosense_app_policy::agent::is_agent_file(&file, manifest) {
            found.extend(script_references(&file, &text, manifest));
            continue;
        }
        // Plain documentation (including a bundled font's required license)
        // does not load its links. Agent guidance was checked above; executable
        // card data and SVG resources remain subject to their normal checks.
        // This never adds a network grant to the app's resolved policy.
        if !matches!(extension.as_str(), "card" | "json" | "l0" | "octoscript") {
            continue;
        }
        for needle in ["http://", "https://", "file://", "../"] {
            if let Some(at) = text.find(needle) {
                let snippet: String = text[at..].chars().take(60).collect();
                found.push(format!("{} contains {}", file.display(), snippet.replace('\n', " ")));
                break;
            }
        }
    }
    Ok(found)
}

/// Check unsafe source references independently of usage declarations.
/// Plain `http://`, `file://` and paths out of the bundle remain admission
/// refusals. HTTPS destinations need no matching disclosure to be admitted.
fn script_references(file: &Path, text: &str, _manifest: &AppManifest) -> Vec<String> {
    let mut found = Vec::new();
    for needle in ["http://", "file://", "../"] {
        if let Some(at) = text.find(needle) {
            let snippet: String = text[at..].chars().take(60).collect();
            found.push(format!("{} contains {}", file.display(), snippet.replace('\n', " ")));
        }
    }
    found.sort();
    found.dedup();
    found
}

/// Password and one-time-code fields declared in the bundle's scripts.
fn secret_fields(root: &Path) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    for file in list_files(root)? {
        let extension = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if !matches!(extension.as_str(), "card" | "l0" | "octoscript" | "splash") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(root.join(&file)) else { continue };
        let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        for needle in ["is_password:true", "TextInputContentType.Password", "TextInputContentType.NewPassword", "TextInputContentType.OneTimeCode"] {
            if compact.contains(needle) {
                found.push(format!("{} declares {needle}", file.display()));
            }
        }
    }
    Ok(found)
}

/// The splash runtime's `fs` methods, all of which need the app's storage.
const FS_METHODS: &[&str] = &["read", "read_bytes", "write", "append", "exists", "list", "mkdir", "remove"];

/// Incomplete storage disclosures for review. The app still receives its
/// private jail and quota; this warning does not deny execution.
fn storage_warnings(root: &Path, manifest: &AppManifest) -> Result<Vec<String>, String> {
    let has = |capability: &str| manifest.capabilities.iter().any(|c| c == capability);
    if has("storage") {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for file in list_files(root)? {
        if file.extension().and_then(|e| e.to_str()) != Some("splash") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(root.join(&file)) else { continue };
        let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let calls: Vec<String> = FS_METHODS.iter().filter(|m| compact.contains(&format!("fs.{m}("))).map(|m| format!("fs.{m}")).collect();
        if !calls.is_empty() {
            found.push(format!("{} calls {}, without a storage usage disclosure; its jail and quota still apply", file.display(), calls.join(", ")));
        }
    }
    if has("camera") {
        found.push("camera captures use the app's storage; add storage to the usage disclosures".to_string());
    }
    Ok(found)
}

/// Build the index entry for a bundle the gate passed.
#[allow(clippy::too_many_arguments)]
pub fn entry_for(
    bundle: &Path,
    report: &GateReport,
    publisher: &str,
    publisher_key: &str,
    repository: &str,
    commit: &str,
    admitted: &str,
) -> Result<Entry, String> {
    if !report.passed() {
        return Err("cannot make an entry from a refused gate report".into());
    }
    let manifest_json = std::fs::read_to_string(bundle.join(octosense_app_policy::MANIFEST_FILE)).map_err(|e| e.to_string())?;
    let manifest = AppManifest::parse(&manifest_json)?;
    if manifest.id.starts_with("os.") {
        return Err("system development reports cannot become store entries".into());
    }
    // The entry is what the gate checked, byte for byte, or nothing.
    if serde_json::to_vec(&manifest).map_err(|e| e.to_string())? != report.admitted_manifest
        || manifest.id != report.app_id
        || manifest.version != report.version
        || digest_dir(bundle)? != report.digest
    {
        return Err("the bundle or its manifest changed after the gate; check it again".into());
    }
    match &manifest.integrity.signature {
        Some(signature) => {
            if publisher.is_empty() || publisher != signature.key_id {
                return Err(format!("the publisher must be the manifest's signer, {:?}", signature.key_id));
            }
            crate::PublisherKeys::new()
                .with(publisher, publisher_key)
                .verify(publisher, &signature.value, &manifest.signing_bytes()?)?;
        }
        None if publisher.is_empty() => return Err("an entry needs a publisher".into()),
        None if !publisher_key.is_empty() => return Err("an unsigned release records no publisher key".into()),
        None => {}
    }
    if let Some(github) = &manifest.integrity.github {
        if publisher != format!("github:{}",github.repository_id) {
            return Err("GitHub publisher must be github: followed by its authenticated repository id".into());
        }
        crate::github_publisher::verify(github, &manifest.signing_bytes()?)?;
        if repository != github.repository_url() || commit != github.commit {
            return Err("entry source does not match authenticated publisher repository and commit".into());
        }
    }
    let listing = std::fs::read_to_string(bundle.join(LISTING_FILE)).ok().and_then(|t| Listing::parse(&t).ok());
    let mut tools = octosense_app_policy::agent::read_tools(bundle)?.map(|t| t.tools).unwrap_or_default();
    // Catalog tools describe permissions; dispatch reads the digest-pinned
    // bundle instead. Older stores strictly parse ToolSpec, so including this
    // new routing field here would make them reject the entire catalog.
    // Keep every permission field, and leave the signed bundle untouched.
    for tool in &mut tools {
        tool.host_method = None;
    }
    Ok(Entry {
        artifact: format!("artifacts/{}-{}.bundle", report.app_id, report.version),
        manifest,
        listing,
        tools,
        publisher: publisher.to_string(),
        publisher_key: publisher_key.to_string(),
        source: crate::index::Source { repository: repository.to_string(), commit: commit.to_string() },
        status: crate::index::Status::Offered,
        admitted: admitted.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_app_reaches_only_the_hosts_it_declares() {
        let dir = std::env::temp_dir().join(format!("gate-script-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("main.splash"),
            "fn load(){ net.http_request({url: \"https://api.example.com/v1\"}, fn(r){}) }\nlet more = \"https://tracker.example.net/p\"\nImage{src: \"{{assets}}/a.png\"}",
        )
        .unwrap();
        let manifest = |caps: &[&str], hosts: &[&str]| {
            AppManifest::parse(&serde_json::json!({
                "schema": 1, "id": "dev.example.app", "version": "1.0.0", "name": "App",
                "integrity": {"bundle_blake3": ""}, "capabilities": caps, "network": {"hosts": hosts}
            }).to_string()).unwrap()
        };
        let refused = external_references(&dir, &manifest(&["net"], &["api.example.com"])).unwrap();
        assert!(refused.is_empty(), "unlisted public hosts are declarations, not refusals: {refused:?}");
        assert!(external_references(&dir, &manifest(&["net"], &["api.example.com", "tracker.example.net"])).unwrap().is_empty());
        assert!(external_references(&dir, &manifest(&["net", "images"], &["api.example.com"])).unwrap().is_empty(), "images reaches any public host");
        std::fs::write(dir.join("main.splash"), "let x = \"http://api.example.com\"").unwrap();
        assert!(!external_references(&dir, &manifest(&["net", "web"], &["api.example.com"])).unwrap().is_empty(), "plain http is never allowed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_store_bundle_may_not_take_a_system_app_id() {
        let dir = std::env::temp_dir().join(format!("gate-os-id-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.splash"), "Label{text: \"hi\"}").unwrap();
        let write = |id: &str| {
            std::fs::write(
                dir.join(octosense_app_policy::MANIFEST_FILE),
                serde_json::json!({"schema": 1, "id": id, "version": "1.0.0", "name": "App", "integrity": {"bundle_blake3": ""}}).to_string(),
            )
            .unwrap()
        };
        let limits = HostLimits::default().with_require_signature(false);
        let identity = |dir: &Path| {
            check_bundle(dir, &limits, &octosense_app_policy::RefuseAllSignatures, None)
                .unwrap()
                .findings
                .into_iter()
                .filter(|f| f.check == "identity")
                .count()
        };
        write("os.mail");
        assert_eq!(identity(&dir), 1, "os. ids belong to system apps");
        write("dev.example.mail");
        assert_eq!(identity(&dir), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bundle_that_asks_for_a_password_is_refused() {
        let dir = std::env::temp_dir().join(format!("gate-secrets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cards")).unwrap();
        std::fs::write(dir.join("main.card"), "View{ name := TextInput{empty_text: \"Name\"} }").unwrap();
        std::fs::write(dir.join("cards/login.card"), "View{ pw := TextInput{ is_password : true } }").unwrap();
        std::fs::write(dir.join("cards/otp.card"), "TextInput{content_type: TextInputContentType.OneTimeCode}").unwrap();
        std::fs::write(dir.join("notes.md"), "Set is_password: true in your own app, not here.").unwrap();
        let mut found = secret_fields(&dir).unwrap();
        found.sort();
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].contains("login.card") && found[0].contains("is_password"));
        assert!(found[1].contains("otp.card") && found[1].contains("OneTimeCode"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bundle_that_needs_storage_without_asking_is_warned() {
        let dir = std::env::temp_dir().join(format!("gate-storage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.splash"), "fn save(text){ fs.write(\"notes.txt\", text) }\nfn load(){ fs.read(\"notes.txt\") }").unwrap();
        let write = |caps: &[&str]| {
            std::fs::write(
                dir.join(octosense_app_policy::MANIFEST_FILE),
                serde_json::json!({"schema": 1, "id": "dev.example.jotter", "version": "1.0.0", "name": "Jotter", "integrity": {"bundle_blake3": ""}, "capabilities": caps}).to_string(),
            )
            .unwrap()
        };
        let limits = HostLimits::default().with_require_signature(false);
        let warnings = |dir: &Path| -> Vec<String> {
            let report = check_bundle(dir, &limits, &octosense_app_policy::RefuseAllSignatures, None).unwrap();
            let storage: Vec<Finding> = report.findings.into_iter().filter(|f| f.check == "storage").collect();
            assert!(storage.iter().all(|f| f.severity == Severity::Warning), "{storage:?}");
            storage.into_iter().map(|f| f.detail).collect()
        };
        write(&[]);
        let found = warnings(&dir);
        assert_eq!(found.len(), 1, "one warning per script: {found:?}");
        assert!(found[0].contains("main.splash") && found[0].contains("fs.write"), "{found:?}");
        let grants = |dir: &Path| check_bundle(dir, &limits, &octosense_app_policy::RefuseAllSignatures, None).unwrap().render();
        assert!(grants(&dir).contains("storage 16777216 bytes"), "{}", grants(&dir));
        write(&["storage"]);
        assert!(warnings(&dir).is_empty());
        assert!(grants(&dir).contains("storage 16777216 bytes"), "{}", grants(&dir));
        write(&["camera", "storage"]);
        assert!(warnings(&dir).is_empty());
        std::fs::write(dir.join("main.splash"), "Label{text: \"no files here\"}").unwrap();
        write(&["camera"]);
        let found = warnings(&dir);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("camera") && found[0].contains("capture"), "{found:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
