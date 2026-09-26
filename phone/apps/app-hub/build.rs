//! Embed the native app's canonical icon using the Hub listing path convention.
//! Built-ins consume the icon declaration only; publishing a Card bundle still
//! requires the complete Hub listing and manifest, validated by the Hub gate.
//!
//! Also pack the system apps the shell includes (ADR 0004). Their bundles live
//! with their apps in OctoSense-System-Apps, `apps/<name>/bundle/`, pinned by
//! `native-apps.lock.json`; the shell's `system-apps.json` names which to
//! include and mounts artwork the shell owns:
//! `{"source": "../.sources/system-apps/apps", "apps": ["news"],
//!   "assets": {"photos": {"photos": "apps/photos/resources/photos"}}}`.
//! Each bundle becomes a pack with its digest stamped; each asset directory is
//! compiled in as static artwork served at `<prefix>/<file>`.
//! `OCTOSENSE_SYSTEM_APPS` points at another selection file.
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
    let crate_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let shell = crate_dir.join("../..").canonicalize().expect("shell root");
    println!("cargo:rerun-if-env-changed=OCTOSENSE_SYSTEM_APPS");
    let selection_path = std::env::var_os("OCTOSENSE_SYSTEM_APPS").map(PathBuf::from).unwrap_or_else(|| shell.join("system-apps.json"));
    println!("cargo:rerun-if-changed={}", selection_path.display());
    let selection: serde_json::Value = std::fs::read(&selection_path)
        .map(|bytes| serde_json::from_slice(&bytes).expect("parse system-apps.json"))
        .unwrap_or_else(|_| serde_json::json!({"apps": []}));
    let base = selection_path.parent().unwrap_or(&shell).to_path_buf();
    let source = base.join(selection["source"].as_str().unwrap_or("."));
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let names: Vec<String> = selection["apps"].as_array().map(|a| a.iter().filter_map(|n| n.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let dirs: Vec<(String, PathBuf)> = names.into_iter().map(|name| {
        let dir = source.join(&name).join("bundle");
        assert!(dir.join("manifest.json").is_file(), "system app {name}: no bundle at {} (run scripts/setup-home.py)", dir.display());
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

fn watch(dir: &Path) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        println!("cargo:rerun-if-changed={}", entry.path().display());
        if entry.path().is_dir() {
            watch(&entry.path());
        }
    }
}
