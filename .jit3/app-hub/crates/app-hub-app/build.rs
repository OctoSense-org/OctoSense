//! Embed the native app's canonical icon using the Hub listing path convention.
//! Built-ins consume the icon declaration only; publishing a Card bundle still
//! requires the complete Hub listing and manifest, validated by the Hub gate.
//!
//! Also pack the system apps the shell includes (ADR 0004). Their bundles live
//! with their apps in OctoSense, `apps/<name>/bundle/`; the shell's
//! selection file names which to include and mounts artwork the shell owns:
//! `{"source": "../apps", "apps": ["news"],
//!   "assets": {"photos": {"photos": "../apps/photos/native/resources/photos"}}}`.
//! `source` and asset paths are relative to the selection file's directory.
//! Each bundle becomes a pack with its digest stamped; each asset directory is
//! compiled in as static artwork served at `<prefix>/<file>`.
//!
//! The selection file is named by `OCTOSENSE_SYSTEM_APPS`. This crate is
//! linked by several shells, usually as a git dependency, so its own location
//! says nothing about which shell is building it. A shell sets, in its
//! `.cargo/config.toml`:
//! `[env] OCTOSENSE_SYSTEM_APPS = { value = "system-apps.json", relative = true }`
//! which cargo hands over as an absolute path. A relative value set some other
//! way resolves against the consuming workspace (the directory holding the
//! `Cargo.lock` above the build's target directory). Unset, the build ships no
//! system apps and says so with a `cargo:warning`.
use std::path::{Component, Path, PathBuf};

fn main() {
    system_apps();
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    println!("cargo:rerun-if-changed=listing.json");
    let listing: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("listing.json")).expect("read App Hub listing"),
    ).expect("parse App Hub listing");
    assert_eq!(listing["schema"].as_u64(), Some(1));
    let path = PathBuf::from(listing["icon"].as_str().expect("listing.icon"));
    assert!(path.components().all(|part| matches!(part, Component::Normal(_))));
    assert_eq!(path.extension().and_then(|s| s.to_str()), Some("svg"));
    let source = root.join(&path).canonicalize().expect("canonical icon exists");
    assert!(source.starts_with(root.canonicalize().unwrap()));
    println!("cargo:rerun-if-changed={}", path.display());
    let output = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("app_icon.rs");
    std::fs::write(output, format!(
        "pub const APP_ICON_SVG: &str = include_str!({:?});\n", source.to_str().unwrap(),
    )).unwrap();
}

fn system_apps() {
    println!("cargo:rerun-if-env-changed=OCTOSENSE_SYSTEM_APPS");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let Some(selection_path) = selection_path(&out) else {
        println!("cargo:warning=OCTOSENSE_SYSTEM_APPS is not set: building with no system apps. A shell sets it in .cargo/config.toml: [env] OCTOSENSE_SYSTEM_APPS = {{ value = \"system-apps.json\", relative = true }}");
        std::fs::write(out.join("system_apps.rs"), "pub fn register_system_apps() {}\npub const SYSTEM_ICONS: &[(&str, bool, &[u8])] = &[];\n").unwrap();
        return;
    };
    println!("cargo:rerun-if-changed={}", selection_path.display());
    let bytes = std::fs::read(&selection_path).unwrap_or_else(|err| panic!("OCTOSENSE_SYSTEM_APPS: read {}: {err}", selection_path.display()));
    let selection: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|err| panic!("OCTOSENSE_SYSTEM_APPS: parse {}: {err}", selection_path.display()));
    let base = selection_path.parent().expect("selection file directory").to_path_buf();
    let source = base.join(selection["source"].as_str().unwrap_or("."));
    let names: Vec<String> = selection["apps"].as_array().map(|a| a.iter().filter_map(|n| n.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let dirs: Vec<(String, PathBuf)> = names.into_iter().map(|name| {
        let dir = source.join(&name).join("bundle");
        assert!(dir.join("manifest.json").is_file(), "system app {name}: no bundle at {} (check `source` in {})", dir.display(), selection_path.display());
        (name, dir)
    }).collect();
    let mut code = String::from("pub fn register_system_apps() {\n");
    // Launcher art a system app ships in its bundle, by its short id.
    let mut icons = String::from("pub const SYSTEM_ICONS: &[(&str, bool, &[u8])] = &[\n");
    for (name, dir) in dirs {
        watch(&dir);
        let packed = octosense_app_hub::pack::pack_system_app(&dir).expect("pack system app");
        let pack_path = out.join(format!("system-{name}.pack.json"));
        std::fs::write(&pack_path, &packed.pack_json).unwrap();
        let mut assets = String::new();
        if let Some(mounts) = selection["assets"][&name].as_object() {
            for (prefix, rel) in mounts {
                let dir = base.join(rel.as_str().expect("assets path"));
                println!("cargo:rerun-if-changed={}", dir.display());
                let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).expect("assets dir").flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
                files.sort();
                for file in files {
                    let file_name = file.file_name().unwrap().to_string_lossy().to_string();
                    assets.push_str(&format!("({:?}, include_bytes!({:?}) as &[u8]), ", format!("{prefix}/{file_name}"), file.to_str().unwrap()));
                }
            }
        }
        let short = packed.id.strip_prefix("os.").unwrap_or(&packed.id).to_string();
        for (file, svg) in [("icon.svg", true), ("icon.png", false)] {
            let icon = dir.join(file);
            if icon.is_file() {
                icons.push_str(&format!("    ({short:?}, {svg}, include_bytes!({:?})),\n", icon.to_str().unwrap()));
                break;
            }
        }
        code.push_str(&format!(
            "    octosense_appstore::system::register_system_app(octosense_appstore::system::SystemApp {{ id: {:?}, name: {:?}, pack: include_str!({:?}), assets: &[{assets}] }});\n",
            packed.id, packed.name, pack_path.to_str().unwrap()
        ));
    }
    code.push_str("}\n");
    icons.push_str("];\n");
    code.push_str(&icons);
    std::fs::write(out.join("system_apps.rs"), code).unwrap();
}

/// The selection file: `OCTOSENSE_SYSTEM_APPS` as given when absolute,
/// otherwise against the consuming workspace. `None` when unset.
fn selection_path(out: &Path) -> Option<PathBuf> {
    let value = PathBuf::from(std::env::var_os("OCTOSENSE_SYSTEM_APPS").filter(|v| !v.is_empty())?);
    if value.is_absolute() {
        return Some(value);
    }
    // OUT_DIR is <target>/[<triple>/]<profile>/build/<pkg>/out; the consuming
    // workspace is the nearest ancestor holding a Cargo.lock.
    let workspace = out.ancestors().find(|dir| dir.join("Cargo.lock").is_file()).unwrap_or_else(|| {
        panic!("OCTOSENSE_SYSTEM_APPS={} is relative and the consuming workspace is not above the target directory; set it in .cargo/config.toml with `relative = true`, or give an absolute path", value.display())
    });
    Some(workspace.join(value))
}

fn watch(dir: &Path) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        println!("cargo:rerun-if-changed={}", entry.path().display());
        if entry.path().is_dir() {
            watch(&entry.path());
        }
    }
}
