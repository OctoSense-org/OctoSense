//! What a bundle runs: an L0 card, or a script app.
//!
//! A card bundle holds `page.card` (plus `page.data.json` and a `kit/`),
//! which the host lowers to widgets: presentation, no logic. A script app
//! holds [`SCRIPT_ENTRY`], a Splash program with its own state, handlers,
//! requests and storage, which the isolate evaluates as it is. Both run under
//! the same resolved policy; the entry only decides what source the isolate
//! gets.
use std::path::Path;

/// A script app's program, at the bundle root.
pub const SCRIPT_ENTRY: &str = "main.splash";

/// Where a script app's source names its own artwork: the host replaces this
/// with the loopback origin serving the bundle, so the app never learns (or
/// guesses) a URL it was not given.
pub const ASSETS_PLACEHOLDER: &str = "{{assets}}";

/// The isolate source for a script app, or `None` for a bundle that is a
/// card. `asset_origin` is the bundle's loopback origin, with or without a
/// trailing slash.
pub fn script_source(bundle: &Path, asset_origin: &str) -> Option<Result<String, String>> {
    let path = bundle.join(SCRIPT_ENTRY);
    if !path.is_file() {
        return None;
    }
    Some(
        std::fs::read_to_string(&path)
            .map_err(|e| format!("{SCRIPT_ENTRY}: {e}"))
            .map(|source| source.replace(ASSETS_PLACEHOLDER, asset_origin.trim_end_matches('/'))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_app_gets_its_program_with_the_asset_origin_filled_in() {
        let dir = std::env::temp_dir().join(format!("app-contract-entry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(script_source(&dir, "http://127.0.0.1:5/").is_none(), "no program: a card");
        std::fs::write(dir.join(SCRIPT_ENTRY), "Image{src: http_resource(\"{{assets}}/a.png\")}").unwrap();
        assert_eq!(
            script_source(&dir, "http://127.0.0.1:5/").unwrap().unwrap(),
            "Image{src: http_resource(\"http://127.0.0.1:5/a.png\")}"
        );
    }
}
