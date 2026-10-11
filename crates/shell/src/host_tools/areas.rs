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
//! account and caller kind. It resolves the agent's area ([`agent_area`])
//! before dispatch, registers it as a [`Grant`] held until the call is
//! answered, and puts its root in the `ServiceCall`'s `host_dir`, the one
//! field of a call only host code sets. The resolver ([`resolve_in`])
//! answers a granted root with its area, and App Hub's own `host_dir` (the
//! shared `<apps root>/.host`, which the Card runner hands every request of
//! an app's isolate) with the app's storage ([`app_area`]); it refuses any
//! other. A service run without the resolver (tests, App Hub's card-host)
//! keeps its legacy private folder `<host dir>/<family>`.
//!
//! A signed-out, suspended (an uninstalled app's) or refused account is
//! refused, and says why. What each service then lets a call do there
//! (contained paths, no replacing for agents, the quota, the paths written
//! inside documents) is the service's, through `octosense_engine_area`.

use std::path::{Path, PathBuf};
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

/// The agents' areas of the calls in flight, by root: what the resolver
/// answers for a `host_dir` the executor set. Each holds a count of the
/// calls it serves.
static GRANTS: Mutex<Vec<(PathBuf, Area, usize)>> = Mutex::new(Vec::new());

/// An agent's area, granted for one call until it is dropped (when the
/// call is answered or cancelled).
pub struct Grant {
    root: PathBuf,
}

/// Grant `area` to a call in flight: [`resolve_in`] answers its root with it.
pub fn grant(area: Area) -> Grant {
    let root = area.root.clone();
    let mut grants = GRANTS.lock().unwrap_or_else(|e| e.into_inner());
    match grants.iter_mut().find(|(r, _, _)| *r == root) {
        Some(held) => {
            held.1 = area;
            held.2 += 1;
        }
        None => grants.push((root.clone(), area, 1)),
    }
    Grant { root }
}

impl Drop for Grant {
    fn drop(&mut self) {
        let mut grants = GRANTS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = grants.iter().position(|(r, _, _)| *r == self.root) {
            grants[i].2 -= 1;
            if grants[i].2 == 0 {
                grants.swap_remove(i);
            }
        }
    }
}

fn granted(root: &Path) -> Option<Area> {
    GRANTS.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(r, _, _)| r == root).map(|(_, area, _)| area.clone())
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

/// The area of an agent's `call` to a tool of `owner` (the app the executor
/// runs it as, set at registration): for a craft engine's tool, the
/// caller's own folder, the system agent's workspace or the calling app
/// agent's account folder; for an app's own tool, that app's agent folder,
/// for its own agent's account or else the account the app acts for now.
/// No write there replaces a file; an app's folder keeps to what is left of
/// its storage. `Err((kind, why))` when there is none now.
pub fn agent_area(env: &dyn AreaEnv, call: &HostToolCall, owner: &str) -> Result<Area, (&'static str, String)> {
    let own = call.caller_kind == CallerKind::AppPeer && super::app_of_peer(&call.calling_app) == owner;
    if !belongs_to_no_app(owner) && !own {
        let account = env.current_account(owner);
        return agent_folder(env, &peer_of_app(owner), account.as_deref());
    }
    match call.caller_kind {
        CallerKind::System => {
            let root = env.system_workspace().ok_or(("no_workspace", "the system agent's workspace is not known yet: it is the folder its conversation opens in, so open the Assistant once".to_string()))?;
            Ok(Area::new(root, None, false))
        }
        CallerKind::AppPeer => agent_folder(env, &call.calling_app, call.account.as_deref()),
    }
}

/// The folder of `peer`'s agent for `account` (`None`: the device's, for an
/// app without accounts), refused while that account is signed out or its
/// folder refused, or when the agent has no files.
fn agent_folder(env: &dyn AreaEnv, peer: &str, account: Option<&str>) -> Result<Area, (&'static str, String)> {
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
    let root = super::agent_workspace_in(storage, peer, account).ok_or(("no_workspace", format!("{app}'s agent has no folder of its own")))?;
    let quota_left = quota_left(env, storage, app).map_err(|why| ("no_workspace", why))?;
    Ok(Area::new(root, quota_left, false))
}

/// What is left of `app`'s storage: its ceiling less what its jail holds
/// now (`None`: no ceiling).
fn quota_left(env: &dyn AreaEnv, storage: &Storage, app: &str) -> Result<Option<u64>, String> {
    let Some(ceiling) = env.jail_quota(app)?.bytes else { return Ok(None) };
    let used = storage.usage(app).map_err(|e| format!("{app}: {e}"))?.jail_bytes;
    Ok(Some(ceiling.saturating_sub(used)))
}

// ------------------------------------------------------------ apps

/// The area of `app`'s own request: its storage, the jail its `fs.*` sees
/// (App Hub roots an isolate's storage at the jail, not at an account's
/// folder), so the app finds an engine's output where it looks. Only an
/// app with storage has one, and only while its account is signed in and
/// its jail was not refused; a foreground call may replace a file, a
/// background one (a glance tile's) may not; within what is left of its
/// storage.
pub fn app_area(env: &dyn AreaEnv, app: &str, may_prompt: bool) -> Result<Area, String> {
    let storage = env.storage().ok_or("this host keeps no app storage")?;
    crate::apps::check_script_app_id(app)?;
    let quota = env.jail_quota(app)?;
    if !quota.storage {
        return Err(format!("{app} has no storage of its own, so an engine has no folder to work in for it"));
    }
    let current = env.current_account(app);
    let account = if storage.spec(app).accounts {
        Some(current.ok_or_else(|| format!("{app} has no signed-in account"))?)
    } else {
        None
    };
    if storage.is_signed_out(app, account.as_deref()) {
        return Err(format!("{app}'s account is signed out"));
    }
    if let Some(why) = storage.refused(app, account.as_deref()) {
        return Err(format!("{app}'s folder was refused: {why}"));
    }
    let jail = storage.layout().app(app)?.jail;
    crate::app_storage::ensure_private_dir(storage.layout().apps_root(), &jail).map_err(|e| format!("{app}: {e}"))?;
    let quota_left = match quota.bytes {
        Some(ceiling) => Some(ceiling.saturating_sub(storage.usage(app).map_err(|e| format!("{app}: {e}"))?.jail_bytes)),
        None => None,
    };
    Ok(Area::new(jail, quota_left, may_prompt))
}

// ------------------------------------------------------------ the resolver

/// Where `call` works, on `env`: the area granted to the agent call whose
/// root is its `host_dir`, or, for App Hub's shared `host_dir`, the calling
/// app's own storage ([`app_area`]); refused for any other.
pub fn resolve_in(env: &dyn AreaEnv, call: &ServiceCall) -> Result<Area, String> {
    if let Some(area) = granted(&call.host_dir) {
        return Ok(area);
    }
    let shared = env.storage().map(|storage| storage.layout().apps_root().join(".host"));
    if shared.is_some_and(|shared| shared == call.host_dir) {
        return app_area(env, &call.app_id, call.may_prompt);
    }
    Err("this call names no folder the host gave it".into())
}

/// The resolver the shell installs on every engine service.
pub fn resolver() -> octosense_engine_area::Resolver {
    Arc::new(|call: &ServiceCall| resolve_in(&ShellEnv, call))
}

/// At startup, beside the engines' registration: every engine works in its
/// caller's area from now on.
pub fn install_resolvers() {
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
#[derive(Default)]
pub(crate) struct FixedEnv {
    pub storage: Option<Arc<Storage>>,
    pub system: Option<PathBuf>,
    pub quotas: std::collections::HashMap<String, JailQuota>,
    pub accounts: std::collections::HashMap<String, String>,
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
        match self.accounts.get(app) {
            Some(account) => Some(account.clone()),
            None => self.storage.as_ref().filter(|s| !s.spec(app).accounts).map(|_| crate::app_storage::DEVICE.to_string()),
        }
    }
}

#[cfg(test)]
#[path = "areas_tests.rs"]
mod tests;
