use action_receipts::*;
use serde_json::json;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

struct Backend {
    revision: AtomicU64,
    calls: AtomicUsize,
    evidence: Mutex<Option<Evidence>>,
    mode: u8,
}
impl Backend {
    fn new(mode: u8) -> Self {
        Self {
            revision: AtomicU64::new(0),
            calls: AtomicUsize::new(0),
            evidence: Mutex::new(None),
            mode,
        }
    }
}
impl Adapter for Backend {
    fn validate(&self, _: &Action, revision: u64) -> Result<()> {
        if self.revision.load(Ordering::SeqCst) != revision {
            Err(Error::Conflict)
        } else {
            Ok(())
        }
    }
    fn dispatch(&self, op: &Operation) -> Dispatch {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.mode == 2 {
            return Dispatch::Rejected("permission refused".into());
        }
        let mut e = Evidence {
            operation_id: op.id.clone(),
            external_id: format!("fixture:{}", op.id),
            account: op.account.clone(),
            target: op.action.target.clone(),
            digest: op.digest.clone(),
        };
        if self.mode == 3 {
            e.target = "another recipient".into();
        }
        *self.evidence.lock().unwrap() = Some(e.clone());
        self.revision.fetch_add(1, Ordering::SeqCst);
        if self.mode == 1 {
            Dispatch::Uncertain("lost acknowledgement".into())
        } else {
            Dispatch::Verified(e)
        }
    }
    fn lookup(&self, _: &Operation) -> Result<Option<Evidence>> {
        Ok(self.evidence.lock().unwrap().clone())
    }
}
fn path() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("buwei-journal-{}", new_id()));
    std::fs::create_dir_all(&p).unwrap();
    p.join("operations.db")
}
fn setup() -> (Authority, Grant, Journal) {
    let a = Authority::default();
    a.set_account(Some("alice"));
    let g = a.grant("buwei", &["invite"], 100, 1000).unwrap();
    let j = Journal::open(path()).unwrap();
    (a, g, j)
}
fn action() -> Action {
    Action {
        permission: "invite".into(),
        target: "lin".into(),
        summary: "reserve one local fixture seat".into(),
        payload: json!({"seats":1}),
    }
}
fn queued(j: &mut Journal, g: &Grant, b: &Backend) -> Operation {
    let o = j.prepare(g, action(), 0, 100, 120).unwrap();
    j.confirm(g, &o.id, b, 101).unwrap()
}

#[test]
fn normal_and_repeated_execute_have_one_effect() {
    let (_, g, mut j) = setup();
    let b = Backend::new(0);
    let o = queued(&mut j, &g, &b);
    assert_eq!(
        j.execute(&g, &o.id, &b, 102).unwrap().status,
        Status::Confirmed
    );
    assert_eq!(
        j.execute(&g, &o.id, &b, 103).unwrap().status,
        Status::Confirmed
    );
    assert_eq!(b.calls.load(Ordering::SeqCst), 1);
}
#[test]
fn invalidating_previews_preserves_attempts_and_other_accounts_or_apps() {
    let authority = Authority::default();
    authority.set_account(Some("alice"));
    let grant = authority.grant("buwei", &["invite", "read"], 100, 1000).unwrap();
    let mut journal = Journal::open(path()).unwrap();
    let confirmed_backend = Backend::new(0);
    let confirmed = queued(&mut journal, &grant, &confirmed_backend);
    journal.execute(&grant, &confirmed.id, &confirmed_backend, 102).unwrap();
    let uncertain_backend = Backend::new(1);
    let uncertain = queued(&mut journal, &grant, &uncertain_backend);
    journal.execute(&grant, &uncertain.id, &uncertain_backend, 102).unwrap();
    let preview = journal.prepare(&grant, action(), 0, 103, 120).unwrap();

    let other_authority = Authority::default();
    other_authority.set_account(Some("bob"));
    let other_account = other_authority.grant("buwei", &["invite", "read"], 100, 1000).unwrap();
    let bob_preview = journal.prepare(&other_account, action(), 0, 103, 120).unwrap();
    let other_app = authority.grant("another-app", &["invite", "read"], 100, 1000).unwrap();
    let app_preview = journal.prepare(&other_app, action(), 0, 103, 120).unwrap();

    // Only cancellation crosses a consent boundary: an old action is never sent.
    let renewed = authority.grant("buwei", &["read"], 104, 1000).unwrap();
    assert_eq!(journal.invalidate_previews(&renewed, 105).unwrap(), 1);
    assert_eq!(journal.receipt(&grant, &preview.id, 105).unwrap().status, Status::Cancelled);
    assert!(journal.confirm(&grant, &preview.id, &Backend::new(0), 105).is_err());
    assert_eq!(journal.receipt(&grant, &confirmed.id, 105).unwrap().status, Status::Confirmed);
    assert_eq!(journal.receipt(&grant, &uncertain.id, 105).unwrap().status, Status::Unknown);
    assert_eq!(journal.receipt(&other_account, &bob_preview.id, 105).unwrap().status, Status::Prepared);
    assert_eq!(journal.receipt(&other_app, &app_preview.id, 105).unwrap().status, Status::Prepared);
    assert_eq!(confirmed_backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(uncertain_backend.calls.load(Ordering::SeqCst), 1);
    renewed.revoke();
    assert!(journal.invalidate_previews(&renewed, 106).is_err());
    assert!(journal.invalidate_previews(&authority.grant("buwei", &["invite"], 106, 1000).unwrap(), 107).is_err());
}

#[test]
fn invalidated_queued_operation_remains_unsent_after_restart() {
    let authority = Authority::default();
    authority.set_account(Some("alice"));
    let grant = authority.grant("buwei", &["invite", "read"], 100, 1000).unwrap();
    let db_path = path();
    let mut journal = Journal::open(&db_path).unwrap();
    let backend = Backend::new(0);
    let preview = queued(&mut journal, &grant, &backend);
    let renewed = authority.grant("buwei", &["read"], 104, 1000).unwrap();
    assert_eq!(journal.invalidate_previews(&renewed, 105).unwrap(), 1);
    drop(journal);
    let mut recovered = Journal::open(&db_path).unwrap();
    assert_eq!(recovered.receipt(&grant, &preview.id, 106).unwrap().status, Status::Cancelled);
    assert!(recovered.execute(&grant, &preview.id, &backend, 106).is_err());
    assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
    assert_eq!(recovered.invalidate_previews(&renewed, 107).unwrap(), 0);
    authority.set_account(Some("bob"));
    assert!(recovered.invalidate_previews(&renewed, 108).is_err());
}
#[test]
fn confirmation_visibility_agrees_with_expiry_and_new_consent() {
    let (a, g, mut j) = setup();
    let backend = Backend::new(0);
    let preview = j.prepare(&g, action(), 0, 100, 10).unwrap();
    assert!(Journal::preview_current(&g, &preview, 109));
    assert!(!Journal::preview_current(&g, &preview, 110));
    let renewed = a.grant("buwei", &["invite"], 104, 1000).unwrap();
    assert!(!Journal::preview_current(&renewed, &preview, 105));
    assert!(j.confirm(&renewed, &preview.id, &backend, 105).is_err());
    g.revoke();
    assert!(!Journal::preview_current(&g, &preview, 105));
    assert!(j.confirm(&g, &preview.id, &backend, 105).is_err());
    assert_eq!(
        j.receipt(&renewed, &preview.id, 105).unwrap().status,
        Status::Prepared
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn account_switch_and_switch_back_invalidate_old_context() {
    let (a, g, mut j) = setup();
    let b = Backend::new(0);
    let o = j.prepare(&g, action(), 0, 100, 120).unwrap();
    a.set_account(Some("bob"));
    a.set_account(Some("alice"));
    assert!(matches!(
        j.confirm(&g, &o.id, &b, 101),
        Err(Error::Authorization)
    ));
    let fresh = a.grant("buwei", &["invite"], 101, 1000).unwrap();
    assert!(matches!(
        j.confirm(&fresh, &o.id, &b, 102),
        Err(Error::Authorization)
    ));
    assert_eq!(b.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn revoke_after_preview_rejects_confirmation() {
    let (_, g, mut j) = setup();
    let b = Backend::new(0);
    let o = j.prepare(&g, action(), 0, 100, 120).unwrap();
    g.revoke();
    assert!(matches!(
        j.confirm(&g, &o.id, &b, 101),
        Err(Error::Authorization)
    ));
}
#[test]
fn revoke_after_confirmation_rejects_execution() {
    let (_, g, mut j) = setup();
    let b = Backend::new(0);
    let o = queued(&mut j, &g, &b);
    g.revoke();
    assert!(matches!(
        j.execute(&g, &o.id, &b, 102),
        Err(Error::Authorization)
    ));
    assert_eq!(b.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn expired_confirmation_is_not_queued() {
    let (_, g, mut j) = setup();
    let b = Backend::new(0);
    let o = j.prepare(&g, action(), 0, 100, 2).unwrap();
    assert!(matches!(j.confirm(&g, &o.id, &b, 102), Err(Error::Expired)));
    assert_eq!(j.receipt(&g, &o.id, 103).unwrap().status, Status::Expired);
}
#[test]
fn expiry_between_confirmation_and_execute_never_dispatches() {
    let (_, g, mut j) = setup();
    let b = Backend::new(0);
    let o = queued(&mut j, &g, &b);
    assert!(matches!(j.execute(&g, &o.id, &b, 220), Err(Error::Expired)));
    assert_eq!(b.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn changed_business_revision_invalidates_preview() {
    let (_, g, mut j) = setup();
    let b = Backend::new(0);
    let o = j.prepare(&g, action(), 0, 100, 120).unwrap();
    b.revision.store(1, Ordering::SeqCst);
    assert!(matches!(
        j.confirm(&g, &o.id, &b, 101),
        Err(Error::Conflict)
    ));
}
#[test]
fn changed_business_revision_after_confirmation_never_dispatches() {
    let (_, g, mut j) = setup();
    let b = Backend::new(0);
    let o = queued(&mut j, &g, &b);
    b.revision.store(1, Ordering::SeqCst);
    assert!(matches!(
        j.execute(&g, &o.id, &b, 102),
        Err(Error::Conflict)
    ));
    assert_eq!(b.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn rejected_result_is_failed_without_success_evidence() {
    let (_, g, mut j) = setup();
    let b = Backend::new(2);
    let o = queued(&mut j, &g, &b);
    let r = j.execute(&g, &o.id, &b, 102).unwrap();
    assert_eq!(r.status, Status::Failed);
    assert!(r.receipt.unwrap().evidence.is_none());
    assert!(j.execute(&g, &o.id, &b, 103).is_err());
}
#[test]
fn lost_reply_can_be_reconciled_without_resending() {
    let (_, g, mut j) = setup();
    let b = Backend::new(1);
    let o = queued(&mut j, &g, &b);
    assert_eq!(
        j.execute(&g, &o.id, &b, 102).unwrap().status,
        Status::Unknown
    );
    assert!(j.execute(&g, &o.id, &b, 103).is_err());
    assert_eq!(
        j.reconcile(&g, &o.id, &b, 104).unwrap().status,
        Status::Confirmed
    );
    assert_eq!(b.calls.load(Ordering::SeqCst), 1);
}
#[test]
fn unavailable_receipt_stays_unknown() {
    let (_, g, mut j) = setup();
    let b = Backend::new(1);
    let o = queued(&mut j, &g, &b);
    j.execute(&g, &o.id, &b, 102).unwrap();
    *b.evidence.lock().unwrap() = None;
    assert_eq!(
        j.reconcile(&g, &o.id, &b, 103).unwrap().status,
        Status::Unknown
    );
    assert_eq!(b.calls.load(Ordering::SeqCst), 1);
}
#[test]
fn mismatched_external_evidence_never_confirms() {
    let (_, g, mut j) = setup();
    let b = Backend::new(3);
    let o = queued(&mut j, &g, &b);
    assert_eq!(
        j.execute(&g, &o.id, &b, 102).unwrap().status,
        Status::Unknown
    );
}
#[test]
fn external_operation_id_and_digest_must_match() {
    let (_, g, mut j) = setup();
    let b = Backend::new(1);
    let o = queued(&mut j, &g, &b);
    j.execute(&g, &o.id, &b, 102).unwrap();
    b.evidence.lock().unwrap().as_mut().unwrap().operation_id = "wrong".into();
    assert_eq!(
        j.reconcile(&g, &o.id, &b, 103).unwrap().status,
        Status::Unknown
    );
    b.evidence.lock().unwrap().as_mut().unwrap().operation_id = o.id.clone();
    b.evidence.lock().unwrap().as_mut().unwrap().digest = "wrong".into();
    assert_eq!(
        j.reconcile(&g, &o.id, &b, 104).unwrap().status,
        Status::Unknown
    );
}
#[test]
fn restart_recovers_unknown_with_fresh_same_account_consent() {
    let p = path();
    let a = Authority::default();
    a.set_account(Some("alice"));
    let g = a.grant("buwei", &["invite"], 100, 1000).unwrap();
    let b = Backend::new(1);
    let id = {
        let mut j = Journal::open(&p).unwrap();
        let o = queued(&mut j, &g, &b);
        j.execute(&g, &o.id, &b, 102).unwrap();
        o.id
    };
    let a2 = Authority::default();
    a2.set_account(Some("alice"));
    let g2 = a2.grant("buwei", &["invite"], 400, 1000).unwrap();
    let mut j = Journal::open(&p).unwrap();
    assert_eq!(
        j.reconcile(&g2, &id, &b, 401).unwrap().status,
        Status::Confirmed
    );
    assert_eq!(b.calls.load(Ordering::SeqCst), 1);
}
#[test]
fn queued_work_is_not_automatically_reauthorized_on_restart() {
    let p = path();
    let a = Authority::default();
    a.set_account(Some("alice"));
    let g = a.grant("buwei", &["invite"], 100, 1000).unwrap();
    let b = Backend::new(0);
    let id = {
        let mut j = Journal::open(&p).unwrap();
        queued(&mut j, &g, &b).id
    };
    let a2 = Authority::default();
    a2.set_account(Some("alice"));
    let g2 = a2.grant("buwei", &["invite"], 101, 1000).unwrap();
    let mut j = Journal::open(&p).unwrap();
    assert!(matches!(
        j.execute(&g2, &id, &b, 102),
        Err(Error::Authorization)
    ));
    assert_eq!(b.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn wrong_account_or_app_cannot_read_receipts() {
    let (a, g, mut j) = setup();
    let o = j.prepare(&g, action(), 0, 100, 120).unwrap();
    let other = a.grant("article", &["invite"], 100, 1000).unwrap();
    assert!(matches!(
        j.receipt(&other, &o.id, 101),
        Err(Error::Authorization)
    ));
    a.set_account(Some("bob"));
    let other = a.grant("buwei", &["invite"], 100, 1000).unwrap();
    assert!(matches!(
        j.receipt(&other, &o.id, 101),
        Err(Error::Authorization)
    ));
}
#[test]
fn undeclared_permission_is_rejected() {
    let (_, g, mut j) = setup();
    let mut act = action();
    act.permission = "publish".into();
    assert!(matches!(
        j.prepare(&g, act, 0, 100, 120),
        Err(Error::Authorization)
    ));
}
#[test]
fn cancel_prevents_execution() {
    let (_, g, mut j) = setup();
    let b = Backend::new(0);
    let o = queued(&mut j, &g, &b);
    j.cancel(&g, &o.id, 102).unwrap();
    assert!(j.execute(&g, &o.id, &b, 103).is_err());
    assert_eq!(b.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn uncertain_effect_cannot_be_cancelled_locally() {
    let (_, g, mut j) = setup();
    let b = Backend::new(1);
    let o = queued(&mut j, &g, &b);
    j.execute(&g, &o.id, &b, 102).unwrap();
    assert!(j.cancel(&g, &o.id, 103).is_err());
}
#[test]
fn journal_tampering_is_detected_before_effects() {
    let p = path();
    let (_, g, _) = setup();
    let mut j = Journal::open(&p).unwrap();
    let o = j.prepare(&g, action(), 0, 100, 120).unwrap();
    let db = rusqlite::Connection::open(&p).unwrap();
    db.execute(
        "UPDATE operations SET body=json_set(body,'$.action.target','forged')",
        [],
    )
    .unwrap();
    let b = Backend::new(0);
    assert!(matches!(
        j.confirm(&g, &o.id, &b, 101),
        Err(Error::Integrity)
    ));
    assert_eq!(b.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn two_host_connections_do_not_dispatch_twice() {
    let p = path();
    let (a, g, _) = setup();
    let mut j1 = Journal::open(&p).unwrap();
    let mut j2 = Journal::open(&p).unwrap();
    let b = Backend::new(0);
    let o = queued(&mut j1, &g, &b);
    j1.execute(&g, &o.id, &b, 102).unwrap();
    assert_eq!(
        j2.execute(&g, &o.id, &b, 103).unwrap().status,
        Status::Confirmed
    );
    assert_eq!(b.calls.load(Ordering::SeqCst), 1);
    drop(a);
}
#[test]
fn same_library_serves_another_application() {
    let (a, _, mut j) = setup();
    let g = a.grant("article", &["publish"], 100, 1000).unwrap();
    let b = Backend::new(0);
    let act = Action {
        permission: "publish".into(),
        target: "local-fixture-room".into(),
        summary: "publish a fixture article".into(),
        payload: json!({"markdown":"hello"}),
    };
    let o = j.prepare(&g, act, 0, 100, 120).unwrap();
    j.confirm(&g, &o.id, &b, 101).unwrap();
    assert_eq!(
        j.execute(&g, &o.id, &b, 102).unwrap().status,
        Status::Confirmed
    );
}
#[test]
fn invalid_confirmation_ttl_and_oversized_action_are_rejected() {
    let (_, g, mut j) = setup();
    assert!(j.prepare(&g, action(), 0, 100, 121).is_err());
    let mut act = action();
    act.payload = json!({"text":"x".repeat(40000)});
    assert!(j.prepare(&g, act, 0, 100, 120).is_err());
}
#[test]
fn unsupported_journal_version_is_preserved() {
    let p = path();
    let db = rusqlite::Connection::open(&p).unwrap();
    db.execute_batch("PRAGMA user_version=99;").unwrap();
    assert!(Journal::open(&p).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        99
    );
}

#[test]
fn editing_content_invalidates_old_preview_and_queued_confirmation() {
    let (_, g, mut j) = setup();
    let b = Backend::new(0);
    let old = queued(&mut j, &g, &b);
    let mut edited = action();
    edited.summary = "edited invitation".into();
    edited.payload = json!({"seats":1,"note":"new content"});
    let fresh = j.prepare(&g, edited, 0, 102, 120).unwrap();
    assert_ne!(old.digest, fresh.digest);
    assert_eq!(
        j.receipt(&g, &old.id, 103).unwrap().status,
        Status::Cancelled
    );
    assert!(j.execute(&g, &old.id, &b, 103).is_err());
    j.confirm(&g, &fresh.id, &b, 103).unwrap();
    j.execute(&g, &fresh.id, &b, 104).unwrap();
    assert_eq!(b.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn empty_journal_requires_valid_authorization() {
    let (_, g, j) = setup();
    g.revoke();
    assert!(matches!(j.recent(&g, 101), Err(Error::Authorization)));
    let (_, g, j) = setup();
    assert!(matches!(j.recent(&g, 1100), Err(Error::Expired)));
}

#[test]
fn receipt_from_another_sender_is_not_confirmed() {
    let (_, g, mut j) = setup();
    let b = Backend::new(1);
    let o = queued(&mut j, &g, &b);
    j.execute(&g, &o.id, &b, 102).unwrap();
    b.evidence.lock().unwrap().as_mut().unwrap().account = "bob".into();
    assert_eq!(
        j.reconcile(&g, &o.id, &b, 103).unwrap().status,
        Status::Unknown
    );
    assert_eq!(b.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn simultaneous_host_connections_dispatch_once() {
    use std::sync::{Arc, Barrier};
    let p = path();
    let (_, g, _) = setup();
    let b = Arc::new(Backend::new(0));
    let mut j = Journal::open(&p).unwrap();
    let op = queued(&mut j, &g, &b);
    drop(j);
    let barrier = Arc::new(Barrier::new(2));
    let threads = (0..2)
        .map(|_| {
            let p = p.clone();
            let g = g.clone();
            let b = b.clone();
            let id = op.id.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut j = Journal::open(p).unwrap();
                barrier.wait();
                j.execute(&g, &id, b.as_ref(), 102)
            })
        })
        .collect::<Vec<_>>();
    for t in threads {
        let _ = t.join().unwrap();
    }
    assert_eq!(b.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        Journal::open(p)
            .unwrap()
            .receipt(&g, &op.id, 103)
            .unwrap()
            .status,
        Status::Confirmed
    );
}

struct CrashAfterCommit(Backend);
impl Adapter for CrashAfterCommit {
    fn validate(&self, a: &Action, r: u64) -> Result<()> {
        self.0.validate(a, r)
    }
    fn dispatch(&self, o: &Operation) -> Dispatch {
        self.0.dispatch(o);
        panic!("simulated host crash after durable external effect")
    }
    fn lookup(&self, o: &Operation) -> Result<Option<Evidence>> {
        self.0.lookup(o)
    }
}
#[test]
fn crash_after_effect_recovers_dispatching_without_resend() {
    let p = path();
    let (_, g, _) = setup();
    let b = CrashAfterCommit(Backend::new(0));
    let mut j = Journal::open(&p).unwrap();
    let o = j.prepare(&g, action(), 0, 100, 120).unwrap();
    j.confirm(&g, &o.id, &b, 101).unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || j.execute(&g, &o.id, &b, 102)
        ))
        .is_err()
    );
    assert_eq!(
        j.receipt(&g, &o.id, 103).unwrap().status,
        Status::Dispatching
    );
    drop(j);
    let a = Authority::default();
    a.set_account(Some("alice"));
    let fresh = a.grant("buwei", &["invite"], 104, 1000).unwrap();
    let mut j = Journal::open(&p).unwrap();
    assert_eq!(
        j.reconcile(&fresh, &o.id, &b, 105).unwrap().status,
        Status::Confirmed
    );
    assert_eq!(b.0.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn task_recovery_keeps_confirmed_receipts_beyond_display_history() {
    let (_, g, mut journal) = setup();
    let backend = Backend::new(0);
    let oldest = queued(&mut journal, &g, &backend);
    journal.execute(&g, &oldest.id, &backend, 102).unwrap();
    for _ in 0..45 {
        let later_backend = Backend::new(0);
        let later = queued(&mut journal, &g, &later_backend);
        journal.execute(&g, &later.id, &later_backend, 102).unwrap();
    }
    assert!(
        !journal
            .recent(&g, 103)
            .unwrap()
            .iter()
            .any(|op| op.id == oldest.id)
    );
    let recovered = journal
        .recent_with_references(&g, &[oldest.id.clone(), oldest.id.clone()], 103)
        .unwrap();
    assert_eq!(recovered.len(), 41);
    assert_eq!(recovered.iter().filter(|op| op.id == oldest.id).count(), 1);
    assert_eq!(
        recovered
            .iter()
            .find(|op| op.id == oldest.id)
            .unwrap()
            .status,
        Status::Confirmed
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    g.revoke();
    assert!(
        journal
            .recent_with_references(&g, &[oldest.id], 104)
            .is_err()
    );
}

#[test]
fn referenced_receipts_cannot_read_another_accounts_operations() {
    let (authority, alice, mut journal) = setup();
    let oldest = queued(&mut journal, &alice, &Backend::new(0));
    authority.set_account(Some("bob"));
    let bob = authority.grant("buwei", &["invite"], 102, 1000).unwrap();
    assert!(matches!(
        journal.recent_with_references(&bob, &[oldest.id], 103),
        Err(Error::Authorization)
    ));
}
