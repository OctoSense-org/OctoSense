//! Trusted publisher identity, kept apart from a submission's key claims.
//!
//! The catalog is the record: once a release signed by a key is on it, that
//! key is the publisher's, for every later version and for their other apps.
//! A submission names a key; it never redefines one. V1 uses the publisher
//! id as the signature's key id, and has no authorized rotation: history
//! that disagrees with itself needs an operator, not a choice of entry.
//!
//! Unsigned releases (`--allow-unsigned`) put an owner on record but no key.
//! The app's first signed update records the key from then on.
use crate::{github_publisher::GithubBinding, Catalog, PublisherKeys};
use octosense_app_policy::{AppManifest, SignatureVerifier};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublisherBinding {
    pub publisher_id: String,
    pub key_id: String,
    pub public_key_hex: String,
}

/// Read-only identity lookup. Authenticated registry or rotation records can
/// implement it later without letting a submission change a key.
pub trait PublisherRegistry {
    fn binding(&self, key_id: &str) -> Option<&PublisherBinding>;
    fn app_owner(&self, app_id: &str) -> Option<&str>;
    fn github_binding(&self, _app_id: &str) -> Option<&GithubBinding> { None }
    fn github_version(&self, _app_id: &str) -> Option<&str> { None }
}

#[derive(Default)]
pub struct CatalogPublishers {
    bindings: BTreeMap<String, PublisherBinding>,
    owners: BTreeMap<String, String>,
    github: BTreeMap<String, GithubBinding>,
    legacy_apps: std::collections::BTreeSet<String>,
    versions: BTreeMap<String,String>,
}

impl CatalogPublishers {
    /// The bindings and owners `catalog` records. The caller authenticates
    /// the catalog first. Every entry counts, withdrawn ones too, so the
    /// order of entries never picks a key.
    pub fn from_catalog(catalog: &Catalog) -> Result<Self, String> {
        let mut registry = Self::default();
        for entry in &catalog.entries {
            if entry.publisher.is_empty() {
                return Err(format!("{} has no publisher on record; operator reconciliation required", entry.app_id()));
            }
            if let Some(owner) = registry.owners.get(entry.app_id()) {
                if owner != &entry.publisher {
                    return Err(format!("conflicting catalog owners for {}; operator reconciliation required", entry.app_id()));
                }
            }
            entry.manifest.check_requires()?;
            if let Some(github) = &entry.manifest.integrity.github {
                if entry.manifest.integrity.signature.is_some() || !entry.publisher_key.is_empty() {
                    return Err("GitHub publisher releases do not carry a legacy signature or key".into());
                }
                if registry.legacy_apps.contains(entry.app_id()) {
                    return Err("existing legacy app ownership cannot be adopted by GitHub provenance".into());
                }
                crate::github_publisher::verify(github, &entry.manifest.signing_bytes()?)?;
                if entry.publisher != format!("github:{}", github.repository_id)
                    || entry.source.repository != github.repository_url() || entry.source.commit != github.commit {
                    return Err("GitHub publisher identity or source differs from authenticated provenance".into());
                }
                let binding=GithubBinding::from(github);
                if registry.github.get(entry.app_id()).is_some_and(|old|old!=&binding) {
                    return Err("GitHub publisher repository, owner or workflow changed".into());
                }
                check_version_advance(&entry.manifest.version,registry.versions.get(entry.app_id()).map(String::as_str))?;
                registry.versions.insert(entry.app_id().into(),entry.version().into());
                registry.github.insert(entry.app_id().into(),binding);
            } else {
                if registry.github.contains_key(entry.app_id()) {
                    return Err("GitHub-owned app cannot downgrade to legacy or unsigned authentication".into());
                }
                registry.legacy_apps.insert(entry.app_id().into());
            }
            if let Some(signature) = &entry.manifest.integrity.signature {
                if signature.key_id != entry.publisher {
                    return Err(format!("{}'s catalog publisher does not match its manifest signature", entry.app_id()));
                }
                if entry.publisher_key.is_empty() {
                    return Err(format!("{} is signed but has no key on record; operator reconciliation required", entry.app_id()));
                }
                let binding = PublisherBinding {
                    publisher_id: entry.publisher.clone(),
                    key_id: signature.key_id.clone(),
                    public_key_hex: entry.publisher_key.to_ascii_lowercase(),
                };
                if registry.bindings.get(&binding.key_id).is_some_and(|previous| previous != &binding) {
                    return Err(format!("conflicting catalog bindings for {:?}; operator reconciliation required", binding.key_id));
                }
                PublisherKeys::new()
                    .with(&binding.key_id, &binding.public_key_hex)
                    .verify(&signature.key_id, &signature.value, &entry.manifest.signing_bytes()?)?;
                registry.bindings.insert(binding.key_id.clone(), binding);
            }
            registry.owners.insert(entry.app_id().to_string(), entry.publisher.clone());
        }
        Ok(registry)
    }

    /// `keys` with every recorded binding added, so a known publisher's
    /// release verifies against its recorded key. A supplied key that
    /// disagrees with one stays in `keys`, and verification refuses both.
    pub fn trusted_keys(&self, mut keys: PublisherKeys) -> PublisherKeys {
        for binding in self.bindings.values() {
            keys = keys.with(&binding.key_id, &binding.public_key_hex);
        }
        keys
    }
}

impl PublisherRegistry for CatalogPublishers {
    fn github_binding(&self, app_id: &str) -> Option<&GithubBinding> { self.github.get(app_id) }
    fn github_version(&self,app_id:&str)->Option<&str>{self.versions.get(app_id).map(String::as_str)}
    fn binding(&self, key_id: &str) -> Option<&PublisherBinding> {
        self.bindings.get(key_id)
    }
    fn app_owner(&self, app_id: &str) -> Option<&str> {
        self.owners.get(app_id).map(String::as_str)
    }
}

/// Check `manifest` against the publishers on record, using only the
/// recorded keys, never a caller's.
pub fn verify_continuity(manifest: &AppManifest, registry: &dyn PublisherRegistry) -> Result<(), String> {
    let owner = registry.app_owner(&manifest.id);
    if let Some(github)=&manifest.integrity.github {
        authorize_github_update(manifest,registry)?;
        crate::github_publisher::verify(github, &manifest.signing_bytes()?)?;
        return Ok(());
    }
    if registry.github_binding(&manifest.id).is_some() {
        return Err("GitHub-owned app cannot downgrade to legacy or unsigned authentication".into());
    }
    let Some(signature) = &manifest.integrity.signature else {
        return match owner {
            Some(owner) => Err(format!("{} is already published by {owner:?}; an update must carry that key", manifest.id)),
            None => Ok(()),
        };
    };
    if let Some(owner) = owner {
        if signature.key_id != owner {
            return Err(format!(
                "{} was published by {owner:?}; this version is signed by {:?}. Re-keying is a reviewed change.",
                manifest.id, signature.key_id
            ));
        }
    }
    if let Some(binding) = registry.binding(&signature.key_id) {
        PublisherKeys::new()
            .with(&binding.key_id, &binding.public_key_hex)
            .verify(&signature.key_id, &signature.value, &manifest.signing_bytes()?)
            .map_err(|e| format!("not signed by the key on record for {:?}: {e}", binding.publisher_id))?;
    }
    Ok(())
}

pub(crate) fn check_version_advance(next:&str,previous:Option<&str>)->Result<(),String>{
    let next=semver::Version::parse(next).map_err(|_|"GitHub publisher version must be semantic version major.minor.patch")?;
    if let Some(previous)=previous {
        let previous=semver::Version::parse(previous).map_err(|_|"recorded GitHub publisher version is invalid")?;
        if !next.cmp_precedence(&previous).is_gt(){return Err("GitHub publisher version must advance; replay and rollback are refused".into());}
    }
    Ok(())
}

fn authorize_github_update(manifest:&AppManifest,registry:&dyn PublisherRegistry)->Result<(),String>{
    let github=manifest.integrity.github.as_ref().ok_or("GitHub publisher is missing")?;
    if manifest.integrity.signature.is_some(){return Err("GitHub publisher releases cannot also carry a legacy signature".into());}
    if let Some(binding)=registry.github_binding(&manifest.id) {
        if binding!=&GithubBinding::from(github){return Err("GitHub publisher repository, owner or workflow changed".into());}
    } else if registry.app_owner(&manifest.id).is_some(){
        return Err("existing legacy app ownership cannot be adopted by GitHub provenance".into());
    }
    check_version_advance(&manifest.version,registry.github_version(&manifest.id))
}

#[cfg(test)]
mod github_policy_tests {
    use super::*;
    use serde_json::json;
    fn manifest()->AppManifest{
        AppManifest::parse(&json!({"schema":1,"id":"example.app","version":"1.2.3","name":"Example",
            "requires":["publisher-github-v1"],"integrity":{"bundle_blake3":"00","github":{
                "repository":"example/app","repository_id":"123","owner_id":"456","workflow":".github/workflows/publish-app.yml",
                "tag":"v1.2.3","commit":"a".repeat(40)}}}).to_string()).unwrap()
    }
    #[test]
    fn identity_policy_accepts_fresh_or_same_authority_advances_and_refuses_changes(){
        let m=manifest(); let mut registry=CatalogPublishers::default();
        authorize_github_update(&m,&registry).unwrap(); // Authorization policy only; no synthetic crypto success.
        registry.owners.insert(m.id.clone(),"github:123".into());
        assert!(authorize_github_update(&m,&registry).unwrap_err().contains("legacy"));
        registry.github.insert(m.id.clone(),GithubBinding::from(m.integrity.github.as_ref().unwrap()));
        registry.versions.insert(m.id.clone(),"1.2.2".into());
        authorize_github_update(&m,&registry).unwrap();
        for field in ["repository","repository_id","owner_id","workflow"]{
            let mut value=serde_json::to_value(&m).unwrap();value["integrity"]["github"][field]=json!(match field{
                "repository"=>"other/app","repository_id"=>"999","owner_id"=>"888",_=>".github/workflows/other.yml"});
            let changed=AppManifest::parse(&value.to_string()).unwrap();
            assert!(authorize_github_update(&changed,&registry).is_err(),"{field}");
        }
        for previous in ["1.2.3","1.2.3+other","2.0.0"]{
            registry.versions.insert(m.id.clone(),previous.into());
            assert!(authorize_github_update(&m,&registry).is_err(),"{previous}");
        }
    }
    #[test]
    fn unsigned_or_legacy_update_cannot_bypass_recorded_github_authority(){
        let mut m=manifest();let mut registry=CatalogPublishers::default();
        registry.github.insert(m.id.clone(),GithubBinding::from(m.integrity.github.as_ref().unwrap()));
        m.integrity.github=None;m.requires.clear();
        assert!(verify_continuity(&m,&registry).unwrap_err().contains("downgrade"));
    }
}
