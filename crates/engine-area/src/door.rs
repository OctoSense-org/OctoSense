//! A command door's gate (ADR 0013, #418): which of an engine's command ids
//! one `<family>.run` call may run, and with which parameters.
//!
//! The gate is an **allowlist**, never a deny-list. It is built from the
//! engine's reviewed classification: `skill/safety.json`, which
//! `octosense-skill-gen` generates from the hand-written
//! `skill/safety-rules.json` and checks against the pinned engine's live
//! catalog. A command runs only when its id is classed
//!
//! - `safe` (work inside the open document), or
//! - `file` **and** reviewed for the door ([`FileRead`]): its only file
//!   access is reading the files its listed parameters name, and each of
//!   those must be a relative path to an existing file inside the call's
//!   area, handed to the engine as that file's resolved absolute path. The
//!   files one call reads total at most [`MAX_READ_BYTES`].
//!
//! `code`, `network`, `device` and `host` ids are refused, and so is an id
//! the classification does not know: a command added upstream stays refused
//! until someone reviews it (the skill's drift test fails until they do).
//! Composite commands (batches, macros, actions, scripts, replays) run
//! other commands that a check on the outer id cannot see: every engine's
//! classification classes them `code`, so they are refused outright.
//!
//! Beyond the classes, an engine's reviewer settles two more kinds of
//! parameter ([`Reviewed`]):
//!
//! - **setters** classed `safe` that set an app-wide variable by key
//!   (cadcraft's `setvar`): only reviewed keys pass, and a setter with none
//!   is refused;
//! - **inner ids**, a parameter that names another command, effect or
//!   plug-in ([`Inner`]): a named command must start with its reviewed
//!   prefix and pass this gate itself, with its own parameters; a named
//!   effect must be one the engine builds in, never a plug-in.
//!
//! A service admits a whole call with [`Door::admit_all`] before its engine
//! runs any command, so one refused id refuses the call with nothing done.
//! What the commands then do to the open document is the service's to fence
//! after each one (a document's links, an effect's file parameters), since
//! no id check sees the data a command plants.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::Area;

/// How deep inner commands may nest (a command naming a command naming…).
const MAX_DEPTH: usize = 4;

/// The most commands one `run` call admits.
pub const MAX_COMMANDS: usize = 64;

/// The most bytes the reviewed reads of one call may name, together: an
/// engine reads each such file whole into memory.
pub const MAX_READ_BYTES: u64 = 64 << 20;

/// A command's reviewed class, as `safety.json` records it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Safe,
    File,
    Code,
    Network,
    Device,
    Host,
}

impl Class {
    fn parse(name: &str) -> Option<Class> {
        Some(match name {
            "safe" => Class::Safe,
            "file" => Class::File,
            "code" => Class::Code,
            "network" => Class::Network,
            "device" => Class::Device,
            "host" => Class::Host,
            _ => return None,
        })
    }

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

    /// Why the door never runs a command of this class.
    fn refusal(self) -> &'static str {
        match self {
            Class::Safe | Class::File => "",
            Class::Code => "it installs or runs code, or runs other commands",
            Class::Network => "it uses the network",
            Class::Device => "it uses a device or starts a program",
            Class::Host => "it changes the app's own state (windows, views, preferences, the clipboard)",
        }
    }
}

/// A `file` command reviewed for the door: it reads, and only reads, the
/// files these parameters name.
pub struct FileRead {
    pub id: &'static str,
    pub params: &'static [&'static str],
}

/// A `safe` command that sets an app-wide variable by key.
pub struct Setter {
    pub id: &'static str,
    /// The keys it may set through the door; empty, it is refused.
    pub keys: &'static [&'static str],
    /// The keys one call sets.
    pub keys_of: fn(&Value) -> Vec<String>,
}

/// What a parameter naming another id must name.
pub enum InnerRule {
    /// A command: its id starts with `prefix` and passes the gate, with the
    /// value of the `params` parameter as its own parameters.
    Command { prefix: &'static str, params: &'static str },
    /// An effect the engine builds in (never a plug-in): `builtin` says
    /// whether a name or id is one.
    Effect { builtin: fn(&str) -> bool },
}

/// A command parameter that names another command or effect.
pub struct Inner {
    pub id: &'static str,
    pub param: &'static str,
    pub rule: InnerRule,
}

/// What an engine's reviewer settled beyond the classes.
pub struct Reviewed {
    pub file_reads: &'static [FileRead],
    pub setters: &'static [Setter],
    pub inner: &'static [Inner],
}

impl Reviewed {
    pub const NONE: Reviewed = Reviewed { file_reads: &[], setters: &[], inner: &[] };
}

/// One engine's command door.
pub struct Door {
    family: &'static str,
    classes: BTreeMap<String, Class>,
    reviewed: &'static Reviewed,
}

impl std::fmt::Debug for Door {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Door").field("family", &self.family).field("commands", &self.classes.len()).finish()
    }
}

impl Door {
    /// The door of `family`, from its `safety.json` and its reviewer's
    /// settlements. Every reviewed id must be one the classification knows,
    /// with the class its settlement needs: a reviewed `file` read must be
    /// a `file` command, a setter or an inner id a `safe` one.
    pub fn new(family: &'static str, safety_json: &str, reviewed: &'static Reviewed) -> Result<Door, String> {
        let root: Value = serde_json::from_str(safety_json).map_err(|e| format!("{family}: safety.json is not JSON: {e}"))?;
        let list = root["commands"].as_object().ok_or(format!("{family}: safety.json has no `commands`"))?;
        let mut classes = BTreeMap::new();
        for (id, class) in list {
            let class = class.as_str().and_then(Class::parse).ok_or(format!("{family}: `{id}` has no known class in safety.json"))?;
            classes.insert(id.clone(), class);
        }
        let door = Door { family, classes, reviewed };
        for read in reviewed.file_reads {
            door.expect(read.id, Class::File, "a reviewed file read")?;
        }
        for setter in reviewed.setters {
            door.expect(setter.id, Class::Safe, "a reviewed setter")?;
        }
        for inner in reviewed.inner {
            door.expect(inner.id, Class::Safe, "a command with a reviewed inner id")?;
        }
        Ok(door)
    }

    fn expect(&self, id: &str, class: Class, what: &str) -> Result<(), String> {
        match self.classes.get(id) {
            Some(c) if *c == class => Ok(()),
            Some(c) => Err(format!("{}: {what}, `{id}`, is classed {}, not {}", self.family, c.name(), class.name())),
            None => Err(format!("{}: {what}, `{id}`, is not in the catalog", self.family)),
        }
    }

    /// The class `safety.json` gives `id`.
    pub fn class(&self, id: &str) -> Option<Class> {
        self.classes.get(id).copied()
    }

    /// Whether the door can run `id` at all (its parameters still checked
    /// per call): a `safe` id that is no setter without keys, or a reviewed
    /// `file` read.
    pub fn runs(&self, id: &str) -> bool {
        match self.class(id) {
            Some(Class::Safe) => !self.reviewed.setters.iter().any(|s| s.id == id && s.keys.is_empty()),
            Some(Class::File) => self.reviewed.file_reads.iter().any(|r| r.id == id),
            _ => false,
        }
    }

    /// Every id the door can run, sorted.
    pub fn runnable(&self) -> Vec<&str> {
        self.classes.keys().map(String::as_str).filter(|id| self.runs(id)).collect()
    }

    /// Admit a whole `run` call working in `area` before any of it runs:
    /// `cmds` is a list of at most [`MAX_COMMANDS`] `{id, params?}`, each
    /// admitted by [`Door::admit`], whose reviewed reads together name at
    /// most [`MAX_READ_BYTES`]. Returns each command's id and the parameters
    /// to run it with.
    pub fn admit_all(&self, cmds: &Value, area: &Area) -> Result<Vec<(String, Value)>, String> {
        let family = self.family;
        let cmds = cmds.as_array().ok_or_else(|| format!("{family}.run: `cmds` is a list of {{id, params?}}"))?;
        if cmds.len() > MAX_COMMANDS {
            return Err(format!("{family}.run: at most {MAX_COMMANDS} commands per call"));
        }
        let mut read_total = 0;
        let mut admitted = Vec::with_capacity(cmds.len());
        for c in cmds {
            let id = c["id"].as_str().filter(|id| !id.is_empty()).ok_or_else(|| format!("{family}.run: each command has an `id`"))?;
            admitted.push((id.to_string(), self.admit_at(id, &c["params"], area, 0, &mut read_total)?));
        }
        Ok(admitted)
    }

    /// Admit one command of a `run` call working in `area`: its id must be
    /// one the door runs, its setter keys reviewed, any id it names admitted
    /// in turn, and its reviewed file parameters inside the area. Returns
    /// the parameters to run it with (file parameters made absolute).
    pub fn admit(&self, id: &str, params: &Value, area: &Area) -> Result<Value, String> {
        self.admit_at(id, params, area, 0, &mut 0)
    }

    /// [`Door::admit`] at nesting `depth`, adding what its reviewed reads
    /// name to the call's `read_total` bytes.
    fn admit_at(&self, id: &str, params: &Value, area: &Area, depth: usize, read_total: &mut u64) -> Result<Value, String> {
        let family = self.family;
        if depth > MAX_DEPTH {
            return Err(format!("{family}.run: `{id}`: commands nest too deep"));
        }
        let class = self.class(id).ok_or_else(|| format!("{family}.run: `{id}` is not a reviewed {family} command, so the door does not run it"))?;
        let mut params = if params.is_null() { Value::Object(Default::default()) } else { params.clone() };
        if !params.is_object() {
            return Err(format!("{family}.run: `{id}`: `params` is an object"));
        }
        match class {
            Class::Safe => {}
            Class::File => {
                let read = self.reviewed.file_reads.iter().find(|r| r.id == id).ok_or_else(|| {
                    format!("{family}.run: `{id}` reads or writes files beyond the door's `path` and `out`, and is not reviewed to run through it")
                })?;
                for key in read.params {
                    if let Some(rel) = params.get(*key).cloned() {
                        let rel = rel.as_str().ok_or_else(|| format!("{family}.run: `{id}`: `{key}` is a relative path"))?;
                        let (abs, len) = existing_file(area, rel).map_err(|e| format!("{family}.run: `{id}`: `{key}`: {e}"))?;
                        *read_total = read_total.saturating_add(len);
                        if *read_total > MAX_READ_BYTES {
                            return Err(format!("{family}.run: `{id}`: the files this call reads total more than {MAX_READ_BYTES} bytes"));
                        }
                        params[*key] = Value::String(abs.to_string_lossy().into_owned());
                    }
                }
            }
            other => {
                return Err(format!("{family}.run: `{id}` is classed {}: {}, so the door never runs it", other.name(), other.refusal()));
            }
        }
        if let Some(setter) = self.reviewed.setters.iter().find(|s| s.id == id) {
            let keys = (setter.keys_of)(&params);
            if setter.keys.is_empty() {
                return Err(format!("{family}.run: `{id}` sets app-wide variables, and no key of it is reviewed for the door"));
            }
            if keys.is_empty() {
                return Err(format!("{family}.run: `{id}`: name the key it sets"));
            }
            if let Some(key) = keys.iter().find(|k| !setter.keys.contains(&k.as_str())) {
                return Err(format!("{family}.run: `{id}`: `{key}` is not a key reviewed for the door"));
            }
        }
        for inner in self.reviewed.inner.iter().filter(|i| i.id == id) {
            let Some(named) = params.get(inner.param).cloned() else { continue };
            match &inner.rule {
                InnerRule::Command { prefix, params: key } => {
                    let inner_id = named.as_str().ok_or_else(|| format!("{family}.run: `{id}`: `{}` names a command", inner.param))?;
                    if !inner_id.starts_with(prefix) {
                        return Err(format!("{family}.run: `{id}` runs only `{prefix}*` commands, not `{inner_id}`"));
                    }
                    let inner_params = params.get(*key).cloned().unwrap_or(Value::Null);
                    let admitted = self.admit_at(inner_id, &inner_params, area, depth + 1, read_total)?;
                    params[*key] = admitted;
                }
                InnerRule::Effect { builtin } => {
                    let names: Vec<&str> = match &named {
                        Value::String(s) => vec![s.as_str()],
                        Value::Array(items) => items.iter().map(|v| v.as_str().unwrap_or("")).collect(),
                        _ => vec![""],
                    };
                    for name in names {
                        if !builtin(name) {
                            return Err(format!(
                                "{family}.run: `{id}`: `{name}` is not an effect the engine builds in (an effect plug-in never runs through the door)"
                            ));
                        }
                    }
                }
            }
        }
        Ok(params)
    }
}

/// `rel`, a relative path to an existing file inside the area (through
/// links), as its resolved absolute path, and its length. The engine gets
/// the resolved path, so a link swapped in after this check is not followed
/// out of the area.
fn existing_file(area: &Area, rel: &str) -> Result<(PathBuf, u64), String> {
    let p = Path::new(rel);
    if rel.is_empty() || p.is_absolute() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err("a path stays inside this call's folder, relative to it".into());
    }
    let joined = area.root.join(p);
    let root = area.root.canonicalize().map_err(|e| format!("this call's folder: {e}"))?;
    let real = joined.canonicalize().map_err(|_| format!("`{rel}` is not there"))?;
    if !real.starts_with(&root) {
        return Err("a path stays inside this call's folder, relative to it".into());
    }
    let meta = std::fs::metadata(&real).map_err(|_| format!("`{rel}` is not there"))?;
    if !meta.is_file() {
        return Err(format!("`{rel}` is not a file"));
    }
    Ok((real, meta.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SAFETY: &str = r#"{"engine": "democraft", "commands": {
        "shape.rect": "safe", "shape.ellipse": "safe", "edit.undo": "safe",
        "insert.picture": "file", "file.saveAs": "file",
        "command.batch": "code", "plugin.install": "code", "help.online": "network",
        "media.play": "device", "prefs.set": "host",
        "setvar": "safe", "perspective.draw": "safe", "effect.apply": "safe"
    }}"#;

    fn keys_of(params: &Value) -> Vec<String> {
        params["name"].as_str().map(|k| vec![k.to_string()]).unwrap_or_default()
    }

    fn builtin(name: &str) -> bool {
        ["stylize.dropShadow", "Gaussian Blur"].contains(&name)
    }

    static REVIEWED: Reviewed = Reviewed {
        file_reads: &[FileRead { id: "insert.picture", params: &["path"] }],
        setters: &[Setter { id: "setvar", keys: &["ORTHOMODE"], keys_of }],
        inner: &[
            Inner { id: "perspective.draw", param: "command", rule: InnerRule::Command { prefix: "shape.", params: "params" } },
            Inner { id: "effect.apply", param: "effect", rule: InnerRule::Effect { builtin } },
        ],
    };

    fn door() -> Door {
        Door::new("demo", SAFETY, &REVIEWED).unwrap()
    }

    fn area() -> (tempfile::TempDir, Area) {
        let dir = tempfile::tempdir().unwrap();
        let area = Area::new(dir.path(), None, false);
        (dir, area)
    }

    #[test]
    fn safe_ids_run_and_every_other_class_is_refused() {
        let (_dir, area) = area();
        let d = door();
        assert_eq!(d.admit("shape.rect", &json!({"w": 2}), &area).unwrap(), json!({"w": 2}));
        assert_eq!(d.admit("edit.undo", &Value::Null, &area).unwrap(), json!({}), "no params: an empty object");
        for (id, class) in [("command.batch", "code"), ("plugin.install", "code"), ("help.online", "network"), ("media.play", "device"), ("prefs.set", "host")] {
            let e = d.admit(id, &json!({}), &area).unwrap_err();
            assert!(e.contains(&format!("classed {class}")) && e.contains("never runs it"), "{id}: {e}");
        }
        let e = d.admit("brand.new", &json!({}), &area).unwrap_err();
        assert!(e.contains("not a reviewed demo command"), "{e}");
        assert!(d.admit("shape.rect", &json!([1]), &area).unwrap_err().contains("`params` is an object"));
    }

    #[test]
    fn an_unreviewed_file_command_is_refused_and_a_reviewed_read_stays_in_the_area() {
        let (dir, area) = area();
        let d = door();
        let e = d.admit("file.saveAs", &json!({"path": "x.docx"}), &area).unwrap_err();
        assert!(e.contains("not reviewed to run through it"), "{e}");
        std::fs::create_dir(dir.path().join("pics")).unwrap();
        std::fs::write(dir.path().join("pics/a.png"), b"png").unwrap();
        let ok = d.admit("insert.picture", &json!({"path": "pics/a.png", "width": 3}), &area).unwrap();
        assert_eq!(ok["path"], json!(dir.path().canonicalize().unwrap().join("pics/a.png").to_string_lossy()), "the resolved path");
        assert_eq!(ok["width"], json!(3));
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.png"), b"s").unwrap();
        for bad in ["../secret.png", "/etc/hosts", "pics/../../x", "", "pics", "missing.png"] {
            assert!(d.admit("insert.picture", &json!({"path": bad}), &area).is_err(), "{bad}");
        }
        assert!(d.admit("insert.picture", &json!({"path": 7}), &area).is_err(), "a path is a string");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path().join("secret.png"), dir.path().join("link.png")).unwrap();
            assert!(d.admit("insert.picture", &json!({"path": "link.png"}), &area).is_err(), "a link out is refused");
        }
    }

    #[test]
    fn a_setter_runs_only_with_reviewed_keys() {
        let (_dir, area) = area();
        let d = door();
        assert!(d.admit("setvar", &json!({"name": "ORTHOMODE", "value": 1}), &area).is_ok());
        let e = d.admit("setvar", &json!({"name": "SAVEFILEPATH", "value": "/tmp"}), &area).unwrap_err();
        assert!(e.contains("`SAVEFILEPATH` is not a key reviewed"), "{e}");
        assert!(d.admit("setvar", &json!({"value": 1}), &area).unwrap_err().contains("name the key"));
    }

    #[test]
    fn an_inner_command_passes_the_gate_itself_and_an_inner_effect_must_be_built_in() {
        let (_dir, area) = area();
        let d = door();
        let ok = d.admit("perspective.draw", &json!({"command": "shape.rect", "params": {"w": 1}}), &area).unwrap();
        assert_eq!(ok["params"], json!({"w": 1}));
        let e = d.admit("perspective.draw", &json!({"command": "plugin.install", "params": {}}), &area).unwrap_err();
        assert!(e.contains("runs only `shape.*` commands"), "{e}");
        assert!(d.admit("effect.apply", &json!({"effect": "stylize.dropShadow"}), &area).is_ok());
        assert!(d.admit("effect.apply", &json!({"effect": ["Gaussian Blur"]}), &area).is_ok());
        let e = d.admit("effect.apply", &json!({"effect": "plugin.evil"}), &area).unwrap_err();
        assert!(e.contains("`plugin.evil` is not an effect the engine builds in"), "{e}");
        assert!(d.admit("effect.apply", &json!({"effect": 3}), &area).is_err());
    }

    #[test]
    fn the_door_lists_what_it_runs_and_checks_its_settlements() {
        let d = door();
        assert_eq!(d.runnable(), ["edit.undo", "effect.apply", "insert.picture", "perspective.draw", "setvar", "shape.ellipse", "shape.rect"]);
        assert!(!d.runs("file.saveAs") && !d.runs("command.batch") && !d.runs("nope"));
        static WRONG: Reviewed = Reviewed { file_reads: &[FileRead { id: "shape.rect", params: &["path"] }], setters: &[], inner: &[] };
        assert!(Door::new("demo", SAFETY, &WRONG).unwrap_err().contains("classed safe, not file"));
        static MISSING: Reviewed = Reviewed { file_reads: &[], setters: &[], inner: &[Inner { id: "gone", param: "x", rule: InnerRule::Effect { builtin } }] };
        assert!(Door::new("demo", SAFETY, &MISSING).unwrap_err().contains("not in the catalog"));
        assert!(Door::new("demo", "{}", &Reviewed::NONE).is_err());
        assert!(Door::new("demo", r#"{"commands": {"a": "harmless"}}"#, &Reviewed::NONE).is_err());
    }

    #[test]
    fn nesting_is_bounded() {
        let (_dir, area) = area();
        static LOOP: Reviewed = Reviewed {
            file_reads: &[],
            setters: &[],
            inner: &[Inner { id: "perspective.draw", param: "command", rule: InnerRule::Command { prefix: "perspective.", params: "params" } }],
        };
        let d = Door::new("demo", SAFETY, &LOOP).unwrap();
        let mut params = json!({});
        for _ in 0..8 {
            params = json!({"command": "perspective.draw", "params": params});
        }
        assert!(d.admit("perspective.draw", &params, &area).unwrap_err().contains("nest too deep"));
    }

    #[test]
    fn a_whole_call_is_admitted_before_any_of_it_runs() {
        let (dir, area) = area();
        let d = door();
        let cmds = json!([{"id": "shape.rect", "params": {"w": 1}}, {"id": "edit.undo"}]);
        let admitted = d.admit_all(&cmds, &area).unwrap();
        assert_eq!(admitted, [("shape.rect".to_string(), json!({"w": 1})), ("edit.undo".to_string(), json!({}))]);
        let e = d.admit_all(&json!([{"id": "shape.rect"}, {"id": "plugin.install"}]), &area).unwrap_err();
        assert!(e.contains("`plugin.install` is classed code"), "{e}");
        assert!(d.admit_all(&json!([{"params": {}}]), &area).unwrap_err().contains("each command has an `id`"));
        assert!(d.admit_all(&json!({"id": "shape.rect"}), &area).unwrap_err().contains("`cmds` is a list"));
        let many: Vec<Value> = (0..MAX_COMMANDS + 1).map(|_| json!({"id": "edit.undo"})).collect();
        assert!(d.admit_all(&Value::Array(many), &area).unwrap_err().contains("at most 64 commands"));
        assert_eq!(d.admit_all(&json!([]), &area).unwrap(), []);
        // The reviewed reads of one call share one budget, nested ones too.
        let big = std::fs::File::create(dir.path().join("big.png")).unwrap();
        big.set_len(MAX_READ_BYTES / 2 + 1).unwrap();
        let one = json!([{"id": "insert.picture", "params": {"path": "big.png"}}]);
        assert!(d.admit_all(&one, &area).is_ok());
        let two = json!([{"id": "insert.picture", "params": {"path": "big.png"}}, {"id": "insert.picture", "params": {"path": "big.png"}}]);
        assert!(d.admit_all(&two, &area).unwrap_err().contains("total more than"), "two halves pass the budget");
    }
}
