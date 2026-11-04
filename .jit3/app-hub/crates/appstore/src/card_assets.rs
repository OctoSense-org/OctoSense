//! Resolve bundle resources after L0 kit styles and tokens become typed nodes.
use std::path::Path;

/// Prepare an admitted bundle's card for either native lowering. `origin` must
/// be the host's AssetServer for this bundle, never a URL supplied by the app.
/// Data rewriting alone misses literal and token-based resources in kit.json.
pub fn prepare(
    card: &str,
    data: &serde_json::Value,
    bundle: &Path,
    origin: &str,
) -> Result<octoscript_makepad::l0::PreparedCard, String> {
    let mut prepared = octoscript_makepad::l0::prepare(card, data, &bundle.join("kit"))?;
    let root = bundle.canonicalize().map_err(|e| format!("bundle assets: {e}"))?;
    let mut nodes = vec![&mut prepared.tree];
    while let Some(node) = nodes.pop() {
        for (field, extensions) in [
            (&mut node.attrs.font_src, &["ttf", "otf"] as &[&str]),
            (&mut node.attrs.src, &["svg", "png", "jpg", "jpeg", "webp"] as &[&str]),
        ] {
            if let Some(path) = field {
                if let Some(url) = bundle_asset(&root, origin, path, extensions) {
                    *path = url;
                }
            }
        }
        nodes.extend(node.children.iter_mut());
    }
    Ok(prepared)
}

fn bundle_asset(root: &Path, origin: &str, path: &str, extensions: &[&str]) -> Option<String> {
    // Keep built-in crate fonts and already-resolved host URLs unchanged.
    // Never turn an absolute path, traversal, or an external link into a load.
    octosense_app_hub::admission::safe_relative(path).ok()?;
    let extension = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
    if !extensions.contains(&extension.as_str()) { return None; }
    let file = root.join(path).canonicalize().ok()?;
    if !file.starts_with(root) || !file.is_file() { return None; }
    // AssetServer decodes URI paths. Encode spaces, Unicode and URL delimiters
    // so one bundled filename cannot become a query, fragment or another path.
    let mut encoded = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~/".contains(&byte) {
            encoded.push(byte as char);
        } else {
            use std::fmt::Write;
            write!(encoded, "%{byte:02X}").unwrap();
        }
    }
    Some(format!("{}/{encoded}", origin.trim_end_matches('/')))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!("card-font-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
            std::fs::create_dir_all(root.join("kit/native/light")).unwrap();
            std::fs::create_dir_all(root.join("assets")).unwrap();
            std::fs::write(root.join("assets/Body.ttf"), b"font fixture").unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

    #[test]
    fn kit_literal_and_token_fonts_use_the_host_origin_in_both_lowerings() {
        let fixture = Fixture::new();
        let card = include_str!("../../app-hub/tests/fixtures/card/page.card").replace("Focus Timer", "assets/Body.ttf");
        let data: serde_json::Value = serde_json::from_str(include_str!("../../app-hub/tests/fixtures/card/page.data.json")).unwrap();
        let mut kit: serde_json::Value = serde_json::from_str(include_str!("../../app-hub/tests/fixtures/card/kit/native/light/kit.json")).unwrap();
        kit["components"]["title"]["style"]["font_src"] = json!("assets/Body.ttf");
        kit["components"]["subtitle"]["style"]["font_src"] = json!({"$token":"body_font"});
        kit["tokens"]["body_font"] = json!({"type":"font","value":"assets/Body.ttf"});
        std::fs::write(fixture.0.join("kit/native/light/kit.json"), kit.to_string()).unwrap();
        let prepared = prepare(&card, &data, &fixture.0, "http://127.0.0.1:12345/").unwrap();
        for source in [octoscript_makepad::design::to_makepad_ui(&prepared.tree).unwrap(), octoscript_makepad::to_makepad_l0_ui(&prepared.tree)] {
            assert!(source.matches("http_resource(\"http://127.0.0.1:12345/assets/Body.ttf\")").count() >= 2, "literal and resolved token must reach the HTTP font loader");
            assert!(!source.contains("crate_resource(\"assets/Body.ttf\")"));
            assert!(source.contains("text: \"assets/Body.ttf\""), "display text is never rewritten as a resource");
            assert!(source.contains("crate_resource(\"makepad_widgets:resources/Inter.ttf\")"));
        }
        assert!(crate::card_source(&fixture.0, "http://127.0.0.1:12345/").is_err(), "no unrelated page is created or guessed");
        std::fs::write(fixture.0.join("page.card"), &card).unwrap();
        std::fs::write(fixture.0.join("page.data.json"), data.to_string()).unwrap();
        assert!(crate::card_source(&fixture.0, "http://127.0.0.1:12345/").unwrap().contains("http_resource(\"http://127.0.0.1:12345/assets/Body.ttf\")"));
    }

    #[test]
    fn only_existing_safe_resources_are_rewritten_and_names_are_encoded() {
        let fixture = Fixture::new();
        let root = fixture.0.canonicalize().unwrap();
        let rewrite = |path| bundle_asset(&root, "http://127.0.0.1:12345/", path, &["ttf", "otf"]);
        for path in ["makepad_widgets:resources/Inter.ttf", "https://example.test/font.ttf", "/tmp/font.ttf", "../Body.ttf", "assets/missing.ttf", "page.data.json"] {
            assert!(rewrite(path).is_none(), "must not rewrite {path}");
        }
        std::fs::write(root.join("assets/中文 #%.ttf"), b"font").unwrap();
        assert_eq!(rewrite("assets/中文 #%.ttf").as_deref(), Some("http://127.0.0.1:12345/assets/%E4%B8%AD%E6%96%87%20%23%25.ttf"));
        #[cfg(unix)] {
            let other = Fixture::new();
            std::os::unix::fs::symlink(&other.0, root.join("outside")).unwrap();
            assert!(rewrite("outside/assets/Body.ttf").is_none());
        }
    }
}
