//! Manifest validation and the static check: what a template may declare and
//! reference, and what is refused before anything runs.

mod common;

use common::{manifest, probe};
use octosense_toolbox::library::{Template, TemplateOrigin};
use octosense_toolbox::{ErrorKind, Manifest};
use serde_json::json;

const OK: &str = "use mod.research\nlet found = research.search({topic: \"x\"}).await()\n{status: \"ready\", data: {n: 1}}\n";

fn manifest_error(value: serde_json::Value) -> ErrorKind {
    Manifest::parse(value.to_string().as_bytes())
        .expect_err("refused")
        .kind
}

#[test]
fn a_valid_probe_is_admitted() {
    probe(&["search"], OK).unwrap();
}

#[test]
fn manifests_are_validated() {
    let mut unknown_field = manifest(&["search"]);
    unknown_field["network"] = json!(true);
    assert_eq!(manifest_error(unknown_field), ErrorKind::Manifest);

    let mut bad_id = manifest(&["search"]);
    bad_id["id"] = json!("../escape");
    assert_eq!(manifest_error(bad_id), ErrorKind::Manifest);

    let mut unknown_module = manifest(&["search"]);
    unknown_module["modules"] = json!([{"module": "fs", "methods": ["read"]}]);
    assert_eq!(manifest_error(unknown_module), ErrorKind::Manifest);

    let mut unknown_method = manifest(&["search"]);
    unknown_method["modules"] = json!([{"module": "research", "methods": ["crawl"]}]);
    assert_eq!(manifest_error(unknown_method), ErrorKind::Manifest);

    let mut duplicate = manifest(&["search", "search"]);
    duplicate["id"] = json!("dup");
    assert_eq!(manifest_error(duplicate), ErrorKind::Manifest);

    let mut over_ceiling = manifest(&["search"]);
    over_ceiling["budget"]["max_model_calls"] = json!(99);
    assert_eq!(manifest_error(over_ceiling), ErrorKind::Manifest);

    let mut zero_concurrency = manifest(&["search"]);
    zero_concurrency["budget"]["max_concurrency"] = json!(0);
    assert_eq!(manifest_error(zero_concurrency), ErrorKind::Manifest);

    let mut no_provenance = manifest(&["search"]);
    no_provenance["provenance"] = json!(false);
    assert_eq!(manifest_error(no_provenance), ErrorKind::Manifest);

    let mut optional_without_default = manifest(&["search"]);
    optional_without_default["params"] =
        json!({"type": "object", "properties": {"limit": {"type": "integer"}}});
    assert_eq!(
        manifest_error(optional_without_default),
        ErrorKind::Manifest
    );

    let mut bad_schema = manifest(&["search"]);
    bad_schema["output"] = json!({"type": "object", "properties": {"x": {"pattern": "a+"}}});
    assert_eq!(manifest_error(bad_schema), ErrorKind::Manifest);

    let too_big = format!("{{\"id\": \"{}\"}}", "a".repeat(40 * 1024));
    assert_eq!(
        Manifest::parse(too_big.as_bytes()).unwrap_err().kind,
        ErrorKind::Manifest
    );
}

#[test]
fn only_declared_modules_and_methods_may_be_referenced() {
    let refusals = [
        // The generic tool surface is never available to a template.
        "use mod.tool\nlet r = tool.start_json(\"research.search\", {topic: \"x\"}).await()\n{status: \"ready\", data: {}}\n",
        // An undeclared host module.
        "use mod.fs\n{status: \"ready\", data: {}}\n",
        // A declared module, an undeclared method.
        "use mod.research\nlet a = research.article({id: \"s1\"}).await()\n{status: \"ready\", data: {}}\n",
        // A method the module does not have at all.
        "use mod.research\nlet a = research.crawl({site: \"x\"}).await()\n{status: \"ready\", data: {}}\n",
        // An aliased binding is resolved, not trusted.
        "use mod.research\nlet r = research\nlet a = r.digest({task: \"digest\", language: \"en\", article_ids: [\"s1\"]}).await()\n{status: \"ready\", data: {}}\n",
        // A direct module path outside `use`.
        "use mod.research\nlet a = mod.research.search({topic: \"x\"}).await()\n{status: \"ready\", data: {}}\n",
        // Not canonical OctoScript.
        "use mod.research\nlet a = true ? 1 : 2\n{status: \"ready\", data: {}}\n",
        // Not a std module.
        "use mod.std.net\n{status: \"ready\", data: {}}\n",
    ];
    for source in refusals {
        let err = probe(&["search"], source).expect_err(source);
        assert_eq!(err.kind, ErrorKind::Check, "{source}: {err}");
    }
    // Standard modules are fine.
    probe(
        &["search"],
        "use mod.research\nuse mod.std.array\nuse mod.std.text\nlet a = array.len([1])\n{status: \"ready\", data: {a: a}}\n",
    )
    .unwrap();
    // A string that mentions a module is not a reference.
    probe(
        &["search"],
        "use mod.research\nlet note = \"mod.tool is not here\"\n{status: \"ready\", data: {note: note}}\n",
    )
    .unwrap();
}

#[test]
fn source_size_is_capped() {
    let big = format!(
        "use mod.research\n{}{{status: \"ready\", data: {{}}}}\n",
        "// padding\n".repeat(4000)
    );
    assert_eq!(probe(&["search"], &big).unwrap_err().kind, ErrorKind::Check);
}

#[test]
fn digests_pin_both_files() {
    let a = probe(&["search"], OK).unwrap();
    let b = probe(&["search"], &OK.replace("\"x\"", "\"y\"")).unwrap();
    assert_ne!(a.digest, b.digest);
    let mut m = manifest(&["search"]);
    m["title"] = json!("Other");
    let c = Template::from_parts(&m.to_string(), OK, TemplateOrigin::Library).unwrap();
    assert_ne!(a.digest, c.digest);
    // Line endings do not change a digest.
    let crlf = Template::from_parts(
        &manifest(&["search"]).to_string(),
        &OK.replace('\n', "\r\n"),
        TemplateOrigin::Library,
    )
    .unwrap();
    assert_eq!(a.digest, crlf.digest);
}

#[test]
fn the_lock_refuses_a_changed_template() {
    let template = common::template("news-digest");
    let mut lock = octosense_toolbox::Library::lock_for(std::slice::from_ref(&template));
    octosense_toolbox::Library::pinned(vec![template.clone()], &lock).unwrap();
    lock.templates.get_mut("news-digest").unwrap().digest = "0".repeat(64);
    let err = octosense_toolbox::Library::pinned(vec![template], &lock).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Pin);
}
