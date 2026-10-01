//! The host read tools (ADR 0004 §11): `files.list`, `files.read` and
//! `files.search`, registered on every consented app peer whose agent has a
//! workspace and executed by the shell over the calling account's folder.
//!
//! A request context is fenced by octos to its own `contexts/<id>/` (octos
//! creates it as `<peer folder>/contexts/<kernel context id>`, the id the
//! host stamps on the call) and refused the account folder itself; these
//! tools are how a context turn reads its account's data. The app's own
//! agent (its peer session) reads the folder directly and may use them too.
//!
//! **The fence.** The account folder is writable by the app, which may be
//! sandboxed, while the shell is not: nothing here may be steered outside
//! the folder. So every path is walked one component at a time from a
//! directory descriptor anchored at the account folder, each component
//! opened with `O_NOFOLLOW` (a symbolic link anywhere fails the call; there
//! is no check-then-open window to swap one in), and FIFOs, sockets and
//! devices are refused. Another context's folder is recognised by identity
//! (device and inode of the real `contexts` folder and of the caller's own
//! context folder), never by spelling, so `Contexts/c2` on a case-insensitive
//! file system is refused too. The peer's own session sees no context
//! folder.
//!
//! **Budgets.** Reads return at most [`MAX_READ_BYTES`] per call; listings
//! [`MAX_ENTRIES`]; searches read at most [`MAX_SEARCH_FILES`] files and
//! [`MAX_SEARCH_TOTAL_BYTES`] in all, skipping files over
//! [`MAX_SEARCH_FILE_BYTES`]; at most [`MAX_IN_FLIGHT`] calls run at once
//! (more are refused `busy`).
//!
//! Unix only: elsewhere the tools are not declared (the Windows sandbox is
//! not built yet, #137). Narrowing by an app's per-client grants (a Rinx mini
//! app seeing only its own rooms' exports) waits for a manifest field to
//! declare them.

use serde_json::{json, Value};

/// The tools.
pub const LIST: &str = "files.list";
pub const READ: &str = "files.read";
pub const SEARCH: &str = "files.search";
pub const TOOLS: &[&str] = &[LIST, READ, SEARCH];

/// Octos's request contexts' folders, inside the account folder.
pub const CONTEXTS_DIR: &str = "contexts";
/// At most this many entries or matches per call.
pub const MAX_ENTRIES: usize = 500;
pub const MAX_MATCHES: usize = 100;
/// `files.read` returns at most this many bytes per call (continue with
/// `next_offset`).
pub const MAX_READ_BYTES: usize = 128 * 1024;
/// `files.search` skips files larger than this, reads at most this many
/// files and this many bytes in all.
pub const MAX_SEARCH_FILE_BYTES: u64 = 1024 * 1024;
pub const MAX_SEARCH_FILES: usize = 2000;
pub const MAX_SEARCH_TOTAL_BYTES: u64 = 32 * 1024 * 1024;
/// How deep `files.list` recurses and `files.search` walks.
pub const MAX_DEPTH: usize = 12;
/// At most this many calls run at once, across every app.
pub const MAX_IN_FLIGHT: usize = 8;

/// Whether this platform runs the tools (see the module docs).
pub const SUPPORTED: bool = cfg!(unix);

/// The three declarations, as `app`'s own tools.
pub fn declarations(app: &str) -> Vec<Value> {
    vec![
        json!({
            "name": LIST,
            "app": app,
            "description": "List files and folders in your account's data folder (the app's files for this account). `path` is relative to that folder (default: its top); `recursive` lists everything below it.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "maxLength": 1024},
                    "recursive": {"type": "boolean"}
                },
                "additionalProperties": false
            },
            "risk": "read",
            "shareable": false
        }),
        json!({
            "name": READ,
            "app": app,
            "description": "Read a text file in your account's data folder. `path` is relative to that folder. Returns at most 128 KiB from `offset` (bytes); continue with `next_offset` when `truncated`.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "minLength": 1, "maxLength": 1024},
                    "offset": {"type": "integer", "minimum": 0}
                },
                "required": ["path"],
                "additionalProperties": false
            },
            "risk": "read",
            "shareable": false
        }),
        json!({
            "name": SEARCH,
            "app": app,
            "description": "Search the text files in your account's data folder for lines containing `query` (case-insensitive). `path` narrows it to a folder or file, relative to the data folder.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "minLength": 1, "maxLength": 256},
                    "path": {"type": "string", "maxLength": 1024}
                },
                "required": ["query"],
                "additionalProperties": false
            },
            "risk": "read",
            "shareable": false
        }),
    ]
}

/// The folder a call reads: the account folder, and the calling context.
pub struct Scope<'a> {
    pub root: &'a std::path::Path,
    /// The request context the call came from (`None`: the peer's own
    /// session), whose own folder under `contexts/` it may read.
    pub context: Option<&'a str>,
}

type Failure = (String, String);

fn fail(kind: &str, message: impl Into<String>) -> Failure {
    (kind.to_string(), message.into())
}

/// Run one of the tools (at most [`MAX_IN_FLIGHT`] at once).
pub fn run(tool: &str, scope: &Scope, args: &Value) -> Result<Value, Failure> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
    if IN_FLIGHT.fetch_add(1, Ordering::SeqCst) >= MAX_IN_FLIGHT {
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
        return Err(fail("busy", "too many file reads at once; try again"));
    }
    struct Release<'a>(&'a AtomicUsize);
    impl Drop for Release<'_> {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    let _release = Release(&IN_FLIGHT);
    #[cfg(unix)]
    {
        imp::run(tool, scope, args)
    }
    #[cfg(not(unix))]
    {
        let _ = (tool, scope, args);
        Err(fail("unsupported", "the host read tools run on macOS, Linux and Android only"))
    }
}

#[cfg(unix)]
mod imp {
    use super::*;
    use std::ffi::{CString, OsStr, OsString};
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::{Component, Path, PathBuf};

    type Id = (u64, u64);

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Dir,
        File,
        /// A link, FIFO, socket or device: never opened.
        Other,
    }

    struct Stat {
        id: Id,
        kind: Kind,
        size: u64,
        modified: i64,
    }

    fn stat_of(st: &libc::stat) -> Stat {
        let kind = match st.st_mode & libc::S_IFMT {
            libc::S_IFDIR => Kind::Dir,
            libc::S_IFREG => Kind::File,
            _ => Kind::Other,
        };
        Stat { id: (st.st_dev as u64, st.st_ino as u64), kind, size: st.st_size as u64, modified: st.st_mtime as i64 }
    }

    fn cstr(name: &OsStr) -> std::io::Result<CString> {
        CString::new(name.as_bytes()).map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))
    }

    /// A directory held open; everything below it is opened relative to it.
    struct Dir(OwnedFd);

    impl Dir {
        /// The account folder itself, not through a link.
        fn open_root(path: &Path) -> std::io::Result<Dir> {
            let c = cstr(path.as_os_str())?;
            let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC) };
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(Dir(unsafe { OwnedFd::from_raw_fd(fd) }))
        }
        fn openat(&self, name: &OsStr, flags: libc::c_int) -> std::io::Result<OwnedFd> {
            let c = cstr(name)?;
            let fd = unsafe { libc::openat(self.0.as_raw_fd(), c.as_ptr(), flags | libc::O_NOFOLLOW | libc::O_CLOEXEC) };
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(unsafe { OwnedFd::from_raw_fd(fd) })
        }
        /// A child folder, never through a link.
        fn dir(&self, name: &OsStr) -> std::io::Result<Dir> {
            self.openat(name, libc::O_RDONLY | libc::O_DIRECTORY).map(Dir)
        }
        /// A child regular file, never through a link, never a FIFO.
        fn file(&self, name: &OsStr) -> std::io::Result<(File, Stat)> {
            let fd = self.openat(name, libc::O_RDONLY | libc::O_NONBLOCK)?;
            let st = fstat(fd.as_raw_fd())?;
            if st.kind != Kind::File {
                return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
            }
            Ok((File::from(fd), st))
        }
        fn stat(&self) -> std::io::Result<Stat> {
            fstat(self.0.as_raw_fd())
        }
        fn stat_at(&self, name: &OsStr) -> std::io::Result<Stat> {
            let c = cstr(name)?;
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            if unsafe { libc::fstatat(self.0.as_raw_fd(), c.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(stat_of(&st))
        }
        /// The names in this folder, sorted (`.` and `..` left out).
        fn names(&self) -> std::io::Result<Vec<OsString>> {
            let fd = unsafe { libc::dup(self.0.as_raw_fd()) };
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let dir = unsafe { libc::fdopendir(fd) };
            if dir.is_null() {
                unsafe { libc::close(fd) };
                return Err(std::io::Error::last_os_error());
            }
            unsafe { libc::rewinddir(dir) };
            let mut out = Vec::new();
            loop {
                let entry = unsafe { libc::readdir(dir) };
                if entry.is_null() {
                    break;
                }
                let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes().to_vec();
                if name != b"." && name != b".." {
                    out.push(OsString::from_vec(name));
                }
            }
            unsafe { libc::closedir(dir) };
            out.sort();
            Ok(out)
        }
    }

    fn fstat(fd: libc::c_int) -> std::io::Result<Stat> {
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstat(fd, &mut st) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(stat_of(&st))
    }

    /// The account folder, open, with the identities that fence contexts.
    struct Fence {
        root: Dir,
        /// The real `contexts` folder.
        contexts: Option<Id>,
        /// The caller's own context folder in it.
        own: Option<Id>,
    }

    impl Fence {
        fn open(scope: &Scope) -> Result<Fence, Failure> {
            let root = Dir::open_root(scope.root).map_err(|e| fail("no_workspace", format!("the data folder cannot be opened: {e}")))?;
            let contexts_dir = root.dir(OsStr::new(CONTEXTS_DIR)).ok();
            let contexts = contexts_dir.as_ref().and_then(|d| d.stat().ok()).map(|s| s.id);
            let own = match (scope.context, &contexts_dir) {
                (Some(id), Some(dir)) if !id.is_empty() && !id.contains('/') => dir.dir(OsStr::new(id)).ok().and_then(|d| d.stat().ok()).map(|s| s.id),
                _ => None,
            };
            Ok(Fence { root, contexts, own })
        }
        /// Whether folder `child` (in folder `parent`) may be entered.
        fn allows(&self, parent: Id, child: Id) -> bool {
            Some(parent) != self.contexts || Some(child) == self.own
        }
    }

    enum Target {
        Dir(Dir, Id),
        File(File, Stat),
    }

    /// Walk `path` from the account folder, one component at a time.
    fn resolve(fence: &Fence, path: Option<&str>) -> Result<(Target, PathBuf), Failure> {
        let path = path.unwrap_or("").trim();
        let rel = Path::new(path);
        if rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir)) {
            return Err(fail("invalid_args", format!("{path}: use a path inside your data folder, without `..`")));
        }
        let parts: Vec<&OsStr> = rel.components().filter_map(|c| if let Component::Normal(p) = c { Some(p) } else { None }).collect();
        let shown: PathBuf = parts.iter().collect();
        let not_found = || fail("not_found", format!("{}: no such file or folder (links are not followed)", shown.display()));
        let mut dir = Dir(fence.root.0.try_clone().map_err(|e| fail("app_error", e.to_string()))?);
        let mut id = dir.stat().map_err(|e| fail("app_error", e.to_string()))?.id;
        for (i, part) in parts.iter().enumerate() {
            let last = i + 1 == parts.len();
            match dir.dir(part) {
                Ok(next) => {
                    let next_id = next.stat().map_err(|e| fail("app_error", e.to_string()))?.id;
                    if !fence.allows(id, next_id) {
                        return Err(fail("not_found", format!("{}: another conversation's folder", shown.display())));
                    }
                    dir = next;
                    id = next_id;
                }
                Err(_) if last => {
                    let (file, st) = dir.file(part).map_err(|_| not_found())?;
                    return Ok((Target::File(file, st), shown));
                }
                Err(_) => return Err(not_found()),
            }
        }
        Ok((Target::Dir(dir, id), shown))
    }

    fn entry_json(rel: &Path, st: &Stat) -> Value {
        json!({
            "path": rel.to_string_lossy(),
            "kind": if st.kind == Kind::Dir { "folder" } else { "file" },
            "size": if st.kind == Kind::Dir { Value::Null } else { json!(st.size) },
            "modified": st.modified,
        })
    }

    /// Visit every visible folder and regular file below `dir` (links and
    /// other kinds skipped), depth first in name order; `visit` returns
    /// false to stop. True when stopped.
    fn walk(fence: &Fence, dir: &Dir, id: Id, rel: &Path, depth: usize, visit: &mut dyn FnMut(&Dir, &OsStr, &Path, &Stat) -> bool) -> bool {
        let Ok(names) = dir.names() else { return false };
        for name in names {
            let Ok(st) = dir.stat_at(&name) else { continue };
            let child_rel = rel.join(&name);
            match st.kind {
                Kind::Other => continue,
                Kind::File => {
                    if !visit(dir, &name, &child_rel, &st) {
                        return true;
                    }
                }
                Kind::Dir => {
                    let Ok(child) = dir.dir(&name) else { continue };
                    let Ok(child_st) = child.stat() else { continue };
                    if !fence.allows(id, child_st.id) {
                        continue;
                    }
                    if !visit(dir, &name, &child_rel, &child_st) {
                        return true;
                    }
                    if depth > 1 && walk(fence, &child, child_st.id, &child_rel, depth - 1, visit) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn list(fence: &Fence, args: &Value) -> Result<Value, Failure> {
        let (target, rel) = resolve(fence, args["path"].as_str())?;
        let Target::Dir(dir, id) = target else {
            return Err(fail("invalid_args", format!("{}: not a folder", rel.display())));
        };
        let depth = if args["recursive"] == true { MAX_DEPTH } else { 1 };
        let mut out = Vec::new();
        let truncated = walk(fence, &dir, id, &rel, depth, &mut |_, _, r, st| {
            if out.len() >= MAX_ENTRIES {
                return false;
            }
            out.push(entry_json(r, st));
            true
        });
        Ok(json!({"path": rel.to_string_lossy(), "entries": out, "truncated": truncated}))
    }

    fn read(fence: &Fence, args: &Value) -> Result<Value, Failure> {
        let (target, rel) = resolve(fence, args["path"].as_str())?;
        let Target::File(mut file, st) = target else {
            return Err(fail("invalid_args", format!("{}: not a file", rel.display())));
        };
        let size = st.size;
        let mut offset = args["offset"].as_u64().unwrap_or(0).min(size);
        file.seek(SeekFrom::Start(offset)).map_err(|e| fail("app_error", e.to_string()))?;
        let mut bytes = Vec::new();
        file.take(MAX_READ_BYTES as u64 + 3).read_to_end(&mut bytes).map_err(|e| fail("app_error", e.to_string()))?;
        // An offset inside a character: start at the next one.
        let skip = bytes.iter().take(3).take_while(|b| (**b & 0xC0) == 0x80).count();
        bytes.drain(..skip);
        offset += skip as u64;
        bytes.truncate(MAX_READ_BYTES);
        // Never split a character at the end either.
        let text = match std::str::from_utf8(&bytes) {
            Ok(t) => t.to_string(),
            Err(e) if e.error_len().is_none() => String::from_utf8_lossy(&bytes[..e.valid_up_to()]).into_owned(),
            Err(_) => return Err(fail("binary_file", format!("{}: not a text file", rel.display()))),
        };
        let next = offset + text.len() as u64;
        Ok(json!({
            "path": rel.to_string_lossy(),
            "size": size,
            "offset": offset,
            "content": text,
            "truncated": next < size,
            "next_offset": if next < size { json!(next) } else { Value::Null },
        }))
    }

    fn search(fence: &Fence, args: &Value) -> Result<Value, Failure> {
        let query = args["query"].as_str().unwrap_or("").to_lowercase();
        if query.is_empty() {
            return Err(fail("invalid_args", "an empty query"));
        }
        let (target, rel) = resolve(fence, args["path"].as_str())?;
        let mut matches = Vec::new();
        let (mut files, mut bytes, mut truncated) = (0usize, 0u64, false);
        let mut scan = |file: File, st: &Stat, r: &Path, matches: &mut Vec<Value>| -> bool {
            if st.size > MAX_SEARCH_FILE_BYTES {
                return true;
            }
            if files >= MAX_SEARCH_FILES || bytes + st.size > MAX_SEARCH_TOTAL_BYTES {
                truncated = true;
                return false;
            }
            files += 1;
            bytes += st.size;
            let mut text = String::new();
            if file.take(MAX_SEARCH_FILE_BYTES).read_to_string(&mut text).is_err() {
                return true;
            }
            for (n, line) in text.lines().enumerate() {
                if line.to_lowercase().contains(&query) {
                    if matches.len() >= MAX_MATCHES {
                        truncated = true;
                        return false;
                    }
                    let shown: String = line.chars().take(300).collect();
                    matches.push(json!({"path": r.to_string_lossy(), "line": n + 1, "text": shown}));
                }
            }
            true
        };
        match target {
            Target::File(file, st) => {
                scan(file, &st, &rel, &mut matches);
            }
            Target::Dir(dir, id) => {
                walk(fence, &dir, id, &rel, MAX_DEPTH, &mut |parent, name, r, st| {
                    if st.kind != Kind::File {
                        return true;
                    }
                    match parent.file(name) {
                        Ok((file, st)) => scan(file, &st, r, &mut matches),
                        Err(_) => true,
                    }
                });
            }
        }
        Ok(json!({"query": args["query"], "matches": matches, "truncated": truncated}))
    }

    pub fn run(tool: &str, scope: &Scope, args: &Value) -> Result<Value, Failure> {
        let fence = Fence::open(scope)?;
        match tool {
            LIST => list(&fence, args),
            READ => read(&fence, args),
            SEARCH => search(&fence, args),
            other => Err(fail("not_granted", format!("{other} is not a host read tool"))),
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn folder(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("octosense-files-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("notes")).unwrap();
        std::fs::create_dir_all(dir.join("contexts/c1")).unwrap();
        std::fs::create_dir_all(dir.join("contexts/c2")).unwrap();
        std::fs::write(dir.join("notes/today.md"), "Buy milk\nCall Ana about the Budget\n").unwrap();
        std::fs::write(dir.join("contexts/c1/mine.txt"), "budget in c1\n").unwrap();
        std::fs::write(dir.join("contexts/c2/theirs.txt"), "budget in c2\n").unwrap();
        dir
    }

    fn paths(v: &Value, key: &str) -> Vec<String> {
        v[key].as_array().unwrap().iter().map(|e| e["path"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn a_context_lists_its_account_folder_and_its_own_context_only() {
        let root = folder("list");
        let scope = Scope { root: &root, context: Some("c1") };
        let all = run(LIST, &scope, &json!({"recursive": true})).unwrap();
        let seen = paths(&all, "entries");
        assert!(seen.contains(&"notes/today.md".to_string()));
        assert!(seen.contains(&"contexts/c1/mine.txt".to_string()));
        assert!(!seen.iter().any(|p| p.starts_with("contexts/c2")), "never another context's folder: {seen:?}");
        assert!(run(LIST, &scope, &json!({"path": "contexts/c2"})).is_err());
        // The peer's own session: no context folder at all.
        let own = Scope { root: &root, context: None };
        let seen = paths(&run(LIST, &own, &json!({"recursive": true})).unwrap(), "entries");
        assert!(!seen.iter().any(|p| p.starts_with("contexts/")), "{seen:?}");
    }

    /// The fence is by identity, not spelling: on a case-insensitive file
    /// system (APFS's default) `Contexts/c2` is the same folder as
    /// `contexts/c2`, and is refused like it; elsewhere it does not exist.
    #[test]
    fn another_contexts_folder_is_refused_whatever_its_spelling() {
        let root = folder("case");
        let scope = Scope { root: &root, context: Some("c1") };
        for path in ["contexts/c2/theirs.txt", "Contexts/c2/theirs.txt", "CONTEXTS/C2/theirs.txt", "contexts/C2/theirs.txt", "./contexts/c2/theirs.txt"] {
            assert!(run(READ, &scope, &json!({"path": path})).is_err(), "{path}");
        }
        for path in ["Contexts", "CONTEXTS"] {
            if let Ok(listed) = run(LIST, &scope, &json!({"path": path, "recursive": true})) {
                let seen = paths(&listed, "entries");
                assert!(!seen.iter().any(|p| p.to_lowercase().contains("c2")), "{path}: {seen:?}");
            }
        }
        let found = run(SEARCH, &scope, &json!({"query": "budget", "path": "Contexts"}));
        if let Ok(found) = found {
            assert!(!paths(&found, "matches").iter().any(|p| p.to_lowercase().contains("c2")));
        }
    }

    #[test]
    fn nothing_outside_the_account_folder_is_reached() {
        let root = folder("escape");
        let scope = Scope { root: &root, context: Some("c1") };
        for path in ["../x", "/etc/passwd", "notes/../../x", "contexts/c2/theirs.txt"] {
            assert!(run(READ, &scope, &json!({"path": path})).is_err(), "{path}");
        }
        std::os::unix::fs::symlink("/etc", root.join("notes/etc")).unwrap();
        std::os::unix::fs::symlink("/etc/hosts", root.join("notes/hosts")).unwrap();
        assert!(run(READ, &scope, &json!({"path": "notes/etc/hosts"})).is_err(), "a linked folder is not entered");
        assert!(run(READ, &scope, &json!({"path": "notes/hosts"})).is_err(), "a linked file is not opened");
        let seen = paths(&run(LIST, &scope, &json!({"path": "notes"})).unwrap(), "entries");
        assert_eq!(seen, ["notes/today.md"], "links are not listed");
        // The account folder itself swapped for a link: refused.
        let linked = std::env::temp_dir().join(format!("octosense-files-linkroot-{}", std::process::id()));
        let _ = std::fs::remove_file(&linked);
        std::os::unix::fs::symlink(&root, &linked).unwrap();
        assert!(run(LIST, &Scope { root: &linked, context: None }, &json!({})).is_err());
        // A FIFO is never opened (it would block the shell).
        let fifo = std::ffi::CString::new(root.join("notes/pipe").as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(run(READ, &scope, &json!({"path": "notes/pipe"})).is_err());
    }

    /// The app swaps a link in while the shell reads: the read either gets
    /// the real file or fails, never the link's target.
    #[test]
    fn a_link_swapped_in_during_reads_is_never_followed() {
        let root = folder("swap");
        let secret_dir = std::env::temp_dir().join(format!("octosense-files-secret-{}", std::process::id()));
        std::fs::create_dir_all(&secret_dir).unwrap();
        std::fs::write(secret_dir.join("today.md"), "SECRET budget\n").unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (r, s, sd) = (root.clone(), stop.clone(), secret_dir.clone());
        let swapper = std::thread::spawn(move || {
            let (file, dir, real) = (r.join("notes/today.md"), r.join("notes"), r.join("notes-real"));
            while !s.load(std::sync::atomic::Ordering::Relaxed) {
                // The file, then the folder above it, each swapped for a link.
                let _ = std::fs::remove_file(&file);
                let _ = std::os::unix::fs::symlink(sd.join("today.md"), &file);
                let _ = std::fs::remove_file(&file);
                let _ = std::fs::write(&file, "Buy milk\n");
                let _ = std::fs::rename(&dir, &real);
                let _ = std::os::unix::fs::symlink(&sd, &dir);
                let _ = std::fs::remove_file(&dir);
                let _ = std::fs::rename(&real, &dir);
            }
        });
        let scope = Scope { root: &root, context: None };
        for _ in 0..3000 {
            if let Ok(got) = run(READ, &scope, &json!({"path": "notes/today.md"})) {
                assert!(!got["content"].as_str().unwrap().contains("SECRET"), "read through a link");
            }
            if let Ok(found) = run(SEARCH, &scope, &json!({"query": "secret"})) {
                assert!(found["matches"].as_array().unwrap().is_empty(), "searched through a link: {found}");
            }
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        swapper.join().unwrap();
    }

    #[test]
    fn a_file_is_read_in_pieces_and_searched_by_line() {
        let root = folder("read");
        let scope = Scope { root: &root, context: Some("c1") };
        let got = run(READ, &scope, &json!({"path": "notes/today.md"})).unwrap();
        assert_eq!(got["content"], "Buy milk\nCall Ana about the Budget\n");
        assert_eq!(got["truncated"], false);
        let tail = run(READ, &scope, &json!({"path": "notes/today.md", "offset": 9})).unwrap();
        assert_eq!(tail["content"], "Call Ana about the Budget\n");
        // An offset inside a character starts at the next one.
        std::fs::write(root.join("utf8.txt"), "é中x").unwrap();
        let mid = run(READ, &scope, &json!({"path": "utf8.txt", "offset": 3})).unwrap();
        assert_eq!((mid["content"].as_str(), mid["offset"].as_u64()), (Some("x"), Some(5)));
        let big = "x".repeat(MAX_READ_BYTES + 10);
        std::fs::write(root.join("big.txt"), &big).unwrap();
        let first = run(READ, &scope, &json!({"path": "big.txt"})).unwrap();
        assert_eq!((first["truncated"].as_bool(), first["next_offset"].as_u64()), (Some(true), Some(MAX_READ_BYTES as u64)));
        std::fs::write(root.join("blob.bin"), [0xff, 0xfe, 0x00, 0x80]).unwrap();
        assert_eq!(run(READ, &scope, &json!({"path": "blob.bin"})).unwrap_err().0, "binary_file");
        let found = run(SEARCH, &scope, &json!({"query": "BUDGET"})).unwrap();
        let hits: Vec<(String, u64)> = found["matches"].as_array().unwrap().iter().map(|m| (m["path"].as_str().unwrap().to_string(), m["line"].as_u64().unwrap())).collect();
        assert_eq!(hits, [("contexts/c1/mine.txt".to_string(), 1), ("notes/today.md".to_string(), 2)], "never c2's");
    }
}
