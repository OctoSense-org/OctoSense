//! Client processes and the app-launching model, behavior read from
//! omarchy's source (local/agent_state/wm/omarchy-launch-model.md):
//!
//! - the terminal is ALWAYS a fresh instance, opened in the cwd of the
//!   focused terminal (omarchy-launch-terminal + omarchy-cmd-terminal-cwd;
//!   our children report pwd over OSC 7 -> Layer B custom message),
//! - other apps use launch-or-focus: `\b<pattern>\b` case-insensitive
//!   against window class OR title focuses an existing window, else spawns
//!   (bin/omarchy-launch-or-focus).
//!
//! Children are Makepad apps launched with `--stdin-loop` and
//! `STUDIO_HOST`/`STUDIO_BUILD` pointing at the in-process hub, exactly
//! like studio launches run targets.
//!
//! **They are built before they start** (`cargo build --release`, see
//! [`launch_plan`]) whenever the shell runs out of a checkout, so a stale
//! or missing binary is rebuilt on launch instead of failing or showing
//! yesterday's app; a native app from the OctoSense workspace, held to its
//! `Cargo.lock`. The build runs outside any sandbox, in its checkout, and
//! its "Compiling …" output lands in the client's log; then the built
//! binary itself starts under the app's sandbox ([`ClientSlot::continue_launch`]).
//! Cargo never starts the app (no `cargo run`): what the app can write never
//! decides what the next build runs. An installed shell with no checkout
//! around it starts the sibling binary.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use crate::host;
#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[cfg(unix)]
use makepad_widgets::makepad_platform::thread::CancellationToken;
use makepad_widgets::makepad_platform::thread::{Lane, SignalToUI, TaskPool, ThreadSpawner, ThreadOptions};
#[cfg(any(unix, test))]
use makepad_widgets::Cx;

use crate::hub::ClientId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchPolicy {
    /// Every invocation spawns a new instance (the terminal, viewers).
    AlwaysNew,
    /// Focus a running instance of this app if one exists, else spawn.
    OrFocus,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppDef {
    /// Registry id, also the launch-or-focus window pattern.
    pub id: String,
    /// The name a human reads in the menu.
    pub label: String,
    /// Binary name, for the installed (no checkout) fallback.
    pub bin: String,
    /// Cargo package name — what `cargo run -p` gets.
    pub package: String,
    /// Package directory relative to the checkout root.
    pub dir: String,
    /// A crate outside the root workspace (its own workspace root) needs
    /// its manifest named explicitly; relative to the checkout root.
    pub manifest: Option<String>,
    pub args: Vec<String>,
    pub policy: LaunchPolicy,
    /// Where `cargo` should build this app. Set for rows that resolve into
    /// a checkout this project does not own, so the build lands in our tree
    /// instead of someone else's cache.
    pub target_dir: Option<String>,
}

impl AppDef {
    fn app(
        id: &str,
        label: &str,
        package: &str,
        dir: &str,
        bin: &str,
        policy: LaunchPolicy,
    ) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            bin: bin.to_string(),
            package: package.to_string(),
            dir: dir.to_string(),
            manifest: None,
            args: Vec::new(),
            policy,
            target_dir: None,
        }
    }

    /// True when this app can actually be started right now — the honest
    /// filter behind the menu (no row that cannot run).
    pub fn is_available(&self) -> bool {
        if let Some(manifest) = &self.manifest {
            return Path::new(manifest).is_file();
        }
        resolve_bin(&self.bin).is_some()
    }

}

/// One `name = "..."` value out of a Cargo.toml `[package]` table.
#[cfg(test)]
fn manifest_value(manifest: &str, key: &str) -> Option<String> {
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        return Some(v.trim().trim_matches('"').to_string());
    }
    None
}

/// The app registry: the applications this WM is built around, in menu
/// order. Curated on purpose — every row is one we run and verify, not a
/// scan of whatever the workspace happens to contain: the catalog's rows,
/// the linked modules, then the apps App Hub's Card runner hosts (system
/// apps and installed apps, read fresh so an install needs no restart).
pub fn registry() -> Vec<AppDef> {
    let base = crate::octosense::catalog::loaded().as_ref().cloned().unwrap_or_default();
    merge_catalog(
        base,
        crate::apps::bundled_modules_catalog(),
        crate::apps::system_card_apps(),
        crate::apps::installed_card_apps(),
    )
}

/// One row per id. A system app replaces a catalog row of the same id in
/// place (it keeps that row's menu position): the system Mail, not
/// Makepad's example Mail, even in a personal catalog. Otherwise the first
/// definition wins — catalog rows, then linked modules, so native
/// definitions take precedence. Installed ids live under `hub:`, so a
/// manifest named after a built-in becomes its own row beside it, never a
/// replacement. What `apps::catalog_visible` hides (the internal `card`
/// host, the retired `appstore`) is no row.
fn merge_catalog(base: Vec<AppDef>, bundled: Vec<AppDef>, system: Vec<AppDef>, installed: Vec<AppDef>) -> Vec<AppDef> {
    let mut system: Vec<Option<AppDef>> = system.into_iter().map(Some).collect();
    let mut take_system = |id: &str| system.iter_mut().find(|a| a.as_ref().is_some_and(|a| a.id == id)).and_then(Option::take);
    let mut rows: Vec<AppDef> = Vec::new();
    for app in base.into_iter().chain(bundled) {
        rows.push(take_system(&app.id).unwrap_or(app));
    }
    rows.extend(system.into_iter().flatten());
    rows.extend(installed);
    let mut ids = std::collections::HashSet::new();
    rows.retain(|app| crate::apps::catalog_visible(&app.id) && ids.insert(app.id.clone()));
    rows
}

/// Catalog rows, bundled modules and the Card runner's apps (system and
/// installed through App Hub), read fresh on each call.
pub fn available_apps() -> Vec<AppDef> {
    registry()
}

/// Registered ids take precedence over binary aliases. A linked module
/// without a catalog row (a module-only app such as `appcard`, which has no
/// process form) is still an app: its bundled definition answers, and the
/// hosting rules decide whether it may open (`--module <id>` on a desktop).
pub fn find_app(id: &str) -> Option<AppDef> {
    let apps = registry();
    apps.iter().find(|a| a.id == id)
        .or_else(|| apps.iter().find(|a| a.bin == id && a.bin != "card")).cloned()
}

/// `bin/omarchy-launch-or-focus`'s window test, verbatim:
/// `test("\\b" + pattern + "\\b"; "i")` — a case-insensitive WHOLE-WORD
/// match, where a word boundary is any non-alphanumeric/underscore.
pub fn word_match(haystack: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    let hay = haystack.to_lowercase();
    let pat = pattern.to_lowercase();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let bytes: Vec<char> = hay.chars().collect();
    let needle: Vec<char> = pat.chars().collect();
    if needle.len() > bytes.len() {
        return false;
    }
    for start in 0..=bytes.len() - needle.len() {
        if bytes[start..start + needle.len()] != needle[..] {
            continue;
        }
        let before_ok = start == 0 || !word(bytes[start - 1]);
        let end = start + needle.len();
        let after_ok = end == bytes.len() || !word(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// The checkout root: `MAKEPAD_WM_ROOT`, else the checkout above the
/// running exe (`target/<profile>/wm`), else the checkout at or above the
/// current directory — a wm started from the repo root with its target
/// dir elsewhere (CARGO_TARGET_DIR) is still running out of a checkout,
/// and every app is then one `cargo run` away.
pub fn repo_root() -> Option<PathBuf> {
    crate::octosense::paths::project_root()
}

/// Resolve a sibling binary of the running wm executable (`.exe` on
/// Windows, where a bare name never exists).
pub fn resolve_bin(bin: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let mut path = dir.join(bin);
    if cfg!(windows) {
        path.set_extension("exe");
    }
    if !path.is_file() { return None; }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if path.metadata().ok()?.permissions().mode() & 0o111 == 0 { return None; }
    }
    Some(path)
}

/// The cargo to launch with: whatever is on PATH, else the rustup default.
fn cargo_bin() -> PathBuf {
    if let Ok(cargo) = std::env::var("CARGO") {
        return PathBuf::from(cargo);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let rustup = PathBuf::from(home).join(".cargo/bin/cargo");
        if rustup.exists() {
            return rustup;
        }
    }
    PathBuf::from("cargo")
}

// ======================================================================
// The warm-instance pool
// ======================================================================

/// How many DORMANT instances of an app the pool keeps standing by, so a
/// new window is a swap instead of a launch. The user's sizing: terminals
/// get two (people burst-open them), the rest one each. An app that is not
/// in this table is never pre-spawned.
///
/// This is the whole registry of warmable apps — `is_warm_app` and the
/// startup top-up both read it, so adding an app here is the only edit an
/// app needs to join the pool.
pub const WARM_CAPACITY: &[(&str, usize)] = &[
    ("terminal", 2),
    ("browser", 1),
    ("files", 1),
    ("task", 1),
];

/// The env a warm instance is spawned with. `makepad_wm_api::warm_start()` reads
/// exactly this: the app boots its window and draws once, then IDLES — no
/// samplers, no refresh timers, no polling — until `WmEvent::Adopted`
/// arrives. Without it a cached task manager would sit there sampling
/// every process on the machine for nothing.
pub const WARM_ENV: (&str, &str) = ("MAKEPAD_WM_WARM_START", "1");

/// Crash budget: this many UNEXPECTED warm deaths per app inside
/// `WARM_CRASH_WINDOW`, after which the pool gives that app up quietly and
/// every launch takes the cold path (which always works). Adoption
/// replacements are NOT crashes and are never capped — capping those would
/// switch the pool off for anyone who opens four terminals in a minute,
/// which is exactly who it exists for.
pub const WARM_CRASH_LIMIT: usize = 3;
pub const WARM_CRASH_WINDOW: f64 = 60.0;

/// What the WM knows about one pooled instance right now, handed to
/// `WarmPool::adopt` so the pool itself stays free of WM state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WarmStatus {
    pub client: ClientId,
    /// Still in the client table: the process has not been reaped.
    pub alive: bool,
    /// Connected to the hub AND past `CreateWindow` — it has a framebuffer
    /// and a drawn frame, so a tile can show it this instant. A warm
    /// instance that is still building (or still starting) is not one.
    pub connected: bool,
}

/// The pool: per app, the ids of the instances standing by.
///
/// Deliberately a plain state machine over ids — no processes, no cx, no
/// layout — so the rules that matter (adopt clears and tops back up, a
/// dead instance falls back to a cold spawn, a cwd override skips the pool,
/// MAKEPAD_WM_NO_WARM turns it off, crash loops give up) are unit-testable
/// without a running window manager.
#[derive(Debug)]
pub struct WarmPool {
    enabled: bool,
    /// Appearance in which browser pages were warmed. A loaded page may
    /// choose its theme only once, so a media-query update is not sufficient.
    browser_dark: Option<bool>,
    /// app id -> the warm clients of that app, oldest first.
    ready: HashMap<String, Vec<ClientId>>,
    /// app id -> when (platform seconds) its warm instances died unexpectedly, newest last.
    crashes: HashMap<String, Vec<f64>>,
}

impl Default for WarmPool {
    fn default() -> Self {
        Self::from_env()
    }
}

/// MAKEPAD_WM_NO_WARM disables the pool entirely. An empty or `0` value is not a
/// request — `MAKEPAD_WM_NO_WARM=` in a stale profile should not silently cost
/// everyone the feature.
pub fn warm_enabled(no_warm: Option<&str>) -> bool {
    match no_warm {
        None => true,
        Some(v) => matches!(v.trim(), "" | "0"),
    }
}

impl WarmPool {
    pub fn from_env() -> Self {
        // A build without processes has nothing to keep warm.
        Self::new(host::processes_available() && crate::octosense::policy::requested("--prewarm") && warm_enabled(std::env::var("MAKEPAD_WM_NO_WARM").ok().as_deref()))
    }

    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            browser_dark: None,
            ready: HashMap::new(),
            crashes: HashMap::new(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// How many instances of this app the pool wants standing by; 0 for an
    /// app that is not pooled at all.
    pub fn capacity(app: &str) -> usize {
        WARM_CAPACITY
            .iter()
            .find(|(id, _)| *id == app)
            .map(|(_, n)| *n)
            .unwrap_or(0)
    }

    pub fn is_warm_app(app: &str) -> bool {
        Self::capacity(app) > 0
    }

    /// How many instances of this app are currently held.
    pub fn held(&self, app: &str) -> usize {
        self.ready.get(app).map(|v| v.len()).unwrap_or(0)
    }

    /// Retire only unused browsers on a light/dark change. Removing them
    /// from the adoption pool is immediate; the host closes their processes
    /// and refills after they exit. Deliberate retirement is not a crash.
    pub fn set_browser_appearance(&mut self, dark: bool) -> Vec<ClientId> {
        let previous = self.browser_dark.replace(dark);
        if previous.is_some_and(|previous| previous != dark) {
            self.ready.remove("browser").unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// Every warm client, whatever the app — the shutdown / close-all
    /// paths walk this so no pooled process is ever left behind.
    pub fn clients(&self) -> Vec<ClientId> {
        let mut all: Vec<ClientId> = self.ready.values().flatten().copied().collect();
        all.sort_unstable();
        all
    }

    pub fn holds(&self, client: ClientId) -> bool {
        self.ready.values().any(|v| v.contains(&client))
    }

    /// True while this app is under capacity and inside its crash budget:
    /// the WM may spawn one more standby instance now.
    pub fn wants(&self, app: &str, now: f64) -> bool {
        self.enabled
            && self.held(app) < Self::capacity(app)
            && self.recent_crashes(app, now) < WARM_CRASH_LIMIT
    }

    /// The next app that is short an instance, in table order — the tick
    /// tops the pool up ONE spawn at a time so a cold start never forks
    /// five cargo builds into the same target-dir lock at once.
    pub fn next_missing(&self, now: f64) -> Option<String> {
        WARM_CAPACITY
            .iter()
            .map(|(app, _)| *app)
            .find(|app| self.wants(app, now))
            .map(str::to_string)
    }

    /// A standby instance was spawned for `app`.
    pub fn note_spawned(&mut self, app: &str, client: ClientId) {
        self.ready.entry(app.to_string()).or_default().push(client);
    }

    /// A warm instance died on its own. Counted against the crash budget;
    /// a DELIBERATE close (WM shutdown, close-all) calls `forget` instead.
    pub fn note_crash(&mut self, app: &str, now: f64) {
        self.crashes.entry(app.to_string()).or_default().push(now);
    }

    fn recent_crashes(&self, app: &str, now: f64) -> usize {
        self.crashes
            .get(app)
            .map(|v| {
                v.iter()
                    .filter(|t| now - **t < WARM_CRASH_WINDOW)
                    .count()
            })
            .unwrap_or(0)
    }

    /// Drop a client from the pool however it left (died, was closed with
    /// everything else). Returns the app it was standing by for, which is
    /// the app the caller then tops back up.
    pub fn forget(&mut self, client: ClientId) -> Option<String> {
        let mut which = None;
        for (app, ids) in self.ready.iter_mut() {
            if let Some(pos) = ids.iter().position(|c| *c == client) {
                ids.remove(pos);
                which = Some(app.clone());
                break;
            }
        }
        which
    }

    /// THE decision, for one launch of `app`.
    ///
    /// `Some(client)` = adopt that instance into a real tile (it leaves the
    /// pool; the caller tops the app back up immediately). `None` = spawn
    /// cold exactly as before. Dead entries are pruned on the way past, so
    /// a crashed instance both falls back cleanly AND frees its slot for
    /// the next respawn.
    ///
    /// THE CWD CARVE-OUT (omarchy's rule, `omarchy-cmd-terminal-cwd`): a
    /// new terminal opens in the FOCUSED terminal's directory. A warm
    /// terminal's shell started long ago, in the default directory — it
    /// cannot be moved after the fact without lying about where it is — so
    /// when a cwd is being inherited the pool stands aside and the launch
    /// goes cold. Correct beats instant; the instant path is what you get
    /// from the desktop, the bar and any non-terminal focus.
    pub fn adopt(
        &mut self,
        app: &str,
        cwd_override: bool,
        status: &[WarmStatus],
    ) -> Option<ClientId> {
        if !self.enabled {
            return None;
        }
        let ids = self.ready.get_mut(app)?;
        ids.retain(|id| {
            status
                .iter()
                .any(|s| s.client == *id && s.alive)
        });
        if cwd_override {
            return None;
        }
        let pos = ids.iter().position(|id| {
            status
                .iter()
                .any(|s| s.client == *id && s.connected)
        })?;
        Some(ids.remove(pos))
    }
}

pub struct ClientSlot {
    #[allow(dead_code)]
    pub id: ClientId,
    /// Registry id of the app this client runs.
    pub app: String,
    pub title: String,
    /// The process behind the slot: the build (`cargo build`) while
    /// `pending` holds the app, then the app itself.
    pub child: Option<ProcessGroup>,
    /// The app a launch starts once its build succeeded
    /// ([`ClientSlot::continue_launch`]).
    pending: Option<PendingApp>,
    /// Why the app could not start after its build (shown on failure).
    pub launch_error: Option<String>,
    task_pool: Option<TaskPool>,
    pub sender: Option<Sender<Vec<u8>>>,
    pub socket: Option<u64>,
    /// The child's main window id in the studio protocol (0 until
    /// CreateWindow says otherwise).
    pub window_id: usize,
    /// CreateWindow arrived: the child is ready for a swapchain.
    pub ready: bool,
    /// Working directory reported by the child (terminals, via OSC 7).
    pub pwd: Option<PathBuf>,
    /// Opened as a Quick-Look preview: a centered float that Escape or
    /// Space dismisses.
    pub is_preview: bool,
    /// A DORMANT warm-pool instance (see `WarmPool`): the process is up,
    /// connected and drawing into its own off-desk framebuffer, but it has
    /// NO tile. Everything the desk enumerates works off the LAYOUT, which
    /// a warm client is never in, so this flag is only needed where the WM
    /// walks the client table itself — launch-or-focus matching, and the
    /// tile plumbing that must stay away until adoption.
    pub warm: bool,
    /// When this client was opened as a real window (launched cold, or
    /// adopted out of the pool) and whether that open was the warm path —
    /// the pair behind the "first frame in Nms" log line that measures the
    /// pool honestly.
    pub open_at: Option<f64>,
    pub opened_warm: bool,
    /// FOCUS RULE: a Quick-Look preview never takes key focus — keys keep
    /// flowing to the requesting tile (files). `focus_client` refuses to
    /// focus a client with this false; every normal client defaults true.
    pub takes_focus: bool,
    /// Launched through cargo, so the tile can say "building…" until the
    /// child actually connects.
    #[allow(dead_code)]
    pub via_cargo: bool,
    /// The newest line the child (or cargo) wrote, shown on the tile
    /// under "starting…" until the first frame arrives.
    pub status: String,
    /// Last output before presentation filtering, retained for failure feedback.
    pub diagnostic: String,
    pub log_path: Option<PathBuf>,
    /// cargo has finished linking and handed over: the child's first exec
    /// is the one macOS scans.
    pub linked: bool,
    pub linked_at: Option<f64>,
    /// A polite close was sent at this instant (omarchy's
    /// `hl.dsp.window.close()`); the hard kill is only the fallback.
    pub closing: Option<f64>,
    /// The aichat child seated in the AI pane: not in the layout, no tile.
    pub pane: bool,
    /// On the phone, this client owns the left and right edges of its
    /// viewport (a map that pans from the edge): the shell's back gesture
    /// is not recognised over it. Off by default; an app opts in.
    pub owns_edges: bool,
    /// Its process died unexpectedly: the tile stays, closed, with a
    /// Restart (ADR 0004 §2); nothing runs behind it any more.
    pub stopped: bool,
}

impl ClientSlot {
    /// The slot of an IN-PROCESS module instance (aicontrol §3): a window
    /// in the layout like any other — the bar, alt-tab and the `os` service
    /// see it — with no process behind it: no child, no socket, no build.
    pub fn module(id: ClientId, app: &str, title: &str) -> ClientSlot {
        ClientSlot {
            id,
            app: app.to_string(),
            title: title.to_string(),
            child: None,
            pending: None,
            launch_error: None,
            task_pool: None,
            sender: None,
            socket: None,
            window_id: 0,
            ready: true,
            pwd: None,
            is_preview: false,
            warm: false,
            open_at: Some(host::now()),
            opened_warm: false,
            takes_focus: true,
            owns_edges: false,
            via_cargo: false,
            status: String::new(),
            diagnostic: String::new(),
            log_path: None,
            linked: false,
            linked_at: None,
            closing: None,
            pane: false,
            stopped: false,
        }
    }
}

/// How long a client gets to honor a close request before it is killed.
pub const CLOSE_GRACE: std::time::Duration = std::time::Duration::from_millis(1500);

/// SIGTERM-to-SIGKILL escalation gap inside `kill_child_group`, once a
/// caller has already decided to hard-kill (past `CLOSE_GRACE`, or the
/// client never got that far — still building when it was closed).
pub const GROUP_KILL_GRACE: std::time::Duration = std::time::Duration::from_millis(300);

/// Put `cmd`'s child at the head of a brand-new process group (unix only):
/// `process_group(0)` is `setpgid(0, 0)` before exec, so the pgid becomes
/// the child's own pid. Every process it forks (rustc under a build, a
/// helper the app starts) inherits that same pgid, so the whole tree can be
/// reached by one negative-pid signal later.
#[cfg(unix)]
fn own_process_group(cmd: &mut Command) {
    cmd.process_group(0);
}

/// `kill(2)` by hand — this crate has no `libc` dependency, and a
/// two-liner beats pulling one in for a single syscall pair.
#[cfg(unix)]
mod signal {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    pub const SIGTERM: i32 = 15;
    pub const SIGKILL: i32 = 9;

    /// Signal the whole process group led by `pid` — the POSIX convention
    /// of a negative pid.
    pub fn kill_group(pid: i32, sig: i32) {
        unsafe { kill(-pid, sig) };
    }

    /// `kill(pid, 0)` sends nothing. A negative pid checks the entire
    /// process group, including descendants whose leader has been reaped.
    pub fn alive(pid: i32) -> bool {
        unsafe { kill(pid, 0) == 0 }
    }
}

/// What a [`ProcessGroup`] shares with the escalations it starts: its
/// leader's pid and whether that leader has been reaped.
#[derive(Debug)]
struct GroupState {
    pid: i32,
    /// Held while the leader is reaped and while the group is signalled, so
    /// no signal ever goes out after the reap.
    reaped: std::sync::Mutex<bool>,
    /// Group signals sent, for the tests.
    sent: std::sync::atomic::AtomicUsize,
}

impl GroupState {
    /// Signal the group, unless its leader was reaped: from then on its pid,
    /// and with it the group id, may name someone else's process group.
    #[cfg_attr(not(unix), allow(unused_variables))]
    fn signal(&self, sig: i32, only_if_alive: bool) -> bool {
        let reaped = self.reaped.lock().unwrap_or_else(|e| e.into_inner());
        if *reaped {
            return false;
        }
        #[cfg(unix)]
        {
            if only_if_alive && !signal::alive(-self.pid) {
                return false;
            }
            signal::kill_group(self.pid, sig);
            self.sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            true
        }
        #[cfg(not(unix))]
        false
    }
}

/// A child at the head of its own process group (unix), and the rule for
/// signalling that group: **only until its leader is reaped**. An unreaped
/// leader (running, or a zombie) holds its pid, so the group id is still
/// this group's. The moment [`ProcessGroup::try_wait`] or
/// [`ProcessGroup::wait`] reaps it, whatever the group still holds is killed
/// at once (the leader's leftovers: nothing of an ended app outlives it),
/// and after that nothing, not a close, not a slot's drop, not an
/// escalation already under way, signals that group id again.
#[derive(Debug)]
pub struct ProcessGroup {
    child: Child,
    state: std::sync::Arc<GroupState>,
}

impl ProcessGroup {
    pub fn new(child: Child) -> ProcessGroup {
        let pid = child.id() as i32;
        ProcessGroup { child, state: std::sync::Arc::new(GroupState { pid, reaped: std::sync::Mutex::new(false), sent: Default::default() }) }
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// The leader's exit status, reaping it if it has exited (then the
    /// group's leftovers are killed, and the group is never signalled again).
    pub fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        let mut reaped = self.state.reaped.lock().unwrap_or_else(|e| e.into_inner());
        let status = self.child.try_wait();
        if !*reaped && matches!(status, Ok(Some(_))) {
            Self::sweep(&self.state);
            *reaped = true;
        }
        status
    }

    /// Block until the leader exits, then as [`ProcessGroup::try_wait`].
    pub fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        let mut reaped = self.state.reaped.lock().unwrap_or_else(|e| e.into_inner());
        let status = self.child.wait();
        if !*reaped && status.is_ok() {
            Self::sweep(&self.state);
            *reaped = true;
        }
        status
    }

    /// Right after the reap: whatever is left in the group. The group id is
    /// still ours while anything is left in it (a group id is not reused
    /// while its group exists).
    #[cfg_attr(not(unix), allow(unused_variables))]
    fn sweep(state: &GroupState) {
        #[cfg(unix)]
        if signal::alive(-state.pid) {
            signal::kill_group(state.pid, signal::SIGKILL);
            state.sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// SIGKILL to the group now (the child alone on Windows); nothing once
    /// the leader has been reaped.
    pub fn kill_now(&mut self) {
        #[cfg(unix)]
        self.state.signal(signal::SIGKILL, false);
        #[cfg(not(unix))]
        if !self.reaped() {
            let _ = self.child.kill();
        }
    }

    /// Whether the leader has been reaped.
    pub fn reaped(&self) -> bool {
        *self.state.reaped.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// How many group signals were sent (tests).
    #[cfg(test)]
    fn signals_sent(&self) -> usize {
        self.state.sent.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Kill the whole process group a `spawn_client` child heads — the fix for
/// the leak `Child::kill()` had through a wrapper (then `cargo run`, which
/// left the app and any still-building rustc running as orphans). SIGTERM
/// now, SIGKILL after `grace` for whatever is still alive; the escalation
/// runs off-thread so a UI-thread caller never blocks on it, and it never
/// fires once the leader has been reaped ([`ProcessGroup`]). Windows keeps
/// the plain `Child::kill()` this replaced.
#[cfg(unix)]
pub fn kill_child_group(child: &mut ProcessGroup, grace: std::time::Duration, pool: &TaskPool) {
    if !child.state.signal(signal::SIGTERM, false) {
        return;
    }
    let state = child.state.clone();
    let wait = CancellationToken::new();
    let submitted = pool.submit(Lane::Heavy, move || {
        let _ = wait.wait_until(Cx::monotonic_now() + grace.as_secs_f64());
        state.signal(signal::SIGKILL, true);
    });
    match submitted {
        Ok(task) => task.detach(),
        Err(_) => {
            child.state.signal(signal::SIGKILL, true);
        }
    }
}

#[cfg(not(unix))]
pub fn kill_child_group(child: &mut ProcessGroup, _grace: std::time::Duration, _pool: &TaskPool) {
    if !child.reaped() {
        let _ = child.child.kill();
    }
}

/// Final slot teardown owns the child from here on. Signal and reap it wholly
/// on a heavy pool worker; dropping a slot on the UI thread never waits for a
/// process or decoder wrapper to exit. A leader reaped already (the tick saw
/// it exit) is left alone: its group id may be someone else's by now.
fn reap_child_group(mut child: ProcessGroup, _grace: std::time::Duration, pool: &TaskPool) {
    if child.reaped() {
        return;
    }
    #[cfg(unix)]
    child.state.signal(signal::SIGTERM, false);
    match pool.reserve(Lane::Heavy) {
        Ok(slot) => slot
            .submit(move || {
                #[cfg(unix)]
                {
                    let wait = CancellationToken::new();
                    let _ = wait.wait_until(Cx::monotonic_now() + _grace.as_secs_f64());
                    child.state.signal(signal::SIGKILL, true);
                }
                #[cfg(not(unix))]
                let _ = child.child.kill();
                let _ = child.wait();
            })
            .detach(),
        Err(_) => {
            #[cfg(unix)]
            child.state.signal(signal::SIGKILL, true);
            #[cfg(not(unix))]
            let _ = child.child.kill();
        }
    }
}

/// Final application shutdown cannot depend on UI ticks, destructors, or
/// detached workers. Stop every group together and reap the direct children
/// within one shared deadline, including builds that never connected.
pub fn shutdown_clients(clients: &mut HashMap<ClientId, ClientSlot>) {
    let mut children: Vec<ProcessGroup> = Vec::new();
    let mut groups: Vec<i32> = Vec::new();
    for slot in clients.values_mut() {
        if let Some(sender) = slot.sender.take() {
            crate::hub::send_to_app(&sender, vec![makepad_studio_protocol::StudioToApp::Kill]);
        }
        slot.pending = None;
        if let Some(child) = slot.child.take() {
            #[cfg(not(unix))]
            let mut child = child;
            #[cfg(unix)]
            child.state.signal(signal::SIGTERM, false);
            #[cfg(not(unix))]
            if !child.reaped() {
                let _ = child.child.kill();
            }
            groups.push(child.id() as i32);
            children.push(child);
        }
        slot.task_pool = None;
    }
    // No remaining slot owns a child or sender, so Drop cannot queue cleanup.
    clients.clear();
    for (phase, grace) in [GROUP_KILL_GRACE, std::time::Duration::from_secs(2)].into_iter().enumerate() {
        #[cfg(unix)]
        if phase == 1 {
            for child in &children {
                child.state.signal(signal::SIGKILL, true);
            }
        }
        #[cfg(not(unix))]
        let _ = phase;
        let deadline = std::time::Instant::now() + grace;
        loop {
            // Reaping sweeps each group's leftovers (ProcessGroup::try_wait).
            children.retain_mut(|child| match child.try_wait() {
                Ok(Some(_)) => false,
                Ok(None) => true,
                Err(error) => {
                    makepad_widgets::log!("octosense: could not reap child {}: {error}", child.id());
                    false
                }
            });
            // Only looked at, never signalled: the swept leftovers going.
            #[cfg(unix)]
            let group_alive = groups.iter().any(|pid| signal::alive(-*pid));
            #[cfg(not(unix))]
            let group_alive = { let _ = &groups; false };
            if children.is_empty() && !group_alive { return; }
            if std::time::Instant::now() >= deadline { break; }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    makepad_widgets::log!("octosense: child cleanup reached its shutdown deadline after termination");
}

impl ClientSlot {
    pub fn exit_failure(&self, status: std::process::ExitStatus, label: &str) -> Option<String> {
        if self.closing.is_some() || self.warm || (self.ready && status.success()) {
            return None;
        }
        let mut message = match &self.launch_error {
            Some(why) => format!("{label} did not start: {why}."),
            None => {
                let reason = if self.ready { "exited unexpectedly" } else { "exited before opening a window" };
                format!("{label} {reason} ({status}).")
            }
        };
        if !self.diagnostic.is_empty() {
            message.push('\n');
            message.extend(self.diagnostic.chars().take(400));
        }
        if let Some(path) = &self.log_path {
            message.push_str(&format!("\nLog: {}", path.display()));
        }
        Some(message)
    }

    /// Whether an exit leaves this client's tile in place, closed with a
    /// Restart: an unexpected death (`failure`) of a window the person
    /// has. A warm instance, the AI pane, a Quick-Look preview and a
    /// client being closed go away as before.
    pub fn stops_in_place(&self, failure: bool) -> bool {
        failure && self.ready && !self.warm && !self.pane && !self.is_preview && self.closing.is_none() && self.child.is_some()
    }

    pub fn display_title(&self) -> &str {
        if self.title.is_empty() {
            &self.app
        } else {
            &self.title
        }
    }
}

/// One line of a child's output, on its way to the tile.
#[derive(Clone, Debug)]
pub struct ClientLine {
    pub client: ClientId,
    pub text: String,
}

/// Cargo (and rustc) paint their progress; a pipe usually turns that off,
/// but a stray CSI sequence must never reach a Label.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            if c != '\r' {
                out.push(c);
            }
            continue;
        }
        // ESC [ … <final byte 0x40..0x7e>, or ESC ] … BEL (OSC).
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                for c in chars.by_ref() {
                    if c == '\u{7}' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Read a child stream line by line into the log file and the UI channel.
fn pump<R: std::io::Read + Send + 'static>(
    spawner: &ThreadSpawner,
    client: ClientId,
    stream: R,
    mut log: Option<std::fs::File>,
    lines: Sender<ClientLine>,
) {
    // Each pipe lives for the child's entire lifetime. A blocking reader
    // must not occupy a finite pool worker: enough open apps would starve
    // new compile logs and even process cleanup.
    let submitted = spawner.spawn_worker(ThreadOptions {
        name: Some(format!("wm-client-{client}-output").into()),
        ..Default::default()
    }, move || {
        use std::io::{BufRead, BufReader, Write};
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if let Some(file) = log.as_mut() {
                let _ = writeln!(file, "{}", line);
            }
            let text = strip_ansi(&line).trim().to_string();
            if text.is_empty() {
                continue;
            }
            if lines.send(ClientLine { client, text }).is_err() {
                break;
            }
            SignalToUI::set_ui_signal();
        }
    });
    match submitted {
        Ok(task) => task.detach(),
        Err(error) => makepad_widgets::log!("wm: could not queue client output pump: {error}"),
    }
}

/// Cargo output that changes the launch panel. Compiler diagnostics stay in
/// the client log and do not overwrite a useful build stage with source text.
pub fn cargo_progress(raw: &str) -> Option<(String, bool)> {
    let raw = raw.trim();
    if raw.starts_with("Blocking waiting for file lock") {
        Some(("waiting for another build…".into(), false))
    } else if raw.starts_with("Running ") || raw.starts_with("Finished ") {
        Some(("launching…".into(), true))
    } else if let Some(rest) = raw.strip_prefix("Compiling ") {
        let package = rest.split(" (").next().unwrap_or(rest).trim();
        Some((format!("compiling {package}…"), false))
    } else if raw.starts_with("error:") || raw.starts_with("error[") {
        Some(("build failed — see the app log".into(), false))
    } else {
        None
    }
}

/// The build a process launch runs before its app starts, outside any
/// sandbox (ADR 0004 §3): `cargo build --release`, never `cargo run`.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildStep {
    pub cargo: PathBuf,
    pub args: Vec<String>,
    /// Cargo's working directory, which decides the `.cargo/config.toml`
    /// and toolchain files it reads: always the checkout (the OctoSense
    /// workspace for a native app), never the directory the app opens in.
    pub dir: PathBuf,
    /// The checkout it builds from.
    pub checkout: PathBuf,
    pub target_dir: PathBuf,
}

/// How one launch starts: the build, if any, then the app itself.
#[derive(Clone, Debug, PartialEq)]
pub struct LaunchPlan {
    pub build: Option<BuildStep>,
    /// The app's binary: what the build produces, or an installed one.
    pub program: PathBuf,
    /// Its arguments: `--stdin-loop`, the row's own, then the launch's.
    pub args: Vec<String>,
}

/// The workspace member a native app's build selects with the app's crate
/// (`crates/process-apps`, generated from `native-apps.json`): it carries the
/// features the app's binary needs (`bin_features`), which cargo cannot be
/// given for a crate outside the workspace. `--bin` keeps its empty library
/// out of the build.
pub const PROCESS_APPS: &str = "octosense-process-apps";

/// A binary's file name on this platform.
fn exe_name(bin: &str) -> String {
    if cfg!(windows) { format!("{bin}.exe") } else { bin.to_string() }
}

/// The Cargo workspace `dir` belongs to: the outermost `Cargo.toml` with a
/// `[workspace]` table at or above it (`dir` itself when there is none).
pub fn workspace_root(dir: &Path) -> PathBuf {
    dir.ancestors()
        .filter(|at| {
            std::fs::read_to_string(at.join("Cargo.toml"))
                .is_ok_and(|toml| toml.lines().any(|line| line.trim() == "[workspace]"))
        })
        .last()
        .unwrap_or(dir)
        .to_path_buf()
}

/// `CARGO_TARGET_DIR` as cargo would read it from `dir`.
fn env_target_dir(dir: &Path) -> Option<PathBuf> {
    std::env::var_os("CARGO_TARGET_DIR").filter(|v| !v.is_empty()).map(|v| dir.join(v))
}

/// The launch of `app` (the release-only law: a hosted app is always a
/// release build).
///
/// - **A native app** (`native-apps.json` with a `bin`) in a checkout is
///   built from the OctoSense workspace, held to its `Cargo.lock`:
///   `cargo build --release --locked --manifest-path <root>/Cargo.toml -p
///   octosense-process-apps -p <crate> --bin <bin>`, the crate and binary
///   the manifest names, with the features its binary needs (never
///   the catalog row's), into the workspace's target dir, with cargo
///   running in the workspace. The Terminal's crate is a workspace
///   dependency, so it builds with the pinned versions the shell does.
/// - **Any other catalog row** with a manifest builds from that manifest's
///   checkout the same way; `--locked` only where that checkout has a lock
///   (Makepad ignores its own, and `--locked` would refuse to create one).
/// - **No checkout**: the binary beside the shell's own.
///
/// Then the built binary itself starts, under the app's sandbox
/// ([`spawn_client`]); cargo never starts the app.
pub fn launch_plan(app: &AppDef, root: Option<&Path>, extra_args: &[String]) -> Result<LaunchPlan, String> {
    let mut args = vec!["--stdin-loop".to_string()];
    args.extend(app.args.iter().cloned());
    args.extend(extra_args.iter().cloned());
    if app.package.is_empty() {
        let program = resolve_bin(&app.bin).ok_or_else(|| format!("binary not found: {}", app.bin))?;
        return Ok(LaunchPlan { build: None, program, args });
    }
    let native = crate::native_apps::find(&app.id).and_then(|n| Some((crate::native_apps::package_of(n.id)?, n.bin?)));
    let manifest_root = app.manifest.as_ref().and_then(|m| Path::new(m).parent()).map(Path::to_path_buf);
    let (checkout, manifest, package, bin, target_dir, locked, with) = match (native, root) {
        (Some((package, bin)), Some(root)) => {
            // The desktop package lives in the workspace (`desktop/`): build
            // from the workspace itself, where its lock and target dir are.
            let root = &workspace_root(root);
            let target = env_target_dir(root).unwrap_or_else(|| root.join("target"));
            (root.to_path_buf(), root.join("Cargo.toml"), package.to_string(), bin.to_string(), target, true, Some(PROCESS_APPS))
        }
        _ => {
            let Some(checkout) = manifest_root.clone().or_else(|| root.map(Path::to_path_buf)) else {
                let program = resolve_bin(&app.bin).ok_or_else(|| format!("binary not found: {}", app.bin))?;
                return Ok(LaunchPlan { build: None, program, args });
            };
            let manifest = match &app.manifest {
                Some(m) => PathBuf::from(m),
                None => checkout.join("Cargo.toml"),
            };
            let target = app.target_dir.as_ref().map(PathBuf::from).or_else(|| env_target_dir(&checkout)).unwrap_or_else(|| checkout.join("target"));
            let locked = checkout.join("Cargo.lock").is_file();
            (checkout, manifest, app.package.clone(), app.bin.clone(), target, locked, None)
        }
    };
    let mut build = vec!["build".to_string(), "--release".to_string()];
    if locked {
        build.push("--locked".to_string());
    }
    build.push("--manifest-path".to_string());
    build.push(manifest.to_string_lossy().into_owned());
    if let Some(with) = with {
        build.push("-p".to_string());
        build.push(with.to_string());
    }
    for arg in ["-p".to_string(), package, "--bin".into(), bin.clone()] {
        build.push(arg);
    }
    build.push("--target-dir".to_string());
    build.push(target_dir.to_string_lossy().into_owned());
    let program = target_dir.join("release").join(exe_name(&bin));
    Ok(LaunchPlan {
        build: Some(BuildStep { cargo: cargo_bin(), args: build, dir: checkout.clone(), checkout, target_dir }),
        program,
        args,
    })
}

/// Everything cargo reads configuration or a toolchain from on the way from
/// `dir` to the root: `.cargo/` (config and credentials) and
/// `rust-toolchain(.toml)` in `dir` and each of its ancestors.
pub fn cargo_config_paths(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for at in dir.ancestors() {
        out.push(at.join(".cargo"));
        out.push(at.join("rust-toolchain"));
        out.push(at.join("rust-toolchain.toml"));
    }
    out
}

/// The build's `PATH`: the shell's, without the entries inside the
/// person's home a sandboxed app could write (the Terminal's `home:rw`),
/// except the toolchain's own read-only homes (`keep`). A build never runs a
/// program such an app could have put there.
pub fn build_path(path: &std::ffi::OsStr, home: &Path, keep: &[PathBuf]) -> std::ffi::OsString {
    let kept: Vec<PathBuf> = std::env::split_paths(path)
        .filter(|entry| !entry.starts_with(home) || keep.iter().any(|k| entry.starts_with(k)))
        .collect();
    std::env::join_paths(kept).unwrap_or_default()
}

/// The toolchain's homes: `CARGO_HOME` and `RUSTUP_HOME` (or their
/// defaults under the person's home).
fn toolchain_homes(home: &Path) -> (PathBuf, PathBuf) {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    (var("CARGO_HOME").unwrap_or_else(|| home.join(".cargo")), var("RUSTUP_HOME").unwrap_or_else(|| home.join(".rustup")))
}

/// The build's command: in its checkout, with the shell's allow-listed
/// environment and a `PATH` no sandboxed app can have written to.
fn build_command(build: &BuildStep) -> Command {
    let mut cmd = Command::new(&build.cargo);
    cmd.args(&build.args).current_dir(&build.dir);
    // Cargo colors its output when it thinks a terminal is watching; the
    // pipe already turns that off, and this makes it certain.
    cmd.env("CARGO_TERM_COLOR", "never");
    if let Some(home) = crate::sandbox::person_home() {
        let (cargo_home, rustup_home) = toolchain_homes(&home);
        if let Some(path) = std::env::var_os("PATH") {
            cmd.env("PATH", build_path(&path, &home, &[cargo_home, rustup_home]));
        }
    }
    // rustc beside the cargo it runs (both read-only to every app), not
    // whichever `rustc` a PATH lookup finds.
    if std::env::var_os("RUSTC").is_none() {
        if let Some(rustc) = build.cargo.parent().map(|d| d.join(exe_name("rustc"))).filter(|r| r.is_file()) {
            cmd.env("RUSTC", rustc);
        }
    }
    crate::sandbox::scrub_env(&mut cmd);
    cmd
}

/// The sandbox of a native app's process launch (`None` for a catalog app
/// that is not in `native-apps.json`). Its jail and secrets folders are
/// created first. Its program and resources (readable, never writable) are
/// the checkout it was built from, the target dir and cargo's source cache
/// (crate resources) or, installed, the binary's own directory. Everything
/// the next build reads or runs is kept read-only whatever the manifest
/// grants ([`crate::sandbox::Policy::read_only`]): the checkout, the target
/// dir, the cargo and rustup homes, every `.cargo/` and toolchain file on
/// the way up from the build's directory, and the shell's own directory.
pub fn sandbox_policy(app: &AppDef, plan: &LaunchPlan, hub_port: u16) -> Option<crate::sandbox::Policy> {
    let native = crate::native_apps::find(&app.id)?;
    let layout = match crate::app_storage::host() {
        Some(host) => host.layout().clone(),
        None => crate::app_storage::Layout::platform(None).ok()?,
    };
    let paths = layout.app(native.id).ok()?;
    for (base, dir) in [(layout.apps_root(), &paths.jail), (layout.secrets_root(), &paths.secrets)] {
        if let Err(e) = crate::app_storage::ensure_private_dir(base, dir) {
            makepad_widgets::log!("sandbox: {}: cannot prepare {}: {e}", native.id, dir.display());
        }
    }
    // Its declared layout (accounts, common, cache) and its quota, measured
    // by the shell: the sandbox cannot count bytes (ADR 0004 §11).
    if let Some(host) = crate::app_storage::host() {
        match host.open(native.id) {
            Ok(_) => crate::app_storage::lifecycle::check_quota_later(host, native.id),
            Err(e) => makepad_widgets::log!("sandbox: {}: {e}", native.id),
        }
    }
    let home = crate::sandbox::person_home().unwrap_or_else(|| PathBuf::from("/nonexistent"));
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut read_only: Vec<PathBuf> = Vec::new();
    if let Some(build) = &plan.build {
        let (cargo_home, rustup_home) = toolchain_homes(&home);
        roots.push(build.checkout.clone());
        roots.push(build.target_dir.clone());
        roots.push(cargo_home.join("git"));
        roots.push(cargo_home.join("registry"));
        read_only.extend([build.checkout.clone(), build.target_dir.clone(), cargo_home, rustup_home]);
        read_only.extend(cargo_config_paths(&build.dir));
    }
    if let Some(dir) = plan.program.parent() {
        roots.push(dir.to_path_buf());
        read_only.push(dir.to_path_buf());
    }
    // The shell's own directory: an installed launch's sibling binary, and
    // the shell's binary itself.
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        roots.push(dir.clone());
        read_only.push(dir);
    }
    roots.sort();
    roots.dedup();
    read_only.sort();
    read_only.dedup();
    let mut policy = crate::sandbox::Policy::for_app(native, paths.jail, paths.secrets, &home, roots, hub_port);
    // The host's private directories stay closed whatever the manifest
    // grants (G6): the OctoSense home (peer host tokens, every app's jail
    // and secrets), the storage roots and the kernel's core dir.
    let core = crate::ai_host::core_dir(None);
    policy.private = crate::sandbox::host_private_dirs(&crate::octosense::paths::home(), layout.apps_root(), layout.secrets_root(), core.as_deref());
    policy.read_only = read_only;
    Some(if crate::sandbox::narrowed_by_env(native.id) { policy.narrowed() } else { policy })
}

/// The app a launch starts once its build has finished: its command, ready
/// (sandbox, environment, working directory), and where its output goes.
struct PendingApp {
    cmd: Command,
    spawner: ThreadSpawner,
    lines: Sender<ClientLine>,
    log: Option<std::fs::File>,
    /// The hub's secret for this launch, written to the app's stdin.
    launch_token: String,
}

impl std::fmt::Debug for PendingApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingApp").field("cmd", &self.cmd).finish_non_exhaustive()
    }
}

/// Start `cmd` at the head of its own process group with both streams
/// piped into the client's log and tile. `stdin_line`: the app's hub
/// secret, written as the only line of its stdin (then EOF); `None` (a
/// build): no stdin at all.
fn start_piped(
    mut cmd: Command,
    spawner: &ThreadSpawner,
    id: ClientId,
    log: Option<std::fs::File>,
    lines: Sender<ClientLine>,
    stdin_line: Option<&str>,
) -> std::io::Result<ProcessGroup> {
    // Its own group (unix): whatever it forks (rustc under a build) shares
    // one fresh pgid, so `kill_child_group` reaches the whole tree.
    #[cfg(unix)]
    own_process_group(&mut cmd);
    // Both streams are piped so a reader thread can put the newest line on
    // the tile while the app builds — cargo talks on stderr.
    cmd.stdin(if stdin_line.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    if let (Some(line), Some(mut stdin)) = (stdin_line, child.stdin.take()) {
        use std::io::Write;
        // One line, then EOF (the pipe closes here): the app reads nothing
        // else from us.
        let _ = stdin.write_all(format!("{line}\n").as_bytes());
    }
    if let Some(out) = child.stdout.take() {
        pump(spawner, id, out, log.as_ref().and_then(|f| f.try_clone().ok()), lines.clone());
    }
    if let Some(err) = child.stderr.take() {
        pump(spawner, id, err, log, lines);
    }
    Ok(ProcessGroup::new(child))
}

/// Where a client's output is kept: `<OctoSense home>/logs/clients/`,
/// which no sandboxed app can read or write (it was the shared temp dir).
pub fn client_log_path(id: ClientId) -> PathBuf {
    let dir = crate::octosense::paths::private_dir("logs/clients").unwrap_or_else(|_| host::homeless_root());
    dir.join(format!("octosense-{}-client-{}.log", std::process::id(), id))
}

/// Spawn an app as a hub client.
pub fn spawn_client(
    pool: &TaskPool,
    spawner: &ThreadSpawner,
    app: &AppDef,
    id: ClientId,
    hub_port: u16,
    cwd: Option<&PathBuf>,
    term_colors: Option<&str>,
    // `extra_args` is appended after the app's own args: the file to open,
    // with `--preview` in front of it for a Quick-Look popup.
    extra_args: &[String],
    // A DORMANT warm-pool instance: same launch in every other way — same
    // build, same env, same log — plus `WARM_ENV`, which tells the app to
    // come up and then idle until it is adopted.
    warm: bool,
    // Every output line the child writes is forwarded here, so the tile
    // can show cargo's progress instead of a bare "starting…".
    lines: Sender<ClientLine>,
) -> Result<ClientSlot, String> {
    let root = repo_root();
    let plan = launch_plan(app, root.as_deref(), extra_args)?;
    let via_cargo = plan.build.is_some();
    // A native app runs under the OS sandbox its manifest entry builds
    // (ADR 0004 §3, sandbox/): the built binary itself, never the build.
    let policy = sandbox_policy(app, &plan, hub_port);
    let (mut cmd, applied) = crate::sandbox::command(&plan.program, &plan.args, policy.as_ref(), false);
    // A sandboxed app's Makepad home is its own jail (ADR 0004 §11): the
    // OctoSense home is closed to it (G6), so settings it kept under
    // `<OctoSense home>/<app id>/` could no longer be written. Its old data
    // is copied into the jail once, by the host.
    if let Some(policy) = &policy {
        // Started at startup off the UI thread (`app_storage::init`); a
        // launch waits only for a copy still running, and copies itself
        // only when none was started.
        let (legacy, into) = (crate::octosense::paths::home().join(&app.id), policy.jail.join(&app.id));
        wait_adopted(&into);
        adopt_legacy_app_home(&legacy, &into);
        cmd.env("MAKEPAD_HOME", &policy.jail);
    }
    match &applied {
        Some(crate::sandbox::Applied::Sandboxed(how)) => makepad_widgets::log!("sandbox: {how}"),
        Some(crate::sandbox::Applied::Unavailable(why)) => makepad_widgets::log!("sandbox: UNSANDBOXED {why}"),
        None => {}
    }
    crate::sandbox::note_launch(&app.id, applied.as_ref());
    // The hub admits this launch's socket only with this secret (ADR 0004
    // §5, hub.rs): it goes to the APP on its stdin (never to the build),
    // never in its environment or command line.
    let launch_token = crate::hub::issue_launch_token(id);
    cmd.env("STUDIO_HOST", format!("http://127.0.0.1:{}", hub_port))
        .env("STUDIO_BUILD", id.to_string())
        .env("STUDIO_CRATE", &app.bin)
        .env(crate::hub::HANDSHAKE_STDIN_ENV, "1");
    // The directory the app opens in is the app's alone: the build runs in
    // its checkout whatever this is.
    if let Some(cwd) = cwd.filter(|dir| dir.is_dir()) {
        // The terminal's Omarchy behavior: open where the focused one is.
        cmd.arg("--cwd").arg(cwd);
        cmd.current_dir(cwd);
    } else if let Some(build) = &plan.build {
        // Apps resolve their data (resources) relative to the checkout,
        // like a `cargo run` from it.
        cmd.current_dir(&build.checkout);
    } else if !app.dir.is_empty() {
        cmd.current_dir(&app.dir);
    } else if let Some(root) = &root {
        cmd.current_dir(root);
    }
    if let Some(colors) = term_colors {
        cmd.env("MAKEPAD_TERMINAL_COLORS", colors);
        // Truly translucent terminals over the wallpaper (the user's
        // default; omarchy gets this from ghostty background-opacity —
        // its window rule alone, 0.985/0.96, reads as opaque).
        // "focused unfocused"; MAKEPAD_WM_TERM_OPACITY overrides.
        let opacity = std::env::var("MAKEPAD_WM_TERM_OPACITY")
            .unwrap_or_else(|_| "0.78 0.70".to_string());
        cmd.env("MAKEPAD_TERMINAL_OPACITY", opacity);
    }
    // Every Makepad app styles itself from the WM's theme.splash.
    if let Ok(theme) = std::env::var("MAKEPAD_WM_THEME_SPLASH") {
        cmd.env("MAKEPAD_WM_THEME_SPLASH", theme);
    }
    if warm {
        cmd.env(WARM_ENV.0, WARM_ENV.1);
    }
    // No child inherits the kernel's descriptors or the host token: a
    // process app reaches its agent only over the peer link (ADR 0004 §3).
    crate::sandbox::scrub_env(&mut cmd);
    // Child output (and cargo's "Compiling …") goes to a per-client log —
    // silent children are undebuggable — and every line also reaches the
    // UI so the tile can show what the build is doing.
    let log_path = client_log_path(id);
    makepad_widgets::log!("octosense: client {id} log: {}", log_path.display());
    let log = std::fs::File::create(&log_path).ok();
    let (child, pending) = match &plan.build {
        Some(build) => {
            let pending = PendingApp {
                cmd,
                spawner: spawner.clone(),
                lines: lines.clone(),
                log: log.as_ref().and_then(|f| f.try_clone().ok()),
                launch_token,
            };
            let child = start_piped(build_command(build), spawner, id, log, lines, None).map_err(|e| {
                crate::hub::revoke_launch_token(id);
                format!("build {}: {}", app.package, e)
            })?;
            (child, Some(pending))
        }
        None => {
            let child = start_piped(cmd, spawner, id, log, lines, Some(&launch_token)).map_err(|e| {
                crate::hub::revoke_launch_token(id);
                format!("spawn {}: {}", app.bin, e)
            })?;
            (child, None)
        }
    };
    Ok(ClientSlot {
        id,
        app: app.id.to_string(),
        title: String::new(),
        child: Some(child),
        pending,
        launch_error: None,
        task_pool: Some(pool.clone()),
        sender: None,
        socket: None,
        window_id: 0,
        ready: false,
        pwd: None,
        is_preview: false,
        warm,
        open_at: (!warm).then(host::now),
        opened_warm: false,
        // A warm instance is not a window yet: nothing may focus it until
        // adoption hands it a tile.
        takes_focus: !warm,
        owns_edges: false,
        via_cargo,
        status: String::new(),
        diagnostic: String::new(),
        log_path: Some(log_path),
        linked: false,
        linked_at: None,
        closing: None,
        pane: false,
        stopped: false,
    })
}

type Adoption = std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>;

/// Adoptions started, by target folder: done or still copying.
fn adoptions() -> &'static std::sync::Mutex<HashMap<PathBuf, Adoption>> {
    static ADOPTIONS: std::sync::OnceLock<std::sync::Mutex<HashMap<PathBuf, Adoption>>> = std::sync::OnceLock::new();
    ADOPTIONS.get_or_init(Default::default)
}

/// [`adopt_legacy_app_home`] on its own thread (the shell starts it at
/// startup for every app that runs as a process, so a launch rarely waits),
/// once per target per run.
pub(crate) fn adopt_legacy_app_home_later(legacy: PathBuf, into: PathBuf) {
    if into.exists() || !legacy.is_dir() {
        return;
    }
    let state: Adoption = std::sync::Arc::default();
    {
        let mut all = adoptions().lock().unwrap_or_else(|e| e.into_inner());
        if all.contains_key(&into) {
            return;
        }
        all.insert(into.clone(), state.clone());
    }
    let spawned = std::thread::Builder::new().name("adopt-app-home".into()).spawn({
        let state = state.clone();
        move || {
            adopt_legacy_app_home(&legacy, &into);
            *state.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
            state.1.notify_all();
        }
    });
    if spawned.is_err() {
        *state.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
    }
}

/// At startup: adopt, in the background, the legacy home of every native
/// app that runs as a process on this target (its launch sets
/// `MAKEPAD_HOME` to its jail).
pub fn adopt_legacy_homes_later(layout: &crate::app_storage::Layout) {
    use crate::native_apps::Hosting;
    for app in crate::native_apps::APPS {
        let hosting = if cfg!(target_os = "macos") {
            app.macos
        } else if cfg!(target_os = "windows") {
            app.windows
        } else if cfg!(target_os = "linux") {
            app.linux
        } else {
            Hosting::Module
        };
        if matches!(hosting, Hosting::Module) {
            continue;
        }
        if let Ok(paths) = layout.app(app.id) {
            adopt_legacy_app_home_later(crate::octosense::paths::home().join(app.id), paths.jail.join(app.id));
        }
    }
}

impl ClientSlot {
    /// A launch whose build is still running, or whose app has not started.
    pub fn building(&self) -> bool {
        self.pending.is_some()
    }

    /// The second half of a launch: once its build has exited, start the
    /// app (its group replaces the build's), or give up. `None` while there
    /// is nothing to do (no build pending, or it is still running); the
    /// tick calls it for every slot before it reaps exits. A launch closed
    /// while building starts nothing. A failed build or start leaves the
    /// reaped build as the slot's child, so the tick reports it as a
    /// failed start, with [`ClientSlot::launch_error`] saying why.
    pub fn continue_launch(&mut self, id: ClientId) -> Option<Result<(), String>> {
        self.pending.as_ref()?;
        let status = match self.child.as_mut()?.try_wait() {
            Ok(Some(status)) => status,
            Ok(None) => return None,
            Err(e) => {
                self.pending = None;
                crate::hub::revoke_launch_token(id);
                let why = format!("could not wait for its build: {e}");
                self.launch_error = Some(why.clone());
                return Some(Err(why));
            }
        };
        let pending = self.pending.take()?;
        if self.closing.is_some() {
            // Nothing will ever present this launch's secret.
            crate::hub::revoke_launch_token(id);
            return Some(Ok(()));
        }
        if !status.success() {
            crate::hub::revoke_launch_token(id);
            let why = format!("its build failed ({status})");
            self.launch_error = Some(why.clone());
            return Some(Err(why));
        }
        match start_piped(pending.cmd, &pending.spawner, id, pending.log, pending.lines, Some(&pending.launch_token)) {
            Ok(app) => {
                self.child = Some(app);
                Some(Ok(()))
            }
            Err(e) => {
                crate::hub::revoke_launch_token(id);
                let why = format!("it could not start: {e}");
                self.launch_error = Some(why.clone());
                Some(Err(why))
            }
        }
    }
}

/// Wait for a background adoption into `into` still copying (a launch
/// needs the data); returns at once when none was started or it is done.
pub(crate) fn wait_adopted(into: &Path) {
    let Some(state) = adoptions().lock().unwrap_or_else(|e| e.into_inner()).get(into).cloned() else { return };
    let mut done = state.0.lock().unwrap_or_else(|e| e.into_inner());
    while !*done {
        done = state.1.wait(done).unwrap_or_else(|e| e.into_inner());
    }
}

/// Copies an app's data from where it lived before its jail (`legacy`) to
/// `into`, once: only when `into` does not exist yet. Files and folders
/// are copied, links skipped; the old copy stays where it was. The copy
/// goes to a staging folder beside `into` and is renamed into place only
/// when complete, so a copy that fails part way (or a crash) leaves no
/// `into`, and the next launch copies again.
pub(crate) fn adopt_legacy_app_home(legacy: &Path, into: &Path) {
    fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let target = to.join(entry.file_name());
            if kind.is_dir() {
                copy(&entry.path(), &target)?;
            } else if kind.is_file() {
                std::fs::copy(entry.path(), &target)?;
            }
        }
        Ok(())
    }
    if into.exists() || !legacy.is_dir() {
        return;
    }
    let Some(name) = into.file_name() else { return };
    let mut staging_name = name.to_os_string();
    staging_name.push(".adopting");
    let staging = into.with_file_name(staging_name);
    let _ = std::fs::remove_dir_all(&staging);
    match copy(legacy, &staging).and_then(|()| std::fs::rename(&staging, into)) {
        Ok(()) => makepad_widgets::log!("storage: moved {} into its jail ({})", legacy.display(), into.display()),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            makepad_widgets::log!("storage: could not copy {} into {} (retried at the next launch): {e}", legacy.display(), into.display());
        }
    }
}

#[cfg(test)]
mod tests {

    /// A copy that failed part way leaves nothing in the jail, so the next
    /// launch copies again (it used to leave a partial folder, and "once"
    /// then meant never).
    #[cfg(unix)]
    #[test]
    fn should_retry_the_copy_when_an_earlier_one_failed_part_way() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("octosense-adopt-retry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let legacy = root.join("home/terminal");
        let jail = root.join("home/apps/terminal/terminal");
        std::fs::create_dir_all(legacy.join("a")).unwrap();
        std::fs::write(legacy.join("a/first.conf"), "1").unwrap();
        std::fs::write(legacy.join("locked.conf"), "2").unwrap();
        std::fs::set_permissions(legacy.join("locked.conf"), std::fs::Permissions::from_mode(0o000)).unwrap();
        let readable = std::fs::read(legacy.join("locked.conf")).is_ok(); // root reads anything
        super::adopt_legacy_app_home(&legacy, &jail);
        if !readable {
            assert!(!jail.exists(), "a failed copy leaves no jail folder behind");
        }
        std::fs::set_permissions(legacy.join("locked.conf"), std::fs::Permissions::from_mode(0o600)).unwrap();
        super::adopt_legacy_app_home(&legacy, &jail);
        assert_eq!(std::fs::read_to_string(jail.join("locked.conf")).unwrap(), "2", "retried");
        assert!(jail.join("a/first.conf").is_file());
        let leftovers: Vec<_> = std::fs::read_dir(jail.parent().unwrap()).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("terminal")], "no staging folder left");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The copy runs off the UI thread; a launch waits only for a copy
    /// still running.
    #[test]
    fn should_adopt_in_the_background_and_let_a_launch_wait_for_it() {
        let root = std::env::temp_dir().join(format!("octosense-adopt-bg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let legacy = root.join("home/terminal");
        let jail = root.join("home/apps/terminal/terminal");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("settings.conf"), "x").unwrap();
        super::adopt_legacy_app_home_later(legacy.clone(), jail.clone());
        super::wait_adopted(&jail);
        assert!(jail.join("settings.conf").is_file());
        super::wait_adopted(&root.join("never-started"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_apps_legacy_home_is_copied_into_its_jail_once() {
        let root = std::env::temp_dir().join(format!("octosense-adopt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let legacy = root.join("home/terminal");
        let jail = root.join("home/apps/terminal/terminal");
        std::fs::create_dir_all(legacy.join("profiles")).unwrap();
        std::fs::write(legacy.join("settings.conf"), "font_size = 14").unwrap();
        std::fs::write(legacy.join("profiles/dark.conf"), "x").unwrap();
        super::adopt_legacy_app_home(&legacy, &jail);
        assert_eq!(std::fs::read_to_string(jail.join("settings.conf")).unwrap(), "font_size = 14");
        assert!(jail.join("profiles/dark.conf").is_file());
        assert!(legacy.join("settings.conf").is_file(), "the old copy stays");
        // Once: a later change in the jail is never overwritten.
        std::fs::write(jail.join("settings.conf"), "font_size = 16").unwrap();
        super::adopt_legacy_app_home(&legacy, &jail);
        assert_eq!(std::fs::read_to_string(jail.join("settings.conf")).unwrap(), "font_size = 16");
        // Nothing to adopt: nothing created.
        let none = root.join("home/apps/other/other");
        super::adopt_legacy_app_home(&root.join("home/other"), &none);
        assert!(!none.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
    use super::*;

    #[test]
    fn system_apps_replace_catalog_rows_and_installed_apps_never_shadow() {
        let app = |id: &str, label: &str, bin: &str| AppDef::app(id, label, "", "", bin, LaunchPolicy::OrFocus);
        let base = vec![app("browser", "Browser", "browser"), app("mail", "Makepad Mail", "mail"), app("notes", "Notes", "notes")];
        let bundled = vec![app("apphub", "App Hub", "apphub"), app("card", "Internal host", "card")];
        let system = vec![app("news", "News", "card"), app("mail", "Mail", "card")];
        let installed = vec![app("hub:demo", "Demo", "card"), app("hub:mail", "Untrusted Mail", "card")];
        let merged = merge_catalog(base, bundled, system, installed);
        assert_eq!(
            merged.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            ["browser", "mail", "notes", "apphub", "news", "hub:demo", "hub:mail"]
        );
        // The system Mail took the example's place, not its name only.
        assert_eq!((merged[1].label.as_str(), merged[1].bin.as_str()), ("Mail", "card"));
    }

    /// Cargo's checkout is Cargo's to manage: a build there is invisible to
    /// `cargo clean`, survives no refetch, and quietly grows the shared
    /// cache. Apps from the pinned revision build into OctoSense's own tree.
    #[test]
    fn makepad_launches_build_outside_cargos_checkout() {
        let apps = crate::octosense::catalog::parse_catalog(
            br#"[{"id":"browser","label":"Browser","source":"makepad","package":"makepad-browser","bin":"browser"}]"#,
            Path::new("/catalog"),
            Some(Path::new("/cargo/checkouts/makepad-d00a/ad8f372")),
        )
        .unwrap();
        let plan = launch_plan(&apps[0], Some(Path::new("/unrelated")), &[]).unwrap();
        let build = plan.build.expect("a checkout row is built first");
        assert!(build.args.windows(2).any(|p| p == ["--manifest-path", "/cargo/checkouts/makepad-d00a/ad8f372/Cargo.toml"]));
        let at = build.args.iter().position(|arg| arg == "--target-dir").expect("builds are redirected out of the cargo cache");
        assert!(!build.args[at + 1].starts_with("/cargo/checkouts"), "{}", build.args[at + 1]);
        assert_eq!(plan.program, build.target_dir.join("release").join(exe_name("browser")));
        assert_eq!(build.dir, Path::new("/cargo/checkouts/makepad-d00a/ad8f372"), "cargo runs in its checkout");
    }

    #[test]
    fn installed_catalog_refreshes_without_shadowing_native_apps() {
        let app = |id: &str, label: &str| AppDef::app(id, label, "", "", "card", LaunchPolicy::OrFocus);
        let native = vec![app("apphub", "App Hub"), app("settings", "OctoSense Settings"), app("appstore", "Apps")];
        // Installed ids stay namespaced; legacy claimed built-in ids cannot shadow a native app.
        let installed = vec![app("hub:demo", "Demo"), app("hub:apphub", "Installed App Hub"),
            app("hub:settings", "Installed Settings"), app("settings", "Untrusted replacement")];
        let bundled = vec![app("apphub", "Linked module"), app("card", "Internal host"), app("appstore", "Apps")];
        let first = merge_catalog(native.clone(), bundled.clone(), vec![], installed);
        assert_eq!(first.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["apphub", "settings", "hub:demo", "hub:apphub", "hub:settings"]);
        assert_eq!(first[0].label, "App Hub");
        assert_eq!(first[1].label, "OctoSense Settings");
        let after_remove = merge_catalog(native, bundled, vec![], vec![]);
        assert_eq!(after_remove.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["apphub", "settings"]);
    }

    #[test]
    fn catalog_launches_select_the_binary_and_preserve_literal_arguments() {
        let apps = crate::octosense::catalog::parse_catalog(br#"[
            {"id":"ref","label":"Reference","manifest":"../apps/reference/Cargo.toml","package":"octosense-reference","bin":"octosense-reference","args":["two words"]},
            {"id":"installed","label":"Installed","executable":"/usr/bin/true","args":["$(literal)"]}
        ]"#, Path::new("/catalog"), None).unwrap();
        let plan = launch_plan(&apps[0], Some(Path::new("/unrelated")), &[]).unwrap();
        let build = plan.build.unwrap();
        assert!(build.args.windows(2).any(|p| p == ["--bin", "octosense-reference"]));
        assert!(build.args.windows(2).any(|p| p == ["--manifest-path", "/catalog/../apps/reference/Cargo.toml"]));
        assert_eq!(plan.args, ["--stdin-loop", "two words"]);
        let plan = launch_plan(&apps[1], Some(Path::new("/unrelated")), &[]).unwrap();
        assert!(plan.build.is_none());
        assert_eq!(plan.program, Path::new("/usr/bin/true"));
        assert_eq!(plan.args, ["--stdin-loop", "$(literal)"]);
    }

    /// A launch's extra arguments (a URL from `WmRequest::Launch`, a file
    /// to open) come last, after `--stdin-loop` and the app's own args,
    /// for a built launch and an installed binary alike.
    #[test]
    fn launch_extra_arguments_come_after_the_apps_own() {
        let apps = crate::octosense::catalog::parse_catalog(br#"[
            {"id":"ref","label":"Reference","manifest":"../apps/reference/Cargo.toml","package":"octosense-reference","bin":"octosense-reference","args":["two words"]},
            {"id":"installed","label":"Installed","executable":"/usr/bin/true","args":["--demo"]}
        ]"#, Path::new("/catalog"), None).unwrap();
        let extra = ["https://x/a".to_string()];
        let plan = launch_plan(&apps[0], Some(Path::new("/unrelated")), &extra).unwrap();
        assert_eq!(plan.args, ["--stdin-loop", "two words", "https://x/a"]);
        let plan = launch_plan(&apps[1], None, &extra).unwrap();
        assert_eq!(plan.args, ["--stdin-loop", "--demo", "https://x/a"]);
    }

    #[test]
    fn the_default_catalog_keeps_the_local_reference_app() {
        let apps = crate::octosense::catalog::parse_catalog(include_bytes!("../../../desktop/config/apps.json"), Path::new("/catalog"), None).unwrap();
        let reference = apps.iter().find(|app| app.id == "reference").expect("Reference must remain in the default catalog");
        assert_eq!(reference.manifest.as_deref(), Some("/catalog/../../apps/reference/Cargo.toml"));
        assert_eq!(reference.package, "octosense-reference");
        assert_eq!(reference.bin, "octosense-reference");
        assert_eq!(reference.policy, LaunchPolicy::AlwaysNew);
    }

    #[test]
    fn cargo_progress_keeps_the_build_stage_readable() {
        assert_eq!(cargo_progress("   Compiling makepad-photos v0.1.0 (/a/checkout)"), Some(("compiling makepad-photos v0.1.0…".into(), false)));
        assert_eq!(cargo_progress("Blocking waiting for file lock on build directory"), Some(("waiting for another build…".into(), false)));
        assert_eq!(cargo_progress("    Finished `release` profile in 2s"), Some(("launching…".into(), true)));
        assert_eq!(cargo_progress("     Running `/a/checkout/target/release/photos`"), Some(("launching…".into(), true)));
        assert!(cargo_progress("warning: unused variable").is_none());
        assert!(cargo_progress(" --> /a/checkout/src/main.rs:2").is_none());
        assert!(cargo_progress("app: first frame").is_none());
        assert_eq!(cargo_progress("error[E0308]: type mismatch"), Some(("build failed — see the app log".into(), false)));
    }

    /// ADR 0004 §3 (review 2026-09-30): a native app is built from the
    /// OctoSense workspace, held to its lock, with the crate and binary
    /// `native-apps.json` names, whatever its catalog row says; cargo runs in
    /// the workspace (whose `.cargo/config.toml` is the one it reads), never
    /// in the directory the app opens in; and cargo only builds: the app is
    /// the built binary, started by the shell. USER LAW: a release build.
    #[test]
    fn a_native_app_builds_from_the_workspace_locked_and_starts_its_binary() {
        let mut app = AppDef::app("terminal", "Terminal", "not-the-terminal", "apps/terminal", "not-its-bin", LaunchPolicy::AlwaysNew);
        app.manifest = Some("/somewhere/else/Cargo.toml".into());
        app.target_dir = Some("/somewhere/else/target".into());
        let root = PathBuf::from("/checkout");
        let plan = launch_plan(&app, Some(&root), &["--preview".to_string(), "/a.png".to_string()]).unwrap();
        let build = plan.build.clone().expect("built first");
        let target = env_target_dir(&root).unwrap_or_else(|| root.join("target"));
        assert_eq!(
            build.args,
            ["build", "--release", "--locked", "--manifest-path", "/checkout/Cargo.toml", "-p", PROCESS_APPS, "-p", "makepad-terminal", "--bin", "terminal", "--target-dir", target.to_str().unwrap()]
        );
        assert!(build.cargo.to_string_lossy().ends_with("cargo"), "{:?}", build.cargo);
        assert_eq!(build.dir, root, "cargo runs in the workspace, not the app's cwd");
        assert_eq!(build.checkout, root);
        assert_eq!(plan.program, target.join("release").join(exe_name("terminal")), "the built binary itself starts");
        assert_eq!(plan.args, ["--stdin-loop", "--preview", "/a.png"]);
        assert!(!build.args.iter().any(|a| a == "run" || a == "--"), "never `cargo run`: {:?}", build.args);
    }

    /// The shell runs from the desktop package inside the workspace: the
    /// build is the workspace's, with its lock and target dir.
    #[test]
    fn a_native_app_builds_from_the_workspace_around_the_desktop_package() {
        let ws = std::env::temp_dir().join(format!("os-ws-{}", std::process::id()));
        std::fs::create_dir_all(ws.join("desktop")).unwrap();
        std::fs::write(ws.join("Cargo.toml"), "[workspace]\nmembers = [\"desktop\"]\n").unwrap();
        std::fs::write(ws.join("desktop/Cargo.toml"), "[package]\nname = \"octosense\"\n").unwrap();
        assert_eq!(workspace_root(&ws.join("desktop")), ws);
        let app = AppDef::app("terminal", "Terminal", "makepad-terminal", "", "terminal", LaunchPolicy::AlwaysNew);
        let plan = launch_plan(&app, Some(&ws.join("desktop")), &[]).unwrap();
        let build = plan.build.unwrap();
        assert_eq!(build.dir, ws);
        assert!(build.args.windows(2).any(|p| p[0] == "--manifest-path" && Path::new(&p[1]) == ws.join("Cargo.toml")), "{:?}", build.args);
        assert!(plan.program.starts_with(env_target_dir(&ws).unwrap_or_else(|| ws.join("target"))));
        // The real checkout: the desktop package is inside this workspace.
        if let Some(root) = repo_root() {
            let ws = workspace_root(&root);
            assert!(ws.join("native-apps.json").is_file(), "{}", ws.display());
        }
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Another catalog row is held to its checkout's lock where it has one
    /// (Makepad ignores its own, and `--locked` would refuse to create it).
    #[test]
    fn a_catalog_row_is_locked_where_its_checkout_has_a_lock() {
        let app = AppDef::app("files", "Files", "makepad-files", "apps/files", "files", LaunchPolicy::OrFocus);
        let unlocked = std::env::temp_dir().join(format!("os-unlocked-{}", std::process::id()));
        let locked = std::env::temp_dir().join(format!("os-locked-{}", std::process::id()));
        for dir in [&unlocked, &locked] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(locked.join("Cargo.lock"), "").unwrap();
        let args = |root: &Path| launch_plan(&app, Some(root), &[]).unwrap().build.unwrap().args;
        assert!(!args(&unlocked).iter().any(|a| a == "--locked"));
        assert!(args(&locked).iter().any(|a| a == "--locked"));
        assert_eq!(args(&locked)[..2], ["build", "--release"]);
        let _ = std::fs::remove_dir_all(&unlocked);
        let _ = std::fs::remove_dir_all(&locked);
    }

    #[test]
    fn the_installed_fallback_execs_the_sibling_binary() {
        // No checkout: run the binary next to the running shell, which for
        // a release shell is target/release/<bin>.
        let app = AppDef::app("terminal", "Terminal", "makepad-terminal", "apps/terminal", "terminal", LaunchPolicy::AlwaysNew);
        match launch_plan(&app, None, &[]) {
            Ok(plan) => {
                let exe = std::env::current_exe().unwrap();
                assert!(plan.build.is_none());
                assert_eq!(plan.program.parent(), exe.parent());
                assert_eq!(plan.program.file_name().unwrap(), "terminal");
                assert_eq!(plan.args, vec!["--stdin-loop".to_string()]);
            }
            // The test binary does not sit next to terminal; the law that
            // matters is that it resolves a SIBLING or fails, never cargo.
            Err(e) => assert!(e.contains("binary not found"), "{}", e),
        }
    }

    /// What a sandboxed app could write never decides what the build runs:
    /// the build's PATH keeps no entry inside the person's home but the
    /// toolchain's own (read-only) homes; every `.cargo/` and toolchain file
    /// on the way up from the build's directory is listed for the sandbox.
    #[test]
    fn the_build_runs_nothing_a_sandboxed_app_could_have_written() {
        let home = Path::new("/Users/p");
        let path = std::env::join_paths(["/Users/p/.local/bin", "/usr/bin", "/Users/p/.cargo/bin", "/opt/homebrew/bin", "/Users/p/bin"]).unwrap();
        let kept = build_path(&path, home, &[PathBuf::from("/Users/p/.cargo"), PathBuf::from("/Users/p/.rustup")]);
        let kept: Vec<PathBuf> = std::env::split_paths(&kept).collect();
        assert_eq!(kept, [PathBuf::from("/usr/bin"), PathBuf::from("/Users/p/.cargo/bin"), PathBuf::from("/opt/homebrew/bin")]);
        let paths = cargo_config_paths(Path::new("/Users/p/src/OctoSense"));
        for want in ["/Users/p/src/OctoSense/.cargo", "/Users/p/src/.cargo", "/Users/p/.cargo", "/Users/.cargo", "/.cargo", "/Users/p/src/rust-toolchain.toml", "/Users/p/rust-toolchain"] {
            assert!(paths.contains(&PathBuf::from(want)), "{want}: {paths:?}");
        }
    }

    #[test]
    fn launch_or_focus_matches_whole_words_either_side() {
        // `\bfiles\b`, case-insensitive, over class OR title.
        assert!(word_match("files", "files"));
        assert!(word_match("Files", "files"));
        assert!(word_match("~/Pictures — files", "FILES"));
        assert!(word_match("makepad-files - files (2)", "files"));
        // Not a word boundary: no match.
        assert!(!word_match("makepadfiles", "files"));
        assert!(!word_match("filesystem", "files"));
        assert!(!word_match("", "files"));
        assert!(!word_match("files", ""));
        // A dot/dash counts as a boundary, like the regex \b.
        assert!(word_match("org.omarchy.btop", "btop"));
        assert!(word_match("btop-tui", "btop"));
    }

    #[test]
    fn every_registered_app_names_a_real_crate() {
        // The no-fake-UI law: a menu row must be startable. In a checkout
        // that means the package directory really is there.
        let Some(root) = repo_root() else {
            return; // installed layout: nothing to check against
        };
        let mut metadata = std::collections::HashMap::new();
        for app in registry() {
            // Linked modules and the apps the Card runner hosts are no
            // process: nothing to build.
            if app.package.is_empty() {
                continue;
            }
            let manifest = app
                .manifest
                .clone()
                .unwrap_or_else(|| format!("{}/Cargo.toml", app.dir));
            let path = root.join(&manifest);
            if !path.exists() {
                // Optional private clones (sandbox) may be absent; they are
                // filtered out of the menu by is_available().
                assert!(!app.is_available(), "{} claims to be available", app.id);
                continue;
            }
            // Catalog entries may select a package through its workspace
            // manifest, which has no package name of its own. Ask Cargo for
            // the actual packages and binaries once per launch manifest.
            let value = metadata.entry(path.clone()).or_insert_with(|| {
                let output = std::process::Command::new("cargo")
                    .args(["metadata", "--format-version", "1", "--no-deps", "--locked", "--offline", "--manifest-path"])
                    .arg(&path)
                    .output().unwrap();
                assert!(output.status.success(), "{}: {}", path.display(), String::from_utf8_lossy(&output.stderr));
                makepad_strict_json::parse(&output.stdout).unwrap()
            });
            let packages = value.get("packages").and_then(makepad_strict_json::Value::as_arr).unwrap();
            let package = packages.iter().find(|p| p.get("name").and_then(makepad_strict_json::Value::as_str) == Some(app.package.as_str()))
                .unwrap_or_else(|| panic!("{} points at the wrong package", app.id));
            let targets = package.get("targets").and_then(makepad_strict_json::Value::as_arr).unwrap();
            assert!(targets.iter().any(|target| {
                target.get("name").and_then(makepad_strict_json::Value::as_str) == Some(app.bin.as_str())
                    && target.get("kind").and_then(makepad_strict_json::Value::as_arr).unwrap()
                        .iter().any(|kind| kind.as_str() == Some("bin"))
            }), "{} points at the wrong binary", app.id);
            assert!(app.is_available(), "{} should be available", app.id);
        }
    }

    // ------------------------------------------------------------------
    // The warm pool
    // ------------------------------------------------------------------

    /// A pool holding `ids` for `app`, every one of them live and ready.
    fn pool_with(app: &str, ids: &[ClientId]) -> (WarmPool, Vec<WarmStatus>) {
        let mut pool = WarmPool::new(true);
        for id in ids {
            pool.note_spawned(app, *id);
        }
        let status = ids
            .iter()
            .map(|id| WarmStatus {
                client: *id,
                alive: true,
                connected: true,
            })
            .collect();
        (pool, status)
    }

    #[test]
    fn appearance_retires_only_unused_browsers_and_never_counts_as_a_crash() {
        let (mut pool,status)=pool_with("browser", &[40,41]);
        pool.note_spawned("files",42);
        assert!(pool.set_browser_appearance(true).is_empty());
        assert_eq!(pool.adopt("browser",false,&status),Some(40));
        assert!(pool.set_browser_appearance(true).is_empty());
        assert_eq!(pool.set_browser_appearance(false),vec![41]);
        assert_eq!(pool.adopt("browser",false,&status),None);
        assert!(pool.holds(42));
        // Reaping intentional retirements cannot charge the crash budget.
        assert_eq!(pool.forget(41),None);
        for dark in [true,false,true,false] {assert!(pool.set_browser_appearance(dark).is_empty());}
        assert!(pool.wants("browser",0.0));
        pool.note_spawned("browser",43);
        assert!(pool.set_browser_appearance(false).is_empty());
        assert!(pool.holds(43));
    }

    #[test]
    fn appearance_changes_do_not_enable_a_disabled_pool() {
        let mut pool=WarmPool::new(false);
        pool.set_browser_appearance(true);
        pool.set_browser_appearance(false);
        assert!(!pool.wants("browser",0.0));
    }

    #[test]
    fn the_pool_sizes_are_the_users_two_terminals_and_one_of_the_rest() {
        assert_eq!(WarmPool::capacity("terminal"), 2);
        for app in ["browser", "files", "task"] {
            assert_eq!(WarmPool::capacity(app), 1, "{}", app);
        }
        // Everything else launches cold, as it always did.
        for app in ["vj", "fab", "studio", "image", "nonesuch"] {
            assert_eq!(WarmPool::capacity(app), 0, "{}", app);
            assert!(!WarmPool::is_warm_app(app), "{}", app);
        }
    }

    #[test]
    fn adopting_clears_the_slot_and_asks_for_a_respawn() {
        let (mut pool, status) = pool_with("browser", &[7]);
        // Fill the rest of the shelf, so a full pool asks for nothing and
        // the top-up below names exactly the app that was adopted.
        pool.note_spawned("terminal", 1);
        pool.note_spawned("terminal", 2);
        pool.note_spawned("files", 3);
        pool.note_spawned("task", 4);
        assert!(!pool.wants("browser", host::now()), "already full");
        assert_eq!(pool.next_missing(host::now()), None, "nothing missing");
        assert_eq!(pool.adopt("browser", false, &status), Some(7));
        // Out of the pool, and the pool now wants its replacement.
        assert_eq!(pool.held("browser"), 0);
        assert!(!pool.holds(7));
        assert!(pool.wants("browser", host::now()));
        assert_eq!(pool.next_missing(host::now()).as_deref(), Some("browser"));
        // The same instance can never be adopted twice.
        assert_eq!(pool.adopt("browser", false, &status), None);
    }

    #[test]
    fn two_terminals_stand_by_and_both_open_instantly() {
        let (mut pool, status) = pool_with("terminal", &[3, 4]);
        assert_eq!(pool.held("terminal"), 2);
        assert!(!pool.wants("terminal", host::now()));
        // Back-to-back opens: both are swaps, oldest first.
        assert_eq!(pool.adopt("terminal", false, &status), Some(3));
        assert_eq!(pool.held("terminal"), 1);
        assert_eq!(pool.adopt("terminal", false, &status), Some(4));
        assert_eq!(pool.held("terminal"), 0);
        // A third open in the same breath falls back to cold, and the pool
        // is two short — one spawn per tick, so it tops up twice.
        assert_eq!(pool.adopt("terminal", false, &status), None);
        assert!(pool.wants("terminal", host::now()));
        pool.note_spawned("terminal", 9);
        assert!(pool.wants("terminal", host::now()));
        pool.note_spawned("terminal", 10);
        assert!(!pool.wants("terminal", host::now()));
        assert_eq!(pool.next_missing(host::now()).as_deref(), Some("browser"));
    }

    #[test]
    fn a_dead_or_unconnected_warm_instance_falls_back_to_a_cold_spawn() {
        // Killed behind our back: not in the client table any more.
        let (mut pool, _) = pool_with("terminal", &[3, 4]);
        let gone = [
            WarmStatus { client: 3, alive: false, connected: false },
            WarmStatus { client: 4, alive: true, connected: true },
        ];
        assert_eq!(pool.adopt("terminal", false, &gone), Some(4));
        // The dead one was pruned on the way past, so the pool asks for
        // two replacements rather than counting a corpse.
        assert_eq!(pool.held("terminal"), 0);

        // Still building / still starting: alive but not connected. No
        // adoption (there is no frame to show), and it KEEPS its slot —
        // it will be ready for the next launch.
        let (mut pool, _) = pool_with("browser", &[5]);
        let starting = [WarmStatus { client: 5, alive: true, connected: false }];
        assert_eq!(pool.adopt("browser", false, &starting), None);
        assert_eq!(pool.held("browser"), 1);
        assert!(!pool.wants("browser", host::now()));

        // An app with nothing standing by: cold, quietly.
        let mut empty = WarmPool::new(true);
        assert_eq!(empty.adopt("terminal", false, &[]), None);
    }

    #[test]
    fn a_cwd_override_skips_adoption_and_keeps_the_instance() {
        // THE CARVE-OUT: a new terminal must open in the focused
        // terminal's cwd, and the warm shell already started elsewhere.
        let (mut pool, status) = pool_with("terminal", &[3, 4]);
        assert_eq!(pool.adopt("terminal", true, &status), None);
        // Nothing was consumed: the next launch WITHOUT an override is
        // still instant.
        assert_eq!(pool.held("terminal"), 2);
        assert_eq!(pool.adopt("terminal", false, &status), Some(3));
    }

    #[test]
    fn wm_no_warm_turns_the_pool_off_entirely() {
        assert!(warm_enabled(None));
        // An empty or 0 value is not a request.
        assert!(warm_enabled(Some("")));
        assert!(warm_enabled(Some("0")));
        assert!(!warm_enabled(Some("1")));
        assert!(!warm_enabled(Some("yes")));

        let mut off = WarmPool::new(false);
        assert!(!off.enabled());
        // Nothing is ever spawned…
        assert!(!off.wants("terminal", host::now()));
        assert_eq!(off.next_missing(host::now()), None);
        // …and even a hand-fed instance is never adopted.
        off.note_spawned("terminal", 1);
        let status = [WarmStatus { client: 1, alive: true, connected: true }];
        assert_eq!(off.adopt("terminal", false, &status), None);
    }

    #[test]
    fn a_crash_loop_gives_up_quietly_after_three_a_minute() {
        let now = host::now();
        let mut pool = WarmPool::new(true);
        for i in 0..WARM_CRASH_LIMIT {
            assert!(pool.wants("browser", now), "attempt {}", i);
            pool.note_spawned("browser", i as ClientId);
            // Up, then dead before anyone could adopt it.
            let app = pool.forget(i as ClientId).expect("pooled");
            pool.note_crash(&app, now);
        }
        assert!(!pool.wants("browser", now), "the budget should be spent");
        assert_eq!(pool.next_missing(now).as_deref(), Some("terminal"));
        // The budget is per app…
        assert!(pool.wants("terminal", now));
        // …and it is a WINDOW: a minute later the app is tried again.
        assert!(pool.wants("browser", now + WARM_CRASH_WINDOW + 1.0));
    }

    #[test]
    fn a_deliberate_close_costs_no_budget_and_tops_back_up() {
        // CTRL+ALT+DELETE closes the warm instances with everything else;
        // that is not a crash, so the pool refills at once instead of
        // spending the loop budget on the user's own gesture.
        let now = host::now();
        let mut pool = WarmPool::new(true);
        for id in 0..6 {
            pool.note_spawned("terminal", id);
            assert_eq!(pool.forget(id).as_deref(), Some("terminal"));
        }
        assert!(pool.wants("terminal", now));
        assert_eq!(pool.forget(99), None, "an unknown client is not ours");
    }

    #[test]
    fn every_warm_client_is_reachable_for_shutdown() {
        let mut pool = WarmPool::new(true);
        pool.note_spawned("terminal", 3);
        pool.note_spawned("terminal", 4);
        pool.note_spawned("browser", 1);
        assert_eq!(pool.clients(), vec![1, 3, 4]);
        assert!(pool.holds(4) && !pool.holds(5));
    }

    #[test]
    fn a_warm_instance_launches_exactly_like_a_cold_one() {
        // Same argv — the registry's own args included, which is how the
        // warm Files inherits `--demo` without the pool knowing about it.
        let mut files = AppDef::app("files", "Files", "makepad-files", "apps/files", "files", LaunchPolicy::OrFocus);
        files.args.push("--demo".into());
        let root = std::path::PathBuf::from("/checkout");
        let plan = launch_plan(&files, Some(&root), &[]).unwrap();
        assert!(plan.build.as_ref().unwrap().args.contains(&"makepad-files".to_string()), "{:?}", plan);
        assert_eq!(plan.args.last().map(String::as_str), Some("--demo"), "{:?}", plan.args);
    }

    #[test]
    fn the_warm_env_is_the_one_the_apps_read() {
        // The contract with `makepad_wm_api::warm_start()`: a dormant app idles
        // (no samplers, no refresh) until `WmEvent::Adopted`. If these two
        // ever drift, a warm task manager silently burns a core.
        assert!(!makepad_wm_api::warm_start(), "MAKEPAD_WM_WARM_START leaked in");
        std::env::set_var(WARM_ENV.0, WARM_ENV.1);
        assert!(makepad_wm_api::warm_start());
        std::env::remove_var(WARM_ENV.0);
        assert!(!makepad_wm_api::warm_start());
    }

    #[test]
    fn manifest_values_come_from_the_package_table_only() {
        let toml = "[package]\nname = \"a\"\n\n[dependencies]\nname = \"b\"\n";
        assert_eq!(manifest_value(toml, "name").as_deref(), Some("a"));
    }

    /// `own_process_group` really does make the child its own group leader
    /// — the precondition `kill_child_group`'s negative-pid signal relies
    /// on. `getpgid` is one more syscall this crate has no `libc` for.
    #[cfg(unix)]
    #[test]
    fn spawned_children_lead_their_own_process_group() {
        extern "C" {
            fn getpgid(pid: i32) -> i32;
        }
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c").arg("sleep 5");
        own_process_group(&mut cmd);
        let mut child = ProcessGroup::new(cmd.spawn().expect("spawn /bin/sh"));
        let pid = child.id() as i32;
        let pgid = unsafe { getpgid(pid) };
        assert_eq!(pgid, pid, "the child should lead its own new group");
        let pool = Cx::new(Box::new(|_, _| {})).task_pool();
        kill_child_group(&mut child, std::time::Duration::from_millis(50), &pool);
        let _ = child.wait();
    }

    #[cfg(unix)]
    #[test]
    fn escalation_kills_a_term_resistant_descendant_after_the_wrapper_is_reaped() {
        use std::io::{BufRead, BufReader};
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "/bin/sh -c 'trap \"\" TERM; echo $$; exec sleep 30' & wait"]);
        own_process_group(&mut cmd);
        cmd.stdout(Stdio::piped());
        let mut child = ProcessGroup::new(cmd.spawn().expect("spawn wrapper"));
        let group = child.id() as i32;
        let mut line = String::new();
        BufReader::new(child.child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        let descendant: i32 = line.trim().parse().unwrap();
        let pool = Cx::new(Box::new(|_, _| {})).task_pool();
        kill_child_group(&mut child, std::time::Duration::from_millis(100), &pool);
        child.wait().expect("reap wrapper before escalation");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while signal::alive(descendant) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let escaped = signal::alive(descendant);
        // Keep a failing regression from leaving its fixture behind.
        signal::kill_group(group, signal::SIGKILL);
        assert!(!escaped, "the group still needs SIGKILL after its leader has been reaped");
    }

    /// Review 2026-09-30: a slot dropped after the tick reaped its child
    /// must not signal that group id again; it may be someone else's. The
    /// reap itself sweeps what the group still held.
    #[cfg(unix)]
    #[test]
    fn a_reaped_group_is_never_signalled_again() {
        use std::io::{BufRead, BufReader};
        // The leader exits at once, leaving a TERM-proof descendant behind.
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "/bin/sh -c 'trap \"\" TERM; echo $$; exec sleep 30' &"]);
        own_process_group(&mut cmd);
        cmd.stdout(Stdio::piped());
        let mut child = ProcessGroup::new(cmd.spawn().unwrap());
        let group = child.id() as i32;
        let mut line = String::new();
        BufReader::new(child.child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        let descendant: i32 = line.trim().parse().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while child.try_wait().unwrap().is_none() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(child.reaped());
        let swept = child.signals_sent();
        assert_eq!(swept, 1, "the reap sweeps the group's leftovers once");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while signal::alive(descendant) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let escaped = signal::alive(descendant);
        if escaped { signal::kill_group(group, signal::SIGKILL); }
        assert!(!escaped, "nothing of an ended app outlives it");
        // From here on: no close, kill or drop signals that group id.
        let pool = Cx::new(Box::new(|_, _| {})).task_pool();
        kill_child_group(&mut child, std::time::Duration::from_millis(10), &pool);
        child.kill_now();
        assert_eq!(child.signals_sent(), swept, "no signal after the reap");
        let mut slot = ClientSlot::module(3, "terminal", "Terminal");
        slot.task_pool = Some(pool);
        slot.child = Some(child);
        drop(slot);
    }

    /// A live group's escalation is still delivered, but an escalation
    /// under way when the leader is reaped does not fire.
    #[cfg(unix)]
    #[test]
    fn an_escalation_under_way_stops_at_the_reap() {
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "exec sleep 30"]);
        own_process_group(&mut cmd);
        let mut child = ProcessGroup::new(cmd.spawn().unwrap());
        let pool = Cx::new(Box::new(|_, _| {})).task_pool();
        kill_child_group(&mut child, std::time::Duration::from_millis(300), &pool);
        assert_eq!(child.signals_sent(), 1, "SIGTERM went out");
        child.wait().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert_eq!(child.signals_sent(), 1, "the SIGKILL escalation found the leader reaped and sent nothing");
    }

    /// A launch's second half: a successful build starts the app into the
    /// same slot; a failed one starts nothing and says why; a launch closed
    /// while building starts nothing.
    #[cfg(unix)]
    #[test]
    fn a_launch_starts_its_app_only_after_a_successful_build() {
        let spawner = Cx::new(Box::new(|_, _| {})).thread_spawner();
        let (tx, _rx) = std::sync::mpsc::channel();
        let slot_for = |build: &str, app: &str| {
            let mut slot = ClientSlot::module(4, "terminal", "Terminal");
            slot.ready = false;
            let mut cmd = Command::new("/bin/sh");
            cmd.args(["-c", build]);
            slot.child = Some(start_piped(cmd, &spawner, 4, None, tx.clone(), None).unwrap());
            let mut app_cmd = Command::new("/bin/sh");
            app_cmd.args(["-c", app]);
            slot.pending = Some(PendingApp { cmd: app_cmd, spawner: spawner.clone(), lines: tx.clone(), log: None, launch_token: "the-secret".into() });
            slot
        };
        let settle = |slot: &mut ClientSlot| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                if let Some(result) = slot.continue_launch(4) { return result; }
                assert!(std::time::Instant::now() < deadline, "the build never finished");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };
        // The build has no stdin (a read fails at once); the app reads the
        // hub secret as its stdin's only line (#220).
        let mut ok = slot_for("read x && exit 1; exit 0", "read t || exit 2; [ \"$t\" = the-secret ] || exit 3; read u && exit 4; exit 7");
        let build_pid = ok.child.as_ref().unwrap().id();
        assert!(ok.building());
        assert_eq!(settle(&mut ok), Ok(()));
        assert!(!ok.building());
        assert_ne!(ok.child.as_ref().unwrap().id(), build_pid, "the app's group replaced the build's");
        assert_eq!(ok.child.as_mut().unwrap().wait().unwrap().code(), Some(7), "the app ran, read the secret, then EOF");
        assert!(ok.continue_launch(4).is_none(), "nothing left to continue");

        let mut failed = slot_for("exit 101", "exit 0");
        assert!(settle(&mut failed).unwrap_err().contains("build failed"));
        let status = failed.child.as_mut().unwrap().try_wait().unwrap().unwrap();
        let message = failed.exit_failure(status, "Terminal").unwrap();
        assert!(message.starts_with("Terminal did not start: its build failed"), "{message}");

        let mut closed = slot_for("exit 0", "echo started > /dev/null; exit 3");
        closed.closing = Some(0.0);
        assert_eq!(settle(&mut closed), Ok(()));
        assert!(closed.child.as_ref().unwrap().reaped(), "the reaped build stays; no app started");
    }

    #[cfg(unix)]
    #[test]
    fn final_shutdown_reaps_every_group_without_a_task_pool() {
        use std::io::{BufRead, BufReader};
        struct Cleanup(Vec<i32>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                for group in &self.0 { signal::kill_group(*group, signal::SIGKILL); }
            }
        }
        let mut cleanup = Cleanup(Vec::new());
        let mut clients = HashMap::new();
        for id in [1, 2] {
            let mut cmd = Command::new("/bin/sh");
            cmd.args(["-c", "/bin/sh -c 'trap \"\" TERM; echo $$; exec sleep 30' & wait"]);
            own_process_group(&mut cmd);
            cmd.stdout(Stdio::piped());
            let mut child = cmd.spawn().unwrap();
            cleanup.0.push(child.id() as i32);
            let mut ready = String::new();
            BufReader::new(child.stdout.take().unwrap()).read_line(&mut ready).unwrap();
            let mut slot = ClientSlot::module(id, "reference", "Reference");
            slot.child = Some(ProcessGroup::new(child));
            // Represents an ordinary child still building, with no hub sender.
            slot.ready = false;
            clients.insert(id, slot);
        }
        let started = std::time::Instant::now();
        shutdown_clients(&mut clients);
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        assert!(clients.is_empty(), "shutdown must take child handles before normal Drop");
        for group in &cleanup.0 {
            assert!(!signal::alive(-*group), "shutdown returned with a surviving group {group}");
            extern "C" { fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32; }
            assert_eq!(unsafe { waitpid(*group, std::ptr::null_mut(), 1) }, -1,
                "the wrapper must already be reaped");
        }
    }

    #[cfg(unix)]
    #[test]
    fn failed_startup_reports_the_label_diagnostic_and_log() {
        use std::os::unix::process::ExitStatusExt;
        let mut slot = ClientSlot::module(7, "reference", "");
        slot.ready = false;
        slot.diagnostic = "error: package octosense-missing was not found".into();
        slot.log_path = Some(PathBuf::from("/tmp/octosense-123-client-7.log"));
        let message = slot.exit_failure(std::process::ExitStatus::from_raw(101 << 8), "Reference App").unwrap();
        assert!(message.contains("Reference App"));
        assert!(message.contains(&slot.diagnostic));
        assert!(message.contains("/tmp/octosense-123-client-7.log"));
        assert!(slot.exit_failure(std::process::ExitStatus::from_raw(0), "Reference App").is_some(),
            "exiting successfully before creating a window is also a startup failure");
    }

    /// ADR 0004 §2: a process app that dies takes only itself down. Its
    /// tile stays, closed with a Restart; a warm instance, the AI pane, a
    /// preview and a client being closed are removed as before.
    #[cfg(unix)]
    #[test]
    fn an_unexpected_death_keeps_the_tile_closed_with_restart() {
        let mut slot = ClientSlot::module(9, "terminal", "Terminal");
        slot.child = Some(ProcessGroup::new(std::process::Command::new("/usr/bin/true").spawn().unwrap()));
        assert!(slot.stops_in_place(true));
        assert!(!slot.stops_in_place(false), "a clean exit closes the window");
        slot.closing = Some(0.0);
        assert!(!slot.stops_in_place(true), "the person closed it");
        slot.closing = None;
        for (warm, pane, preview, ready) in [(true, false, false, true), (false, true, false, true), (false, false, true, true), (false, false, false, false)] {
            (slot.warm, slot.pane, slot.is_preview, slot.ready) = (warm, pane, preview, ready);
            assert!(!slot.stops_in_place(true), "warm {warm} pane {pane} preview {preview} ready {ready}");
        }
        let _ = slot.child.take().map(|mut c| c.wait());
        slot.ready = true;
        assert!(!slot.stops_in_place(true), "a module slot has no process to stop");
    }

    #[cfg(unix)]
    #[test]
    fn requested_closes_warm_exits_and_normal_exits_do_not_notify() {
        use std::os::unix::process::ExitStatusExt;
        let failed = std::process::ExitStatus::from_raw(101 << 8);
        let mut slot = ClientSlot::module(7, "reference", "Reference");
        assert!(slot.exit_failure(std::process::ExitStatus::from_raw(0), "Reference").is_none());
        slot.closing = Some(0.0);
        assert!(slot.exit_failure(failed, "Reference").is_none());
        slot.closing = None;
        slot.warm = true;
        assert!(slot.exit_failure(failed, "Reference").is_none());
    }

    /// The bug this fixes: a child launched through a wrapper (`cargo run`
    /// stands in for it here as any process that forks a grandchild rather
    /// than exec-replacing itself) leaks that grandchild when only the
    /// wrapper is killed. Reproduce it with a shell that backgrounds a
    /// `sleep` and prints its pid, then confirm `kill_child_group` reaps
    /// BOTH — the regression `Child::kill()` alone could not clear.
    #[cfg(unix)]
    #[test]
    fn killing_the_group_reaps_a_grandchild_the_wrapper_leaked() {
        extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c").arg("sleep 30 & echo $!; wait");
        own_process_group(&mut cmd);
        cmd.stdout(Stdio::piped());
        let mut child = ProcessGroup::new(cmd.spawn().expect("spawn /bin/sh"));

        use std::io::BufRead;
        let stdout = child.child.stdout.take().expect("piped stdout");
        let mut reader = std::io::BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).expect("read grandchild pid");
        let grandchild_pid: i32 = line.trim().parse().expect("a pid line");

        // The grandchild is alive and NOT the pid we hold — it really is
        // one generation further down, like the app under `cargo run`.
        assert_ne!(grandchild_pid, child.id() as i32);
        assert_eq!(unsafe { kill(grandchild_pid, 0) }, 0, "grandchild not up yet");

        let pool = Cx::new(Box::new(|_, _| {})).task_pool();
        kill_child_group(&mut child, std::time::Duration::from_millis(50), &pool);
        // Past the SIGTERM->SIGKILL escalation: nothing in the group is
        // still standing, wrapper or grandchild.
        let _ = child.wait();
        // The wrapper can exit before the asynchronous escalation and before
        // launchd reaps the orphan. Wait only in this test, never on the UI.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while unsafe { kill(grandchild_pid, 0) } == 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            unsafe { kill(grandchild_pid, 0) },
            -1,
            "the grandchild the wrapper orphaned should be gone too"
        );
    }
}

impl Drop for ClientSlot {
    fn drop(&mut self) {
        // Ask the app to go first over its own socket, then end and reap
        // its group (never once the tick has reaped it: ProcessGroup).
        if let Some(sender) = self.sender.take() {
            crate::hub::send_to_app(
                &sender,
                vec![makepad_studio_protocol::StudioToApp::Kill],
            );
        }
        if let Some(mut child) = self.child.take() {
            if let Some(pool) = &self.task_pool {
                reap_child_group(child, GROUP_KILL_GRACE, pool);
            } else {
                child.kill_now();
            }
        }
    }
}
