// Run as a temporary octosense-app-hub example; see README.md.
// No signing keys, provider configurations, models or native UI are involved.
use octosense_app_hub::{unpack, Catalog, Remote, Store, DEFAULT_ANCHOR};
use octosense_app_policy::{AgentBundle, HostLimits};
use serde_json::json;
use std::{fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};
struct Scratch(PathBuf);
impl Drop for Scratch { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 { return Err("pass candidate HTTPS base, baseline HTTPS base, receipt path".into()); }
    if !args[0].starts_with("https://raw.githubusercontent.com/OctoSense-org/OctoSense-App-Hub/") ||
       !args[1].starts_with("https://raw.githubusercontent.com/OctoSense-org/OctoSense-App-Hub/") {
        return Err("only public immutable official-repository HTTPS bases are accepted".into());
    }
    for base in &args[..2] {
        let revision = base.trim_end_matches('/').rsplit('/').next().unwrap();
        if revision.len() != 40 || !revision.bytes().all(|c| c.is_ascii_hexdigit()) { return Err("base must pin a full Git commit".into()); }
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let scratch = Scratch(std::env::temp_dir().join(format!("hub-release-proof-{}-{nonce}",std::process::id())));
    let mut dir = fs::DirBuilder::new();
    #[cfg(unix)] { use std::os::unix::fs::DirBuilderExt; dir.mode(0o700); }
    dir.create(&scratch.0)?;
    let candidate = Remote::new(&args[0]); let baseline = Remote::new(&args[1]);
    let candidate_text = candidate.catalog()?; let baseline_text = baseline.catalog()?;
    let catalog: Catalog = serde_json::from_str(&candidate_text)?;
    let old: Catalog = serde_json::from_str(&baseline_text)?;
    if catalog.sequence != 10 || old.sequence != 7 { return Err("unexpected reviewed catalog sequence".into()); }
    let root = scratch.0.join("upgrade"); let fresh_root = scratch.0.join("fresh");
    let mut store = Store::new(DEFAULT_ANCHOR,&root,HostLimits::default());
    store.accept_catalog(&baseline_text)?;
    let mut fresh = Store::new(DEFAULT_ANCHOR,&fresh_root,HostLimits::default()); fresh.accept_catalog(&candidate_text)?;
    let today = octosense_app_hub::today(); let mut results=Vec::new();
    for id in ["org.octosense.samples.inbox","org.octosense.samples.githubnotes","org.octosense.samples.googlecalendar"] {
        let before=old.entries.iter().find(|e|e.manifest.id==id && e.manifest.version=="0.1.0").ok_or("missing previous release")?;
        let after=catalog.entries.iter().find(|e|e.manifest.id==id && e.manifest.version=="0.1.1").ok_or("missing candidate release")?;
        // A dedicated old Store for each upgrade keeps the old latest-version selection.
        let mut device=Store::new(DEFAULT_ANCHOR,&root,HostLimits::default());device.accept_catalog(&baseline_text)?;
        let old_bundle=scratch.0.join(format!("{id}-old")); unpack(&baseline.pack(&before.artifact)?,&old_bundle)?;
        let old_policy=device.install_staged(id,&old_bundle,&device.publisher_keys(),&today)?;
        let jail=old_policy.jail_root(&root);fs::create_dir_all(&jail)?;let sentinel=jail.join("release-retention.txt");fs::write(&sentinel,"fictional local state retained")?;
        device.accept_catalog(&candidate_text)?;
        if device.installed_version(id).as_deref()!=Some("0.1.0") { return Err("catalog refresh silently changed installed release".into()); }
        device.may_run(id)?;
        let bundle=scratch.0.join(format!("{id}-new"));unpack(&candidate.pack(&after.artifact)?,&bundle)?;
        let policy=device.install_staged(id,&bundle,&device.publisher_keys(),&today)?;
        if fs::read_to_string(&sentinel)?!="fictional local state retained" { return Err("upgrade changed app data".into()); }
        let prepared=device.prepare_launch(id)?;device.validate_prepared_launch(&prepared)?;
        if prepared.manifest.version!="0.1.1" || prepared.policy!=policy {return Err("prepared release/grants mismatch".into());}
        AgentBundle::load(prepared.bundle(),&prepared.manifest)?;
        let mut reopened=Store::new(DEFAULT_ANCHOR,&root,HostLimits::default());reopened.accept_catalog(&candidate_text)?;
        if reopened.may_run(id)?!=policy {return Err("cold Store changed admitted policy".into());}
        let fresh_policy=fresh.install_staged(id,&bundle,&fresh.publisher_keys(),&today)?;
        if fresh_policy!=policy { return Err("fresh and upgraded grants differ".into()); }
        let fresh_prepared=fresh.prepare_launch(id)?;fresh.validate_prepared_launch(&fresh_prepared)?;
        results.push(json!({"app_id":id,"version":"0.1.1","bundle_digest":after.manifest.integrity.bundle_blake3,
            "fresh_signed_install":true,"upgrade_from_0_1_0":true,"old_release_remains_openable_before_upgrade":true,
            "local_state_retained":true,"prepared_launch_validated":true,"declared_agent_files_loaded":true,
            "cold_store_policy_matches":true,"direct_network_hosts":policy.hosts}));
    }
    let location=scratch.0.clone();drop(scratch);if location.exists(){return Err("temporary proof profile cleanup failed".into());}
    let receipt=json!({"schema":1,"result":"pass","proof":"official-anchor remote catalog/pack verification and real Store fresh install, upgrade and reopen",
        "candidate_base":args[0],"baseline_base":args[1],"catalog_sequence":catalog.sequence,"apps":results,
        "cleanup":"owned temporary profiles removed","not_verified":["default main catalog availability","native rendering or workflows","agent peer/model execution","live provider login or remote writes","Android"]});
    fs::write(&args[2],serde_json::to_vec_pretty(&receipt)?)?;println!("PASS: three fresh installs, upgrades, retained state and prepared/cold launches; owned profiles removed.");Ok(())
}
fn main(){if let Err(e)=run(){eprintln!("release proof: {e}");std::process::exit(1);}}
