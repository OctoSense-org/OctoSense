//! The manifests and the account sources reaching the host's storage (ADR
//! 0004 §11), on scratch homes only.

use super::*;
use crate::app_storage::tests::Scratch;
use crate::app_storage::{AgentWorkspace, Layout, StorageError, Usage};
use serde_json::json;

fn storage(home: &Path) -> Arc<Storage> {
    Storage::with_file_secrets(Layout::new(home).unwrap())
}

fn write_json(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, value.to_string()).unwrap();
}

// ---- native manifests -----------------------------------------------------

/// Every `native-apps.json` block is one the shell accepts, and agrees with
/// the fields the generator also writes out.
#[test]
fn every_native_storage_block_parses_and_agrees_with_the_entry() {
    for app in crate::native_apps::APPS {
        let spec = native_spec(app).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(spec.accounts, app.accounts, "{}", app.id);
        let external: Vec<String> = spec
            .external
            .iter()
            .map(|e| format!("{}:{}", e.place, if e.writable { "rw" } else { "ro" }))
            .collect();
        assert_eq!(external, app.external, "{}", app.id);
    }
}

/// Rinx declares accounts: registered at startup, the handle the shell
/// offers its module keeps one folder per Matrix account.
#[test]
fn rinx_is_offered_per_account_storage_from_its_entry() {
    let home = Scratch::new("rinx-entry");
    let host = storage(&home.0);
    assert!(!host.spec("rinx").accounts, "nothing registered yet: the default");
    register_native_specs(&host);
    let spec = host.spec("rinx");
    assert!(spec.accounts);
    assert_eq!(spec.agent_workspace, AgentWorkspace::Account);
    assert_eq!(host.spec("terminal").agent_workspace, AgentWorkspace::None);
    // The module host's handoff (module_host.rs → `offer`), as Rinx would
    // claim it in `create`.
    crate::ai_host::app_peers::storage::offer("rinx", "i9g9", host.open("rinx").unwrap());
    let rinx = crate::ai_host::app_peers::storage::claim("rinx", "i9g9").unwrap();
    assert!(rinx.has_accounts());
    assert!(matches!(rinx.account_folder(None), Err(StorageError::Accounts(_))), "no device folder for an app with accounts");
    let alice = rinx.account_folder(Some("@alice:example.org")).unwrap();
    assert_eq!(alice, home.0.join("apps/rinx/accounts").join(crate::app_storage::account_hash("@alice:example.org")));
    assert_eq!(rinx.agent_workspace(Some("@alice:example.org")).unwrap(), alice);
}

// ---- accounts through the broker's report --------------------------------

/// Rinx's Matrix login and logout reach storage as `set_account` does:
/// sign-in opens the folder, sign-out suspends and keeps the data, a new
/// sign-in resumes the same folder. Rinx's own data folder (`RINX_DATA_DIR`,
/// `<home>/apps/rinx/data` under an explicit home) is never touched.
#[test]
fn rinx_signs_in_out_and_in_again() {
    let home = Scratch::new("rinx-accounts");
    let host = storage(&home.0);
    register_native_specs(&host);
    let legacy = home.0.join("apps/rinx/data");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("session.db"), "matrix").unwrap();

    let change = account_changed(&host, "rinx", None, Some("@alice:x"));
    let Change::SignedIn { folder, .. } = change else { panic!("{change:?}") };
    assert!(folder.is_dir());
    std::fs::write(folder.join("thread.md"), "export").unwrap();
    let rinx = host.open("rinx").unwrap();
    assert_eq!(rinx.agent_workspace(Some("@alice:x")).unwrap(), folder);

    assert_eq!(account_changed(&host, "rinx", Some("@alice:x"), None), Change::SignedOut { app: "rinx".into(), account: Some("@alice:x".into()) });
    assert_eq!(rinx.agent_workspace(Some("@alice:x")), Err(StorageError::SignedOut));
    assert!(host.is_signed_out("rinx", Some("@alice:x")));
    assert_eq!(std::fs::read_to_string(folder.join("thread.md")).unwrap(), "export", "signing out keeps the data");

    // A restart keeps the suspension.
    let again = storage(&home.0);
    register_native_specs(&again);
    assert!(again.is_signed_out("rinx", Some("@alice:x")));
    account_changed(&again, "rinx", None, Some("@alice:x"));
    assert!(!again.is_signed_out("rinx", Some("@alice:x")), "signing in resumes the same agent");
    assert!(!storage(&home.0).is_signed_out("rinx", Some("@alice:x")), "and is recorded");

    assert_eq!(std::fs::read_to_string(legacy.join("session.db")).unwrap(), "matrix", "Rinx's own data is not moved");
    assert!(again.startup_check().is_clean());
}

#[test]
fn a_switch_signs_the_new_account_in_and_leaves_the_old_one() {
    let home = Scratch::new("switch");
    let host = storage(&home.0);
    register_native_specs(&host);
    account_changed(&host, "rinx", None, Some("a"));
    account_changed(&host, "rinx", Some("a"), Some("b"));
    assert!(!host.is_signed_out("rinx", Some("a")));
    assert!(!host.is_signed_out("rinx", Some("b")));
    assert!(host.open("rinx").unwrap().account_folder(Some("b")).unwrap().is_dir());
}

/// An app without accounts acts for the device: its account binding (a
/// contained app's fixed `device`) never signs anything out.
#[test]
fn an_app_without_accounts_never_signs_out() {
    let home = Scratch::new("device");
    let host = storage(&home.0);
    register_native_specs(&host);
    assert_eq!(account_changed(&host, "card.os.mail", None, Some("device")), Change::None);
    assert_eq!(account_changed(&host, "card.os.mail", Some("device"), None), Change::None);
    assert_eq!(account_changed(&host, "sheets", Some("x"), None), Change::None);
    assert!(!host.is_signed_out("os.mail", None));
    assert_eq!(app_of("card.os.mail"), "os.mail");
    assert_eq!(app_of("rinx"), "rinx");
}

/// A contained app that declared accounts is looked up by its manifest id.
#[test]
fn a_contained_apps_peer_id_names_its_storage() {
    let home = Scratch::new("contained");
    let host = storage(&home.0);
    host.set_spec("os.notes", StorageSpec { accounts: true, ..Default::default() });
    assert!(matches!(account_changed(&host, "card.os.notes", None, Some("me")), Change::SignedIn { .. }));
    account_changed(&host, "card.os.notes", Some("me"), None);
    assert!(host.is_signed_out("os.notes", Some("me")));
}

// ---- Mail's accounts --------------------------------------------------------

#[cfg(any(feature = "app-hub", native_mobile))]
#[test]
fn mails_accounts_open_and_remove_their_folders() {
    use octosense_mail_service::AccountEvent;
    let home = Scratch::new("mail");
    let host = storage(&home.0);
    // App Hub's schema has no `storage.accounts` yet: Mail acts for the
    // device, and its account events leave storage alone.
    let added = AccountEvent::Added { app_id: "os.mail".into(), account: "id1".into() };
    assert_eq!(mail_account(&host, &added), Change::None);
    // Declared, each account gets its folder, and removing it deletes it.
    host.set_spec("os.mail", StorageSpec { accounts: true, ..Default::default() });
    let Change::SignedIn { folder, .. } = mail_account(&host, &added) else { panic!() };
    assert!(folder.is_dir());
    let removed = AccountEvent::Removed { app_id: "os.mail".into(), account: "id1".into() };
    assert!(matches!(mail_account(&host, &removed), Change::SignedOut { .. }));
    assert!(!folder.exists());
    assert!(host.is_signed_out("os.mail", Some("id1")), "its agent stays suspended");
    assert!(memory_notice(&host, "os.mail").unwrap().contains("memory remains"));
    // Adding it again resumes the same agent.
    mail_account(&host, &added);
    assert!(!host.is_signed_out("os.mail", Some("id1")));
    assert_eq!(memory_notice(&host, "os.mail"), None);
}

// ---- script manifests, install and uninstall -----------------------------

#[test]
fn a_script_apps_manifest_block_is_recorded_at_install_and_launch() {
    let home = Scratch::new("script");
    let host = storage(&home.0);
    let root = host.layout().apps_root().to_path_buf();
    write_json(&root.join("org.example.timer/bundle/manifest.json"), &json!({"id": "org.example.timer", "storage": {"max_bytes": 4096}}));
    let spec = prepare_script_app(&host, &root, "org.example.timer").unwrap();
    assert_eq!(spec.max_bytes, Some(4096));
    assert_eq!(host.spec("org.example.timer"), spec);
    for dir in ["accounts", "common", "cache"] {
        assert!(root.join("org.example.timer").join(dir).is_dir(), "{dir}");
    }
    assert!(home.0.join("secrets/org.example.timer").is_dir());
    // A system app, from its newest unpacked build; none yet is the default.
    assert_eq!(prepare_script_app(&host, &root, "os.news").unwrap(), StorageSpec::default());
    write_json(&root.join(".system/os.news/0123/manifest.json"), &json!({"id": "os.news", "storage": {"max_bytes": 77}}));
    assert_eq!(prepare_script_app(&host, &root, "os.news").unwrap().max_bytes, Some(77));
    // A block the host refuses is an error, and the default holds.
    write_json(&root.join("org.example.bad/bundle/manifest.json"), &json!({"storage": {"external": ["home:rw"]}}));
    assert!(prepare_script_app(&host, &root, "org.example.bad").unwrap_err().contains("native apps only"));
    assert_eq!(host.spec("org.example.bad"), StorageSpec::default());
}

#[test]
fn uninstalling_deletes_the_hosts_folders_and_keeps_the_agents_suspended() {
    let home = Scratch::new("uninstall");
    let host = storage(&home.0);
    let root = host.layout().apps_root().to_path_buf();
    write_json(&root.join("org.example.chat/bundle/manifest.json"), &json!({"id": "org.example.chat"}));
    host.set_spec("org.example.chat", StorageSpec { accounts: true, ..Default::default() });
    let chat = host.open("org.example.chat").unwrap();
    chat.account_folder(Some("me")).unwrap();
    chat.secrets().put("token", b"t").unwrap();

    assert!(!app_uninstalled(&host, &root, "org.example.chat"), "the jail is still there: an update, not an uninstall");
    assert!(!app_uninstalled(&host, &root, "os.mail"), "system apps are never uninstalled");
    // App Hub's uninstall removes the jail; the host then removes the rest.
    std::fs::remove_dir_all(root.join("org.example.chat")).unwrap();
    assert!(app_uninstalled(&host, &root, "org.example.chat"));
    assert!(!home.0.join("secrets/org.example.chat").exists());
    assert!(host.is_signed_out("org.example.chat", Some("me")));
    assert!(host.is_signed_out("org.example.chat", Some("someone-else")), "every account of it");
    assert!(storage(&home.0).is_signed_out("org.example.chat", Some("me")), "across a restart");
    assert!(memory_notice(&host, "org.example.chat").is_some());

    // Installed again: each account resumes as it signs in, the rest stay
    // suspended (their folders were gone before the host could list them).
    write_json(&root.join("org.example.chat/bundle/manifest.json"), &json!({"id": "org.example.chat"}));
    prepare_script_app(&host, &root, "org.example.chat").unwrap();
    host.set_spec("org.example.chat", StorageSpec { accounts: true, ..Default::default() });
    assert!(host.is_signed_out("org.example.chat", Some("me")));
    account_changed(&host, "card.org.example.chat", None, Some("me"));
    assert!(!host.is_signed_out("org.example.chat", Some("me")));
    assert!(host.is_signed_out("org.example.chat", Some("someone-else")));
    assert!(!storage(&home.0).is_signed_out("org.example.chat", Some("me")), "recorded");
}

/// An app without accounts acts for the device: installing it again
/// resumes its one agent.
#[test]
fn a_device_app_installed_again_resumes() {
    let home = Scratch::new("reinstall");
    let host = storage(&home.0);
    let root = host.layout().apps_root().to_path_buf();
    write_json(&root.join("org.example.timer/bundle/manifest.json"), &json!({"id": "org.example.timer"}));
    prepare_script_app(&host, &root, "org.example.timer").unwrap();
    std::fs::remove_dir_all(root.join("org.example.timer")).unwrap();
    assert!(app_uninstalled(&host, &root, "org.example.timer"));
    assert!(host.is_signed_out("org.example.timer", None));
    write_json(&root.join("org.example.timer/bundle/manifest.json"), &json!({"id": "org.example.timer"}));
    prepare_script_app(&host, &root, "org.example.timer").unwrap();
    assert!(!host.is_signed_out("org.example.timer", None));
}

#[test]
fn a_corrupt_suspension_record_suspends_nothing() {
    let home = Scratch::new("corrupt");
    std::fs::create_dir_all(home.0.join("secrets/.host")).unwrap();
    std::fs::write(home.0.join("secrets/.host/suspended.json"), "{not json").unwrap();
    assert_eq!(storage(&home.0).suspended_accounts("rinx"), 0);
}

#[cfg(unix)]
#[test]
fn the_suspension_record_is_owner_only_and_outside_every_jail() {
    use std::os::unix::fs::PermissionsExt;
    let home = Scratch::new("record");
    let host = storage(&home.0);
    host.sign_out("rinx", Some("a"));
    let record = home.0.join("secrets/.host/suspended.json");
    assert_eq!(std::fs::metadata(&record).unwrap().permissions().mode() & 0o777, 0o600);
    assert!(!std::fs::read_to_string(&record).unwrap().contains("\"a\""), "account ids are kept hashed");
}

// ---- quotas -----------------------------------------------------------------

#[test]
fn the_shell_measures_and_warns_over_the_declared_ceilings() {
    let home = Scratch::new("quota");
    let host = storage(&home.0);
    host.set_spec("probe", StorageSpec { max_bytes: Some(100), cache_max_bytes: Some(10), ..Default::default() });
    let probe = host.open("probe").unwrap();
    std::fs::write(probe.common().join("a"), vec![0u8; 60]).unwrap();
    std::fs::write(probe.cache().join("c"), vec![0u8; 8]).unwrap();
    assert_eq!(host.usage("probe").unwrap(), Usage { jail_bytes: 68, cache_bytes: 8 });
    assert!(check_quota(&host, "probe").is_empty());
    std::fs::write(probe.cache().join("d"), vec![0u8; 50]).unwrap();
    let warnings = check_quota(&host, "probe");
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings[0].contains("storage.max_bytes of 100"));
    assert!(warnings[1].contains("storage.cache_max_bytes of 10"));
    // A symlink out of the jail is not counted (nor followed).
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/", probe.common().join("root")).unwrap();
        assert_eq!(host.usage("probe").unwrap().jail_bytes, 118);
    }
    // No ceiling, nothing measured.
    host.set_spec("free", StorageSpec::default());
    assert!(check_quota(&host, "free").is_empty());
}

// ---- Settings ---------------------------------------------------------------

#[test]
fn settings_says_a_suspended_agents_memory_remains() {
    use crate::approvals::consent::State;
    use crate::approvals::settings_page::agent_state_text;
    let home = Scratch::new("notice");
    let host = storage(&home.0);
    assert_eq!(memory_notice(&host, "rinx"), None);
    assert_eq!(agent_state_text(&State::Allowed, memory_notice(&host, "rinx")), "Allowed");
    host.sign_out("rinx", Some("a"));
    host.remove_account("rinx", Some("b")).unwrap();
    let text = agent_state_text(&State::Allowed, memory_notice(&host, "rinx"));
    assert_eq!(text, "Allowed \u{00b7} 2 accounts are signed out or removed; its agent's memory remains until octos can erase it");
}
