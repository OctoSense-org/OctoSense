//! Embed the native app's canonical icon using the Hub listing path convention.
//! Built-ins consume the icon declaration only; publishing a Card bundle still
//! requires the complete Hub listing and manifest, validated by the Hub gate.
use std::path::{Component, PathBuf};

fn main() {
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
