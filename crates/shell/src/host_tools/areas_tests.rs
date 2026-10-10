//! Where an engine works, decided from trusted host data alone: an agent's
//! area from the call's stamped identity, an app's own from App Hub's
//! shared host directory, and nothing for any other.

use super::*;
use crate::ai_host::app_peers::host_tools::CallOrigin;
use crate::app_storage::tests::Scratch;
use crate::app_storage::{Layout, StorageSpec};
use serde_json::json;
use std::sync::Arc;

fn storage(home: &Path) -> Arc<Storage> {
    Storage::with_file_secrets(Layout::new(home).unwrap())
}

/// A tool call as its host stamps it.
fn tool_call(calling: &str, kind: CallerKind, account: Option<&str>) -> HostToolCall {
    let mut call = HostToolCall::parse(&json!({"peer": "p1", "session_id": "s", "turn_id": "t", "call_id": "c", "name": "word.info", "args": {"path": "a.docx"}})).unwrap();
    call.calling_app = calling.into();
    call.caller_kind = kind;
    call.account = account.map(str::to_string);
    call.origin = if kind == CallerKind::System { CallOrigin::System } else { CallOrigin::Context };
    call
}

/// An app's request as App Hub's Card runner hands it to a service.
fn app_request(app: &str, host_dir: &Path, may_prompt: bool) -> ServiceCall {
    ServiceCall { app_id: app.into(), service: "word.info".into(), args: json!({}), from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
}

/// A script app with an agent (its manifest's storage block), installed.
fn script_app(storage: &Storage, app: &str, accounts: bool) {
    storage.set_spec(app, StorageSpec { accounts, ..StorageSpec::default() });
}

#[test]
fn the_engines_methods_work_in_an_area_and_nothing_else_does() {
    for method in ["sheet.open", "sheet.eval"] {
        assert!(needs_area(method), "{method}");
    }
    #[cfg(feature = "craft-engines")]
    for method in ["photo.convert", "photos.info", "word.info", "deck.new", "cad.render", "light.develop", "sound.mix", "design.export", "film.export", "effect.render", "vector.convert", "pdf.split"] {
        assert!(needs_area(method), "{method}");
    }
    // Without the desktop's engines (Home), their methods work in no area:
    // the executor refuses them as unavailable first.
    #[cfg(not(feature = "craft-engines"))]
    for method in ["photo.convert", "photos.info", "word.info"] {
        assert!(!needs_area(method), "{method}");
    }
    for method in ["photos.notify", "mail.list", "calendar.events", "news.read", "glance.publish", "oauth.connect", "sheets.get", "wordy.info"] {
        assert!(!needs_area(method), "{method}");
    }
}

/// The system agent works in its workspace on a craft engine's tool, never
/// replacing a file, with no quota; until its workspace is known it has
/// none.
#[cfg(feature = "craft-engines")]
#[test]
fn the_system_agent_works_in_its_workspace() {
    let home = Scratch::new("areas-system");
    let env = FixedEnv { system: Some(home.0.join("ws")), ..FixedEnv::default() };
    let area = agent_area(&env, &tool_call("system", CallerKind::System, None), "os.word").unwrap();
    assert_eq!(area, Area::new(home.0.join("ws"), None, false));
    let unknown = agent_area(&FixedEnv::default(), &tool_call("system", CallerKind::System, None), "os.word").unwrap_err();
    assert_eq!(unknown.0, "no_workspace", "{unknown:?}");
}

/// An app's agent works in its account's folder, the same folder its own
/// conversation's workspace is, within what is left of the app's storage,
/// on its app's own engine tool and (with `craft-engines`) on a craft
/// engine's alike; a signed-out account, a refused folder and an agent
/// with no folder are refused, each saying why.
#[test]
fn an_apps_agent_works_in_its_accounts_folder() {
    let home = Scratch::new("areas-agent");
    let host = storage(&home.0);
    script_app(&host, "org.example.notes", true);
    let mut env = FixedEnv { storage: Some(host.clone()), ..FixedEnv::default() };
    env.quotas.insert("org.example.notes".into(), JailQuota { bytes: Some(1000), storage: true });
    let call = tool_call("card.org.example.notes", CallerKind::AppPeer, Some("ana@example.org"));
    let own = "org.example.notes";
    let area = agent_area(&env, &call, own).unwrap();
    let folder = host.layout().app("org.example.notes").unwrap().account(Some("ana@example.org"));
    assert_eq!(area, Area::new(&folder, Some(1000), false));
    assert_eq!(Some(folder.clone()), crate::host_tools::agent_workspace_in(&host, "card.org.example.notes", "ana@example.org"), "the agent's own workspace");
    #[cfg(feature = "craft-engines")]
    assert_eq!(agent_area(&env, &call, "os.word").unwrap(), area, "a craft engine works in the caller's folder");
    // What the jail holds already counts against the quota.
    std::fs::write(folder.join("big.bin"), vec![0u8; 400]).unwrap();
    assert_eq!(agent_area(&env, &call, own).unwrap().quota_left, Some(600));
    // Another account of the same app is another folder.
    let other = agent_area(&env, &tool_call("card.org.example.notes", CallerKind::AppPeer, Some("bo@example.org")), own).unwrap();
    assert_ne!(other.root, folder);
    // Signed out: refused.
    host.sign_out("org.example.notes", Some("ana@example.org"));
    assert_eq!(agent_area(&env, &call, own).unwrap_err().0, "signed_out");
    host.sign_in("org.example.notes", Some("ana@example.org"));
    assert!(agent_area(&env, &call, own).is_ok());
    // No account at all, for an app that keeps accounts: refused.
    assert_eq!(agent_area(&env, &tool_call("card.org.example.notes", CallerKind::AppPeer, None), own).unwrap_err().0, "signed_out");
    // An agent with no files of its own has no area.
    host.set_spec("org.example.tools-only", StorageSpec { agent_workspace: crate::app_storage::AgentWorkspace::None, ..StorageSpec::default() });
    let tools_only = tool_call("card.org.example.tools-only", CallerKind::AppPeer, Some("device"));
    assert_eq!(agent_area(&env, &tools_only, "org.example.tools-only").unwrap_err().0, "no_workspace");
    // No storage on this host: no area.
    assert!(agent_area(&FixedEnv::default(), &call, own).is_err());
}

/// An app's own engine tool works on that app's data whoever was granted
/// it: the system agent's `sheets.get`, or another app's agent's, works in
/// Sheets' agent folder (where its own agent opened the workbook), never in
/// the caller's; for an app that keeps accounts, the account it acts for
/// now, refused while none is signed in.
#[test]
fn an_apps_own_tool_works_in_its_folder_whoever_calls_it() {
    let home = Scratch::new("areas-owner");
    let host = storage(&home.0);
    script_app(&host, "org.example.notes", false);
    let env = FixedEnv { storage: Some(host.clone()), system: Some(home.0.join("ws")), ..FixedEnv::default() };
    let sheets = host.layout().app("sheets").unwrap().account(None);
    let own = agent_area(&env, &tool_call("sheets", CallerKind::AppPeer, Some("device")), "sheets").unwrap();
    assert_eq!(own, Area::new(&sheets, None, false));
    for caller in [tool_call("system", CallerKind::System, None), tool_call("card.org.example.notes", CallerKind::AppPeer, Some("device"))] {
        assert_eq!(agent_area(&env, &caller, "sheets").unwrap(), own, "{}", caller.calling_app);
    }
    // A script app's own tool (Photos' `photos.info`), called by another.
    script_app(&host, "os.photos", false);
    let photos = agent_area(&env, &tool_call("system", CallerKind::System, None), "os.photos").unwrap();
    assert_eq!(photos.root, host.layout().app("os.photos").unwrap().account(None));
    // An owner that keeps accounts acts for its signed-in one.
    script_app(&host, "os.mailish", true);
    assert_eq!(agent_area(&env, &tool_call("system", CallerKind::System, None), "os.mailish").unwrap_err().0, "signed_out");
    let mut signed_in = env;
    signed_in.accounts.insert("os.mailish".into(), "ana@example.org".into());
    let area = agent_area(&signed_in, &tool_call("system", CallerKind::System, None), "os.mailish").unwrap();
    assert_eq!(area.root, host.layout().app("os.mailish").unwrap().account(Some("ana@example.org")));
}

/// An app's own request works in its storage: its jail, the folder its
/// `fs.*` sees; a foreground request may replace a file, a background one
/// may not; only an app with storage has one, while its account is signed
/// in.
#[test]
fn an_apps_own_request_works_in_its_storage() {
    let home = Scratch::new("areas-app");
    let host = storage(&home.0);
    script_app(&host, "os.jotter", false);
    let mut env = FixedEnv { storage: Some(host.clone()), ..FixedEnv::default() };
    env.quotas.insert("os.jotter".into(), JailQuota { bytes: Some(64 << 20), storage: true });
    let jail = host.layout().app("os.jotter").unwrap().jail;
    let shared = host.layout().apps_root().join(".host");
    let foreground = resolve_in(&env, &app_request("os.jotter", &shared, true)).unwrap();
    assert_eq!(foreground, Area::new(&jail, Some(64 << 20), true));
    assert!(jail.is_dir(), "made for it, as its isolate's own storage is");
    let background = resolve_in(&env, &app_request("os.jotter", &shared, false)).unwrap();
    assert!(!background.may_replace, "a glance tile never replaces a file");
    // No `storage` capability: no folder of its own.
    env.quotas.insert("os.nostore".into(), JailQuota { bytes: Some(1), storage: false });
    assert!(resolve_in(&env, &app_request("os.nostore", &shared, true)).unwrap_err().contains("no storage"));
    // An app that keeps accounts: only with one signed in, and not signed out.
    script_app(&host, "os.mailish", true);
    assert!(resolve_in(&env, &app_request("os.mailish", &shared, true)).unwrap_err().contains("no signed-in account"));
    env.accounts.insert("os.mailish".into(), "ana@example.org".into());
    assert!(resolve_in(&env, &app_request("os.mailish", &shared, true)).is_ok());
    host.sign_out("os.mailish", Some("ana@example.org"));
    assert!(resolve_in(&env, &app_request("os.mailish", &shared, true)).unwrap_err().contains("signed out"));
    // A native app never sends App Hub requests: not a script app's id.
    assert!(resolve_in(&env, &app_request("terminal", &shared, true)).is_err());
}

/// One app's request works in its own jail, so another app's folder is
/// out of its reach: the area of each is its own, and no `host_dir` the
/// host did not give names one.
#[test]
fn an_apps_area_is_its_own_and_no_other_folder_is_reachable() {
    let home = Scratch::new("areas-isolation");
    let host = storage(&home.0);
    script_app(&host, "os.one", false);
    script_app(&host, "os.two", false);
    let env = FixedEnv { storage: Some(host.clone()), ..FixedEnv::default() };
    let shared = host.layout().apps_root().join(".host");
    let one = resolve_in(&env, &app_request("os.one", &shared, true)).unwrap();
    let two = resolve_in(&env, &app_request("os.two", &shared, true)).unwrap();
    assert!(!one.root.starts_with(&two.root) && !two.root.starts_with(&one.root));
    // A call that names another app's folder as its host directory (no
    // host code does) is refused, not answered with that folder.
    for host_dir in [two.root.clone(), host.layout().apps_root().to_path_buf(), home.0.clone()] {
        assert!(resolve_in(&env, &app_request("os.one", &host_dir, true)).is_err(), "{}", host_dir.display());
    }
}

/// An agent's area is answered only while it is granted to a call in
/// flight; the grant is counted, so a second call in the same area keeps
/// it after the first ends.
#[test]
fn a_granted_area_is_answered_only_while_its_call_runs() {
    let home = Scratch::new("areas-grant");
    let root = home.0.join("ws");
    std::fs::create_dir_all(&root).unwrap();
    let env = FixedEnv::default();
    let call = app_request("os.word", &root, false);
    assert!(resolve_in(&env, &call).is_err(), "not granted yet");
    let first = grant(Area::new(&root, None, false));
    let second = grant(Area::new(&root, None, false));
    assert_eq!(resolve_in(&env, &call).unwrap(), Area::new(&root, None, false));
    drop(first);
    assert!(resolve_in(&env, &call).is_ok(), "the second call still runs there");
    drop(second);
    assert!(resolve_in(&env, &call).is_err(), "no call runs there any more");
}

/// An error names the caller's files relative to its area, whichever
/// spelling of the root the engine was handed; answers and
/// acknowledgements pass through untouched.
#[test]
fn errors_name_files_relative_to_the_area() {
    let home = Scratch::new("areas-errors");
    let root = home.0.join("ws");
    std::fs::create_dir_all(&root).unwrap();
    for spelled in [root.clone(), root.canonicalize().unwrap()] {
        let sent: Arc<std::sync::Mutex<Vec<Value>>> = Default::default();
        let s = sent.clone();
        let reply = relative_errors(ToolReply::new("e", move |v| s.lock().unwrap().push(v)), &root);
        let message = format!("effect.info: cannot read {}: No such file", spelled.join("clips/none.ecproj").display());
        reply.finish(ToolOutcome::error("app_error", message));
        let got = sent.lock().unwrap()[0].clone();
        assert_eq!(got["error"]["kind"], "app_error");
        assert_eq!(got["error"]["message"], "effect.info: cannot read clips/none.ecproj: No such file", "{got}");
    }
    let sent: Arc<std::sync::Mutex<Vec<Value>>> = Default::default();
    let s = sent.clone();
    let reply = relative_errors(ToolReply::new("ok", move |v| s.lock().unwrap().push(v)), &root);
    let data = json!({"path": root.join("kept.png").display().to_string()});
    reply.finish(ToolOutcome::Ok(data.clone()));
    assert_eq!(sent.lock().unwrap()[0]["data"], data, "an answer is the engine's own");
}
