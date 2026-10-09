//! `octosense-pdf-service` — the `pdf` host service (ADR 0013).
//!
//! pdfcraft's PDF engine through its own headless automation layer. Every
//! call is a fresh, stateless session whose file access is confined to the
//! call's area, first by this service's own path check and then by the
//! engine's root-confined resolver, which refuses `..`, absolute paths and
//! symlink escapes before any I/O. A reference inside a PDF (a remote
//! go-to, a launch action, an external file specification) is reported,
//! never followed, and a document's own scripts never run: every session
//! turns the engine's JavaScript off before it opens anything, so an XFA
//! form's initialize and calculate scripts (which the engine runs on
//! opening by default, in its sandbox) and field scripts stay inert.
//!
//! **Where a call works** (ADR 0013, 2026-10-08): the caller's own folder,
//! the [`Area`] the shell's resolver gives it ([`set_area_resolver`]), or
//! without one the legacy `<host dir>/pdf`. A write that may not replace (an
//! agent's) only creates new files, within the area's quota: a rendered page
//! goes through [`Area::write`], and what the engine writes itself (a merge,
//! a split) lands in a staging folder inside the area first and is moved
//! into place all or nothing ([`octosense_engine_area::Stage`]).
//!
//! Methods (all under the `pdf` family; paths relative to the area):
//! - `info {path}` → the document inspected as JSON (pages, metadata,
//!   outline, fonts, security, repair notes)
//! - `text {path, pages?}` → `{pages: [{page, text}]}` — 1-based pages,
//!   in reading order
//! - `render {path, page, out, max_side?}` → `{out, width, height,
//!   bytes}` — one page written as a PNG
//! - `merge {paths, out}` → `{out, bytes}` — the files combined, in order
//! - `split {path, out_dir, every? | before?}` → `{files}` — split every
//!   N pages, or before the given 1-based page numbers
//!
//! The service serves system apps only until ADR 0013's store capability
//! is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::path::{Component, Path, PathBuf};

use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::{Area, Slot, Stage};
use pdfcraft_automation::{Automation, Content};
use serde_json::{json, Value as Json};

/// The longest page edge `render` produces.
const MAX_RENDER_SIDE: u64 = 4096;
/// The largest PDF the service opens (bytes).
const MAX_PDF_BYTES: u64 = 128 << 20;
/// The most files one `merge` combines.
const MAX_MERGE_FILES: usize = 16;
/// The most files one `split` writes.
const MAX_SPLIT_FILES: u64 = 256;
/// The most pages one `text` call extracts.
const MAX_TEXT_PAGES: usize = 512;

/// Which apps may call the service: system apps, as Sheets and Photo.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct PdfService;

/// Register the `pdf` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(PdfService));
}

/// The shell's resolver: where each call works (`None` removes it, and
/// calls work in the legacy `<host dir>/pdf` again).
static AREAS: Slot = Slot::new();

pub fn set_area_resolver(resolver: Option<octosense_engine_area::Resolver>) {
    AREAS.set(resolver);
}

/// The `pdf.*` agent tools (ADR 0013, wave 2), in App Hub's `tools.json`
/// shape: the shell declares them for the virtual owner `os.pdf` and grants
/// the system agent its reviewed share (`crates/shell/src/host_tools/engines.rs`,
/// `crates/shell/src/system_chat/grants.rs` `ENGINE_TOOLS`).
pub const TOOLS_JSON: &str = include_str!("../tools.json");

impl HostService for PdfService {
    fn family(&self) -> &'static str {
        "pdf"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        reply.send(serve(&AREAS, &call));
    }
}

/// One call, in the area `areas` gives it.
fn serve(areas: &Slot, call: &ServiceCall) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err("The pdf service serves system apps only.".into());
    }
    let area = areas.area(call, "pdf").map_err(|e| format!("pdf: {e}"))?;
    dispatch_in(call.method(), &call.args, &area)
}

/// A fresh engine session whose every file read and write is root-confined
/// to the area by the engine's own resolver, with the document's own
/// scripts off (the engine's Preferences ▸ JavaScript switch, on by
/// default).
fn session(area: &Area) -> Result<Automation, String> {
    let mut a = Automation::new().with_root(area.root.clone()).map_err(|e| format!("pdf: {e}"))?;
    tool_json(&mut a, "js_enabled", json!({"enabled": false}), "pdf")?;
    if a.session().javascript() {
        return Err("pdf: the engine's scripting could not be turned off".into());
    }
    Ok(a)
}

fn dispatch_in(method: &str, args: &Json, area: &Area) -> Result<Json, String> {
    match method {
        "info" => info(args, area),
        "text" => text(args, area),
        "render" => render(args, area),
        "merge" => merge(args, area),
        "split" => split(args, area),
        other => Err(format!("pdf.{other} is not a method of the pdf service")),
    }
}

/// [`dispatch_in`] in the legacy area `<host_dir>/pdf`, as a call without
/// the shell's resolver works.
#[cfg(test)]
fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    let area = Area::legacy(host_dir, "pdf");
    std::fs::create_dir_all(&area.root).map_err(|e| format!("pdf: {e}"))?;
    dispatch_in(method, args, &area)
}

/// A staging path (`name` inside `stage`) as the engine names paths: relative
/// to its root, the area's.
fn staged(area: &Area, stage: &Stage<'_>, name: &str) -> Result<String, String> {
    let dir = stage.dir().strip_prefix(&area.root).map_err(|_| "pdf: the staging folder is outside the area".to_string())?;
    Ok(dir.join(name).to_string_lossy().into_owned())
}

fn need_str<'a>(args: &'a Json, key: &str, m: &str) -> Result<&'a str, String> {
    args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("{m}: `{key}` is required"))
}

/// A path strictly inside the area: relative, normal components only, and
/// the resolved deepest existing ancestor must stay under the area even
/// through symlinks (the stance the sheet service and the files host tools
/// take). The engine's resolver enforces the same bound again at I/O time.
fn contained(area: &Area, rel: &str, key: &str, m: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(format!("{m}: `{key}` is required"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("{m}: `{key}` stays inside this call's folder"));
    }
    let root = &area.root;
    let joined = root.join(rel_path);
    let check_root = root.canonicalize().map_err(|e| format!("{m}: folder: {e}"))?;
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
    let resolved = deepest.canonicalize().map_err(|e| format!("{m}: {e}"))?;
    if !resolved.starts_with(&check_root) {
        return Err(format!("{m}: `{key}` stays inside this call's folder"));
    }
    Ok(joined)
}

/// A contained output path the call may write, refused before the engine
/// works when the area's rules would refuse it.
fn out_path(area: &Area, rel: &str, key: &str, m: &str) -> Result<PathBuf, String> {
    let out = contained(area, rel, key, m)?;
    area.check(&out, 0).map_err(|e| format!("{m}: {e}"))?;
    Ok(out)
}

/// The containment check plus the size cap, for a file a method will read.
fn checked_input(area: &Area, rel: &str, key: &str, m: &str) -> Result<(), String> {
    let path = contained(area, rel, key, m)?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("{m}: {rel}: {e}"))?;
    if meta.len() > MAX_PDF_BYTES {
        return Err(format!("{m}: {rel} is larger than the service reads ({} MB)", MAX_PDF_BYTES >> 20));
    }
    Ok(())
}

/// Run one engine tool and keep its JSON result.
fn tool_json(a: &mut Automation, tool: &str, args: Json, m: &str) -> Result<Json, String> {
    let contents = a.call(tool, &args).map_err(|e| format!("{m}: {e}"))?;
    for c in contents {
        if let Content::Json(v) = c {
            return Ok(v);
        }
    }
    Err(format!("{m}: the engine returned no result"))
}

/// Open `path` (already checked) and return the session's document id and
/// page count.
fn open(a: &mut Automation, path: &str, m: &str) -> Result<(Json, u64), String> {
    let opened = tool_json(a, "doc_open", json!({ "path": path }), m)?;
    let pages = opened["pages"].as_u64().unwrap_or(0);
    Ok((opened["doc"].clone(), pages))
}

fn info(args: &Json, area: &Area) -> Result<Json, String> {
    const M: &str = "pdf.info";
    let path = need_str(args, "path", M)?;
    checked_input(area, path, "path", M)?;
    let mut a = session(area)?;
    let (doc, _) = open(&mut a, path, M)?;
    let mut out = tool_json(&mut a, "doc_info", json!({ "doc": doc }), M)?;
    out["file"] = json!(path);
    Ok(out)
}

fn text(args: &Json, area: &Area) -> Result<Json, String> {
    const M: &str = "pdf.text";
    let path = need_str(args, "path", M)?;
    checked_input(area, path, "path", M)?;
    let mut a = session(area)?;
    let (doc, total) = open(&mut a, path, M)?;
    let call_args = match &args["pages"] {
        Json::Null => {
            if total as usize > MAX_TEXT_PAGES {
                return Err(format!("{M}: the document has {total} pages; pass `pages` ({MAX_TEXT_PAGES} per call)"));
            }
            json!({ "doc": doc })
        }
        Json::Array(list) if !list.is_empty() && list.len() <= MAX_TEXT_PAGES => {
            json!({ "doc": doc, "pages": list })
        }
        Json::Array(_) => return Err(format!("{M}: `pages` is 1..={MAX_TEXT_PAGES} page numbers")),
        other => return Err(format!("{M}: `pages` is a list of 1-based page numbers, not {other}")),
    };
    tool_json(&mut a, "text_extract", call_args, M)
}

fn render(args: &Json, area: &Area) -> Result<Json, String> {
    const M: &str = "pdf.render";
    let path = need_str(args, "path", M)?;
    let out = need_str(args, "out", M)?;
    let page = args["page"].as_u64().filter(|p| *p >= 1).ok_or(format!("{M}: `page` is a 1-based page number"))?;
    let max_side = args["max_side"].as_u64().unwrap_or(1024).clamp(16, MAX_RENDER_SIDE);
    checked_input(area, path, "path", M)?;
    let out_abs = out_path(area, out, "out", M)?;
    let mut a = session(area)?;
    let (doc, _) = open(&mut a, path, M)?;
    // The page's size in points decides the dpi that fits `max_side`.
    let inspected = tool_json(&mut a, "doc_info", json!({ "doc": doc }), M)?;
    let dims = &inspected["pages"][(page - 1) as usize];
    let side = dims["width"].as_f64().unwrap_or(0.0).max(dims["height"].as_f64().unwrap_or(0.0));
    if side <= 0.0 {
        return Err(format!("{M}: the document has no page {page}"));
    }
    let dpi = (72.0 * max_side as f64 / side).clamp(1.0, 600.0);
    let contents =
        a.call("page_render", &json!({ "doc": doc, "page": page, "dpi": dpi })).map_err(|e| format!("{M}: {e}"))?;
    let Some(Content::Png { data, width, height }) =
        contents.into_iter().find(|c| matches!(c, Content::Png { .. }))
    else {
        return Err(format!("{M}: the engine returned no image"));
    };
    area.write(&out_abs, &data).map_err(|e| format!("{M}: {e}"))?;
    Ok(json!({ "out": out, "width": width, "height": height, "bytes": data.len() }))
}

fn merge(args: &Json, area: &Area) -> Result<Json, String> {
    const M: &str = "pdf.merge";
    let out = need_str(args, "out", M)?;
    let out_abs = out_path(area, out, "out", M)?;
    let paths = args["paths"].as_array().ok_or(format!("{M}: `paths` is a list of files"))?;
    if paths.len() < 2 || paths.len() > MAX_MERGE_FILES {
        return Err(format!("{M}: `paths` is 2..={MAX_MERGE_FILES} files"));
    }
    for p in paths {
        let p = p.as_str().filter(|s| !s.is_empty()).ok_or(format!("{M}: every path is a file name"))?;
        checked_input(area, p, "paths", M)?;
    }
    // The engine writes the merge itself: into a staging folder, then into
    // place under the area's rules.
    let stage = area.stage().map_err(|e| format!("{M}: {e}"))?;
    let mut a = session(area)?;
    let combined = tool_json(&mut a, "doc_combine", json!({ "paths": paths, "out": staged(area, &stage, "merged.pdf")?, "open": false }), M)?;
    stage.commit(&[(stage.path("merged.pdf"), out_abs)]).map_err(|e| format!("{M}: {e}"))?;
    Ok(json!({ "out": out, "bytes": combined["bytes"] }))
}

fn split(args: &Json, area: &Area) -> Result<Json, String> {
    const M: &str = "pdf.split";
    let path = need_str(args, "path", M)?;
    let out_dir = need_str(args, "out_dir", M)?;
    checked_input(area, path, "path", M)?;
    let out_dir_abs = contained(area, out_dir, "out_dir", M)?;
    let every = args["every"].as_u64();
    let before = args["before"].as_array();
    let mut a = session(area)?;
    let (doc, total) = open(&mut a, path, M)?;
    // The engine writes the parts itself: into a staging folder, then all
    // of them into `out_dir` under the area's rules, or none.
    let stage = area.stage().map_err(|e| format!("{M}: {e}"))?;
    let staging = staged(area, &stage, "")?;
    let call_args = match (every, before) {
        (Some(n), None) if n >= 1 => {
            if total.div_ceil(n) > MAX_SPLIT_FILES {
                return Err(format!("{M}: splitting {total} pages every {n} makes too many files (at most {MAX_SPLIT_FILES})"));
            }
            json!({ "doc": doc, "out_dir": staging, "every": n })
        }
        (None, Some(b)) if !b.is_empty() && (b.len() as u64) < MAX_SPLIT_FILES => {
            json!({ "doc": doc, "out_dir": staging, "before": b })
        }
        _ => return Err(format!("{M}: pass exactly one of `every` (pages per file, ≥ 1) or `before` (1-based page numbers)")),
    };
    let parts = tool_json(&mut a, "doc_split", call_args, M)?;
    let moves: Vec<(PathBuf, PathBuf)> = stage.files().into_iter().map(|rel| (stage.path(&rel), out_dir_abs.join(rel))).collect();
    stage.commit(&moves).map_err(|e| format!("{M}: {e}"))?;
    // The engine reports where it wrote, in the staging folder; callers
    // speak area-relative paths, where the parts are now.
    let files: Vec<Json> = parts["files"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|mut f| {
            if let Some(name) = f["path"].as_str().and_then(|p| Path::new(p).file_name()) {
                f["path"] = json!(Path::new(out_dir).join(name).to_string_lossy());
            }
            f
        })
        .collect();
    Ok(json!({ "files": files }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The value of form field `name` in `path`, as `session` opens it.
    fn field(a: &mut Automation, path: &str, name: &str) -> Json {
        let (doc, _) = open(a, path, "test").unwrap();
        let fields = tool_json(a, "form_fields", json!({"doc": doc}), "test").unwrap();
        let list = if fields.is_array() { fields } else { fields["fields"].clone() };
        list.as_array().unwrap().iter().find(|f| f["name"] == name).map(|f| f["value"].clone()).unwrap_or_else(|| panic!("no field {name}: {list}"))
    }

    /// A document's own scripts never run in a service session. The
    /// hostile fixture is the engine's own scripted XFA form, whose
    /// initialize script sets `qty` when the form opens with scripting on,
    /// as the engine's default session does; through the service it opens
    /// as the file has it, by every method that opens it.
    #[test]
    fn a_documents_own_scripts_never_run() {
        let dir = tempfile::tempdir().unwrap();
        let form = pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::scripted_template());
        std::fs::write(dir.path().join("form.pdf"), &form).unwrap();
        let mut scripted = Automation::new().with_root(dir.path()).unwrap();
        assert!(scripted.session().javascript(), "the engine runs scripts by default");
        assert_eq!(field(&mut scripted, "form.pdf", "qty"), json!("2"), "the fixture's script runs in the engine's default session");
        let area = Area::new(dir.path(), None, false);
        let mut a = session(&area).unwrap();
        assert!(!a.session().javascript(), "every service session turns scripts off");
        assert_ne!(field(&mut a, "form.pdf", "qty"), json!("2"), "no script ran");
        dispatch_in("info", &json!({"path": "form.pdf"}), &area).unwrap();
        dispatch_in("text", &json!({"path": "form.pdf"}), &area).unwrap();
        dispatch_in("render", &json!({"path": "form.pdf", "page": 1, "out": "form.png", "max_side": 64}), &area).unwrap();
    }

    /// A tiny, valid two-page PDF written by this test: Helvetica text on
    /// each page, exact stream lengths and xref offsets, no compression —
    /// so the suite needs no fixtures on disk.
    fn tiny_pdf(page1: &str, page2: &str) -> Vec<u8> {
        let (s1, s2) = (
            format!("BT /F1 12 Tf 20 50 Td ({page1}) Tj ET"),
            format!("BT /F1 12 Tf 20 50 Td ({page2}) Tj ET"),
        );
        let page = |contents: u32| {
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] \
                 /Resources << /Font << /F1 7 0 R >> >> /Contents {contents} 0 R >>"
            )
        };
        let stream = |s: &str| format!("<< /Length {} >>\nstream\n{s}\nendstream", s.len());
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".to_string(),
            page(4),
            stream(&s1),
            page(6),
            stream(&s2),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in objs.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend(format!("{} 0 obj\n{body}\nendobj\n", i + 1).bytes());
        }
        let xref = pdf.len();
        pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).bytes());
        for off in &offsets {
            pdf.extend(format!("{off:010} 00000 n \n").bytes());
        }
        pdf.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF", objs.len() + 1).bytes());
        pdf
    }

    #[test]
    fn info_text_render_merge_split_on_a_real_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        std::fs::create_dir_all(host.join("pdf")).unwrap();
        std::fs::write(host.join("pdf/a.pdf"), tiny_pdf("Hello PDF", "Page two")).unwrap();
        std::fs::write(host.join("pdf/b.pdf"), tiny_pdf("Second file", "Last page")).unwrap();

        let doc = dispatch("info", &json!({"path": "a.pdf"}), host).unwrap();
        assert_eq!(doc["document"]["pages"], json!(2), "{doc}");
        assert_eq!(doc["file"], json!("a.pdf"));
        assert_eq!(doc["pages"][0]["width"], json!(200.0), "{doc}");

        let text = dispatch("text", &json!({"path": "a.pdf"}), host).unwrap();
        let all = text["pages"].to_string();
        assert!(all.contains("Hello PDF") && all.contains("Page two"), "{text}");
        let one = dispatch("text", &json!({"path": "a.pdf", "pages": [2]}), host).unwrap();
        assert_eq!(one["pages"][0]["page"], json!(2), "{one}");
        assert!(one["pages"][0]["text"].as_str().unwrap().contains("Page two"), "{one}");

        let rend = dispatch("render", &json!({"path": "a.pdf", "page": 1, "out": "prev.png", "max_side": 64}), host).unwrap();
        assert_eq!(rend["out"], json!("prev.png"));
        assert!(rend["bytes"].as_u64().unwrap() > 0, "{rend}");
        let (w, h) = (rend["width"].as_u64().unwrap(), rend["height"].as_u64().unwrap());
        assert!((16..=65).contains(&w.max(h)), "{rend}");
        assert!(host.join("pdf/prev.png").metadata().unwrap().len() > 0, "the PNG lands inside the area");

        let merged = dispatch("merge", &json!({"paths": ["a.pdf", "b.pdf"], "out": "m.pdf"}), host).unwrap();
        assert_eq!(merged["out"], json!("m.pdf"));
        assert!(merged["bytes"].as_u64().unwrap() > 0, "{merged}");
        let minfo = dispatch("info", &json!({"path": "m.pdf"}), host).unwrap();
        assert_eq!(minfo["document"]["pages"], json!(4), "{minfo}");

        let split = dispatch("split", &json!({"path": "m.pdf", "out_dir": "parts", "every": 2}), host).unwrap();
        let files = split["files"].as_array().unwrap();
        assert_eq!(files.len(), 2, "{split}");
        assert_eq!(files[0]["first_page"], json!(1), "{split}");
        assert_eq!(files[1]["last_page"], json!(4), "{split}");
        for f in files {
            let rel = f["path"].as_str().unwrap();
            assert!(!rel.starts_with('/'), "paths come back area-relative: {rel}");
            assert!(host.join("pdf").join(rel).metadata().unwrap().len() > 0, "{rel}");
        }
    }

    #[test]
    fn paths_outside_the_pdf_area_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        for bad in ["../up.pdf", "/etc/up.pdf", "a/../../up.pdf", ""] {
            assert!(dispatch("info", &json!({"path": bad}), host).is_err(), "{bad}");
            assert!(dispatch("text", &json!({"path": bad}), host).is_err(), "{bad}");
            assert!(dispatch("render", &json!({"path": bad, "page": 1, "out": "o.png"}), host).is_err(), "{bad}");
            assert!(dispatch("merge", &json!({"paths": [bad, bad], "out": "m.pdf"}), host).is_err(), "{bad}");
            assert!(dispatch("split", &json!({"path": bad, "out_dir": "p", "every": 1}), host).is_err(), "{bad}");
        }
        // Outputs are confined too, even with a fine input.
        std::fs::create_dir_all(host.join("pdf")).unwrap();
        std::fs::write(host.join("pdf/a.pdf"), tiny_pdf("x", "y")).unwrap();
        for bad in ["../o.png", "/tmp/o.png", "a/../../o.png"] {
            assert!(dispatch("render", &json!({"path": "a.pdf", "page": 1, "out": bad}), host).is_err(), "{bad}");
            assert!(dispatch("merge", &json!({"paths": ["a.pdf", "a.pdf"], "out": bad}), host).is_err(), "{bad}");
            assert!(dispatch("split", &json!({"path": "a.pdf", "out_dir": bad, "every": 1}), host).is_err(), "{bad}");
        }
    }

    #[test]
    fn area_is_the_pdf_subdirectory_of_the_host_dir() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let a = Slot::new().area(&service_call("info", json!({}), host, false), "pdf").unwrap();
        assert_eq!(a.root, host.join("pdf"));
        assert!(a.root.is_dir(), "created on first use");
        std::fs::write(a.root.join("a.pdf"), tiny_pdf("in the area", "p2")).unwrap();
        for _ in 0..2 {
            serve(&Slot::new(), &service_call("render", json!({"path": "a.pdf", "page": 1, "out": "o.png"}), host, false)).unwrap();
        }
        assert!(a.root.join("o.png").is_file(), "writes land inside the area, replacing as before");
        assert!(!host.join("o.png").exists(), "nothing lands beside the area in the shared host dir");
    }

    /// A call as App Hub hands it to the service.
    fn service_call(method: &str, args: Json, host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall { app_id: "os.fixture".into(), service: format!("pdf.{method}"), args, from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
    }

    /// A resolver shaped like the shell's: every call works in `root`, an
    /// app's own foreground call may replace a file and an agent's may not,
    /// within `quota`.
    fn resolver(root: &Path, quota: Option<u64>) -> Slot {
        let slot = Slot::new();
        let root = root.to_path_buf();
        slot.set(Some(std::sync::Arc::new(move |call: &ServiceCall| Ok(Area::new(&root, quota, call.may_prompt)))));
        slot
    }

    fn no_staging_left(root: &Path) -> bool {
        std::fs::read_dir(root).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with(octosense_engine_area::STAGING_PREFIX))
    }

    /// With the shell's resolver every path is relative to the caller's own
    /// folder and stays inside it, through a link too; what the engine
    /// writes itself lands where it was asked, and no staging folder stays.
    #[test]
    fn the_resolver_root_is_used_and_paths_stay_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("a.pdf"), tiny_pdf("Hello PDF", "Page two")).unwrap();
        std::fs::write(root.join("b.pdf"), tiny_pdf("Second file", "Last page")).unwrap();
        let areas = resolver(&root, None);
        let host = dir.path().join(".host");
        let doc = serve(&areas, &service_call("info", json!({"path": "a.pdf"}), &host, false)).unwrap();
        assert_eq!(doc["document"]["pages"], json!(2), "{doc}");
        let merged = serve(&areas, &service_call("merge", json!({"paths": ["a.pdf", "b.pdf"], "out": "out/m.pdf"}), &host, false)).unwrap();
        assert!(merged["bytes"].as_u64().unwrap() > 0, "{merged}");
        assert!(root.join("out/m.pdf").is_file());
        let split = serve(&areas, &service_call("split", json!({"path": "out/m.pdf", "out_dir": "parts", "every": 2}), &host, false)).unwrap();
        let files = split["files"].as_array().unwrap();
        assert_eq!(files.len(), 2, "{split}");
        for f in files {
            let rel = f["path"].as_str().unwrap();
            assert!(rel.starts_with("parts/") && root.join(rel).is_file(), "{rel}");
        }
        serve(&areas, &service_call("render", json!({"path": "a.pdf", "page": 1, "out": "p1.png", "max_side": 32}), &host, false)).unwrap();
        assert!(root.join("p1.png").is_file() && !host.exists() && !root.join("pdf").exists());
        assert!(no_staging_left(&root));
        std::fs::write(dir.path().join("beside.pdf"), tiny_pdf("x", "y")).unwrap();
        for bad in ["../beside.pdf", "/etc/hosts", "out/../../beside.pdf"] {
            assert!(serve(&areas, &service_call("info", json!({"path": bad}), &host, false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("merge", json!({"paths": ["a.pdf", "b.pdf"], "out": bad}), &host, true)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("split", json!({"path": "a.pdf", "out_dir": bad, "every": 1}), &host, true)).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            assert!(serve(&areas, &service_call("info", json!({"path": "up/beside.pdf"}), &host, false)).is_err());
            assert!(serve(&areas, &service_call("render", json!({"path": "a.pdf", "page": 1, "out": "up/o.png"}), &host, true)).is_err());
            assert!(serve(&areas, &service_call("split", json!({"path": "a.pdf", "out_dir": "up", "every": 1}), &host, true)).is_err());
            assert!(!dir.path().join("o.png").exists() && !dir.path().join("a-part1.pdf").exists());
        }
    }

    /// An agent's call never replaces a file, a split's parts included (all
    /// or none land); an app's own foreground call may.
    #[test]
    fn an_agent_never_replaces_a_file_and_an_app_may() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.pdf"), tiny_pdf("Hello PDF", "Page two")).unwrap();
        std::fs::write(dir.path().join("taken.pdf"), b"keep me").unwrap();
        let areas = resolver(dir.path(), None);
        let refused = serve(&areas, &service_call("merge", json!({"paths": ["a.pdf", "a.pdf"], "out": "taken.pdf"}), dir.path(), false)).unwrap_err();
        assert!(refused.contains("`taken.pdf` already exists"), "{refused}");
        std::fs::write(dir.path().join("p.png"), b"keep").unwrap();
        assert!(serve(&areas, &service_call("render", json!({"path": "a.pdf", "page": 1, "out": "p.png"}), dir.path(), false)).is_err());
        // One part's name is taken: no part lands.
        std::fs::create_dir(dir.path().join("parts")).unwrap();
        std::fs::write(dir.path().join("parts/a-part2.pdf"), b"theirs").unwrap();
        let refused = serve(&areas, &service_call("split", json!({"path": "a.pdf", "out_dir": "parts", "every": 1}), dir.path(), false)).unwrap_err();
        assert!(refused.contains("already exists"), "{refused}");
        assert!(!dir.path().join("parts/a-part1.pdf").exists(), "all or nothing");
        assert_eq!(std::fs::read(dir.path().join("parts/a-part2.pdf")).unwrap(), b"theirs");
        assert_eq!(std::fs::read(dir.path().join("taken.pdf")).unwrap(), b"keep me");
        assert!(no_staging_left(dir.path()));
        serve(&areas, &service_call("merge", json!({"paths": ["a.pdf", "a.pdf"], "out": "taken.pdf"}), dir.path(), true)).unwrap();
        assert!(std::fs::read(dir.path().join("taken.pdf")).unwrap().starts_with(b"%PDF"));
        serve(&areas, &service_call("split", json!({"path": "a.pdf", "out_dir": "parts", "every": 1}), dir.path(), true)).unwrap();
        assert!(std::fs::read(dir.path().join("parts/a-part2.pdf")).unwrap().starts_with(b"%PDF"));
    }

    /// What a call writes, the engine's own output included, must fit what
    /// is left of the area's quota.
    #[test]
    fn output_over_the_quota_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.pdf"), tiny_pdf("Hello PDF", "Page two")).unwrap();
        let tight = resolver(dir.path(), Some(64));
        for (method, args) in [
            ("merge", json!({"paths": ["a.pdf", "a.pdf"], "out": "m.pdf"})),
            ("split", json!({"path": "a.pdf", "out_dir": "parts", "every": 1})),
            ("render", json!({"path": "a.pdf", "page": 1, "out": "p.png"})),
        ] {
            let refused = serve(&tight, &service_call(method, args, dir.path(), true)).unwrap_err();
            assert!(refused.contains("bytes left"), "{method}: {refused}");
        }
        assert!(!dir.path().join("m.pdf").exists() && !dir.path().join("p.png").exists() && !dir.path().join("parts/a-part1.pdf").exists());
        assert!(no_staging_left(dir.path()));
    }

    #[test]
    fn only_system_apps_may_call() {
        assert!(may_call("os.files"));
        assert!(!may_call("org.example.app"));
        assert!(!may_call(""));
    }

    /// The agent tools (`tools.json`) pass App Hub's own loader, as the shell
    /// reads them, keep the object schemas octos takes both ways, and name
    /// only methods this service dispatches.
    #[test]
    fn the_agent_tools_pass_app_hubs_loader_and_name_real_methods() {
        use octosense_app_policy::{ImplementedBy, ToolHost, ToolManifest};
        let (manifest, _) = ToolManifest::load(TOOLS_JSON, "pdf", ToolHost::Contained, false).unwrap();
        assert!(!manifest.tools.is_empty());
        let dir = tempfile::tempdir().unwrap();
        for tool in &manifest.tools {
            assert_eq!(tool.implemented_by, ImplementedBy::HostService, "{}", tool.name);
            assert!(tool.host_method.is_none(), "{}: the shell routes each tool to the method of its own name", tool.name);
            for schema in [&tool.input_schema, &tool.output_schema] {
                assert_eq!(schema["type"], json!("object"), "{}: octos takes object schemas only", tool.name);
            }
            assert!(tool.description.is_ascii(), "{}: descriptions stay within octos's byte limit", tool.name);
            let method = tool.name.strip_prefix("pdf.").unwrap();
            if let Err(e) = dispatch(method, &json!({}), dir.path()) {
                assert!(!e.contains("is not a method"), "{}: {e}", tool.name);
            }
        }
    }
}
