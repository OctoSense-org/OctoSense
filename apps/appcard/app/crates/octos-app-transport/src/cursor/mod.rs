//! Per-session cursor tracking.
//!
//! `UiCursor` is the resumable consumption position for the per-session
//! ledger (see octos-core ui_protocol.rs:62). Transport keeps the last applied
//! cursor in memory; the `CursorPersist` callback lets W04 plug in SQLite
//! without changing this surface.

use std::collections::HashMap;

use octos_core::{SessionKey, ui_protocol::UiCursor};

/// In-memory `SessionKey → UiCursor`, optionally write-through to a durable
/// [`CursorPersist`] backend. A fresh store is hydrated from the backend via
/// [`Self::new_persisted`] on (re)connect, so per-session replay cursors survive
/// a transport re-spawn / process restart (W08/W04). The stdio child *is* the
/// connection with no auto-restart, so "reconnect" = a new `spawn()`; without
/// this, every reconnect started with empty cursors.
#[derive(Clone)]
pub struct CursorStore {
    inner: HashMap<SessionKey, UiCursor>,
    persist: std::sync::Arc<dyn CursorPersist>,
}

impl std::fmt::Debug for CursorStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CursorStore")
            .field("sessions", &self.inner.len())
            .finish()
    }
}

impl Default for CursorStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorStore {
    /// In-memory only (no durability) — tests and callers without a backend.
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
            persist: std::sync::Arc::new(NoopCursorPersist),
        }
    }

    /// Hydrate all sessions from `persist` and write through on every mutation.
    /// This is what makes cursors survive reconnect/restart.
    pub fn new_persisted(persist: std::sync::Arc<dyn CursorPersist>) -> Self {
        let inner = persist.load_all().unwrap_or_else(|e| {
            log::warn!("cursor: load_all failed ({e}); starting empty");
            HashMap::new()
        });
        Self { inner, persist }
    }

    pub fn get(&self, session: &SessionKey) -> Option<&UiCursor> {
        self.inner.get(session)
    }
    pub fn set(&mut self, session: SessionKey, cursor: UiCursor) {
        // Write through first; a persistence failure is logged, never fatal —
        // the in-memory cursor still advances (a later cold start just downgrades
        // to a REST rehydrate, never corruption).
        if let Err(e) = self.persist.save(&session, &cursor) {
            log::warn!("cursor: save failed for {session:?} ({e})");
        }
        self.inner.insert(session, cursor);
    }
    pub fn drop(&mut self, session: &SessionKey) {
        self.delete(session);
    }
    /// Synonym for `drop` — kept because `drop` shadows the prelude trait
    /// name in some grep contexts. Prefer this in new code.
    pub fn delete(&mut self, session: &SessionKey) {
        if let Err(e) = self.persist.forget(session) {
            log::warn!("cursor: forget failed for {session:?} ({e})");
        }
        self.inner.remove(session);
    }
    pub fn iter(&self) -> impl Iterator<Item = (&SessionKey, &UiCursor)> {
        self.inner.iter()
    }
}

/// Pluggable persistence for cursors. W04 implements this against SQLite.
/// Errors flatten to `String` to stay free of W04's error type. Transport
/// logs and continues on persistence failure — losing a cursor downgrades to
/// a REST rehydrate, never to data corruption.
pub trait CursorPersist: Send + Sync + 'static {
    fn load(&self, session: &SessionKey) -> Result<Option<UiCursor>, String>;
    fn save(&self, session: &SessionKey, cursor: &UiCursor) -> Result<(), String>;
    fn forget(&self, session: &SessionKey) -> Result<(), String>;
    /// Load the ENTIRE store at once — used to hydrate a fresh [`CursorStore`]
    /// on (re)connect. Default empty so existing impls need no change.
    fn load_all(&self) -> Result<HashMap<SessionKey, UiCursor>, String> {
        Ok(HashMap::new())
    }
}

/// No-op impl for tests and the in-memory-only path.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopCursorPersist;

impl CursorPersist for NoopCursorPersist {
    fn load(&self, _: &SessionKey) -> Result<Option<UiCursor>, String> { Ok(None) }
    fn save(&self, _: &SessionKey, _: &UiCursor) -> Result<(), String> { Ok(()) }
    fn forget(&self, _: &SessionKey) -> Result<(), String> { Ok(()) }
}

/// Durable [`CursorPersist`] backed by a single JSON file (a *list* of
/// `(SessionKey, UiCursor)` pairs — a list, not a map, so it is independent of
/// `SessionKey`'s serialized form). Whole-file rewrite per mutation (cursor
/// writes are infrequent). The lean alternative to a full SQLite backend (W04);
/// enough to make per-session cursors survive a transport re-spawn / app
/// restart. All errors are non-fatal (logged upstream).
///
/// Crash-safety (W08 Layer 1b hardening):
/// - **Atomic replace**: each write goes to a unique sibling temp file that is
///   fsync'd (`sync_all`) *before* being `rename`d over the target, and the
///   parent directory is fsync'd *after* the rename. A concurrent reader
///   therefore sees either the whole old file or the whole new one — never a
///   torn read — and a power loss can't leave the target pointing at unwritten
///   blocks or silently revert the rename. See [`FileCursorPersist::write_map`].
/// - **Corruption resilience**: [`FileCursorPersist::read_map`] never panics and
///   never hard-errors; a missing / empty / truncated / garbage /
///   checksum-mismatched / unknown-version file all recover to an empty map so a
///   bad file can't brick transport startup (worst case = a REST rehydrate).
/// - **Versioned + checksummed on disk**: the file is a `{version, checksum,
///   entries}` wrapper so a future format change or bit-rot is *detected*, not
///   silently misread. The legacy bare-`Vec` array is still read for backward
///   compat (see [`FileCursorPersist::read_map`]).
pub struct FileCursorPersist {
    path: std::path::PathBuf,
    lock: std::sync::Mutex<()>,
}

/// On-disk schema version for the cursor file. Bump when the shape of `entries`
/// changes; a reader that sees a *higher* version it doesn't understand recovers
/// to empty rather than silently misreading it.
const CURSOR_FILE_VERSION: u32 = 1;

/// Monotonic suffix for temp-file names so concurrent writers (or a writer
/// racing a crashed writer's leftover tmp) never collide on the same path.
static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Wrapping struct persisted to disk. Versioned + checksummed so a truncated
/// write, bit-rot, or a future format change is *detected* (→ recover empty)
/// instead of being silently misread. `entries` is a list (not a map) so it
/// stays independent of `SessionKey`'s serialized form and reads back in a
/// deterministic order.
#[derive(serde::Serialize, serde::Deserialize)]
struct CursorFile {
    version: u32,
    /// FNV-1a/64 of the serialized `entries` array (see [`fnv1a`]). Guards
    /// against a valid-JSON-but-corrupt file that atomic rename alone can't.
    checksum: u64,
    entries: Vec<(SessionKey, UiCursor)>,
}

/// Tiny std-only, version-stable checksum (FNV-1a, 64-bit). Deliberately NOT
/// `std::hash::DefaultHasher`: SipHash's output is not guaranteed stable across
/// toolchain releases, which would false-positive as corruption after a Rust
/// upgrade and wipe every persisted cursor. FNV-1a is fixed forever.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// fsync the directory that contains `path` so a preceding `rename` is durable
/// across a crash. On most filesystems the rename is only a metadata change in
/// the parent directory; without this fsync the file can revert to its old
/// contents (or vanish) after a power loss even though `rename(2)` returned 0.
/// Best-effort: a filesystem that refuses to fsync a directory handle must not
/// fail the whole write. Unix-only (the Android target); a no-op elsewhere.
#[cfg(unix)]
fn fsync_parent_dir(path: &std::path::Path) {
    let dir = match path.parent() {
        // An empty parent means the CWD; open "." there instead.
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => std::path::Path::new("."),
    };
    if let Ok(f) = std::fs::File::open(dir) {
        let _ = f.sync_all();
    }
}
#[cfg(not(unix))]
fn fsync_parent_dir(_path: &std::path::Path) {}

impl FileCursorPersist {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: std::sync::Mutex::new(()),
        }
    }

    /// Read the whole store. NEVER panics and NEVER hard-errors: a missing,
    /// empty, truncated, garbage, checksum-mismatched, or unknown-version file
    /// all recover to an empty map (logged) so a corrupt cursor file can never
    /// brick transport startup — the worst case is a REST rehydrate. Reads the
    /// current versioned+checksummed format first, then falls back to the legacy
    /// bare `Vec<(SessionKey, UiCursor)>` array so pre-existing
    /// `a2app-cursors.json` files still load.
    fn read_map(&self) -> HashMap<SessionKey, UiCursor> {
        let raw = match std::fs::read_to_string(&self.path) {
            Ok(s) => s,
            Err(e) => {
                // A not-yet-created file is the normal cold-start case, not an
                // error worth logging.
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("cursor: read {:?} failed ({e}); starting empty", self.path);
                }
                return HashMap::new();
            }
        };
        if raw.trim().is_empty() {
            return HashMap::new();
        }
        // Current format: versioned + checksummed wrapper.
        match serde_json::from_str::<CursorFile>(&raw) {
            Ok(file) => {
                if file.version > CURSOR_FILE_VERSION {
                    log::warn!(
                        "cursor: file {:?} version {} newer than supported {}; starting empty",
                        self.path,
                        file.version,
                        CURSOR_FILE_VERSION,
                    );
                    return HashMap::new();
                }
                // Re-serialize `entries` exactly as it was checksummed on write
                // (a Vec preserves order) and compare — catches a valid-JSON but
                // corrupt file that atomic rename alone can't detect.
                let got = fnv1a(serde_json::to_string(&file.entries).unwrap_or_default().as_bytes());
                if got != file.checksum {
                    log::warn!(
                        "cursor: file {:?} checksum mismatch (want {:#018x}, got {:#018x}); starting empty",
                        self.path,
                        file.checksum,
                        got,
                    );
                    return HashMap::new();
                }
                file.entries.into_iter().collect()
            }
            // Legacy bare-array format (pre-versioning). No checksum to verify;
            // read best-effort so existing cursor files keep working.
            Err(_) => match serde_json::from_str::<Vec<(SessionKey, UiCursor)>>(&raw) {
                Ok(v) => v.into_iter().collect(),
                Err(e) => {
                    log::warn!("cursor: corrupt file {:?} ({e}); starting empty", self.path);
                    HashMap::new()
                }
            },
        }
    }

    /// Atomically replace the file with `map`, crash-safely. Writes a unique
    /// sibling temp file, fsyncs its contents, `rename`s it over the target,
    /// then fsyncs the parent directory so the rename survives a power loss.
    /// Handles first-write (target/parent dir absent). Cleans up the temp file
    /// on any failure so a crash mid-write never leaves stray `.tmp` litter.
    fn write_map(&self, map: &HashMap<SessionKey, UiCursor>) -> Result<(), String> {
        use std::io::Write as _;

        // Deterministic order → a stable, reproducible file and a checksum that
        // does not depend on HashMap iteration order.
        let mut entries: Vec<(SessionKey, UiCursor)> =
            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        entries.sort_by(|(ka, _), (kb, _)| ka.0.cmp(&kb.0));

        let entries_json = serde_json::to_string(&entries).map_err(|e| e.to_string())?;
        let checksum = fnv1a(entries_json.as_bytes());
        let file = CursorFile { version: CURSOR_FILE_VERSION, checksum, entries };
        let data = serde_json::to_string(&file).map_err(|e| e.to_string())?;

        if let Some(dir) = self.path.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
        }

        // Unique sibling temp name: survives concurrent writers and never reuses
        // a crashed writer's leftover. Write → fsync → rename → fsync-dir.
        let tmp = self.tmp_path();
        let write_res = (|| -> std::io::Result<()> {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(data.as_bytes())?;
            // fsync the tmp's *contents* before the rename — otherwise a crash
            // can leave the renamed file pointing at unwritten / zeroed blocks.
            f.sync_all()?;
            Ok(())
        })();
        if let Err(e) = write_res {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.to_string());
        }

        if let Err(e) = std::fs::rename(&tmp, &self.path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.to_string());
        }

        // Make the rename itself durable across a crash (see helper).
        fsync_parent_dir(&self.path);
        Ok(())
    }

    /// A process-and-call-unique sibling path for the atomic temp write, e.g.
    /// `.cursors.json.tmp.<pid>.<seq>`. Uniqueness avoids clobbering a
    /// concurrent writer's temp and never adopts a crashed writer's leftover.
    fn tmp_path(&self) -> std::path::PathBuf {
        let seq = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = self.path.file_name().and_then(|s| s.to_str()).unwrap_or("cursors");
        let mut tmp = self.path.clone();
        tmp.set_file_name(format!(".{name}.tmp.{}.{seq}", std::process::id()));
        tmp
    }
}

impl CursorPersist for FileCursorPersist {
    fn load(&self, session: &SessionKey) -> Result<Option<UiCursor>, String> {
        Ok(self.read_map().get(session).cloned())
    }
    fn save(&self, session: &SessionKey, cursor: &UiCursor) -> Result<(), String> {
        let _g = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let mut map = self.read_map();
        map.insert(session.clone(), cursor.clone());
        self.write_map(&map)
    }
    fn forget(&self, session: &SessionKey) -> Result<(), String> {
        let _g = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let mut map = self.read_map();
        map.remove(session);
        self.write_map(&map)
    }
    fn load_all(&self) -> Result<HashMap<SessionKey, UiCursor>, String> {
        Ok(self.read_map())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_set_get_delete_round_trip() {
        let key = SessionKey::new("cli", "demo");
        let cur = UiCursor { stream: "main".into(), seq: 7 };
        let mut s = CursorStore::new();
        assert!(s.get(&key).is_none());
        s.set(key.clone(), cur.clone());
        assert_eq!(s.get(&key), Some(&cur));
        s.delete(&key);
        assert!(s.get(&key).is_none());
    }

    #[test]
    fn two_sessions_keep_independent_cursors() {
        // W08 invariant (the multi-session fix in proto.rs relies on this):
        // advancing one session's cursor must never touch another's. Before the
        // fix, `SharedState` held ONE cursor, so a notification for session B
        // clobbered session A's replay position.
        let a = SessionKey::new("cli", "weather");
        let b = SessionKey::new("cli", "shopping");
        let mut s = CursorStore::new();
        s.set(a.clone(), UiCursor { stream: "main".into(), seq: 5 });
        s.set(b.clone(), UiCursor { stream: "main".into(), seq: 9 });
        assert_eq!(s.get(&a).map(|c| c.seq), Some(5));
        assert_eq!(s.get(&b).map(|c| c.seq), Some(9));
        // A later notification for `b` advances b only.
        s.set(b.clone(), UiCursor { stream: "main".into(), seq: 12 });
        assert_eq!(s.get(&a).map(|c| c.seq), Some(5), "session a cursor unchanged");
        assert_eq!(s.get(&b).map(|c| c.seq), Some(12));
    }

    #[test]
    fn file_persist_survives_respawn() {
        // Layer 1b: cursors written by one store must be visible to a BRAND-NEW
        // store built on the same file — i.e. they survive a transport re-spawn
        // (the stdio child has no auto-restart, so reconnect = fresh store).
        let dir = std::env::temp_dir().join(format!("w08cursors-{}", std::process::id()));
        let file = dir.join("cursors.json");
        let _ = std::fs::remove_dir_all(&dir);
        let persist = std::sync::Arc::new(FileCursorPersist::new(&file));

        let a = SessionKey::new("cli", "weather");
        let b = SessionKey::new("cli", "shopping");
        {
            let mut s = CursorStore::new_persisted(persist.clone());
            s.set(a.clone(), UiCursor { stream: "main".into(), seq: 5 });
            s.set(b.clone(), UiCursor { stream: "main".into(), seq: 9 });
        }
        // Simulate a re-spawn: a fresh store hydrating from the same backend.
        let s2 = CursorStore::new_persisted(persist.clone());
        assert_eq!(s2.get(&a).map(|c| c.seq), Some(5), "cursor a survived respawn");
        assert_eq!(s2.get(&b).map(|c| c.seq), Some(9), "cursor b survived respawn");

        // `delete` is durable — a forgotten cursor does not resurrect.
        {
            let mut s = CursorStore::new_persisted(persist.clone());
            s.delete(&a);
        }
        let s3 = CursorStore::new_persisted(persist);
        assert!(s3.get(&a).is_none(), "deleted cursor stayed gone");
        assert_eq!(s3.get(&b).map(|c| c.seq), Some(9));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- W08 Layer 1b: crash-safety / corruption-resilience hardening ----

    /// A fresh, empty temp dir unique to this test (parallel test threads share
    /// one process, so the name must be per-test). Caller owns cleanup.
    fn fresh_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("w08cursor-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn cur(seq: u64) -> UiCursor {
        UiCursor { stream: "main".into(), seq }
    }

    #[test]
    fn load_missing_file_is_empty_and_set_still_works() {
        let dir = fresh_dir("missing");
        let file = dir.join("cursors.json");
        let p = FileCursorPersist::new(&file);
        // Missing file: load recovers to empty (no panic, no error).
        assert!(p.load_all().unwrap().is_empty());
        assert!(!file.exists());
        // First write creates the file and its parent dir, and round-trips.
        p.save(&SessionKey::new("cli", "a"), &cur(1)).unwrap();
        assert!(file.exists(), "first write created the file");
        assert_eq!(p.load_all().unwrap().get(&SessionKey::new("cli", "a")).map(|c| c.seq), Some(1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn first_write_creates_missing_parent_dirs() {
        let dir = fresh_dir("mkdir");
        // Two levels of parent that do not exist yet.
        let file = dir.join("nested").join("deeper").join("cursors.json");
        let p = FileCursorPersist::new(&file);
        p.save(&SessionKey::new("cli", "a"), &cur(7)).unwrap();
        assert!(file.exists(), "write created the nested parent dirs and the file");
        assert_eq!(p.load_all().unwrap().get(&SessionKey::new("cli", "a")).map(|c| c.seq), Some(7));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_empty_file_is_empty_and_heals_on_set() {
        let dir = fresh_dir("empty");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("cursors.json");
        std::fs::write(&file, "").unwrap();
        let p = FileCursorPersist::new(&file);
        assert!(p.load_all().unwrap().is_empty(), "empty file loads as empty, not a parse error");
        // Subsequent write heals it into a valid versioned file.
        p.save(&SessionKey::new("cli", "a"), &cur(3)).unwrap();
        assert_eq!(p.load_all().unwrap().get(&SessionKey::new("cli", "a")).map(|c| c.seq), Some(3));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn crash_mid_write_garbage_recovers_empty_then_set_works() {
        let dir = fresh_dir("garbage");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("cursors.json");
        // Simulate a crash that left non-JSON bytes in the file.
        std::fs::write(&file, "this is not json at all {{{").unwrap();
        let p = FileCursorPersist::new(&file);
        assert!(p.load_all().unwrap().is_empty(), "garbage recovers to empty, no panic");
        // A brand-new store hydrating from the corrupt file also survives.
        let store = CursorStore::new_persisted(std::sync::Arc::new(FileCursorPersist::new(&file)));
        assert!(store.iter().next().is_none());
        // And a later write repairs the file.
        p.save(&SessionKey::new("cli", "a"), &cur(4)).unwrap();
        assert_eq!(p.load_all().unwrap().get(&SessionKey::new("cli", "a")).map(|c| c.seq), Some(4));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn truncated_json_recovers_empty() {
        let dir = fresh_dir("truncated");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("cursors.json");
        // A write that died partway through (valid prefix, no closing bytes).
        std::fs::write(&file, r#"{"version":1,"checksum":12345,"entries":[["cli:a",{"stream":"main"#).unwrap();
        let p = FileCursorPersist::new(&file);
        assert!(p.load_all().unwrap().is_empty(), "truncated JSON recovers to empty");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn checksum_mismatch_recovers_empty() {
        let dir = fresh_dir("checksum");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("cursors.json");
        // Well-formed, current-version JSON but a deliberately wrong checksum:
        // catches valid-JSON bit-rot that atomic rename alone can't.
        std::fs::write(
            &file,
            r#"{"version":1,"checksum":1,"entries":[["cli:a",{"stream":"main","seq":3}]]}"#,
        )
        .unwrap();
        let p = FileCursorPersist::new(&file);
        assert!(p.load_all().unwrap().is_empty(), "checksum mismatch recovers to empty");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_future_version_recovers_empty() {
        let dir = fresh_dir("version");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("cursors.json");
        // A file written by a hypothetical newer client. Even with a matching
        // checksum, an unsupported version must not be silently misread.
        let entries_json = r#"[["cli:a",{"stream":"main","seq":3}]]"#;
        let checksum = super::fnv1a(entries_json.as_bytes());
        std::fs::write(
            &file,
            format!(r#"{{"version":999,"checksum":{checksum},"entries":{entries_json}}}"#),
        )
        .unwrap();
        let p = FileCursorPersist::new(&file);
        assert!(p.load_all().unwrap().is_empty(), "unknown future version recovers to empty");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_legacy_bare_array_format() {
        // Backward compat: an existing $HOME/a2app-cursors.json written by the
        // pre-versioning code is a bare `[[key, cursor], ...]` array.
        let dir = fresh_dir("legacy");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("cursors.json");
        std::fs::write(
            &file,
            r#"[["cli:weather",{"stream":"main","seq":5}],["cli:shopping",{"stream":"main","seq":9}]]"#,
        )
        .unwrap();
        let p = FileCursorPersist::new(&file);
        let m = p.load_all().unwrap();
        assert_eq!(m.get(&SessionKey::new("cli", "weather")).map(|c| c.seq), Some(5));
        assert_eq!(m.get(&SessionKey::new("cli", "shopping")).map(|c| c.seq), Some(9));
        // A subsequent write upgrades the file to the versioned format...
        p.save(&SessionKey::new("cli", "weather"), &cur(6)).unwrap();
        let raw = std::fs::read_to_string(&file).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["version"], 1, "legacy file rewritten in versioned format");
        // ...while preserving the other session and the update.
        let m2 = p.load_all().unwrap();
        assert_eq!(m2.get(&SessionKey::new("cli", "weather")).map(|c| c.seq), Some(6));
        assert_eq!(m2.get(&SessionKey::new("cli", "shopping")).map(|c| c.seq), Some(9));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn on_disk_file_is_versioned_and_checksummed() {
        let dir = fresh_dir("format");
        let file = dir.join("cursors.json");
        let p = FileCursorPersist::new(&file);
        p.save(&SessionKey::new("cli", "a"), &cur(1)).unwrap();
        let raw = std::fs::read_to_string(&file).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["version"], CURSOR_FILE_VERSION);
        assert!(v["checksum"].is_u64(), "checksum present as a u64");
        assert!(v["entries"].is_array(), "entries present as an array");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_temp_file_left_after_write() {
        let dir = fresh_dir("notmp");
        let file = dir.join("cursors.json");
        let p = FileCursorPersist::new(&file);
        p.save(&SessionKey::new("cli", "a"), &cur(1)).unwrap();
        p.forget(&SessionKey::new("cli", "a")).unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp."))
            .collect();
        assert!(leftovers.is_empty(), "atomic write left temp litter: {leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn many_sessions_round_trip_survives_respawn() {
        let dir = fresh_dir("many");
        let file = dir.join("cursors.json");
        let persist = std::sync::Arc::new(FileCursorPersist::new(&file));
        const N: u64 = 250;
        {
            let mut s = CursorStore::new_persisted(persist.clone());
            for i in 0..N {
                s.set(SessionKey::new("cli", &format!("s{i}")), cur(i));
            }
        }
        // Fresh store hydrates every one of them from the same file.
        let s2 = CursorStore::new_persisted(persist);
        for i in 0..N {
            assert_eq!(
                s2.get(&SessionKey::new("cli", &format!("s{i}"))).map(|c| c.seq),
                Some(i),
                "session s{i} survived respawn",
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
