//! Locating the Makepad checkout used by this build's Cargo dependencies.
//!
//! The catalog's process-hosted apps live in the Makepad repository, not in
//! this one. Cargo reports either the shared runtime's path overrides or a
//! cached Git checkout. Ask it which source this build uses so hosted apps
//! use the same framework as the host.

use makepad_strict_json::{self as json, Value};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// `cargo metadata` reports a deeper graph than `json`'s strict default
/// of 8 admits; 16 clears it with room for future fields.
const METADATA_MAX_DEPTH: u32 = 16;

/// The repository a `cargo metadata` source string points at, named by its
/// final URL segment: `git+https://host/owner/makepad.git?rev=...#...` is
/// `makepad`. Sibling dependencies such as `makepad-diagram-kit` and
/// `Octoscript-Makepad` are different repositories and answer differently.
fn git_repo_name(source: &str) -> Option<String> {
    let url = source.strip_prefix("git+")?;
    let url = url.split(['?', '#']).next()?.trim_end_matches('/');
    let name = url.rsplit('/').next()?;
    Some(name.strip_suffix(".git").unwrap_or(name).to_lowercase())
}

/// Manifests belonging to the pinned Makepad repository, as reported by
/// `cargo metadata`. The repository vendors crates under their own names,
/// so Git membership follows the URL rather than the package name. Path
/// overrides have a null source and are identified by the WM tree instead.
pub fn makepad_manifests(metadata: &[u8]) -> Result<Vec<PathBuf>, String> {
    // Cargo's graph nests one level past the strict default; bound it
    // generously rather than leaving the depth unchecked.
    let value = json::parse_depth(metadata, METADATA_MAX_DEPTH)
        .map_err(|e| format!("invalid cargo metadata: {e}"))?;
    let packages = value
        .get("packages")
        .and_then(Value::as_arr)
        .ok_or("cargo metadata has no packages array")?;
    Ok(packages
        .iter()
        .filter(|package| match package.get("source") {
            Some(Value::Null) => package
                .get("manifest_path")
                .and_then(Value::as_str)
                .and_then(|path| repo_root_for(Path::new(path)))
                .is_some(),
            Some(source) => source
                .as_str()
                .and_then(git_repo_name)
                .is_some_and(|name| name == "makepad"),
            None => false,
        })
        .filter_map(|package| package.get("manifest_path").and_then(Value::as_str))
        .map(PathBuf::from)
        .collect())
}

/// The repository root above one of its manifests. Makepad is identified
/// by the window manager this project forked from: no other checkout in the
/// dependency graph carries `apps/wm`.
pub fn repo_root_for(manifest: &Path) -> Option<PathBuf> {
    manifest
        .ancestors()
        .skip(1)
        .find(|dir| dir.join("apps/wm/Cargo.toml").is_file())
        .map(Path::to_path_buf)
}

/// The pinned Makepad checkout for this build, resolved once per process.
pub fn makepad_root() -> Option<&'static Path> {
    static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    ROOT.get_or_init(|| {
        let project = crate::octosense::paths::project_root()?;
        // Offline on purpose: this binary's framework sources are already
        // on disk, either prepared by setup or fetched by Cargo. Never
        // reach the network on the way to opening a menu.
        let output = std::process::Command::new("cargo")
            .args([
                "metadata",
                "--format-version",
                "1",
                "--locked",
                "--offline",
                "--manifest-path",
            ])
            .arg(project.join("Cargo.toml"))
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        makepad_manifests(&output.stdout)
            .ok()?
            .iter()
            .find_map(|manifest| repo_root_for(manifest))
    })
    .as_deref()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three dependencies carry "makepad" in their URL and only one is the
    /// repository the catalog launches from. The repository also vendors
    /// crates under unrelated names, so selection follows the git URL and
    /// never the package name.
    #[test]
    fn the_pinned_makepad_checkout_is_told_apart_from_similar_dependencies() {
        let metadata = br#"{"packages":[
            {"name":"makepad-diagram-kit",
             "source":"git+https://github.com/Project-Robius-China/makepad-diagram-kit.git?rev=0536492#0536492",
             "manifest_path":"/cargo/checkouts/makepad-diagram-kit-895b/0536492/Cargo.toml"},
            {"name":"octoscript-makepad",
             "source":"git+https://github.com/OctoSense-org/Octoscript-Makepad.git?rev=abc123#abc123",
             "manifest_path":"/cargo/checkouts/Octoscript-Makepad-1234/abc123/Cargo.toml"},
            {"name":"ab_glyph_rasterizer",
             "source":"git+https://github.com/OctoSense-org/makepad.git?rev=ad8f3729d#ad8f3729d",
             "manifest_path":"/cargo/checkouts/makepad-d00a/ad8f372/libs/ab_glyph_rasterizer/Cargo.toml"},
            {"name":"octosense","source":null,"manifest_path":"/project/Cargo.toml"}
        ]}"#;
        assert_eq!(
            makepad_manifests(metadata).unwrap(),
            [PathBuf::from(
                "/cargo/checkouts/makepad-d00a/ad8f372/libs/ab_glyph_rasterizer/Cargo.toml"
            )]
        );
    }

    #[test]
    fn the_shared_runtime_path_override_is_found() {
        let tmp =
            std::env::temp_dir().join(format!("octosense-makepad-path-{}", std::process::id()));
        let root = tmp.join("shared-framework");
        std::fs::create_dir_all(root.join("apps/wm")).unwrap();
        std::fs::write(root.join("apps/wm/Cargo.toml"), b"[package]").unwrap();
        let manifest = root.join("widgets/Cargo.toml");
        let metadata = format!(
            r#"{{"packages":[
                {{"name":"makepad-widgets","source":null,"manifest_path":{}}},
                {{"name":"octosense","source":null,"manifest_path":{}}},
                {{"name":"makepad-diagram-kit","source":null,"manifest_path":{}}}
            ]}}"#,
            json::s(manifest.to_string_lossy()).to_json(),
            json::s(tmp.join("app/Cargo.toml").to_string_lossy()).to_json(),
            json::s(tmp.join("makepad-diagram-kit/Cargo.toml").to_string_lossy()).to_json(),
        );
        let manifests = makepad_manifests(metadata.as_bytes()).unwrap();
        std::fs::remove_dir_all(&tmp).unwrap();
        assert_eq!(manifests, [manifest]);
    }

    /// A vendored crate sits several directories below the checkout root;
    /// the launcher needs the root itself to reach `apps/`.
    #[test]
    fn the_repository_root_is_the_tree_holding_the_wm_this_project_forked_from() {
        let tmp =
            std::env::temp_dir().join(format!("octosense-makepad-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("checkouts/makepad-d00a/ad8f372");
        std::fs::create_dir_all(root.join("apps/wm")).unwrap();
        std::fs::create_dir_all(root.join("libs/ab_glyph_rasterizer")).unwrap();
        std::fs::write(root.join("Cargo.toml"), b"[workspace]").unwrap();
        std::fs::write(root.join("apps/wm/Cargo.toml"), b"[package]").unwrap();
        let manifest = root.join("libs/ab_glyph_rasterizer/Cargo.toml");
        std::fs::write(&manifest, b"[package]").unwrap();

        assert_eq!(repo_root_for(&manifest).as_deref(), Some(root.as_path()));

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// This project also has a root manifest and an `apps/` directory, so a
    /// looser search would find it instead of the Makepad checkout and
    /// launch every catalog row against the wrong tree.
    #[test]
    fn this_project_is_not_mistaken_for_the_makepad_checkout() {
        let tmp =
            std::env::temp_dir().join(format!("octosense-not-makepad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("apps/reference")).unwrap();
        std::fs::write(tmp.join("Cargo.toml"), b"[workspace]").unwrap();
        let manifest = tmp.join("apps/reference/Cargo.toml");
        std::fs::write(&manifest, b"[package]").unwrap();

        assert_eq!(repo_root_for(&manifest), None);

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// The end-to-end path against this project's own dependency graph:
    /// building these tests already required the framework sources, so the
    /// catalog must resolve them whether they are Git or path dependencies.
    #[test]
    fn the_checkout_cargo_uses_for_this_build_is_found() {
        let root = makepad_root().expect("cargo resolved the build's makepad checkout");
        assert!(
            root.join("apps/wm/Cargo.toml").is_file(),
            "{}",
            root.display()
        );
        assert!(
            root.join("apps/browser/Cargo.toml").is_file(),
            "{}",
            root.display()
        );
    }
}
