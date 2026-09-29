//! Loading, checking and pinning templates.

use crate::check::check_source;
use crate::manifest::Manifest;
use crate::{Error, ErrorKind, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const MANIFEST_FILE: &str = "template.json";
pub const SOURCE_FILE: &str = "template.octoscript";
pub const LOCK_FILE: &str = "library.lock.json";

/// Where a template came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum TemplateOrigin {
    /// The toolbox library.
    Library,
    /// An app-local fork, in the app's folder.
    Fork { path: PathBuf },
}

/// A checked template, pinned by the digest of its two files.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub manifest: Manifest,
    pub source: String,
    /// Hex SHA-256 over `template.json` and `template.octoscript`.
    pub digest: String,
    pub origin: TemplateOrigin,
    /// The exact `template.json` bytes (line endings normalized) the digest
    /// covers.
    pub manifest_text: String,
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// The content digest of a template's two files.
pub fn digest(manifest_text: &str, source: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"octosense-toolbox/template/v1\n");
    let manifest = normalize(manifest_text);
    let source = normalize(source);
    hash.update((manifest.len() as u64).to_be_bytes());
    hash.update(manifest.as_bytes());
    hash.update((source.len() as u64).to_be_bytes());
    hash.update(source.as_bytes());
    hex(&hash.finalize())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Template {
    /// Parses, validates and statically checks a template from its two files.
    pub fn from_parts(manifest_text: &str, source: &str, origin: TemplateOrigin) -> Result<Self> {
        let manifest_text = normalize(manifest_text);
        let source = normalize(source);
        let manifest = Manifest::parse(manifest_text.as_bytes())?;
        check_source(&manifest, &source)?;
        Ok(Self {
            digest: digest(&manifest_text, &source),
            manifest,
            source,
            origin,
            manifest_text,
        })
    }

    /// Loads `<dir>/template.json` and `<dir>/template.octoscript`; the
    /// directory name must be the template id.
    pub fn load_dir(dir: &Path, origin: TemplateOrigin) -> Result<Self> {
        let read = |name: &str| {
            let path = dir.join(name);
            let meta = std::fs::metadata(&path)
                .map_err(|e| Error::new(ErrorKind::Io, format!("{}: {e}", path.display())))?;
            if meta.len() > 64 * 1024 {
                return Err(Error::new(
                    ErrorKind::Manifest,
                    format!("{} is too large", path.display()),
                ));
            }
            std::fs::read_to_string(&path)
                .map_err(|e| Error::new(ErrorKind::Io, format!("{}: {e}", path.display())))
        };
        let template = Self::from_parts(&read(MANIFEST_FILE)?, &read(SOURCE_FILE)?, origin)?;
        let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if name != template.manifest.id {
            return Err(Error::new(
                ErrorKind::Manifest,
                format!(
                    "template id {:?} does not match its directory {name:?}",
                    template.manifest.id
                ),
            ));
        }
        Ok(template)
    }

    pub fn id(&self) -> &str {
        &self.manifest.id
    }
}

/// `library.lock.json`: the pinned digest of every library template.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryLock {
    pub schema_version: u32,
    pub templates: BTreeMap<String, LockEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockEntry {
    pub version: String,
    pub digest: String,
}

/// The toolbox template library: every template checked and pinned.
#[derive(Debug, Clone, Default)]
pub struct Library {
    templates: BTreeMap<String, Template>,
}

macro_rules! builtin {
    ($($id:literal),* $(,)?) => {
        &[$((
            $id,
            include_str!(concat!("../templates/", $id, "/template.json")),
            include_str!(concat!("../templates/", $id, "/template.octoscript")),
        )),*]
    };
}

/// The library shipped with this crate: (id, template.json, source).
pub static BUILTIN: &[(&str, &str, &str)] = builtin!(
    "news-digest",
    "topic-brief",
    "weather-plan",
    "market-brief",
    "briefing",
    "compare",
    "dossier",
);

const BUILTIN_LOCK: &str = include_str!("../templates/library.lock.json");

impl Library {
    /// The library shipped with this crate, verified against its lock.
    pub fn builtin() -> Result<Self> {
        let lock: LibraryLock = serde_json::from_str(BUILTIN_LOCK)
            .map_err(|e| Error::new(ErrorKind::Pin, format!("{LOCK_FILE}: {e}")))?;
        let mut templates = Vec::new();
        for (id, manifest, source) in BUILTIN {
            let template = Template::from_parts(manifest, source, TemplateOrigin::Library)?;
            if template.id() != *id {
                return Err(Error::new(
                    ErrorKind::Manifest,
                    format!("builtin {id} declares id {}", template.id()),
                ));
            }
            templates.push(template);
        }
        Self::pinned(templates, &lock)
    }

    /// Loads every `<dir>/<id>/` template and verifies `<dir>/library.lock.json`.
    pub fn load_dir(dir: &Path) -> Result<Self> {
        let lock_text = std::fs::read_to_string(dir.join(LOCK_FILE))
            .map_err(|e| Error::new(ErrorKind::Pin, format!("{LOCK_FILE}: {e}")))?;
        let lock: LibraryLock = serde_json::from_str(&lock_text)
            .map_err(|e| Error::new(ErrorKind::Pin, format!("{LOCK_FILE}: {e}")))?;
        let mut templates = Vec::new();
        let entries = std::fs::read_dir(dir)
            .map_err(|e| Error::new(ErrorKind::Io, format!("{}: {e}", dir.display())))?;
        for entry in entries {
            let entry = entry.map_err(|e| Error::new(ErrorKind::Io, e.to_string()))?;
            if entry.path().is_dir() {
                templates.push(Template::load_dir(&entry.path(), TemplateOrigin::Library)?);
            }
        }
        Self::pinned(templates, &lock)
    }

    /// A library of `templates`, each of which must match `lock` exactly (and
    /// the lock must name nothing else).
    pub fn pinned(templates: Vec<Template>, lock: &LibraryLock) -> Result<Self> {
        if lock.schema_version != 1 {
            return Err(Error::new(
                ErrorKind::Pin,
                "unsupported lock schema_version",
            ));
        }
        let mut library = Library::default();
        for template in templates {
            let id = template.id().to_owned();
            let Some(entry) = lock.templates.get(&id) else {
                return Err(Error::new(ErrorKind::Pin, format!("{id} is not pinned")));
            };
            if entry.digest != template.digest || entry.version != template.manifest.version {
                return Err(Error::new(
                    ErrorKind::Pin,
                    format!(
                        "{id} {} has digest {}; the lock pins {} {}",
                        template.manifest.version, template.digest, entry.version, entry.digest
                    ),
                ));
            }
            if library.templates.insert(id.clone(), template).is_some() {
                return Err(Error::new(
                    ErrorKind::Manifest,
                    format!("{id} appears twice"),
                ));
            }
        }
        if let Some(missing) = lock
            .templates
            .keys()
            .find(|id| !library.templates.contains_key(*id))
        {
            return Err(Error::new(
                ErrorKind::Pin,
                format!("the lock pins {missing}, which is missing"),
            ));
        }
        Ok(library)
    }

    /// The lock that pins `templates` as they are.
    pub fn lock_for(templates: &[Template]) -> LibraryLock {
        LibraryLock {
            schema_version: 1,
            templates: templates
                .iter()
                .map(|t| {
                    (
                        t.id().to_owned(),
                        LockEntry {
                            version: t.manifest.version.clone(),
                            digest: t.digest.clone(),
                        },
                    )
                })
                .collect(),
        }
    }

    pub fn get(&self, id: &str) -> Option<&Template> {
        self.templates.get(id)
    }

    pub fn templates(&self) -> impl Iterator<Item = &Template> {
        self.templates.values()
    }

    pub fn len(&self) -> usize {
        self.templates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }
}
