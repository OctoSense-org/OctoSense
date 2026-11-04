//! Shared components (App Hub ADR 0003): reviewed WebAssembly components
//! that App Hub publishes on their own, and that apps pin by id, exact
//! version and digest, like an npm package locked to one tarball.
//!
//! - [`ComponentRelease`] is what a publisher releases: the component's
//!   manifest ([`ComponentManifest`]) and its [`ComponentListing`]. Its
//!   [`subject_bytes`](ComponentRelease::subject_bytes) are what GitHub
//!   provenance attests, as an app's canonical manifest is.
//! - [`ComponentEntry`] is one component version in the catalog: the
//!   release plus the Hub's record (artifact, publisher, source, status).
//!   [`check_catalog`] is what every reader of a catalog checks about them.
//! - The device keeps each component once per digest, read-only, at
//!   `<apps root>/.components/<blake3>.wasm` ([`keep`], [`collect`]); a
//!   system app ships its own at `components/<blake3>.wasm` in its bundle
//!   ([`bundled`]). Every read verifies the BLAKE3 digest ([`Resolved`]).
//!
//! What a component may reach is its app's: the gate checks that an app
//! grants what each of its components imports, and a host runs each app's
//! instance under that app's grants.
use crate::github_publisher::GithubBinding;
use crate::index::{Catalog, Source, Status};
use octosense_app_policy::manifest::GithubPublisher;
use octosense_app_policy::{AppManifest, Publisher};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The shared component store under the apps root. A name starting with `.`
/// is never an app id, so this is no app's storage.
pub const COMPONENTS_DIR: &str = ".components";

/// Where a system app's bundle carries the components its manifest names:
/// `components/<blake3>.wasm`.
pub const BUNDLED_DIR: &str = "components";

/// The file name GitHub provenance attests for a component release.
pub const COMPONENT_SUBJECT: &str = "octosense-component.json";

/// The one schema of [`ComponentManifest`].
pub const COMPONENT_SCHEMA: u32 = 1;

/// The largest component file: a bundle's ceiling.
pub const MAX_COMPONENT_BYTES: u64 = crate::gate::MAX_BUNDLE_BYTES;

/// The largest release document (manifest, listing and a 48 KiB proof).
pub const MAX_RELEASE_BYTES: u64 = 512 * 1024;

/// The most component versions one catalog holds.
pub const MAX_CATALOG_COMPONENTS: usize = 4096;

/// A component as its publisher describes it and the Hub reviews it. The
/// file's digest, size, imports and exports are derived from the file
/// (`hub component-prepare`) and checked against it again at every review.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentManifest {
    /// [`COMPONENT_SCHEMA`].
    pub schema: u32,
    /// The component's id: an app id's rules, never `os.`, and no app's id.
    pub id: String,
    /// One exact semantic version.
    pub version: String,
    /// What a person calls it.
    pub name: String,
    /// Lowercase hex BLAKE3 of the `.wasm` file.
    pub wasm_blake3: String,
    /// The file's size in bytes.
    pub bytes: u64,
    /// Its top-level imports, as `hub component-info` lists them.
    pub imports: Vec<String>,
    /// Its exported functions, with WIT parameters and results.
    pub exports: Vec<crate::functions::Export>,
    /// Who publishes it, as an app listing's publisher.
    pub publisher: Publisher,
    /// Its SPDX licence expression.
    pub license: String,
    /// Its GitHub provenance, as an app manifest's `integrity.github`.
    pub integrity: ComponentIntegrity,
}

/// A component's provenance. Empty only in a development catalog: App Hub
/// accepts only GitHub-attested releases (ADR 0002).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentIntegrity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GithubPublisher>,
}

/// What a store may show about a component.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentListing {
    /// One line under the name, at most 80 characters.
    #[serde(default)]
    pub subtitle: String,
    /// What it does, for a developer choosing it and a reviewer reading it.
    pub description: String,
    /// At most 10.
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// A component release: what `hub component-pack` writes as
/// `<id>-<version>.component.json` and a publisher submits beside the
/// `.wasm` file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentRelease {
    pub component: ComponentManifest,
    pub listing: ComponentListing,
}

/// One component version in the catalog: the reviewed release and the Hub's
/// record of it, as an app's [`crate::Entry`] holds its manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentEntry {
    pub component: ComponentManifest,
    pub listing: ComponentListing,
    /// The Hub's copy of the file, relative to the catalog:
    /// `artifacts/<id>-<version>.wasm`.
    pub artifact: String,
    /// `github:<repository_id>`, as for an app.
    pub publisher: String,
    /// Where the source lives. Never fetched.
    pub source: Source,
    /// Offered, or withdrawn with a reason a person reads.
    pub status: Status,
    /// When the Hub admitted it, as an ISO 8601 date.
    pub admitted: String,
}

impl ComponentEntry {
    pub fn id(&self) -> &str {
        &self.component.id
    }

    pub fn version(&self) -> &str {
        &self.component.version
    }

    /// The release the publisher submitted.
    pub fn release(&self) -> ComponentRelease {
        ComponentRelease { component: self.component.clone(), listing: self.listing.clone() }
    }
}

/// A component's catalog artifact path.
pub fn artifact_path(id: &str, version: &str) -> String {
    format!("artifacts/{id}-{version}.wasm")
}

/// A component's index file in a catalog candidate.
pub fn index_path(id: &str, version: &str) -> String {
    format!("index/components/{id}-{version}.json")
}

impl ComponentRelease {
    /// The bytes GitHub provenance attests: the release without its
    /// attestation, in canonical JSON (sorted keys, no whitespace).
    pub fn subject_bytes(&self) -> Result<Vec<u8>, String> {
        let mut bare = self.clone();
        if let Some(github) = &mut bare.component.integrity.github {
            github.attestation = None;
        }
        let value = serde_json::to_value(&bare).map_err(|e| e.to_string())?;
        Ok(crate::index::canonical(&value).into_bytes())
    }

    /// `org.example.markdown 1.2.0`.
    pub fn describe(&self) -> String {
        format!("{} {}", self.component.id, self.component.version)
    }

    /// Read a release document, within [`MAX_RELEASE_BYTES`].
    pub fn read(path: &Path) -> Result<Self, String> {
        let bytes = crate::admission::read_bounded(path, MAX_RELEASE_BYTES)?;
        serde_json::from_slice(&bytes).map_err(|e| format!("{}: not a component release: {e}", path.display()))
    }

    /// Fill in what the file says (digest, size, imports, exports) for the
    /// publisher's description in `draft`. Refuses a file that is not an
    /// admissible component, and a description that breaks a rule.
    pub fn from_draft(draft: ComponentDraft, wasm: &[u8]) -> Result<Self, String> {
        if wasm.len() as u64 > MAX_COMPONENT_BYTES {
            return Err(format!("the component is {} bytes, over the {MAX_COMPONENT_BYTES} ceiling", wasm.len()));
        }
        if !crate::functions::is_component(wasm) {
            return Err("not a WebAssembly component: build it with cargo build --target wasm32-wasip2".into());
        }
        let info = crate::functions::admissible_component(wasm)?;
        let ComponentDraft { component: d, listing } = draft;
        let release = ComponentRelease {
            component: ComponentManifest {
                schema: d.schema,
                id: d.id,
                version: d.version,
                name: d.name,
                wasm_blake3: blake3::hash(wasm).to_hex().to_string(),
                bytes: wasm.len() as u64,
                imports: info.imports,
                exports: info.exports,
                publisher: d.publisher,
                license: d.license,
                integrity: ComponentIntegrity::default(),
            },
            listing,
        };
        let problems = release.problems();
        if let Some(first) = problems.first() {
            return Err(first.clone());
        }
        Ok(release)
    }

    /// Every rule the release's own fields break, in words a publisher can
    /// act on. The file and the catalog are checked by the gate.
    pub fn problems(&self) -> Vec<String> {
        let mut found = Vec::new();
        let c = &self.component;
        if c.schema != COMPONENT_SCHEMA {
            found.push(format!("component schema {} is not {COMPONENT_SCHEMA}", c.schema));
        }
        if let Err(e) = octosense_app_policy::manifest::check_component_id(&c.id) {
            found.push(e);
        }
        if !octosense_app_policy::manifest::is_exact_version(&c.version) {
            found.push(format!("component version {:?} must be one exact semantic version, major.minor.patch", c.version));
        }
        if c.name.trim().is_empty() || c.name.chars().count() > 64 {
            found.push("component name must be 1 to 64 characters".into());
        }
        if !is_digest(&c.wasm_blake3) {
            found.push("wasm_blake3 must be 64 lowercase hex characters".into());
        }
        if c.bytes == 0 || c.bytes > MAX_COMPONENT_BYTES {
            found.push(format!("bytes must be 1 to {MAX_COMPONENT_BYTES}"));
        }
        let license_ok = !c.license.trim().is_empty()
            && c.license.len() <= 128
            && c.license.chars().all(|ch| ch.is_ascii_alphanumeric() || " .+-:()".contains(ch));
        if !license_ok {
            found.push("license must be an SPDX licence expression, such as \"MIT OR Apache-2.0\"".into());
        }
        if c.publisher.name.trim().is_empty() {
            found.push("publisher name is empty".into());
        }
        if c.publisher.support.trim().is_empty() {
            found.push("publisher support (a URL or an email) is empty".into());
        }
        if !c.publisher.privacy_policy_url.starts_with("https://") {
            found.push("publisher privacy policy must be an https URL".into());
        }
        let l = &self.listing;
        if l.description.trim().is_empty() {
            found.push("listing description is empty".into());
        }
        if l.description.chars().count() > octosense_app_policy::listing::MAX_DESCRIPTION {
            found.push(format!("listing description is over {} characters", octosense_app_policy::listing::MAX_DESCRIPTION));
        }
        if l.subtitle.chars().count() > octosense_app_policy::listing::MAX_SUBTITLE {
            found.push(format!("listing subtitle is over {} characters", octosense_app_policy::listing::MAX_SUBTITLE));
        }
        if l.keywords.len() > octosense_app_policy::listing::MAX_KEYWORDS {
            found.push(format!("listing has more than {} keywords", octosense_app_policy::listing::MAX_KEYWORDS));
        }
        found
    }

    /// The component's imports as [`crate::functions::ComponentInfo`], for
    /// its reach line and grant checks.
    pub fn info(&self) -> crate::functions::ComponentInfo {
        info_of(&self.component)
    }
}

pub(crate) fn info_of(component: &ComponentManifest) -> crate::functions::ComponentInfo {
    // A release names its imports and exports; its crate list stays in its file.
    crate::functions::ComponentInfo { imports: component.imports.clone(), exports: component.exports.clone(), crates: None }
}

/// What a publisher writes before `hub component-prepare` fills in the rest:
/// `{"component": {schema, id, version, name, publisher, license},
/// "listing": {subtitle, description, keywords}}`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentDraft {
    pub component: DraftManifest,
    pub listing: ComponentListing,
}

/// The part of a [`ComponentManifest`] its publisher writes.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftManifest {
    pub schema: u32,
    pub id: String,
    pub version: String,
    pub name: String,
    pub publisher: Publisher,
    pub license: String,
}

/// Lowercase hex, 64 characters: a BLAKE3 digest as the Hub writes it.
pub fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

// ---- the catalog ----------------------------------------------------------

/// Who owns each component id, from the catalog's history: the GitHub
/// authority on record, or an unsigned development history, and the newest
/// GitHub version. Every entry counts, withdrawn ones too.
#[derive(Default)]
pub struct ComponentPublishers {
    github: BTreeMap<String, GithubBinding>,
    unsigned: BTreeSet<String>,
    versions: BTreeMap<String, String>,
    published: BTreeSet<(String, String)>,
}

impl ComponentPublishers {
    /// The history `catalog` records, every entry checked: see
    /// [`check_catalog`].
    pub fn from_catalog(catalog: &Catalog) -> Result<Self, String> {
        if catalog.components.len() > MAX_CATALOG_COMPONENTS {
            return Err("catalog exceeds component limit".into());
        }
        let apps: BTreeSet<&str> = catalog.entries.iter().map(|e| e.app_id()).collect();
        let mut registry = Self::default();
        for entry in &catalog.components {
            if apps.contains(entry.id()) {
                return Err(format!("{} is both an app and a component in the catalog", entry.id()));
            }
            registry.record(entry)?;
        }
        Ok(registry)
    }

    fn record(&mut self, entry: &ComponentEntry) -> Result<(), String> {
        let c = &entry.component;
        let key = (c.id.clone(), c.version.clone());
        if self.published.contains(&key) {
            return Err(format!("catalog repeats component {} {}", c.id, c.version));
        }
        if c.schema != COMPONENT_SCHEMA {
            return Err(format!("component {} {} has schema {}", c.id, c.version, c.schema));
        }
        octosense_app_policy::manifest::check_component_id(&c.id)?;
        if !octosense_app_policy::manifest::is_exact_version(&c.version) || !is_digest(&c.wasm_blake3) {
            return Err(format!("component {} {} has no exact version or digest", c.id, c.version));
        }
        if entry.artifact != artifact_path(&c.id, &c.version) {
            return Err(format!("component {} {} has a noncanonical artifact path", c.id, c.version));
        }
        if entry.publisher.is_empty() {
            return Err(format!("component {} has no publisher on record", c.id));
        }
        self.authorize(&entry.release())?;
        match &c.integrity.github {
            Some(github) => {
                crate::github_publisher::verify_component(github, &entry.release().subject_bytes()?)?;
                if entry.publisher != format!("github:{}", github.repository_id)
                    || entry.source.repository != github.repository_url()
                    || entry.source.commit != github.commit
                {
                    return Err(format!("component {}'s publisher or source differs from its authenticated provenance", c.id));
                }
                self.github.insert(c.id.clone(), GithubBinding::from(github));
                self.versions.insert(c.id.clone(), c.version.clone());
            }
            None => {
                self.unsigned.insert(c.id.clone());
            }
        }
        self.published.insert(key);
        Ok(())
    }

    /// Whether `release` may follow this history: a new version, from the
    /// same GitHub repository, owner and workflow as the id's earlier
    /// versions, with a higher semantic version. A GitHub-owned component
    /// never goes unsigned, and GitHub provenance never takes over an id
    /// with an unsigned history.
    pub fn authorize(&self, release: &ComponentRelease) -> Result<(), String> {
        let c = &release.component;
        if self.published.contains(&(c.id.clone(), c.version.clone())) {
            return Err(format!("version {} of component {} is already published; publish a new version", c.version, c.id));
        }
        match &c.integrity.github {
            Some(github) => {
                if let Some(binding) = self.github.get(&c.id) {
                    if binding != &GithubBinding::from(github) {
                        return Err("GitHub publisher repository, owner or workflow changed".into());
                    }
                } else if self.unsigned.contains(&c.id) {
                    return Err("an unsigned component history cannot be adopted by GitHub provenance".into());
                }
                crate::publishers::check_version_advance(&c.version, self.versions.get(&c.id).map(String::as_str))
            }
            None if self.github.contains_key(&c.id) => {
                Err("a GitHub-owned component cannot downgrade to an unsigned release".into())
            }
            None => Ok(()),
        }
    }

    /// Whether the catalog holds any version of `id`.
    pub fn knows(&self, id: &str) -> bool {
        self.github.contains_key(id) || self.unsigned.contains(id)
    }
}

/// What every reader of a catalog checks about its components, beside the
/// catalog's own signature or proof: each `(id, version)` once, a component
/// id that no app uses, exact versions and digests, canonical artifact
/// paths, and per id one GitHub authority whose proofs verify and whose
/// versions advance.
pub fn check_catalog(catalog: &Catalog) -> Result<(), String> {
    ComponentPublishers::from_catalog(catalog).map(|_| ())
}

/// The catalog's entry for exactly this component version.
pub fn find<'a>(catalog: &'a Catalog, id: &str, version: &str) -> Option<&'a ComponentEntry> {
    catalog.components.iter().find(|c| c.id() == id && c.version() == version)
}

/// Resolve one dependency against a verified catalog: the entry must exist,
/// be offered and hash to the pinned digest.
pub fn resolve<'a>(catalog: &'a Catalog, dependency: &octosense_app_policy::ComponentDependency) -> Result<&'a ComponentEntry, String> {
    let entry = find(catalog, &dependency.id, &dependency.version)
        .ok_or_else(|| format!("component {} is not in the catalog", dependency.describe()))?;
    if let Status::Withdrawn(reason) = &entry.status {
        return Err(format!("component {} was withdrawn: {reason}", dependency.describe()));
    }
    if entry.component.wasm_blake3 != dependency.blake3 {
        return Err(format!(
            "component {} pins blake3 {}, but the catalog's file hashes to {}",
            dependency.describe(),
            dependency.blake3,
            entry.component.wasm_blake3
        ));
    }
    Ok(entry)
}

/// Compatibility hook retained for callers. Capability declarations do not
/// deny component imports. Admission still validates supported imports,
/// exact digests, provenance and ABI; execution retains the app jail/quotas
/// and host-service consent, account and inter-app sharing checks.
pub fn missing_grant(_manifest: &AppManifest, _info: &crate::functions::ComponentInfo) -> Option<&'static str> {
    None
}

// ---- the device's store -----------------------------------------------------

/// A component an app pins, as a host loads it: the app's name for it, its
/// id, version and digest, and the verified file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub alias: String,
    pub id: String,
    pub version: String,
    pub blake3: String,
    pub path: PathBuf,
}

impl Resolved {
    fn of(dependency: &octosense_app_policy::ComponentDependency, path: PathBuf) -> Self {
        Resolved {
            alias: dependency.alias.clone(),
            id: dependency.id.clone(),
            version: dependency.version.clone(),
            blake3: dependency.blake3.clone(),
            path,
        }
    }
}

/// The shared store: `<apps root>/.components`.
pub fn store_dir(app_data_root: &Path) -> PathBuf {
    app_data_root.join(COMPONENTS_DIR)
}

/// Where the store keeps the component with digest `blake3`.
pub fn stored_path(app_data_root: &Path, blake3: &str) -> PathBuf {
    store_dir(app_data_root).join(format!("{blake3}.wasm"))
}

/// Check that `path` is a regular file holding exactly the bytes `blake3`
/// names.
pub fn verify_file(path: &Path, blake3: &str) -> Result<(), String> {
    let bytes = crate::admission::read_bounded(path, MAX_COMPONENT_BYTES)?;
    let digest = blake3::hash(&bytes).to_hex();
    if digest.as_str() != blake3 {
        return Err(format!("{} hashes to {digest}, not {blake3}", path.display()));
    }
    Ok(())
}

/// Keep `bytes` in the shared store as the component `blake3` names, once
/// per digest: refused unless they hash to it, written beside the final
/// name, made read-only and renamed into place. A file already there that
/// verifies is kept; one that does not is replaced.
pub fn keep(app_data_root: &Path, blake3: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    if !is_digest(blake3) {
        return Err(format!("{blake3:?} is not a component digest"));
    }
    let digest = blake3::hash(bytes).to_hex();
    if digest.as_str() != blake3 {
        return Err(format!("the downloaded component hashes to {digest}, the catalog says {blake3}"));
    }
    let dir = store_dir(app_data_root);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create the component store: {e}"))?;
    let target = stored_path(app_data_root, blake3);
    if verify_file(&target, blake3).is_ok() {
        return Ok(target);
    }
    let temporary = dir.join(format!(".{blake3}.{}.tmp", random_suffix()?));
    let written = (|| {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|e| format!("cannot write the component store: {e}"))?;
        file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| format!("cannot write the component store: {e}"))?;
        drop(file);
        set_readonly(&temporary, true)?;
        if std::fs::symlink_metadata(&target).is_ok() {
            remove_entry(&target)?;
        }
        std::fs::rename(&temporary, &target).map_err(|e| format!("cannot keep the component: {e}"))
    })();
    if let Err(error) = written {
        let _ = set_readonly(&temporary, false);
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(target)
}

/// The digests installed apps pin: every manifest under `.bundles/<id>/`,
/// the installed `bundle/` and an update's `.bundle-next/` and
/// `.bundle-previous/`. Refuses when a manifest there cannot be read, so a
/// collection never removes what an unreadable app might pin.
pub fn pinned(app_data_root: &Path) -> Result<BTreeSet<String>, String> {
    let mut digests = BTreeSet::new();
    let installs = app_data_root.join(crate::client::INSTALLS_DIR);
    let entries = match std::fs::read_dir(&installs) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(digests),
        Err(e) => return Err(format!("cannot read the installed apps: {e}")),
    };
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            continue;
        }
        for copy in ["bundle", ".bundle-next", ".bundle-previous"] {
            let manifest = entry.path().join(copy).join(octosense_app_policy::MANIFEST_FILE);
            match std::fs::symlink_metadata(&manifest) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                _ => {}
            }
            let text = crate::admission::read_text(&manifest, crate::admission::MAX_MANIFEST_BYTES)?;
            let parsed = AppManifest::parse(&text).map_err(|e| format!("{}: {e}", manifest.display()))?;
            digests.extend(parsed.components.into_iter().map(|c| c.blake3));
        }
    }
    Ok(digests)
}

/// Remove every stored component that no installed app pins ([`pinned`]),
/// and return their digests. Callers serialize this with installs, as the
/// shell's App Hub does under its catalog lock: a component written for an
/// install that has not finished is pinned by no installed app yet.
pub fn collect(app_data_root: &Path) -> Result<Vec<String>, String> {
    let dir = store_dir(app_data_root);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read the component store: {e}")),
    };
    let pinned = pinned(app_data_root)?;
    let mut removed = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let Some(digest) = name.to_str().and_then(|n| n.strip_suffix(".wasm")) else { continue };
        if !is_digest(digest) || pinned.contains(digest) {
            continue;
        }
        remove_entry(&entry.path())?;
        removed.push(digest.to_string());
    }
    removed.sort();
    Ok(removed)
}

/// A system app's components (`components/<blake3>.wasm` in its bundle),
/// each verified against the digest its manifest pins. No catalog is
/// involved: the bundle shipped with the build.
pub fn bundled(bundle: &Path) -> Result<Vec<Resolved>, String> {
    let text = crate::admission::read_text(&bundle.join(octosense_app_policy::MANIFEST_FILE), crate::admission::MAX_MANIFEST_BYTES)?;
    let manifest = AppManifest::parse(&text)?;
    manifest
        .components
        .iter()
        .map(|dependency| {
            let path = bundle.join(BUNDLED_DIR).join(format!("{}.wasm", dependency.blake3));
            if std::fs::symlink_metadata(&path).is_err() {
                return Err(format!(
                    "component {} is missing from {}'s bundle ({BUNDLED_DIR}/{}.wasm)",
                    dependency.describe(),
                    manifest.id,
                    dependency.blake3
                ));
            }
            verify_file(&path, &dependency.blake3).map_err(|e| format!("component {}: {e}", dependency.describe()))?;
            Ok(Resolved::of(dependency, path))
        })
        .collect()
}

/// The verified store file for each of `manifest`'s components, as
/// `resolve`d entries already vouch for: present, a regular file, and
/// hashing to the pinned digest.
pub(crate) fn stored(app_data_root: &Path, manifest: &AppManifest) -> Result<Vec<Resolved>, String> {
    manifest
        .components
        .iter()
        .map(|dependency| {
            let path = stored_path(app_data_root, &dependency.blake3);
            if std::fs::symlink_metadata(&path).is_err() {
                return Err(format!(
                    "component {} is not in this device's component store; update or reinstall {}",
                    dependency.describe(),
                    manifest.id
                ));
            }
            verify_file(&path, &dependency.blake3).map_err(|e| format!("component {}: {e}; reinstall {}", dependency.describe(), manifest.id))?;
            Ok(Resolved::of(dependency, path))
        })
        .collect()
}

fn set_readonly(path: &Path, readonly: bool) -> Result<(), String> {
    let mut permissions = std::fs::metadata(path).map_err(|e| e.to_string())?.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(if readonly { 0o444 } else { 0o644 });
    }
    #[cfg(not(unix))]
    permissions.set_readonly(readonly);
    std::fs::set_permissions(path, permissions).map_err(|e| e.to_string())
}

/// Remove a store entry, which is read-only (Windows refuses to remove a
/// read-only file) and never followed if it is a link.
fn remove_entry(path: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        return std::fs::remove_dir_all(path).map_err(|e| format!("cannot remove {}: {e}", path.display()));
    }
    if meta.is_file() {
        let _ = set_readonly(path, false);
    }
    std::fs::remove_file(path).map_err(|e| format!("cannot remove {}: {e}", path.display()))
}

fn random_suffix() -> Result<String, String> {
    let mut bytes = [0u8; 8];
    rand_core::TryRngCore::try_fill_bytes(&mut rand_core::OsRng, &mut bytes).map_err(|_| "the OS has no randomness")?;
    Ok(hex::encode(bytes))
}
