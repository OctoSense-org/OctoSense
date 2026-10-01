//! Every rule of ADR 0004 §11 the host enforces: paths, the account hash,
//! permissions, the manifest block, secrets, sign-out and the startup check.

use super::*;
use serde_json::json;

/// A fresh, canonical scratch directory, removed on drop.
pub(crate) struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new(tag: &str) -> Self {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("octosense-storage-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(std::fs::canonicalize(&dir).unwrap())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn storage(home: &Path) -> Arc<Storage> {
    Storage::with_file_secrets(Layout::new(home).unwrap())
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

// ---- paths ----------------------------------------------------------------

#[test]
fn the_layout_computes_every_path_from_the_home() {
    let layout = Layout::new(Path::new("/h")).unwrap();
    assert_eq!(layout.apps_root(), Path::new("/h/apps"));
    assert_eq!(layout.secrets_root(), Path::new("/h/secrets"));
    let app = layout.app("os.mail").unwrap();
    assert_eq!(app.jail, Path::new("/h/apps/os.mail"));
    assert_eq!(app.accounts, Path::new("/h/apps/os.mail/accounts"));
    assert_eq!(app.common, Path::new("/h/apps/os.mail/common"));
    assert_eq!(app.cache, Path::new("/h/apps/os.mail/cache"));
    assert_eq!(app.secrets, Path::new("/h/secrets/os.mail"));
    assert_eq!(app.account(None), Path::new("/h/apps/os.mail/accounts/device"));
    assert_eq!(
        app.account(Some("a@example.org")),
        Path::new("/h/apps/os.mail/accounts").join(account_hash("a@example.org"))
    );
}

#[test]
fn the_secrets_root_never_overlaps_the_apps_root() {
    assert!(Layout::with_roots("/h/apps".into(), "/h/apps/secrets".into()).is_err());
    assert!(Layout::with_roots("/h/secrets/apps".into(), "/h/secrets".into()).is_err());
    assert!(Layout::with_roots("/h/apps".into(), "/h/apps/../apps/x".into()).is_err(), "lexically inside");
    assert!(Layout::with_roots("apps".into(), "/h/secrets".into()).is_err(), "relative roots are refused");
    assert!(Layout::with_roots("/h/apps".into(), "/h/secrets".into()).is_ok());
    // Through a symlink: the secrets root resolves inside the apps root.
    #[cfg(unix)]
    {
        let dir = Scratch::new("overlap");
        std::fs::create_dir_all(dir.0.join("apps/x")).unwrap();
        std::os::unix::fs::symlink(dir.0.join("apps/x"), dir.0.join("secrets")).unwrap();
        assert!(Layout::new(&dir.0).is_err());
    }
}

#[test]
fn app_ids_are_single_plain_path_components() {
    for ok in ["os.mail", "rinx", "sheets", "org.example.timer", "a-b_c.1"] {
        assert!(validate_app_id(ok).is_ok(), "{ok}");
    }
    let long = "a".repeat(129);
    for bad in ["", ".host", ".system", "..", "a/b", "a\\b", "a..b", "-x", "catalog.json", "a b", long.as_str()] {
        assert!(validate_app_id(bad).is_err(), "{bad:?}");
        assert!(Layout::new(Path::new("/h")).unwrap().app(bad).is_err());
    }
}

// ---- the account hash -----------------------------------------------------

#[test]
fn the_account_hash_is_stable_normalized_and_opaque() {
    let h = account_hash("alice@example.org");
    // COMPATIBILITY CONTRACT: this value names real folders on people's
    // devices. If it changes (salt, normalization or truncation), every
    // app's per-account data and agent workspace is orphaned. Do not update
    // the pin without a migration that renames existing account folders.
    assert_eq!(h, PINNED_ALICE);
    assert_eq!(h.len(), ACCOUNT_HASH_LEN);
    assert!(h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    assert_eq!(account_hash("  Alice@Example.ORG\n"), h, "trimmed and lowercased");
    assert_ne!(account_hash("bob@example.org"), h);
    assert_ne!(account_hash("@alice:example.org"), h);
    assert!(!h.contains("alice"));
    assert_ne!(account_hash(""), DEVICE);
    assert_eq!(normalize_account(" X@Y "), "x@y");
}

/// One account key: the folder name and the agent's memory tag agree on
/// which ids are one account.
#[test]
fn should_key_the_folder_and_the_memory_tag_the_same_way_when_ids_differ_in_case() {
    use crate::ai_host::app_peers::broker::account_tag;
    for (a, b) in [("Alice@Example.org", "alice@example.org"), (" @bob:x ", "@bob:x"), ("alice@example.org", "bob@example.org")] {
        assert_eq!(account_hash(a) == account_hash(b), account_tag(a) == account_tag(b), "{a:?} / {b:?}");
    }
    assert_eq!(normalize_account(" X@Y "), crate::ai_host::app_peers::storage::normalize_account(" X@Y "));
}

/// `SHA-256("octosense.account.v1\0alice@example.org")`, first 16 bytes.
const PINNED_ALICE: &str = "d0c3ec9a8159479a7cf0933b0a539aa2";

// ---- directories and permissions -----------------------------------------

#[test]
fn opening_an_app_lays_out_owner_only_folders() {
    let home = Scratch::new("open");
    let host = storage(&home.0);
    let app = host.open("os.mail").unwrap();
    assert_eq!(app.jail(), home.0.join("apps/os.mail"));
    for dir in [app.jail().to_path_buf(), app.jail().join("accounts"), app.common().to_path_buf(), app.cache().to_path_buf(), home.0.join("secrets/os.mail")] {
        assert!(dir.is_dir(), "{dir:?}");
        #[cfg(unix)]
        assert_eq!(mode(&dir), 0o700, "{dir:?}");
    }
    assert!(!app.jail().join("secrets").exists(), "secrets are never in the jail");
}

#[cfg(unix)]
#[test]
fn open_tightens_loose_folders_and_refuses_symlinked_ones() {
    use std::os::unix::fs::PermissionsExt;
    let home = Scratch::new("perms");
    std::fs::create_dir_all(home.0.join("apps/loose/common")).unwrap();
    std::fs::set_permissions(home.0.join("apps/loose/common"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let host = storage(&home.0);
    host.open("loose").unwrap();
    assert_eq!(mode(&home.0.join("apps/loose/common")), 0o700);

    // A jail that is a symlink (here into the secrets) is never used.
    std::fs::create_dir_all(home.0.join("secrets/victim")).unwrap();
    std::os::unix::fs::symlink(home.0.join("secrets/victim"), home.0.join("apps/evil")).unwrap();
    assert!(matches!(host.open("evil"), Err(StorageError::Io(_))));
    assert!(!home.0.join("secrets/victim/accounts").exists());
    // ensure_private_dir refuses paths outside its root too.
    assert!(ensure_private_dir(&home.0.join("apps"), &home.0.join("secrets/x")).is_err());
}

#[test]
fn account_folders_follow_the_declared_accounts() {
    let home = Scratch::new("accounts");
    let host = storage(&home.0);
    let device = host.open("notes").unwrap();
    assert!(!device.has_accounts());
    let dir = device.account_folder(None).unwrap();
    assert_eq!(dir, home.0.join("apps/notes/accounts/device"));
    assert!(dir.is_dir());
    #[cfg(unix)]
    assert_eq!(mode(&dir), 0o700);
    assert!(matches!(device.account_folder(Some("a@b")), Err(StorageError::Accounts(_))));

    host.set_spec("rinx", StorageSpec { accounts: true, ..Default::default() });
    let rinx = host.open("rinx").unwrap();
    assert!(rinx.has_accounts());
    let a = rinx.account_folder(Some("@alice:example.org")).unwrap();
    assert_eq!(a, home.0.join("apps/rinx/accounts").join(account_hash("@alice:example.org")));
    assert_ne!(a, rinx.account_folder(Some("@bob:example.org")).unwrap());
    assert!(matches!(rinx.account_folder(None), Err(StorageError::Accounts(_))));
    assert!(matches!(rinx.account_folder(Some("  ")), Err(StorageError::Accounts(_))));
    assert_eq!(rinx.agent_workspace(Some("@alice:example.org")).unwrap(), a);
}

// ---- secrets --------------------------------------------------------------

#[test]
fn secrets_are_owner_only_files_outside_the_jail() {
    let home = Scratch::new("secrets");
    let host = storage(&home.0);
    let app = host.open("rinx").unwrap();
    let secrets = app.secrets();
    assert_eq!(secrets.get("matrix.token").unwrap(), None);
    secrets.put("matrix.token", b"s3cret").unwrap();
    secrets.put("matrix.token", b"s3cret2").unwrap();
    assert_eq!(secrets.get("matrix.token").unwrap().as_deref(), Some(&b"s3cret2"[..]));
    let file = home.0.join("secrets/rinx/matrix.token");
    assert!(file.is_file());
    #[cfg(unix)]
    assert_eq!(mode(&file), 0o600);
    assert!(!file.starts_with(home.0.join("apps")));
    let leftovers: Vec<_> = std::fs::read_dir(home.0.join("secrets/rinx")).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(leftovers.len(), 1, "no temporaries left: {leftovers:?}");
    secrets.remove("matrix.token").unwrap();
    secrets.remove("matrix.token").unwrap();
    assert_eq!(secrets.get("matrix.token").unwrap(), None);
    for bad in ["", ".index", "../x", "a/b", "a b", "k~"] {
        assert!(matches!(secrets.put(bad, b"x"), Err(StorageError::InvalidKey(_))), "{bad:?}");
    }
}

// ---- sign-out, removal, uninstall ----------------------------------------

#[test]
fn signing_out_suspends_the_workspace_and_keeps_the_data() {
    let home = Scratch::new("signout");
    let host = storage(&home.0);
    host.set_spec("rinx", StorageSpec { accounts: true, ..Default::default() });
    let rinx = host.open("rinx").unwrap();
    let dir = rinx.agent_workspace(Some("alice")).unwrap();
    std::fs::write(dir.join("notes.md"), "kept").unwrap();
    host.sign_out("rinx", Some("Alice"));
    assert!(host.is_signed_out("rinx", Some("alice")), "the same account, normalized");
    assert_eq!(rinx.agent_workspace(Some("alice")), Err(StorageError::SignedOut));
    assert_eq!(rinx.account_folder(Some("alice")).unwrap(), dir, "the app itself still reaches its data");
    assert!(rinx.agent_workspace(Some("bob")).is_ok(), "other accounts are untouched");
    assert_eq!(std::fs::read_to_string(dir.join("notes.md")).unwrap(), "kept");
    host.sign_in("rinx", Some("alice"));
    assert_eq!(rinx.agent_workspace(Some("alice")).unwrap(), dir, "signing in resumes the same workspace");
}

#[test]
fn an_agent_without_a_workspace_gets_none() {
    let home = Scratch::new("noworkspace");
    let host = storage(&home.0);
    host.set_spec("calc", StorageSpec { agent_workspace: AgentWorkspace::None, ..Default::default() });
    let calc = host.open("calc").unwrap();
    assert_eq!(calc.agent_workspace(None), Err(StorageError::NoWorkspace));
    assert!(calc.account_folder(None).is_ok());
}

#[test]
fn removing_an_account_or_the_app_deletes_its_folders() {
    let home = Scratch::new("remove");
    let host = storage(&home.0);
    host.set_spec("mail", StorageSpec { accounts: true, ..Default::default() });
    let mail = host.open("mail").unwrap();
    let a = mail.account_folder(Some("a@x")).unwrap();
    let b = mail.account_folder(Some("b@x")).unwrap();
    mail.secrets().put("a", b"pw").unwrap();
    host.remove_account("mail", Some("a@x")).unwrap();
    assert!(!a.exists() && b.exists());
    assert_eq!(mail.agent_workspace(Some("a@x")), Err(StorageError::SignedOut));
    host.uninstall("mail").unwrap();
    assert!(!home.0.join("apps/mail").exists());
    assert!(!home.0.join("secrets/mail").exists());
    assert!(host.is_signed_out("mail", Some("b@x")));
}

/// A run that cannot reach the keychain (headless, tests) cannot delete
/// the app's keychain items: it keeps the index that names them, so a
/// later run with the keychain can, and deletes everything else.
#[test]
fn should_keep_the_keychain_index_when_an_uninstall_cannot_purge_it() {
    let home = Scratch::new("keep-index");
    let host = storage(&home.0);
    let mail = host.open("mail").unwrap();
    mail.secrets().put("a", b"pw").unwrap();
    let index = home.0.join("secrets/mail").join(secrets::KEYCHAIN_INDEX);
    std::fs::write(&index, "matrix.token\n").unwrap();
    host.uninstall("mail").unwrap();
    assert!(!home.0.join("apps/mail").exists());
    assert!(index.is_file(), "the index of items this run could not delete stays");
    assert!(!home.0.join("secrets/mail/a").exists(), "every other secret goes");
}

// ---- the manifest block ---------------------------------------------------

#[test]
fn the_storage_block_defaults_every_field() {
    for block in [None, Some(json!(null)), Some(json!({}))] {
        let spec = StorageSpec::parse(block.as_ref(), AppKind::Script).unwrap();
        assert_eq!(spec, StorageSpec::default());
        assert!(!spec.accounts);
        assert_eq!(spec.agent_workspace, AgentWorkspace::Account);
        assert!(spec.external.is_empty());
    }
    // App Hub's existing block alone.
    let spec = StorageSpec::parse(Some(&json!({"max_bytes": 4096})), AppKind::Script).unwrap();
    assert_eq!(spec.max_bytes, Some(4096));
}

#[test]
fn the_adr_storage_block_parses() {
    let block = json!({
        "max_bytes": 536870912u64, "accounts": true, "agent_workspace": "account",
        "cache_max_bytes": 1073741824u64, "external": []
    });
    let spec = StorageSpec::parse(Some(&block), AppKind::Script).unwrap();
    assert_eq!(spec.max_bytes, Some(536870912));
    assert_eq!(spec.cache_max_bytes, Some(1073741824));
    assert!(spec.accounts);
    let terminal = json!({"storage": {"accounts": false, "agent_workspace": "none", "external": ["home:rw", "documents/Invoices:ro"]}});
    let spec = StorageSpec::from_manifest(&terminal, AppKind::Native).unwrap();
    assert_eq!(spec.agent_workspace, AgentWorkspace::None);
    assert_eq!(spec.external, vec![
        spec::External { place: "home".into(), writable: true },
        spec::External { place: "documents/Invoices".into(), writable: false },
    ]);
}

#[test]
fn invalid_storage_blocks_are_refused() {
    let bad = [
        (json!([]), AppKind::Native),
        (json!({"quota": 1}), AppKind::Native),
        (json!({"max_bytes": 0}), AppKind::Native),
        (json!({"max_bytes": -1}), AppKind::Native),
        (json!({"max_bytes": "1G"}), AppKind::Native),
        (json!({"cache_max_bytes": 1.5}), AppKind::Native),
        (json!({"accounts": "yes"}), AppKind::Native),
        (json!({"agent_workspace": "home"}), AppKind::Native),
        (json!({"agent_workspace": true}), AppKind::Native),
        (json!({"external": "home:rw"}), AppKind::Native),
        (json!({"external": ["home"]}), AppKind::Native),
        (json!({"external": ["home:rwx"]}), AppKind::Native),
        (json!({"external": ["root:rw"]}), AppKind::Native),
        (json!({"external": ["/etc:ro"]}), AppKind::Native),
        (json!({"external": ["home/../..:ro"]}), AppKind::Native),
        (json!({"external": ["home//x:ro"]}), AppKind::Native),
        (json!({"external": [1]}), AppKind::Native),
        // Script apps never reach outside their jail.
        (json!({"external": ["home:ro"]}), AppKind::Script),
    ];
    for (block, kind) in bad {
        assert!(StorageSpec::parse(Some(&block), kind).is_err(), "{block} ({kind:?})");
    }
}

/// Every system app's bundle manifest (what App Hub's Card runner reads)
/// carries a storage block this host accepts, with the new fields defaulted.
#[test]
fn every_system_app_manifest_storage_block_is_valid() {
    let apps = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps");
    let mut seen = 0;
    for shell in ["desktop", "phone"] {
        let list = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../{shell}/system-apps.json"))).unwrap();
        let list: serde_json::Value = serde_json::from_str(&list).unwrap();
        for id in list["apps"].as_array().unwrap() {
            let path = apps.join(id.as_str().unwrap()).join("bundle/manifest.json");
            let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let spec = StorageSpec::from_manifest(&manifest, AppKind::Script);
            assert!(spec.is_ok(), "{}: {:?}", path.display(), spec);
            assert!(validate_app_id(manifest["id"].as_str().unwrap()).is_ok());
            seen += 1;
        }
    }
    assert!(seen >= 5);
}

// ---- the startup check ----------------------------------------------------

/// A home with two apps, one account each, and one secret.
fn populated(tag: &str) -> (Scratch, Arc<Storage>, PathBuf, PathBuf) {
    let home = Scratch::new(tag);
    let host = storage(&home.0);
    host.set_spec("rinx", StorageSpec { accounts: true, ..Default::default() });
    let rinx = host.open("rinx").unwrap();
    let ws = rinx.account_folder(Some("alice")).unwrap();
    std::fs::create_dir_all(ws.join("rooms/general")).unwrap();
    std::fs::write(ws.join("rooms/general/export.md"), "hello").unwrap();
    rinx.secrets().put("token", b"s3cret").unwrap();
    host.open("notes").unwrap().account_folder(None).unwrap();
    // App Hub's own entries beside the jails are not apps.
    std::fs::create_dir_all(home.0.join("apps/.host/model")).unwrap();
    std::fs::write(home.0.join("apps/catalog.json"), "{}").unwrap();
    let secret = home.0.join("secrets/rinx/token");
    (home, host, ws, secret)
}

#[test]
fn a_clean_home_passes_the_startup_check() {
    let (_home, host, ws, _) = populated("clean");
    #[cfg(unix)]
    {
        // Links that stay away from the secrets are fine.
        std::os::unix::fs::symlink(ws.join("rooms/general/export.md"), ws.join("latest.md")).unwrap();
        std::os::unix::fs::symlink("rooms", ws.join("r")).unwrap();
    }
    let report = host.startup_check();
    assert!(report.is_clean(), "{:?}", report.violations);
    assert_eq!(report.workspaces, 2);
    assert!(!report.incomplete);
    assert!(host.open("rinx").unwrap().agent_workspace(Some("alice")).is_ok());
}

#[cfg(unix)]
#[test]
fn a_symlink_escape_into_the_secrets_is_refused() {
    let (home, host, ws, secret) = populated("escape");
    std::os::unix::fs::symlink(&secret, ws.join("rooms/general/token")).unwrap();
    let report = host.startup_check();
    assert_eq!(report.violations.len(), 1, "{:?}", report.violations);
    let v = &report.violations[0];
    assert_eq!((v.app_id.as_str(), v.account.as_str()), ("rinx", account_hash("alice").as_str()));
    assert_eq!(v.path, ws.join("rooms/general/token"));
    let rinx = host.open("rinx").unwrap();
    assert!(matches!(rinx.agent_workspace(Some("alice")), Err(StorageError::Refused(_))));
    assert!(host.open("notes").unwrap().agent_workspace(None).is_ok(), "other apps are untouched");
    assert!(secret.is_file() && ws.join("rooms/general/token").exists(), "nothing is deleted");
    // Fixed: the next start lets the workspace back.
    std::fs::remove_file(ws.join("rooms/general/token")).unwrap();
    assert!(host.startup_check().is_clean());
    assert!(rinx.agent_workspace(Some("alice")).is_ok());
    drop(home);
}

#[cfg(unix)]
#[test]
fn relative_dangling_and_ancestor_links_are_refused_too() {
    for (tag, target) in [
        // Relative: from accounts/<hash>/ up to the home, then into secrets.
        ("relative", PathBuf::from("../../../../secrets/rinx")),
        // Not there yet: a secret written later would appear in the workspace.
        ("dangling", PathBuf::from("../../../../secrets/rinx/later")),
        // A folder containing the secrets (the home, or /).
        ("ancestor", PathBuf::from("../../../..")),
        ("root", PathBuf::from("/")),
    ] {
        let (_home, host, ws, _) = populated(tag);
        std::os::unix::fs::symlink(&target, ws.join("link")).unwrap();
        let report = host.startup_check();
        assert_eq!(report.violations.len(), 1, "{tag}: {:?}", report.violations);
        assert_eq!(report.violations[0].path, ws.join("link"), "{tag}");
    }
}

#[cfg(unix)]
#[test]
fn a_symlinked_workspace_or_jail_is_refused() {
    let (home, host, _, _) = populated("wslink");
    // The workspace itself a symlink, into the secrets.
    let fake = home.0.join("apps/rinx/accounts").join(account_hash("mallory"));
    std::os::unix::fs::symlink(home.0.join("secrets/rinx"), &fake).unwrap();
    // A symlinked workspace pointing anywhere else is refused as well.
    let other = home.0.join("apps/rinx/accounts").join(account_hash("eve"));
    std::fs::create_dir_all(home.0.join("elsewhere")).unwrap();
    std::os::unix::fs::symlink(home.0.join("elsewhere"), &other).unwrap();
    let report = host.startup_check();
    let flagged: Vec<_> = report.violations.iter().map(|v| v.account.clone()).collect();
    assert!(flagged.contains(&account_hash("mallory")) && flagged.contains(&account_hash("eve")), "{flagged:?}");
    assert!(!flagged.contains(&account_hash("alice")));

    // A whole jail linked into the secrets refuses every account of that app.
    std::fs::create_dir_all(home.0.join("secrets/bad/accounts/device")).unwrap();
    std::os::unix::fs::symlink(home.0.join("secrets/bad"), home.0.join("apps/bad")).unwrap();
    host.startup_check();
    assert!(host.refused("bad", None).is_some());
    assert!(host.refused("bad", Some("anyone")).is_some());
}

#[cfg(unix)]
#[test]
fn a_hard_link_to_a_secret_is_refused() {
    let (_home, host, ws, secret) = populated("hardlink");
    std::fs::hard_link(&secret, ws.join("copy")).unwrap();
    let report = host.startup_check();
    assert_eq!(report.violations.len(), 1, "{:?}", report.violations);
    assert_eq!(report.violations[0].reason, "a hard link to a secret");
}

#[test]
fn an_empty_home_is_clean() {
    let home = Scratch::new("empty");
    let report = storage(&home.0).startup_check();
    assert!(report.is_clean());
    assert_eq!(report.workspaces, 0);
}

// ---- the handoff ----------------------------------------------------------

#[test]
fn without_host_storage_no_module_is_offered_any() {
    // Unit tests never run `init`: nothing is written under a real home.
    assert!(host().is_none());
    assert!(!offer("sheets", &["storage"], "i1g1"));
}

#[test]
fn a_module_claims_the_storage_offered_to_it() {
    let home = Scratch::new("handoff");
    let host = storage(&home.0);
    let app = host.open("sheets").unwrap();
    crate::ai_host::app_peers::storage::offer("sheets", "i7g7", app);
    let claimed = crate::ai_host::app_peers::storage::claim("sheets", "i7g7").unwrap();
    assert_eq!(claimed.app_id(), "sheets");
    assert_eq!(claimed.jail(), home.0.join("apps/sheets"));
    withdraw("sheets", "i7g7");
}

// ---- the secrets backend --------------------------------------------------

/// The keychain can prompt and hang an unattended run: tests and headless
/// runs always get the file store.
#[test]
fn the_keychain_is_never_used_in_tests_or_headless_runs() {
    use secrets::{select_backend, Backend};
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
    };
    // This test binary itself.
    assert_eq!(secrets::backend(), Backend::File, "cfg(test) never reaches the login keychain");
    assert_eq!(select_backend(true, true, env(&[("OCTOSENSE_SECRETS", "keychain")])), Backend::File);
    // No vault on the platform.
    assert_eq!(select_backend(false, false, env(&[("OCTOSENSE_SECRETS", "keychain")])), Backend::File);
    // A person's desktop session.
    assert_eq!(select_backend(true, false, env(&[])), Backend::Keychain);
    assert_eq!(select_backend(true, false, env(&[("CI", "")])), Backend::Keychain, "an empty variable is unset");
    assert_eq!(select_backend(true, false, env(&[("OCTOSENSE_SECRETS", "file")])), Backend::File);
    for headless in secrets::HEADLESS_VARS {
        let pairs: &'static [(&'static str, &'static str)] = Box::leak(vec![(*headless, "1")].into_boxed_slice());
        assert_eq!(select_backend(true, false, env(pairs)), Backend::File, "{headless}");
    }
    assert_eq!(select_backend(true, false, env(&[("CI", "true"), ("OCTOSENSE_SECRETS", "keychain")])), Backend::Keychain, "explicit opt-in");
    // The platform store in this binary writes owner-only files.
    let home = Scratch::new("backend");
    let store = secrets::platform(&home.0.join("secrets"), "probe");
    store.put("k", b"v").unwrap();
    assert!(home.0.join("secrets/probe/k").is_file());
}

/// A host built with `Storage::new` (the platform store, as `init` does)
/// keeps secrets in files under the scratch home in tests.
#[test]
fn a_platform_host_in_tests_keeps_secrets_in_files() {
    let home = Scratch::new("platformhost");
    let host = Storage::new(Layout::new(&home.0).unwrap());
    host.open("probe").unwrap().secrets().put("k", b"v").unwrap();
    assert!(home.0.join("secrets/probe/k").is_file());
    host.uninstall("probe").unwrap();
    assert!(!home.0.join("secrets/probe").exists());
}

/// The host's answer to the broker: a refused workspace is refused for the
/// account the relay keys it by (an app with accounts: that account; a
/// script app's `card.<id>` peer: the device), for prepare, resume, input
/// and calls alike.
#[cfg(unix)]
#[test]
fn should_answer_refused_for_the_peers_account_when_the_startup_check_flags_it() {
    let (home, host, ws, secret) = populated("host-refused");
    let notes = host.open("notes").unwrap().account_folder(None).unwrap();
    std::os::unix::fs::symlink(&secret, ws.join("token")).unwrap();
    std::os::unix::fs::symlink(&secret, notes.join("token")).unwrap();
    host.startup_check();
    let why = crate::host_tools::workspace_refused_in(&host, "rinx", "alice");
    assert!(why.is_some_and(|w| w.contains("secrets")), "rinx keeps accounts: alice's folder is refused");
    assert!(crate::host_tools::workspace_refused_in(&host, "rinx", "bob").is_none(), "another account is not");
    assert!(crate::host_tools::workspace_refused_in(&host, "card.notes", "anyone").is_some(), "a script app's peer uses the device folder");
    drop(home);
}
