//! `octosense-design-service` — the `design` host service (ADR 0013).
//!
//! designcraft's page-layout engine (InDesign-class: spreads, frames,
//! threaded stories, paragraph/character styles, a Knuth–Plass composer,
//! IDML interchange) behind typed `design.*` methods. Every call is a
//! fresh, stateless session: open, act, reply, drop.
//!
//! Methods (all under the `design` family; paths relative to the call's
//! area):
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
//! resolves every path itself.
//!
//! **Where a call works** (ADR 0013, 2026-10-08): the caller's own folder,
//! the [`Area`] the shell's resolver gives it ([`set_area_resolver`]), or
//! without one the legacy `<host dir>/design`. Every path the caller names
//! stays inside it, and so does every path written *inside* a document,
//! which the engine would otherwise follow wherever it points
//! ([`fence_before_open`], [`sanitize`], [`check_placed_svgs`]):
//!
//! - an IDML whose placed graphics are links, not embedded contents, is
//!   refused before the engine opens it (the engine's own importer is run
//!   first with a reader that records the links it would read);
//! - a `Document Fonts` folder beside the document, or a font in it, that
//!   leads outside the area refuses the document;
//! - after it opens, what the document says about files elsewhere (data
//!   merge sources, placed graphics' original paths) is cleared, so nothing
//!   about them reaches an output;
//! - a placed SVG that links a file by its `<image href>` refuses the
//!   document, whatever the call: the engine's usvg reads such a link
//!   wherever it points as soon as it parses the SVG (to draw it, or only
//!   to size it).
//!
//! Writes keep the area's rules (a write that may not replace, an agent's,
//! only creates new files, within the quota): a rendered page through
//! [`Area::write`], an export, which the engine writes itself, into a
//! staging folder inside the area first ([`octosense_engine_area::Stage`]).
//! The service serves system apps only until ADR 0013's store capability is
//! designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use designcraft_engine::Session;
use designcraft_images::usvg;
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::{Area, Slot};
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

/// The shell's resolver: where each call works (`None` removes it, and
/// calls work in the legacy `<host dir>/design` again).
static AREAS: Slot = Slot::new();

pub fn set_area_resolver(resolver: Option<octosense_engine_area::Resolver>) {
    AREAS.set(resolver);
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
        reply.send(serve(&AREAS, &call));
    }
}

/// One call, in the area `areas` gives it.
fn serve(areas: &Slot, call: &ServiceCall) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err("The design service serves system apps only.".into());
    }
    if call.method() == "commands" {
        return commands();
    }
    let area = areas.area(call, "design").map_err(|e| format!("design: {e}"))?;
    dispatch_in(call.method(), &call.args, &area)
        .map(|answer| area.relative_json(answer))
        .map_err(|error| area.relative_text(&error))
}

fn dispatch_in(method: &str, args: &Json, area: &Area) -> Result<Json, String> {
    match method {
        "info" => info(args, area),
        "render" => render(args, area),
        "export" => export(args, area),
        "commands" => commands(),
        other => Err(format!("design.{other} is not a method of the design service")),
    }
}

/// [`dispatch_in`] in the legacy area `<host_dir>/design`, as a call
/// without the shell's resolver works.
#[cfg(test)]
fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    if method == "commands" {
        return commands();
    }
    let area = Area::legacy(host_dir, "design");
    std::fs::create_dir_all(&area.root).map_err(|e| format!("design: {e}"))?;
    dispatch_in(method, args, &area)
}

/// A path strictly inside the call's area: relative, no `..`, no absolute
/// component; the resolved parent must stay under the area even through
/// symlinks (the stance the sheet service and the files host tools take).
fn contained(method: &str, area: &Area, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(format!("design.{method}: a path is required"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("design.{method}: paths stay inside this call's folder"));
    }
    let joined = area.root.join(rel_path);
    let check_root = area.root.canonicalize().map_err(|e| format!("design.{method}: folder: {e}"))?;
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
        return Err(format!("design.{method}: paths stay inside this call's folder"));
    }
    Ok(joined)
}

/// The folder beside a document whose fonts the engine loads with it
/// (designcraft-fonts' `DOCUMENT_FONTS_FOLDER`).
const DOCUMENT_FONTS: &str = "Document Fonts";

/// What the engine would read on its own when it opens `abs`, fenced
/// before it does: the fonts in the `Document Fonts` folder beside the
/// document (the folder and each font must resolve inside the area), and,
/// for an IDML, the files its placed graphics link to. The engine reads an
/// IDML's links eagerly, trying the link's own path first (an absolute path
/// as it is, a relative one against the process's working folder), so an
/// IDML whose graphics are links is refused: its links are found by running
/// the engine's own importer with a reader that records them and reads
/// nothing.
fn fence_before_open(method: &str, area: &Area, abs: &Path, rel: &str) -> Result<(), String> {
    let root = area.root.canonicalize().map_err(|e| format!("design.{method}: folder: {e}"))?;
    let inside = |p: &Path| p.canonicalize().is_ok_and(|real| real.starts_with(&root));
    if let Some(fonts) = abs.parent().map(|dir| dir.join(DOCUMENT_FONTS)) {
        if std::fs::symlink_metadata(&fonts).is_ok() {
            let mut leads_out = !inside(&fonts);
            for entry in std::fs::read_dir(&fonts).into_iter().flatten().flatten() {
                let ext = entry.path().extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                if matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") && !inside(&entry.path()) {
                    leads_out = true;
                }
            }
            if leads_out {
                return Err(format!("design.{method}: `{rel}`'s `{DOCUMENT_FONTS}` lead outside this call's folder"));
            }
        }
    }
    if abs.to_string_lossy().to_ascii_lowercase().ends_with(".idml") {
        let bytes = std::fs::read(abs).map_err(|e| format!("design.{method}: {rel}: {e}"))?;
        let links: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let _ = designcraft_idml::import_idml_with(&bytes, &|uri: &str| {
            links.lock().unwrap_or_else(|e| e.into_inner()).push(uri.to_string());
            None
        });
        let links = links.into_inner().unwrap_or_else(|e| e.into_inner());
        if let Some(link) = links.first() {
            return Err(format!(
                "design.{method}: `{rel}` links {} graphic(s) it does not embed (`{link}`): the engine would read them from wherever they point; export it with its images embedded",
                links.len()
            ));
        }
    }
    Ok(())
}

/// What an opened document says about files elsewhere, cleared: its data
/// merge sources' paths, and what the engine found there on opening (it
/// stats them: whether they exist, their size and time), and its placed
/// graphics' original paths. The service never merges or relinks, and
/// nothing about those files may reach an output.
fn sanitize(s: &mut Session, method: &str) -> Result<(), String> {
    let st = s.doc().map_err(|e| format!("design.{method}: {e}"))?;
    let refers = st.doc.data_merge.sources.iter().any(|src| src.path.is_some() || src.relative_path.is_some() || src.fingerprint.is_some())
        || st.doc.assets.values().any(|a| a.link.is_some());
    if !refers {
        return Ok(());
    }
    s.edit(|doc, _| {
        for src in &mut doc.data_merge.sources {
            src.path = None;
            src.relative_path = None;
            src.fingerprint = None;
            src.status = Default::default();
        }
        for asset in doc.assets.values_mut() {
            if asset.link.is_some() {
                Arc::make_mut(asset).link = None;
            }
        }
        Ok(())
    })
    .map_err(|e| format!("design.{method}: {e}"))
}

/// The files a placed SVG's `<image>` elements link to, as the engine's own
/// usvg would resolve them: its parse, with a resolver that records each
/// linked (non-`data:`) href and reads nothing.
fn linked_hrefs(svg: &[u8]) -> Vec<String> {
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let record = seen.clone();
    let opts = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(move |href: &str, _: &usvg::Options| {
                record.lock().unwrap_or_else(|e| e.into_inner()).push(href.to_string());
                None
            }),
        },
        ..Default::default()
    };
    let _ = usvg::Tree::from_data(svg, &opts);
    let found = seen.lock().unwrap_or_else(|e| e.into_inner()).clone();
    found
}

/// Refuse a document whose placed SVG links a file: the engine parses a
/// placed SVG (to draw a page or a PDF, or to size it) with usvg's default
/// resolver, which reads an `<image href>` from wherever it points.
fn check_placed_svgs(s: &Session, method: &str) -> Result<(), String> {
    let st = s.doc().map_err(|e| format!("design.{method}: {e}"))?;
    for asset in st.doc.assets.values() {
        if !designcraft_images::is_svg(&asset.data) {
            continue;
        }
        if let Some(href) = linked_hrefs(&asset.data).first() {
            return Err(format!(
                "design.{method}: the placed SVG `{}` links `{href}`, a file outside the document: only images embedded in it are drawn here",
                asset.name
            ));
        }
    }
    Ok(())
}

fn arg_str<'a>(method: &str, args: &'a Json, key: &str) -> Result<&'a str, String> {
    args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("design.{method}: `{key}` is required"))
}

/// Open the document at the caller's relative `path` (`.designcraft`
/// native JSON or `.idml`) in a fresh session, bounded by [`MAX_DOC_BYTES`],
/// with the paths inside it fenced ([`fence_before_open`], [`sanitize`]).
fn open(method: &str, args: &Json, area: &Area) -> Result<(Session, Json), String> {
    let rel = arg_str(method, args, "path")?;
    let abs = contained(method, area, rel)?;
    let meta = std::fs::metadata(&abs).map_err(|e| format!("design.{method}: {rel}: {e}"))?;
    if meta.len() > MAX_DOC_BYTES {
        return Err(format!("design.{method}: the file is larger than the service opens"));
    }
    fence_before_open(method, area, &abs, rel)?;
    let mut s = Session::new();
    let opened = s
        .execute("file.open", &json!({"path": abs.to_string_lossy()}))
        .map_err(|e| format!("design.{method}: {e}"))?;
    sanitize(&mut s, method)?;
    check_placed_svgs(&s, method)?;
    Ok((s, opened))
}

/// An output path inside the area that the call may write, refused before
/// the engine works when the area's rules would refuse it.
fn out_path(method: &str, args: &Json, area: &Area) -> Result<(PathBuf, String), String> {
    let rel = arg_str(method, args, "out")?;
    let abs = contained(method, area, rel)?;
    area.check(&abs, 0).map_err(|e| format!("design.{method}: {e}"))?;
    Ok((abs, rel.to_string()))
}

fn info(args: &Json, area: &Area) -> Result<Json, String> {
    let (mut s, opened) = open("info", args, area)?;
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

fn render(args: &Json, area: &Area) -> Result<Json, String> {
    let (out_abs, out_rel) = out_path("render", args, area)?;
    let (s, _) = open("render", args, area)?;
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
    area.write(&out_abs, &png).map_err(|e| format!("design.render: {e}"))?;
    Ok(json!({"out": out_rel, "page": page, "width": img.width, "height": img.height, "bytes": png.len()}))
}

fn export(args: &Json, area: &Area) -> Result<Json, String> {
    let (out_abs, out_rel) = out_path("export", args, area)?;
    let lower = out_rel.to_ascii_lowercase();
    if ![".pdf", ".idml", ".epub", ".designcraft"].iter().any(|ext| lower.ends_with(ext)) {
        return Err("design.export: `out` ends with .pdf, .idml, .epub or .designcraft".into());
    }
    let (mut s, _) = open("export", args, area)?;
    // The engine writes the export itself: into a staging folder, then into
    // place under the area's rules.
    let stage = area.stage().map_err(|e| format!("design.export: {e}"))?;
    let staged = stage.path(out_abs.file_name().unwrap_or_default());
    let abs = staged.to_string_lossy();
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
    stage.commit(&[(staged.clone(), out_abs)]).map_err(|e| format!("design.export: {e}"))?;
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
        fixture_in(&host.join("design"))
    }

    /// The fixture document, saved straight into `dir` (a caller's folder).
    fn fixture_in(dir: &Path) -> String {
        std::fs::create_dir_all(dir).unwrap();
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
        let a = Slot::new().area(&service_call("info", json!({}), host, false), "design").unwrap();
        assert_eq!(a.root, host.join("design"));
        assert!(a.root.is_dir(), "created on first use");
        let path = fixture(host);
        for _ in 0..2 {
            serve(&Slot::new(), &service_call("export", json!({"path": path, "out": "copy.designcraft"}), host, false)).unwrap();
        }
        assert!(host.join("design/copy.designcraft").is_file(), "without a resolver, replacing as before");
    }

    /// A call as App Hub hands it to the service.
    fn service_call(method: &str, args: Json, host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall { app_id: "os.fixture".into(), service: format!("design.{method}"), args, from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
    }

    /// A resolver shaped like the shell's: every call works in `root`, an
    /// app's own foreground call may replace a file and an agent's may not,
    /// within `quota`.
    fn resolver(root: &Path, quota: Option<u64>) -> Slot {
        let slot = Slot::new();
        let root = root.to_path_buf();
        slot.set(Some(Arc::new(move |call: &ServiceCall| Ok(Area::new(&root, quota, call.may_prompt)))));
        slot
    }

    fn no_staging_left(root: &Path) -> bool {
        std::fs::read_dir(root).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with(octosense_engine_area::STAGING_PREFIX))
    }

    /// A 12x8 RGB PNG (two colour bands).
    fn png() -> Vec<u8> {
        const HEX: &str = "89504e470d0a1a0a0000000d494844520000000c000000080802000000428689a60000001d49444154789c6378616383866c725ea021063a2bb279d14310d15911005b9497817c6155610000000049454e44ae426082";
        (0..HEX.len()).step_by(2).map(|i| u8::from_str_radix(&HEX[i..i + 2], 16).unwrap()).collect()
    }

    /// With the shell's resolver every path is relative to the caller's own
    /// folder and stays inside it, through a link too; an export the engine
    /// writes itself lands where it was asked.
    #[test]
    fn the_resolver_root_is_used_and_paths_stay_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        let path = fixture_in(&root);
        let areas = resolver(&root, None);
        let host = dir.path().join(".host");
        let doc = serve(&areas, &service_call("info", json!({"path": path}), &host, false)).unwrap();
        assert_eq!(doc["pageCount"], json!(2), "{doc}");
        serve(&areas, &service_call("render", json!({"path": path, "out": "prev/p.png", "max_side": 64}), &host, false)).unwrap();
        serve(&areas, &service_call("export", json!({"path": path, "out": "out/mag.pdf"}), &host, false)).unwrap();
        assert!(root.join("prev/p.png").is_file() && root.join("out/mag.pdf").is_file());
        assert!(!host.exists() && !root.join("design").exists() && no_staging_left(&root));
        std::fs::write(dir.path().join("beside.designcraft"), b"{}").unwrap();
        for bad in ["../beside.designcraft", "/etc/hosts", "out/../../beside.designcraft"] {
            assert!(serve(&areas, &service_call("info", json!({"path": bad}), &host, false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("export", json!({"path": path, "out": bad}), &host, true)).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            assert!(serve(&areas, &service_call("info", json!({"path": "up/beside.designcraft"}), &host, false)).is_err());
            assert!(serve(&areas, &service_call("render", json!({"path": path, "out": "up/p.png"}), &host, true)).is_err());
            assert!(serve(&areas, &service_call("export", json!({"path": path, "out": "up/x.pdf"}), &host, true)).is_err());
            assert!(!dir.path().join("p.png").exists() && !dir.path().join("x.pdf").exists());
        }
    }

    /// An agent's call never replaces a file, before the engine works; an
    /// app's own foreground call may.
    #[test]
    fn an_agent_never_replaces_a_file_and_an_app_may() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture_in(dir.path());
        let areas = resolver(dir.path(), None);
        std::fs::write(dir.path().join("taken.pdf"), b"keep me").unwrap();
        std::fs::write(dir.path().join("taken.png"), b"keep me").unwrap();
        for (method, args) in [
            ("export", json!({"path": path, "out": "taken.pdf"})),
            ("render", json!({"path": path, "out": "taken.png"})),
            ("export", json!({"path": path, "out": path})),
        ] {
            let refused = serve(&areas, &service_call(method, args, dir.path(), false)).unwrap_err();
            assert!(refused.contains("already exists"), "{method}: {refused}");
        }
        assert_eq!(std::fs::read(dir.path().join("taken.pdf")).unwrap(), b"keep me");
        assert!(no_staging_left(dir.path()));
        serve(&areas, &service_call("export", json!({"path": path, "out": "taken.pdf"}), dir.path(), true)).unwrap();
        assert!(std::fs::read(dir.path().join("taken.pdf")).unwrap().starts_with(b"%PDF"));
    }

    /// What a call writes, the engine's own export included, must fit what
    /// is left of the area's quota.
    #[test]
    fn output_over_the_quota_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture_in(dir.path());
        let tight = resolver(dir.path(), Some(64));
        for (method, args) in [
            ("export", json!({"path": path, "out": "m.pdf"})),
            ("render", json!({"path": path, "out": "p.png", "max_side": 64})),
        ] {
            let refused = serve(&tight, &service_call(method, args, dir.path(), true)).unwrap_err();
            assert!(refused.contains("bytes left"), "{method}: {refused}");
        }
        assert!(!dir.path().join("m.pdf").exists() && !dir.path().join("p.png").exists() && no_staging_left(dir.path()));
    }

    /// An IDML whose placed graphic is a link (here to a picture outside the
    /// caller's folder) is refused before the engine opens it, so the
    /// engine never reads the link; the same layout with its images
    /// embedded opens.
    #[test]
    fn an_idml_that_links_its_graphics_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let outside = dir.path().join("private.png");
        std::fs::write(&outside, png()).unwrap();
        let mut s = Session::new();
        s.execute("file.new", &json!({"pages": 1})).unwrap();
        s.execute("file.place", &json!({"path": outside.to_string_lossy(), "x": 36, "y": 36, "width": 120})).unwrap();
        s.execute("file.exportIdml", &json!({"path": root.join("linked.idml").to_string_lossy(), "embedImages": false})).unwrap();
        s.execute("file.exportIdml", &json!({"path": root.join("embedded.idml").to_string_lossy()})).unwrap();
        let areas = resolver(&root, None);
        for (method, args) in [
            ("info", json!({"path": "linked.idml"})),
            ("render", json!({"path": "linked.idml", "out": "l.png"})),
            ("export", json!({"path": "linked.idml", "out": "l.pdf"})),
        ] {
            let refused = serve(&areas, &service_call(method, args, &root, false)).unwrap_err();
            assert!(refused.contains("does not embed") && refused.contains("private.png"), "{method}: {refused}");
        }
        let ok = serve(&areas, &service_call("info", json!({"path": "embedded.idml"}), &root, false)).unwrap();
        assert_eq!(ok["pageCount"], json!(1), "{ok}");
    }

    /// A placed SVG that links a file outside the document (`<image href>`
    /// to a picture beyond the caller's folder) refuses the document for
    /// every call, before the engine parses the SVG: nothing is drawn,
    /// sized or exported from it.
    #[test]
    fn a_placed_svg_that_links_a_file_is_not_drawn() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let outside = dir.path().join("private.png");
        std::fs::write(&outside, png()).unwrap();
        let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><image href="{}" width="40" height="20"/></svg>"#, outside.display());
        let b64 = {
            const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let bytes = svg.as_bytes();
            let mut out = String::new();
            for chunk in bytes.chunks(3) {
                let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
                for i in 0..4 {
                    out.push(if i <= chunk.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
                }
            }
            out
        };
        let mut s = Session::new();
        s.execute("file.new", &json!({"pages": 1})).unwrap();
        s.execute("file.place", &json!({"base64": b64, "name": "linked.svg", "x": 36, "y": 36, "width": 120})).unwrap();
        s.execute("file.saveAs", &json!({"path": root.join("svg.designcraft").to_string_lossy()})).unwrap();
        assert!(linked_hrefs(svg.as_bytes()).iter().any(|h| h.contains("private.png")), "the engine's usvg sees the link");
        let areas = resolver(&root, None);
        for (method, args) in [
            ("info", json!({"path": "svg.designcraft"})),
            ("render", json!({"path": "svg.designcraft", "out": "p.png"})),
            ("export", json!({"path": "svg.designcraft", "out": "p.pdf"})),
            ("export", json!({"path": "svg.designcraft", "out": "p.epub"})),
            ("export", json!({"path": "svg.designcraft", "out": "p.idml"})),
        ] {
            let refused = serve(&areas, &service_call(method, args, &root, false)).unwrap_err();
            assert!(refused.contains("links") && refused.contains("private.png"), "{method}: {refused}");
        }
        for out in ["p.png", "p.pdf", "p.epub", "p.idml"] {
            assert!(!root.join(out).exists(), "{out}");
        }
        // An image embedded in the SVG is drawn.
        assert!(linked_hrefs(br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><image href="data:image/png;base64,iVBORw0KGgo=" width="4" height="4"/></svg>"#).is_empty());
    }

    /// What a document says about files elsewhere never reaches an output:
    /// a data merge source outside the folder (whose existence, size and
    /// time the engine looks up on opening) leaves no trace in an export.
    #[test]
    fn a_documents_paths_elsewhere_never_reach_an_output() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let csv = dir.path().join("people.csv");
        std::fs::write(&csv, "name,city\nAda,London\n").unwrap();
        let mut s = Session::new();
        s.execute("file.new", &json!({"pages": 1})).unwrap();
        s.execute("data.source.select", &json!({"path": csv.to_string_lossy()})).unwrap();
        s.execute("file.saveAs", &json!({"path": root.join("merge.designcraft").to_string_lossy()})).unwrap();
        let mut original = Session::new();
        original.execute("file.open", &json!({"path": root.join("merge.designcraft").to_string_lossy()})).unwrap();
        assert!(original.doc().unwrap().doc.data_merge.sources[0].path.as_deref().is_some_and(|p| p.ends_with("people.csv")), "the source names the file");
        let areas = resolver(&root, None);
        serve(&areas, &service_call("export", json!({"path": "merge.designcraft", "out": "copy.designcraft"}), &root, false)).unwrap();
        let mut back = Session::new();
        back.execute("file.open", &json!({"path": root.join("copy.designcraft").to_string_lossy()})).unwrap();
        let sources = back.doc().unwrap().doc.data_merge.sources.clone();
        assert!(!sources.is_empty(), "the merge table itself is kept");
        for src in sources {
            assert!(src.path.is_none() && src.relative_path.is_none() && src.fingerprint.is_none(), "{src:?}");
        }
    }

    /// The fonts the engine loads from the `Document Fonts` folder beside a
    /// document must be inside the caller's folder: a font, or the folder,
    /// that leads outside refuses the document.
    #[cfg(unix)]
    #[test]
    fn document_fonts_that_lead_outside_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        let path = fixture_in(&root);
        let areas = resolver(&root, None);
        let elsewhere = dir.path().join("fonts");
        std::fs::create_dir(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("secret.ttf"), b"not a font").unwrap();
        std::fs::create_dir(root.join(DOCUMENT_FONTS)).unwrap();
        std::os::unix::fs::symlink(elsewhere.join("secret.ttf"), root.join(DOCUMENT_FONTS).join("linked.ttf")).unwrap();
        let refused = serve(&areas, &service_call("info", json!({"path": path}), &root, false)).unwrap_err();
        assert!(refused.contains(DOCUMENT_FONTS), "{refused}");
        std::fs::remove_dir_all(root.join(DOCUMENT_FONTS)).unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.join(DOCUMENT_FONTS)).unwrap();
        assert!(serve(&areas, &service_call("info", json!({"path": path}), &root, false)).unwrap_err().contains(DOCUMENT_FONTS));
        std::fs::remove_file(root.join(DOCUMENT_FONTS)).unwrap();
        serve(&areas, &service_call("info", json!({"path": path}), &root, false)).unwrap();
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
