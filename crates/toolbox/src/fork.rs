//! Forks: an app's own variant of a library template, in the app's folder,
//! with lineage. A fork passes the same static check as a library template
//! and can never declare a module, a method or a budget beyond its parent's.

use crate::library::{Library, Template, TemplateOrigin, MANIFEST_FILE, SOURCE_FILE};
use crate::manifest::{valid_id, Lineage, Manifest};
use crate::{Error, ErrorKind, Result};
use std::path::Path;

/// Copies library template `parent_id` into `templates_dir/<new_id>/` with
/// lineage `{parent id, version, digest}` and returns the loaded fork. The
/// default id is `<parent_id>.fork`.
pub fn fork(
    library: &Library,
    parent_id: &str,
    templates_dir: &Path,
    new_id: Option<&str>,
) -> Result<Template> {
    let parent = library
        .get(parent_id)
        .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("no template {parent_id}")))?;
    let id = new_id
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{parent_id}.fork"));
    if !valid_id(&id) || library.get(&id).is_some() {
        return Err(Error::new(
            ErrorKind::Manifest,
            format!("{id:?} cannot name a fork"),
        ));
    }
    let dir = templates_dir.join(&id);
    if dir.exists() {
        return Err(Error::new(
            ErrorKind::Io,
            format!("a fork named {id} already exists"),
        ));
    }
    let mut manifest = parent.manifest.clone();
    manifest.id = id;
    manifest.lineage = Some(Lineage {
        parent_id: parent.manifest.id.clone(),
        parent_version: parent.manifest.version.clone(),
        parent_digest: parent.digest.clone(),
    });
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| Error::new(ErrorKind::Manifest, e.to_string()))?
        + "\n";
    let io = |e: std::io::Error| Error::new(ErrorKind::Io, format!("{}: {e}", dir.display()));
    std::fs::create_dir_all(&dir).map_err(io)?;
    std::fs::write(dir.join(MANIFEST_FILE), text).map_err(io)?;
    std::fs::write(dir.join(SOURCE_FILE), &parent.source).map_err(io)?;
    load_fork(library, &dir)
}

/// Loads an (edited) fork from its directory and checks it against its
/// parent in `library`.
pub fn load_fork(library: &Library, dir: &Path) -> Result<Template> {
    let template = Template::load_dir(
        dir,
        TemplateOrigin::Fork {
            path: dir.to_path_buf(),
        },
    )?;
    check_against_parent(library, &template.manifest)?;
    Ok(template)
}

/// Refuses a fork that is not a fork of a library template, or that widens
/// its parent: a module or method the parent does not declare, or a budget
/// field above the parent's.
pub fn check_against_parent(library: &Library, manifest: &Manifest) -> Result<()> {
    let Some(lineage) = &manifest.lineage else {
        return Err(Error::new(
            ErrorKind::Manifest,
            "an app-local template must be a fork (lineage) of a library template",
        ));
    };
    if library.get(&manifest.id).is_some() {
        return Err(Error::new(
            ErrorKind::Manifest,
            format!("a fork may not take the library id {}", manifest.id),
        ));
    }
    let parent = library.get(&lineage.parent_id).ok_or_else(|| {
        Error::new(
            ErrorKind::NotFound,
            format!("parent {} is not in the library", lineage.parent_id),
        )
    })?;
    let parent_methods = parent.manifest.declared_methods();
    let widened: Vec<String> = manifest
        .declared_methods()
        .difference(&parent_methods)
        .map(|(module, method)| format!("{module}.{method}"))
        .collect();
    if !widened.is_empty() {
        return Err(Error::new(
            ErrorKind::Widening,
            format!(
                "the fork declares what {} does not: {}",
                parent.manifest.id,
                widened.join(", ")
            ),
        ));
    }
    let over = manifest.budget.exceeds(&parent.manifest.budget);
    if !over.is_empty() {
        return Err(Error::new(
            ErrorKind::Widening,
            format!(
                "the fork's budget exceeds {}'s: {}",
                parent.manifest.id,
                over.join(", ")
            ),
        ));
    }
    if parent.manifest.provenance && !manifest.provenance {
        return Err(Error::new(
            ErrorKind::Widening,
            "a fork may not turn provenance off",
        ));
    }
    Ok(())
}

/// Every fork in an app's templates directory. Forks that fail to load are
/// returned as errors next to their directory name, not dropped silently.
pub fn list_forks(library: &Library, templates_dir: &Path) -> Vec<(String, Result<Template>)> {
    let Ok(entries) = std::fs::read_dir(templates_dir) else {
        return Vec::new();
    };
    let mut forks: Vec<(String, Result<Template>)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                load_fork(library, &e.path()),
            )
        })
        .collect();
    forks.sort_by(|a, b| a.0.cmp(&b.0));
    forks
}

/// True when the fork's parent in `library` still has the digest the fork
/// was copied from.
pub fn lineage_current(library: &Library, template: &Template) -> Option<bool> {
    let lineage = template.manifest.lineage.as_ref()?;
    Some(
        library
            .get(&lineage.parent_id)
            .is_some_and(|p| p.digest == lineage.parent_digest),
    )
}
