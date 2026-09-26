//! Read a provider QR out of a picture the way the import sheet does, for
//! scripts (the desktop's remote e2e run reads the export sheet's grab with
//! it). Keys are never printed, only their last four characters.
//!
//!   cargo run --example read_qr_image -- <image.png|jpg> [PIN [CORE_DIR]]
//!
//! Without a PIN it prints the code's format. With one it opens the code and
//! prints the provider set and masked keys. With a core dir it also compares
//! them to `<core_dir>/profiles/_main.json` (keys in the profile, as with
//! `OCTOSENSE_LLM_VAULT=file`) and exits 1 unless they are the same.
use octosense_llm_config::{profile, qr};
use octosense_llm_service::image_qr::find_code;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::exit;

fn masked(secrets: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    secrets
        .iter()
        .map(|(env, key)| {
            let tail: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
            (env.clone(), format!("••••{tail}"))
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(image) = args.first() else {
        eprintln!("usage: read_qr_image <image> [PIN [CORE_DIR]]");
        exit(2);
    };
    let bytes = std::fs::read(image).unwrap_or_else(|e| {
        eprintln!("{image}: {e}");
        exit(2)
    });
    let code = find_code(&bytes).unwrap_or_else(|e| {
        eprintln!("{e}");
        exit(1)
    });
    let format = format!("{:?}", qr::format_of(&code).unwrap());
    let Some(pin) = args.get(1) else {
        println!("{}", json!({"format": format, "chars": code.len()}));
        return;
    };
    let opened = qr::decode(&code, Some(pin)).unwrap_or_else(|e| {
        eprintln!("{e}");
        exit(1)
    });
    let mut out = json!({"format": format, "set": opened.set, "keys": masked(&opened.secrets)});
    if let Some(dir) = args.get(2) {
        let saved = profile::load(&profile::profile_path(&PathBuf::from(dir))).unwrap_or_else(|e| {
            eprintln!("{e}");
            exit(2)
        });
        let keys_match = opened.secrets.iter().all(|(env, key)| saved.env_vars.get(env) == Some(key));
        let same = saved.set == opened.set && keys_match;
        out["same_as_profile"] = json!(same);
        println!("{out}");
        exit(if same { 0 } else { 1 });
    }
    println!("{out}");
}
