//! Developer mode (ADR 0004 §13): everything granted, for building apps fast.
//!
//! **State.** Off, or on for all apps or chosen apps, with where it came
//! from ([`Origin`]: Settings, `OCTOSENSE_DEV_MODE`, `--dev-grant-all`) and
//! when it ends ([`Active::expires`]).
//!
//! **Who turns it on.** Only the person: Settings → Developer options with
//! the typed phrase [`CONFIRM_PHRASE`] (the desktop menu, `shell/menu.rs`),
//! or at launch `OCTOSENSE_DEV_MODE=all` / `--dev-grant-all`. The setter,
//! [`turn_on`], takes a [`PersonGesture`], and nothing outside this module
//! and the shell's Settings rows can make one: the AI bus (`ai_bus.rs`), the
//! `os` service and the host services never do (a test scans the sources).
//! Turning it OFF is always allowed, from anywhere.
//!
//! | Build ([`BuildKind`]) | `OCTOSENSE_DEV_MODE` | `--dev-grant-all` | Settings |
//! | --- | --- | --- | --- |
//! | development (`cfg(dev_mode)`: debug builds, or `--features dev-mode`) | yes | yes | yes |
//! | release | ignored | yes | only in a run launched with the flag |
//! | store (`OCTOSENSE_STORE_BUILD` at compile time) | ignored | ignored | never |
//!
//! **Profiles.** A home holding [`PROFILE_MARKER`] is a developer profile:
//! developer mode stays on until turned off and survives a restart
//! ([`STATE_FILE`]). Any other home is taken to hold real accounts: the
//! shell warns, and developer mode ends after [`REAL_ACCOUNT_LIMIT_S`] or at
//! restart (nothing is persisted). A [`DevTag`] stamps anything made in
//! developer mode and stops being valid when developer mode ends or in
//! another profile, so such grants and rules never carry over.
//!
//! **Audit.** Every mode change, and while it is on every tool call, `dev.run`
//! command and automatic approval, is appended to [`AUDIT_FILE`] under the
//! home (JSON lines; [`audit_tool_call`], [`audit_auto_approval`],
//! [`audit_dev_run`]).
//!
//! **Approvals: none.** Developer mode overrides EVERY approval for the apps
//! it covers (ADR 0004 §13, decided on #110): the shell answers each one
//! itself, with no live sheet, no standing rule and no app's own
//! `confirm: app` sheet, destructive, outward and `auto_approvable: false`
//! tools included (Terminal commands, granted command execution, `dev.run`,
//! deletes, payments). Every approval path asks [`answers_approval`] (one
//! decision for every [`ApprovalKind`]) and logs each answer with
//! [`audit_auto_approval`]. The paths today:
//!
//! - the chat pane's confirm card for a destructive tool, the Terminal's
//!   `run` among them (`ai_bus.rs` over the socket, `pane_links.rs` in
//!   process): a covered app is announced with every tool pre-approved, and
//!   re-announced whenever the mode changes;
//! - octos's own `approval/requested` in an app agent's context: the app
//!   peer broker asks the shell's hook ([`answer_octos_approval`]) and
//!   answers `approve` itself instead of handing it to the app.
//!
//! Seams for what octos#2567 and ADR 0004 step 7 add: the `confirm: app`
//! hand-off asks [`overrides_app_confirm`] and skips the owning app's sheet;
//! granted command execution (`terminal.run`, the system agent's granted
//! commands, `dev.run`) asks [`approves_command`]; the approval router asks
//! [`answers_approval`] before any standing rule. The module host offers a
//! covered module every assistant service it declares ([`grants_all`]).
//!
//! **Never for external clients** (ADR 0003): developer grants and `dev.run`
//! go only to app peer sessions on the shell's host connection
//! ([`dev_grants_allowed_on`]). `dev.run` is a host tool registered only on
//! a covered app's peer, which the brokers open on the host connection
//! (`host_tools::relay::Catalog::offered`), run by the shell
//! (`host_tools::dev_run`), and withdrawn when the mode changes
//! (`host_tools::developer_mode_changed`); [`may_register_dev_run`] states
//! the rule that registration follows.

use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Launch-time environment switch: `all`, or a comma list of app ids.
pub const ENV: &str = "OCTOSENSE_DEV_MODE";
/// Launch flag: developer mode for all apps (any build but a store build).
pub const FLAG_ALL: &str = "--dev-grant-all";
/// What the person types in Settings → Developer options to turn it on.
pub const CONFIRM_PHRASE: &str = "turn on developer mode";
/// A file in the OctoSense home that marks it as a developer profile.
pub const PROFILE_MARKER: &str = "developer-profile";
/// Where a developer profile keeps developer mode across restarts.
pub const STATE_FILE: &str = "dev-mode.json";
/// The append-only audit log, relative to the home.
pub const AUDIT_FILE: &str = "logs/dev-audit.jsonl";
/// With real accounts, developer mode ends by itself after this long.
pub const REAL_ACCOUNT_LIMIT_S: u64 = 8 * 3600;
/// The host tool developer mode adds (`host_tools::dev_run`).
pub const DEV_RUN_TOOL: &str = "dev.run";

/// Which kind of build this is, for what may turn developer mode on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildKind {
    Development,
    Release,
    Store,
}

impl BuildKind {
    pub fn current() -> Self {
        if option_env!("OCTOSENSE_STORE_BUILD").is_some() {
            BuildKind::Store
        } else if cfg!(dev_mode) {
            BuildKind::Development
        } else {
            BuildKind::Release
        }
    }
    fn honours_env(self) -> bool {
        self == BuildKind::Development
    }
    fn honours_flag(self) -> bool {
        self != BuildKind::Store
    }
    /// Whether Settings may turn it on: a development build, or a release
    /// build launched with the flag — never Settings alone.
    fn honours_settings(self, launched_with_flag: bool) -> bool {
        match self {
            BuildKind::Development => true,
            BuildKind::Release => launched_with_flag,
            BuildKind::Store => false,
        }
    }
}

/// Which apps developer mode covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    AllApps,
    /// Chosen apps, by app id or AI service id (`os.mail`, `mail`).
    Apps(BTreeSet<String>),
}

impl Scope {
    /// `all`, or a comma list of ids; `None` for nothing usable.
    pub fn parse(s: &str) -> Option<Scope> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("all") || s == "1" {
            return Some(Scope::AllApps);
        }
        let apps: BTreeSet<String> =
            s.split(',').map(|a| a.trim().to_lowercase()).filter(|a| !a.is_empty()).collect();
        (!apps.is_empty()).then_some(Scope::Apps(apps))
    }
    pub fn covers(&self, app: &str) -> bool {
        match self {
            Scope::AllApps => true,
            Scope::Apps(apps) => {
                // A system app `os.mail` and its AI service `mail` are one app.
                let short = |id: &str| id.strip_prefix("os.").unwrap_or(id).to_owned();
                let app = short(&app.to_lowercase());
                apps.iter().any(|a| short(a) == app)
            }
        }
    }
    fn to_json(&self) -> Value {
        match self {
            Scope::AllApps => json!("all"),
            Scope::Apps(apps) => json!(apps),
        }
    }
    fn from_json(v: &Value) -> Option<Scope> {
        match v {
            Value::String(s) => Scope::parse(s),
            Value::Array(items) => {
                let apps: BTreeSet<String> = items.iter().filter_map(Value::as_str).map(str::to_owned).collect();
                (!apps.is_empty()).then_some(Scope::Apps(apps))
            }
            _ => None,
        }
    }
}

/// Where developer mode was turned on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Settings,
    Env,
    Flag,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Settings => "settings",
            Origin::Env => "env",
            Origin::Flag => "flag",
        }
    }
    fn parse(s: &str) -> Option<Origin> {
        Some(match s {
            "settings" => Origin::Settings,
            "env" => Origin::Env,
            "flag" => Origin::Flag,
            _ => return None,
        })
    }
}

/// What the OctoSense home is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileKind {
    /// Marked with [`PROFILE_MARKER`]: test accounts, no expiry.
    Developer,
    /// Anything else: taken to hold the person's real accounts.
    RealAccounts,
}

/// Developer mode, on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Active {
    pub scope: Scope,
    pub origin: Origin,
    /// Unix seconds.
    pub since: u64,
    /// Unix seconds; `None` in a developer profile.
    pub expires: Option<u64>,
}

/// Proof that the person asked. Only this module and the shell's Settings
/// rows construct one; see the module docs.
pub struct PersonGesture {
    origin: Origin,
}

impl PersonGesture {
    /// Settings → Developer options: the person typed [`CONFIRM_PHRASE`]
    /// (case and surrounding space ignored).
    pub(crate) fn settings_phrase(typed: &str) -> Option<PersonGesture> {
        let typed = typed.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        (typed == CONFIRM_PHRASE).then_some(PersonGesture { origin: Origin::Settings })
    }
    fn launch(origin: Origin) -> PersonGesture {
        PersonGesture { origin }
    }
}

/// What the process was started with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub build: BuildKind,
    pub env: Option<String>,
    pub flag_all: bool,
}

impl Launch {
    /// This process: its build, `OCTOSENSE_DEV_MODE` and its arguments.
    pub fn from_process() -> Launch {
        let args: Vec<String> = std::env::args().collect();
        Launch::from_parts(BuildKind::current(), std::env::var(ENV).ok(), &args)
    }
    pub fn from_parts(build: BuildKind, env: Option<String>, args: &[String]) -> Launch {
        let env = env.filter(|v| !v.trim().is_empty() && v.trim() != "0");
        Launch { build, env, flag_all: args.iter().any(|a| a == FLAG_ALL) }
    }
}

/// A stamp on a grant or rule made in developer mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevTag {
    pub profile_id: String,
    pub since: u64,
}

/// Every kind of approval there is (ADR 0004 §8, §10, §12). Developer mode
/// answers them all alike; the kind is recorded in the audit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalKind {
    /// The chat pane's confirm card for a destructive tool (the Terminal's
    /// `run` included).
    PaneConfirm,
    /// A `confirm: app` tool: the owning app's own sheet.
    AppConfirm,
    /// octos's `approval/requested` in an app agent's context.
    OctosApproval,
    /// A shell-drawn sheet the approval router would evaluate against
    /// standing rules (step 7).
    HostConfirm,
    /// Command execution: `terminal.run`, granted commands, `dev.run`.
    Command,
}

impl ApprovalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ApprovalKind::PaneConfirm => "pane_confirm",
            ApprovalKind::AppConfirm => "app_confirm",
            ApprovalKind::OctosApproval => "octos_approval",
            ApprovalKind::HostConfirm => "host_confirm",
            ApprovalKind::Command => "command",
        }
    }
}

/// Which connection a session is reached on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    /// The shell's own host connection to its kernel.
    Host,
    /// A Talk to Octos external client (ADR 0003).
    External,
}

/// Whether `session_key` is an app agent's peer session (`…#peer-<slug>`,
/// `…#peerctx-…`): the sessions ADR 0003 keeps every external client out of.
pub fn is_app_peer_session(session_key: &str) -> bool {
    session_key
        .rsplit_once('#')
        .is_some_and(|(_, topic)| topic.starts_with("peer-") || topic.starts_with("peerctx-"))
}

/// Whether developer grants (and `dev.run`) may reach a session. Only app
/// peer sessions on the host connection: never an external client, and never
/// a session an external client can open (the system conversation included).
pub fn dev_grants_allowed_on(connection: Connection, session_key: &str) -> Result<(), &'static str> {
    if connection == Connection::External {
        return Err("Talk to Octos clients never get developer grants");
    }
    if !is_app_peer_session(session_key) {
        return Err("developer grants go only to app agents' peer sessions, which external clients cannot reach");
    }
    Ok(())
}

/// The append-only audit log.
#[derive(Clone, Debug)]
pub struct Audit {
    path: PathBuf,
}

impl Audit {
    pub fn in_home(home: &Path) -> Audit {
        Audit { path: home.join(AUDIT_FILE) }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Append one entry (`ts` is added). Failures are logged, never fatal.
    pub fn append(&self, now: u64, kind: &str, mut entry: Value) {
        if let Value::Object(map) = &mut entry {
            map.insert("ts".into(), json!(now));
            map.insert("kind".into(), json!(kind));
        }
        let line = format!("{entry}\n");
        if let Err(e) = self.write(line.as_bytes()) {
            eprintln!("dev-mode: could not write the audit log {}: {e}", self.path.display());
        }
    }
    fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut options = std::fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&self.path)?.write_all(bytes)
    }
}

/// Developer mode for one home: the whole state machine, clock passed in.
#[derive(Debug)]
pub struct Controller {
    home: PathBuf,
    profile: ProfileKind,
    profile_id: String,
    launch: Launch,
    active: Option<Active>,
    audit: Audit,
    /// Bumped on every change, so the shell knows when to redraw and
    /// re-announce apps' tools.
    generation: u64,
    /// Things the person must be told (the real-accounts warning).
    notices: Vec<String>,
}

fn profile_id(home: &Path) -> String {
    std::fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf()).to_string_lossy().into_owned()
}

impl Controller {
    /// Set up for `home` and apply the launch: the flag, the environment,
    /// or a developer profile's saved state, in that order.
    pub fn start(home: &Path, launch: Launch, now: u64) -> Controller {
        let profile = if home.join(PROFILE_MARKER).exists() { ProfileKind::Developer } else { ProfileKind::RealAccounts };
        let mut c = Controller {
            home: home.to_path_buf(),
            profile,
            profile_id: profile_id(home),
            audit: Audit::in_home(home),
            launch,
            active: None,
            generation: 0,
            notices: Vec::new(),
        };
        let build = c.launch.build;
        if c.launch.flag_all && build.honours_flag() {
            let _ = c.activate(PersonGesture::launch(Origin::Flag), Scope::AllApps, now, false);
        } else if c.launch.flag_all {
            c.refused(now, Origin::Flag, "a store build never turns on developer mode");
        }
        if c.active.is_none() {
            if let Some(value) = c.launch.env.clone() {
                if !build.honours_env() {
                    c.refused(now, Origin::Env, "only a development build honours OCTOSENSE_DEV_MODE; launch with --dev-grant-all");
                } else if let Some(scope) = Scope::parse(&value) {
                    let _ = c.activate(PersonGesture::launch(Origin::Env), scope, now, false);
                } else {
                    c.refused(now, Origin::Env, "OCTOSENSE_DEV_MODE is `all` or a comma list of app ids");
                }
            }
        }
        if c.active.is_none() {
            c.restore(now);
        }
        if c.profile == ProfileKind::RealAccounts {
            // At restart it ends: nothing a real-accounts home saved counts.
            let _ = std::fs::remove_file(c.home.join(STATE_FILE));
        }
        c
    }

    fn refused(&mut self, now: u64, origin: Origin, why: &str) {
        eprintln!("dev-mode: not turned on from {}: {why}", origin.as_str());
        self.audit.append(now, "mode_refused", json!({"origin": origin.as_str(), "reason": why}));
    }

    /// A developer profile's saved state, when this build may keep it.
    fn restore(&mut self, now: u64) {
        if self.profile != ProfileKind::Developer || !self.launch.build.honours_settings(self.launch.flag_all) {
            return;
        }
        let Ok(text) = std::fs::read_to_string(self.home.join(STATE_FILE)) else { return };
        let Ok(saved) = serde_json::from_str::<Value>(&text) else { return };
        // Saved for another home (a copied profile): never carried over.
        if saved["profile_id"].as_str() != Some(self.profile_id.as_str()) {
            let _ = std::fs::remove_file(self.home.join(STATE_FILE));
            return;
        }
        let (Some(scope), Some(origin)) =
            (Scope::from_json(&saved["scope"]), saved["origin"].as_str().and_then(Origin::parse))
        else {
            return;
        };
        let since = saved["since"].as_u64().unwrap_or(now);
        self.active = Some(Active { scope, origin, since, expires: None });
        self.generation += 1;
        self.audit.append(now, "mode_on", self.mode_entry(true));
    }

    fn mode_entry(&self, restored: bool) -> Value {
        let a = self.active.as_ref();
        json!({
            "scope": a.map(|a| a.scope.to_json()),
            "origin": a.map(|a| a.origin.as_str()),
            "expires": a.and_then(|a| a.expires),
            "profile": match self.profile { ProfileKind::Developer => "developer", ProfileKind::RealAccounts => "real-accounts" },
            "restored": restored,
        })
    }

    /// Turn developer mode on: the person's gesture, never anything else.
    pub fn turn_on(&mut self, gesture: PersonGesture, scope: Scope, now: u64) -> Result<(), String> {
        self.activate(gesture, scope, now, true)
    }

    fn activate(&mut self, gesture: PersonGesture, scope: Scope, now: u64, from_settings: bool) -> Result<(), String> {
        let origin = gesture.origin;
        if from_settings && !self.launch.build.honours_settings(self.launch.flag_all) {
            let why = match self.launch.build {
                BuildKind::Store => "a store build never turns on developer mode",
                _ => "a release build turns on developer mode only when launched with --dev-grant-all",
            };
            self.refused(now, origin, why);
            return Err(why.into());
        }
        let expires = match self.profile {
            ProfileKind::Developer => None,
            ProfileKind::RealAccounts => Some(now + REAL_ACCOUNT_LIMIT_S),
        };
        self.active = Some(Active { scope, origin, since: now, expires });
        self.generation += 1;
        if self.profile == ProfileKind::RealAccounts {
            self.notices.push(
                "This OctoSense home has your real accounts. Developer mode ends after 8 hours or at restart; \
                 for longer, use a developer profile (its own OCTOSENSE_HOME with a `developer-profile` file)."
                    .into(),
            );
        }
        self.audit.append(now, "mode_on", self.mode_entry(false));
        self.save();
        Ok(())
    }

    fn save(&self) {
        let path = self.home.join(STATE_FILE);
        match (&self.active, self.profile) {
            (Some(a), ProfileKind::Developer) => {
                let saved = json!({
                    "scope": a.scope.to_json(),
                    "origin": a.origin.as_str(),
                    "since": a.since,
                    "profile_id": self.profile_id,
                });
                let _ = std::fs::create_dir_all(&self.home);
                if let Err(e) = std::fs::write(&path, saved.to_string()) {
                    eprintln!("dev-mode: could not save {}: {e}", path.display());
                }
            }
            _ => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    /// Turn it off (anyone may; `reason` goes to the audit log).
    pub fn turn_off(&mut self, reason: &str, now: u64) {
        let Some(was) = self.active.take() else { return };
        self.generation += 1;
        self.audit.append(now, "mode_off", json!({"reason": reason, "origin": was.origin.as_str(), "since": was.since}));
        self.save();
    }

    /// End an expired mode. True when something changed.
    pub fn tick(&mut self, now: u64) -> bool {
        match &self.active {
            Some(Active { expires: Some(t), .. }) if now >= *t => {
                self.turn_off("expired", now);
                true
            }
            _ => false,
        }
    }

    pub fn active(&self, now: u64) -> Option<&Active> {
        self.active.as_ref().filter(|a| a.expires.is_none_or(|t| now < t))
    }
    pub fn grants_all(&self, app: &str, now: u64) -> bool {
        self.active(now).is_some_and(|a| a.scope.covers(app))
    }
    pub fn auto_approve(&self, app: &str, now: u64) -> bool {
        self.grants_all(app, now)
    }
    /// Whether developer mode answers this approval itself: for every kind,
    /// whatever the tool declares (`auto_approvable: false` changes
    /// nothing), when it covers the owning app and the call came over the
    /// shell's host connection. A Talk to Octos client's approvals are its
    /// own (ADR 0003) and never answered here.
    pub fn answers_approval(&self, owning_app: &str, _kind: ApprovalKind, _auto_approvable: bool, connection: Connection, now: u64) -> bool {
        connection == Connection::Host && self.grants_all(owning_app, now)
    }
    /// octos's `approval/requested`, seen by an app peer broker: answered
    /// here only on an app agent's own peer session. A turn on any session a
    /// Talk to Octos client can reach (the system conversation, a web
    /// client's own) keeps its normal path: the client that started it
    /// answers, even with developer mode on for all apps.
    pub fn answers_octos_approval(&self, app_id: &str, session_key: &str, now: u64) -> bool {
        is_app_peer_session(session_key)
            && self.answers_approval(app_id, ApprovalKind::OctosApproval, false, Connection::Host, now)
    }
    /// See [`may_register_dev_run`](fn@may_register_dev_run).
    pub fn may_register_dev_run(&self, app: &str, connection: Connection, session_key: &str, now: u64) -> bool {
        self.grants_all(app, now) && dev_grants_allowed_on(connection, session_key).is_ok()
    }
    pub fn profile(&self) -> ProfileKind {
        self.profile
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn audit(&self) -> &Audit {
        &self.audit
    }
    /// Whether Settings offers Developer options at all in this run.
    pub fn settings_available(&self) -> bool {
        self.launch.build.honours_settings(self.launch.flag_all)
    }
    pub fn take_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notices)
    }
    pub fn tag(&self, now: u64) -> Option<DevTag> {
        self.active(now).map(|a| DevTag { profile_id: self.profile_id.clone(), since: a.since })
    }
    /// A tagged grant or rule holds only in the developer mode that made it,
    /// in the profile that made it.
    pub fn tag_valid(&self, tag: &DevTag, now: u64) -> bool {
        self.tag(now).as_ref() == Some(tag)
    }
}

// ---------------------------------------------------------------- the shell's

static GLOBAL: Mutex<Option<Controller>> = Mutex::new(None);

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn with<R>(f: impl FnOnce(&mut Controller) -> R) -> Option<R> {
    GLOBAL.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(f)
}

/// At startup, once: this home and this process's launch.
pub fn init(home: &Path) {
    // octos approvals in app agents' contexts ask developer mode first.
    octosense_ai_host::app_peers::host_approvals::set_override(answer_octos_approval);
    let c = Controller::start(home, Launch::from_process(), now());
    if let Some(a) = c.active(now()) {
        eprintln!("dev-mode: ON ({:?}, from {}); audit: {}", a.scope, a.origin.as_str(), c.audit.path().display());
    }
    *GLOBAL.lock().unwrap_or_else(|e| e.into_inner()) = Some(c);
}

/// See [`Controller::turn_on`].
pub fn turn_on(gesture: PersonGesture, scope: Scope) -> Result<(), String> {
    with(|c| c.turn_on(gesture, scope, now())).unwrap_or_else(|| Err("developer mode is not set up".into()))
}
pub fn turn_off(reason: &str) {
    with(|c| c.turn_off(reason, now()));
}
/// Expire; true when it changed.
pub fn tick() -> bool {
    with(|c| c.tick(now())).unwrap_or(false)
}
/// The mode in force, if on.
pub fn status() -> Option<(Active, ProfileKind)> {
    with(|c| c.active(now()).cloned().map(|a| (a, c.profile()))).flatten()
}
pub fn is_on() -> bool {
    status().is_some()
}
/// Whether `app` (an app id or its AI service id) has every grant.
pub fn grants_all(app: &str) -> bool {
    with(|c| c.grants_all(app, now())).unwrap_or(false)
}
/// Whether an approval for `app`'s tool is answered yes automatically.
pub fn auto_approve(app: &str) -> bool {
    with(|c| c.auto_approve(app, now())).unwrap_or(false)
}
/// See [`Controller::answers_approval`]. The caller that gets `true` answers
/// yes itself and logs it with [`audit_auto_approval`].
pub fn answers_approval(owning_app: &str, kind: ApprovalKind, auto_approvable: bool, connection: Connection) -> bool {
    with(|c| c.answers_approval(owning_app, kind, auto_approvable, connection, now())).unwrap_or(false)
}
/// Every approval of `app`'s tools, on the host connection (the pane and
/// the app peers are the shell's own).
pub fn overrides_every_approval(app: &str) -> bool {
    answers_approval(app, ApprovalKind::PaneConfirm, false, Connection::Host)
}
/// Seam for the `confirm: app` hand-off (ADR 0004 §8, octos#2567): when this
/// is true the shell does NOT hand the confirmation to the owning app's
/// sheet; it answers the call approved, logs it
/// (`audit_auto_approval(.., ApprovalKind::AppConfirm)`), and passes the call
/// on marked as confirmed. Today's in-process modules that confirm a tool
/// themselves still show their sheet: the module contract has no field to
/// carry a host's confirmation yet.
pub fn overrides_app_confirm(owning_app: &str) -> bool {
    answers_approval(owning_app, ApprovalKind::AppConfirm, false, Connection::Host)
}
/// Seam for granted command execution (`terminal.run`, the system agent's
/// granted commands, `dev.run`): whether a command runs without its live
/// approval. `auto_approvable: false` does not stop it; an external client
/// never gets it. The caller logs the command ([`audit_dev_run`] or
/// [`audit_auto_approval`] with [`ApprovalKind::Command`]).
pub fn approves_command(owning_app: &str, connection: Connection) -> bool {
    answers_approval(owning_app, ApprovalKind::Command, false, connection)
}
/// The app peers' hook (`octosense_app_peers::host_approvals`): octos asked
/// an app agent's context for an approval; developer mode answers it.
pub fn answer_octos_approval(app_id: &str, tool: &str, session_key: &str, params: &Value) -> bool {
    if !with(|c| c.answers_octos_approval(app_id, session_key, now())).unwrap_or(false) {
        return false;
    }
    audit_auto_approval(app_id, tool, &params.to_string(), "octos", ApprovalKind::OctosApproval);
    true
}
pub fn generation() -> u64 {
    with(|c| c.generation()).unwrap_or(0)
}
/// Before [`init`], as a launch without the flag would be.
pub fn settings_available() -> bool {
    with(|c| c.settings_available()).unwrap_or_else(|| BuildKind::current().honours_settings(false))
}
pub fn take_notices() -> Vec<String> {
    with(|c| c.take_notices()).unwrap_or_default()
}
pub fn tag() -> Option<DevTag> {
    with(|c| c.tag(now())).flatten()
}
pub fn tag_valid(tag: &DevTag) -> bool {
    with(|c| c.tag_valid(tag, now())).unwrap_or(false)
}

/// Whether `dev.run` may be registered on this session now (the seam for
/// octos#2567: the registration code calls this and registers nothing on
/// `false`, and withdraws the tool when [`generation`] moves and this turns
/// false).
pub fn may_register_dev_run(app: &str, connection: Connection, session_key: &str) -> bool {
    with(|c| c.may_register_dev_run(app, connection, session_key, now())).unwrap_or(false)
}

fn audit_if_on(kind: &str, entry: Value) {
    with(|c| {
        let t = now();
        if c.active(t).is_some() {
            c.audit.append(t, kind, entry);
        }
    });
}

/// One tool call, while developer mode is on: the owning app, the tool, the
/// exact arguments and who called.
pub fn audit_tool_call(owning_app: &str, tool: &str, args: &str, caller: &str) {
    audit_if_on("tool_call", json!({"app": owning_app, "tool": tool, "args": args, "caller": caller}));
}
/// One automatic approval developer mode gave, and of which kind.
pub fn audit_auto_approval(owning_app: &str, tool: &str, args: &str, caller: &str, kind: ApprovalKind) {
    audit_if_on(
        "auto_approval",
        json!({"app": owning_app, "tool": tool, "args": args, "caller": caller, "approval": kind.as_str()}),
    );
}
/// One `dev.run` command and how it ended.
pub fn audit_dev_run(app: &str, command: &str, cwd: Option<&str>, exit: Option<i32>) {
    audit_if_on("dev_run", json!({"app": app, "command": command, "cwd": cwd, "exit": exit}));
}

/// The banner's line.
pub fn banner_text(active: &Active) -> String {
    match &active.scope {
        Scope::AllApps => "Developer mode: all apps have full access".into(),
        Scope::Apps(apps) => format!(
            "Developer mode: {} have full access",
            apps.iter().cloned().collect::<Vec<_>>().join(", ")
        ),
    }
}

/// How long it lasts, for the banner and Settings.
pub fn lasts_text(active: &Active, profile: ProfileKind, now: u64) -> String {
    match (active.expires, profile) {
        (Some(t), _) => {
            let left = t.saturating_sub(now);
            format!("ends in {}h {:02}m or at restart", left / 3600, (left % 3600) / 60)
        }
        (None, _) => "developer profile: on until turned off".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Home(PathBuf);
    impl Home {
        fn new(tag: &str) -> Home {
            let dir = std::env::temp_dir().join(format!(
                "octosense-devmode-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Home(dir)
        }
        fn developer(tag: &str) -> Home {
            let h = Home::new(tag);
            std::fs::write(h.0.join(PROFILE_MARKER), "").unwrap();
            h
        }
        fn audit(&self) -> Vec<Value> {
            std::fs::read_to_string(self.0.join(AUDIT_FILE))
                .unwrap_or_default()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect()
        }
    }
    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn launch(build: BuildKind, env: Option<&str>, flag: bool) -> Launch {
        let args: Vec<String> = if flag { vec!["octosense".into(), FLAG_ALL.into()] } else { vec!["octosense".into()] };
        Launch::from_parts(build, env.map(str::to_owned), &args)
    }

    const T0: u64 = 1_800_000_000;

    #[test]
    fn activation_sources_per_build_kind() {
        use BuildKind::*;
        // (build, env, flag) → on at launch?
        for (build, env, flag, on) in [
            (Development, None, false, false),
            (Development, Some("all"), false, true),
            (Development, None, true, true),
            (Release, None, false, false),
            (Release, Some("all"), false, false),
            (Release, None, true, true),
            (Store, Some("all"), false, false),
            (Store, None, true, false),
        ] {
            let home = Home::new("matrix");
            let c = Controller::start(&home.0, launch(build, env, flag), T0);
            assert_eq!(c.active(T0).is_some(), on, "{build:?} env={env:?} flag={flag}");
            if on {
                let origin = c.active(T0).unwrap().origin;
                assert_eq!(origin, if flag { Origin::Flag } else { Origin::Env });
            }
        }
        // Settings: a development build; a release build only in a run
        // launched with the flag; a store build never.
        for (build, flag, allowed) in [
            (Development, false, true),
            (Release, false, false),
            (Release, true, true),
            (Store, false, false),
            (Store, true, false),
        ] {
            let home = Home::new("settings");
            let mut c = Controller::start(&home.0, launch(build, None, flag), T0);
            c.turn_off("test", T0);
            let g = PersonGesture::settings_phrase(CONFIRM_PHRASE).unwrap();
            assert_eq!(c.turn_on(g, Scope::AllApps, T0).is_ok(), allowed, "{build:?} flag={flag}");
            assert_eq!(c.settings_available(), allowed);
            assert_eq!(c.active(T0).is_some(), allowed);
        }
    }

    /// Developer mode answers every kind of approval for a covered app,
    /// `auto_approvable: false` included; never an uncovered app's, never an
    /// external client's, and nothing once it is off.
    #[test]
    fn every_approval_is_answered_for_a_covered_app() {
        use ApprovalKind::*;
        let home = Home::new("approvals");
        let mut c = Controller::start(&home.0, launch(BuildKind::Development, Some("terminal,os.mail"), false), T0);
        for kind in [PaneConfirm, AppConfirm, OctosApproval, HostConfirm, Command] {
            for auto_approvable in [true, false] {
                assert!(c.answers_approval("terminal", kind, auto_approvable, Connection::Host, T0), "{kind:?}");
                assert!(c.answers_approval("mail", kind, auto_approvable, Connection::Host, T0), "{kind:?}");
                assert!(!c.answers_approval("os.photos", kind, auto_approvable, Connection::Host, T0), "{kind:?}");
                assert!(!c.answers_approval("terminal", kind, auto_approvable, Connection::External, T0), "{kind:?}");
            }
        }
        c.turn_off("test", T0);
        assert!(!c.answers_approval("terminal", Command, false, Connection::Host, T0));
    }

    /// ADR 0003 × §13: with developer mode on for ALL apps, a Talk to Octos
    /// external connection still gets no developer grant, no `dev.run`, no
    /// command approval, and no automatic answer to an approval raised on a
    /// turn it can reach; app agents' own peer sessions do.
    #[test]
    fn external_connections_get_nothing_even_with_developer_mode_on_for_all() {
        use ApprovalKind::*;
        let home = Home::new("external-all");
        let c = Controller::start(&home.0, launch(BuildKind::Development, Some("all"), false), T0);
        assert!(c.grants_all("anything", T0), "on for all apps");
        let system = "_main:api:octosense#system";
        let web = "_main:api:web#chat-1";
        let peer = "_main:api:octosense#peer-k3f9";
        let context = "_main:api:octosense#peerctx-k3f9-1";
        for session in [system, web, peer, context, "_main"] {
            assert!(!c.may_register_dev_run("os.mail", Connection::External, session, T0), "{session}");
            for kind in [PaneConfirm, AppConfirm, OctosApproval, HostConfirm, Command] {
                assert!(!c.answers_approval("os.mail", kind, false, Connection::External, T0), "{kind:?}");
            }
        }
        // An approval on a turn an external client can reach goes to its
        // normal path; only an app agent's own sessions are answered here.
        for session in [system, web, "_main", "_main:api:octosense#peer"] {
            assert!(!c.answers_octos_approval("os.mail", session, T0), "{session}");
            assert!(!c.may_register_dev_run("os.mail", Connection::Host, session, T0), "{session}");
        }
        for session in [peer, context] {
            assert!(c.answers_octos_approval("os.mail", session, T0), "{session}");
            assert!(c.may_register_dev_run("os.mail", Connection::Host, session, T0), "{session}");
        }
    }

    #[test]
    fn debug_builds_are_development_builds() {
        // Tests build with debug assertions (build.rs sets `dev_mode`); a
        // `--release` build without `--features dev-mode` is `Release`.
        assert_eq!(BuildKind::current(), BuildKind::Development);
    }

    #[test]
    fn the_confirmation_phrase_must_be_typed_exactly() {
        assert!(PersonGesture::settings_phrase("  Turn on   developer MODE ").is_some());
        for wrong in ["", "turn on", "turn on developer mode please", "yes"] {
            assert!(PersonGesture::settings_phrase(wrong).is_none(), "{wrong:?}");
        }
    }

    #[test]
    fn chosen_apps_from_the_environment() {
        let home = Home::new("chosen");
        let c = Controller::start(&home.0, launch(BuildKind::Development, Some("os.mail, news"), false), T0);
        assert!(c.grants_all("os.mail", T0));
        assert!(c.grants_all("mail", T0), "a system app's AI service is the app");
        assert!(c.grants_all("news", T0) && c.grants_all("os.news", T0));
        assert!(!c.grants_all("os.maps", T0));
        assert!(!c.auto_approve("os.maps", T0));
        let off = Controller::start(&Home::new("zero").0, launch(BuildKind::Development, Some("0"), false), T0);
        assert!(off.active(T0).is_none(), "OCTOSENSE_DEV_MODE=0 is off");
    }

    #[test]
    fn with_real_accounts_it_warns_and_ends_after_eight_hours_or_at_restart() {
        let home = Home::new("real");
        let mut c = Controller::start(&home.0, launch(BuildKind::Development, None, false), T0);
        assert_eq!(c.profile(), ProfileKind::RealAccounts);
        c.turn_on(PersonGesture::settings_phrase(CONFIRM_PHRASE).unwrap(), Scope::AllApps, T0).unwrap();
        assert_eq!(c.take_notices().len(), 1, "the person is warned");
        assert_eq!(c.active(T0).unwrap().expires, Some(T0 + REAL_ACCOUNT_LIMIT_S));
        assert!(!home.0.join(STATE_FILE).exists(), "nothing persists with real accounts");
        let before = c.generation();
        assert!(!c.tick(T0 + REAL_ACCOUNT_LIMIT_S - 1));
        assert!(c.grants_all("os.mail", T0 + REAL_ACCOUNT_LIMIT_S - 1));
        assert!(!c.grants_all("os.mail", T0 + REAL_ACCOUNT_LIMIT_S), "a query never sees an expired mode");
        assert!(c.tick(T0 + REAL_ACCOUNT_LIMIT_S));
        assert!(c.generation() > before);
        assert!(c.active(T0).is_none());
        // Restart: off.
        let c = Controller::start(&home.0, launch(BuildKind::Development, None, false), T0 + 10);
        assert!(c.active(T0 + 10).is_none());
        let kinds: Vec<String> = home.audit().iter().map(|e| e["kind"].as_str().unwrap().to_owned()).collect();
        assert_eq!(kinds, ["mode_on", "mode_off"]);
        assert_eq!(home.audit()[1]["reason"], "expired");
    }

    #[test]
    fn in_a_developer_profile_it_persists_until_turned_off() {
        let home = Home::developer("devprofile");
        let mut c = Controller::start(&home.0, launch(BuildKind::Development, None, false), T0);
        assert_eq!(c.profile(), ProfileKind::Developer);
        c.turn_on(PersonGesture::settings_phrase(CONFIRM_PHRASE).unwrap(), Scope::AllApps, T0).unwrap();
        assert!(c.take_notices().is_empty(), "no warning in a developer profile");
        assert_eq!(c.active(T0).unwrap().expires, None);
        let much_later = T0 + 30 * 24 * 3600;
        assert!(!c.tick(much_later));
        assert!(c.grants_all("anything", much_later));
        // Restart: still on, same origin.
        let mut c = Controller::start(&home.0, launch(BuildKind::Development, None, false), much_later);
        let a = c.active(much_later).expect("restored");
        assert_eq!((a.origin, a.since), (Origin::Settings, T0));
        // A release build without the flag does not pick it up.
        let r = Controller::start(&home.0, launch(BuildKind::Release, None, false), much_later);
        assert!(r.active(much_later).is_none());
        // Turned off: gone for good.
        c.turn_off("settings", much_later);
        assert!(!home.0.join(STATE_FILE).exists());
        let c = Controller::start(&home.0, launch(BuildKind::Development, None, false), much_later);
        assert!(c.active(much_later).is_none());
    }

    #[test]
    fn nothing_made_in_developer_mode_carries_to_another_profile() {
        let a = Home::developer("profile-a");
        let b = Home::developer("profile-b");
        let mut ca = Controller::start(&a.0, launch(BuildKind::Development, Some("all"), false), T0);
        let tag = ca.tag(T0).expect("tagged while on");
        assert!(ca.tag_valid(&tag, T0));
        // Another profile, even in developer mode itself, does not honour it.
        let cb = Controller::start(&b.0, launch(BuildKind::Development, Some("all"), false), T0);
        assert!(!cb.tag_valid(&tag, T0));
        // A copied profile's saved state is not restored in the copy.
        std::fs::copy(a.0.join(STATE_FILE), b.0.join(STATE_FILE)).unwrap();
        let cb = Controller::start(&b.0, launch(BuildKind::Development, None, false), T0);
        assert!(cb.active(T0).is_none(), "state saved for profile A never turns B on");
        assert!(!b.0.join(STATE_FILE).exists());
        // Off, the tag is dead in its own profile too.
        ca.turn_off("test", T0);
        assert!(!ca.tag_valid(&tag, T0));
    }

    #[test]
    fn mode_changes_and_calls_are_audited() {
        let home = Home::new("audit");
        let mut c = Controller::start(&home.0, launch(BuildKind::Release, Some("all"), false), T0);
        assert!(c.active(T0).is_none());
        c.turn_on(PersonGesture::settings_phrase(CONFIRM_PHRASE).unwrap(), Scope::AllApps, T0).unwrap_err();
        let log = home.audit();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0]["kind"], "mode_refused");
        assert_eq!(log[0]["origin"], "env");
        assert_eq!(log[1]["kind"], "mode_refused");
        assert_eq!(log[1]["origin"], "settings");
        let mut c = Controller::start(&home.0, launch(BuildKind::Release, None, true), T0 + 1);
        c.audit().append(T0 + 2, "tool_call", json!({"app": "os.mail", "tool": "send", "args": "{\"to\":\"a\"}"}));
        c.turn_off("banner", T0 + 3);
        let log = home.audit();
        assert_eq!(log.len(), 5);
        assert_eq!((log[2]["kind"].as_str(), log[2]["origin"].as_str()), (Some("mode_on"), Some("flag")));
        assert_eq!(log[3]["args"], "{\"to\":\"a\"}");
        assert_eq!((log[4]["kind"].as_str(), log[4]["reason"].as_str()), (Some("mode_off"), Some("banner")));
        assert!(log.iter().all(|e| e["ts"].as_u64().is_some()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(c.audit().path()).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "the audit log is the person's alone");
        }
    }

    #[test]
    fn external_clients_never_get_developer_grants_or_dev_run() {
        // ADR 0003: the system conversation is reachable by a paired client;
        // app peer sessions are not.
        let system = "_main:api:octosense#system";
        let peer = "_main:api:octosense#peer-k3f9";
        let context = "_main:api:octosense#peerctx-k3f9-1";
        assert!(dev_grants_allowed_on(Connection::External, peer).is_err());
        assert!(dev_grants_allowed_on(Connection::External, system).is_err());
        assert!(dev_grants_allowed_on(Connection::Host, system).is_err());
        assert!(dev_grants_allowed_on(Connection::Host, "_main").is_err());
        assert!(dev_grants_allowed_on(Connection::Host, "_main:api:octosense#peer").is_err());
        assert!(dev_grants_allowed_on(Connection::Host, peer).is_ok());
        assert!(dev_grants_allowed_on(Connection::Host, context).is_ok());
        // A web client's own sessions (profile `_main`, any topic but a peer's).
        assert!(dev_grants_allowed_on(Connection::Host, "_main:api:web#chat-1").is_err());
    }

    /// The setter takes a `PersonGesture`, and only Settings' own rows and
    /// this module make one. The AI bus, the `os` service, the host
    /// services and every other module never do.
    #[test]
    fn no_agent_app_or_service_can_reach_the_setter() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut makers = Vec::new();
        let mut stack = vec![src.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    let rel = path.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
                    for needle in ["PersonGesture::", "PersonGesture {", "dev_mode::turn_on("] {
                        if text.contains(needle) && rel != "dev_mode.rs" {
                            makers.push(format!("{rel}: {needle}"));
                        }
                    }
                }
            }
        }
        makers.sort();
        makers.dedup();
        // The one caller outside this module: the Settings row's handler.
        assert_eq!(makers, ["lib.rs: PersonGesture::", "lib.rs: dev_mode::turn_on("], "{makers:?}");
        let lib = std::fs::read_to_string(src.join("lib.rs")).unwrap();
        let at = lib.find("PersonGesture::").unwrap();
        let handler = lib[..at].rfind("fn ").map(|i| &lib[i..at]).unwrap();
        assert!(handler.starts_with("fn developer_options_activate"), "only Settings makes a gesture: {}", &handler[..60.min(handler.len())]);
        // The bus and the `os` service expose nothing that turns it on.
        let os = crate::ai_bus::AiBus::os_manifest(&[]);
        assert!(os.tools.iter().all(|t| !t.name.contains("dev")), "no developer tool on the os service");
        assert!(!std::fs::read_to_string(src.join("ai_bus.rs")).unwrap().contains("turn_on"));
        assert!(!std::fs::read_to_string(src.join("glance.rs")).unwrap().contains("dev_mode"));
        // The environment is read once, at launch: an agent setting it later
        // changes nothing.
        assert!(!lib.contains("OCTOSENSE_DEV_MODE"), "only dev_mode.rs reads the switch");
    }
}
