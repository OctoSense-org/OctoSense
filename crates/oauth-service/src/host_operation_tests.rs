//! Blocked provider work must not hold the global metadata lock or another
//! app's operation lock. These closures make no provider or vault requests.
use super::*;

#[test]
fn a_slow_provider_operation_does_not_block_other_apps_or_metadata() {
    use std::sync::mpsc;
    let root = std::env::temp_dir().join(format!("oauth-operation-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(root.join("oauth")).unwrap();
    std::fs::write(root.join("oauth/clients.json"), "{}").unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let first_root = root.clone();
    let first = std::thread::spawn(move || {
        with_provider_api(&first_root, "app.slow", |_| {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(())
        })
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let same = operation_lock(&root, "app.slow");
    assert!(
        same.try_lock().is_err(),
        "same-app changes must wait for the in-flight operation"
    );
    let other_root = root.clone();
    let (other_tx, other_rx) = mpsc::channel();
    let other = std::thread::spawn(move || {
        let result = with_provider_api(&other_root, "app.fast", |_| Ok(()));
        other_tx.send(result).unwrap();
    });
    let independent = other_rx.recv_timeout(Duration::from_secs(2));
    // Release even after a failed assertion, so the slow worker cannot leak.
    release_tx.send(()).unwrap();
    assert!(independent.unwrap().is_ok());
    first.join().unwrap().unwrap();
    other.join().unwrap();
    assert!(same.try_lock().is_ok());
    std::fs::remove_dir_all(root).unwrap();
}
