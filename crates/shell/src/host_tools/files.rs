//! The host read tools (ADR 0004 §11): `files.list`, `files.read` and
//! `files.search`, registered on every app peer whose agent has a workspace
//! and executed by the shell over the calling account's folder.
//!
//! A request context is fenced by octos to its own `contexts/<id>/` and
//! refused the account folder itself; these tools are how a context turn
//! reads its account's data. They see the account folder, never outside it
//! (no absolute paths, no `..`, symbolic links neither followed nor listed)
//! and never another context's folder: under `contexts/` only the calling
//! context's own. The app's own agent (its peer session) reads the folder
//! directly and may use them too.
//!
//! Declared as the calling app's own tools (`risk: read`, no confirmation),
//! run by the relay's host executor ([`super::relay::HOST_EXECUTOR`]).
//! Narrowing by an app's per-client grants (a Rinx mini app seeing only its
//! own rooms' exports) waits for a manifest field to declare them.

use std::path::{Component, Path, PathBuf};

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
/// `offset`).
pub const MAX_READ_BYTES: usize = 128 * 1024;
/// `files.search` skips files larger than this.
pub const MAX_SEARCH_FILE_BYTES: u64 = 1024 * 1024;
/// How deep `files.list` recurses and `files.search` walks.
pub const MAX_DEPTH: usize = 12;

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
    pub root: &'a Path,
    /// The request context the call came from (`None`: the peer's own
    /// session), whose own folder under `contexts/` it may read.
    pub context: Option<&'a str>,
}

impl Scope<'_> {
    /// Whether `rel` (relative, normalised) is visible: not another
    /// context's folder.
    fn visible(&self, rel: &Path) -> bool {
        let mut parts = rel.components();
        match parts.next() {
            Some(Component::Normal(first)) if first == CONTEXTS_DIR => match parts.next() {
                None => true,
                Some(Component::Normal(id)) => self.context.is_some_and(|c| id == c),
                _ => false,
            },
            _ => true,
        }
    }

    /// `path` (relative to the account folder) as a real path inside it.
    fn resolve(&self, path: Option<&str>) -> Result<(PathBuf, PathBuf), String> {
        let path = path.unwrap_or("").trim();
        let rel = Path::new(path);
        if rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir)) {
            return Err(format!("{path}: use a path inside your data folder, without `..`"));
        }
        let rel: PathBuf = rel.components().filter(|c| matches!(c, Component::Normal(_))).collect();
        if !self.visible(&rel) {
            return Err(format!("{}: another conversation's folder", rel.display()));
        }
        // No symbolic link on the way: nothing outside the folder is reached.
        let mut at = self.root.to_path_buf();
        for part in rel.components() {
            at.push(part);
            match std::fs::symlink_metadata(&at) {
                Ok(m) if m.file_type().is_symlink() => return Err(format!("{}: a link, not followed", rel.display())),
                Ok(_) => {}
                Err(_) => return Err(format!("{}: no such file or folder", rel.display())),
            }
        }
        Ok((at, rel))
    }

    /// Everything visible under `dir` (`rel` its relative path), files and
    /// folders, links skipped, at most `limit`, `depth` levels.
    fn walk(&self, dir: &Path, rel: &Path, depth: usize, limit: usize, out: &mut Vec<(PathBuf, std::fs::Metadata)>) -> bool {
        let Ok(read) = std::fs::read_dir(dir) else { return false };
        let mut entries: Vec<_> = read.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let Ok(meta) = entry.path().symlink_metadata() else { continue };
            if meta.file_type().is_symlink() {
                continue;
            }
            let child_rel = rel.join(entry.file_name());
            if !self.visible(&child_rel) {
                continue;
            }
            if out.len() >= limit {
                return true;
            }
            let is_dir = meta.is_dir();
            out.push((child_rel.clone(), meta));
            if is_dir && depth > 1 && self.walk(&entry.path(), &child_rel, depth - 1, limit, out) {
                return true;
            }
        }
        false
    }
}

fn entry_json(rel: &Path, meta: &std::fs::Metadata) -> Value {
    let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs());
    json!({
        "path": rel.to_string_lossy().replace('\\', "/"),
        "kind": if meta.is_dir() { "folder" } else { "file" },
        "size": if meta.is_dir() { Value::Null } else { json!(meta.len()) },
        "modified": modified,
    })
}

/// `files.list`.
pub fn list(scope: &Scope, args: &Value) -> Result<Value, (String, String)> {
    let (dir, rel) = scope.resolve(args["path"].as_str()).map_err(|e| ("not_found".to_string(), e))?;
    if !dir.is_dir() {
        return Err(("invalid_args".into(), format!("{}: not a folder", rel.display())));
    }
    let depth = if args["recursive"] == true { MAX_DEPTH } else { 1 };
    let mut out = Vec::new();
    let truncated = scope.walk(&dir, &rel, depth, MAX_ENTRIES, &mut out);
    Ok(json!({
        "path": rel.to_string_lossy(),
        "entries": out.iter().map(|(r, m)| entry_json(r, m)).collect::<Vec<_>>(),
        "truncated": truncated,
    }))
}

/// `files.read`.
pub fn read(scope: &Scope, args: &Value) -> Result<Value, (String, String)> {
    use std::io::{Read, Seek, SeekFrom};
    let (file, rel) = scope.resolve(args["path"].as_str()).map_err(|e| ("not_found".to_string(), e))?;
    if !file.is_file() {
        return Err(("invalid_args".into(), format!("{}: not a file", rel.display())));
    }
    let size = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
    let offset = args["offset"].as_u64().unwrap_or(0).min(size);
    let mut f = std::fs::File::open(&file).map_err(|e| ("app_error".to_string(), format!("{}: {e}", rel.display())))?;
    f.seek(SeekFrom::Start(offset)).map_err(|e| ("app_error".to_string(), e.to_string()))?;
    let mut bytes = Vec::new();
    f.take(MAX_READ_BYTES as u64).read_to_end(&mut bytes).map_err(|e| ("app_error".to_string(), e.to_string()))?;
    // Never split a character: stop before an incomplete one at the end.
    let text = match std::str::from_utf8(&bytes) {
        Ok(t) => t.to_string(),
        Err(e) if e.error_len().is_none() => {
            bytes.truncate(e.valid_up_to());
            String::from_utf8(bytes.clone()).unwrap_or_default()
        }
        Err(_) => return Err(("binary_file".into(), format!("{}: not a text file", rel.display()))),
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

/// `files.search`.
pub fn search(scope: &Scope, args: &Value) -> Result<Value, (String, String)> {
    let query = args["query"].as_str().unwrap_or("").to_lowercase();
    if query.is_empty() {
        return Err(("invalid_args".into(), "an empty query".into()));
    }
    let (at, rel) = scope.resolve(args["path"].as_str()).map_err(|e| ("not_found".to_string(), e))?;
    let mut files: Vec<(PathBuf, std::fs::Metadata)> = Vec::new();
    if at.is_file() {
        if let Ok(meta) = std::fs::metadata(&at) {
            files.push((rel.clone(), meta));
        }
    } else {
        scope.walk(&at, &rel, MAX_DEPTH, 20 * MAX_ENTRIES, &mut files);
    }
    let mut matches = Vec::new();
    let mut truncated = false;
    'files: for (file_rel, meta) in files.iter().filter(|(_, m)| m.is_file() && m.len() <= MAX_SEARCH_FILE_BYTES) {
        let Ok(text) = std::fs::read_to_string(scope.root.join(file_rel)) else { continue };
        for (n, line) in text.lines().enumerate() {
            if line.to_lowercase().contains(&query) {
                if matches.len() >= MAX_MATCHES {
                    truncated = true;
                    break 'files;
                }
                let shown: String = line.chars().take(300).collect();
                matches.push(json!({"path": file_rel.to_string_lossy().replace('\\', "/"), "line": n + 1, "text": shown}));
            }
        }
        let _ = meta;
    }
    Ok(json!({"query": args["query"], "matches": matches, "truncated": truncated}))
}

/// Run one of the tools.
pub fn run(tool: &str, scope: &Scope, args: &Value) -> Result<Value, (String, String)> {
    match tool {
        LIST => list(scope, args),
        READ => read(scope, args),
        SEARCH => search(scope, args),
        other => Err(("not_granted".into(), format!("{other} is not a host read tool"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let all = list(&scope, &json!({"recursive": true})).unwrap();
        let seen = paths(&all, "entries");
        assert!(seen.contains(&"notes/today.md".to_string()));
        assert!(seen.contains(&"contexts/c1/mine.txt".to_string()));
        assert!(!seen.iter().any(|p| p.starts_with("contexts/c2")), "never another context's folder: {seen:?}");
        assert!(list(&scope, &json!({"path": "contexts/c2"})).is_err());
        // The peer's own session: no context folder at all.
        let own = Scope { root: &root, context: None };
        let seen = paths(&list(&own, &json!({"recursive": true})).unwrap(), "entries");
        assert!(!seen.iter().any(|p| p.starts_with("contexts/")), "{seen:?}");
    }

    #[test]
    fn nothing_outside_the_account_folder_is_reached() {
        let root = folder("escape");
        let scope = Scope { root: &root, context: Some("c1") };
        for path in ["../x", "/etc/passwd", "notes/../../x", "contexts/c2/theirs.txt"] {
            assert!(read(&scope, &json!({"path": path})).is_err(), "{path}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc", root.join("notes/etc")).unwrap();
            assert!(read(&scope, &json!({"path": "notes/etc/hosts"})).is_err(), "links are not followed");
            let seen = paths(&list(&scope, &json!({"path": "notes"})).unwrap(), "entries");
            assert_eq!(seen, ["notes/today.md"], "links are not listed");
        }
    }

    #[test]
    fn a_file_is_read_in_pieces_and_searched_by_line() {
        let root = folder("read");
        let scope = Scope { root: &root, context: Some("c1") };
        let got = read(&scope, &json!({"path": "notes/today.md"})).unwrap();
        assert_eq!(got["content"], "Buy milk\nCall Ana about the Budget\n");
        assert_eq!(got["truncated"], false);
        let tail = read(&scope, &json!({"path": "notes/today.md", "offset": 9})).unwrap();
        assert_eq!(tail["content"], "Call Ana about the Budget\n");
        let big = "x".repeat(MAX_READ_BYTES + 10);
        std::fs::write(root.join("big.txt"), &big).unwrap();
        let first = read(&scope, &json!({"path": "big.txt"})).unwrap();
        assert_eq!((first["truncated"].as_bool(), first["next_offset"].as_u64()), (Some(true), Some(MAX_READ_BYTES as u64)));
        std::fs::write(root.join("blob.bin"), [0xff, 0xfe, 0x00, 0x80]).unwrap();
        assert_eq!(read(&scope, &json!({"path": "blob.bin"})).unwrap_err().0, "binary_file");
        let found = search(&scope, &json!({"query": "BUDGET"})).unwrap();
        let hits: Vec<(String, u64)> = found["matches"].as_array().unwrap().iter().map(|m| (m["path"].as_str().unwrap().to_string(), m["line"].as_u64().unwrap())).collect();
        assert_eq!(hits, [("contexts/c1/mine.txt".to_string(), 1), ("notes/today.md".to_string(), 2)], "never c2's");
    }
}
