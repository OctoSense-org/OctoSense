//! The twelve host services share one bounded queue. Ordering engine work
//! across families also orders their reads, writes and storage quota checks.
//! App Hub can keep serving requests while the worker holds an engine session.
use super::isolate::Isolate;
use octosense_appstore::services::{Replier, ServiceCall};
use serde_json::Value;
use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Mutex, Once, OnceLock};

type Task = Box<dyn FnOnce() + Send>;
const QUEUE_DEPTH: usize = 32;

static WORKER: Mutex<Option<SyncSender<Task>>> = Mutex::new(None);
type Prepare = fn(&mut ServiceCall) -> Result<Option<Box<dyn Send>>, String>;
static PREPARE: OnceLock<Prepare> = OnceLock::new();

/// The shell can bind trusted caller/account data to one request before
/// it queues. Keep that grant alive until the call finishes or is refused.
pub fn set_prepare(prepare: Prepare) {
    let _ = PREPARE.set(prepare);
}
static SWEEPS: Mutex<Vec<fn()>> = Mutex::new(Vec::new());
static DIRTY: AtomicBool = AtomicBool::new(false);

fn sender() -> Result<SyncSender<Task>, String> {
    let mut worker = WORKER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(tx) = worker.as_ref() {
        return Ok(tx.clone());
    }
    let (tx, rx) = sync_channel::<Task>(QUEUE_DEPTH);
    std::thread::Builder::new()
        .name("craft-engines".into())
        .spawn(move || {
            while let Ok(task) = rx.recv() {
                if DIRTY.swap(false, Ordering::AcqRel) {
                    let sweeps = SWEEPS.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    for sweep in sweeps {
                        // A failing cleanup must not strand other requests.
                        let _ = catch_unwind(AssertUnwindSafe(sweep));
                    }
                }
                task();
            }
        })
        .map_err(|e| format!("cannot start the engine worker: {e}"))?;
    static LISTEN: Once = Once::new();
    LISTEN.call_once(|| octosense_appstore::services::on_isolate_closed(close));
    *worker = Some(tx.clone());
    Ok(tx)
}

struct ActiveCall {
    reply: Replier,
    isolate: Isolate,
    scope: Option<super::ScopeCheck>,
}

thread_local! {
    static ACTIVE: RefCell<Option<ActiveCall>> = const { RefCell::new(None) };
}

/// Only methods that resolve a private folder need its account grant. Keep
/// the check until reply so a read in flight cannot reveal a previous account.
pub(crate) fn note_scope(check: super::ScopeCheck) {
    ACTIVE.with_borrow_mut(|active| {
        if let Some(active) = active {
            active.scope = Some(check);
        }
    });
}

/// Check a worker request again just before making an engine output visible.
/// Direct engine callers (including synchronous fixtures) have no pending
/// host request, so their area rules apply as usual.
pub fn ensure_live() -> Result<(), String> {
    ACTIVE.with_borrow(|active| {
        if let Some(active) = active {
            if !active.isolate.is_live() {
                return Err("the caller's storage scope changed".into());
            }
            if !active.reply.is_pending() {
                return Err("the engine request expired or was cancelled".into());
            }
        }
        Ok(())
    })
}

fn validate_reply() -> Result<(), String> {
    ensure_live()?;
    let scope = ACTIVE.with_borrow(|active| active.as_ref().and_then(|call| call.scope.clone()));
    if let Some(check) = scope {
        check()?;
    }
    Ok(())
}

/// Enqueue engine work without waiting for queue space. A queued request
/// can time out or be cancelled, including when its heap address is reused.
pub fn call(
    mut call: ServiceCall,
    reply: Replier,
    work: impl FnOnce(ServiceCall, Isolate) -> Result<Value, String> + Send + 'static,
) {
    let isolate = Isolate::capture(reply.isolate_key(), &call.app_id);
    let tx = match sender() {
        Ok(tx) => tx,
        Err(error) => return reply.send(Err(error)),
    };
    let grant = match PREPARE.get() {
        Some(prepare) => match prepare(&mut call) {
            Ok(held) => held,
            Err(error) => return reply.send(Err(error)),
        },
        None => None,
    };
    let answer = reply.clone();
    let task = Box::new(move || {
        let _grant = grant;
        if answer.is_pending() {
            if !isolate.is_live() {
                answer.send(Err("the caller's storage scope changed".into()));
                return;
            }
            ACTIVE.set(Some(ActiveCall {
                reply: answer.clone(),
                isolate: isolate.clone(),
                scope: None,
            }));
            let result = catch_unwind(AssertUnwindSafe(|| {
                let result = work(call, isolate);
                validate_reply()?;
                result
            }))
            .unwrap_or_else(|_| Err("the engine failed while processing the request".into()));
            ACTIVE.set(None);
            answer.send(result);
        }
    });
    if let Err(error) = tx.try_send(task) {
        let reason = match error {
            TrySendError::Full(_) => {
                "the engine queue is full; retry when an earlier request finishes"
            }
            TrySendError::Disconnected(_) => "the engine worker stopped",
        };
        reply.send(Err(reason.into()));
    }
}

/// Register a sweep of a service's worker-owned document table. Closing an
/// isolate invalidates its handles immediately and wakes the worker to drop
/// sessions and their cached files, even if no further engine call arrives.
pub fn on_isolate_closed(sweep: fn()) {
    let mut sweeps = SWEEPS.lock().unwrap_or_else(|e| e.into_inner());
    if !sweeps.iter().any(|f| std::ptr::fn_addr_eq(*f, sweep)) {
        sweeps.push(sweep);
    }
    octosense_appstore::services::on_isolate_closed(close);
}

fn close(heap: usize) {
    if super::isolate::close(heap) && !DIRTY.swap(true, Ordering::AcqRel) {
        let worker = WORKER.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(tx) = worker.as_ref() {
            // If the queue is full, its next task already wakes the worker.
            let _ = tx.try_send(Box::new(|| {}));
        }
    }
}
