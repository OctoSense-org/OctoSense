//! `octosense-engine-area` — where a craft engine works (ADR 0013).
//!
//! A craft engine is an OctoSense-wide tool that runs in its caller's own
//! folder: the system agent's workspace, an app agent's account folder, or
//! an app's own storage. Every engine host service asks the same question
//! before it touches a file, and [`Slot::area`] answers it as an [`Area`]:
//!
//! - **where** the call works ([`Area::root`]): every path the caller names
//!   is relative to it, and the service keeps each one inside it (its own
//!   containment check, symlinks resolved);
//! - **whether a write may replace** an existing file
//!   ([`Area::may_replace`]): an agent's call never does, an app's own
//!   foreground call may, as before;
//! - **how many bytes** the call may still add ([`Area::quota_left`]): what
//!   remains of an app's storage quota, or no limit beyond the service's own
//!   per-call caps.
//!
//! The shell installs one resolver per service ([`Slot::set`]); it decides
//! from trusted host data only, never from a call's arguments. With no
//! resolver (tests, App Hub's card-host) a call works in the legacy private
//! folder `<host dir>/<family>`, may replace, and has no quota
//! ([`Area::legacy`]).
//!
//! Writes go through [`Area::write`] when the service holds the bytes, or a
//! [`Stage`] when the engine writes its own files: either way a write that
//! may not replace refuses any existing entry (file, folder or link) and
//! never races one into place, no write goes through a symbolic link, and
//! the bytes one call adds stay within its quota.

use std::collections::HashSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use octosense_appstore::services::ServiceCall;

/// Where one call works, and what its writes may do there.
pub struct Area {
    /// The folder every path of the call is relative to.
    pub root: PathBuf,
    /// The bytes the call may still add here (`None`: no quota beyond the
    /// service's own per-call caps).
    pub quota_left: Option<u64>,
    /// Whether a write may replace an existing file.
    pub may_replace: bool,
    /// What this call's writes added, and what the files they replaced
    /// held: one budget for every write of the call.
    added: AtomicU64,
    freed: AtomicU64,
}

impl Clone for Area {
    fn clone(&self) -> Area {
        Area {
            root: self.root.clone(),
            quota_left: self.quota_left,
            may_replace: self.may_replace,
            added: AtomicU64::new(self.added.load(Ordering::Relaxed)),
            freed: AtomicU64::new(self.freed.load(Ordering::Relaxed)),
        }
    }
}

impl PartialEq for Area {
    fn eq(&self, other: &Area) -> bool {
        self.root == other.root && self.quota_left == other.quota_left && self.may_replace == other.may_replace
    }
}

impl std::fmt::Debug for Area {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Area")
            .field("root", &self.root)
            .field("quota_left", &self.quota_left)
            .field("may_replace", &self.may_replace)
            .finish()
    }
}

/// Staging folders' names start with this, inside the area they stage for.
pub const STAGING_PREFIX: &str = ".engine-staging-";

static NEXT: AtomicU64 = AtomicU64::new(1);

impl Area {
    pub fn new(root: impl Into<PathBuf>, quota_left: Option<u64>, may_replace: bool) -> Area {
        Area { root: root.into(), quota_left, may_replace, added: AtomicU64::new(0), freed: AtomicU64::new(0) }
    }

    /// The legacy private folder of a family under a host directory:
    /// `<host_dir>/<family>`, replacing allowed, no quota. What a service
    /// uses without the shell's resolver.
    pub fn legacy(host_dir: &Path, family: &str) -> Area {
        Area::new(host_dir.join(family), None, true)
    }

    /// The bytes this call may still add (`None`: no quota).
    pub fn room(&self) -> Option<u64> {
        let quota = self.quota_left?;
        let added = self.added.load(Ordering::Relaxed);
        let freed = self.freed.load(Ordering::Relaxed);
        Some(quota.saturating_add(freed).saturating_sub(added))
    }

    fn spend(&self, added: u64, freed: u64) {
        self.added.fetch_add(added, Ordering::Relaxed);
        self.freed.fetch_add(freed, Ordering::Relaxed);
    }

    /// `path` as the caller names it: relative to the root, never the
    /// host's own spelling of it.
    pub fn shown(&self, path: &Path) -> String {
        match path.strip_prefix(&self.root) {
            Ok(rel) if !rel.as_os_str().is_empty() => rel.display().to_string(),
            _ => path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        }
    }

    /// `text` with the host's spellings of the root taken out: a path inside
    /// the area reads relative to it, and the root itself reads `.`. An
    /// engine can name the absolute path the service opened, in a result
    /// (pdfcraft's `document.path`) or an error; the person's home directory
    /// is in it, and the caller works in relative paths anyway.
    pub fn relative_text(&self, text: &str) -> String {
        let mut text = text.to_string();
        for root in self.root_spellings() {
            if text == root {
                return ".".into();
            }
            for separator in ['/', '\\'] {
                text = text.replace(&format!("{root}{separator}"), "");
            }
        }
        text
    }

    /// `value` with [`Area::relative_text`] applied to every string in it:
    /// what a service's answer may show its caller.
    pub fn relative_json(&self, value: serde_json::Value) -> serde_json::Value {
        use serde_json::Value;
        fn walk(value: Value, area: &Area) -> Value {
            match value {
                Value::String(text) => Value::String(area.relative_text(&text)),
                Value::Array(items) => Value::Array(items.into_iter().map(|item| walk(item, area)).collect()),
                Value::Object(fields) => Value::Object(fields.into_iter().map(|(key, item)| (key, walk(item, area))).collect()),
                other => other,
            }
        }
        walk(value, self)
    }

    /// The root as given and as the filesystem resolves it (macOS: `/var`
    /// is `/private/var`), the longest first, so a spelling is never cut
    /// out of a longer one.
    fn root_spellings(&self) -> Vec<String> {
        let mut spellings: Vec<String> = std::iter::once(self.root.clone())
            .chain(self.root.canonicalize().ok())
            .map(|root| root.display().to_string())
            .filter(|root| !root.is_empty())
            .collect();
        spellings.sort_by_key(|root| std::cmp::Reverse(root.len()));
        spellings.dedup();
        spellings
    }

    /// Whether a write of `len` bytes may land at `path` (already contained
    /// in the area): the bytes the entry there holds now, which the write
    /// would free. An existing entry is refused unless the call may replace
    /// it; a folder is never replaced.
    fn admit(&self, path: &Path, len: u64) -> Result<u64, String> {
        let replaced = match std::fs::symlink_metadata(path) {
            Ok(meta) => {
                if !self.may_replace {
                    return Err(format!("`{}` already exists, and this call never replaces a file: choose a new name", self.shown(path)));
                }
                if meta.is_dir() {
                    return Err(format!("`{}` is a folder", self.shown(path)));
                }
                // Replacing a link replaces the link itself, which frees no
                // bytes of the area.
                if meta.file_type().is_symlink() { 0 } else { meta.len() }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
            Err(e) => return Err(format!("`{}`: {e}", self.shown(path))),
        };
        if let Some(room) = self.room() {
            if len > room.saturating_add(replaced) {
                return Err(format!("the result is {len} bytes, more than the {room} bytes left in this storage"));
            }
        }
        Ok(replaced)
    }

    /// Whether a write of `len` bytes to `path` would be admitted, without
    /// writing: for a caller that must know before it starts (several
    /// outputs of one call checked before the first is written).
    pub fn check(&self, path: &Path, len: u64) -> Result<(), String> {
        self.admit(path, len).map(|_| ())
    }

    /// Write `bytes` to `path` (already contained in the area), creating its
    /// parent folders, under the area's rules: no replacement unless
    /// [`Area::may_replace`] (and then by an atomic rename, which replaces a
    /// link rather than writing through it), within the quota.
    pub fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        let len = bytes.len() as u64;
        let replaced = self.admit(path, len)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("`{}`: {e}", self.shown(path)))?;
        }
        if self.may_replace {
            let tmp = beside(path);
            let written = (|| -> std::io::Result<()> {
                let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
                file.write_all(bytes)?;
                drop(file);
                std::fs::rename(&tmp, path)
            })();
            if let Err(e) = written {
                let _ = std::fs::remove_file(&tmp);
                return Err(format!("`{}`: {e}", self.shown(path)));
            }
        } else {
            // `create_new` refuses any entry already there, a dangling link
            // included, so nothing raced into place is replaced either.
            let mut file = match std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    return Err(format!("`{}` already exists, and this call never replaces a file: choose a new name", self.shown(path)));
                }
                Err(e) => return Err(format!("`{}`: {e}", self.shown(path))),
            };
            if let Err(e) = file.write_all(bytes) {
                drop(file);
                let _ = std::fs::remove_file(path);
                return Err(format!("`{}`: {e}", self.shown(path)));
            }
        }
        self.spend(len, replaced);
        Ok(())
    }

    /// A fresh staging folder inside the area, for an engine that writes its
    /// own output files: the engine writes there, then [`Stage::commit`]
    /// moves the results into place under the area's rules. Inside the area,
    /// so a move is a rename or a link on one file system. Removed when the
    /// [`Stage`] is dropped; one a crashed process left behind is removed by
    /// the next staging in the same area.
    pub fn stage(&self) -> Result<Stage<'_>, String> {
        std::fs::create_dir_all(&self.root).map_err(|e| format!("cannot prepare a staging folder: {e}"))?;
        let mine = format!("{STAGING_PREFIX}{}-", std::process::id());
        if let Ok(entries) = std::fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let stale = name.starts_with(STAGING_PREFIX) && !name.starts_with(&mine);
                if stale && entry.file_type().is_ok_and(|t| t.is_dir()) {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
        loop {
            let dir = self.root.join(format!("{mine}{}", NEXT.fetch_add(1, Ordering::Relaxed)));
            match std::fs::create_dir(&dir) {
                Ok(()) => return Ok(Stage { area: self, dir }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("cannot prepare a staging folder: {e}")),
            }
        }
    }
}

/// A temporary name beside `path`, for an atomic replacement.
fn beside(path: &Path) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!(".{name}.{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)))
}

/// A staging folder inside an [`Area`] ([`Area::stage`]).
pub struct Stage<'a> {
    area: &'a Area,
    dir: PathBuf,
}

impl Stage<'_> {
    /// The staging folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// A path in the staging folder.
    pub fn path(&self, rel: impl AsRef<Path>) -> PathBuf {
        self.dir.join(rel)
    }

    /// Every regular file under the staging folder, relative to it, sorted.
    /// A link the engine left is not a file, and is never moved.
    pub fn files(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut todo = vec![self.dir.clone()];
        while let Some(dir) = todo.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
                if meta.is_dir() {
                    todo.push(entry.path());
                } else if meta.is_file() {
                    if let Ok(rel) = entry.path().strip_prefix(&self.dir) {
                        out.push(rel.to_path_buf());
                    }
                }
            }
        }
        out.sort();
        out
    }

    /// Move each staged file to its destination (inside the area), all or
    /// nothing: every destination is admitted, and the sum of the results
    /// checked against the quota, before anything moves. A destination that
    /// may not be replaced is placed by a hard link, which refuses whatever
    /// raced into place (else a fresh copy).
    pub fn commit(&self, moves: &[(PathBuf, PathBuf)]) -> Result<(), String> {
        let mut added = 0u64;
        let mut freed = 0u64;
        let mut seen = HashSet::new();
        for (staged, dest) in moves {
            let meta = match std::fs::symlink_metadata(staged) {
                Ok(meta) if staged.starts_with(&self.dir) && meta.is_file() => meta,
                _ => return Err(format!("the engine wrote no file for `{}`", self.area.shown(dest))),
            };
            if !seen.insert(dest.clone()) {
                return Err(format!("two results would land on `{}`", self.area.shown(dest)));
            }
            freed += self.area.admit(dest, 0)?;
            added += meta.len();
        }
        if let Some(room) = self.area.room() {
            if added > room.saturating_add(freed) {
                return Err(format!("the results are {added} bytes, more than the {room} bytes left in this storage"));
            }
        }
        let mut placed: Vec<&Path> = Vec::new();
        for (staged, dest) in moves {
            let moved = (|| -> std::io::Result<()> {
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                if self.area.may_replace { std::fs::rename(staged, dest) } else { place_new(staged, dest) }
            })();
            if let Err(e) = moved {
                // What this commit created goes again; nothing was replaced.
                if !self.area.may_replace {
                    for dest in placed {
                        let _ = std::fs::remove_file(dest);
                    }
                }
                return Err(match e.kind() {
                    std::io::ErrorKind::AlreadyExists => {
                        format!("`{}` already exists, and this call never replaces a file: choose a new name", self.area.shown(dest))
                    }
                    _ => format!("`{}`: {e}", self.area.shown(dest)),
                });
            }
            placed.push(dest);
        }
        self.area.spend(added, freed);
        Ok(())
    }
}

impl Drop for Stage<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// `staged` at `dest` only when nothing is there: a hard link (atomic; it
/// refuses an existing entry), else, where links are not supported, a copy
/// into a newly created file.
fn place_new(staged: &Path, dest: &Path) -> std::io::Result<()> {
    match std::fs::hard_link(staged, dest) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(e),
        Err(_) => {
            let mut src = std::fs::File::open(staged)?;
            let mut out = std::fs::OpenOptions::new().write(true).create_new(true).open(dest)?;
            if let Err(e) = std::io::copy(&mut src, &mut out) {
                drop(out);
                let _ = std::fs::remove_file(dest);
                return Err(e);
            }
            Ok(())
        }
    }
}

/// The shell's resolver: where a call works, decided from trusted host
/// data (the call's identity and its `host_dir`, which only host code sets),
/// never from its arguments.
pub type Resolver = Arc<dyn Fn(&ServiceCall) -> Result<Area, String> + Send + Sync>;

/// One service's resolver, installed by the shell at registration.
pub struct Slot {
    resolver: RwLock<Option<Resolver>>,
}

impl Default for Slot {
    fn default() -> Slot {
        Slot::new()
    }
}

impl Slot {
    pub const fn new() -> Slot {
        Slot { resolver: RwLock::new(None) }
    }

    /// Install the resolver (`None` removes it).
    pub fn set(&self, resolver: Option<Resolver>) {
        *self.resolver.write().unwrap_or_else(|e| e.into_inner()) = resolver;
    }

    pub fn is_set(&self) -> bool {
        self.resolver.read().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// The area `call` works in: the resolver's, which must be an existing
    /// folder, or without one the legacy private folder of `family` under
    /// the call's host directory, created on first use.
    pub fn area(&self, call: &ServiceCall, family: &str) -> Result<Area, String> {
        let resolver = self.resolver.read().unwrap_or_else(|e| e.into_inner()).clone();
        match resolver {
            Some(resolve) => {
                let area = resolve(call)?;
                if !area.root.is_dir() {
                    return Err("this call's folder is not there".into());
                }
                Ok(area)
            }
            None => {
                let area = Area::legacy(&call.host_dir, family);
                std::fs::create_dir_all(&area.root).map_err(|e| format!("cannot prepare its folder: {e}"))?;
                Ok(area)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall {
            app_id: "os.fixture".into(),
            service: "word.info".into(),
            args: serde_json::json!({}),
            from_sheet: false,
            may_prompt,
            host_dir: host_dir.to_path_buf(),
        }
    }

    #[test]
    fn without_a_resolver_a_call_works_in_its_familys_legacy_folder() {
        let dir = tempfile::tempdir().unwrap();
        let slot = Slot::new();
        assert!(!slot.is_set());
        let area = slot.area(&call(dir.path(), false), "word").unwrap();
        assert_eq!(area, Area::new(dir.path().join("word"), None, true));
        assert!(dir.path().join("word").is_dir(), "created on first use");
    }

    #[test]
    fn a_resolver_decides_the_area_and_its_folder_must_exist() {
        let dir = tempfile::tempdir().unwrap();
        let mine = dir.path().join("mine");
        std::fs::create_dir(&mine).unwrap();
        let slot = Slot::new();
        let root = mine.clone();
        slot.set(Some(Arc::new(move |call: &ServiceCall| Ok(Area::new(&root, Some(10), call.may_prompt)))));
        assert!(slot.is_set());
        assert_eq!(slot.area(&call(dir.path(), true), "word").unwrap(), Area::new(&mine, Some(10), true));
        assert!(!dir.path().join("word").exists(), "the legacy folder is not made");
        slot.set(Some(Arc::new(|_: &ServiceCall| Err("signed out".to_string()))));
        assert_eq!(slot.area(&call(dir.path(), true), "word").unwrap_err(), "signed out");
        let gone = dir.path().join("gone");
        slot.set(Some(Arc::new(move |_: &ServiceCall| Ok(Area::new(&gone, None, false)))));
        assert!(slot.area(&call(dir.path(), true), "word").is_err(), "a missing folder is refused, not made");
        slot.set(None);
        assert_eq!(slot.area(&call(dir.path(), true), "word").unwrap().root, dir.path().join("word"));
    }

    #[test]
    fn a_call_that_may_not_replace_only_creates() {
        let dir = tempfile::tempdir().unwrap();
        let area = Area::new(dir.path(), None, false);
        area.write(&dir.path().join("a/new.txt"), b"one").unwrap();
        assert_eq!(std::fs::read(dir.path().join("a/new.txt")).unwrap(), b"one");
        let refused = area.write(&dir.path().join("a/new.txt"), b"two").unwrap_err();
        assert!(refused.contains("`a/new.txt` already exists"), "{refused}");
        assert_eq!(std::fs::read(dir.path().join("a/new.txt")).unwrap(), b"one", "untouched");
        std::fs::create_dir(dir.path().join("folder")).unwrap();
        assert!(area.write(&dir.path().join("folder"), b"x").is_err());
        assert!(area.check(&dir.path().join("a/new.txt"), 1).is_err());
        assert!(area.check(&dir.path().join("a/other.txt"), 1).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn no_write_goes_through_a_link() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("secret.txt");
        std::fs::write(&target, b"keep").unwrap();
        std::os::unix::fs::symlink(&target, dir.path().join("link.txt")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("made.txt"), dir.path().join("dangling.txt")).unwrap();
        // A call that may not replace refuses both, a dangling link too.
        let agent = Area::new(dir.path(), None, false);
        assert!(agent.write(&dir.path().join("link.txt"), b"x").is_err());
        assert!(agent.write(&dir.path().join("dangling.txt"), b"x").is_err());
        assert!(!outside.path().join("made.txt").exists(), "nothing written through the dangling link");
        // A call that may replace replaces the link itself, never its target.
        let app = Area::new(dir.path(), None, true);
        app.write(&dir.path().join("link.txt"), b"new").unwrap();
        app.write(&dir.path().join("dangling.txt"), b"new").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"keep");
        assert!(!outside.path().join("made.txt").exists());
        assert!(!std::fs::symlink_metadata(dir.path().join("link.txt")).unwrap().file_type().is_symlink());
    }

    #[test]
    fn a_call_that_may_replace_replaces_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let area = Area::new(dir.path(), None, true);
        std::fs::write(dir.path().join("out.txt"), b"old").unwrap();
        area.write(&dir.path().join("out.txt"), b"new").unwrap();
        assert_eq!(std::fs::read(dir.path().join("out.txt")).unwrap(), b"new");
        let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(names, ["out.txt"], "no temporary file is left");
    }

    #[test]
    fn writes_share_one_quota_and_a_replacement_frees_what_it_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let agent = Area::new(dir.path(), Some(10), false);
        agent.write(&dir.path().join("a"), b"123456").unwrap();
        assert_eq!(agent.room(), Some(4));
        let refused = agent.write(&dir.path().join("b"), b"12345").unwrap_err();
        assert!(refused.contains("more than the 4 bytes left"), "{refused}");
        assert!(!dir.path().join("b").exists(), "nothing written over the quota");
        agent.write(&dir.path().join("c"), b"1234").unwrap();
        assert_eq!(agent.room(), Some(0));
        // A replacement counts only what it adds.
        let app = Area::new(dir.path(), Some(2), true);
        app.write(&dir.path().join("a"), b"12345678").unwrap();
        assert_eq!(app.room(), Some(0));
        assert!(app.write(&dir.path().join("d"), b"1").is_err());
        assert_eq!(Area::new(dir.path(), None, false).room(), None);
    }

    #[test]
    fn a_stage_commits_all_or_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let area = Area::new(dir.path(), None, false);
        std::fs::write(dir.path().join("taken.pdf"), b"theirs").unwrap();
        {
            let stage = area.stage().unwrap();
            assert!(stage.dir().starts_with(dir.path()) && stage.dir().is_dir());
            std::fs::write(stage.path("one.pdf"), b"1").unwrap();
            std::fs::create_dir(stage.path("sub")).unwrap();
            std::fs::write(stage.path("sub/two.pdf"), b"22").unwrap();
            assert_eq!(stage.files(), [PathBuf::from("one.pdf"), PathBuf::from("sub/two.pdf")]);
            // One destination taken: nothing moves.
            let refused = stage
                .commit(&[(stage.path("one.pdf"), dir.path().join("out/one.pdf")), (stage.path("sub/two.pdf"), dir.path().join("taken.pdf"))])
                .unwrap_err();
            assert!(refused.contains("`taken.pdf` already exists"), "{refused}");
            assert!(!dir.path().join("out/one.pdf").exists());
            assert_eq!(std::fs::read(dir.path().join("taken.pdf")).unwrap(), b"theirs");
            // Two results on one name are refused too.
            assert!(stage.commit(&[(stage.path("one.pdf"), dir.path().join("x.pdf")), (stage.path("sub/two.pdf"), dir.path().join("x.pdf"))]).is_err());
            assert!(stage.commit(&[(stage.path("none.pdf"), dir.path().join("y.pdf"))]).is_err(), "a file the engine never wrote");
            stage
                .commit(&[(stage.path("one.pdf"), dir.path().join("out/one.pdf")), (stage.path("sub/two.pdf"), dir.path().join("out/two.pdf"))])
                .unwrap();
            assert_eq!(std::fs::read(dir.path().join("out/two.pdf")).unwrap(), b"22");
        }
        let left: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(!left.iter().any(|n| n.starts_with(STAGING_PREFIX)), "the staging folder goes with the stage: {left:?}");
    }

    #[test]
    fn a_stage_keeps_to_the_quota_and_replaces_only_when_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let tight = Area::new(dir.path(), Some(3), false);
        let stage = tight.stage().unwrap();
        std::fs::write(stage.path("a"), b"12").unwrap();
        std::fs::write(stage.path("b"), b"34").unwrap();
        let refused = stage.commit(&[(stage.path("a"), dir.path().join("a")), (stage.path("b"), dir.path().join("b"))]).unwrap_err();
        assert!(refused.contains("4 bytes, more than the 3 bytes left"), "{refused}");
        assert!(!dir.path().join("a").exists() && !dir.path().join("b").exists());
        stage.commit(&[(stage.path("a"), dir.path().join("a"))]).unwrap();
        assert_eq!(tight.room(), Some(1));
        drop(stage);
        let app = Area::new(dir.path(), Some(0), true);
        let stage = app.stage().unwrap();
        std::fs::write(stage.path("a"), b"zz").unwrap();
        stage.commit(&[(stage.path("a"), dir.path().join("a"))]).unwrap();
        assert_eq!(std::fs::read(dir.path().join("a")).unwrap(), b"zz", "replaced within what it freed");
    }

    #[test]
    fn a_stale_staging_folder_from_another_process_is_swept() {
        let dir = tempfile::tempdir().unwrap();
        let stale = dir.path().join(format!("{STAGING_PREFIX}999999999-1"));
        std::fs::create_dir(&stale).unwrap();
        std::fs::write(stale.join("half.mp4"), b"partial").unwrap();
        let area = Area::new(dir.path(), None, false);
        let stage = area.stage().unwrap();
        assert!(!stale.exists(), "swept");
        assert!(stage.dir().exists(), "its own stays while it lives");
    }

    #[test]
    fn messages_name_paths_relative_to_the_area() {
        let area = Area::new("/host/apps/os.notes", None, false);
        assert_eq!(area.shown(Path::new("/host/apps/os.notes/docs/a.docx")), "docs/a.docx");
        assert_eq!(area.shown(Path::new("/elsewhere/b.docx")), "b.docx");
    }

    #[test]
    fn answers_and_errors_never_spell_the_host_path_of_the_area() {
        let area = Area::new("/host/apps/os.notes", None, false);
        let answer = serde_json::json!({
            "document": {"path": "/host/apps/os.notes/docs/a.pdf", "pages": 2},
            "parts": ["/host/apps/os.notes/parts/1.pdf", "/host/apps/os.notes"],
            "note": "saved /host/apps/os.notes/out.png and /host/apps/os.notes\\win.png",
            "sibling": "/host/apps/os.notes2/x.pdf",
            "elsewhere": "/tmp/x.pdf",
        });
        assert_eq!(
            area.relative_json(answer),
            serde_json::json!({
                "document": {"path": "docs/a.pdf", "pages": 2},
                "parts": ["parts/1.pdf", "."],
                "note": "saved out.png and win.png",
                "sibling": "/host/apps/os.notes2/x.pdf",
                "elsewhere": "/tmp/x.pdf",
            })
        );
        assert_eq!(area.relative_text("cannot read /host/apps/os.notes/a.pdf"), "cannot read a.pdf");
    }

    #[test]
    fn the_resolved_spelling_of_the_root_is_taken_out_too() {
        let dir = tempfile::tempdir().unwrap();
        let resolved = dir.path().canonicalize().unwrap();
        let area = Area::new(dir.path(), None, false);
        let text = format!("{}/a.pdf and {}/b.pdf", dir.path().display(), resolved.display());
        assert_eq!(area.relative_text(&text), "a.pdf and b.pdf");
    }
}
