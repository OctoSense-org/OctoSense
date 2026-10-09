//! `octosense-word-service` — the `word` host service (ADR 0013).
//!
//! wordcraft's document engine behind typed `word.*` methods. Every call
//! is a fresh, stateless session: the service reads the file itself,
//! hands the engine bytes, and writes the engine's bytes back — the
//! engine never touches the disk. Everything is JSON at the boundary; the
//! engine's types never cross it.
//!
//! Methods (all under the `word` family; paths relative to the service's
//! own `word/` area under the caller's host directory):
//! - `info {path}` → pages, words, paragraphs, sections, comments and
//!   properties of a document (docx, md, html, rtf, odt, txt, json)
//! - `text {path}` → `{text, words, paragraphs}` — plain-text extraction
//! - `inspect {path, text?}` → the document's structure for agents:
//!   paragraphs with styles and runs, tables with cells
//! - `convert {path, out, format?}` → `{out, format, bytes}` — write the
//!   document as docx, md, html, rtf, odt, txt, json, pdf or png;
//!   `format` overrides `out`'s extension
//! - `new {out, text?, title?}` → `{out, words, paragraphs}` — write a
//!   minimal new document (format by `out`'s extension, usually docx)
//!
//! Paths never leave `<host dir>/word/`: `..`, absolute paths and symlink
//! escapes are refused — the shared `.host` directory also holds Mail's
//! and Calendar's data, which this service must not see. The service
//! serves system apps only until ADR 0013's store capability is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::path::{Component, Path, PathBuf};

use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use serde_json::{json, Value as Json};
use wordcraft_doc::{Document, StoryRef};
use wordcraft_engine::Session;

/// The largest document the service reads or writes (bytes).
const MAX_DOC_BYTES: u64 = 64 << 20;
/// The most text `new` accepts (bytes).
const MAX_NEW_TEXT_BYTES: usize = 4 << 20;

/// Which apps may call the service: system apps, as News and Sheets.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct WordService;

/// Register the `word` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(WordService));
}

/// The `word.*` agent tools (ADR 0013, wave 2), in App Hub's `tools.json`
/// shape: the shell declares them for the virtual owner `os.word` and grants
/// the system agent its reviewed share (`crates/shell/src/host_tools/engines.rs`,
/// `crates/shell/src/system_chat/grants.rs` `ENGINE_TOOLS`).
pub const TOOLS_JSON: &str = include_str!("../tools.json");

impl HostService for WordService {
    fn family(&self) -> &'static str {
        "word"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        if !may_call(&call.app_id) {
            reply.send(Err("The word service serves system apps only.".into()));
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
        "text" => text(args, host_dir),
        "inspect" => inspect(args, host_dir),
        "convert" => convert(args, host_dir),
        "new" => new(args, host_dir),
        other => Err(format!("word.{other} is not a method of the word service")),
    }
}

/// The service's own area, `<host_dir>/word`, created on first use. The
/// host directory itself is shared (`.host` holds Mail's and Calendar's
/// data), so every path the service touches is contained in this
/// subdirectory.
fn area(host_dir: &Path) -> Result<PathBuf, String> {
    let area = host_dir.join("word");
    std::fs::create_dir_all(&area).map_err(|e| format!("word: {e}"))?;
    Ok(area)
}

/// A path strictly inside the `word/` area: relative, no `..`, no
/// absolute component; the resolved parent must stay under the area even
/// through symlinks.
fn contained(host_dir: &Path, key: &str, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(format!("word: `{key}` is required"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("word: `{key}` stays inside the service's `word/` area"));
    }
    let area = area(host_dir)?;
    let joined = area.join(rel_path);
    let check_root = area.canonicalize().map_err(|e| format!("word: area: {e}"))?;
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
    let resolved = deepest.canonicalize().map_err(|e| format!("word: {e}"))?;
    if !resolved.starts_with(&check_root) {
        return Err(format!("word: `{key}` stays inside the service's `word/` area"));
    }
    Ok(joined)
}

fn arg_str<'a>(args: &'a Json, key: &str) -> Result<&'a str, String> {
    args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("word: `{key}` is required"))
}

/// Read and parse the document at `path` (relative to the area); the
/// engine parses bytes, the service does the I/O.
fn open(args: &Json, host_dir: &Path) -> Result<(Document, String), String> {
    let rel = arg_str(args, "path")?;
    let path = contained(host_dir, "path", rel)?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("word: {rel}: {e}"))?;
    if meta.len() > MAX_DOC_BYTES {
        return Err(format!("word: {rel} is larger than the service reads"));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("word: {rel}: {e}"))?;
    let doc = wordcraft_engine::io::open_bytes(rel, &bytes)?;
    Ok((doc, rel.to_string()))
}

/// Serialise `doc` in `name`'s format (by extension) and write it to the
/// contained `out` path, creating parents inside the area.
fn write(doc: &Document, name: &str, out: &Path) -> Result<usize, String> {
    let bytes = wordcraft_engine::io::save_bytes(name, doc)?;
    if bytes.len() as u64 > MAX_DOC_BYTES {
        return Err("word: the document is larger than the service writes".into());
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("word: {e}"))?;
    }
    std::fs::write(out, &bytes).map_err(|e| format!("word: {e}"))?;
    Ok(bytes.len())
}

fn info(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let (doc, rel) = open(args, host_dir).map_err(|e| format!("word.info: {e}"))?;
    let mut s = Session::new(doc);
    s.path = Some(rel.clone().into());
    let mut r = s.run("file.info", &json!({})).map_err(|e| format!("word.info: {e}"))?;
    r["file"] = json!(rel);
    Ok(r)
}

fn text(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let (doc, _) = open(args, host_dir).map_err(|e| format!("word.text: {e}"))?;
    Ok(json!({
        "text": doc.plain_text(StoryRef::Body),
        "words": doc.word_count(),
        "paragraphs": doc.paragraph_count(),
    }))
}

fn inspect(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let (doc, rel) = open(args, host_dir).map_err(|e| format!("word.inspect: {e}"))?;
    let mut s = Session::new(doc);
    s.path = Some(rel.into());
    let with_text = args["text"].as_bool().unwrap_or(true);
    s.run("document.inspect", &json!({"text": with_text})).map_err(|e| format!("word.inspect: {e}"))
}

fn convert(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let (doc, _) = open(args, host_dir).map_err(|e| format!("word.convert: {e}"))?;
    let out_rel = arg_str(args, "out").map_err(|e| format!("word.convert: {e}"))?;
    let out = contained(host_dir, "out", out_rel).map_err(|e| format!("word.convert: {e}"))?;
    // `format` overrides the extension of `out`; the engine's own format
    // dispatch decides what it can save.
    let name = match args["format"].as_str().map(|f| f.trim_start_matches('.')).filter(|f| !f.is_empty()) {
        Some(fmt) => format!("out.{fmt}"),
        None => out_rel.to_string(),
    };
    let bytes = write(&doc, &name, &out).map_err(|e| format!("word.convert: {e}"))?;
    let format = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    Ok(json!({"out": out_rel, "format": format, "bytes": bytes}))
}

fn new(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let out_rel = arg_str(args, "out").map_err(|e| format!("word.new: {e}"))?;
    let out = contained(host_dir, "out", out_rel).map_err(|e| format!("word.new: {e}"))?;
    let text = args["text"].as_str().unwrap_or("");
    if text.len() > MAX_NEW_TEXT_BYTES {
        return Err("word.new: `text` is larger than the service accepts".into());
    }
    let mut doc = Document::from_text(text);
    if let Some(title) = args["title"].as_str() {
        doc.core.title = title.to_string();
    }
    write(&doc, out_rel, &out).map_err(|e| format!("word.new: {e}"))?;
    Ok(json!({"out": out_rel, "words": doc.word_count(), "paragraphs": doc.paragraph_count()}))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A docx written by the engine itself: the suite needs no fixtures
    /// on disk.
    fn fixture(host: &Path) -> &'static str {
        let made = dispatch(
            "new",
            &json!({"out": "in.docx", "text": "Hello wordcraft\nA second paragraph for the fixture.", "title": "Fixture"}),
            host,
        )
        .unwrap();
        assert_eq!(made["out"], json!("in.docx"), "{made}");
        "in.docx"
    }

    #[test]
    fn new_writes_a_docx_the_engine_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let made = dispatch("new", &json!({"out": "a/fresh.docx", "text": "One\nTwo"}), host).unwrap();
        assert_eq!(made["paragraphs"], json!(2), "{made}");
        assert_eq!(made["words"], json!(2));
        // The file is real and lands inside the area, not beside it.
        assert!(host.join("word/a/fresh.docx").metadata().unwrap().len() > 0);
        assert!(!host.join("a/fresh.docx").exists());
        let back = dispatch("text", &json!({"path": "a/fresh.docx"}), host).unwrap();
        assert_eq!(back["text"].as_str().unwrap().trim(), "One\nTwo");
    }

    #[test]
    fn info_reports_pages_words_and_properties() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);
        let doc = dispatch("info", &json!({"path": input}), host).unwrap();
        assert_eq!(doc["file"], json!(input), "{doc}");
        assert_eq!(doc["paragraphs"], json!(2));
        assert_eq!(doc["words"], json!(8));
        assert!(doc["pages"].as_u64().unwrap() >= 1);
        assert_eq!(doc["properties"]["title"], json!("Fixture"));
    }

    #[test]
    fn text_extracts_the_plain_text() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);
        let got = dispatch("text", &json!({"path": input}), host).unwrap();
        let text = got["text"].as_str().unwrap();
        assert!(text.contains("Hello wordcraft"), "{text}");
        assert!(text.contains("A second paragraph"), "{text}");
        assert_eq!(got["paragraphs"], json!(2));
    }

    #[test]
    fn inspect_lists_the_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);
        let got = dispatch("inspect", &json!({"path": input}), host).unwrap();
        let blocks = got["blocks"].as_array().expect("blocks");
        assert_eq!(blocks.len(), 2, "{got}");
        assert_eq!(blocks[0]["type"], json!("paragraph"));
        assert_eq!(blocks[0]["text"], json!("Hello wordcraft"));
        // Without text, the structure stays and the text goes.
        let bare = dispatch("inspect", &json!({"path": input, "text": false}), host).unwrap();
        assert!(bare["blocks"][0]["text"].is_null(), "{bare}");
    }

    #[test]
    fn convert_round_trips_docx_md_and_txt() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);

        let md = dispatch("convert", &json!({"path": input, "out": "out.md"}), host).unwrap();
        assert_eq!(md["format"], json!("md"), "{md}");
        let md_text = std::fs::read_to_string(host.join("word/out.md")).unwrap();
        assert!(md_text.contains("Hello wordcraft"), "{md_text}");

        // `format` overrides the extension: plain text into a .log name.
        let txt = dispatch("convert", &json!({"path": input, "out": "notes.log", "format": "txt"}), host).unwrap();
        assert_eq!(txt["format"], json!("txt"));
        let log = std::fs::read_to_string(host.join("word/notes.log")).unwrap();
        assert!(log.contains("A second paragraph"), "{log}");

        // Markdown reads back in as a document: a real docx comes out.
        let back = dispatch("convert", &json!({"path": "out.md", "out": "back.docx"}), host).unwrap();
        assert!(back["bytes"].as_u64().unwrap() > 0);
        let info = dispatch("info", &json!({"path": "back.docx"}), host).unwrap();
        assert_eq!(info["paragraphs"], json!(2), "{info}");

        // A format the engine cannot save is the engine's error, prefixed.
        let bad = dispatch("convert", &json!({"path": input, "out": "x.xyz"}), host).unwrap_err();
        assert!(bad.starts_with("word.convert: "), "{bad}");
    }

    #[test]
    fn the_area_is_the_word_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        assert_eq!(area(host).unwrap(), host.join("word"));
        assert!(host.join("word").is_dir(), "created on first use");
        // A sibling of the area — another service's file in the shared
        // host dir — is out of reach by name.
        std::fs::write(host.join("events.json"), b"calendar data").unwrap();
        let miss = dispatch("text", &json!({"path": "events.json"}), host).unwrap_err();
        assert!(miss.starts_with("word.text: "), "{miss}");
    }

    #[test]
    fn paths_stay_inside_the_area() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        std::fs::write(host.join("up.docx"), b"outside").unwrap();
        for bad in ["../up.docx", "/etc/x.docx", "a/../../up.docx", ""] {
            assert!(dispatch("info", &json!({"path": bad}), host).is_err(), "{bad}");
            assert!(dispatch("new", &json!({"out": bad, "text": "x"}), host).is_err(), "{bad}");
            assert!(dispatch("convert", &json!({"path": "in.docx", "out": bad}), host).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_system_apps_may_call() {
        assert!(may_call("os.notes"));
        assert!(!may_call("org.example.anything"));
        assert!(!may_call(""));
    }

    /// The agent tools (`tools.json`) pass App Hub's own loader, as the shell
    /// reads them, keep the object schemas octos takes both ways, and name
    /// only methods this service dispatches.
    #[test]
    fn the_agent_tools_pass_app_hubs_loader_and_name_real_methods() {
        use octosense_app_policy::{ImplementedBy, ToolHost, ToolManifest};
        let (manifest, _) = ToolManifest::load(TOOLS_JSON, "word", ToolHost::Contained, false).unwrap();
        assert!(!manifest.tools.is_empty());
        let dir = tempfile::tempdir().unwrap();
        for tool in &manifest.tools {
            assert_eq!(tool.implemented_by, ImplementedBy::HostService, "{}", tool.name);
            assert!(tool.host_method.is_none(), "{}: the shell routes each tool to the method of its own name", tool.name);
            for schema in [&tool.input_schema, &tool.output_schema] {
                assert_eq!(schema["type"], json!("object"), "{}: octos takes object schemas only", tool.name);
            }
            assert!(tool.description.is_ascii(), "{}: descriptions stay within octos's byte limit", tool.name);
            let method = tool.name.strip_prefix("word.").unwrap();
            if let Err(e) = dispatch(method, &json!({}), dir.path()) {
                assert!(!e.contains("is not a method"), "{}: {e}", tool.name);
            }
        }
    }
}
