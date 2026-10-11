//! Public app composers use Mail's transport and receipt engine, but never its
//! historical reply drafts. Native review preparation and sending run off UI.
use super::*;
use octosense_appstore::services::{AgentAccess, HostApiMethod};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc::{sync_channel, Receiver, SyncSender},
};

pub type ReviewHook = Arc<dyn Fn(ReviewRequest) -> Result<String, String> + Send + Sync>;
fn hook() -> &'static Mutex<Option<ReviewHook>> {
    static HOOK: std::sync::OnceLock<Mutex<Option<ReviewHook>>> = std::sync::OnceLock::new();
    HOOK.get_or_init(Default::default)
}
pub fn on_review(callback: Option<ReviewHook>) {
    *hook().lock().unwrap_or_else(|e| e.into_inner()) = callback;
}

static WORKERS: AtomicUsize = AtomicUsize::new(0);
struct WorkerSlot;
impl WorkerSlot {
    fn reserve() -> Result<Self, String> {
        WORKERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 16).then_some(n + 1)
            })
            .map(|_| Self)
            .map_err(|_| "resource_limit: Mail review workers are busy; retry later".into())
    }
}
impl Drop for WorkerSlot {
    fn drop(&mut self) {
        WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(crate) fn work(task: impl FnOnce() + Send + 'static) -> Result<(), String> {
    let slot = WorkerSlot::reserve()?;
    std::thread::Builder::new()
        .name("mail-composer".into())
        .spawn(move || {
            let _slot = slot;
            task();
        })
        .map(|_| ())
        .map_err(|_| "Could not start the Mail composer worker".into())
}

// If a closed UI drops its queue, the queued capability is retired immediately.
// Retirement touches only the bounded capability map, never draft I/O/SERIAL.
struct Prepared(Option<drafts::Review>);
impl Drop for Prepared {
    fn drop(&mut self) {
        if let Some(review) = self.0.take() {
            drafts::retire_public_review(review);
        }
    }
}
enum Update {
    Ready(Prepared),
    Outcome(Result<Value, String>),
}

/// Neither serializable nor cloneable. UI polls this bounded channel; it never
/// waits on the worker's draft lock or a worker-held result mutex.
pub struct ReviewRequest {
    review: Option<drafts::Review>,
    snapshot: Value,
    reply: Option<Replier>,
    updates: Receiver<Update>,
    sender: SyncSender<Update>,
    result: Option<Result<Value, String>>,
    preparing: bool,
    cancelled: bool,
}
impl ReviewRequest {
    pub fn snapshot(&self) -> &Value {
        &self.snapshot
    }
    pub fn is_preparing(&self) -> bool {
        self.preparing
    }
    pub fn can_approve(&self) -> bool {
        !self.preparing && self.review.is_some() && self.result.is_none()
    }
    pub fn result(&mut self) -> Option<Result<Value, String>> {
        // At most one pending update per request; no unbounded UI draining.
        if let Ok(update) = self.updates.try_recv() {
            match update {
                Update::Ready(mut prepared) if !self.cancelled => {
                    if let Some(review) = prepared.0.take() {
                        self.snapshot = review.snapshot().clone();
                        self.review = Some(review);
                    }
                    self.preparing = false;
                }
                Update::Outcome(result) if !self.cancelled => {
                    self.result = Some(result);
                    self.preparing = false;
                }
                _ => {}
            }
        }
        self.result.clone()
    }
    pub fn approve(&mut self, down: bool, up: bool) -> Result<(), String> {
        if !down || !up {
            return Err(
                "Sending requires a physical activation of the native Approve & Send control"
                    .into(),
            );
        }
        if self.preparing {
            return Err("The complete review is still being prepared".into());
        }
        if !self.reply.as_ref().is_some_and(Replier::is_pending) {
            return Err(
                "The originating app closed or cancelled this request; review again".into(),
            );
        }
        let slot = WorkerSlot::reserve()?;
        let review = self
            .review
            .take()
            .ok_or("This review was already submitted or cancelled")?;
        // One ownership handoff only. The UI never polls this mutex; a failed
        // thread start restores the capability without touching draft storage.
        let work = Arc::new(Mutex::new(Some((
            Prepared(Some(review)),
            self.reply.take(),
        ))));
        let worker = work.clone();
        let output = self.sender.clone();
        let started=std::thread::Builder::new().name("mail-reviewed-send".into()).spawn(move||{
            let _slot=slot;
            let Some((mut prepared,reply))=worker.lock().unwrap_or_else(|e|e.into_inner()).take() else{return};
            let result=if let Some(reply)=reply.as_ref().filter(|r|r.is_pending()) {
                drafts::approve_and_send_pending(prepared.0.take().expect("owned review"),reply).and_then(|draft|{
                    if draft["status"]=="accepted" {Ok(receipt(draft))}
                    else {Err(format!("Submission status: {}. Inspect mail.compose_status for compose_id {} before retrying; do not automatically resend.",text(&draft,"status"),text(&draft,"compose_id")))}
                })
            } else {Err("The originating app closed before submission; nothing was sent".into())};
            let _=output.try_send(Update::Outcome(result.clone()));
            if let Some(reply)=reply {reply.send(result);}
            drafts::wake_ui();
        });
        if started.is_err() {
            if let Some((mut prepared, reply)) =
                work.lock().unwrap_or_else(|e| e.into_inner()).take()
            {
                self.review = prepared.0.take();
                self.reply = reply;
            }
            return Err("Could not start the send worker; nothing was submitted".into());
        }
        Ok(())
    }
    pub fn cancel(&mut self) -> Result<(), String> {
        if self.reply.is_some() {
            self.cancelled = true;
            self.preparing = false;
            if let Some(review) = self.review.take() {
                drafts::retire_public_review(review);
            }
            let result = Err("Review cancelled; nothing was sent".to_string());
            self.result = Some(result.clone());
            if let Some(reply) = self.reply.take() {
                reply.send(result);
            }
        }
        Ok(())
    }
}
impl Drop for ReviewRequest {
    fn drop(&mut self) {
        let _ = self.cancel();
    }
}

fn receipt(draft: Value) -> Value {
    let attempt = draft["attempts"]
        .as_array()
        .and_then(|a| a.last())
        .cloned()
        .unwrap_or(Value::Null);
    json!({"accepted":true,"status":"accepted","id":attempt["payload"]["message_id"],
        "compose_id":draft["compose_id"],"draft_id":draft["draft_id"],"revision":draft["revision"],
        "operation_id":attempt["operation_id"],"receipt":attempt["receipt"]})
}

pub(crate) fn open(store: &Store, call: &ServiceCall, reply: Replier, host: &mut dyn ServiceHost) {
    let result = (|| {
        if !call.may_prompt || call.from_sheet {
            return Err("Open the app to review this message".into());
        }
        if !cfg!(any(target_os = "macos", target_os = "android")) {
            return Err("Physical Mail send approval is unavailable on this platform".into());
        }
        let mount = hook()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or("This host has no native Mail review surface")?;
        let slot = WorkerSlot::reserve()?;
        let (sender, updates) = sync_channel(1);
        let output = sender.clone();
        let pending = reply.clone();
        let worker_store = Store::at(&call.host_dir, store.vault.clone());
        let app = call.app_id.clone();
        let args = call.args.clone();
        std::thread::Builder::new()
            .name("mail-prepare-review".into())
            .spawn(move || {
                let _slot = slot;
                if !pending.is_pending() {
                    return;
                }
                match drafts::composer_review(&worker_store, &app, &args) {
                    Ok(review) => {
                        let _ = output.try_send(Update::Ready(Prepared(Some(review))));
                    }
                    Err(error) => {
                        let _ = output.try_send(Update::Outcome(Err(error.clone())));
                        pending.send(Err(error));
                    }
                }
                drafts::wake_ui();
            })
            .map_err(|_| "Could not start review preparation; nothing was submitted")?;
        mount(ReviewRequest {
            review: None,
            snapshot: json!({"publisher":call.app_id,"account":call.args["account"],"payload":{}}),
            reply: Some(reply.clone()),
            updates,
            sender,
            result: None,
            preparing: true,
            cancelled: false,
        })
    })();
    match result {
        Ok(source) => host.open_sheet(source),
        Err(error) => reply.send(Err(error)),
    }
}

pub(crate) fn api_methods() -> Vec<HostApiMethod> {
    let compose = json!({"type":"object","properties":{
        "account":{"type":"string","minLength":1},"to":{"type":"string","minLength":1,"maxLength":254},
        "subject":{"type":"string","maxLength":512},"body":{"type":"string","maxLength":8192},
        "compose_id":{"type":"string","pattern":"^compose-[a-zA-Z0-9-]+$"},
        "expected_revision":{"type":"integer","minimum":1},"folder":{"type":"string","maxLength":256},"message":{"type":"string","maxLength":128}},
        "required":["account","to","subject","body"],"additionalProperties":false});
    let status = json!({"type":"object","properties":{"account":{"type":"string","minLength":1},"compose_id":{"type":"string","minLength":1}},"required":["account","compose_id"],"additionalProperties":false});
    [("mail.compose", "Save an app/account-bound draft; never send or prompt", compose.clone(), AgentAccess::Allowed),
        ("mail.compose_status", "Read this app/account's draft and submission receipt", status, AgentAccess::Allowed),
        ("mail.review_send", "Review the exact composed message in a native host sheet; physical approval required", compose.clone(), AgentAccess::ForegroundOnly),
        ("mail.send", "Compatibility review request; returns acceptance only after physical approval and SMTP response", compose, AgentAccess::ForegroundOnly)]
        .into_iter().map(|(name,summary,input,access)| HostApiMethod::new(name,1,"mail",summary,input,json!({"type":"object"}))
            .with_platforms(if access == AgentAccess::ForegroundOnly { &["macos","android"] } else { &["macos","android","linux","windows"] })
            .with_agent_access(access)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_distinguishes_staging_from_physical_review_and_hides_sheet_controls() {
        let methods = api_methods();
        assert_eq!(methods.len(), 4);
        for method in methods {
            method.validate().unwrap();
            assert_eq!(method.capability, "mail");
            assert!(!method.name.contains("sheet"));
            if ["mail.compose", "mail.compose_status"].contains(&method.name.as_str()) {
                assert_eq!(method.agent_access, AgentAccess::Allowed);
                assert!(method.supports("linux") && method.supports("windows"));
            } else {
                assert_eq!(method.agent_access, AgentAccess::ForegroundOnly);
                assert!(method.supports("macos") && method.supports("android"));
                assert!(!method.supports("linux") && !method.supports("windows"));
            }
        }
    }
}
