//! Where a craft engine works (ADR 0013, 9 Oct 2026): in its caller's own
//! folder. The engines are OctoSense-wide tools that run in an app's file
//! folders, not in a folder of their own.
//!
//! Every engine service (sheet; with the desktop's `craft-engines`, also
//! photo, with Photos' own `photos.info`, and word, deck, cad, light,
//! sound, design, film, effect, vector and pdf) takes its area from the
//! resolver this module installs at registration ([`install_resolvers`]).
//! It decides from trusted host data alone, never from a call's arguments:
//!
//! | The call | Its area | May replace a file | Quota |
//! | --- | --- | --- | --- |
//! | the system agent's call to a craft engine's tool | its workspace ([`crate::system_chat::workspace`]) | no | none beyond the service's own caps |
//! | an app agent's call to a craft engine's tool | that account's folder, `accounts/<hash>/` in the app's jail ([`super::agent_workspace_in`]) | no | what is left of the app's storage |
//! | any agent's call to an app's own engine tool (Sheets' `sheets.*`, Photos' `photos.info`) | that app's agent folder: its own agent's account, else the account the app acts for now | no | what is left of the app's storage |
//! | an app's own `host.request` | its storage, the jail its `fs.*` sees | in the foreground only | what is left of the app's storage |
//!
//! The craft engines' tools belong to no app (their owners `os.<family>`
//! are virtual, `engines.rs`), so they work in the caller's own folder. An
//! app's own tools work on that app's data, as every app tool does,
//! whoever was granted them: the system agent's `sheets.get` reads the
//! workbook Sheets' agent opened, in Sheets' folder.
//!
//! **The plumbing.** A tool call reaches its engine through an executor
//! (`engines.rs`'s, or a script app's for Photos' `photos.info`), which
//! knows who called: the [`HostToolCall`]'s broker-authenticated app,
//! account and caller kind. It resolves the agent's area ([`grant_agent`])
//! before dispatch and puts a unique [`Grant`] token in the `ServiceCall`'s
//! host-only `host_dir`. For a Card request the worker submission hook binds
//! the app's current account. Both grants live through the queued call; the
//! worker rechecks the captured identity and current quota in [`resolve_in`].
//! A service run without the resolver (tests, App Hub's card-host)
//! keeps its legacy private folder `<host dir>/<family>`.
//!
//! A signed-out, suspended (an uninstalled app's) or refused account is
//! refused, and says why. What each service then lets a call do there
//! (contained paths, no replacing for agents, the quota, the paths written
//! inside documents) is the service's, through `octosense_engine_area`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use octosense_appstore::services::ServiceCall;
pub use octosense_engine_area::Area;
use serde_json::Value;

use crate::ai_host::app_peers::host_tools::{CallerKind, HostToolCall, ToolOutcome, ToolReply};
use crate::app_storage::Storage;

/// The engines every shell with App Hub links: the native Sheets app's
/// agent tools run on the sheet engine, on the phone too.
const ENGINES: &[&str] = &["sheet"];

/// The engines only the desktop links (`craft-engines`): the photo engine,
/// which Photos' `photos.info` runs on, and the ten behind the system agent.
#[cfg(feature = "craft-engines")]
const CRAFT_ENGINES: &[&str] = &["photo", "word", "deck", "cad", "light", "sound", "design", "film", "effect", "vector", "pdf"];
#[cfg(not(feature = "craft-engines"))]
const CRAFT_ENGINES: &[&str] = &[];

/// Whether `method` (`family.name`) works in its caller's area: every
/// method of a linked engine's family, and Photos' own `photos.info`, which
/// runs on the photo engine where it is linked (not `photos.notify`, which
/// touches no file). A method on an engine this build leaves out works
/// nowhere: its executor refuses it before any area
/// (`script_apps::unlinked_engine`).
pub fn needs_area(method: &str) -> bool {
    let family = method.split('.').next().unwrap_or("");
    let photos_info = method == "photos.info" && CRAFT_ENGINES.contains(&"photo");
    photos_info || ENGINES.contains(&family) || CRAFT_ENGINES.contains(&family)
}

// ------------------------------------------------------------ what the host knows

/// An app's storage as the engines see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JailQuota {
    /// The whole jail's ceiling in bytes (`None`: none declared, a native
    /// app's without `storage.max_bytes`).
    pub bytes: Option<u64>,
    /// The app's isolate has storage of its own (App Hub's `storage`
    /// capability): an app without it sees no folder.
    pub storage: bool,
}

/// What the resolver reads: the shell's ([`shell`]), or a test's.
pub trait AreaEnv: Send + Sync {
    /// The host's app storage, when it is set up.
    fn storage(&self) -> Option<&Storage>;
    /// The system agent's workspace, when known.
    fn system_workspace(&self) -> Option<PathBuf>;
    /// `app`'s storage: its ceiling, and whether it has any.
    fn jail_quota(&self, app: &str) -> Result<JailQuota, String>;
    /// The account `app`'s own surface acts for now (`"device"` for an app
    /// without accounts; `None` when it keeps accounts and none is signed
    /// in).
    fn current_account(&self, app: &str) -> Option<String>;
}

/// The shell's [`AreaEnv`].
pub fn shell() -> Arc<dyn AreaEnv> {
    Arc::new(ShellEnv)
}

struct ShellEnv;

impl AreaEnv for ShellEnv {
    fn storage(&self) -> Option<&Storage> {
        crate::app_storage::host().map(|storage| &**storage)
    }

    fn system_workspace(&self) -> Option<PathBuf> {
        crate::system_chat::workspace::current()
    }

    fn jail_quota(&self, app: &str) -> Result<JailQuota, String> {
        if crate::native_apps::find(app).is_some() {
            let storage = self.storage().ok_or("this host keeps no app storage")?;
            return Ok(JailQuota { bytes: storage.spec(app).max_bytes, storage: true });
        }
        // A system app's policy is packed into this build, so it is admitted
        // once (admission hashes the whole bundle); an installed app's can
        // change with an update, so it is read each time.
        static SYSTEM: Mutex<Option<std::collections::HashMap<String, JailQuota>>> = Mutex::new(None);
        let system = octosense_appstore::system::system_app(app).is_some();
        if system {
            if let Some(quota) = SYSTEM.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|known| known.get(app).copied()) {
                return Ok(quota);
            }
        }
        let policy = admitted_policy(app)?;
        let quota = JailQuota { bytes: Some(policy.storage_bytes), storage: true };
        if system {
            SYSTEM.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(app.to_string(), quota);
        }
        Ok(quota)
    }

    fn current_account(&self, app: &str) -> Option<String> {
        crate::app_storage::lifecycle::contained_account_in(self.storage()?, app)
    }
}

/// A script app's policy as App Hub admits it now: a system app's from its
/// packed bundle (under App Hub's system ceilings), an installed one's from
/// its verified install (the store's ceilings). Its storage ceiling is the
/// one its isolate's `fs.*` keeps to.
fn admitted_policy(app: &str) -> Result<octosense_app_contract::AppPolicy, String> {
    crate::apps::check_script_app_id(app)?;
    let root = octosense_appstore::data_root_if_set().ok_or("App Hub has no apps root yet")?;
    if let Some(system) = octosense_appstore::system::system_app(app) {
        return octosense_appstore::system::prepare(&root, &system).map(|(_, policy)| policy.app);
    }
    let bundle = super::admission::installed_bundle(&root, app)?;
    let text = std::fs::read_to_string(bundle.join(octosense_app_contract::MANIFEST_FILE)).map_err(|e| format!("{app}: {e}"))?;
    let manifest = octosense_app_contract::AppManifest::parse(&text)?;
    // The install was verified (signature included) by `installed_bundle`.
    octosense_app_contract::resolve(&manifest, &octosense_app_contract::HostLimits::default().with_require_signature(false))
}

// ------------------------------------------------------------ grants

/// A relay call or queued app request carries a host-only token in
/// `host_dir`. The token has no filesystem meaning and identifies exactly
/// one grant, even when two calls use the same folder.
static NEXT_GRANT: AtomicU64 = AtomicU64::new(1);
static GRANTS: Mutex<Vec<(PathBuf, octosense_engine_area::Resolver)>> = Mutex::new(Vec::new());

pub struct Grant {
    pub(super) token: PathBuf,
    live: Arc<AtomicBool>,
}

fn grant(
    owner: String,
    check: impl Fn() -> Result<(), String> + Send + Sync + 'static,
    area: impl Fn() -> Result<Area, String> + Send + Sync + 'static,
) -> Grant {
    let token = PathBuf::from(format!("engine-call-{}", NEXT_GRANT.fetch_add(1, Ordering::Relaxed)));
    let live = Arc::new(AtomicBool::new(true));
    let active = live.clone();
    let scope = Arc::new(move || {
        if !active.load(Ordering::Acquire) {
            return Err("the engine call was cancelled".into());
        }
        check()
    });
    let bound = Arc::new(move |call: &ServiceCall| {
        if call.app_id != owner {
            return Err("this engine call belongs to another app".into());
        }
        scope()?;
        Ok(area()?.with_scope_check(scope.clone()))
    });
    GRANTS.lock().unwrap_or_else(|e| e.into_inner()).push((token.clone(), bound));
    Grant { token, live }
}

impl Drop for Grant {
    fn drop(&mut self) {
        self.live.store(false, Ordering::Release);
        GRANTS.lock().unwrap_or_else(|e| e.into_inner()).retain(|(token, _)| token != &self.token);
    }
}

fn granted(token: &Path) -> Option<octosense_engine_area::Resolver> {
    GRANTS.lock().unwrap_or_else(|e| e.into_inner()).iter()
        .find(|(key, _)| key == token).map(|(_, resolve)| resolve.clone())
}

// ------------------------------------------------------------ agents

/// Whether `owner`'s tools belong to no app: a craft engine's virtual owner.
fn belongs_to_no_app(owner: &str) -> bool {
    #[cfg(feature = "craft-engines")]
    {
        super::engines::is_virtual_owner(owner)
    }
    #[cfg(not(feature = "craft-engines"))]
    {
        let _ = owner;
        false
    }
}

/// The peer id of `app`'s agent: a native app's id, a script app's
/// `card.<app id>` (`agents::peer_of`).
fn peer_of_app(app: &str) -> String {
    if crate::native_apps::find(app).is_some() {
        app.to_string()
    } else {
        format!("{}{app}", crate::ai_host::contained::PEER_PREFIX)
    }
}

/// Capture the target account before handing the call to the engine worker.
#[derive(Clone)]
enum AgentTarget {
    System,
    Account { peer: String, account: Option<String>, current: bool },
}

impl AgentTarget {
    fn for_call(env: &dyn AreaEnv, call: &HostToolCall, owner: &str) -> Self {
        let own = call.caller_kind == CallerKind::AppPeer && super::app_of_peer(&call.calling_app) == owner;
        if !belongs_to_no_app(owner) && !own {
            return Self::Account { peer: peer_of_app(owner), account: env.current_account(owner), current: true };
        }
        match call.caller_kind {
            CallerKind::System => Self::System,
            CallerKind::AppPeer => Self::Account { peer: call.calling_app.clone(), account: call.account.clone(), current: false },
        }
    }

    fn root(&self, env: &dyn AreaEnv) -> Result<PathBuf, (&'static str, String)> {
        match self {
            Self::System => env.system_workspace().ok_or(("no_workspace", "the system agent's workspace is not known yet: it is the folder its conversation opens in, so open the Assistant once".into())),
            Self::Account { peer, account, current } => {
                if *current && env.current_account(super::app_of_peer(peer)) != *account {
                    return Err(("signed_out", "the owning app's active account changed".into()));
                }
                agent_folder(env, peer, account.as_deref())
            }
        }
    }

    fn area(&self, env: &dyn AreaEnv, root: PathBuf) -> Result<Area, String> {
        match self {
            Self::System => Ok(Area::new(root, None, false)),
            Self::Account { peer, .. } => {
                let app = super::app_of_peer(peer);
                let storage = env.storage().ok_or("this host keeps no app storage")?;
                storage_area(storage, app, root, env.jail_quota(app)?, false)
            }
        }
    }
}

pub(super) fn grant_agent(env: Arc<dyn AreaEnv>, call: &HostToolCall, owner: &str) -> Result<(PathBuf, Grant), (&'static str, String)> {
    let target = AgentTarget::for_call(&*env, call, owner);
    let root = target.root(&*env)?;
    let expected = root.clone();
    let scope = target.clone();
    let scope_env = env.clone();
    let check = move || {
        let now = scope.root(&*scope_env).map_err(|(_, why)| why)?;
        if now != expected {
            return Err("the engine workspace changed while the call was queued".into());
        }
        Ok(())
    };
    let path = root.clone();
    let grant = grant(owner.to_owned(), check, move || target.area(&*env, path.clone()));
    Ok((root, grant))
}

/// The folder of `peer`'s agent for `account` (`None`: the device's, for an
/// app without accounts), refused while that account is signed out or its
/// folder refused, or when the agent has no files.
fn agent_folder(env: &dyn AreaEnv, peer: &str, account: Option<&str>) -> Result<PathBuf, (&'static str, String)> {
    let storage = env.storage().ok_or(("no_workspace", "this host keeps no app storage".to_string()))?;
    let app = super::app_of_peer(peer);
    let account = match account {
        Some(account) => account,
        None if !super::keeps_accounts(storage, peer) => crate::app_storage::DEVICE,
        None => return Err(("signed_out", format!("{app} has no signed-in account"))),
    };
    if super::suspended_in(storage, peer, Some(account)) {
        return Err(("signed_out", format!("{app}'s account is signed out")));
    }
    if let Some(why) = super::workspace_refused_in(storage, peer, account) {
        return Err(("workspace_refused", format!("{app}'s folder for the account was refused: {why}")));
    }
    super::agent_workspace_in(storage, peer, account).ok_or(("no_workspace", format!("{app}'s agent has no folder of its own")))
}

/// The engine preflights against the remaining quota, then checks the
/// whole app's jail again when committing its writes.
fn storage_area(storage: &Storage, app: &str, root: PathBuf, quota: JailQuota, may_replace: bool) -> Result<Area, String> {
    if !quota.storage {
        return Err(format!("{app} has no storage of its own, so an engine has no folder to work in for it"));
    }
    let Some(ceiling) = quota.bytes else { return Ok(Area::new(root, None, may_replace)) };
    let jail = storage.layout().app(app)?.jail;
    let used = storage.usage(app).map_err(|e| format!("{app}: {e}"))?.jail_bytes;
    Ok(Area::new(root, Some(ceiling.saturating_sub(used)), may_replace).with_jail_quota(jail, ceiling))
}

// ------------------------------------------------------------ apps

/// The area of `app`'s own request: its storage, the jail its `fs.*` sees
/// (App Hub roots an isolate's storage at the jail, not at an account's
/// folder), so the app finds an engine's output where it looks. Only an
/// app with storage has one, and only while its account is signed in and
/// its jail was not refused; a foreground call may replace a file, a
/// background one (a glance tile's) may not; within what is left of its
/// storage.
#[cfg(wasm_functions)]
pub(crate) fn app_area(env: &dyn AreaEnv, app: &str, may_prompt: bool) -> Result<Area, String> {
    active_app_account(env, app)?;
    app_jail_area(env, app, may_prompt)
}

/// Resolve the jail after the caller has checked the app's current access.
fn app_jail_area(env: &dyn AreaEnv, app: &str, may_prompt: bool) -> Result<Area, String> {
    let storage = env.storage().ok_or("this host keeps no app storage")?;
    let jail = storage.layout().app(app)?.jail;
    let area = storage_area(storage, app, jail, env.jail_quota(app)?, may_prompt)?;
    crate::app_storage::ensure_private_dir(storage.layout().apps_root(), &area.root).map_err(|e| format!("{app}: {e}"))?;
    Ok(area)
}

// ------------------------------------------------------------ the resolver

/// Get the active account before reading the spec: production discovers a
/// previously unseen app's account settings while looking it up.
fn app_account(env: &dyn AreaEnv, app: &str) -> Result<Option<String>, String> {
    crate::apps::check_script_app_id(app)?;
    let storage = env.storage().ok_or("this host keeps no app storage")?;
    let current = env.current_account(app);
    if storage.spec(app).accounts {
        current.map(Some).ok_or_else(|| format!("{app} has no signed-in account"))
    } else {
        Ok(None)
    }
}

/// Both Wasm calls and queued engine grants must honor sign-out and storage refusal.
fn active_app_account(env: &dyn AreaEnv, app: &str) -> Result<Option<String>, String> {
    let account = app_account(env, app)?;
    let storage = env.storage().ok_or("this host keeps no app storage")?;
    if storage.is_signed_out(app, account.as_deref()) {
        return Err(format!("{app}'s account is signed out"));
    }
    if let Some(why) = storage.refused(app, account.as_deref()) {
        return Err(format!("{app}'s folder was refused: {why}"));
    }
    Ok(account)
}

fn prepare_app(env: Arc<dyn AreaEnv>, call: &mut ServiceCall) -> Option<Grant> {
    // Agent tools carry their own per-call grant. Only the Card runner
    // supplies the shared host directory for an app's own request.
    let shared = env.storage().map(|storage| storage.layout().apps_root().join(".host"));
    if shared.as_ref() != Some(&call.host_dir) {
        return None;
    }
    let app = call.app_id.clone();
    let may_replace = call.may_prompt;
    // Some engine methods (sheet.eval without a book, command catalogs)
    // need no files. Carry an unavailable account to the area resolver so
    // it refuses only methods which actually request an area.
    let account = app_account(&*env, &app);
    let scope_env = env.clone();
    let owner = app.clone();
    let check = move || {
        let account = account.as_ref().map_err(Clone::clone)?;
        if active_app_account(&*scope_env, &owner)? != *account {
            return Err(format!("{owner}'s active account changed while its engine call was queued"));
        }
        Ok(())
    };
    let grant = grant(app.clone(), check, move || app_jail_area(&*env, &app, may_replace));
    call.host_dir = grant.token.clone();
    Some(grant)
}

fn prepare(call: &mut ServiceCall) -> Result<Option<Box<dyn Send>>, String> {
    Ok(prepare_app(shell(), call).map(|grant| Box::new(grant) as Box<dyn Send>))
}

pub fn resolve_in(call: &ServiceCall) -> Result<Area, String> {
    let resolve = granted(&call.host_dir).ok_or("this call names no folder the host gave it")?;
    resolve(call)
}

/// The resolver the shell installs on every engine service.
pub fn resolver() -> octosense_engine_area::Resolver {
    Arc::new(resolve_in)
}

/// At startup, beside the engines' registration: every engine works in its
/// caller's area from now on.
pub fn install_resolvers() {
    octosense_engine_area::worker::set_prepare(prepare);
    let resolver = resolver();
    // The desktop's engines (`craft-engines`): the photo engine and the ten.
    #[cfg(feature = "craft-engines")]
    {
        octosense_photo_service::set_area_resolver(Some(resolver.clone()));
        octosense_word_service::set_area_resolver(Some(resolver.clone()));
        octosense_deck_service::set_area_resolver(Some(resolver.clone()));
        octosense_cad_service::set_area_resolver(Some(resolver.clone()));
        octosense_light_service::set_area_resolver(Some(resolver.clone()));
        octosense_sound_service::set_area_resolver(Some(resolver.clone()));
        octosense_design_service::set_area_resolver(Some(resolver.clone()));
        octosense_film_service::set_area_resolver(Some(resolver.clone()));
        octosense_effect_service::set_area_resolver(Some(resolver.clone()));
        octosense_vector_service::set_area_resolver(Some(resolver.clone()));
        octosense_pdf_service::set_area_resolver(Some(resolver.clone()));
    }
    // The sheet engine, wherever App Hub is linked (Home too), last: it
    // takes the resolver itself, so no build leaves it unused.
    octosense_sheets_service::set_area_resolver(Some(resolver));
}

// ------------------------------------------------------------ answers

/// `reply`, with the host's own spelling of an area's root taken out of its
/// error messages: a tool's paths are relative to the caller's folder, and
/// the host's layout (the person's home directory in it) is not the
/// model's business. Some engines name the absolute path the service
/// handed them (effectcraft: `cannot read /…/x.ecproj`).
pub fn relative_errors(reply: ToolReply, root: &Path) -> ToolReply {
    let mut roots = vec![root.to_path_buf()];
    roots.extend(root.canonicalize().ok());
    let mut prefixes: Vec<String> = roots.iter().map(|r| format!("{}/", r.display())).collect();
    // The longest first: a root before a shorter spelling inside it.
    prefixes.sort_by_key(|prefix| std::cmp::Reverse(prefix.len()));
    prefixes.dedup();
    let outer = reply.clone();
    ToolReply::new(reply.call_id().to_string(), move |fields: Value| {
        if fields.get("status").is_some() {
            outer.acknowledge();
            return;
        }
        if fields["ok"] == true {
            outer.finish(ToolOutcome::Ok(fields.get("data").cloned().unwrap_or(Value::Null)));
            return;
        }
        let mut message = fields["error"]["message"].as_str().unwrap_or("").to_string();
        for prefix in &prefixes {
            message = message.replace(prefix.as_str(), "");
        }
        outer.finish(ToolOutcome::error(fields["error"]["kind"].as_str().unwrap_or("error"), message));
    })
}

// ------------------------------------------------------------ tests' host

/// A test's [`AreaEnv`]: its own storage and system workspace, and fixed
/// quotas (an app without one: no ceiling, storage granted).
#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct FixedEnv {
    pub storage: Option<Arc<Storage>>,
    pub system: Option<PathBuf>,
    pub quotas: std::collections::HashMap<String, JailQuota>,
    pub accounts: Arc<Mutex<std::collections::HashMap<String, String>>>,
}

#[cfg(test)]
impl AreaEnv for FixedEnv {
    fn storage(&self) -> Option<&Storage> {
        self.storage.as_deref()
    }
    fn system_workspace(&self) -> Option<PathBuf> {
        self.system.clone()
    }
    fn jail_quota(&self, app: &str) -> Result<JailQuota, String> {
        Ok(self.quotas.get(app).copied().unwrap_or(JailQuota { bytes: None, storage: true }))
    }
    fn current_account(&self, app: &str) -> Option<String> {
        match self.accounts.lock().unwrap().get(app) {
            Some(account) => Some(account.clone()),
            None => self.storage.as_ref().filter(|s| !s.spec(app).accounts).map(|_| crate::app_storage::DEVICE.to_string()),
        }
    }
}

#[cfg(test)]
#[path = "areas_tests.rs"]
mod tests;
