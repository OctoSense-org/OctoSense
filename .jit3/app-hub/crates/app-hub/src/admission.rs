//! Bounded, headless structural checks on a bundle (store plan 03).
//!
//! The inventory is taken from metadata before anything is read: file count,
//! depth, portable paths, sizes. Then every file is checked for what its
//! extension claims: text is UTF-8 (and JSON parses), images decode within
//! dimension and allocation limits, SVG loads nothing from outside, and the
//! bundle has an entry the runtime can start, `main.splash` (a script app)
//! or `page.card` with its kit (a card). Executing app code is not done
//! here: that belongs to a runtime validator, never to the hub's process.
use crate::gate::{Finding, MAX_BUNDLE_BYTES};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

pub const MAX_ENTRIES: usize = 2048;
pub const MAX_DEPTH: usize = 32;
pub const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
pub const MAX_TEXT_BYTES: u64 = 1024 * 1024;
pub const MAX_IMAGE_DIMENSION: u32 = 4096;
/// The listing's icon: a square, at most this many pixels a side.
pub const MAX_ICON_DIMENSION: u32 = 1024;
const MAX_DECODED_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct BundleFile {
    pub path: PathBuf,
    pub bytes: u64,
}

/// Every file in the bundle, from metadata alone, within the limits. Empty
/// directories count toward the same entry limit as files.
pub fn inventory(root: &Path) -> Result<Vec<BundleFile>, String> {
    fn walk(root: &Path, dir: &Path, depth: usize, entries: &mut usize, remaining: &mut u64, out: &mut Vec<BundleFile>) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("the bundle exceeds the directory depth limit".into());
        }
        if !fs::symlink_metadata(dir).map_err(|e| e.to_string())?.is_dir() {
            return Err("bundle directories must be real directories, not symlinks".into());
        }
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            *entries += 1;
            if *entries > MAX_ENTRIES {
                return Err("the bundle exceeds the file count limit".into());
            }
            let path = entry.path();
            let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
            // Name it the way a bundle writes it, not the way this host
            // separates paths. Reading it back from the host's `Path` puts a
            // `\` on Windows, which `safe_relative` then refuses - so a
            // Windows hub rejected its own subdirectories (#75).
            let name = octosense_app_policy::portable_path(relative)
                .ok_or("bundle paths must be plain UTF-8 names")?;
            safe_relative(&name)?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                walk(root, &path, depth + 1, entries, remaining, out)?;
            } else if kind.is_file() {
                let bytes = entry.metadata().map_err(|e| e.to_string())?.len();
                if relative == Path::new(octosense_app_policy::MANIFEST_FILE) {
                    if bytes > MAX_MANIFEST_BYTES {
                        return Err("manifest.json exceeds the size limit".into());
                    }
                } else {
                    *remaining = remaining.checked_sub(bytes).ok_or("the bundle exceeds the size limit")?;
                }
                out.push(BundleFile { path: relative.into(), bytes });
            } else {
                return Err(format!("{name}: only regular files and directories are allowed"));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    let mut entries = 0;
    let mut remaining = MAX_BUNDLE_BYTES;
    walk(root, root, 0, &mut entries, &mut remaining, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// A relative path every platform reads the same way.
pub fn safe_relative(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.contains(['\\', ':', '\0'])
        || name.split('/').any(|s| s.is_empty() || s == "." || s == "..")
        || Path::new(name).components().any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!("not a portable bundle path: {name:?}"));
    }
    Ok(())
}

/// A regular file's bytes, refusing more than `max`.
pub fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() || meta.len() > max {
        return Err(format!("{}: not a regular file within the size limit", path.display()));
    }
    let mut bytes = Vec::new();
    fs::File::open(path).map_err(|e| e.to_string())?.take(max + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > max {
        return Err(format!("{}: exceeds the size limit", path.display()));
    }
    Ok(bytes)
}

pub fn read_text(path: &Path, max: u64) -> Result<String, String> {
    String::from_utf8(read_bounded(path, max)?).map_err(|e| format!("{}: invalid UTF-8: {e}", path.display()))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceKind {
    Image,
    Font,
    SvgReference,
    DisplayUrl,
}

#[derive(Clone, Debug, Serialize)]
pub struct ResourceReference {
    pub path: String,
    pub target: String,
    pub kind: ResourceKind,
}

/// JSON pointers name the property a developer has to fix. Display URLs are
/// recorded apart from loads; the gate's textual URL rule still applies.
fn resources(value: &Value, path: &str, out: &mut Vec<ResourceReference>, rendered: bool, tokens: &Value, findings: &mut Vec<Finding>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                let pointer = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                if rendered && key == "font_src" {
                    font_resource(value, &pointer, tokens, out, findings);
                    continue;
                }
                if let Some(target) = value.as_str() {
                    let kind = match key.as_str() {
                        "src" | "image" if rendered => Some(ResourceKind::Image),
                        _ if target.contains("https://") || target.contains("http://") => Some(ResourceKind::DisplayUrl),
                        _ => None,
                    };
                    if let Some(kind) = kind {
                        out.push(ResourceReference { path: pointer.clone(), target: target.into(), kind });
                    }
                }
                resources(value, &pointer, out, rendered, tokens, findings);
            }
        }
        Value::Array(items) => {
            for (index, value) in items.iter().enumerate() {
                resources(value, &format!("{path}/{index}"), out, rendered, tokens, findings);
            }
        }
        _ => {}
    }
}

/// Native kits resolve one token layer before constructing typed font attributes.
/// Arbitrary objects and nested references must not bypass asset validation.
fn font_resource(value: &Value, path: &str, tokens: &Value, out: &mut Vec<ResourceReference>, findings: &mut Vec<Finding>) {
    let resolved = match value.as_object() {
        Some(object) if object.len() == 1 => object.get("$token").and_then(Value::as_str)
            .and_then(|key| tokens.get(key)).and_then(|token| token.get("value")),
        Some(_) => None,
        None => Some(value),
    };
    match resolved {
        Some(Value::String(target)) if !target.is_empty() => out.push(ResourceReference {
            path: path.into(), target: target.clone(), kind: ResourceKind::Font,
        }),
        // The renderer treats absent, null and empty fonts as its default family.
        Some(Value::Null) => {},
        Some(Value::String(target)) if target.is_empty() => {},
        _ => findings.push(Finding::at("resource-invalid", path,
            "font_src must be a bundled font path, a supported built-in font, or one token resolving to a string")),
    }
}

/// The structural findings for a bundle, and what it references.
pub fn validate(root: &Path, files: &[BundleFile]) -> (Vec<Finding>, Vec<ResourceReference>) {
    let mut findings = Vec::new();
    let mut refs = Vec::new();
    let icon = read_text(&root.join(octosense_app_policy::LISTING_FILE), MAX_TEXT_BYTES)
        .ok()
        .and_then(|text| octosense_app_policy::Listing::parse(&text).ok())
        .and_then(|listing| listing.icon);
    for file in files {
        // The name the bundle knows this file by. Read back from the host's
        // `Path` it would carry the host's separator, and on Windows neither
        // `kit/native/...` nor a listing's `assets/icon.svg` would match -
        // the checks inside a kit would silently not run.
        let name = octosense_app_policy::portable_path(&file.path)
            .unwrap_or_else(|| file.path.to_string_lossy().replace('\\', "/"));
        let path = root.join(&file.path);
        let extension = file.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let is_icon = icon.as_deref() == Some(name.as_str());
        let result = match extension.as_str() {
            "splash" | "card" | "json" | "l0" | "octoscript" | "txt" | "md" => read_text(&path, MAX_TEXT_BYTES).and_then(|text| {
                if extension == "json" {
                    let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                    if name != octosense_app_policy::MANIFEST_FILE && name != octosense_app_policy::LISTING_FILE {
                        // App data and kit prop declarations are not resource
                        // requests; only the renderer's known fields are.
                        resources(&value, &name, &mut refs, false, &Value::Null, &mut findings);
                        if name == "page.data.json" {
                            resources(&value["$kit"]["placements"], "page.data.json/$kit/placements", &mut refs, true, &Value::Null, &mut findings);
                        } else if name.starts_with("kit/native/") && name.ends_with("/kit.json") {
                            if let Some(font) = value["tokens"]["typography.body.font_src"].get("value") {
                                font_resource(font, &format!("{name}/tokens/typography.body.font_src/value"), &Value::Null, &mut refs, &mut findings);
                            }
                            if let Some(components) = value["components"].as_object() {
                                for (component, spec) in components {
                                    resources(&spec["style"], &format!("{name}/components/{component}/style"), &mut refs, true, &value["tokens"], &mut findings);
                                }
                            }
                        }
                    }
                }
                Ok(())
            }),
            "png" | "jpg" | "jpeg" | "webp" => validate_bitmap(&path, &extension, is_icon),
            "svg" => validate_svg(&path, &name, &mut refs, is_icon),
            "wasm" => validate_wasm(&path, &name),
            _ => Ok(()),
        };
        if let Err(error) = result {
            findings.push(Finding::at("contents-invalid", name.as_str(), error));
        }
    }
    if let Err((path, error)) = validate_entry(root) {
        findings.push(Finding::at("entry", path, error));
    }
    for reference in &refs {
        if matches!(reference.kind, ResourceKind::DisplayUrl) {
            continue;
        }
        // These exact assets ship in the pinned Makepad widget resources.
        // Do not admit an arbitrary crate URI or a path below resources/.
        if matches!(reference.kind, ResourceKind::Font) && matches!(reference.target.as_str(),
            "makepad_widgets:resources/Inter.ttf"
            | "makepad_widgets:resources/LXGWWenKaiRegular.ttf"
            | "makepad_widgets:resources/LXGWWenKaiBold.ttf") {
            continue;
        }
        if matches!(reference.kind, ResourceKind::SvgReference) && reference.target.starts_with('#') {
            continue;
        }
        let result = safe_relative(&reference.target).and_then(|_| {
            if files.iter().any(|f| f.path == Path::new(&reference.target)) {
                Ok(())
            } else {
                Err(format!("missing bundled resource {}", reference.target))
            }
        });
        if let Err(error) = result {
            findings.push(Finding::at("resource-invalid", &reference.path, error));
        }
    }
    (findings, refs)
}

/// What the runtime starts: `main.splash` for a script app (it wins, as in
/// the card runner), else `page.card` and its kit for a card.
fn validate_entry(root: &Path) -> Result<(), (String, String)> {
    let script = root.join(octosense_app_policy::SCRIPT_ENTRY);
    if script.is_file() {
        // UTF-8 and size are the contents check's; parsing is the runtime's.
        return Ok(());
    }
    if root.join("page.card").is_file() {
        return validate_card(root);
    }
    Err((
        octosense_app_policy::SCRIPT_ENTRY.into(),
        format!("the bundle has no entry: {} (a script app) or page.card (a card)", octosense_app_policy::SCRIPT_ENTRY),
    ))
}

fn validate_card(root: &Path) -> Result<(), (String, String)> {
    let card = read_text(&root.join("page.card"), octoscript_ui_l0::DEFAULT_MAX_SOURCE_BYTES as u64).map_err(|e| ("page.card".into(), e))?;
    let data: Value = if root.join("page.data.json").exists() {
        serde_json::from_str(&read_text(&root.join("page.data.json"), MAX_TEXT_BYTES).map_err(|e| ("page.data.json".into(), e))?)
            .map_err(|e| ("page.data.json".into(), e.to_string()))?
    } else {
        serde_json::json!({})
    };
    let report = octoscript_ui_l0::check_ui_l0(&card);
    if !report.valid {
        let diagnostics: Vec<_> = report.diagnostics.iter().map(|d| format!("{}:{} {}", d.line, d.column, d.message)).collect();
        return Err(("page.card".into(), diagnostics.join("; ")));
    }
    let mood = octoscript_ui_l0::card_theme(&card).unwrap_or_else(|| "dark".into());
    let native = format!("kit/native/{mood}/kit.json");
    if root.join(&native).is_file() {
        safe_relative(&native).map_err(|e| ("page.card".into(), e))?;
        let pack: Value = serde_json::from_str(&read_text(&root.join(&native), MAX_TEXT_BYTES).map_err(|e| (native.clone(), e))?)
            .map_err(|e| (native.clone(), e.to_string()))?;
        if pack["theme"] != mood {
            return Err((native, "the kit's theme does not match the card's".into()));
        }
        // A native pack can also be a card's optional theme overlay: its
        // presence does not choose the rendering mode or require placements.
        if data["$kit"]["placements"].is_object() {
            check_native_pack(&pack, &data).map_err(|e| (native, e))?;
        }
    } else {
        // Legacy kits are executable Octoscript: check the closure here and
        // leave evaluating it to a bounded runtime validator.
        let mut names = vec![
            "_palette_dark.octoscript".to_string(),
            "_derive_color.octoscript".to_string(),
            "_derive.octoscript".to_string(),
            "_kit.octoscript".to_string(),
        ];
        if mood != "dark" {
            names.push(format!("_palette_{mood}.octoscript"));
        }
        for (axis, value) in octoscript_ui_l0::card_theme_axes(&card) {
            if matches!(value.as_str(), "neutral" | "regular" | "none" | "soft" | "sans") {
                continue;
            }
            names.push(if axis == "accent" { format!("_axis_accent_{value}_{mood}.octoscript") } else { format!("_axis_{axis}_{value}.octoscript") });
        }
        for name in names {
            let name = format!("kit/{name}");
            safe_relative(&name).and_then(|_| read_text(&root.join(&name), MAX_TEXT_BYTES).map(|_| ())).map_err(|e| (name, e))?;
        }
    }
    Ok(())
}

fn check_native_pack(pack: &Value, data: &Value) -> Result<(), String> {
    if pack["schema_version"] != 1 {
        return Err("unsupported kit schema".into());
    }
    let components = pack["components"].as_object().ok_or("the kit has no components")?;
    let placements = data["$kit"]["placements"].as_object().ok_or("the card data has no kit placements")?;
    // Retained JSON memory, counted conservatively without cloning tokens.
    fn weight(value: &Value) -> u64 {
        64 + match value {
            Value::String(s) => s.len() as u64,
            Value::Array(a) => a.iter().map(weight).sum(),
            Value::Object(o) => o.iter().map(|(k, v)| k.len() as u64 + 64 + weight(v)).sum(),
            _ => 0,
        }
    }
    const LIMIT: u64 = 8 * 1024 * 1024;
    // Each token is weighed once, however many styles reuse it.
    let token_weights: std::collections::BTreeMap<&str, u64> = pack["tokens"]
        .as_object()
        .into_iter()
        .flat_map(|tokens| tokens.iter())
        .filter_map(|(name, token)| token.get("value").map(|value| (name.as_str(), weight(value))))
        .collect();
    let mut costs = std::collections::BTreeMap::new();
    for (name, component) in components {
        let style = component["style"].as_object().ok_or("a kit component has no style")?;
        let mut bytes = 0u64;
        for value in style.values() {
            bytes += match value.get("$token").and_then(Value::as_str) {
                Some(token) => *token_weights.get(token).ok_or_else(|| format!("missing kit token {token}"))?,
                None => weight(value),
            };
            if bytes > LIMIT {
                return Err("the kit's expansion exceeds the structural size limit".into());
            }
        }
        costs.insert(name.as_str(), bytes);
    }
    let mut total = 0u64;
    for placement in placements.values() {
        let name = placement["component"].as_str().ok_or("a kit placement names no component")?;
        total += costs.get(name).ok_or_else(|| format!("unknown kit component {name}"))? + weight(placement);
        if total > LIMIT {
            return Err("the kit's expansion exceeds the structural size limit".into());
        }
    }
    Ok(())
}

fn validate_bitmap(path: &Path, extension: &str, icon: bool) -> Result<(), String> {
    let bytes = read_bounded(path, if icon { 1024 * 1024 } else { MAX_BUNDLE_BYTES })?;
    let format = image::ImageFormat::from_extension(extension).ok_or("unsupported image extension")?;
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    let dimension = if icon { MAX_ICON_DIMENSION } else { MAX_IMAGE_DIMENSION };
    limits.max_image_width = Some(dimension);
    limits.max_image_height = Some(dimension);
    limits.max_alloc = Some(MAX_DECODED_BYTES);
    reader.limits(limits);
    use image::ImageDecoder;
    let decoder = reader.into_decoder().map_err(|e| format!("cannot decode the image: {e}"))?;
    if decoder.total_bytes() > MAX_DECODED_BYTES {
        return Err("the decoded image exceeds the size limit".into());
    }
    let (width, height) = decoder.dimensions();
    if icon && width != height {
        return Err("the launcher icon must be square".into());
    }
    image::DynamicImage::from_decoder(decoder).map_err(|e| format!("cannot decode the image: {e}"))?;
    Ok(())
}

/// CSS in an SVG (a `style` attribute or a `<style>` block) may only point
/// at fragments in the same document. Escapes and comments, which can hide
/// a URL token, are refused, and every case-insensitive `url(` is checked.
fn check_svg_css(css: &str) -> Result<(), String> {
    let lower = css.to_ascii_lowercase();
    if lower.contains('\\') || lower.contains("/*") {
        return Err("escaped or commented SVG styling is not supported".into());
    }
    if lower.contains("@import") {
        return Err("SVG styling may not import other stylesheets".into());
    }
    for (at, _) in lower.match_indices("url") {
        let tail = lower[at + 3..].trim_start();
        if let Some(tail) = tail.strip_prefix('(') {
            let end = tail.find(')').ok_or("unterminated SVG resource URL")?;
            let target = tail[..end].trim().trim_matches(['\'', '"']);
            if !target.starts_with('#') || target.len() == 1 || target.contains(char::is_whitespace) {
                return Err("SVG resource URLs must be local fragment references".into());
            }
        }
    }
    Ok(())
}

/// An app's own functions, run by the host's `wasm` service under the `wasm`
/// capability, at `fns/<name>.wasm`: a WebAssembly core module, whose
/// imports and exports are the host's to check when it loads it, or a
/// component (OctoSense ADR 0014), validated here, whose imports must all be
/// WASI packages a host scopes to the app ([`crate::functions`]). Whether
/// the manifest may carry a component is the gate's check.
fn validate_wasm(path: &Path, name: &str) -> Result<(), String> {
    // A system app ships its shared components (App Hub ADR 0003) at
    // components/<blake3>.wasm, named by their digest. Whether a bundle may
    // carry them, and whether the manifest names each, is the gate's check.
    if let Some(stem) = name.strip_prefix("components/").and_then(|rest| rest.strip_suffix(".wasm")) {
        if stem.len() != 64 || !stem.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err("a bundled shared component is named components/<blake3>.wasm, its 64-character lowercase BLAKE3 digest".into());
        }
        let bytes = read_bounded(path, MAX_BUNDLE_BYTES)?;
        if !crate::functions::is_component(&bytes) {
            return Err("a shared component must be a WebAssembly component".into());
        }
        crate::functions::admissible_component(&bytes)?;
        let digest = blake3::hash(&bytes).to_hex().to_string();
        if digest != stem {
            return Err(format!("the file hashes to {digest}, not the digest its name gives"));
        }
        return Ok(());
    }
    let stem = name
        .strip_prefix("fns/")
        .and_then(|rest| rest.strip_suffix(".wasm"))
        .ok_or("a WebAssembly module belongs in fns/, as fns/<name>.wasm")?;
    if stem.is_empty()
        || stem.len() > 64
        || !stem.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
    {
        return Err("a function module is named fns/<name>.wasm, the name [a-z0-9_-] and at most 64 characters".into());
    }
    let bytes = read_bounded(path, MAX_BUNDLE_BYTES)?;
    if crate::functions::is_module(&bytes) {
        return Ok(());
    }
    if !crate::functions::is_component(&bytes) {
        return Err("not a WebAssembly core module (magic and version 1) or component".into());
    }
    crate::functions::admissible_component(&bytes)?;
    Ok(())
}

fn validate_svg(path: &Path, name: &str, refs: &mut Vec<ResourceReference>, icon: bool) -> Result<(), String> {
    let text = read_text(path, MAX_TEXT_BYTES)?;
    let document = roxmltree::Document::parse_with_options(&text, roxmltree::ParsingOptions { nodes_limit: 4096, ..Default::default() })
        .map_err(|e| e.to_string())?;
    let root = document.root_element();
    if root.tag_name().name() != "svg" {
        return Err("expected an SVG document".into());
    }
    let view_box: Vec<f64> = root
        .attribute("viewBox")
        .unwrap_or("")
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    let mut dimensions = Vec::new();
    for (attr, offset) in [("width", 2), ("height", 3)] {
        let value = root
            .attribute(attr)
            .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
            .or_else(|| view_box.get(offset).copied())
            .ok_or("an SVG needs numeric dimensions or a viewBox")?;
        if !value.is_finite() || value <= 0. || value > f64::from(MAX_IMAGE_DIMENSION) {
            return Err("the SVG's dimensions exceed the limits".into());
        }
        dimensions.push(value);
    }
    if icon && dimensions[0] != dimensions[1] {
        return Err("the launcher icon must be square".into());
    }
    for node in root.descendants().filter(|n| n.is_element()) {
        match node.tag_name().name() {
            "script" | "foreignObject" => return Err("an SVG may not hold scripts or foreign content".into()),
            "style" => check_svg_css(&node.descendants().filter_map(|n| n.text()).collect::<String>())?,
            _ => {}
        }
        for attr in node.attributes() {
            if attr.name().starts_with("on") {
                return Err("SVG event handlers are not supported".into());
            }
            if attr.name() == "href" {
                refs.push(ResourceReference {
                    path: format!("{name}/{}@href", node.tag_name().name()),
                    target: attr.value().into(),
                    kind: ResourceKind::SvgReference,
                });
            }
            check_svg_css(attr.value())?;
        }
    }
    Ok(())
}
