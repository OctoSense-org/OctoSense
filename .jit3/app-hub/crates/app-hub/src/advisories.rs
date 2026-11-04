//! RustSec advisories for the crates a component lists
//! ([`crate::functions::CRATES_SECTION`]): `hub check --advisory-db <dir>`,
//! with `<dir>` a checkout of <https://github.com/rustsec/advisory-db>.
//!
//! An advisory is `crates/<crate>/<id>.md`: TOML front matter in a ```` ```toml ````
//! block, then a Markdown body whose first heading is its title. It affects
//! a version that none of its `patched` and `unaffected` requirements match;
//! a withdrawn advisory affects nothing. An informational one (unmaintained,
//! unsound, a notice) is reported as what it is, not as a vulnerability.
//! The gate cannot fetch the database itself: it runs where a reviewer
//! has one.

use std::path::Path;

use semver::{Version, VersionReq};
use serde::Deserialize;

use crate::gate::{Finding, Severity};

/// The largest advisory file read.
const MAX_ADVISORY_BYTES: u64 = 256 << 10;

/// One advisory that affects a crate's version.
#[derive(Clone, Debug, PartialEq)]
pub struct Advisory {
    pub id: String,
    pub title: String,
    /// `None` for a vulnerability; `unmaintained`, `unsound`, `notice`, … for
    /// an informational advisory.
    pub informational: Option<String>,
    /// The fixed versions, as the advisory writes them.
    pub patched: Vec<String>,
}

#[derive(Deserialize)]
struct Front {
    advisory: Meta,
    #[serde(default)]
    versions: Versions,
}

#[derive(Deserialize)]
struct Meta {
    id: String,
    package: String,
    #[serde(default)]
    informational: Option<String>,
    #[serde(default)]
    withdrawn: Option<toml::Value>,
}

#[derive(Default, Deserialize)]
struct Versions {
    #[serde(default)]
    patched: Vec<String>,
    #[serde(default)]
    unaffected: Vec<String>,
}

/// The advisories in `db` that affect `name` at `version`.
pub fn for_crate(db: &Path, name: &str, version: &str) -> Result<Vec<Advisory>, String> {
    // A crate name never leaves the database's crates/ folder.
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err(format!("{name:?} is not a crate name"));
    }
    let version = Version::parse(version).map_err(|e| format!("{name} {version}: {e}"))?;
    let dir = db.join("crates").join(name);
    let Ok(entries) = std::fs::read_dir(&dir) else { return Ok(Vec::new()) };
    let mut found = Vec::new();
    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "md")).collect();
    paths.sort();
    for path in paths {
        let text = crate::admission::read_bounded(&path, MAX_ADVISORY_BYTES)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let text = String::from_utf8_lossy(&text);
        let advisory = parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Some((front, title)) = advisory {
            if front.advisory.package != name || front.advisory.withdrawn.is_some() {
                continue;
            }
            if affects(&front.versions, &version)? {
                found.push(Advisory {
                    id: front.advisory.id,
                    title,
                    informational: front.advisory.informational,
                    patched: front.versions.patched,
                });
            }
        }
    }
    Ok(found)
}

/// The front matter and the title, or `None` for a file with no front
/// matter (the database's README, say).
fn parse(text: &str) -> Result<Option<(Front, String)>, String> {
    let Some(start) = text.find("```toml") else { return Ok(None) };
    let body = &text[start + "```toml".len()..];
    let end = body.find("\n```").ok_or("its front matter does not end")?;
    let front: Front = toml::from_str(&body[..end]).map_err(|e| e.to_string())?;
    let title = body[end + 4..]
        .lines()
        .find_map(|line| line.strip_prefix("# "))
        .unwrap_or("")
        .trim()
        .to_string();
    Ok(Some((front, title)))
}

fn affects(versions: &Versions, version: &Version) -> Result<bool, String> {
    for req in versions.patched.iter().chain(&versions.unaffected) {
        let req = VersionReq::parse(req).map_err(|e| format!("{req:?}: {e}"))?;
        if req.matches(version) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A warning for every listed crate from crates.io that an advisory in
/// `db` affects, in every component of `bundle`'s `fns/`.
pub fn findings(bundle: &Path, db: &Path) -> Result<Vec<Finding>, String> {
    if !db.join("crates").is_dir() {
        return Err(format!("{} is not a RustSec advisory database (it has no crates/ folder)", db.display()));
    }
    let mut findings = Vec::new();
    let Ok(entries) = std::fs::read_dir(bundle.join("fns")) else { return Ok(findings) };
    let mut files: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "wasm")).collect();
    files.sort();
    for file in files {
        let bytes = crate::admission::read_bounded(&file, crate::gate::MAX_BUNDLE_BYTES)?;
        if !crate::functions::is_component(&bytes) {
            continue;
        }
        let name = format!("fns/{}", file.file_name().unwrap_or_default().to_string_lossy());
        let Some(crates) = crate::functions::inspect_component(&bytes)?.crates else { continue };
        for listed in crates.iter().filter(|c| c.source == "crates.io") {
            for advisory in for_crate(db, &listed.name, &listed.version)? {
                let what = match &advisory.informational {
                    Some(kind) => format!("an informational advisory ({kind})"),
                    None => "a vulnerability".to_string(),
                };
                let fixed = if advisory.patched.is_empty() {
                    "no fixed version".to_string()
                } else {
                    format!("fixed in {}", advisory.patched.join(" or "))
                };
                findings.push(Finding {
                    severity: Severity::Warning,
                    check: "advisories",
                    detail: format!(
                        "{name} includes {} {}, which {} reports as {what}: {}; {fixed}",
                        listed.name, listed.version, advisory.id, advisory.title
                    ),
                    path: Some(name.clone()),
                });
            }
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advisory(dir: &Path, name: &str, id: &str, extra: &str, versions: &str) {
        let folder = dir.join("crates").join(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join(format!("{id}.md")),
            format!("```toml\n[advisory]\nid = \"{id}\"\npackage = \"{name}\"\ndate = 2024-01-02\n{extra}\n[versions]\n{versions}\n```\n\n# Title of {id}\n\nBody.\n"),
        )
        .unwrap();
    }

    #[test]
    fn an_advisory_affects_what_it_does_not_patch_or_spare() {
        let db = std::env::temp_dir().join(format!("advisory-db-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&db);
        advisory(&db, "demo", "RUSTSEC-0000-0001", "", "patched = [\">= 1.2.3\"]\nunaffected = [\"< 1.0.0\"]");
        advisory(&db, "demo", "RUSTSEC-0000-0002", "informational = \"unmaintained\"", "patched = []");
        advisory(&db, "demo", "RUSTSEC-0000-0003", "withdrawn = 2024-02-03", "patched = []");
        let ids = |version: &str| -> Vec<String> {
            for_crate(&db, "demo", version).unwrap().into_iter().map(|a| a.id).collect()
        };
        assert_eq!(ids("1.2.0"), ["RUSTSEC-0000-0001", "RUSTSEC-0000-0002"]);
        assert_eq!(ids("1.2.3"), ["RUSTSEC-0000-0002"]);
        assert_eq!(ids("0.9.0"), ["RUSTSEC-0000-0002"]);
        let found = for_crate(&db, "demo", "1.2.0").unwrap();
        assert_eq!(found[0].title, "Title of RUSTSEC-0000-0001");
        assert_eq!(found[1].informational.as_deref(), Some("unmaintained"));
        assert!(for_crate(&db, "other", "1.0.0").unwrap().is_empty());
        assert!(for_crate(&db, "../demo", "1.0.0").is_err());
        let _ = std::fs::remove_dir_all(&db);
    }
}
