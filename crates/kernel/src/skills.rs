//! The system agent's managed skills (ADR 0013): skills OctoSense installs
//! into the kernel's skills dir before every kernel start. Today that is one
//! skill per craft engine linked into the build, which the shell registers
//! at startup (`crates/shell/src/system_chat/skills.rs`) from the files each
//! engine service embeds (`apps/<family>/host-service/skill/`).
//!
//! **Where octos reads them** (octos at the pinned revision). `octos serve`
//! bootstraps one runtime per enabled profile with a model
//! (`commands/serve.rs` → `ProfileRuntime::bootstrap_with_host_plugins`,
//! `runtime/profile.rs`) and assembles there the system prompt every
//! session of that profile starts from (`build_system_prompt`,
//! `commands/gateway/prompt.rs`). Its `## Available Skills` block is
//! `SkillsLoader::new(<profile data dir>).build_skills_summary()`
//! (`skills_scope::build_account_skills_loader`): octos's builtin skills plus
//! one `<skill>` entry (name, description, location) per
//! `<profile data dir>/skills/<name>/SKILL.md`. A session appends that block
//! to its prompt (`runtime/session.rs`, `post_memory`), and a turn takes the
//! session's prompt whole unless it runs on an app peer's session for a
//! foreign connection (`app_context_allowed`), which the system session
//! (`_main:api:octosense#system`) never is. The system session is on the
//! `_main` profile, whose data dir is `<core dir>/profiles/_main/data`, or
//! the profile's own `data_dir` when it names one
//! (`ProfileStore::resolve_data_dir`); that is [`skills_dir`].
//!
//! The same dir is a read zone of every session's file tools
//! (`plugin_dirs` → `SessionScope::with_skill_read_zones`), so the agent's
//! `read_file` and `grep` reach a skill's files by the summary's absolute
//! location, but only if the dir exists when the session is built: it is
//! made here, before the start. The summary is assembled once per profile
//! runtime, so a change applies from the next kernel start. Every session of
//! `_main` sees the same summary: app agents' peers run on `_main` too.
//!
//! **Managed, never the person's.** Each managed skill's dir holds
//! [`MARKER`]. Before every start ([`sync`]):
//!
//! - each registered skill is written when it is missing or differs: built
//!   beside it and swapped in by rename, so a crash leaves only
//!   `.octosense-*` leftovers, which the next start removes;
//! - a dir with the marker whose name is no longer registered is removed
//!   (an engine left out of this build, a renamed skill);
//! - a dir without the marker is the person's (or octos's own
//!   `profile/skills/install`): it is never written, moved or removed, even
//!   when it has a registered skill's name (that skill is then not
//!   installed, and the log says why).
//!
//! Nothing is synced until a shell registers a set ([`set_managed`]): a
//! consumer that never does (a test's kernel, a standalone app) leaves the
//! dir alone. Like the tool policy, nothing is written into the person's
//! own octos home.
//!
//! **Skills fail open.** A skill is guidance, not a boundary: one that
//! cannot be installed is logged and the kernel starts anyway. The tool
//! policy is what fails closed ([`crate::system_tools`]).

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

/// One skill OctoSense manages, embedded in the build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManagedSkill {
    /// Its name: its dir under [`skills_dir`] and its frontmatter `name`
    /// (`word-engine`).
    pub name: &'static str,
    /// Its files, as (path relative to the skill's dir, content);
    /// `SKILL.md` is one of them.
    pub files: &'static [(&'static str, &'static str)],
}

/// The file that marks a skill dir as OctoSense's.
pub const MARKER: &str = ".octosense-managed";

/// What [`MARKER`] says to whoever opens it.
const MARKER_TEXT: &str = "OctoSense installed this skill and writes it again at every assistant start; it removes the skill \
when OctoSense no longer ships it. Edits here do not last. A skill of your own belongs in a folder of another name.\n";

/// The prefix of the dirs a sync builds or retires a skill in.
const SCRATCH_PREFIX: &str = ".octosense-";

/// The longest description a managed skill may have, in bytes: the summary
/// line is what every turn of every `_main` session pays for.
pub const DESCRIPTION_BUDGET: usize = 200;

/// The profile whose sessions read these skills (the system agent's).
const PROFILE: &str = crate::system_tools::SYSTEM_PROFILE;

// ---- the frontmatter, as octos reads it ------------------------------------

/// `SKILL.md`'s frontmatter lines, the way octos splits them
/// (`skills.rs` `split_frontmatter`): after a leading `---`, up to the next
/// line that starts with `---`.
fn frontmatter_lines(content: &str) -> Option<Vec<&str>> {
    let rest = content.trim_start().strip_prefix("---")?;
    let after = rest.trim_start_matches(['\r', '\n']);
    let end = after.find("\n---")?;
    Some(after[..end].lines().collect())
}

/// A frontmatter value the way octos reads it (`skills.rs` `fm_value`): the
/// first line starting with `key:`, cut at a `#`, and `None` for the YAML
/// empty markers.
fn fm_value(lines: &[&str], key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    lines.iter().find_map(|line| {
        let mut value = line.trim().strip_prefix(&prefix)?.trim();
        if let Some(hash) = value.find('#') {
            value = value[..hash].trim();
        }
        (!(value.is_empty() || value == "[]" || value == "\"\"" || value == "~")).then(|| value.to_owned())
    })
}

/// `name` and `description` of a `SKILL.md`, as octos's summary will show
/// them.
pub fn frontmatter(content: &str) -> Option<(String, String)> {
    let lines = frontmatter_lines(content)?;
    Some((fm_value(&lines, "name")?, fm_value(&lines, "description")?))
}

/// Whether `skill` is one OctoSense may install: a plain name, relative
/// file paths, and a `SKILL.md` whose frontmatter octos reads as written
/// (its `name`, a one-line description within [`DESCRIPTION_BUDGET`] with
/// none of the characters octos cuts at or leaves unescaped in its XML
/// summary, and never `always`, which would load the whole body into every
/// turn).
pub fn validate(skill: &ManagedSkill) -> Result<(), String> {
    let name = skill.name;
    let plain = |s: &str| s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if name.is_empty() || name.len() > 64 || !plain(name) || name.starts_with('-') {
        return Err(format!("`{name}` is not a skill name (lowercase letters, digits and dashes)"));
    }
    let mut paths = BTreeSet::new();
    for (path, _) in skill.files {
        let p = Path::new(path);
        let normal = !path.is_empty()
            && !path.contains('\\')
            && p.components().all(|c| matches!(c, Component::Normal(_)))
            && p.components().all(|c| !c.as_os_str().to_string_lossy().starts_with('.'));
        if !normal {
            return Err(format!("{name}: `{path}` is not a plain relative path"));
        }
        if !paths.insert(*path) {
            return Err(format!("{name}: `{path}` is listed twice"));
        }
    }
    let body = skill
        .files
        .iter()
        .find_map(|(path, body)| (*path == "SKILL.md").then_some(*body))
        .ok_or(format!("{name}: no SKILL.md"))?;
    let lines = frontmatter_lines(body).ok_or(format!("{name}: SKILL.md has no frontmatter"))?;
    if fm_value(&lines, "name").as_deref() != Some(name) {
        return Err(format!("{name}: SKILL.md's frontmatter names another skill"));
    }
    let raw = lines
        .iter()
        .find_map(|l| l.trim().strip_prefix("description:"))
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .ok_or(format!("{name}: SKILL.md has no description"))?;
    if raw.len() > DESCRIPTION_BUDGET {
        return Err(format!("{name}: its description is {} bytes, over the {DESCRIPTION_BUDGET}-byte budget", raw.len()));
    }
    if let Some(c) = raw.chars().find(|c| matches!(c, '#' | '<' | '>' | '&')) {
        return Err(format!("{name}: its description has `{c}`, which octos cuts at or puts unescaped into the summary"));
    }
    if raw.starts_with(['"', '\'']) {
        return Err(format!("{name}: its description is quoted; octos would show the quotes"));
    }
    if fm_value(&lines, "always").is_some_and(|v| v != "false") {
        return Err(format!("{name}: a managed skill is never `always`"));
    }
    Ok(())
}

// ---- the registered set -----------------------------------------------------

/// The skills the shell registered; `None` until it does.
static MANAGED: Mutex<Option<Vec<ManagedSkill>>> = Mutex::new(None);

/// The shell's skills for every kernel start from now on (a running kernel
/// picks them up at its next start). A skill that fails [`validate`] is left
/// out; the errors say why.
pub fn set_managed(skills: Vec<ManagedSkill>) -> Vec<String> {
    let mut errors = Vec::new();
    let mut names = BTreeSet::new();
    let mut kept = Vec::new();
    for skill in skills {
        match validate(&skill) {
            Ok(()) if names.insert(skill.name) => kept.push(skill),
            Ok(()) => errors.push(format!("{}: registered twice", skill.name)),
            Err(e) => errors.push(e),
        }
    }
    *MANAGED.lock().unwrap_or_else(|e| e.into_inner()) = Some(kept);
    errors
}

/// The registered skills, `None` before [`set_managed`].
pub fn managed() -> Option<Vec<ManagedSkill>> {
    MANAGED.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

// ---- where they go ----------------------------------------------------------

/// The skills dir octos reads for the system agent's profile:
/// `<core dir>/profiles/_main/data/skills`, or `<data_dir>/skills` when the
/// `_main` profile names its own absolute `data_dir` (octos's
/// `ProfileStore::resolve_data_dir`). A profile that cannot be read leaves
/// the default; a `data_dir` that is not absolute is refused.
pub fn skills_dir(core_dir: &Path) -> Result<PathBuf, String> {
    let profile = crate::dirs::profile_path(core_dir);
    let named = std::fs::read(&profile)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|v| v.get("data_dir").cloned())
        .filter(|v| !v.is_null());
    let data = match named {
        None => core_dir.join("profiles").join(PROFILE).join("data"),
        Some(Value::String(dir)) if Path::new(&dir).is_absolute() => PathBuf::from(dir),
        Some(other) => return Err(format!("{} names a data_dir OctoSense does not follow: {other}", profile.display())),
    };
    Ok(data.join("skills"))
}

// ---- the sync -----------------------------------------------------------------

/// What [`sync`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Synced {
    /// The skills dir.
    pub dir: PathBuf,
    /// The registered skills now installed, by name.
    pub current: Vec<String>,
    /// Of those, the ones this sync wrote (new or changed).
    pub written: Vec<String>,
    /// Managed skills removed because they are no longer registered.
    pub removed: Vec<String>,
    /// Registered skills not installed, with why.
    pub refused: Vec<String>,
}

impl Synced {
    /// One log line.
    pub fn note(&self) -> String {
        let mut note = format!("octos-core: managed skills in {}: {} current", self.dir.display(), self.current.len());
        if !self.written.is_empty() {
            note.push_str(&format!(", wrote {}", self.written.join(" ")));
        }
        if !self.removed.is_empty() {
            note.push_str(&format!(", removed {}", self.removed.join(" ")));
        }
        if !self.refused.is_empty() {
            note.push_str(&format!("; not installed: {}", self.refused.join("; ")));
        }
        note
    }
}

/// Install exactly `skills` as the managed skills of `core_dir`'s system
/// profile (see the module docs): write the missing and changed ones,
/// remove managed ones no longer listed, never touch the person's.
pub fn sync(core_dir: &Path, skills: &[ManagedSkill]) -> Result<Synced, String> {
    sync_unless_shared(core_dir, skills, crate::dirs::persons_octos_home().as_deref())
}

pub(crate) fn sync_unless_shared(core_dir: &Path, skills: &[ManagedSkill], shared: Option<&Path>) -> Result<Synced, String> {
    let dir = skills_dir(core_dir)?;
    if let Some(shared) = shared {
        let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        if canonical(core_dir) == canonical(shared) || dir.starts_with(shared) {
            return Err(format!("{} is the person's own octos home, not OctoSense's", core_dir.display()));
        }
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let mut synced = Synced { dir: dir.clone(), ..Synced::default() };
    // What an interrupted sync left.
    for entry in entries(&dir)? {
        if entry.name.starts_with(SCRATCH_PREFIX) && entry.real_dir {
            let _ = std::fs::remove_dir_all(dir.join(&entry.name));
        }
    }
    for skill in skills {
        if let Err(e) = validate(skill) {
            synced.refused.push(e);
            continue;
        }
        let target = dir.join(skill.name);
        match std::fs::symlink_metadata(&target) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => match install(&dir, skill, None) {
                Ok(()) => synced.written.push(skill.name.to_owned()),
                Err(e) => {
                    synced.refused.push(format!("{}: {e}", skill.name));
                    continue;
                }
            },
            Ok(meta) if meta.is_dir() && is_managed(&target) => {
                if !matches(&target, skill) {
                    match install(&dir, skill, Some(&target)) {
                        Ok(()) => synced.written.push(skill.name.to_owned()),
                        Err(e) => {
                            synced.refused.push(format!("{}: {e}", skill.name));
                            continue;
                        }
                    }
                }
            }
            Ok(_) => {
                synced.refused.push(format!("{}: a skill of that name OctoSense did not install is there; left alone", skill.name));
                continue;
            }
            Err(e) => {
                synced.refused.push(format!("{}: {e}", skill.name));
                continue;
            }
        }
        synced.current.push(skill.name.to_owned());
    }
    let registered: BTreeSet<&str> = skills.iter().map(|s| s.name).collect();
    for entry in entries(&dir)? {
        let path = dir.join(&entry.name);
        if entry.real_dir && !entry.name.starts_with('.') && !registered.contains(entry.name.as_str()) && is_managed(&path) {
            match std::fs::remove_dir_all(&path) {
                Ok(()) => synced.removed.push(entry.name),
                Err(e) => synced.refused.push(format!("{}: could not remove it: {e}", entry.name)),
            }
        }
    }
    Ok(synced)
}

/// Before a kernel start (`launch::prepare`): sync the registered set, if
/// the shell registered one; the line to log.
pub(crate) fn before_start(core_dir: &Path) -> Option<String> {
    let skills = managed()?;
    Some(match sync(core_dir, &skills) {
        Ok(synced) => synced.note(),
        Err(why) => format!("octos-core: managed skills NOT installed: {why}"),
    })
}

struct DirEntry {
    name: String,
    /// A directory, not a symlink to one.
    real_dir: bool,
}

fn entries(dir: &Path) -> Result<Vec<DirEntry>, String> {
    let read = std::fs::read_dir(dir).map_err(|e| format!("could not read {}: {e}", dir.display()))?;
    let mut out = Vec::new();
    for entry in read.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else { continue };
        let real_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        out.push(DirEntry { name, real_dir });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn is_managed(dir: &Path) -> bool {
    std::fs::symlink_metadata(dir.join(MARKER)).is_ok_and(|m| m.is_file())
}

/// Whether `dir` holds exactly `skill`'s files (and the marker).
fn matches(dir: &Path, skill: &ManagedSkill) -> bool {
    let mut found = BTreeSet::new();
    if !walk(dir, Path::new(""), &mut found) {
        return false;
    }
    let expected: BTreeSet<PathBuf> = skill.files.iter().map(|(p, _)| PathBuf::from(p)).chain([PathBuf::from(MARKER)]).collect();
    found == expected
        && std::fs::read(dir.join(MARKER)).is_ok_and(|b| b == MARKER_TEXT.as_bytes())
        && skill.files.iter().all(|(p, body)| std::fs::read(dir.join(p)).is_ok_and(|b| b == body.as_bytes()))
}

/// Every file under `dir` (relative); false at anything but plain files and
/// dirs.
fn walk(dir: &Path, rel: &Path, found: &mut BTreeSet<PathBuf>) -> bool {
    let Ok(read) = std::fs::read_dir(dir.join(rel)) else { return false };
    for entry in read.flatten() {
        let Ok(kind) = entry.file_type() else { return false };
        let path = rel.join(entry.file_name());
        if kind.is_dir() {
            if !walk(dir, &path, found) {
                return false;
            }
        } else if kind.is_file() {
            found.insert(path);
        } else {
            return false;
        }
    }
    true
}

/// Build `skill` beside `<dir>/<name>` and swap it in; `old` is the managed
/// copy it replaces.
fn install(dir: &Path, skill: &ManagedSkill, old: Option<&Path>) -> std::io::Result<()> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let fresh = dir.join(format!("{SCRATCH_PREFIX}new-{}-{id}", skill.name));
    let built = (|| {
        std::fs::create_dir(&fresh)?;
        for (path, body) in skill.files {
            let file = fresh.join(path);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(file, body)?;
        }
        std::fs::write(fresh.join(MARKER), MARKER_TEXT)
    })();
    if let Err(e) = built {
        let _ = std::fs::remove_dir_all(&fresh);
        return Err(e);
    }
    let target = dir.join(skill.name);
    let Some(old) = old else {
        let moved = std::fs::rename(&fresh, &target);
        if moved.is_err() {
            let _ = std::fs::remove_dir_all(&fresh);
        }
        return moved;
    };
    let retired = dir.join(format!("{SCRATCH_PREFIX}old-{}-{id}", skill.name));
    if let Err(e) = std::fs::rename(old, &retired) {
        let _ = std::fs::remove_dir_all(&fresh);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&fresh, &target) {
        let _ = std::fs::rename(&retired, &target);
        let _ = std::fs::remove_dir_all(&fresh);
        return Err(e);
    }
    let _ = std::fs::remove_dir_all(&retired);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORD_MD: &str = "---\nname: word-engine\ndescription: Word documents: create, read, convert. Read before using word.* tools.\n---\n\n# Word\n";
    const WORD: ManagedSkill = ManagedSkill { name: "word-engine", files: &[("SKILL.md", WORD_MD), ("commands.md", "- `a.b` A\n")] };
    const DECK_MD: &str = "---\nname: deck-engine\ndescription: Presentations. Read before using deck.* tools.\n---\nBody\n";
    const DECK: ManagedSkill = ManagedSkill { name: "deck-engine", files: &[("SKILL.md", DECK_MD)] };

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("octos-skills-{tag}-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(dir.join("profiles")).unwrap();
        dir
    }

    fn names(dir: &Path) -> Vec<String> {
        entries(dir).unwrap().into_iter().map(|e| e.name).collect()
    }

    #[test]
    fn the_skills_dir_is_the_main_profiles_data_dir_as_octos_resolves_it() {
        let core = tmp("dir");
        assert_eq!(skills_dir(&core).unwrap(), core.join("profiles/_main/data/skills"));
        // A profile without a data_dir (what the llm service writes) keeps the default.
        std::fs::write(core.join("profiles/_main.json"), r#"{"id":"_main","config":{"llm":{}}}"#).unwrap();
        assert_eq!(skills_dir(&core).unwrap(), core.join("profiles/_main/data/skills"));
        // A profile that names its own absolute data dir moves them, as octos does.
        let own = core.join("elsewhere");
        std::fs::write(core.join("profiles/_main.json"), serde_json::json!({"id": "_main", "data_dir": own}).to_string()).unwrap();
        assert_eq!(skills_dir(&core).unwrap(), own.join("skills"));
        // A relative one is not followed.
        std::fs::write(core.join("profiles/_main.json"), r#"{"id":"_main","data_dir":"rel"}"#).unwrap();
        assert!(skills_dir(&core).is_err());
        let _ = std::fs::remove_dir_all(core);
    }

    #[test]
    fn a_sync_installs_refreshes_and_removes_only_its_own() {
        let core = tmp("sync");
        let dir = core.join("profiles/_main/data/skills");
        // The person's own skill, and one of theirs that takes a registered name.
        std::fs::create_dir_all(dir.join("my-notes")).unwrap();
        std::fs::write(dir.join("my-notes/SKILL.md"), "---\nname: my-notes\ndescription: mine\n---\n").unwrap();
        std::fs::create_dir_all(dir.join("deck-engine")).unwrap();
        std::fs::write(dir.join("deck-engine/SKILL.md"), "theirs").unwrap();

        let first = sync_unless_shared(&core, &[WORD, DECK], None).unwrap();
        assert_eq!(first.dir, dir);
        assert_eq!(first.current, ["word-engine"]);
        assert_eq!(first.written, ["word-engine"]);
        assert_eq!(first.refused.len(), 1, "{first:?}");
        assert!(first.refused[0].starts_with("deck-engine: a skill of that name OctoSense did not install"), "{first:?}");
        assert_eq!(std::fs::read_to_string(dir.join("word-engine/SKILL.md")).unwrap(), WORD_MD);
        assert_eq!(std::fs::read_to_string(dir.join("word-engine/commands.md")).unwrap(), "- `a.b` A\n");
        assert!(dir.join("word-engine").join(MARKER).is_file());
        assert_eq!(std::fs::read_to_string(dir.join("deck-engine/SKILL.md")).unwrap(), "theirs", "never touched");

        // Unchanged: nothing written.
        let again = sync_unless_shared(&core, &[WORD], None).unwrap();
        assert_eq!(again.current, ["word-engine"]);
        assert!(again.written.is_empty() && again.removed.is_empty(), "{again:?}");

        // An edit, a stray file or a missing file is put right.
        std::fs::write(dir.join("word-engine/SKILL.md"), "edited").unwrap();
        std::fs::write(dir.join("word-engine/extra.md"), "stray").unwrap();
        std::fs::remove_file(dir.join("word-engine/commands.md")).unwrap();
        let fixed = sync_unless_shared(&core, &[WORD], None).unwrap();
        assert_eq!(fixed.written, ["word-engine"]);
        assert_eq!(std::fs::read_to_string(dir.join("word-engine/SKILL.md")).unwrap(), WORD_MD);
        assert!(!dir.join("word-engine/extra.md").exists());
        assert!(dir.join("word-engine/commands.md").is_file());

        // No longer registered: removed; the person's skills stay.
        let gone = sync_unless_shared(&core, &[], None).unwrap();
        assert_eq!(gone.removed, ["word-engine"]);
        assert_eq!(names(&dir), ["deck-engine", "my-notes"]);
        let _ = std::fs::remove_dir_all(core);
    }

    #[test]
    fn a_sync_clears_what_an_interrupted_one_left_and_never_follows_a_symlink() {
        let core = tmp("leftovers");
        let dir = core.join("profiles/_main/data/skills");
        std::fs::create_dir_all(dir.join(".octosense-new-word-engine-abc")).unwrap();
        std::fs::create_dir_all(dir.join(".octosense-old-word-engine-abc")).unwrap();
        sync_unless_shared(&core, &[WORD], None).unwrap();
        assert_eq!(names(&dir), ["word-engine"]);
        #[cfg(unix)]
        {
            // A symlink under a registered name is not ours, wherever it points.
            let outside = core.join("outside");
            std::fs::create_dir_all(&outside).unwrap();
            std::fs::write(outside.join(MARKER), MARKER_TEXT).unwrap();
            std::os::unix::fs::symlink(&outside, dir.join("deck-engine")).unwrap();
            let synced = sync_unless_shared(&core, &[WORD, DECK], None).unwrap();
            assert_eq!(synced.current, ["word-engine"]);
            assert!(synced.refused[0].starts_with("deck-engine:"));
            // Nor is it removed as stale.
            sync_unless_shared(&core, &[], None).unwrap();
            assert!(std::fs::symlink_metadata(dir.join("deck-engine")).unwrap().file_type().is_symlink());
            assert!(outside.join(MARKER).is_file());
        }
        let _ = std::fs::remove_dir_all(core);
    }

    #[test]
    fn nothing_is_written_into_the_persons_own_octos_home() {
        let core = tmp("shared");
        assert!(sync_unless_shared(&core, &[WORD], Some(&core)).unwrap_err().contains("own octos home"));
        assert!(!core.join("profiles/_main/data/skills").exists());
        let _ = std::fs::remove_dir_all(core);
    }

    #[test]
    fn validation_follows_what_octos_reads() {
        assert_eq!(validate(&WORD), Ok(()));
        assert_eq!(frontmatter(WORD_MD), Some(("word-engine".into(), "Word documents: create, read, convert. Read before using word.* tools.".into())));
        let bad = |md: &'static str| validate(&ManagedSkill { name: "word-engine", files: Box::leak(Box::new([("SKILL.md", md)])) });
        assert!(bad("no frontmatter").unwrap_err().contains("no frontmatter"));
        assert!(bad("---\nname: other\ndescription: x\n---\n").unwrap_err().contains("another skill"));
        assert!(bad("---\nname: word-engine\n---\n").unwrap_err().contains("no description"));
        assert!(bad("---\nname: word-engine\ndescription: see #3\n---\n").unwrap_err().contains("`#`"));
        assert!(bad("---\nname: word-engine\ndescription: a <b>\n---\n").unwrap_err().contains("`<`"));
        assert!(bad("---\nname: word-engine\ndescription: x\nalways: true\n---\n").unwrap_err().contains("never `always`"));
        let long: &'static str = Box::leak(format!("---\nname: word-engine\ndescription: {}\n---\n", "x".repeat(201)).into_boxed_str());
        assert!(bad(long).unwrap_err().contains("201 bytes"));
        for (name, files) in [
            ("Word", &[("SKILL.md", WORD_MD)][..]),
            ("word-engine", &[("../SKILL.md", WORD_MD)][..]),
            ("word-engine", &[("SKILL.md", WORD_MD), (".hidden", "x")][..]),
            ("word-engine", &[("commands.md", "x")][..]),
        ] {
            assert!(validate(&ManagedSkill { name, files: Box::leak(files.to_vec().into_boxed_slice()) }).is_err(), "{name} {files:?}");
        }
    }

    #[test]
    fn a_set_registers_only_valid_skills_and_nothing_syncs_before_one() {
        let core = tmp("registry");
        // `before_start` reads the process-wide set: before any, nothing happens.
        if managed().is_none() {
            assert_eq!(before_start(&core), None);
            assert!(!core.join("profiles/_main/data/skills").exists());
        }
        let broken = ManagedSkill { name: "broken", files: &[("SKILL.md", "no frontmatter")] };
        let errors = set_managed(vec![WORD, broken, WORD]);
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert_eq!(managed().unwrap(), [WORD]);
        let note = before_start(&core).unwrap();
        assert!(note.contains("1 current") && note.contains("wrote word-engine"), "{note}");
        assert!(core.join("profiles/_main/data/skills/word-engine/SKILL.md").is_file());
        let _ = std::fs::remove_dir_all(core);
    }
}
