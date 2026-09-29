//! A broker tells the host's account lifecycle (ADR 0004 §11) about every
//! account change, before it prepares the new account's peer. Its own test
//! binary: the observer is process-wide.
#![cfg(feature = "broker")]

use std::sync::{Arc, Mutex};

use octosense_app_peers::broker::{BoxFuture, Broker, BrokerConfig, Connector, Link};
use octosense_app_peers::storage::observe_accounts;
use octosense_app_peers::*;

/// A kernel that is never reached: it records when the broker tries.
struct Recording(Arc<Mutex<Vec<String>>>);

impl Connector for Recording {
    fn available(&self) -> Result<(), String> {
        Ok(())
    }
    fn connect(&self) -> BoxFuture<'static, Result<Box<dyn Link>, String>> {
        self.0.lock().unwrap().push("connect".into());
        Box::pin(async { Err("no kernel in this test".to_owned()) })
    }
    fn owns_runtime(&self) -> bool {
        false
    }
    fn shutdown(&self) {}
}

#[test]
fn every_account_change_reaches_the_host_before_the_peer_is_prepared() {
    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen = log.clone();
    observe_accounts(Some(Arc::new(move |app: &str, from: Option<&str>, to: Option<&str>| {
        seen.lock().unwrap().push(format!("{app}: {from:?} -> {to:?}"));
    })));
    let cfg = BrokerConfig::new(
        Deployment::Hosted,
        "_main",
        "_main:api:octosense#system",
        "rinx",
        "Rinx",
        OCTOS_SERVICES.iter().map(|s| s.to_string()).collect(),
    );
    let broker = Broker::new(cfg, Arc::new(Recording(log.clone())));
    broker.set_account(Some("@alice:x"));
    // The peer is prepared on the broker's runtime; wait for its attempt.
    for _ in 0..200 {
        if log.lock().unwrap().iter().any(|e| e == "connect") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    broker.set_account(Some("@alice:x"));
    broker.set_account(None);
    observe_accounts(None);
    let log = log.lock().unwrap().clone();
    assert_eq!(log.first().map(String::as_str), Some("rinx: None -> Some(\"@alice:x\")"), "signed in before the peer: {log:?}");
    assert!(log.iter().any(|e| e == "connect"), "{log:?}");
    let changes: Vec<&String> = log.iter().filter(|e| e.starts_with("rinx:")).collect();
    assert_eq!(changes, ["rinx: None -> Some(\"@alice:x\")", "rinx: Some(\"@alice:x\") -> None"], "no event without a change");
}
