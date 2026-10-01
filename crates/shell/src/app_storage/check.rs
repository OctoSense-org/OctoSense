//! The startup check (ADR 0004 §11): no agent workspace — an app's account
//! folder, `apps/<app id>/accounts/<account>/` — may contain or reach the
//! host's secrets.
//!
//! A workspace is flagged when
//! - it is itself a symlink (a workspace is a real folder in the jail;
//!   `Storage::open` refuses symlinked jail components too), or the app's
//!   jail or `accounts/` is a symlink reaching the secrets;
//! - any symlink inside it points into the secrets root or at a folder
//!   containing it (`/`, the home), whether or not the target exists yet;
//! - any file inside it is a hard link to a file under the secrets root;
//! - the secrets root is inside it.
//!
//! A flagged workspace is refused: the shell logs it and
//! `AppStorage::agent_workspace` answers `Refused` for that account until
//! a later start finds it clean, and the broker (through
//! `ToolHost::workspace_refused`) neither prepares nor resumes its peer,
//! rejects its `peer/input` and answers its calls `workspace_refused`. Nothing is deleted: the person's files
//! stay where they are. The walk never follows symlinks and stops after
//! [`MAX_ENTRIES`] entries (logged as incomplete).

use super::{lexical, resolved, validate_app_id, Layout};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Entries walked in total before the check gives up (and says so).
pub const MAX_ENTRIES: usize = 200_000;
/// Violations recorded per workspace (one is enough to refuse it).
const MAX_PER_WORKSPACE: usize = 8;
/// The account name a jail-level violation refuses: every account.
pub const EVERY_ACCOUNT: &str = "*";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub app_id: String,
    /// The account folder's name (its hash, or `device`), or
    /// [`EVERY_ACCOUNT`].
    pub account: String,
    /// The offending entry.
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct Report {
    /// Account folders checked.
    pub workspaces: usize,
    /// Entries walked.
    pub entries: usize,
    pub violations: Vec<Violation>,
    /// Whether the entry cap stopped the walk.
    pub incomplete: bool,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }

    pub fn log(&self) {
        for v in &self.violations {
            makepad_widgets::log!(
                "app storage: REFUSED the agent workspace of {} account {}: {} ({})",
                v.app_id,
                v.account,
                v.reason,
                v.path.display()
            );
        }
        if self.incomplete {
            makepad_widgets::log!(
                "app storage: the startup check stopped after {} entries; later workspaces were not checked",
                self.entries
            );
        }
    }
}

struct Secrets {
    lexical: PathBuf,
    resolved: PathBuf,
    /// (device, inode) of every file under the secrets root.
    files: HashSet<(u64, u64)>,
}

impl Secrets {
    /// Whether `target` is inside the secrets root or contains it.
    fn reached_by(&self, target: &Path) -> bool {
        [&self.lexical, &self.resolved].iter().any(|s| target.starts_with(s) || s.starts_with(target))
    }
}

#[cfg(unix)]
fn file_id(meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn file_id(_: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

fn secret_files(root: &Path, budget: &mut usize) -> HashSet<(u64, u64)> {
    let mut files = HashSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if *budget == 0 {
                return files;
            }
            *budget -= 1;
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                files.extend(file_id(&meta));
            }
        }
    }
    files
}

/// Where a symlink at `link` points, lexically (it may not exist yet).
fn link_target(link: &Path) -> Option<PathBuf> {
    let target = std::fs::read_link(link).ok()?;
    let base = link.parent().unwrap_or(Path::new("/"));
    Some(lexical(&base.join(target)))
}

/// Whether the symlink at `link` reaches the secrets, by its written target
/// or by what it resolves to now.
fn link_reaches(link: &Path, secrets: &Secrets) -> bool {
    link_target(link).is_some_and(|t| secrets.reached_by(&t) || secrets.reached_by(&resolved(&t)))
        || std::fs::canonicalize(link).is_ok_and(|t| secrets.reached_by(&t))
}

/// Check every agent workspace under `layout`'s apps root.
pub fn check_workspaces(layout: &Layout) -> Report {
    let mut report = Report::default();
    let apps_root = layout.apps_root();
    let Ok(apps) = std::fs::read_dir(apps_root) else { return report };
    let mut budget = MAX_ENTRIES;
    let secrets = Secrets {
        lexical: lexical(layout.secrets_root()),
        resolved: resolved(layout.secrets_root()),
        files: secret_files(layout.secrets_root(), &mut budget),
    };
    let mut budget = MAX_ENTRIES;
    let mut apps: Vec<_> = apps.flatten().collect();
    apps.sort_by_key(|e| e.file_name());
    for app in apps {
        let app_id = app.file_name().to_string_lossy().into_owned();
        if validate_app_id(&app_id).is_err() {
            continue; // App Hub's own `.host`, `.system`, `catalog.json`
        }
        let jail = app.path();
        let accounts = jail.join("accounts");
        for top in [&jail, &accounts] {
            let is_link = std::fs::symlink_metadata(top).is_ok_and(|m| m.file_type().is_symlink());
            if is_link && link_reaches(top, &secrets) {
                report.violations.push(Violation {
                    app_id: app_id.clone(),
                    account: EVERY_ACCOUNT.into(),
                    path: top.clone(),
                    reason: "a symlink into the secrets".into(),
                });
            }
        }
        let Ok(folders) = std::fs::read_dir(&accounts) else { continue };
        let mut folders: Vec<_> = folders.flatten().collect();
        folders.sort_by_key(|e| e.file_name());
        for folder in folders {
            let account = folder.file_name().to_string_lossy().into_owned();
            let path = folder.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
            report.workspaces += 1;
            let mut found = Vec::new();
            if meta.file_type().is_symlink() {
                let reason = if link_reaches(&path, &secrets) {
                    "the workspace is a symlink into the secrets"
                } else {
                    "the workspace is a symlink"
                };
                found.push((path.clone(), reason.to_string()));
            } else if meta.is_dir() {
                if secrets.resolved.starts_with(resolved(&path)) {
                    found.push((path.clone(), "the workspace contains the secrets".into()));
                }
                walk(&path, &secrets, &mut budget, &mut report, &mut found);
            }
            for (at, reason) in found {
                report.violations.push(Violation { app_id: app_id.clone(), account: account.clone(), path: at, reason });
            }
            if report.incomplete {
                return report;
            }
        }
    }
    report
}

fn walk(root: &Path, secrets: &Secrets, budget: &mut usize, report: &mut Report, found: &mut Vec<(PathBuf, String)>) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if found.len() >= MAX_PER_WORKSPACE {
                return;
            }
            if *budget == 0 {
                report.incomplete = true;
                return;
            }
            *budget -= 1;
            report.entries += 1;
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
            if meta.file_type().is_symlink() {
                if link_reaches(&path, secrets) {
                    found.push((path, "a symlink into the secrets".into()));
                }
            } else if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() && file_id(&meta).is_some_and(|id| secrets.files.contains(&id)) {
                found.push((path, "a hard link to a secret".into()));
            }
        }
    }
}
