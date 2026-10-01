//! Offline verification of a packaged sibling, without a program override.
//! Run beside `octos-kernel`; uses a new temporary profile, no provider calls.
use std::{collections::BTreeMap, path::Path, time::{Duration, SystemTime, UNIX_EPOCH}};
use octosense_kernel::{CloseReason, Connection, Core, Options};
use octosense_llm_config::{profile, Provider, ProviderSet};
use serde_json::{json, Value};

async fn call(conn: &mut Connection, id: &str, method: &str, params: Value) -> Value {
    conn.send(json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params}).to_string()).unwrap();
    loop {
        let text = tokio::time::timeout(Duration::from_secs(30), conn.recv()).await.unwrap().unwrap();
        let reply: Value = serde_json::from_str(&text).unwrap();
        if reply["id"] == id {
            assert!(reply.get("error").is_none(), "{reply}");
            return reply["result"].clone();
        }
    }
}

fn provider(root: &Path, family: &str, model: &str) {
    let primary = Provider::new(family, Some(model.into()));
    let env = BTreeMap::from([(primary.key_env.clone(), "test-fixture-not-a-real-key".into())]);
    profile::save_merge(&profile::profile_path(root), &ProviderSet {primary: Some(primary), fallbacks: vec![]}, &env).unwrap();
}

#[tokio::main]
async fn main() {
    assert!(std::env::var_os("OCTOS_APP_CORE_BIN").is_none(), "probe requires automatic discovery");
    let root = std::env::temp_dir().join(format!("octos-packaged-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir(&root).unwrap();
    let core = Core::new(Options::default().core_dir(&root));
    struct Cleanup(Core, std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            self.0.shutdown_within(Duration::from_secs(5));
            let _ = std::fs::remove_dir_all(&self.1);
        }
    }
    let _cleanup = Cleanup(core.clone(), root.clone());
    provider(&root, "deepseek", "deepseek-v4-flash");
    let mut first = core.connect().expect("packaged runtime must be found without an override");
    let mut second = core.connect().unwrap();
    assert_eq!(first.generation(), second.generation());
    let list = call(&mut first, "providers", "profile/llm/list", json!({"profile_id":"_main"})).await;
    assert_eq!(list["primary"]["model"], "deepseek-v4-flash");
    let list = call(&mut second, "providers", "profile/llm/list", json!({"profile_id":"_main"})).await;
    assert_eq!(list["primary"]["model"], "deepseek-v4-flash");
    provider(&root, "moonshot", "kimi-k2.5");
    assert!(core.restart());
    assert_eq!(first.recv().await, Err(CloseReason::Restarted));
    assert_eq!(second.recv().await, Err(CloseReason::Restarted));
    let mut next = core.connect().unwrap();
    assert_eq!(next.generation(), 2);
    let list = call(&mut next, "providers", "profile/llm/list", json!({"profile_id":"_main"})).await;
    assert_eq!(list["primary"]["model"], "kimi-k2.5");
    println!("PASS: sibling runtime, shared kernel, provider profile and restart; no LLM request");
}
