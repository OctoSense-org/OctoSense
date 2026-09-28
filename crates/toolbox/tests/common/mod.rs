#![allow(dead_code)]

use octosense_toolbox::fixture::FixtureCase;
use octosense_toolbox::library::{Template, TemplateOrigin, BUILTIN};
use octosense_toolbox::{AppContext, Library};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

pub fn library() -> Library {
    Library::builtin().unwrap()
}

pub fn template(id: &str) -> Template {
    let (_, manifest, source) = BUILTIN.iter().find(|(i, _, _)| *i == id).unwrap();
    Template::from_parts(manifest, source, TemplateOrigin::Library).unwrap()
}

pub fn case(id: &str, name: &str) -> FixtureCase {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("templates")
        .join(id)
        .join("fixtures")
        .join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A fresh, empty directory under the system temp dir.
pub fn temp_dir(tag: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "octosense-toolbox-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn app(folder: &Path) -> AppContext {
    AppContext::new("os.news", folder).grant("research")
}

/// A minimal manifest declaring `methods` of mod.research, with a free-form
/// output.
pub fn manifest(methods: &[&str]) -> Value {
    json!({
        "id": "probe", "version": "1", "title": "Probe", "description": "",
        "params": {"type": "object"},
        "modules": [{"module": "research", "methods": methods}],
        "budget": {"max_calls": 8, "max_model_calls": 2, "max_reads": 8, "max_ms": 5000, "max_concurrency": 4},
        "output": {"type": "object"},
        "provenance": true
    })
}

pub fn probe(methods: &[&str], source: &str) -> octosense_toolbox::Result<Template> {
    Template::from_parts(
        &manifest(methods).to_string(),
        source,
        TemplateOrigin::Library,
    )
}
