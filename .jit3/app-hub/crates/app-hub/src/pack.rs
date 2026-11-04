//! A bundle as one file, for delivery over HTTP.
//!
//! A bundle is a directory, and the digest is defined over the directory
//! ([`octosense_app_policy::digest_dir`]). Serving a directory over HTTP means
//! a request per file; a pack is the same files in one JSON document, so a
//! device fetches once, unpacks into a staging directory, and hashes THAT —
//! the pack itself carries no authority, the unpacked bytes do.
//!
//! Base64 in JSON is not compact, but it needs no archive dependency, is
//! trivially inspectable, and bundles are small by rule (8 MB ceiling).
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const PACK_SCHEMA: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct Pack {
    pub schema: u32,
    /// Relative path → base64 bytes, sorted so a pack is deterministic.
    pub files: BTreeMap<String, String>,
}

/// Pack every file under `root`, the manifest included, so the unpacked
/// directory is the bundle exactly. The bundle's inventory, within the
/// admission limits, comes first: nothing beyond them is read.
pub fn pack_dir(root: &Path) -> Result<Pack, String> {
    let mut files = BTreeMap::new();
    for file in crate::admission::inventory(root)? {
        let bytes = crate::admission::read_bounded(&root.join(&file.path), file.bytes)?;
        // The pack is a bundle by another name: its keys are bundle names, so
        // they carry `/` on every platform, and a pack made on one host
        // unpacks and re-packs to the same digest on another.
        let name = octosense_app_policy::portable_path(&file.path)
            .ok_or("a bundle path is not a plain UTF-8 name")?;
        files.insert(
            name,
            base64::engine::general_purpose::STANDARD.encode(bytes),
        );
    }
    Ok(Pack { schema: PACK_SCHEMA, files })
}

/// Unpack into `into`, a fresh staging directory. Every path and size is
/// checked, and every file decoded, before anything is written: a pack from
/// a hostile origin can neither write outside the staging directory nor
/// grow past the admission limits, whatever it claims. Files are created,
/// never opened, so nothing already in place (a symlink) redirects a write.
pub fn unpack(pack: &Pack, into: &Path) -> Result<(), String> {
    if pack.schema != PACK_SCHEMA {
        return Err(format!("pack schema {} is not {}", pack.schema, PACK_SCHEMA));
    }
    use crate::admission::{MAX_DEPTH, MAX_ENTRIES, MAX_MANIFEST_BYTES};
    if pack.files.len() > MAX_ENTRIES {
        return Err("the pack exceeds the file count limit".into());
    }
    let mut decoded = Vec::new();
    let mut remaining = crate::gate::MAX_BUNDLE_BYTES;
    let mut entries = BTreeSet::new();
    for (name, encoded) in &pack.files {
        crate::admission::safe_relative(name).map_err(|e| format!("the pack names a path it may not: {e}"))?;
        let relative = Path::new(name);
        if relative.components().count() > MAX_DEPTH + 1 {
            return Err("the pack exceeds the directory depth limit".into());
        }
        for path in relative.ancestors().filter(|p| !p.as_os_str().is_empty()) {
            entries.insert(path);
        }
        if entries.len() > MAX_ENTRIES {
            return Err("the pack exceeds the file count limit".into());
        }
        let manifest = name == octosense_app_policy::MANIFEST_FILE;
        let max = if manifest { MAX_MANIFEST_BYTES } else { remaining };
        if encoded.len() as u64 > max.div_ceil(3) * 4 {
            return Err("the pack exceeds the size limit".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|e| format!("{name}: not base64: {e}"))?;
        if bytes.len() as u64 > max {
            return Err("the pack exceeds the size limit".into());
        }
        if !manifest {
            remaining -= bytes.len() as u64;
        }
        decoded.push((relative, bytes));
    }
    if let Ok(meta) = std::fs::symlink_metadata(into) {
        if !meta.is_dir() || std::fs::read_dir(into).map_err(|e| e.to_string())?.next().is_some() {
            return Err("unpack needs an empty, real staging directory".into());
        }
    }
    std::fs::create_dir_all(into).map_err(|e| e.to_string())?;
    for (relative, bytes) in decoded {
        let target = into.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        use std::io::Write;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .and_then(|mut f| f.write_all(&bytes))
            .map_err(|e| format!("{}: {e}", target.display()))?;
    }
    Ok(())
}

/// A system app packed for a shell's build script: the bundle directory
/// `dir` with the digest of exactly its files stamped into the packed
/// manifest (the source manifest leaves it empty), plus the id and name a
/// launcher shows. `include_str!` the JSON and register it as a SystemApp.
pub struct PackedSystemApp {
    pub id: String,
    pub name: String,
    pub pack_json: String,
}

pub fn pack_system_app(dir: &Path) -> Result<PackedSystemApp, String> {
    let mut pack = pack_dir(dir)?;
    let digest = octosense_app_policy::digest_dir(dir)?;
    let encoded = pack
        .files
        .get(octosense_app_policy::MANIFEST_FILE)
        .ok_or_else(|| format!("{}: no {}", dir.display(), octosense_app_policy::MANIFEST_FILE))?;
    let raw = base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|e| e.to_string())?;
    let mut manifest: serde_json::Value = serde_json::from_slice(&raw).map_err(|e| format!("manifest: {e}"))?;
    manifest["integrity"]["bundle_blake3"] = serde_json::Value::String(digest);
    let id = manifest["id"].as_str().ok_or("manifest has no id")?.to_string();
    let name = manifest["name"].as_str().ok_or("manifest has no name")?.to_string();
    let stamped = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    pack.files.insert(
        octosense_app_policy::MANIFEST_FILE.to_string(),
        base64::engine::general_purpose::STANDARD.encode(stamped),
    );
    Ok(PackedSystemApp { id, name, pack_json: serde_json::to_string(&pack).map_err(|e| e.to_string())? })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("app-hub-pack-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("kit")).unwrap();
        std::fs::write(dir.join("manifest.json"), b"{}").unwrap();
        std::fs::write(dir.join("page.card"), b"card").unwrap();
        std::fs::write(dir.join("kit/kit.json"), b"{\"k\":1}").unwrap();
        dir
    }

    #[test]
    fn a_pack_round_trips_to_the_same_digest() {
        let dir = scratch("roundtrip");
        let before = octosense_app_policy::digest_dir(&dir).unwrap();
        let pack = pack_dir(&dir).unwrap();
        let out = dir.with_extension("unpacked");
        let _ = std::fs::remove_dir_all(&out);
        unpack(&pack, &out).unwrap();
        assert_eq!(before, octosense_app_policy::digest_dir(&out).unwrap());
        assert_eq!(std::fs::read(out.join("manifest.json")).unwrap(), b"{}");
    }

    #[test]
    fn a_packed_system_app_carries_the_digest_of_its_files() {
        let dir = scratch("system");
        std::fs::write(dir.join("manifest.json"), br#"{"schema":1,"id":"os.demo","version":"1","name":"Demo","integrity":{"bundle_blake3":""}}"#).unwrap();
        let packed = pack_system_app(&dir).unwrap();
        assert_eq!((packed.id.as_str(), packed.name.as_str()), ("os.demo", "Demo"));
        let pack: Pack = serde_json::from_str(&packed.pack_json).unwrap();
        let out = dir.with_extension("system-unpacked");
        let _ = std::fs::remove_dir_all(&out);
        unpack(&pack, &out).unwrap();
        let manifest = std::fs::read_to_string(out.join("manifest.json")).unwrap();
        let manifest = octosense_app_policy::AppManifest::parse(&manifest).unwrap();
        assert_eq!(manifest.integrity.bundle_blake3, octosense_app_policy::digest_dir(&out).unwrap());
    }

    #[test]
    fn a_pack_may_not_write_outside_its_staging_directory() {
        let mut pack = Pack { schema: PACK_SCHEMA, files: BTreeMap::new() };
        pack.files.insert("../escape.txt".into(), base64::engine::general_purpose::STANDARD.encode(b"x"));
        let out = std::env::temp_dir().join(format!("app-hub-pack-escape-{}", std::process::id()));
        let err = unpack(&pack, &out).unwrap_err();
        assert!(err.contains("may not"), "{err}");
        pack.files.clear();
        pack.files.insert("/abs.txt".into(), base64::engine::general_purpose::STANDARD.encode(b"x"));
        assert!(unpack(&pack, &out).is_err());
    }
}
