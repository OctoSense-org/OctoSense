//! The files a person chose for a process app in the shell's own open and save panels
//! (ADR 0015, "A Mac's own panels for a sandboxed app").
//!
//! A sandboxed app can't reach a file outside its jail, and its jail stays as its
//! manifest says. When the shell's own panel answers an app, the shell keeps a grant:
//! that process, that file, as it really is at that moment. The app then asks the shell
//! to read or replace exactly that file (`AppToStudio::ChosenFile`); any other path, or
//! another process's file, is refused. Grants live in the shell's memory only, per
//! process connection: a restart, a reconnect or the process ending drops them.
//!
//! The check and the work are apart (the work runs off the UI thread), and the chosen
//! file may sit in a folder the app can change, such as its own. So on Unix the work
//! never trusts the path again: a read opens the file without following a link and reads
//! only when the opened file is the chosen one; a write opens the chosen file's folder the
//! same way and makes and moves its new bytes inside that open folder. A link or a moved
//! folder put in the way after the check finds nothing to use.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::hub::ClientId;

/// The most a chosen file may hold, read or written: 64 MiB.
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// The refusal of a path the shell's panel did not give this app.
pub const NOT_CHOSEN: &str = "This app can only use a file you chose for it.";
const TOO_LARGE: &str = "The file is larger than 64 MB.";
const NOT_A_FILE: &str = "This isn\u{2019}t a file.";

/// Which files each process was given, by its panel's answers.
#[derive(Default)]
pub struct Grants {
    by_client: HashMap<ClientId, HashSet<PathBuf>>,
}

impl Grants {
    /// The person chose `path` in the shell's panel for `client`: while that process's
    /// connection lives it may read and replace that file, and no other. Kept as the
    /// file really is now (links followed; a file not there yet by its folder's real
    /// place and its name). None, and nothing granted, when it can't be placed.
    pub fn grant(&mut self, client: ClientId, path: &Path) -> Option<PathBuf> {
        let real = real_place(path)?;
        self.by_client.entry(client).or_default().insert(real.clone());
        Some(real)
    }

    /// That process is gone, or its connection is: its grants go with it.
    pub fn forget(&mut self, client: ClientId) {
        self.by_client.remove(&client);
    }

    /// Where `path` really is, when the shell's panel gave it to `client`; a plain
    /// refusal for any other path, and for another process's file.
    pub fn granted(&self, client: ClientId, path: &Path) -> Result<PathBuf, String> {
        let real = real_place(path).ok_or_else(|| NOT_CHOSEN.to_string())?;
        if self.by_client.get(&client).is_some_and(|files| files.contains(&real)) {
            Ok(real)
        } else {
            Err(NOT_CHOSEN.to_string())
        }
    }
}

/// Where `path` really is: the file itself when it is there (links followed), else its
/// folder's real place and its name. None for a relative path, or one with no name or
/// no folder.
pub fn real_place(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    if let Ok(real) = std::fs::canonicalize(path) {
        return Some(real);
    }
    let name = path.file_name()?;
    let folder = std::fs::canonicalize(path.parent()?).ok()?;
    Some(folder.join(name))
}

/// A failure in a person's words: the operating system's are not for them.
fn words(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "The file is no longer there.".to_string(),
        // As a Mac's alert says it, the same words whichever part was refused.
        std::io::ErrorKind::PermissionDenied => "You don\u{2019}t have permission to use this file.".to_string(),
        _ => "The disk may be full or unavailable.".to_string(),
    }
}

/// Where an open file or folder really is, by its descriptor: where its path led when
/// it was opened, whatever that path leads to now.
#[cfg(target_os = "macos")]
fn opened_place(fd: &impl std::os::unix::io::AsRawFd) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let mut buf = vec![0u8; libc::PATH_MAX as usize + 1];
    // SAFETY: F_GETPATH writes at most PATH_MAX bytes, its NUL included, into `buf`.
    if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETPATH, buf.as_mut_ptr()) } == -1 {
        return None;
    }
    let len = buf.iter().position(|&b| b == 0)?;
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(&buf[..len])))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn opened_place(fd: &impl std::os::unix::io::AsRawFd) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/self/fd/{}", fd.as_raw_fd())).ok()
}

/// An open that met a link where the chosen file or its folder was: not the chosen file.
/// A link opened as a folder without following it says ENOTDIR rather than ELOOP.
#[cfg(unix)]
fn opened_words(error: &std::io::Error) -> String {
    if matches!(error.raw_os_error(), Some(libc::ELOOP) | Some(libc::ENOTDIR)) {
        NOT_CHOSEN.to_string()
    } else {
        words(error)
    }
}

/// The chosen file's bytes, at most [`MAX_BYTES`]. Opened without following a link and
/// without waiting on a pipe, and read only when the opened file is the chosen one.
#[cfg(unix)]
pub fn read(real: &Path) -> Result<Vec<u8>, String> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(real)
        .map_err(|e| opened_words(&e))?;
    if opened_place(&file).as_deref() != Some(real) {
        return Err(NOT_CHOSEN.into());
    }
    let meta = file.metadata().map_err(|e| words(&e))?;
    if !meta.is_file() {
        return Err(NOT_A_FILE.into());
    }
    if meta.len() > MAX_BYTES {
        return Err(TOO_LARGE.into());
    }
    let mut bytes = Vec::new();
    (&file).take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|e| words(&e))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(TOO_LARGE.into());
    }
    Ok(bytes)
}

/// The chosen file's bytes, at most [`MAX_BYTES`]: elsewhere the path is trusted, as it
/// was checked.
#[cfg(not(unix))]
pub fn read(real: &Path) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(real).map_err(|e| words(&e))?;
    if !meta.is_file() {
        return Err(NOT_A_FILE.into());
    }
    if meta.len() > MAX_BYTES {
        return Err(TOO_LARGE.into());
    }
    let mut bytes = Vec::new();
    let file = std::fs::File::open(real).map_err(|e| words(&e))?;
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|e| words(&e))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(TOO_LARGE.into());
    }
    Ok(bytes)
}

/// Replace the chosen file with `data`, whole: written beside it under a hidden name,
/// then moved over it, so a failed write never cuts the person's file short.
pub fn write(real: &Path, data: &[u8]) -> Result<(), String> {
    write_then(real, data, || Ok(()))
}

/// The hidden name the new bytes are written under, beside the file.
fn scratch_name(name: &std::ffi::OsStr) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(".{}.octosense-{}-{}", name.to_string_lossy(), std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))
}

/// [`write`], with `before_move` run between writing the new bytes and moving them over
/// the file (a test's failure). The chosen file's folder is opened without following a
/// link and checked; what is there, the new file and the move are all done inside that
/// open folder, so a folder moved or linked in after the check is never written to.
#[cfg(unix)]
fn write_then(real: &Path, data: &[u8], before_move: impl FnOnce() -> std::io::Result<()>) -> Result<(), String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::os::unix::io::{AsRawFd, FromRawFd};
    if data.len() as u64 > MAX_BYTES {
        return Err(TOO_LARGE.into());
    }
    let (Some(folder), Some(name)) = (real.parent(), real.file_name()) else {
        return Err(NOT_A_FILE.into());
    };
    let dir = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(folder)
        .map_err(|e| opened_words(&e))?;
    if opened_place(&dir).as_deref() != Some(folder) {
        return Err(NOT_CHOSEN.into());
    }
    let name = CString::new(name.as_bytes()).map_err(|_| NOT_A_FILE.to_string())?;
    let scratch = CString::new(scratch_name(real.file_name().unwrap_or_default())).map_err(|_| NOT_A_FILE.to_string())?;
    // What is in the file's place now, not followed: a link or a folder there isn't the file.
    // SAFETY: `stat` is plain data, filled by fstatat or left unread.
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `dir` is open, `name` is NUL-terminated and `stat` is writable.
    let old_mode = if unsafe { libc::fstatat(dir.as_raw_fd(), name.as_ptr(), &mut stat, libc::AT_SYMLINK_NOFOLLOW) } == 0 {
        if stat.st_mode & libc::S_IFMT != libc::S_IFREG {
            return Err(NOT_A_FILE.into());
        }
        Some((stat.st_mode & 0o7777) as u32)
    } else {
        None
    };
    let result = (|| {
        let flags = libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        // SAFETY: `dir` is open and `scratch` is NUL-terminated; the descriptor returned is ours.
        let fd = unsafe { libc::openat(dir.as_raw_fd(), scratch.as_ptr(), flags, 0o600 as libc::c_uint) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: `fd` was just opened and nothing else owns it.
        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        file.write_all(data)?;
        // The file keeps who may read it.
        if let Some(mode) = old_mode {
            let _ = file.set_permissions(std::fs::Permissions::from_mode(mode));
        }
        file.sync_all()?;
        drop(file);
        before_move()?;
        // SAFETY: both names are NUL-terminated and relative to the same open folder.
        if unsafe { libc::renameat(dir.as_raw_fd(), scratch.as_ptr(), dir.as_raw_fd(), name.as_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    })();
    if let Err(error) = result {
        // SAFETY: as above; a scratch file that was never made is simply not there.
        unsafe { libc::unlinkat(dir.as_raw_fd(), scratch.as_ptr(), 0) };
        return Err(words(&error));
    }
    Ok(())
}

/// [`write`], with `before_move` run between writing the new bytes and moving them over
/// the file (a test's failure). Elsewhere the path is trusted, as it was checked.
#[cfg(not(unix))]
fn write_then(real: &Path, data: &[u8], before_move: impl FnOnce() -> std::io::Result<()>) -> Result<(), String> {
    if data.len() as u64 > MAX_BYTES {
        return Err(TOO_LARGE.into());
    }
    let (Some(folder), Some(name)) = (real.parent(), real.file_name()) else {
        return Err(NOT_A_FILE.into());
    };
    let old = std::fs::symlink_metadata(real).ok();
    if old.as_ref().is_some_and(|meta| !meta.is_file()) {
        return Err(NOT_A_FILE.into());
    }
    let scratch = folder.join(scratch_name(name));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&scratch)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        // The file keeps who may read it.
        if let Some(meta) = &old {
            let _ = std::fs::set_permissions(&scratch, meta.permissions());
        }
        before_move()?;
        std::fs::rename(&scratch, real)
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&scratch);
        return Err(words(&error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own under the temporary folder, as it really is.
    fn place(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("chosen-files-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::canonicalize(dir).unwrap()
    }

    #[test]
    fn chosen_files_a_saved_file_is_written_whole_and_only_there() {
        let dir = place("write");
        let mut grants = Grants::default();
        let target = dir.join("Budget.sheets");
        // A save names a file that isn't there yet.
        let real = grants.grant(7, &target).expect("its folder is there");
        assert_eq!(real, target);
        write(&grants.granted(7, &target).unwrap(), b"first").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"first");
        write(&grants.granted(7, &target).unwrap(), b"second, longer").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"second, longer");
        // Nothing else was left beside it.
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 1, "{names:?}");
        // The folder's other files are not the app's.
        assert_eq!(grants.granted(7, &dir.join("Other.sheets")), Err(NOT_CHOSEN.to_string()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn chosen_files_an_opened_file_is_read_as_it_is() {
        let dir = place("read");
        let file = dir.join("Planning.csv");
        std::fs::write(&file, "Item,Cost\n").unwrap();
        let mut grants = Grants::default();
        // The panel's answer may name it by another way there; the grant is the file.
        let roundabout = dir.join(".").join("Planning.csv");
        grants.grant(3, &roundabout).unwrap();
        assert_eq!(read(&grants.granted(3, &file).unwrap()).unwrap(), b"Item,Cost\n");
        // An opened file may be saved back.
        write(&grants.granted(3, &file).unwrap(), b"Item,Cost\nRent,1200\n").unwrap();
        assert_eq!(read(&grants.granted(3, &file).unwrap()).unwrap(), b"Item,Cost\nRent,1200\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn chosen_files_a_path_never_chosen_is_refused() {
        let dir = place("refuse");
        std::fs::write(dir.join("secret.txt"), "no").unwrap();
        let mut grants = Grants::default();
        assert_eq!(grants.granted(1, &dir.join("secret.txt")), Err(NOT_CHOSEN.to_string()));
        grants.grant(1, &dir.join("open.txt")).unwrap();
        // Not a neighbour, not the folder, not a way out of it, not a relative name.
        for path in [dir.join("secret.txt"), dir.clone(), dir.join("..").join("open.txt"), PathBuf::from("open.txt")] {
            assert_eq!(grants.granted(1, &path), Err(NOT_CHOSEN.to_string()), "{path:?}");
        }
        // A link put where the chosen file was leads elsewhere: refused.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.join("secret.txt"), dir.join("open.txt")).unwrap();
            assert_eq!(grants.granted(1, &dir.join("open.txt")), Err(NOT_CHOSEN.to_string()));
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The check passes, then the app puts a link where the chosen file was before the
    /// work runs: neither the read nor the write follows it.
    #[cfg(unix)]
    #[test]
    fn chosen_files_a_link_put_in_after_the_check_leads_nowhere() {
        let dir = place("swap-link");
        let secret = dir.join("secret.txt");
        std::fs::write(&secret, "not the app's").unwrap();
        let file = dir.join("Budget.sheets");
        std::fs::write(&file, "mine").unwrap();
        let mut grants = Grants::default();
        grants.grant(5, &file).unwrap();
        let real = grants.granted(5, &file).unwrap();
        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(&secret, &file).unwrap();
        assert_eq!(read(&real), Err(NOT_CHOSEN.to_string()));
        assert_eq!(write(&real, b"overwritten"), Err(NOT_A_FILE.to_string()));
        assert_eq!(std::fs::read_to_string(&secret).unwrap(), "not the app's");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The check passes, then the chosen file's folder is moved and a link to another
    /// folder holding a file of the same name put in its place: that file is never used.
    #[cfg(unix)]
    #[test]
    fn chosen_files_a_folder_moved_after_the_check_is_never_used() {
        let dir = place("swap-folder");
        let docs = dir.join("docs");
        let elsewhere = dir.join("elsewhere");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(docs.join("Plan.csv"), "mine").unwrap();
        std::fs::write(elsewhere.join("Plan.csv"), "someone else's").unwrap();
        let mut grants = Grants::default();
        grants.grant(6, &docs.join("Plan.csv")).unwrap();
        let real = grants.granted(6, &docs.join("Plan.csv")).unwrap();
        std::fs::rename(&docs, dir.join("docs-moved")).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &docs).unwrap();
        assert_eq!(read(&real), Err(NOT_CHOSEN.to_string()));
        assert_eq!(write(&real, b"overwritten"), Err(NOT_CHOSEN.to_string()));
        assert_eq!(std::fs::read_to_string(elsewhere.join("Plan.csv")).unwrap(), "someone else's");
        let names: Vec<_> = std::fs::read_dir(&elsewhere).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 1, "nothing was left there: {names:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A pipe put where the chosen file was is refused at once, not waited on.
    #[cfg(unix)]
    #[test]
    fn chosen_files_a_pipe_put_in_its_place_is_not_waited_on() {
        use std::os::unix::ffi::OsStrExt;
        let dir = place("swap-pipe");
        let file = dir.join("Budget.sheets");
        std::fs::write(&file, "mine").unwrap();
        let mut grants = Grants::default();
        grants.grant(8, &file).unwrap();
        let real = grants.granted(8, &file).unwrap();
        std::fs::remove_file(&file).unwrap();
        let name = std::ffi::CString::new(file.as_os_str().as_bytes()).unwrap();
        // SAFETY: a NUL-terminated path in a folder of the test's own.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let refused = read(&real);
        assert!(refused == Err(NOT_A_FILE.to_string()) || refused == Err(NOT_CHOSEN.to_string()), "{refused:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn chosen_files_one_process_cannot_use_anothers_file() {
        let dir = place("across");
        let file = dir.join("Shared.sheets");
        std::fs::write(&file, "mine").unwrap();
        let mut grants = Grants::default();
        grants.grant(10, &file).unwrap();
        assert!(grants.granted(10, &file).is_ok());
        assert_eq!(grants.granted(11, &file), Err(NOT_CHOSEN.to_string()));
        // The process (or its connection) goes: so does what it was given.
        grants.forget(10);
        assert_eq!(grants.granted(10, &file), Err(NOT_CHOSEN.to_string()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn chosen_files_neither_a_read_nor_a_write_passes_64_mb() {
        let dir = place("cap");
        let big = dir.join("big.bin");
        let file = std::fs::File::create(&big).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();
        assert_eq!(read(&big), Err(TOO_LARGE.to_string()));
        let file = std::fs::File::create(&big).unwrap();
        file.set_len(MAX_BYTES).unwrap();
        assert_eq!(read(&big).map(|bytes| bytes.len() as u64), Ok(MAX_BYTES));
        let kept = dir.join("kept.bin");
        std::fs::write(&kept, b"old").unwrap();
        assert_eq!(write(&kept, &vec![0u8; MAX_BYTES as usize + 1]), Err(TOO_LARGE.to_string()));
        assert_eq!(std::fs::read(&kept).unwrap(), b"old");
        assert_eq!(MAX_BYTES, makepad_widgets::makepad_platform::file_dialogs::CHOSEN_FILE_MAX_BYTES, "the app and the shell agree");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn chosen_files_a_failed_write_keeps_the_old_file() {
        let dir = place("atomic");
        let file = dir.join("Budget.sheets");
        std::fs::write(&file, "the person's work").unwrap();
        // Fails after the new bytes are down, before they replace the file.
        let failed = write_then(&file, b"half", || Err(std::io::Error::other("the disk went away")));
        assert_eq!(failed, Err("The disk may be full or unavailable.".to_string()));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "the person's work");
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 1, "nothing is left beside it: {names:?}");
        // A folder that can't take a new file: the old one stays as it was.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
            let refused = write(&file, b"new");
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(refused.is_err());
            assert_eq!(std::fs::read_to_string(&file).unwrap(), "the person's work");
        }
        // A folder where the file was is not a file to replace.
        let folder = dir.join("Folder.sheets");
        std::fs::create_dir(&folder).unwrap();
        assert_eq!(write(&folder, b"x"), Err(NOT_A_FILE.to_string()));
        let _ = std::fs::remove_dir_all(dir);
    }
}
