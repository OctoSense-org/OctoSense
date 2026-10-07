//! Exercise actual Splash ownership without opening a native browser or provider.
use super::wait_for_consent;
use makepad_widgets::{
    web_reader::{register_auth_lifetime, WebReader},
    *,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

fn sheet(cx: &mut Cx, contained: bool) -> Splash {
    let mut splash = cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        let value = vm.eval(script! { use mod.widgets.* Splash {} });
        Splash::script_from_value(vm, value)
    });
    if contained {
        splash.set_policy(cx, Some(vec![]), None);
    }
    splash.set_text(
        cx,
        "lifetime := WebReader {width: 0 height: 0 visible: false}",
    );
    splash
}
fn bind(cx: &mut Cx, sheet: &Splash, ticket: &str) -> bool {
    let reader = sheet
        .view
        .children
        .iter()
        .find(|(id, _)| *id == id!(lifetime))
        .expect("test sheet must render its lifetime anchor")
        .1
        .clone();
    let result = reader
        .borrow_mut::<WebReader>()
        .unwrap()
        .bind_auth_lifetime(cx, ticket);
    result
}
fn ticket() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[test]
fn stopped_and_dropped_sheets_cancel_only_their_own_pending_worker() {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    let mut first = sheet(&mut cx, false);
    let second = sheet(&mut cx, false);
    let first_flag = Arc::new(AtomicBool::new(false));
    let second_flag = Arc::new(AtomicBool::new(false));
    let first_ticket = ticket();
    let second_ticket = ticket();
    assert!(register_auth_lifetime(&first_ticket, &first_flag));
    assert!(register_auth_lifetime(&second_ticket, &second_flag));
    assert!(bind(&mut cx, &first, &first_ticket));
    assert!(bind(&mut cx, &second, &second_ticket));
    let observer = first_flag.clone();
    let worker = std::thread::spawn(move || {
        wait_for_consent(
            &AtomicBool::new(false),
            &observer,
            Instant::now() + Duration::from_secs(2),
        )
    });
    first.set_text(&mut cx, "");
    assert!(first_flag.load(Ordering::SeqCst));
    assert!(!second_flag.load(Ordering::SeqCst));
    assert!(
        worker.join().unwrap().is_err(),
        "retirement must wake a pre-Continue request"
    );
    // No Cx, async pump, browser event, or deferred heap reclamation runs here.
    drop(second);
    assert!(second_flag.load(Ordering::SeqCst));
    makepad_widgets::widget_async::gc_dead_splash_isolates(&mut cx);
}

#[test]
fn auth_ticket_is_one_use_and_contained_views_cannot_consume_it() {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    let mut contained = sheet(&mut cx, true);
    let mut owner = sheet(&mut cx, false);
    let mut other = sheet(&mut cx, false);
    let flag = Arc::new(AtomicBool::new(false));
    let ticket = ticket();
    assert!(register_auth_lifetime(&ticket, &flag));
    assert!(!bind(&mut cx, &contained, &ticket));
    assert!(
        bind(&mut cx, &owner, &ticket),
        "refused app must not consume host capability"
    );
    assert!(
        bind(&mut cx, &owner, &ticket),
        "same owner may reapply its sheet"
    );
    assert!(
        !bind(&mut cx, &other, &ticket),
        "another sheet cannot replay a consumed ticket"
    );
    contained.set_text(&mut cx, "");
    other.set_text(&mut cx, "");
    assert!(!flag.load(Ordering::SeqCst));
    owner.set_text(&mut cx, "");
    assert!(flag.load(Ordering::SeqCst));
}

#[test]
fn expired_or_cancelled_requests_never_begin_provider_authorization() {
    let started = AtomicBool::new(true);
    assert!(wait_for_consent(
        &started,
        &AtomicBool::new(true),
        Instant::now() + Duration::from_secs(1)
    )
    .is_err());
    assert!(wait_for_consent(&started, &AtomicBool::new(false), Instant::now()).is_err());
    assert!(wait_for_consent(
        &started,
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(1)
    )
    .is_ok());
}

#[test]
fn unconsumed_host_tickets_are_bounded_and_cancelled_slots_are_reusable() {
    let flags: Vec<_> = (0..128)
        .map(|_| {
            let flag = Arc::new(AtomicBool::new(false));
            assert!(register_auth_lifetime(&ticket(), &flag));
            flag
        })
        .collect();
    let overflow = Arc::new(AtomicBool::new(false));
    assert!(!register_auth_lifetime(&ticket(), &overflow));
    assert!(overflow.load(Ordering::SeqCst));
    flags[0].store(true, Ordering::SeqCst);
    let next = Arc::new(AtomicBool::new(false));
    assert!(register_auth_lifetime(&ticket(), &next));
    // Registrations are weak and release all slots when request owners leave.
    drop(flags);
    drop(next);
}
