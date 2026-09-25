//! Verified live catalog and a deliberately separate built-in app preview.

use octosense_app_hub::{Availability, Listing, Store};
use octosense_app_policy::HostLimits;
use octosense_appstore::source::Origin;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, TryLockError};

// Multiple Hub windows share one installation directory. Keep catalog acceptance,
// persistence and installs in order even when their workers overlap.
static CATALOG_IO: Mutex<()> = Mutex::new(());
// A verified withdrawal must remain a sequence floor even if the disk cannot
// persist it. Share the floor with existing and future workers for this root.
type CatalogFloors = BTreeMap<(PathBuf, String), u64>;
static VERIFIED_SEQUENCES: OnceLock<Mutex<CatalogFloors>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogKind {
    Live,
    Preview,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryStatus {
    Available,
    Installed,
    UpdateAvailable,
    Unavailable(String),
    BuiltIn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallConsent {
    pub app_id: String,
    pub version: String,
    canonical_entry: String,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub subtitle: String,
    pub description: String,
    pub category: String,
    pub publisher: String,
    pub version: String,
    pub release_notes: String,
    pub icon: Option<String>,
    pub screenshots: Vec<String>,
    pub permissions: Vec<String>,
    pub privacy: Vec<String>,
    pub kind: CatalogKind,
    pub status: EntryStatus,
    pub consent: Option<InstallConsent>,
}

impl Entry {
    pub fn open_target(&self) -> Option<&str> {
        matches!(self.status, EntryStatus::Installed | EntryStatus::BuiltIn)
            .then_some(self.id.as_str())
    }
}

#[derive(Clone, Debug, Default)]
pub struct CatalogSnapshot {
    pub entries: Vec<Entry>,
    pub library: Vec<Entry>,
    pub warning: Option<String>,
    pub published: Option<String>,
    pub can_install: bool,
    pub verified: bool,
}

pub struct Backend {
    root: PathBuf,
    root_key: PathBuf,
    origin: Origin,
    anchor: String,
    store: Store,
    warning: Option<String>,
    persistence_failed: bool,
}

/// Check a shell launch using local verified state, without waiting on Hub work.
pub fn try_may_open_from_environment(root: PathBuf, id: &str) -> Result<(), String> {
    let (origin, anchor) = environment_settings();
    try_may_open(root, origin, anchor, id)
}

fn try_may_open(root: PathBuf, origin: Origin, anchor: String, id: &str) -> Result<(), String> {
    let _guard = match CATALOG_IO.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => {
            return Err("App Hub is updating. Try again shortly.".into())
        }
    };
    Backend::new_unlocked(root, origin, anchor).may_open(id)
}

fn environment_settings() -> (Origin, String) {
    let origin =
        Origin::from_env().unwrap_or_else(|| Origin::parse(octosense_appstore::DEFAULT_HUB));
    let anchor = std::env::var("OCTOSENSE_HUB_ANCHOR")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| octosense_appstore::DEFAULT_ANCHOR.into());
    (origin, anchor)
}

impl Backend {
    pub fn new(root: PathBuf, origin: Origin, anchor: String) -> Self {
        let _guard = CATALOG_IO.lock().unwrap_or_else(|p| p.into_inner());
        Self::new_unlocked(root, origin, anchor)
    }

    // Caller holds CATALOG_IO. This reads local state and performs recovery;
    // neither constructor nor the shell launch gate contacts the origin.
    fn new_unlocked(root: PathBuf, origin: Origin, anchor: String) -> Self {
        let store = Store::new(&anchor, &root, HostLimits::default());
        let recovery_warning = recover_installations(&root).err();
        let mut backend = Self {
            root_key: root_identity(&root),
            root,
            origin,
            anchor,
            store,
            warning: recovery_warning,
            persistence_failed: false,
        };
        backend.load_cache();
        backend
    }

    pub fn from_environment(root: PathBuf) -> Self {
        let (origin, anchor) = environment_settings();
        Self::new(root, origin, anchor)
    }

    /// Blocking I/O: call this on the Hub worker, never on the UI thread.
    pub fn refresh(&mut self) -> CatalogSnapshot {
        let _guard = CATALOG_IO.lock().unwrap_or_else(|p| p.into_inner());
        self.refresh_catalog();
        self.snapshot()
    }

    fn load_cache(&mut self) {
        match std::fs::read_to_string(self.root.join("catalog.json")) {
            Ok(json) => {
                if let Err(error) = self
                    .store
                    .accept_catalog(&json)
                    .and_then(|()| self.remember_sequence(self.store.catalog().unwrap().sequence))
                {
                    self.warning = Some(format!("Could not accept the cached catalog: {error}"));
                } else {
                    self.persistence_failed = false;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                self.warning = Some(format!("Could not read the cached catalog: {error}"))
            }
        }
    }

    fn refresh_catalog(&mut self) {
        let recovery_warning = recover_installations(&self.root).err();
        self.warning = recovery_warning.clone();
        // Establish the durable sequence floor before accepting network bytes,
        // including when another Hub instance refreshed since this worker did.
        self.load_cache();
        match self.origin.catalog().and_then(|json| {
            let mut candidate = Store::new(&self.anchor, &self.root, HostLimits::default());
            if let Some(held) = self.store.catalog() {
                candidate
                    .accept_catalog(&serde_json::to_string(held).map_err(|e| e.to_string())?)?;
            }
            candidate.accept_catalog(&json)?;
            self.remember_sequence(candidate.catalog().unwrap().sequence)?;
            if let Err(error) = persist_catalog(&self.root, &json) {
                self.persistence_failed = true;
                return Err(error);
            }
            self.store = candidate;
            self.persistence_failed = false;
            Ok(())
        }) {
            Ok(()) => self.warning = recovery_warning,
            Err(error) => {
                let prefix = if self.store.catalog().is_some() {
                    "Showing the last verified cached catalog."
                } else {
                    "App Hub could not load a verified catalog."
                };
                self.warning = Some(format!("{prefix} {error}"));
            }
        }
    }

    pub fn snapshot(&self) -> CatalogSnapshot {
        let freshness = self.store.installs_allowed(&octosense_app_hub::today());
        let floor = self.check_sequence_floor();
        let can_install = freshness.is_ok() && floor.is_ok() && !self.persistence_failed;
        let mut warning = self.warning.clone();
        if let Err(error) = floor {
            warning = Some(match warning {
                Some(w) => format!("{w} {error}"),
                None => error,
            });
        }
        if self.store.catalog().is_some() {
            if let Err(error) = freshness {
                warning = Some(match warning {
                    Some(w) => format!("{w} {error}"),
                    None => error,
                });
            }
        }
        let entries: Vec<_> = self
            .store
            .listings()
            .into_iter()
            .map(|listing| self.present(listing, can_install))
            .collect();
        let library = octosense_appstore::installed_apps(&self.root).into_iter().map(|installed| {
            entries.iter().find(|entry| entry.id == installed.id).cloned().unwrap_or_else(|| Entry {
                icon: crate::icons::installed_icon_path(&self.root, &installed.id)
                    .map(|path| path.to_string_lossy().into_owned()),
                id: installed.id, name: installed.name,
                subtitle: "Installed on this device".into(),
                description: "This installed app is not offered by the current verified catalog. It remains in your library, but cannot be opened until the Hub offers this version again.".into(),
                category: "utilities".into(), publisher: String::new(), version: installed.version,
                release_notes: String::new(), screenshots: Vec::new(),
                permissions: Vec::new(), privacy: Vec::new(), kind: CatalogKind::Live,
                status: EntryStatus::Unavailable("Not offered by the current catalog".into()), consent: None,
            })
        }).collect();
        CatalogSnapshot {
            entries,
            library,
            warning,
            can_install,
            published: self.store.catalog().map(|c| c.published.clone()),
            verified: self.store.catalog().is_some(),
        }
    }

    fn present(&self, listing: Listing, can_install: bool) -> Entry {
        let status = match listing.availability {
            Availability::Installable => EntryStatus::Available,
            Availability::Installed { version } if version != listing.version => {
                EntryStatus::UpdateAvailable
            }
            Availability::Installed { .. } => match self.may_open(&listing.app_id) {
                Ok(_) => EntryStatus::Installed,
                Err(error) => EntryStatus::Unavailable(error),
            },
            Availability::Withdrawn { reason } => EntryStatus::Unavailable(reason),
        };
        let consent = if can_install
            && matches!(
                status,
                EntryStatus::Available | EntryStatus::UpdateAvailable
            ) {
            self.store.entry(&listing.app_id).and_then(|entry| {
                octosense_app_policy::policy::resolve(&entry.manifest, &HostLimits::default())
                    .ok()?;
                Some(InstallConsent {
                    app_id: listing.app_id.clone(),
                    version: listing.version.clone(),
                    canonical_entry: serde_json::to_string(entry).ok()?,
                })
            })
        } else {
            None
        };
        let about = listing.about.as_ref();
        let local_icon = matches!(status, EntryStatus::Installed)
            .then(|| crate::icons::installed_icon_path(&self.root, &listing.app_id))
            .flatten().map(|path| path.to_string_lossy().into_owned());
        Entry {
            id: listing.app_id,
            name: listing.name,
            subtitle: about
                .map(|a| a.subtitle.clone())
                .unwrap_or_else(|| "An app for OctoSense".into()),
            description: about.map(|a| a.description.clone()).unwrap_or_else(|| {
                "The publisher has not supplied a description for this version.".into()
            }),
            category: about
                .map(|a| a.category.clone())
                .unwrap_or_else(|| "utilities".into()),
            publisher: about
                .map(|a| a.publisher.name.clone())
                .unwrap_or(listing.publisher),
            version: listing.version,
            release_notes: about.map(|a| a.release_notes.clone()).unwrap_or_default(),
            icon: local_icon.or_else(|| about
                .and_then(|a| a.icon.as_deref())
                .and_then(|asset| asset_location(&self.origin, &listing.artifact, asset))),
            screenshots: about
                .into_iter()
                .flat_map(|a| &a.screenshots)
                .filter_map(|asset| asset_location(&self.origin, &listing.artifact, asset))
                .collect(),
            permissions: listing.permissions,
            privacy: listing.privacy,
            kind: CatalogKind::Live,
            status,
            consent,
        }
    }

    /// The token belongs to the permissions shown before the user confirmed.
    /// Re-fetching may change the listing; any such change requires new consent.
    pub fn install(&mut self, consent: &InstallConsent) -> Result<CatalogSnapshot, String> {
        let _guard = CATALOG_IO.lock().unwrap_or_else(|p| p.into_inner());
        self.refresh_catalog();
        self.check_consent(consent)?;
        let artifact = self.store.entry(&consent.app_id).unwrap().artifact.clone();
        if !safe_relative(&artifact) {
            return Err("The catalog artifact path is not bundle-relative".into());
        }
        let workspace = self.root.join(".app-hub-install");
        let result = (|| {
            let staged = self.origin.stage(&artifact, &workspace)?;
            // Downloads can be long. Observe a withdrawal or revised permissions
            // published while the artifact was arriving before installing it.
            self.refresh_catalog();
            self.check_consent(consent)?;
            let entry = self.store.entry(&consent.app_id).unwrap();
            let staged_json =
                std::fs::read_to_string(staged.join(octosense_app_policy::MANIFEST_FILE))
                    .map_err(|error| format!("Could not read the downloaded manifest: {error}"))?;
            let staged_manifest = octosense_app_policy::AppManifest::parse(&staged_json)?;
            if serde_json::to_value(staged_manifest).map_err(|e| e.to_string())?
                != serde_json::to_value(&entry.manifest).map_err(|e| e.to_string())?
            {
                return Err(
                    "The downloaded manifest differs from the reviewed catalog manifest".into(),
                );
            }
            // The Hub backend copies into its own root. Give it a staging root
            // so a failed copy cannot remove or expose a partial live bundle.
            let mut prepared_store = Store::new(
                &self.anchor,
                &workspace.join("verified"),
                HostLimits::default(),
            );
            prepared_store.accept_catalog(
                &serde_json::to_string(self.store.catalog().unwrap()).map_err(|e| e.to_string())?,
            )?;
            prepared_store.install_staged(
                &consent.app_id,
                &staged,
                &self.store.publisher_keys(),
                &octosense_app_hub::today(),
            )?;
            publish_bundle(
                &self.root.join(&consent.app_id),
                &prepared_store.install_dir(&consent.app_id),
                |from, to| std::fs::rename(from, to),
            )?;
            Ok(self.snapshot())
        })();
        let _ = std::fs::remove_dir_all(&workspace);
        result
    }

    fn check_consent(&self, consent: &InstallConsent) -> Result<(), String> {
        self.check_sequence_floor()?;
        self.store.installs_allowed(&octosense_app_hub::today())?;
        if self.persistence_failed {
            return Err("The verified catalog could not be saved; retry before installing".into());
        }
        let entry = self
            .store
            .entry(&consent.app_id)
            .ok_or("This app is no longer offered by the Hub")?;
        if let octosense_app_hub::Status::Withdrawn(reason) = &entry.status {
            return Err(format!("This app was withdrawn: {reason}"));
        }
        if entry.version() != consent.version
            || serde_json::to_string(entry).map_err(|e| e.to_string())? != consent.canonical_entry
        {
            return Err("This app changed after the permissions were shown. Review its details and confirm again.".into());
        }
        octosense_app_policy::policy::resolve(&entry.manifest, &HostLimits::default())?;
        Ok(())
    }

    pub fn may_open(&self, id: &str) -> Result<(), String> {
        self.check_sequence_floor()?;
        self.store.may_run(id).map(|_| ())
    }

    fn remember_sequence(&self, sequence: u64) -> Result<(), String> {
        let mut floors = VERIFIED_SEQUENCES
            .get_or_init(|| Mutex::new(BTreeMap::new()))
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let floor = floors
            .entry((self.root_key.clone(), self.anchor.clone()))
            .or_insert(sequence);
        if sequence < *floor {
            return Err(format!("Refusing catalog {sequence}: a newer catalog ({floor}) was already verified. Retry until it can be saved."));
        }
        *floor = sequence;
        Ok(())
    }

    fn check_sequence_floor(&self) -> Result<(), String> {
        let floors = VERIFIED_SEQUENCES
            .get_or_init(|| Mutex::new(BTreeMap::new()))
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(floor) = floors.get(&(self.root_key.clone(), self.anchor.clone())) {
            if self
                .store
                .catalog()
                .is_none_or(|catalog| catalog.sequence < *floor)
            {
                return Err(format!("Catalog {floor} was verified but is not available here. Installs and opens pause until it can be saved."));
            }
        }
        Ok(())
    }
}

fn root_identity(root: &Path) -> PathBuf {
    std::fs::canonicalize(root).unwrap_or_else(|_| {
        let absolute = if root.is_absolute() {
            root.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(root)
        };
        // The app root may not exist on first launch, while its parent does.
        match (absolute.parent(), absolute.file_name()) {
            (Some(parent), Some(name)) => std::fs::canonicalize(parent)
                .unwrap_or_else(|_| parent.to_path_buf())
                .join(name),
            _ => absolute,
        }
    })
}

/// These entries describe built-in apps, not downloadable Hub listings.
pub fn preview_entries() -> Vec<Entry> {
    [
        ("news", "News", "Stories worth your time", "Follow the stories that matter to you in OctoSense News.", "news"),
        ("maps", "Maps", "Find your next place", "Explore the world around you with OctoSense Maps.", "travel"),
        ("photos", "Photos", "Your moments, together", "Browse your photos and revisit the moments you have captured.", "photo-video"),
        ("camera", "Camera", "See it. Capture it.", "Open the built-in OctoSense Camera and capture a new moment.", "photo-video"),
        ("mail", "Mail", "Make room for your inbox", "Keep your conversations close with OctoSense Mail.", "productivity"),
        ("sheets", "Sheets", "Give your ideas some structure", "Work with tables and organize your ideas in OctoSense Sheets.", "productivity"),
    ].into_iter().map(|(id, name, subtitle, description, category)| Entry {
        id: id.into(), name: name.into(), subtitle: subtitle.into(),
        description: format!("{description}\n\nPreview catalog — this app is included with OctoSense. This is not a downloadable Hub listing."),
        category: category.into(), publisher: "OctoSense".into(), version: "Built in".into(),
        release_notes: String::new(), icon: None, screenshots: Vec::new(),
        permissions: vec!["Built-in app permissions are managed by OctoSense and the app itself.".into()],
        privacy: vec!["This preview is not a publisher privacy declaration.".into()],
        kind: CatalogKind::Preview, status: EntryStatus::BuiltIn, consent: None,
    }).collect()
}

pub fn filter_entries(entries: &[Entry], query: &str, category: Option<&str>) -> Vec<Entry> {
    let query = query.to_lowercase();
    let words: Vec<_> = query.split_whitespace().collect();
    entries
        .iter()
        .filter(|entry| {
            let category_matches = category
                .filter(|c| !c.is_empty())
                .is_none_or(|c| entry.category.eq_ignore_ascii_case(c));
            let searchable = format!(
                "{} {} {} {} {} {}",
                entry.id,
                entry.name,
                entry.subtitle,
                entry.description,
                entry.publisher,
                entry.category
            )
            .to_lowercase();
            category_matches && words.iter().all(|word| searchable.contains(word))
        })
        .cloned()
        .collect()
}

fn safe_relative(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\\')
        && !value.contains(':')
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn asset_location(origin: &Origin, artifact: &str, asset: &str) -> Option<String> {
    if !safe_relative(artifact) || !safe_relative(asset) {
        return None;
    }
    match origin {
        Origin::Directory(root) => Some(
            root.join(artifact)
                .join(asset)
                .to_string_lossy()
                .into_owned(),
        ),
        Origin::Http(base) => {
            let path = format!("{artifact}/{asset}");
            let encoded: String = path
                .bytes()
                .map(|byte| {
                    if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
                        (byte as char).to_string()
                    } else {
                        format!("%{byte:02X}")
                    }
                })
                .collect();
            Some(format!("{}/{encoded}", base.trim_end_matches('/')))
        }
    }
}

fn persist_catalog(root: &Path, json: &str) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| format!("Could not create the app library: {e}"))?;
    let temporary = root.join(".catalog.json.tmp");
    let result = (|| {
        let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
        file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, root.join("catalog.json")).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result.map_err(|e| format!("Could not save the verified catalog: {e}"))
}

fn publish_bundle(
    app_root: &Path,
    prepared: &Path,
    mut rename: impl FnMut(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), String> {
    recover_bundle(app_root)?;
    std::fs::create_dir_all(app_root)
        .map_err(|e| format!("Could not create the app directory: {e}"))?;
    let bundle = app_root.join("bundle");
    let next = app_root.join(".bundle-next");
    let previous = app_root.join(".bundle-previous");
    rename(prepared, &next)
        .map_err(|e| format!("Could not prepare the verified installation: {e}"))?;
    if bundle.exists() {
        rename(&bundle, &previous)
            .map_err(|e| format!("Could not preserve the previous installation: {e}"))?;
    }
    if let Err(error) = rename(&next, &bundle) {
        if previous.exists() {
            if let Err(rollback) = rename(&previous, &bundle) {
                // Keep both directories: startup recovery can restore the old
                // bundle once the filesystem permits writes again.
                return Err(format!("Could not publish the installation: {error}. The previous bundle is preserved for recovery: {rollback}"));
            }
        }
        let _ = std::fs::remove_dir_all(&next);
        return Err(format!("Could not publish the installation: {error}"));
    }
    // Publication is complete. A cleanup failure is harmless and is retried
    // by recovery; the app's separate data/jail is never renamed or removed.
    if previous.exists() {
        let _ = std::fs::remove_dir_all(previous);
    }
    Ok(())
}

fn recover_bundle(app_root: &Path) -> Result<(), String> {
    let bundle = app_root.join("bundle");
    let previous = app_root.join(".bundle-previous");
    let next = app_root.join(".bundle-next");
    if previous.exists() {
        if bundle.exists() {
            // The final rename happened before interruption: keep the complete
            // new bundle, then discard the previous bundle.
            std::fs::remove_dir_all(&previous)
                .map_err(|e| format!("Could not clean up the previous installation: {e}"))?;
        } else {
            // Interrupted between renames: restore the last complete bundle.
            std::fs::rename(&previous, &bundle)
                .map_err(|e| format!("Could not recover the previous installation: {e}"))?;
        }
    }
    if next.exists() {
        std::fs::remove_dir_all(next)
            .map_err(|e| format!("Could not clean up the interrupted installation: {e}"))?;
    }
    Ok(())
}

fn recover_installations(root: &Path) -> Result<(), String> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Could not inspect interrupted installations: {error}"
            ))
        }
    };
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir()
            && !entry.file_name().to_string_lossy().starts_with('.')
        {
            recover_bundle(&entry.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use octosense_app_hub::{Catalog, HubKey, Source, Status};
    use octosense_app_policy::{AppManifest, MANIFEST_FILE};
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture {
        path: PathBuf,
        anchor: HubKey,
        working: HubKey,
        publisher: HubKey,
    }

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "octosense-hub-catalog-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(path.join("hub")).unwrap();
            Self {
                path,
                anchor: HubKey::from_bytes(&[1; 32]),
                working: HubKey::from_bytes(&[2; 32]),
                publisher: HubKey::from_bytes(&[3; 32]),
            }
        }

        fn root(&self) -> PathBuf {
            self.path.join("apps")
        }

        fn backend(&self) -> Backend {
            Backend::new(
                self.root(),
                Origin::Directory(self.path.join("hub")),
                self.anchor.public_hex(),
            )
        }

        fn entry(&self) -> octosense_app_hub::Entry {
            let artifact = "artifacts/test-app-1.bundle";
            let bundle = self.path.join("hub").join(artifact);
            std::fs::create_dir_all(&bundle).unwrap();
            std::fs::write(bundle.join("main.splash"), "text Hello").unwrap();
            let mut manifest = AppManifest::parse(r#"{"schema":1,"id":"test-app","version":"1","name":"Test App","integrity":{"bundle_blake3":""},"capabilities":["storage"]}"#).unwrap();
            manifest.integrity.bundle_blake3 = octosense_app_policy::digest_dir(&bundle).unwrap();
            octosense_app_hub::sign_manifest(&self.publisher, &mut manifest, "test-publisher")
                .unwrap();
            std::fs::write(
                bundle.join(MANIFEST_FILE),
                serde_json::to_vec(&manifest).unwrap(),
            )
            .unwrap();
            octosense_app_hub::Entry {
                manifest,
                listing: None,
                artifact: artifact.into(),
                publisher: "test-publisher".into(),
                publisher_key: self.publisher.public_hex(),
                source: Source {
                    repository: "https://example.test/app".into(),
                    commit: "test".into(),
                },
                status: Status::Offered,
                admitted: octosense_app_hub::today(),
            }
        }

        fn update(&self) -> octosense_app_hub::Entry {
            let mut entry = self.entry();
            entry.manifest.version = "2".into();
            entry.artifact = "artifacts/test-app-2.bundle".into();
            let bundle = self.path.join("hub").join(&entry.artifact);
            std::fs::create_dir_all(&bundle).unwrap();
            std::fs::write(bundle.join("main.splash"), "text Updated").unwrap();
            entry.manifest.integrity.bundle_blake3 =
                octosense_app_policy::digest_dir(&bundle).unwrap();
            octosense_app_hub::sign_manifest(
                &self.publisher,
                &mut entry.manifest,
                "test-publisher",
            )
            .unwrap();
            std::fs::write(
                bundle.join(MANIFEST_FILE),
                serde_json::to_vec(&entry.manifest).unwrap(),
            )
            .unwrap();
            entry
        }

        fn install_first_version(&self) -> Backend {
            self.publish_today(1, vec![self.entry()]);
            let mut backend = self.backend();
            let consent = backend.refresh().entries[0].consent.clone().unwrap();
            backend.install(&consent).unwrap();
            std::fs::create_dir_all(self.root().join("test-app/data")).unwrap();
            std::fs::write(self.root().join("test-app/data/notes"), "keep my notes").unwrap();
            backend
        }

        fn publish(
            &self,
            sequence: u64,
            date: &str,
            entries: Vec<octosense_app_hub::Entry>,
        ) -> String {
            let mut catalog = Catalog::new(sequence, date, entries);
            self.working
                .sign_catalog(
                    &mut catalog,
                    &self.anchor.certify(&self.working.public_hex()).unwrap(),
                )
                .unwrap();
            let json = serde_json::to_string(&catalog).unwrap();
            std::fs::write(self.path.join("hub/catalog.json"), &json).unwrap();
            json
        }

        fn publish_today(&self, sequence: u64, entries: Vec<octosense_app_hub::Entry>) -> String {
            self.publish(sequence, &octosense_app_hub::today(), entries)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn preview_apps_are_built_in_open_targets_never_install_candidates() {
        let entries = preview_entries();
        assert_eq!(entries.len(), 6);
        for entry in entries {
            assert_eq!(entry.kind, CatalogKind::Preview);
            assert_eq!(entry.status, EntryStatus::BuiltIn);
            assert!(entry.consent.is_none());
            assert_eq!(entry.open_target(), Some(entry.id.as_str()));
        }
    }

    #[test]
    fn search_trims_and_combines_case_insensitive_words_with_category() {
        let entries = preview_entries();
        assert_eq!(
            filter_entries(&entries, "  PHOTOS  ", Some("photo-video")).len(),
            1
        );
        assert!(filter_entries(&entries, "photos", Some("news")).is_empty());
        assert_eq!(filter_entries(&entries, "", None).len(), 6);
    }

    #[test]
    fn empty_live_catalog_does_not_implicitly_show_preview() {
        let f = Fixture::new();
        f.publish_today(1, vec![]);
        let snapshot = f.backend().refresh();
        assert!(snapshot.verified);
        assert!(snapshot.entries.is_empty());
        assert!(snapshot.can_install);
        assert!(snapshot.warning.is_none());
    }

    #[test]
    fn replay_is_refused_after_restarting_and_verified_cache_is_retained() {
        let f = Fixture::new();
        let original = f.publish_today(4, vec![f.entry()]);
        assert_eq!(f.backend().refresh().entries.len(), 1);
        f.publish_today(3, vec![]);
        let snapshot = f.backend().refresh();
        assert_eq!(snapshot.entries.len(), 1);
        assert!(snapshot.warning.unwrap().contains("older"));
        assert_eq!(
            std::fs::read_to_string(f.root().join("catalog.json")).unwrap(),
            original
        );
    }

    #[test]
    fn tampered_catalog_never_replaces_verified_cache() {
        let f = Fixture::new();
        let original = f.publish_today(1, vec![f.entry()]);
        f.backend().refresh();
        std::fs::write(
            f.path.join("hub/catalog.json"),
            original.replace("Test App", "Forged App"),
        )
        .unwrap();
        let snapshot = f.backend().refresh();
        assert_eq!(snapshot.entries[0].name, "Test App");
        assert!(snapshot.warning.unwrap().contains("signature"));
        assert_eq!(
            std::fs::read_to_string(f.root().join("catalog.json")).unwrap(),
            original
        );
    }

    #[test]
    fn a_catalog_that_cannot_be_persisted_does_not_replace_the_accepted_catalog() {
        let f = Fixture::new();
        let original = f.publish_today(1, vec![f.entry()]);
        let mut backend = f.backend();
        backend.refresh();
        std::fs::create_dir(f.root().join(".catalog.json.tmp")).unwrap();
        f.publish_today(2, vec![]);
        let snapshot = backend.refresh();
        assert_eq!(snapshot.entries.len(), 1);
        assert!(!snapshot.can_install);
        assert!(snapshot.warning.unwrap().contains("save"));
        assert_eq!(
            std::fs::read_to_string(f.root().join("catalog.json")).unwrap(),
            original
        );
    }

    #[test]
    fn a_failed_withdrawal_save_prevents_replay_across_backend_instances() {
        let f = Fixture::new();
        let mut entry = f.entry();
        let original = f.publish_today(1, vec![entry.clone()]);
        let mut first = f.backend();
        let consent = first.refresh().entries[0].consent.clone().unwrap();
        first.install(&consent).unwrap();
        let mut second = f.backend();
        std::fs::create_dir(f.root().join(".catalog.json.tmp")).unwrap();
        entry.status = Status::Withdrawn("Withdrawn for review".into());
        f.publish_today(2, vec![entry]);
        assert!(!first.refresh().can_install);
        std::fs::remove_dir(f.root().join(".catalog.json.tmp")).unwrap();
        std::fs::write(f.path.join("hub/catalog.json"), original).unwrap();
        let mut third = f.backend();

        // Existing and new workers retain the highest verified sequence even
        // though the disk still contains the previous catalog.
        for backend in [&mut first, &mut second, &mut third] {
            let snapshot = backend.refresh();
            assert!(!snapshot.can_install);
            assert!(snapshot.entries[0].consent.is_none());
            assert!(snapshot.entries[0].open_target().is_none());
            assert!(backend.may_open("test-app").is_err());
            assert!(backend.install(&consent).is_err());
            assert!(snapshot.warning.unwrap().contains("2"));
        }
    }

    #[test]
    fn offline_refresh_keeps_cached_browse_and_says_it_is_cached() {
        let f = Fixture::new();
        f.publish_today(1, vec![f.entry()]);
        f.backend().refresh();
        std::fs::remove_file(f.path.join("hub/catalog.json")).unwrap();
        let snapshot = f.backend().refresh();
        assert_eq!(snapshot.entries.len(), 1);
        assert!(snapshot.warning.unwrap().contains("cached"));
    }

    #[test]
    fn stale_catalog_is_browsable_but_cannot_supply_install_consent() {
        let f = Fixture::new();
        f.publish(1, "2000-01-01", vec![f.entry()]);
        let snapshot = f.backend().refresh();
        assert!(snapshot.verified);
        assert!(!snapshot.can_install);
        assert_eq!(snapshot.entries.len(), 1);
        assert!(snapshot.entries[0].consent.is_none());
        assert!(snapshot.warning.unwrap().contains("old"));
    }

    #[test]
    fn permission_change_invalidates_prior_consent_before_staging() {
        let f = Fixture::new();
        let entry = f.entry();
        f.publish_today(1, vec![entry.clone()]);
        let mut backend = f.backend();
        let consent = backend.refresh().entries[0].consent.clone().unwrap();
        let mut changed = entry;
        changed.manifest.capabilities.push("camera".into());
        octosense_app_hub::sign_manifest(&f.publisher, &mut changed.manifest, "test-publisher")
            .unwrap();
        f.publish_today(2, vec![changed]);
        assert!(backend.install(&consent).unwrap_err().contains("changed"));
        assert!(!f.root().join("test-app/bundle").exists());
    }

    #[test]
    fn verified_bundle_installs_then_opens_and_appears_in_library() {
        let f = Fixture::new();
        f.publish_today(1, vec![f.entry()]);
        let mut backend = f.backend();
        let consent = backend.refresh().entries[0].consent.clone().unwrap();
        let snapshot = backend.install(&consent).unwrap();
        assert_eq!(snapshot.entries[0].status, EntryStatus::Installed);
        assert_eq!(snapshot.library.len(), 1);
        assert_eq!(snapshot.library[0].open_target(), Some("test-app"));
        assert!(backend.may_open("test-app").is_ok());
    }

    #[test]
    fn installed_app_missing_from_current_catalog_remains_in_library_but_cannot_open() {
        let f = Fixture::new();
        f.publish_today(1, vec![f.entry()]);
        let mut backend = f.backend();
        let consent = backend.refresh().entries[0].consent.clone().unwrap();
        backend.install(&consent).unwrap();
        f.publish_today(2, vec![]);
        let snapshot = backend.refresh();
        assert!(snapshot.entries.is_empty());
        assert_eq!(snapshot.library.len(), 1);
        assert_eq!(snapshot.library[0].open_target(), None);
        assert!(backend.may_open("test-app").is_err());
    }

    #[test]
    fn modified_staged_manifest_is_rejected_even_with_unchanged_bundle_digest() {
        let f = Fixture::new();
        let entry = f.entry();
        f.publish_today(1, vec![entry.clone()]);
        let mut backend = f.backend();
        let consent = backend.refresh().entries[0].consent.clone().unwrap();
        let mut changed = entry.manifest;
        changed.capabilities.push("camera".into());
        std::fs::write(
            f.path.join("hub").join(entry.artifact).join(MANIFEST_FILE),
            serde_json::to_vec(&changed).unwrap(),
        )
        .unwrap();
        assert!(backend.install(&consent).unwrap_err().contains("manifest"));
        assert!(!f.root().join("test-app/bundle").exists());
    }

    #[test]
    fn withdrawal_after_consent_refuses_install_and_open() {
        let f = Fixture::new();
        let mut entry = f.entry();
        f.publish_today(1, vec![entry.clone()]);
        let mut backend = f.backend();
        let consent = backend.refresh().entries[0].consent.clone().unwrap();
        entry.status = Status::Withdrawn("Publisher withdrew this version".into());
        f.publish_today(2, vec![entry]);
        assert!(backend.install(&consent).is_err());
        let snapshot = backend.snapshot();
        assert!(matches!(
            snapshot.entries[0].status,
            EntryStatus::Unavailable(_)
        ));
        assert!(snapshot.entries[0].consent.is_none());
        assert!(backend.may_open("test-app").is_err());
    }

    #[test]
    fn failed_staging_copy_keeps_the_installed_bundle_and_app_data() {
        let f = Fixture::new();
        let mut backend = f.install_first_version();
        f.publish_today(2, vec![f.update()]);
        let consent = backend.refresh().entries[0].consent.clone().unwrap();
        let workspace = f.root().join(".app-hub-install");
        std::fs::create_dir_all(&workspace).unwrap();
        // An actual I/O failure in the backend's copy destination.
        std::fs::write(workspace.join("verified"), "not a directory").unwrap();
        assert!(backend.install(&consent).is_err());
        assert_eq!(
            backend.store.installed_version("test-app").as_deref(),
            Some("1")
        );
        assert_eq!(
            std::fs::read_to_string(f.root().join("test-app/bundle/main.splash")).unwrap(),
            "text Hello"
        );
        assert_eq!(
            std::fs::read_to_string(f.root().join("test-app/data/notes")).unwrap(),
            "keep my notes"
        );
    }

    #[test]
    fn failed_publication_rename_rolls_back_the_previous_bundle() {
        let f = Fixture::new();
        f.install_first_version();
        let update = f.update();
        let prepared = f.path.join("hub").join(update.artifact);
        let app_root = f.root().join("test-app");
        let mut injected = false;
        let result = publish_bundle(&app_root, &prepared, |from, to| {
            if from == app_root.join(".bundle-next") && to == app_root.join("bundle") {
                injected = true;
                Err(std::io::Error::other("injected publication failure"))
            } else {
                std::fs::rename(from, to)
            }
        });
        assert!(result.is_err());
        assert!(injected);
        assert_eq!(
            std::fs::read_to_string(app_root.join("bundle/main.splash")).unwrap(),
            "text Hello"
        );
        assert!(!app_root.join(".bundle-previous").exists());
        assert_eq!(
            std::fs::read_to_string(app_root.join("data/notes")).unwrap(),
            "keep my notes"
        );
    }

    #[test]
    fn restart_before_publication_restores_previous_bundle() {
        let f = Fixture::new();
        f.install_first_version();
        let app_root = f.root().join("test-app");
        let update = f.update();
        std::fs::rename(app_root.join("bundle"), app_root.join(".bundle-previous")).unwrap();
        std::fs::rename(
            f.path.join("hub").join(update.artifact),
            app_root.join(".bundle-next"),
        )
        .unwrap();
        f.backend();
        assert_eq!(
            std::fs::read_to_string(app_root.join("bundle/main.splash")).unwrap(),
            "text Hello"
        );
        assert!(!app_root.join(".bundle-previous").exists());
        assert!(!app_root.join(".bundle-next").exists());
        assert_eq!(
            std::fs::read_to_string(app_root.join("data/notes")).unwrap(),
            "keep my notes"
        );
    }

    #[test]
    fn failed_rollback_keeps_previous_bundle_for_next_startup_recovery() {
        let f = Fixture::new();
        f.install_first_version();
        let update = f.update();
        let app_root = f.root().join("test-app");
        let result = publish_bundle(
            &app_root,
            &f.path.join("hub").join(update.artifact),
            |from, to| {
                if to == app_root.join("bundle") {
                    Err(std::io::Error::other(
                        "filesystem temporarily refuses publication and rollback",
                    ))
                } else {
                    std::fs::rename(from, to)
                }
            },
        );
        assert!(result.unwrap_err().contains("preserved for recovery"));
        assert_eq!(
            std::fs::read_to_string(app_root.join(".bundle-previous/main.splash")).unwrap(),
            "text Hello"
        );
        assert!(!app_root.join("bundle").exists());
        let snapshot = f.backend().snapshot();
        assert_eq!(snapshot.library.len(), 1);
        assert_eq!(
            std::fs::read_to_string(app_root.join("bundle/main.splash")).unwrap(),
            "text Hello"
        );
        assert!(!app_root.join(".bundle-next").exists());
        assert!(!app_root.join(".bundle-previous").exists());
    }

    #[test]
    fn restart_after_publication_keeps_the_complete_new_bundle() {
        let f = Fixture::new();
        f.install_first_version();
        let app_root = f.root().join("test-app");
        let update = f.update();
        std::fs::rename(app_root.join("bundle"), app_root.join(".bundle-previous")).unwrap();
        std::fs::rename(
            f.path.join("hub").join(update.artifact),
            app_root.join("bundle"),
        )
        .unwrap();
        f.backend();
        assert_eq!(
            std::fs::read_to_string(app_root.join("bundle/main.splash")).unwrap(),
            "text Updated"
        );
        assert!(!app_root.join(".bundle-previous").exists());
        assert_eq!(
            std::fs::read_to_string(app_root.join("data/notes")).unwrap(),
            "keep my notes"
        );
    }

    #[test]
    fn abandoned_first_install_is_not_reported_as_installed() {
        let f = Fixture::new();
        let entry = f.entry();
        f.publish_today(1, vec![entry.clone()]);
        let app_root = f.root().join("test-app");
        std::fs::create_dir_all(app_root.join(".bundle-next")).unwrap();
        std::fs::write(
            app_root.join(".bundle-next/manifest.json"),
            serde_json::to_vec(&entry.manifest).unwrap(),
        )
        .unwrap();
        let snapshot = f.backend().refresh();
        assert!(snapshot.library.is_empty());
        assert!(!app_root.join(".bundle-next").exists());
        assert!(!app_root.join("bundle").exists());
    }

    #[test]
    fn successful_update_publishes_complete_bundle_and_preserves_data() {
        let f = Fixture::new();
        let mut backend = f.install_first_version();
        f.publish_today(2, vec![f.update()]);
        let consent = backend.refresh().entries[0].consent.clone().unwrap();
        backend.install(&consent).unwrap();
        let app_root = f.root().join("test-app");
        assert_eq!(
            backend.store.installed_version("test-app").as_deref(),
            Some("2")
        );
        assert_eq!(
            std::fs::read_to_string(app_root.join("bundle/main.splash")).unwrap(),
            "text Updated"
        );
        assert!(!app_root.join(".bundle-previous").exists());
        assert!(!app_root.join(".bundle-next").exists());
        assert_eq!(
            std::fs::read_to_string(app_root.join("data/notes")).unwrap(),
            "keep my notes"
        );
    }

    #[test]
    fn launch_gate_returns_busy_instead_of_waiting_for_catalog_io() {
        let f = Fixture::new();
        let guard = CATALOG_IO.lock().unwrap_or_else(|p| p.into_inner());
        let (sender, receiver) = std::sync::mpsc::channel();
        let root = f.root();
        let worker = std::thread::spawn(move || {
            sender
                .send(try_may_open_from_environment(root, "test-app"))
                .unwrap();
        });
        let result = receiver.recv_timeout(std::time::Duration::from_secs(1));
        drop(guard);
        worker.join().unwrap();
        assert_eq!(
            result.unwrap().unwrap_err(),
            "App Hub is updating. Try again shortly."
        );
    }

    #[test]
    fn launch_gate_uses_cached_catalog_without_contacting_origin() {
        let f = Fixture::new();
        f.install_first_version();
        let missing_origin = Origin::Directory(f.path.join("missing-hub"));
        // Exercise the same post-lock path without racing other tests' workers.
        let _guard = CATALOG_IO.lock().unwrap_or_else(|p| p.into_inner());
        assert!(
            Backend::new_unlocked(f.root(), missing_origin, f.anchor.public_hex())
                .may_open("test-app")
                .is_ok()
        );
    }

    #[test]
    fn launch_gate_refuses_the_unsaved_newer_catalog_floor() {
        let f = Fixture::new();
        let mut backend = f.install_first_version();
        let mut withdrawn = f.entry();
        withdrawn.status = Status::Withdrawn("Withdrawn for review".into());
        std::fs::create_dir(f.root().join(".catalog.json.tmp")).unwrap();
        f.publish_today(2, vec![withdrawn]);
        assert!(!backend.refresh().can_install);
        let _guard = CATALOG_IO.lock().unwrap_or_else(|p| p.into_inner());
        let error = Backend::new_unlocked(
            f.root(),
            Origin::Directory(f.path.join("missing-hub")),
            f.anchor.public_hex(),
        )
        .may_open("test-app")
        .unwrap_err();
        assert!(error.contains("Catalog 2"), "{error}");
    }

    #[test]
    fn asset_paths_resolve_under_the_reviewed_bundle_and_escape_url_characters() {
        let origin = Origin::Http("https://example.test/hub/".into());
        assert_eq!(
            asset_location(&origin, "artifacts/demo.bundle", "screenshots/one two.png"),
            Some("https://example.test/hub/artifacts/demo.bundle/screenshots/one%20two.png".into())
        );
        assert_eq!(asset_location(&origin, "../other", "icon.png"), None);
        assert_eq!(
            asset_location(&origin, "artifacts/demo.bundle", "../icon.png"),
            None
        );
        assert_eq!(
            asset_location(&origin, "artifacts/demo.bundle", "/icon.png"),
            None
        );
    }
}
