//! `octosense-skill-gen`: the generated references of a craft engine's skill
//! (ADR 0013).
//!
//! The system agent learns each craft engine from a skill OctoSense installs
//! into the kernel's skills dir (`octosense-kernel`'s `skills` module). Its
//! `SKILL.md` is written by hand; its references are made from the engine
//! itself, at the revision this workspace pins, so they cannot drift from it:
//!
//! - `commands.md`: the engine's command catalog, one line per id
//!   (`` `id` [tag] label: params ``), each tagged with its safety class. The
//!   model greps it rather than loading hundreds of lines.
//! - `safety.json`: every catalog id's safety [`Class`], expanded from the
//!   skill's hand-written `safety-rules.json` (the analysis: each rule says
//!   why, from the engine's implementation). A reviewed command door's
//!   deny-list is built from it.
//! - Any other reference generated from the engine ([`check_generated`]):
//!   the sheet skill's `functions.md`, the light skill's `controls.md`.
//!
//! An engine service's `tests/skill.rs` builds a [`Catalog`] from its engine
//! and calls [`check_commands`]. That fails when an id has no class, when a
//! rule classifies nothing, or when a checked-in file is not what the pinned
//! engine generates. To regenerate after a pin moves or a rule changes:
//!
//! ```text
//! OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p <service package> --test skill
//! ```
//!
//! `OCTOSENSE_SKILL_DUMP=<file>` also writes the live catalog as JSON, for
//! whoever reviews a classification.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The environment variable that makes the checks write the generated files
/// instead of comparing them.
pub const REGEN_ENV: &str = "OCTOSENSE_SKILL_REGEN";
/// The environment variable naming a file the live catalog is dumped to.
pub const DUMP_ENV: &str = "OCTOSENSE_SKILL_DUMP";
/// The hand-written classification, in the skill dir.
pub const RULES_FILE: &str = "safety-rules.json";
/// The generated catalog reference, in the skill dir.
pub const COMMANDS_FILE: &str = "commands.md";
/// The generated classification, in the skill dir.
pub const SAFETY_FILE: &str = "safety.json";

/// What a command can reach beyond the open document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// Pure document or pixel operations inside the session.
    Safe,
    /// Reads or writes a path.
    File,
    /// Installs or runs code: plug-ins, scripts, or other commands (macros,
    /// actions, batches) a deny-list on the outer id would not see.
    Code,
    /// Uses the network.
    Network,
    /// Uses hardware: audio, MIDI, cameras, scanners, printers, tablets.
    Device,
    /// Host, app or UI state outside the document: windows, panels, views,
    /// preferences, shortcuts, the clipboard, the app's own settings files.
    Host,
}

impl Class {
    pub const ALL: [Class; 6] = [Class::Safe, Class::File, Class::Code, Class::Network, Class::Device, Class::Host];

    pub fn name(self) -> &'static str {
        match self {
            Class::Safe => "safe",
            Class::File => "file",
            Class::Code => "code",
            Class::Network => "network",
            Class::Device => "device",
            Class::Host => "host",
        }
    }

    pub fn parse(name: &str) -> Option<Class> {
        Class::ALL.into_iter().find(|c| c.name() == name)
    }

    /// What the class means, as `safety.json` states it.
    pub fn meaning(self) -> &'static str {
        match self {
            Class::Safe => "pure document or pixel operations inside the session",
            Class::File => "reads or writes a path (a door may allow it only inside the caller's area)",
            Class::Code => "installs or runs code: plug-ins, scripts, expressions with I/O, or other commands (macros, actions, batches)",
            Class::Network => "uses the network",
            Class::Device => "uses hardware: audio, MIDI, cameras, scanners, printers, tablets",
            Class::Host => "host, app or UI state outside the document: windows, panels, views, preferences, shortcuts, the clipboard",
        }
    }

    /// The tag `commands.md` puts after the id; none for [`Class::Safe`].
    fn tag(self) -> Option<&'static str> {
        (self != Class::Safe).then(|| self.name())
    }
}

/// One command of an engine's catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub label: String,
    /// The engine's own parameter description (free text, often JSON-ish).
    pub params: String,
}

impl Entry {
    pub fn new(id: impl Into<String>, label: impl Into<String>, params: impl Into<String>) -> Self {
        Entry { id: id.into(), label: label.into(), params: params.into() }
    }
}

/// An engine's command catalog, as its service's tests read it.
#[derive(Clone, Debug)]
pub struct Catalog {
    /// The service family (`photo`): the skill is `<family>-engine`.
    pub family: &'static str,
    /// The engine (`photocraft`).
    pub engine: &'static str,
    /// The engine crate whose locked source names the pinned revision.
    pub engine_crate: &'static str,
    /// The service package whose tests regenerate the references.
    pub package: &'static str,
    pub entries: Vec<Entry>,
}

impl Catalog {
    /// The command that rewrites this skill's generated files.
    pub fn regen_command(&self) -> String {
        regen_command(self.package)
    }
}

/// `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p <package> --test skill`.
pub fn regen_command(package: &str) -> String {
    format!("{REGEN_ENV}=1 cargo test --locked -p {package} --test skill")
}

/// One rule of `safety-rules.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub matcher: Matcher,
    pub class: Class,
    pub why: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Matcher {
    /// Every id that starts with this (non-empty) prefix, unless a longer
    /// prefix or an exact id decides it.
    Prefix(String),
    /// Exactly these ids; they win over every prefix.
    Ids(Vec<String>),
}

/// The parsed `safety-rules.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rules {
    pub engine: String,
    pub rules: Vec<Rule>,
}

impl Rules {
    /// Parse the rules file: `{"engine", "method"?, "rules": [{"prefix" |
    /// "ids", "class", "why", "evidence"?}]}`. Unknown keys are refused, so
    /// a misspelt matcher cannot silently classify nothing.
    pub fn parse(text: &str) -> Result<Rules, String> {
        let root: Value = serde_json::from_str(text).map_err(|e| format!("{RULES_FILE} is not JSON: {e}"))?;
        let obj = root.as_object().ok_or(format!("{RULES_FILE} is not an object"))?;
        for key in obj.keys() {
            if !["engine", "method", "rules"].contains(&key.as_str()) {
                return Err(format!("{RULES_FILE}: unknown key `{key}`"));
            }
        }
        let engine = obj.get("engine").and_then(Value::as_str).ok_or(format!("{RULES_FILE}: `engine` is required"))?;
        let list = obj.get("rules").and_then(Value::as_array).ok_or(format!("{RULES_FILE}: `rules` is a list"))?;
        let mut rules = Vec::new();
        for (n, rule) in list.iter().enumerate() {
            let at = |what: &str| format!("{RULES_FILE}: rule {n}: {what}");
            let r = rule.as_object().ok_or_else(|| at("not an object"))?;
            for key in r.keys() {
                if !["prefix", "ids", "class", "why", "evidence"].contains(&key.as_str()) {
                    return Err(at(&format!("unknown key `{key}`")));
                }
            }
            let matcher = match (r.get("prefix"), r.get("ids")) {
                (Some(p), None) => {
                    let p = p.as_str().filter(|p| !p.is_empty()).ok_or_else(|| at("`prefix` is a non-empty string"))?;
                    Matcher::Prefix(p.to_owned())
                }
                (None, Some(ids)) => {
                    let ids = ids.as_array().filter(|a| !a.is_empty()).ok_or_else(|| at("`ids` is a non-empty list"))?;
                    let ids = ids
                        .iter()
                        .map(|id| id.as_str().filter(|s| !s.is_empty()).map(str::to_owned))
                        .collect::<Option<Vec<_>>>()
                        .ok_or_else(|| at("`ids` holds non-empty strings"))?;
                    Matcher::Ids(ids)
                }
                _ => return Err(at("give exactly one of `prefix` or `ids`")),
            };
            let class = r.get("class").and_then(Value::as_str).ok_or_else(|| at("`class` is required"))?;
            let class = Class::parse(class).ok_or_else(|| at(&format!("unknown class `{class}`")))?;
            let why = r.get("why").and_then(Value::as_str).filter(|w| !w.trim().is_empty()).ok_or_else(|| at("`why` is required"))?;
            if let Some(evidence) = r.get("evidence") {
                if !evidence.as_array().is_some_and(|a| a.iter().all(Value::is_string)) {
                    return Err(at("`evidence` is a list of strings"));
                }
            }
            rules.push(Rule { matcher, class, why: why.to_owned() });
        }
        Ok(Rules { engine: engine.to_owned(), rules })
    }

    /// Every id's class: an exact id wins, then the longest matching prefix.
    /// Errors name the ids no rule classifies, ids listed twice or missing
    /// from the catalog, and rules that decide no id.
    pub fn classify(&self, ids: &[&str]) -> Result<BTreeMap<String, Class>, String> {
        let catalog: BTreeSet<&str> = ids.iter().copied().collect();
        let mut exact: BTreeMap<&str, usize> = BTreeMap::new();
        let mut problems = Vec::new();
        for (n, rule) in self.rules.iter().enumerate() {
            if let Matcher::Ids(list) = &rule.matcher {
                for id in list {
                    if !catalog.contains(id.as_str()) {
                        problems.push(format!("rule {n} names `{id}`, which is not in the catalog"));
                    }
                    if exact.insert(id.as_str(), n).is_some() {
                        problems.push(format!("`{id}` is listed by more than one rule"));
                    }
                }
            }
        }
        let mut used = vec![false; self.rules.len()];
        let mut classes = BTreeMap::new();
        let mut unclassified = Vec::new();
        for id in &catalog {
            let decided = exact.get(id).copied().or_else(|| {
                self.rules
                    .iter()
                    .enumerate()
                    .filter_map(|(n, r)| match &r.matcher {
                        Matcher::Prefix(p) if id.starts_with(p.as_str()) => Some((p.len(), n)),
                        _ => None,
                    })
                    .max()
                    .map(|(_, n)| n)
            });
            match decided {
                Some(n) => {
                    used[n] = true;
                    classes.insert((*id).to_owned(), self.rules[n].class);
                }
                None => unclassified.push(*id),
            }
        }
        if !unclassified.is_empty() {
            let shown: Vec<&str> = unclassified.iter().take(40).copied().collect();
            problems.push(format!(
                "{} catalog ids have no class (add rules to {RULES_FILE}): {}{}",
                unclassified.len(),
                shown.join(", "),
                if unclassified.len() > shown.len() { ", …" } else { "" }
            ));
        }
        for (n, rule) in self.rules.iter().enumerate() {
            if !used[n] {
                let what = match &rule.matcher {
                    Matcher::Prefix(p) => format!("prefix `{p}`"),
                    Matcher::Ids(ids) => format!("ids {ids:?}"),
                };
                problems.push(format!("rule {n} ({what}) decides no catalog id: remove or fix it"));
            }
        }
        if problems.is_empty() {
            Ok(classes)
        } else {
            Err(problems.join("\n"))
        }
    }
}

/// One line of text: whitespace runs (newlines included) become one space.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// An id's group: its first segment before a `.` or `_` (`filter` for
/// `filter.blur.gaussian`, `doc` for `doc_open`), else the whole id.
fn group_of(id: &str) -> &str {
    id.find(['.', '_']).map_or(id, |at| &id[..at])
}

/// `commands.md` for `catalog`, classified by `classes`.
pub fn render_commands(catalog: &Catalog, revision: &str, classes: &BTreeMap<String, Class>) -> String {
    let mut out = String::new();
    let mut entries: Vec<&Entry> = catalog.entries.iter().collect();
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    let _ = writeln!(out, "# {} engine commands", catalog.family);
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "The {} engine's command catalog at revision {}: {} commands, one per line as `id` label: params.",
        catalog.engine,
        short(revision),
        entries.len()
    );
    let _ = writeln!(out, "Generated from the engine by `{}`; do not edit.", catalog.regen_command());
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "A tag after the id marks a command that reaches past the open document (safety.json has every id's class): \
         [file] reads or writes a path, [code] installs or runs code or other commands, [network] uses the network, \
         [device] uses hardware, [host] changes windows, views, preferences, the clipboard or other app state. \
         An untagged command works on the open document only."
    );
    // Headings per group, unless most groups would hold a single command
    // (cadcraft's ids are bare command names): then one flat list.
    let groups: BTreeSet<&str> = entries.iter().map(|e| group_of(&e.id)).collect();
    let grouped = groups.len() * 3 <= entries.len();
    if !grouped {
        let _ = writeln!(out);
    }
    let mut group: Option<&str> = None;
    for e in entries {
        let g = group_of(&e.id);
        if grouped && group != Some(g) {
            let _ = writeln!(out);
            let _ = writeln!(out, "## {g}");
            let _ = writeln!(out);
            group = Some(g);
        }
        let tag = classes.get(&e.id).and_then(|c| c.tag()).map(|t| format!(" [{t}]")).unwrap_or_default();
        let label = one_line(&e.label);
        let params = one_line(&e.params);
        let _ = if params.is_empty() {
            writeln!(out, "- `{}`{tag} {label}", e.id)
        } else {
            writeln!(out, "- `{}`{tag} {label}: {params}", e.id)
        };
    }
    out
}

/// `safety.json` for `catalog`, classified by `classes`. Written by hand so
/// its bytes never depend on serde_json's map order.
pub fn render_safety(catalog: &Catalog, revision: &str, classes: &BTreeMap<String, Class>) -> String {
    let q = |s: &str| serde_json::to_string(s).expect("a string serializes");
    let mut out = String::from("{\n");
    let _ = writeln!(out, "  \"engine\": {},", q(catalog.engine));
    let _ = writeln!(out, "  \"revision\": {},", q(revision));
    let _ = writeln!(out, "  \"regenerate\": {},", q(&catalog.regen_command()));
    let _ = writeln!(out, "  \"rules\": {},", q(RULES_FILE));
    out.push_str("  \"classes\": {\n");
    let counts: Vec<String> = Class::ALL
        .iter()
        .map(|c| {
            let n = classes.values().filter(|v| *v == c).count();
            format!("    {}: {{\"count\": {n}, \"meaning\": {}}}", q(c.name()), q(c.meaning()))
        })
        .collect();
    out.push_str(&counts.join(",\n"));
    out.push_str("\n  },\n  \"commands\": {\n");
    let lines: Vec<String> = classes.iter().map(|(id, c)| format!("    {}: {}", q(id), q(c.name()))).collect();
    out.push_str(&lines.join(",\n"));
    out.push_str("\n  }\n}\n");
    out
}

fn short(revision: &str) -> &str {
    revision.get(..12).unwrap_or(revision)
}

/// The workspace's `Cargo.lock`.
pub fn workspace_lock() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock")
}

/// The git revision `Cargo.lock` locks `package` to (the `#<sha>` of its
/// source).
pub fn locked_revision(lock: &str, package: &str) -> Result<String, String> {
    let header = format!("name = \"{package}\"\n");
    let at = lock.find(&header).ok_or(format!("{package} is not in Cargo.lock"))?;
    let block = &lock[at..];
    let block = &block[..block.find("\n\n").unwrap_or(block.len())];
    let source = block
        .lines()
        .find_map(|l| l.strip_prefix("source = \""))
        .ok_or(format!("{package} has no source in Cargo.lock"))?;
    let sha = source.trim_end_matches('"').rsplit_once('#').map(|(_, sha)| sha).ok_or(format!("{package} is not a git source"))?;
    Ok(sha.to_owned())
}

/// Whether this run regenerates instead of comparing.
pub fn regenerating() -> bool {
    std::env::var(REGEN_ENV).is_ok_and(|v| v == "1")
}

/// Compare `skill_dir/file` with `content`, or write it when regenerating.
pub fn compare_or_write(skill_dir: &Path, file: &str, content: &str, regen: &str, write: bool) -> Result<(), String> {
    let path = skill_dir.join(file);
    if write {
        std::fs::create_dir_all(skill_dir).map_err(|e| format!("{}: {e}", skill_dir.display()))?;
        return std::fs::write(&path, content).map_err(|e| format!("{}: {e}", path.display()));
    }
    match std::fs::read_to_string(&path) {
        Ok(current) if current == content => Ok(()),
        Ok(current) => {
            let line = current
                .lines()
                .zip(content.lines())
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| current.lines().count().min(content.lines().count()));
            Err(format!(
                "{} is not what the pinned engine generates (first difference at line {}); regenerate it: `{regen}`",
                path.display(),
                line + 1
            ))
        }
        Err(e) => Err(format!("{}: {e}; generate it: `{regen}`", path.display())),
    }
}

/// The classification and both generated files, from the rules in
/// `skill_dir`, without touching the disk otherwise.
pub fn generate(catalog: &Catalog, revision: &str, rules_text: &str) -> Result<(BTreeMap<String, Class>, String, String), String> {
    let mut seen = BTreeSet::new();
    let dups: Vec<&str> = catalog.entries.iter().filter(|e| !seen.insert(e.id.as_str())).map(|e| e.id.as_str()).collect();
    if !dups.is_empty() {
        return Err(format!("the catalog lists these ids more than once: {dups:?}"));
    }
    let rules = Rules::parse(rules_text)?;
    if rules.engine != catalog.engine {
        return Err(format!("{RULES_FILE} is {}'s, not {}'s", rules.engine, catalog.engine));
    }
    let ids: Vec<&str> = catalog.entries.iter().map(|e| e.id.as_str()).collect();
    let classes = rules.classify(&ids)?;
    let commands = render_commands(catalog, revision, &classes);
    let safety = render_safety(catalog, revision, &classes);
    Ok((classes, commands, safety))
}

/// The drift and coverage check an engine's `tests/skill.rs` runs: every
/// catalog id classified, every rule used, and `commands.md` and
/// `safety.json` exactly what the pinned engine generates (written instead
/// when [`REGEN_ENV`] is `1`). Panics with what to do.
pub fn check_commands(catalog: &Catalog, skill_dir: &Path) {
    if let Err(e) = try_check_commands(catalog, skill_dir, regenerating()) {
        panic!("{}-engine skill: {e}", catalog.family);
    }
}

/// [`check_commands`] without the panic; `write` regenerates.
pub fn try_check_commands(catalog: &Catalog, skill_dir: &Path, write: bool) -> Result<(), String> {
    dump(catalog);
    let lock = std::fs::read_to_string(workspace_lock()).map_err(|e| format!("Cargo.lock: {e}"))?;
    let revision = locked_revision(&lock, catalog.engine_crate)?;
    let rules_path = skill_dir.join(RULES_FILE);
    let rules_text = std::fs::read_to_string(&rules_path).map_err(|e| format!("{}: {e}", rules_path.display()))?;
    let (_, commands, safety) = generate(catalog, &revision, &rules_text)?;
    let regen = catalog.regen_command();
    compare_or_write(skill_dir, COMMANDS_FILE, &commands, &regen, write)?;
    compare_or_write(skill_dir, SAFETY_FILE, &safety, &regen, write)
}

/// The drift check for any other generated reference (`functions.md`,
/// `controls.md`): `skill_dir/file` must be `content`, or is written when
/// [`REGEN_ENV`] is `1`. Panics with what to do.
pub fn check_generated(package: &str, skill_dir: &Path, file: &str, content: &str) {
    if let Err(e) = compare_or_write(skill_dir, file, content, &regen_command(package), regenerating()) {
        panic!("{e}");
    }
}

/// With [`DUMP_ENV`] set, write the live catalog there as JSON.
fn dump(catalog: &Catalog) {
    let Some(path) = std::env::var_os(DUMP_ENV) else { return };
    let entries: Vec<Value> = catalog
        .entries
        .iter()
        .map(|e| serde_json::json!({"id": e.id, "label": e.label, "params": e.params}))
        .collect();
    let body = serde_json::json!({"family": catalog.family, "engine": catalog.engine, "commands": entries});
    let _ = std::fs::write(path, serde_json::to_string_pretty(&body).unwrap_or_default());
}

/// A JSON Schema's object properties as a short signature: `{a: string,
/// b?: integer}` (sorted; `?` marks the optional ones). For catalogs whose
/// commands carry a schema instead of a description (pdfcraft's tools).
pub fn params_from_schema(schema: &Value) -> String {
    let Some(props) = schema.get("properties").and_then(Value::as_object) else { return String::new() };
    let required: BTreeSet<&str> =
        schema.get("required").and_then(Value::as_array).map(|r| r.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let mut names: Vec<&String> = props.keys().collect();
    names.sort();
    let parts: Vec<String> = names
        .into_iter()
        .map(|name| {
            let p = &props[name];
            let ty = match p.get("type") {
                Some(Value::String(t)) => t.clone(),
                Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("|"),
                _ if p.get("enum").is_some() => "enum".to_owned(),
                _ => "any".to_owned(),
            };
            let opt = if required.contains(name.as_str()) { "" } else { "?" };
            format!("{name}{opt}: {ty}")
        })
        .collect();
    format!("{{{}}}", parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(ids: &[&str]) -> Catalog {
        Catalog {
            family: "demo",
            engine: "democraft",
            engine_crate: "democraft-engine",
            package: "octosense-demo-service",
            entries: ids.iter().map(|id| Entry::new(*id, format!("Label of {id}"), "{x: number}")).collect(),
        }
    }

    const RULES: &str = r#"{"engine": "democraft", "rules": [
        {"prefix": "file.", "class": "file", "why": "reads paths"},
        {"ids": ["file.new"], "class": "safe", "why": "a blank document"},
        {"prefix": "filter.", "class": "safe", "why": "pixels"},
        {"prefix": "filter.lut.", "class": "file", "why": "loads a LUT file"},
        {"prefix": "plugin.", "class": "code", "why": "loads wasm"}
    ]}"#;

    #[test]
    fn exact_ids_win_then_the_longest_prefix() {
        let rules = Rules::parse(RULES).unwrap();
        let ids = ["file.open", "file.new", "filter.blur", "filter.lut.apply", "plugin.install"];
        let classes = rules.classify(&ids).unwrap();
        assert_eq!(classes["file.open"], Class::File);
        assert_eq!(classes["file.new"], Class::Safe, "the exact id beats `file.`");
        assert_eq!(classes["filter.blur"], Class::Safe);
        assert_eq!(classes["filter.lut.apply"], Class::File, "the longer prefix beats `filter.`");
        assert_eq!(classes["plugin.install"], Class::Code);
    }

    #[test]
    fn an_unclassified_id_a_stale_rule_and_a_duplicate_id_are_errors() {
        let rules = Rules::parse(RULES).unwrap();
        let e = rules.classify(&["file.open", "file.new", "filter.blur", "filter.lut.x", "plugin.a", "view.zoom"]).unwrap_err();
        assert!(e.contains("1 catalog ids have no class") && e.contains("view.zoom"), "{e}");
        let e = rules.classify(&["file.open", "file.new", "filter.blur", "filter.lut.x"]).unwrap_err();
        assert!(e.contains("prefix `plugin.`") && e.contains("decides no catalog id"), "{e}");
        let e = rules.classify(&["file.open", "filter.blur", "filter.lut.x", "plugin.a"]).unwrap_err();
        assert!(e.contains("`file.new`, which is not in the catalog"), "{e}");
        let twice = r#"{"engine": "d", "rules": [{"ids": ["a.b"], "class": "safe", "why": "x"}, {"ids": ["a.b"], "class": "file", "why": "y"}]}"#;
        assert!(Rules::parse(twice).unwrap().classify(&["a.b"]).unwrap_err().contains("more than one rule"));
    }

    #[test]
    fn the_rules_file_refuses_what_it_does_not_understand() {
        for (text, needle) in [
            (r#"{"engine": "d", "rules": [{"prefx": "a.", "class": "safe", "why": "x"}]}"#, "unknown key `prefx`"),
            (r#"{"engine": "d", "rules": [{"prefix": "", "class": "safe", "why": "x"}]}"#, "non-empty"),
            (r#"{"engine": "d", "rules": [{"prefix": "a.", "class": "harmless", "why": "x"}]}"#, "unknown class"),
            (r#"{"engine": "d", "rules": [{"prefix": "a.", "class": "safe"}]}"#, "`why` is required"),
            (r#"{"engine": "d", "rules": [{"prefix": "a.", "ids": ["a.b"], "class": "safe", "why": "x"}]}"#, "exactly one"),
            (r#"{"rules": []}"#, "`engine` is required"),
        ] {
            let e = Rules::parse(text).unwrap_err();
            assert!(e.contains(needle), "{text}: {e}");
        }
    }

    #[test]
    fn the_references_are_deterministic_one_line_each_and_tagged() {
        let mut c = catalog(&["plugin.install", "filter.blur", "file.open", "file.new", "filter.lut.apply"]);
        c.entries[1].params = "{radius:\n   number}".into();
        let (classes, md, json) = generate(&c, "0123456789abcdef0123456789abcdef01234567", RULES).unwrap();
        assert_eq!(classes.len(), 5);
        assert!(md.contains("at revision 0123456789ab: 5 commands"), "{md}");
        assert!(md.contains("- `filter.blur` Label of filter.blur: {radius: number}\n"), "{md}");
        assert!(md.contains("- `file.open` [file] Label of file.open: {x: number}\n"), "{md}");
        assert!(md.contains("- `plugin.install` [code] "), "{md}");
        // Three groups for five commands: one flat list, sorted by id.
        let at = |id: &str| md.find(&format!("- `{id}`")).unwrap();
        assert!(at("file.new") < at("file.open") && at("file.open") < at("filter.blur") && at("filter.lut.apply") < at("plugin.install"));
        assert!(!md.contains("\n## "), "{md}");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["commands"]["file.new"], "safe");
        assert_eq!(v["classes"]["file"]["count"], 2);
        assert_eq!(v["regenerate"], "OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-demo-service --test skill");
        // Another order of the same catalog generates the same bytes.
        let mut shuffled = c.clone();
        shuffled.entries.reverse();
        let (_, md2, json2) = generate(&shuffled, "0123456789abcdef0123456789abcdef01234567", RULES).unwrap();
        assert_eq!((md, json), (md2, json2));
    }

    #[test]
    fn bare_command_names_make_one_flat_list_and_underscores_group() {
        let rules = r#"{"engine": "democraft", "rules": [{"prefix": "", "class": "safe", "why": "x"}]}"#;
        assert!(generate(&catalog(&["a"]), "r", rules).is_err(), "no catch-all");
        let flat = catalog(&["line", "circle", "arc", "offset"]);
        let rules = r#"{"engine": "democraft", "rules": [{"ids": ["line", "circle", "arc", "offset"], "class": "safe", "why": "x"}]}"#;
        let (_, md, _) = generate(&flat, "r", rules).unwrap();
        assert!(!md.contains("\n## "), "{md}");
        assert!(md.contains("- `arc` Label of arc: {x: number}\n- `circle`"), "{md}");
        let tools = catalog(&["doc_open", "doc_save", "doc_info", "page_render", "page_rotate", "page_delete"]);
        let rules = r#"{"engine": "democraft", "rules": [{"prefix": "doc_", "class": "file", "why": "x"}, {"prefix": "page_", "class": "safe", "why": "y"}]}"#;
        let (_, md, _) = generate(&tools, "r", rules).unwrap();
        assert!(md.contains("\n## doc\n\n- `doc_info` [file] ") && md.contains("\n## page\n"), "{md}");
    }

    #[test]
    fn compare_or_write_reports_drift_and_regenerates() {
        let dir = tempfile::tempdir().unwrap();
        let e = compare_or_write(dir.path(), "commands.md", "a\nb\n", "REGEN", false).unwrap_err();
        assert!(e.contains("generate it: `REGEN`"), "{e}");
        compare_or_write(dir.path(), "commands.md", "a\nb\n", "REGEN", true).unwrap();
        compare_or_write(dir.path(), "commands.md", "a\nb\n", "REGEN", false).unwrap();
        let e = compare_or_write(dir.path(), "commands.md", "a\nc\n", "REGEN", false).unwrap_err();
        assert!(e.contains("line 2") && e.contains("regenerate it: `REGEN`"), "{e}");
    }

    #[test]
    fn the_revision_is_the_locked_git_sha() {
        let lock = "[[package]]\nname = \"democraft-engine\"\nversion = \"0.1.0\"\nsource = \"git+https://github.com/x/democraft.git?rev=abc#0123456789abcdef\"\n\n[[package]]\nname = \"other\"\n";
        assert_eq!(locked_revision(lock, "democraft-engine").unwrap(), "0123456789abcdef");
        assert!(locked_revision(lock, "missing").is_err());
        assert!(locked_revision("[[package]]\nname = \"p\"\nversion = \"1\"\n", "p").unwrap_err().contains("no source"));
    }

    #[test]
    fn a_schema_becomes_a_short_signature() {
        let schema = serde_json::json!({"type": "object", "properties": {
            "page": {"type": "integer"}, "doc": {"type": "integer"}, "out": {"type": "string"},
            "mode": {"enum": ["a", "b"]}, "at": {"type": ["number", "string"]}
        }, "required": ["doc", "page"]});
        assert_eq!(params_from_schema(&schema), "{at?: number|string, doc: integer, mode?: enum, out?: string, page: integer}");
        assert_eq!(params_from_schema(&serde_json::json!({"type": "object"})), "");
    }
}
