use std::{env, fs, path::PathBuf};

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let resources = root.join("resources/photos");
    println!("cargo:rerun-if-changed=resources/photos");
    println!("cargo:rerun-if-changed=resources/catalog.json");
    let mut files: Vec<_> = fs::read_dir(resources)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    let mut code =
        String::from("pub fn photo_bytes(file: &str) -> Option<&'static [u8]> { match file {\n");
    for file in files {
        code.push_str(&format!(
            "{:?} => Some(include_bytes!({:?})),\n",
            file.file_name().unwrap().to_str().unwrap(),
            file.to_str().unwrap()
        ));
    }
    code.push_str("_ => None } }\n");
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("photo_assets.rs"),
        code,
    )
    .unwrap();
}
