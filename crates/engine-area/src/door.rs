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
//! Beyond the classes, an engine's reviewer settles three more kinds of
//! parameter ([`Reviewed`]):
//!
//! - **setters** classed `safe` that set an app-wide variable by key
//!   (cadcraft's `setvar`): only reviewed keys pass, and a setter with none
//!   is refused;
//! - **inner ids**, a parameter that names another command, effect or
//!   plug-in ([`Inner`]): a named command must start with its reviewed
//!   prefix and pass this gate itself, with its own parameters; a named
//!   effect must be one the engine builds in, never a plug-in;
//! - **limits** on the parameters that multiply work or memory ([`Limit`]):
//!   array and copy counts, rows and columns, canvas and render sizes,
//!   frame ranges, iteration counts. Engine work runs on the shell's UI
//!   thread, so no single call may ask for unbounded work: a command over
//!   its ceiling is refused, and the copies one call makes (an array of an
//!   array) multiply together within [`Reviewed::copies_per_call`].
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

/// How a [`Limit`] measures one call of its command.
pub enum Measure {
    /// The product of these numeric parameters, each named by its path
    /// (`rows`, or `size.0` for the first element of an array): one that is
    /// absent counts as 1, a negative one by its size.
    Product(&'static [&'static str]),
    /// Worked out by the reviewer from the parameters: `Ok(None)` when the
    /// call asks for nothing this limit bounds, `Err` to refuse the call.
    Custom(fn(&Value) -> Result<Option<f64>, String>),
}

/// A reviewed ceiling on how much work one command may ask the engine for.
pub struct Limit {
    pub id: &'static str,
    /// What the measure counts, for the refusal (`copies`, `pixels`, `frames`).
    pub what: &'static str,
    pub measure: Measure,
    /// The most one call of the command may ask for.
    pub max: f64,
    /// Whether the amount multiplies what the document already holds (an
    /// array, a copy or a blend of the selection, which a later copy
    /// multiplies again): such amounts also multiply together across one
    /// call, within [`Reviewed::copies_per_call`].
    pub copies: bool,
}

/// What an engine's reviewer settled beyond the classes.
pub struct Reviewed {
    pub file_reads: &'static [FileRead],
    pub setters: &'static [Setter],
    pub inner: &'static [Inner],
    pub limits: &'static [Limit],
    /// The most the copy limits' amounts may multiply to in one call.
    pub copies_per_call: f64,
}

impl Reviewed {
    pub const NONE: Reviewed = Reviewed { file_reads: &[], setters: &[], inner: &[], limits: &[], copies_per_call: 1.0 };
}

/// What one call has asked for so far.
struct Budget {
    /// Bytes its reviewed reads name.
    read: u64,
    /// The product of its copy limits' amounts.
    copies: f64,
}

impl Default for Budget {
    fn default() -> Self {
        Budget { read: 0, copies: 1.0 }
    }
}

/// A number for a message: whole numbers without a fraction.
fn shown(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

impl Measure {
    /// The amount `params` ask for (`None`: nothing this measure bounds).
    fn of(&self, params: &Value) -> Result<Option<f64>, String> {
        match self {
            Measure::Custom(f) => f(params),
            Measure::Product(paths) => {
                let mut amount = 1.0f64;
                let mut any = false;
                for path in *paths {
                    let mut at = params;
                    let mut found = true;
                    for step in path.split('.') {
                        let next = match at {
                            Value::Object(map) => map.get(step),
                            Value::Array(items) => step.parse::<usize>().ok().and_then(|i| items.get(i)),
                            _ => None,
                        };
                        match next {
                            Some(v) => at = v,
                            None => {
                                found = false;
                                break;
                            }
                        }
                    }
                    if !found || at.is_null() {
                        continue;
                    }
                    let n = match at {
                        Value::Number(n) => n.as_f64(),
                        Value::String(s) => s.trim().parse::<f64>().ok(),
                        _ => None,
                    }
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| format!("`{path}` is a number the door bounds"))?;
                    amount *= n.abs();
                    any = true;
                }
                Ok(any.then_some(amount))
            }
        }
    }

    /// What a product multiplies, for a message: `rows` × `columns`.
    fn label(&self) -> String {
        match self {
            Measure::Product(paths) => paths.iter().map(|p| format!("`{p}`")).collect::<Vec<_>>().join(" × "),
            Measure::Custom(_) => String::new(),
        }
    }
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
        for limit in reviewed.limits {
            if !door.runs(limit.id) {
                return Err(format!("{family}: a limit on `{}`, which the door does not run", limit.id));
            }
            if limit.max.is_nan() || limit.max <= 0.0 || matches!(&limit.measure, Measure::Product(paths) if paths.is_empty()) {
                return Err(format!("{family}: the limit on `{}` bounds nothing", limit.id));
            }
        }
        if reviewed.copies_per_call.is_nan() || reviewed.copies_per_call < 1.0 {
            return Err(format!("{family}: `copies_per_call` is at least 1"));
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
    /// most [`MAX_READ_BYTES`] and whose copies multiply to at most
    /// [`Reviewed::copies_per_call`]. Returns each command's id and the
    /// parameters to run it with.
    pub fn admit_all(&self, cmds: &Value, area: &Area) -> Result<Vec<(String, Value)>, String> {
        let family = self.family;
        let cmds = cmds.as_array().ok_or_else(|| format!("{family}.run: `cmds` is a list of {{id, params?}}"))?;
        if cmds.len() > MAX_COMMANDS {
            return Err(format!("{family}.run: at most {MAX_COMMANDS} commands per call"));
        }
        let mut budget = Budget::default();
        let mut admitted = Vec::with_capacity(cmds.len());
        for c in cmds {
            let id = c["id"].as_str().filter(|id| !id.is_empty()).ok_or_else(|| format!("{family}.run: each command has an `id`"))?;
            admitted.push((id.to_string(), self.admit_at(id, &c["params"], area, 0, &mut budget)?));
        }
        Ok(admitted)
    }

    /// Admit one command of a `run` call working in `area`: its id must be
    /// one the door runs, its setter keys reviewed, any id it names admitted
    /// in turn, and its reviewed file parameters inside the area. Returns
    /// the parameters to run it with (file parameters made absolute).
    pub fn admit(&self, id: &str, params: &Value, area: &Area) -> Result<Value, String> {
        self.admit_at(id, params, area, 0, &mut Budget::default())
    }

    /// How many times an admitted command multiplies what the document
    /// holds: the product of its copy limits' amounts, 1 for a command
    /// that copies nothing. For a service that checks, before the command
    /// runs, that the document it would grow stays within its own ceiling.
    pub fn copies(&self, id: &str, params: &Value) -> f64 {
        self.reviewed
            .limits
            .iter()
            .filter(|l| l.id == id && l.copies)
            .filter_map(|l| l.measure.of(params).ok().flatten())
            .fold(1.0, |acc, n| acc * n.max(1.0))
    }

    /// [`Door::admit`] at nesting `depth`, adding what it asks for to the
    /// call's `budget`.
    fn admit_at(&self, id: &str, params: &Value, area: &Area, depth: usize, budget: &mut Budget) -> Result<Value, String> {
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
                        budget.read = budget.read.saturating_add(len);
                        if budget.read > MAX_READ_BYTES {
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
                    let admitted = self.admit_at(inner_id, &inner_params, area, depth + 1, budget)?;
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
        for limit in self.reviewed.limits.iter().filter(|l| l.id == id) {
            let Some(amount) = limit.measure.of(&params).map_err(|e| format!("{family}.run: `{id}`: {e}"))? else { continue };
            if amount.is_nan() || amount > limit.max {
                let (n, max, what) = (shown(amount), shown(limit.max), limit.what);
                return Err(match &limit.measure {
                    Measure::Product(_) => format!(
                        "{family}.run: `{id}`: {} is {n}, more than the {max} {what} the door allows in one command",
                        limit.measure.label()
                    ),
                    Measure::Custom(_) => format!("{family}.run: `{id}` asks for {n} {what}, more than the {max} the door allows in one command"),
                });
            }
            if limit.copies {
                budget.copies *= amount.max(1.0);
                if budget.copies.is_nan() || budget.copies > self.reviewed.copies_per_call {
                    return Err(format!(
                        "{family}.run: `{id}`: the copies this call makes multiply to {}, more than the {} the door allows in one call (a copy multiplies what the commands before it made)",
                        shown(budget.copies),
                        shown(self.reviewed.copies_per_call)
                    ));
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
        "setvar": "safe", "perspective.draw": "safe", "effect.apply": "safe",
        "shape.array": "safe", "canvas.new": "safe", "effect.setParam": "safe"
    }}"#;

    fn keys_of(params: &Value) -> Vec<String> {
        params["name"].as_str().map(|k| vec![k.to_string()]).unwrap_or_default()
    }

    fn builtin(name: &str) -> bool {
        ["stylize.dropShadow", "Gaussian Blur"].contains(&name)
    }

    /// A blur's `samples` multiply its work; its other parameters do not.
    fn samples(params: &Value) -> Result<Option<f64>, String> {
        Ok((params["param"] == "samples").then(|| params["value"].as_f64().unwrap_or(f64::INFINITY)))
    }

    static REVIEWED: Reviewed = Reviewed {
        file_reads: &[FileRead { id: "insert.picture", params: &["path"] }],
        setters: &[Setter { id: "setvar", keys: &["ORTHOMODE"], keys_of }],
        inner: &[
            Inner { id: "perspective.draw", param: "command", rule: InnerRule::Command { prefix: "shape.", params: "params" } },
            Inner { id: "effect.apply", param: "effect", rule: InnerRule::Effect { builtin } },
        ],
        limits: &[
            Limit { id: "shape.array", what: "copies", measure: Measure::Product(&["rows", "columns"]), max: 100.0, copies: true },
            Limit { id: "canvas.new", what: "pixels", measure: Measure::Product(&["size.0", "size.1"]), max: 1e6, copies: false },
            Limit { id: "effect.setParam", what: "samples", measure: Measure::Custom(samples), max: 64.0, copies: false },
        ],
        copies_per_call: 1000.0,
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
        assert_eq!(
            d.runnable(),
            ["canvas.new", "edit.undo", "effect.apply", "effect.setParam", "insert.picture", "perspective.draw", "setvar", "shape.array", "shape.ellipse", "shape.rect"]
        );
        assert!(!d.runs("file.saveAs") && !d.runs("command.batch") && !d.runs("nope"));
        static WRONG: Reviewed = Reviewed { file_reads: &[FileRead { id: "shape.rect", params: &["path"] }], ..Reviewed::NONE };
        assert!(Door::new("demo", SAFETY, &WRONG).unwrap_err().contains("classed safe, not file"));
        static MISSING: Reviewed = Reviewed { inner: &[Inner { id: "gone", param: "x", rule: InnerRule::Effect { builtin } }], ..Reviewed::NONE };
        assert!(Door::new("demo", SAFETY, &MISSING).unwrap_err().contains("not in the catalog"));
        static REFUSED: Reviewed = Reviewed {
            limits: &[Limit { id: "command.batch", what: "steps", measure: Measure::Product(&["n"]), max: 1.0, copies: false }],
            ..Reviewed::NONE
        };
        assert!(Door::new("demo", SAFETY, &REFUSED).unwrap_err().contains("which the door does not run"));
        static EMPTY: Reviewed = Reviewed { limits: &[Limit { id: "shape.rect", what: "x", measure: Measure::Product(&[]), max: 1.0, copies: false }], ..Reviewed::NONE };
        assert!(Door::new("demo", SAFETY, &EMPTY).unwrap_err().contains("bounds nothing"));
        static NO_COPIES: Reviewed = Reviewed { copies_per_call: 0.0, ..Reviewed::NONE };
        assert!(Door::new("demo", SAFETY, &NO_COPIES).is_err());
        assert!(Door::new("demo", "{}", &Reviewed::NONE).is_err());
        assert!(Door::new("demo", r#"{"commands": {"a": "harmless"}}"#, &Reviewed::NONE).is_err());
    }

    #[test]
    fn nesting_is_bounded() {
        let (_dir, area) = area();
        static LOOP: Reviewed = Reviewed {
            inner: &[Inner { id: "perspective.draw", param: "command", rule: InnerRule::Command { prefix: "perspective.", params: "params" } }],
            ..Reviewed::NONE
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

    #[test]
    fn a_limit_bounds_a_parameter_or_a_product_before_anything_runs() {
        let (_dir, area) = area();
        let d = door();
        // At the ceiling passes; one over is refused, naming what and how much.
        assert!(d.admit("shape.array", &json!({"rows": 10, "columns": 10}), &area).is_ok());
        let e = d.admit("shape.array", &json!({"rows": 11, "columns": 10}), &area).unwrap_err();
        assert_eq!(e, "demo.run: `shape.array`: `rows` × `columns` is 110, more than the 100 copies the door allows in one command");
        // An absent factor counts as 1, a negative one by its size, a
        // numeric string as its number; anything else is refused.
        assert!(d.admit("shape.array", &json!({"rows": 100}), &area).is_ok());
        assert!(d.admit("shape.array", &json!({"rows": -101}), &area).unwrap_err().contains("is 101"));
        assert!(d.admit("shape.array", &json!({"rows": "1e6"}), &area).unwrap_err().contains("is 1000000"));
        assert!(d.admit("shape.array", &json!({"rows": [5]}), &area).unwrap_err().contains("`rows` is a number the door bounds"));
        assert!(d.admit("shape.array", &json!({}), &area).is_ok(), "nothing asked: the engine's default");
        // A path into an array: a huge canvas.
        assert!(d.admit("canvas.new", &json!({"size": [1000, 1000]}), &area).is_ok());
        let e = d.admit("canvas.new", &json!({"size": [100000, 100000]}), &area).unwrap_err();
        assert!(e.contains("`size.0` × `size.1` is 10000000000, more than the 1000000 pixels"), "{e}");
        // A reviewer's own measure: only the parameter it names is bounded.
        assert!(d.admit("effect.setParam", &json!({"param": "radius", "value": 1e9}), &area).is_ok());
        assert!(d.admit("effect.setParam", &json!({"param": "samples", "value": 64}), &area).is_ok());
        assert!(d.admit("effect.setParam", &json!({"param": "samples", "value": 65}), &area).unwrap_err().contains("asks for 65 samples"));
    }

    #[test]
    fn copies_multiply_across_one_call_and_through_inner_commands() {
        let (_dir, area) = area();
        let d = door();
        let one = json!([{"id": "shape.rect"}, {"id": "shape.array", "params": {"rows": 10, "columns": 10}}]);
        assert!(d.admit_all(&one, &area).is_ok());
        // Two arrays at the ceiling each: the second copies the first's
        // hundred, 10,000 in all, over the call's 1,000.
        let chained = json!([{"id": "shape.array", "params": {"rows": 10, "columns": 10}}, {"id": "shape.array", "params": {"rows": 10, "columns": 10}}]);
        let e = d.admit_all(&chained, &area).unwrap_err();
        assert!(e.contains("the copies this call makes multiply to 10000, more than the 1000"), "{e}");
        // An inner command carries its own parameters past no limit.
        let nested = json!({"command": "shape.array", "params": {"rows": 1000, "columns": 1000}});
        let e = d.admit("perspective.draw", &nested, &area).unwrap_err();
        assert!(e.contains("`shape.array`: `rows` × `columns` is 1000000"), "{e}");
        let nested_ok = json!([{"id": "perspective.draw", "params": {"command": "shape.array", "params": {"rows": 10, "columns": 10}}}, {"id": "shape.array", "params": {"rows": 10, "columns": 10}}]);
        assert!(d.admit_all(&nested_ok, &area).unwrap_err().contains("multiply to 10000"), "an inner command's copies count too");
        // What a service checks before an admitted command runs.
        assert_eq!(d.copies("shape.array", &json!({"rows": 4, "columns": 5})), 20.0);
        assert_eq!(d.copies("shape.rect", &json!({})), 1.0);
        assert_eq!(d.copies("canvas.new", &json!({"size": [10, 10]})), 1.0, "not a copy");
    }
}
