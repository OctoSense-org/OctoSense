//! A real `octos` kernel started by the core from a temp core dir whose
//! profile `octosense_llm_config` wrote, as the AI providers app's `llm`
//! service does. No provider is called: keys are fixtures and the kernel is
//! only asked what it runs on (`profile/llm/list`).
//!
//! Runs when `OCTOS_CORE_TEST_KERNEL` names an `octos` binary built from the
//! octos rev AppCard pins:
//!
//! ```sh
//! cargo build --release -p octos-cli --bin octos --no-default-features --features api,git,ast
//! ```
//!
//! Without it the test says so and passes (CI has no kernel binary).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use octosense_llm_config::{profile, Provider, ProviderSet};
use octosense_octos_core::{CloseReason, Connection, Core, Options};
use serde_json::{json, Value};

async fn call(conn: &mut Connection, id: &str, method: &str, params: Value) -> Value {
    conn.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()).unwrap();
    loop {
        let text = tokio::time::timeout(Duration::from_secs(60), conn.recv())
            .await
            .expect("kernel timed out")
            .expect("kernel frame");
        let frame: Value = serde_json::from_str(&text).unwrap();
        if frame.get("id").and_then(Value::as_str) == Some(id) {
            assert!(frame.get("error").is_none(), "{method} failed: {frame}");
            return frame["result"].clone();
        }
    }
}

/// Save `family/model` as the primary provider, with a fixture key.
fn write_provider(core_dir: &Path, family: &str, model: &str) {
    let primary = Provider::new(family, Some(model.into()));
    let env = BTreeMap::from([(primary.key_env.clone(), format!("sk-octos-core-test-fixture-{family}"))]);
    let set = ProviderSet { primary: Some(primary), fallbacks: vec![] };
    profile::save_merge(&profile::profile_path(core_dir), &set, &env).unwrap();
}

/// What the kernel says it runs on: (family, model).
async fn running_on(conn: &mut Connection, id: &str) -> (String, String) {
    let list = call(conn, id, "profile/llm/list", json!({"profile_id": "_main"})).await;
    let primary = &list["primary"];
    assert_eq!(list["runtime_policy_stamp"]["model"], primary["model"], "{list}");
    (primary["family_id"].as_str().unwrap_or_default().into(), primary["model"].as_str().unwrap_or_default().into())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_real_kernel_runs_on_the_profile_the_providers_app_wrote() {
    let Some(program) = std::env::var_os("OCTOS_CORE_TEST_KERNEL").map(PathBuf::from) else {
        eprintln!("OCTOS_CORE_TEST_KERNEL is not set: skipping the real-kernel test");
        return;
    };
    let dir = std::env::temp_dir().join(format!("octos-core-real-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let core_dir = dir.join("octos-home/.octos");
    write_provider(&core_dir, "deepseek", "deepseek-v4-flash");

    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = lines.clone();
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program).log(move |l| {
        sink.lock().unwrap().push(l.to_owned());
    }));
    let dump = lines.clone();
    let _print_log = Defer(move || eprintln!("kernel log:\n{}", dump.lock().unwrap().join("\n")));

    // A consumer opens a session on the kernel the core started.
    let mut conn = core.connect().expect("a kernel");
    let open = call(&mut conn, "open", "session/open", json!({"session_id": "_main:octos-core-it", "profile_id": "_main"})).await;
    assert_eq!(open["opened"]["session_id"], "_main:octos-core-it");
    assert_eq!(open["opened"]["active_profile_id"], "_main");
    assert_eq!(running_on(&mut conn, "llm1").await, ("deepseek".into(), "deepseek-v4-flash".into()));
    // A second consumer shares that kernel.
    let mut other = core.connect().unwrap();
    assert_eq!(other.generation(), conn.generation());
    assert_eq!(running_on(&mut other, "llm1").await.1, "deepseek-v4-flash");

    // The providers app saves another provider and restarts the kernel.
    write_provider(&core_dir, "moonshot", "kimi-k2.5");
    assert!(core.restart());
    assert_eq!(conn.recv().await, Err(CloseReason::Restarted));
    assert_eq!(other.recv().await, Err(CloseReason::Restarted));
    let mut conn = core.connect().unwrap();
    assert_eq!(conn.generation(), 2);
    call(&mut conn, "open", "session/open", json!({"session_id": "_main:octos-core-it", "profile_id": "_main"})).await;
    assert_eq!(running_on(&mut conn, "llm2").await, ("moonshot".into(), "kimi-k2.5".into()));
    let log = lines.lock().unwrap().join("\n");
    assert!(log.contains("Model: deepseek-v4-flash") && log.contains("Model: kimi-k2.5"), "{log}");

    drop((conn, other));
    assert!(!core.status().running);
    let _ = std::fs::remove_dir_all(&dir);
}

struct Defer<F: FnMut()>(F);
impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)()
    }
}
