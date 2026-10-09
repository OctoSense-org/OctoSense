//! `octosense-design-service` — the `design` host service (ADR 0013).
//!
//! designcraft's page-layout engine (InDesign-class: spreads, frames,
//! threaded stories, paragraph/character styles, a Knuth–Plass composer,
//! IDML interchange) behind typed `design.*` methods. Every call is a
//! fresh, stateless session: open, act, reply, drop.
//!
//! Methods (all under the `design` family; paths relative to the
//! service's own `design/` area of the caller's host directory):
//! - `info {path}` → the document inspected as JSON (pages, spreads,
//!   stories with overset, styles, swatches)
//! - `render {path, out, page?, max_side?}` → one page as a PNG written
//!   to `out`
//! - `export {path, out, pdf?}` → `{out, pages?, bytes?, warnings?}` —
//!   the format follows `out`'s extension: `.pdf`, `.idml`, `.epub` or
//!   `.designcraft` (`pdf` passes export options through to the engine)
//! - `commands {}` → the engine's command catalog, for inspection
//!
//! Unlike photocraft, designcraft has no capability-rooted workspace: its
//! engine reads and writes raw paths (`file.*`, `book.*`, data merge), so
//! no generic command-execution method is exposed, and the service
//! resolves every path itself. All of them stay inside `<host_dir>/design`
//! ([`area`]) — the shared `.host` directory also holds Mail's and
//! Calendar's data, which this service must never reach. The service
//! serves system apps only until ADR 0013's store capability is designed.

use std::path::{Component, Path, PathBuf};

use designcraft_engine::Session;
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use serde_json::{json, Value as Json};

/// The longest page edge `render` produces (pixels).
const MAX_RENDER_SIDE: u64 = 4096;
/// The largest document file the service opens (bytes).
const MAX_DOC_BYTES: u64 = 64 << 20;

/// Which apps may call the service: system apps, as Sheets and Photo.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct DesignService;

/// Register the `design` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(DesignService));
}

/// The `design.*` agent tools (ADR 0013, wave 2), in App Hub's `tools.json`
/// shape: the shell declares them for the virtual owner `os.design` and grants
/// the system agent its reviewed share (`crates/shell/src/host_tools/engines.rs`,
/// `crates/shell/src/system_chat/grants.rs` `ENGINE_TOOLS`).
pub const TOOLS_JSON: &str = include_str!("../tools.json");

impl HostService for DesignService {
    fn family(&self) -> &'static str {
        "design"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        if !may_call(&call.app_id) {
            reply.send(Err("The design service serves system apps only.".into()));
            return;
        }
        let method = call.method().to_string();
        let args = call.args.clone();
        let host_dir = call.host_dir.clone();
        reply.send(dispatch(&method, &args, &host_dir));
    }
}

fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    match method {
        "info" => info(args, host_dir),
        "render" => render(args, host_dir),
        "export" => export(args, host_dir),
        "commands" => commands(),
        other => Err(format!("design.{other} is not a method of the design service")),
    }
}

/// The service's own area under the shared host directory:
/// `<host_dir>/design`, created on first use. Mail's and Calendar's data
/// live beside it in the same `.host`; nothing here may resolve to them.
fn area(host_dir: &Path) -> Result<PathBuf, String> {
    let dir = host_dir.join("design");
    std::fs::create_dir_all(&dir).map_err(|e| format!("design: {e}"))?;
    Ok(dir)
}

/// A path strictly inside the service's area: relative, no `..`, no
/// absolute component; the resolved parent must stay under the area even
/// through symlinks (the stance the sheet service and the files host
/// tools take).
fn contained(method: &str, area: &Path, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(format!("design.{method}: a path is required"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("design.{method}: paths stay inside the service's design area"));
    }
    let joined = area.join(rel_path);
    let check_root = area.canonicalize().map_err(|e| format!("design.{method}: area: {e}"))?;
    let deepest = {
        let mut p = joined.clone();
        while !p.exists() {
            match p.parent() {
                Some(parent) => p = parent.to_path_buf(),
                None => break,
            }
        }
        p
    };
    let resolved = deepest.canonicalize().map_err(|e| format!("design.{method}: {e}"))?;
    if !resolved.starts_with(&check_root) {
        return Err(format!("design.{method}: paths stay inside the service's design area"));
    }
    Ok(joined)
}

fn arg_str<'a>(method: &str, args: &'a Json, key: &str) -> Result<&'a str, String> {
    args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("design.{method}: `{key}` is required"))
}

/// Open the document at the caller's relative `path` (`.designcraft`
/// native JSON or `.idml`) in a fresh session, bounded by [`MAX_DOC_BYTES`].
fn open(method: &str, args: &Json, host_dir: &Path) -> Result<(Session, Json), String> {
    let rel = arg_str(method, args, "path")?;
    let area = area(host_dir)?;
    let abs = contained(method, &area, rel)?;
    let meta = std::fs::metadata(&abs).map_err(|e| format!("design.{method}: {rel}: {e}"))?;
    if meta.len() > MAX_DOC_BYTES {
        return Err(format!("design.{method}: the file is larger than the service opens"));
    }
    let mut s = Session::new();
    let opened = s
        .execute("file.open", &json!({"path": abs.to_string_lossy()}))
        .map_err(|e| format!("design.{method}: {e}"))?;
    Ok((s, opened))
}

/// An output path inside the area, its parent directories created.
fn out_path(method: &str, args: &Json, host_dir: &Path) -> Result<(PathBuf, String), String> {
    let rel = arg_str(method, args, "out")?;
    let area = area(host_dir)?;
    let abs = contained(method, &area, rel)?;
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("design.{method}: {e}"))?;
    }
    Ok((abs, rel.to_string()))
}

fn info(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let (mut s, opened) = open("info", args, host_dir)?;
    let mut doc = s.execute("document.inspect", &json!({})).map_err(|e| format!("design.info: {e}"))?;
    if let Some(o) = doc.as_object_mut() {
        // Session-only state means nothing to a stateless caller, and the
        // host path is the service's business, not the app's.
        for k in ["selection", "tool", "canUndo", "dirty", "activeLayer", "path"] {
            o.remove(k);
        }
        o.insert("file".into(), args["path"].clone());
        o.insert("warnings".into(), opened["warnings"].clone());
    }
    Ok(doc)
}

fn render(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let (s, _) = open("render", args, host_dir)?;
    let (out_abs, out_rel) = out_path("render", args, host_dir)?;
    let page = args["page"].as_u64().unwrap_or(0) as usize;
    let max_side = args["max_side"].as_u64().unwrap_or(1024).clamp(16, MAX_RENDER_SIDE);
    let st = s.doc().map_err(|e| format!("design.render: {e}"))?;
    let p = st.doc.page(page).ok_or_else(|| format!("design.render: no page {page}"))?;
    let scale = max_side as f64 / p.width.max(p.height).max(1.0);
    let mut r = designcraft_render::Renderer::new();
    let img = r
        .render_page(
            &st.doc,
            &s.cache,
            page,
            scale,
            true,
            &designcraft_render::RenderOptions { printing_only: true, ..Default::default() },
        )
        .ok_or_else(|| format!("design.render: no page {page}"))?;
    let png = img.to_png();
    std::fs::write(&out_abs, &png).map_err(|e| format!("design.render: {out_rel}: {e}"))?;
    Ok(json!({"out": out_rel, "page": page, "width": img.width, "height": img.height, "bytes": png.len()}))
}

fn export(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let (mut s, _) = open("export", args, host_dir)?;
    let (out_abs, out_rel) = out_path("export", args, host_dir)?;
    let abs = out_abs.to_string_lossy();
    let lower = out_rel.to_ascii_lowercase();
    let r = if lower.ends_with(".pdf") {
        let mut p = if args["pdf"].is_object() { args["pdf"].clone() } else { json!({}) };
        p["path"] = json!(abs);
        s.execute("file.exportPdf", &p)
    } else if lower.ends_with(".idml") {
        s.execute("file.exportIdml", &json!({"path": abs}))
    } else if lower.ends_with(".epub") {
        s.execute("file.exportEpub", &json!({"path": abs}))
    } else if lower.ends_with(".designcraft") {
        s.execute("file.saveAs", &json!({"path": abs}))
    } else {
        return Err("design.export: `out` ends with .pdf, .idml, .epub or .designcraft".into());
    }
    .map_err(|e| format!("design.export: {e}"))?;
    let mut out = json!({"out": out_rel});
    for k in ["pages", "bytes", "warnings"] {
        if !r[k].is_null() {
            out[k] = r[k].clone();
        }
    }
    Ok(out)
}

fn commands() -> Result<Json, String> {
    serde_json::to_value(Session::new().commands()).map_err(|e| format!("design.commands: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real two-page document, laid out and saved by the real engine:
    /// a text frame whose story the composer sets with the bundled fonts.
    fn fixture(host: &Path) -> String {
        let dir = area(host).unwrap();
        let mut s = Session::new();
        s.execute("file.new", &json!({"pages": 2})).unwrap();
        s.execute(
            "frame.create",
            &json!({"rect": [36.0, 36.0, 420.0, 240.0], "content": "text",
                "text": "The quiet art of layout: a fixture the design service composes itself."}),
        )
        .unwrap();
        s.execute("file.saveAs", &json!({"path": dir.join("mag.designcraft").to_string_lossy()})).unwrap();
        "mag.designcraft".into()
    }

    #[test]
    fn info_inspects_the_real_document() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let path = fixture(host);
        let doc = dispatch("info", &json!({"path": path}), host).unwrap();
        assert_eq!(doc["pageCount"], json!(2), "{doc}");
        assert_eq!(doc["file"], json!("mag.designcraft"));
        assert!(doc["settings"]["pageWidth"].as_f64().unwrap() > 0.0);
        assert_eq!(doc["stories"].as_array().unwrap().len(), 1, "{doc}");
        assert!(doc["stories"][0]["preview"].as_str().unwrap().starts_with("The quiet art"));
        assert!(doc.get("selection").is_none(), "session-only state stays out");
    }

    #[test]
    fn render_writes_a_bounded_png() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let path = fixture(host);
        let r = dispatch("render", &json!({"path": path, "out": "previews/p1.png", "max_side": 256}), host).unwrap();
        assert_eq!(r["out"], json!("previews/p1.png"));
        let (w, h) = (r["width"].as_u64().unwrap(), r["height"].as_u64().unwrap());
        assert!(w.max(h) >= 250 && w.max(h) <= 260, "{r}"); // the longest edge is max_side (± bleed and rounding)
        let png = std::fs::read(host.join("design/previews/p1.png")).unwrap();
        assert_eq!(&png[..4], b"\x89PNG");
        assert!(dispatch("render", &json!({"path": path, "out": "p9.png", "page": 9}), host).is_err(), "no page 9");
    }

    #[test]
    fn export_pdf_idml_and_native_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let path = fixture(host);

        let pdf = dispatch("export", &json!({"path": path, "out": "mag.pdf"}), host).unwrap();
        assert_eq!(pdf["pages"], json!(2), "{pdf}");
        assert!(pdf["bytes"].as_u64().unwrap() > 0);
        assert_eq!(&std::fs::read(host.join("design/mag.pdf")).unwrap()[..5], b"%PDF-");

        // IDML out, and the interchange file opens again through `info`.
        dispatch("export", &json!({"path": path, "out": "mag.idml"}), host).unwrap();
        let round = dispatch("info", &json!({"path": "mag.idml"}), host).unwrap();
        assert_eq!(round["pageCount"], json!(2), "{round}");

        // Native save-as, reopened too.
        dispatch("export", &json!({"path": path, "out": "copy.designcraft"}), host).unwrap();
        let copy = dispatch("info", &json!({"path": "copy.designcraft"}), host).unwrap();
        assert_eq!(copy["pageCount"], json!(2));

        assert!(dispatch("export", &json!({"path": path, "out": "mag.docx"}), host).is_err(), "unknown format");
    }

    /// What `tools.json` promises each answer carries, on a real document:
    /// the shell's relay checks every agent call's answer against it, and
    /// no agent tool can write a layout document for the shell's own check.
    #[test]
    fn answers_carry_what_the_tools_declare() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let path = fixture(host);
        let doc = dispatch("info", &json!({"path": path}), host).unwrap();
        assert!(doc["file"].is_string() && doc["pageCount"].is_u64() && doc["settings"].is_object() && doc["stories"].is_array(), "{doc}");
        assert!(doc["warnings"].is_array() || doc["warnings"].is_null(), "{doc}");
        let r = dispatch("render", &json!({"path": path, "out": "p.png", "max_side": 64}), host).unwrap();
        assert!(r["out"].is_string() && ["page", "width", "height", "bytes"].iter().all(|k| r[*k].is_u64()), "{r}");
        for out in ["x.pdf", "x.idml", "x.epub", "x.designcraft"] {
            let e = dispatch("export", &json!({"path": path, "out": out}), host).unwrap();
            assert!(e["out"].is_string(), "{out}: {e}");
            assert!(["pages", "bytes"].iter().all(|k| e[*k].is_null() || e[*k].is_u64()), "{out}: {e}");
            assert!(e["warnings"].is_null() || e["warnings"].is_array(), "{out}: {e}");
        }
        let idml = dispatch("info", &json!({"path": "x.idml"}), host).unwrap();
        assert!(idml["pageCount"].is_u64() && (idml["warnings"].is_array() || idml["warnings"].is_null()), "{idml}");
        // Export options as the tool declares them reach the engine.
        let pdf = dispatch(
            "export",
            &json!({"path": path, "out": "one.pdf", "pdf": {"pages": "1", "bleed": true, "standard": "none", "view": "fitPage", "pageLayout": "single"}}),
            host,
        )
        .unwrap();
        assert_eq!(pdf["pages"], json!(1), "{pdf}");
    }

    #[test]
    fn commands_lists_the_engine_catalog() {
        let cat = commands().unwrap();
        let cat = cat.as_array().unwrap();
        assert!(cat.len() > 300, "a real catalog, not a stub: {}", cat.len());
        assert!(cat.iter().any(|c| c["id"] == json!("file.exportPdf")));
    }

    #[test]
    fn paths_stay_inside_the_design_area() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let path = fixture(host);
        // A neighbour the shared .host could hold (Mail's, Calendar's data).
        std::fs::create_dir_all(host.join("mail")).unwrap();
        std::fs::write(host.join("mail/accounts.json"), b"{}").unwrap();
        for bad in ["../mail/accounts.json", "../up.designcraft", "/etc/x.designcraft", "a/../../up.designcraft", ""] {
            assert!(dispatch("info", &json!({"path": bad}), host).is_err(), "{bad}");
            assert!(dispatch("render", &json!({"path": path, "out": bad}), host).is_err(), "{bad}");
            assert!(dispatch("export", &json!({"path": path, "out": bad}), host).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_area_is_the_design_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let a = area(host).unwrap();
        assert_eq!(a, host.join("design"));
        assert!(a.is_dir(), "created on first use");
    }

    #[test]
    fn only_system_apps_may_call() {
        assert!(may_call("os.design"));
        assert!(!may_call("org.example.anything"));
        assert!(!may_call(""));
    }

    /// The agent tools (`tools.json`) pass App Hub's own loader, as the shell
    /// reads them, keep the object schemas octos takes both ways, and name
    /// only methods this service dispatches.
    #[test]
    fn the_agent_tools_pass_app_hubs_loader_and_name_real_methods() {
        use octosense_app_policy::{ImplementedBy, ToolHost, ToolManifest};
        let (manifest, _) = ToolManifest::load(TOOLS_JSON, "design", ToolHost::Contained, false).unwrap();
        assert!(!manifest.tools.is_empty());
        let dir = tempfile::tempdir().unwrap();
        for tool in &manifest.tools {
            assert_eq!(tool.implemented_by, ImplementedBy::HostService, "{}", tool.name);
            assert!(tool.host_method.is_none(), "{}: the shell routes each tool to the method of its own name", tool.name);
            for schema in [&tool.input_schema, &tool.output_schema] {
                assert_eq!(schema["type"], json!("object"), "{}: octos takes object schemas only", tool.name);
            }
            assert!(tool.description.is_ascii(), "{}: descriptions stay within octos's byte limit", tool.name);
            let method = tool.name.strip_prefix("design.").unwrap();
            if let Err(e) = dispatch(method, &json!({}), dir.path()) {
                assert!(!e.contains("is not a method"), "{}: {e}", tool.name);
            }
        }
    }
}
