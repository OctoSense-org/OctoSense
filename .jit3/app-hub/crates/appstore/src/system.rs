//! System apps: script apps that ship inside the build.
//!
//! News, Photos and Maps are part of the phone, not store downloads, but
//! they need not be native code. A system app is an ordinary bundle
//! (`manifest.json` and a `main.splash` program, with its artwork) packed
//! into one file and compiled into the shell. It runs exactly as an installed
//! app does: in its own isolate, under the policy its manifest resolves to,
//! with a jail and a host list. Only admission differs. The pack arrived
//! with the signed build, so no catalog vouches for it and no signature is
//! asked for; its bytes must still hash to what its manifest claims, and its
//! ceilings are [`HostLimits::system`], not the build's own reach.
//!
//! Ids under `os.` are reserved for system apps, so a store download can
//! never take over a system app's jail.
use octosense_app_policy::{AppPolicy, HostLimits, RefuseAllSignatures};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

// A cold launch, image server and agent can all prepare the same pack. Each
// process uses one lock per destination; staging names also differ across
// processes, and only a completely admitted directory becomes visible.
fn preparation_lock(dir: &Path) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<std::collections::BTreeMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();
    let mut locks = LOCKS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(dir).and_then(Weak::upgrade) { return lock; }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(dir.into(), Arc::downgrade(&lock));
    lock
}

/// The id prefix only system apps may use.
pub const SYSTEM_ID_PREFIX: &str = "os.";

/// A system app as the shell registers it: its id, the bundle packed as one
/// JSON file (`octosense_app_hub::pack::pack_system_app` in the shell's
/// build script, then `include_str!`), and artwork too large to pack,
/// compiled in and served from memory at bundle paths (`photos/01.png`).
#[derive(Clone, Copy, Debug)]
pub struct SystemApp {
    pub id: &'static str,
    pub name: &'static str,
    pub pack: &'static str,
    pub assets: octosense_app_policy::StaticAssets,
}

static SYSTEM_APPS: Mutex<Vec<SystemApp>> = Mutex::new(Vec::new());
static AGENT_TOOL_OFFERS: Mutex<std::collections::BTreeMap<String, Vec<String>>> = Mutex::new(std::collections::BTreeMap::new());

/// Replace the host's additional agent-tool offer for one shipped app.
/// Call before admitting/preparing that app. The manifest must still request
/// each tool, and the shell must check sharing, caller grants and execution.
/// This does not offer tools to store apps or grant a script UI capability.
pub fn set_agent_tool_offer(app: &str, tools: &[&str]) {
    assert!(app.starts_with(SYSTEM_ID_PREFIX), "only a system app has a system tool offer");
    AGENT_TOOL_OFFERS.lock().unwrap().insert(app.into(), tools.iter().map(|tool| tool.to_string()).collect());
}

/// Make a system app openable through the `card` module. Registering the
/// same id again replaces it.
pub fn register_system_app(app: SystemApp) {
    assert!(app.id.starts_with(SYSTEM_ID_PREFIX), "a system app's id starts with {SYSTEM_ID_PREFIX}");
    let mut apps = SYSTEM_APPS.lock().unwrap();
    apps.retain(|a| a.id != app.id);
    apps.push(app);
}

pub fn system_app(id: &str) -> Option<SystemApp> {
    SYSTEM_APPS.lock().unwrap().iter().find(|a| a.id == id).copied()
}

pub fn system_apps() -> Vec<SystemApp> {
    SYSTEM_APPS.lock().unwrap().clone()
}

/// Unpack a system app's bundle (once per build of it) outside every jail,
/// and admit it: the unpacked bytes must hash to the manifest's digest, the
/// manifest must be the app it was registered as, and the policy resolves
/// under system ceilings. Returns the bundle directory and the policy.
pub fn prepare(app_data_root: &Path, app: &SystemApp) -> Result<(PathBuf, AppPolicy), String> {
    // The pack's own hash names the directory, so a new build of the app
    // unpacks beside the old one instead of over a bundle something reads.
    let pack_hash = octosense_app_policy::bundle_digest(app.pack.as_bytes());
    // `.system` can never be an app id, so it is no app's jail.
    let dir = app_data_root.join(".system").join(app.id).join(&pack_hash[..16]);
    let lock = preparation_lock(&dir);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut limits = HostLimits::system();
    if let Some(tools) = AGENT_TOOL_OFFERS.lock().unwrap().get(app.id) {
        limits.offered_tools.extend(tools.iter().cloned());
    }
    if !dir.join(octosense_app_policy::MANIFEST_FILE).is_file() {
        let pack: octosense_app_hub::pack::Pack =
            serde_json::from_str(app.pack).map_err(|e| format!("{}: not a pack: {e}", app.id))?;
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let staging = dir.with_extension(format!("staging-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        let staged = octosense_app_hub::pack::unpack(&pack, &staging)
            .and_then(|_| admit_directory(&staging,app,&limits));
        if let Err(error) = staged {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error);
        }
        if let Err(error) = std::fs::rename(&staging, &dir) {
            // Another process may have published this immutable pack first.
            // Never delete its directory, and never accept an incomplete one.
            let _ = std::fs::remove_dir_all(&staging);
            if !dir.join(octosense_app_policy::MANIFEST_FILE).is_file() { return Err(format!("{}: {error}",dir.display())); }
        }
    }
    let policy = admit_directory(&dir,app,&limits)?;
    Ok((dir, policy))
}

fn admit_directory(dir:&Path,app:&SystemApp,limits:&HostLimits)->Result<AppPolicy,String>{
    let manifest = std::fs::read_to_string(dir.join(octosense_app_policy::MANIFEST_FILE))
        .map_err(|e| format!("{}: no manifest: {e}", app.id))?;
    let digest = octosense_app_policy::digest_dir(&dir)?;
    let policy = octosense_app_policy::admit_and_resolve_dir(&manifest, &digest, limits, &RefuseAllSignatures)?;
    if policy.app_id != app.id {
        return Err(format!("the pack registered as {} holds {}", app.id, policy.app_id));
    }
    // A system app ships the shared components its manifest pins in its
    // bundle (App Hub ADR 0003): each must be there and hash to its digest.
    octosense_app_hub::components::bundled(dir)?;
    Ok(policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_cold_preparations_never_publish_partial_bundles() {
        let mut owned=vec![("main.splash".to_string(),"Label{text: \"fixture\"}".to_string())];
        for i in 0..80 { owned.push((format!("asset-{i}.txt"),"fixture".repeat(256))); }
        let files:Vec<_>=owned.iter().map(|(name,body)|(name.as_str(),body.as_str())).collect();
        let pack=Box::leak(pack_of("concurrent",&files).into_boxed_str());
        let app=SystemApp{id:"os.demo",name:"Demo",pack,assets:&[]};
        let root=std::env::temp_dir().join(format!("appstore-concurrent-{}",std::process::id()));
        let _=std::fs::remove_dir_all(&root);
        let barrier=Arc::new(std::sync::Barrier::new(12));
        let workers:Vec<_>=(0..12).map(|_|{
            let root=root.clone();let barrier=barrier.clone();
            std::thread::spawn(move||{barrier.wait();for _ in 0..3 {
                let (dir,_)=prepare(&root,&app).unwrap();
                assert_eq!(std::fs::read_dir(&dir).unwrap().count(),82);
            }})
        }).collect();
        for worker in workers {worker.join().unwrap();}
        let _=std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_system_agent_tool_request_needs_its_own_hosts_explicit_offer() {
        let root = std::env::temp_dir().join(format!("appstore-system-tool-offer-{}", std::process::id()));
        let source = root.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("main.splash"), "Label{text: \"fixture\"}").unwrap();
        let digest = octosense_app_policy::digest_dir(&source).unwrap();
        let manifest = serde_json::json!({"schema":1,"id":"os.tool-offer-fixture","name":"Fixture","version":"1",
            "integrity":{"bundle_blake3":digest},"capabilities":["storage"],
            "agent":{"profile":"read-only","tools":["ask_user_question","calendar.events"]}});
        std::fs::write(source.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
        let pack = Box::leak(serde_json::to_string(&octosense_app_hub::pack::pack_dir(&source).unwrap()).unwrap().into_boxed_str());
        let app = SystemApp { id:"os.tool-offer-fixture", name:"Fixture", pack, assets:&[] };
        let refused = prepare(&root, &app).unwrap_err();
        assert!(refused.contains("calendar.events"), "{refused}");
        set_agent_tool_offer("os.another-tool-offer-fixture", &["calendar.events"]);
        assert!(prepare(&root, &app).is_err(), "another app's offer is not inherited");
        set_agent_tool_offer(app.id, &["calendar.events"]);
        let (dir, policy) = prepare(&root, &app).unwrap();
        assert!(!policy.allows("calendar"), "agent tools do not grant raw service access");
        set_agent_tool_offer(app.id, &[]);
        assert!(prepare(&root, &app).is_err(), "cached unpacked bytes are readmitted after withdrawal");
        set_agent_tool_offer(app.id, &["calendar.events"]);
        std::fs::write(dir.join("main.splash"), "tampered").unwrap();
        assert!(prepare(&root, &app).is_err(), "an offer never bypasses integrity");
        let _ = std::fs::remove_dir_all(root);
    }

    fn pack_of(name: &str, files: &[(&str, &str)]) -> String {
        let dir = std::env::temp_dir().join(format!("appstore-system-src-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, body) in files {
            std::fs::write(dir.join(name), body).unwrap();
        }
        let digest = octosense_app_policy::digest_dir(&dir).unwrap();
        let manifest = format!(
            r#"{{"schema":1,"id":"os.demo","version":"1","name":"Demo","integrity":{{"bundle_blake3":"{digest}"}},
                "capabilities":["storage","net","images"],"network":{{"hosts":["example.org"]}}}}"#
        );
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        serde_json::to_string(&octosense_app_hub::pack::pack_dir(&dir).unwrap()).unwrap()
    }

    #[test]
    fn a_system_app_unpacks_outside_every_jail_and_runs_under_its_manifest() {
        let pack: &'static str = Box::leak(pack_of("runs", &[("main.splash", "Label{text: \"hi\"}")]).into_boxed_str());
        let root = std::env::temp_dir().join(format!("appstore-system-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let app = SystemApp { id: "os.demo", name: "Demo", pack, assets: &[] };
        let (dir, policy) = prepare(&root, &app).unwrap();
        assert!(dir.starts_with(root.join(".system")));
        assert!(!dir.starts_with(policy.jail_root(&root)), "the app cannot rewrite its own program");
        assert!(policy.allows("images") && policy.allows_host("example.org"));
        assert!(policy.instruction_budget > HostLimits::default().max_instruction_budget);
        // A second open reuses the unpacked bundle, and still checks it.
        std::fs::write(dir.join("main.splash"), "tampered").unwrap();
        assert!(prepare(&root, &app).is_err(), "a changed byte is refused");
    }

    #[test]
    fn a_pack_must_hold_the_app_it_was_registered_as() {
        let pack: &'static str = Box::leak(pack_of("mismatch", &[("main.splash", "x")]).into_boxed_str());
        let root = std::env::temp_dir().join(format!("appstore-system-root-mismatch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let err = prepare(&root, &SystemApp { id: "os.other", name: "Other", pack, assets: &[] }).unwrap_err();
        assert!(err.contains("holds os.demo"), "{err}");
    }
}
