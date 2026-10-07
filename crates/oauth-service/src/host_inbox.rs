//! Gmail service for ordinary contained apps. Drafts and native send review use
//! the same host-owned store; no service method can approve or submit a reply.
use crate::{
    host::{connections, operation_lock, unix_now, with_provider_api, STORE_LOCK},
    inbox::{DraftStore, ReviewTicket},
};
use octosense_appstore::services::{self, HostService, Replier, ServiceCall, ServiceHost};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

/// The shell registers a native view, stores this request in its private view
/// registry, and returns source that mounts that view in the host-owned sheet.
/// The hook must fail if it cannot present the complete immutable snapshot.
pub type ReviewHook = Arc<dyn Fn(ReviewRequest) -> Result<String, String> + Send + Sync>;
type PublicationVerifier = Arc<dyn Fn(&str, &str, &str) -> bool + Send + Sync>;
fn publication_verifier() -> &'static Mutex<Option<PublicationVerifier>> {
    static VERIFY: OnceLock<Mutex<Option<PublicationVerifier>>> = OnceLock::new();
    VERIFY.get_or_init(|| Mutex::new(None))
}
/// Native lookup must require the exact publisher, bound account and message
/// card ID. An untrusted reported success from a model is never sufficient.
pub fn set_publication_verifier(verify: impl Fn(&str, &str, &str) -> bool + Send + Sync + 'static) {
    *publication_verifier()
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(verify));
}

struct InboxService {
    review: Option<ReviewHook>,
}

pub fn register() {
    services::register_host_service(Box::new(InboxService { review: None }));
}
pub fn register_with_review_hook(
    hook: impl Fn(ReviewRequest) -> Result<String, String> + Send + Sync + 'static,
) {
    services::register_host_service(Box::new(InboxService {
        review: Some(Arc::new(hook)),
    }));
}

/// Never serializable, cloneable or available inside the app isolate. Only the
/// native widget may hold it. Dropping an unsubmitted review cancels its ticket.
pub struct ReviewRequest {
    root: PathBuf,
    app: String,
    connection: String,
    ticket: Option<ReviewTicket>,
    snapshot: Value,
    reply: Option<Replier>,
    result: Arc<Mutex<Option<Result<Value, String>>>>,
}
impl ReviewRequest {
    /// {app, connection, draft, revision, operation, reply:{from,to,subject,body,
    /// thread_id,in_reply_to,references}}. The widget must show from/to/subject/
    /// complete body. Account and app are bound, not selected by this sheet.
    pub fn snapshot(&self) -> &Value {
        &self.snapshot
    }
    pub fn is_pending(&self) -> bool {
        self.ticket.is_some()
    }
    /// Native platform provenance must be checked independently on pointer down
    /// and pointer up. A rejected synthetic input leaves the review available.
    pub fn approve(&mut self, down_is_trusted: bool, up_is_trusted: bool) -> Result<(), String> {
        if !down_is_trusted || !up_is_trusted {
            return Err(
                "Sending requires a physical activation of the native Approve & Send control"
                    .into(),
            );
        }
        let ticket = self
            .ticket
            .take()
            .ok_or("This review was already submitted or cancelled")?;
        let root = self.root.clone();
        let app = self.app.clone();
        let connection = self.connection.clone();
        let work = Arc::new(Mutex::new(Some((ticket, self.reply.take()))));
        let worker_work = work.clone();
        let output = self.result.clone();
        let started = std::thread::Builder::new()
            .name("gmail-reviewed-send".into())
            .spawn(move || {
                let Some((ticket, reply)) =
                    worker_work.lock().unwrap_or_else(|e| e.into_inner()).take()
                else {
                    return;
                };
                let result = with_provider_api(&root, &app, |api| {
                    let mut drafts = DraftStore::open(&root, &app, &connection)?;
                    let draft = drafts.submit(ticket, down_is_trusted, up_is_trusted, api)?;
                    serde_json::to_value(draft).map_err(|_| "Cannot serialize send receipt".into())
                });
                *output.lock().unwrap_or_else(|e| e.into_inner()) = Some(result.clone());
                if let Some(reply) = reply {
                    reply.send(result);
                }
                // The native review observes this result. A late send worker
                // must never dismiss a newer sheet for the same app.
            });
        if started.is_err() {
            if let Some((ticket, reply)) = work.lock().unwrap_or_else(|e| e.into_inner()).take() {
                self.ticket = Some(ticket);
                self.reply = reply;
            }
            return Err("Could not start the send worker; nothing was submitted".into());
        }
        Ok(())
    }

    pub fn result(&self) -> Option<Result<Value, String>> {
        self.result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn cancel(&mut self) -> Result<(), String> {
        let Some(ticket) = self.ticket.take() else {
            return Ok(());
        };
        let root = self.root.clone();
        let app = self.app.clone();
        let connection = self.connection.clone();
        let work = Arc::new(Mutex::new(Some((ticket, self.reply.take()))));
        let worker_work = work.clone();
        let output = self.result.clone();
        let started = std::thread::Builder::new()
            .name("gmail-cancel-review".into())
            .spawn(move || {
                let Some((ticket, reply)) =
                    worker_work.lock().unwrap_or_else(|e| e.into_inner()).take()
                else {
                    return;
                };
                let cancelled = {
                    let operation = operation_lock(&root, &app);
                    let _guard = operation.lock().unwrap_or_else(|e| e.into_inner());
                    DraftStore::open(&root, &app, &connection)
                        .and_then(|mut store| store.cancel(ticket))
                        .map(|draft| json!({"cancelled":true,"draft":draft}))
                };
                if let Some(reply) = reply {
                    reply.send(cancelled.clone());
                }
                *output.lock().unwrap_or_else(|e| e.into_inner()) = Some(cancelled);
            });
        if started.is_err() {
            // The sole capability is dropped, so this stale persisted record
            // cannot send; a future review safely replaces it.
            let error = "Review closed; cleanup worker unavailable. Nothing was sent.".to_string();
            if let Some((_, Some(reply))) = work.lock().unwrap_or_else(|e| e.into_inner()).take() {
                reply.send(Err(error.clone()));
            }
            *self.result.lock().unwrap_or_else(|e| e.into_inner()) = Some(Err(error.clone()));
            return Err(error);
        }
        Ok(())
    }
}
impl Drop for ReviewRequest {
    fn drop(&mut self) {
        let _ = self.cancel();
    }
}

fn field<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    args[name]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Missing {name}"))
}
fn revision(args: &Value) -> Result<u64, String> {
    args["revision"]
        .as_u64()
        .filter(|r| *r > 0)
        .ok_or_else(|| "Missing saved draft revision".into())
}
impl HostService for InboxService {
    fn family(&self) -> &'static str {
        "gmail"
    }
    fn call(&mut self, call: ServiceCall, reply: Replier, host: &mut dyn ServiceHost) {
        if call.method() == "sheet.close" {
            if call.from_sheet {
                host.close_sheet();
                reply.send(Ok(Value::Null));
            } else {
                reply.send(Err(
                    "Only the current host review can close its sheet".into()
                ));
            }
            return;
        }
        if call.method() == "draft.review" {
            if !call.may_prompt {
                reply.send(Err(
                    "Open Inbox Assistant to review the complete reply".into()
                ));
                return;
            }
            let Some(hook) = self.review.as_ref() else {
                reply.send(Err("Native trusted reply review is not available on this host; your saved draft is unchanged".into()));
                return;
            };
            let pending = (|| {
                let connection = field(&call.args, "connection")?.to_string();
                let operation = operation_lock(&call.host_dir, &call.app_id);
                let _guard = operation.try_lock().map_err(|_| {
                    "Another account operation is busy; retry review when it finishes"
                })?;
                let store = connections(&call.host_dir)?;
                store.authorized(
                    &call.app_id,
                    &connection,
                    crate::Provider::Google,
                    crate::api::GMAIL_SEND_SCOPE,
                )?;
                let ticket = DraftStore::open(&call.host_dir, &call.app_id, &connection)?
                    .review(field(&call.args, "draft")?, revision(&call.args)?)?;
                Ok(ReviewRequest {
                    root: call.host_dir,
                    app: call.app_id,
                    connection,
                    snapshot: ticket.snapshot(),
                    ticket: Some(ticket),
                    reply: Some(reply.clone()),
                    result: Arc::new(Mutex::new(None)),
                })
            })();
            match pending {
                Ok(request) => match hook(request) {
                    Ok(source) => host.open_sheet(source),
                    Err(error) => reply.send(Err(error)),
                },
                Err(error) => reply.send(Err(error)),
            }
            return;
        }
        std::thread::spawn(move || {
            let result = (|| {
                let connection = field(&call.args, "connection")?;
                with_provider_api(&call.host_dir, &call.app_id, |api| {
                    // Authorization is checked even for local cached draft data.
                    api.connections.authorized(
                        &call.app_id,
                        connection,
                        crate::Provider::Google,
                        crate::api::GMAIL_READ_SCOPE,
                    )?;
                    match call.method(){
                        "labels"=>api.gmail_labels(&call.app_id,connection),
                        "messages"=>api.gmail_inbox(&call.app_id,connection,call.args["label"].as_str().unwrap_or("INBOX"),call.args["page_token"].as_str()),
                        "message"=>serde_json::to_value(api.inbox_message(&call.app_id,connection,field(&call.args,"message_id")?)?).map_err(|_|"Cannot serialize message".into()),
                        "draft.open"=>{
                            let message=api.inbox_message(&call.app_id,connection,field(&call.args,"message_id")?)?;
                            let sender=api.gmail_sender(&call.app_id,connection)?;
                            let draft=DraftStore::open(&call.host_dir,&call.app_id,connection)?.reply(&message,&sender)?;
                            Ok(json!(draft))
                        },
                        "draft.get"=>Ok(json!(DraftStore::open(&call.host_dir,&call.app_id,connection)?.get(field(&call.args,"draft")?)?)),
                        "draft.edit"=>{
                            let to=field(&call.args,"to")?;let subject=call.args["subject"].as_str().ok_or("Missing subject")?;let body=call.args["body"].as_str().ok_or("Missing body")?;
                            let draft=DraftStore::open(&call.host_dir,&call.app_id,connection)?.edit(field(&call.args,"draft")?,revision(&call.args)?,to,subject,body,call.args["provenance"].as_str().unwrap_or("manual"))?;
                            Ok(json!(draft))
                        },
                        "event.status"=>{
                            let id=event_message(&call.args)?;
                            let store=crate::inbox_events::EventStore::open(&call.host_dir,&call.app_id,connection)?;
                            Ok(json!({"decision":store.decision(id)}))
                        },
                        "events.status"=>Ok(json!(crate::inbox_events::EventStore::open(
                            &call.host_dir,&call.app_id,connection)?.status())),
                        "event.decide"=>{
                            let id=event_message(&call.args)?;let decision=field(&call.args,"decision")?;
                            let verified=if decision=="published" {
                                let verify=publication_verifier().lock().unwrap_or_else(|e|e.into_inner()).clone();
                                verify.is_some_and(|verify|verify(&call.app_id,connection,id))
                            } else {false};
                            let result=crate::inbox_events::EventStore::open(&call.host_dir,&call.app_id,connection)?.decide(id,decision,field(&call.args,"reason")?,verified,unix_now())?;
                            Ok(json!({"recorded":true,"decision":result}))
                        },
                        "send"|"sheet.send"|"draft.send"=>Err("Apps and agents cannot approve sending. Request draft.review and physically activate the native host control.".into()),
                        _=>Err("Unknown Gmail operation".into()),
                    }
                })
            })();
            reply.send(result);
        });
    }
}
fn event_message(args: &Value) -> Result<&str, String> {
    field(args, "event_id")?
        .strip_prefix("gmail.")
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
        .ok_or_else(|| "Invalid Gmail event identity".into())
}
/// A host-created binding plus untrusted message content for the app's peer.
/// The shell must authenticate this app/account and inject the connection into
/// its Gmail tool calls; message text does not supply identity or authority.
pub struct IncomingEvent {
    pub app: String,
    pub connection: String,
    pub event_id: String,
    pub message: crate::inbox::Message,
}
pub type EventHook =
    Arc<dyn Fn(IncomingEvent, EventCompletion) -> Result<(), String> + Send + Sync>;

/// Hold until the app-peer turn completes. Dropping a queued/cancelled/failed
/// turn releases the lease for a later quiet poll; it never sends mail.
pub struct EventCompletion {
    root: PathBuf,
    app: String,
    connection: String,
    lease: Option<crate::inbox_events::EventLease>,
}
impl EventCompletion {
    /// The shell calls this only after its broker reports actual turn success.
    /// Run on the collector worker: return only after durable acknowledgement.
    /// A successful final answer without a recorded decision is a failure, not
    /// a completed email. Drop remains asynchronous to avoid blocking callers.
    pub fn complete(mut self, success: bool) -> Result<(), String> {
        let Some(lease) = self.lease.take() else {
            return Ok(());
        };
        let operation = operation_lock(&self.root, &self.app);
        let _guard = operation.lock().unwrap_or_else(|e| e.into_inner());
        let mut store =
            crate::inbox_events::EventStore::open(&self.root, &self.app, &self.connection)?;
        if success {
            let still_active = connections(&self.root)?
                .active(&self.app)
                .is_some_and(|c| c.handle == self.connection);
            if !still_active || store.decision(lease.message_id()).is_none() {
                store.complete(lease, false, unix_now())?;
                return Err("Gmail event needs its active account and a durable quiet or publication decision".into());
            }
        }
        store.complete(lease, success, unix_now())
    }
    fn finish(&mut self, success: bool) -> Result<(), String> {
        let Some(lease) = self.lease.take() else {
            return Ok(());
        };
        let root = self.root.clone();
        let app = self.app.clone();
        let connection = self.connection.clone();
        std::thread::Builder::new()
            .name("gmail-event-completion".into())
            .spawn(move || {
                let operation = operation_lock(&root, &app);
                let _guard = operation.lock().unwrap_or_else(|e| e.into_inner());
                // Failure to persist an acknowledgement leaves a leased event,
                // which expires and retries with its original stable message ID.
                let _ = crate::inbox_events::EventStore::open(&root, &app, &connection)
                    .and_then(|mut store| store.complete(lease, success, unix_now()));
            })
            .map(|_| ())
            .map_err(|_| "Could not schedule Gmail event acknowledgement".into())
    }
}
impl Drop for EventCompletion {
    fn drop(&mut self) {
        let _ = self.finish(false);
    }
}

#[derive(serde::Serialize)]
pub struct BackgroundReport {
    pub initialized: bool,
    pub added: usize,
    pub dispatched: usize,
    pub pending: usize,
    pub recovered_history: bool,
}

/// Used by the shell's authorized app discovery, never an app service method.
/// Shell must first check the admitted Gmail capability, agent consent,
/// background permission and declared `<app namespace>.new_message` trigger.
pub fn connected_accounts(root: &Path, app: &str) -> Result<Vec<String>, String> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Ok(connections(root)?
        .list(app)
        .into_iter()
        .filter(|c| {
            c.provider == crate::Provider::Google && c.scopes.contains(crate::api::GMAIL_READ_SCOPE)
        })
        .map(|c| c.handle)
        .collect())
}

/// Run on the existing background job worker, not Makepad's UI or a Tokio core
/// worker. This method makes bounded blocking provider requests. It never calls
/// a model directly: the shell hook routes each event to the authenticated app
/// peer, which decides importance using its admitted AGENT.md and skills.
pub fn check_incoming(
    root: &Path,
    app: &str,
    connection: &str,
    hook: &EventHook,
) -> Result<BackgroundReport, String> {
    use crate::inbox_events::{EventStore, GmailEvents};
    let poll = with_provider_api(root, app, |api| {
        api.connections.authorized(
            app,
            connection,
            crate::Provider::Google,
            crate::api::GMAIL_READ_SCOPE,
        )?;
        if !api
            .connections
            .active(app)
            .is_some_and(|active| active.handle == connection)
        {
            return Err(
                "Only the active, consented Gmail account may run background checks".into(),
            );
        }
        let mut store = EventStore::open(root, app, connection)?;
        let now = api.now;
        let poll = store.refresh(
            &mut GmailEvents {
                api,
                app,
                connection,
            },
            now,
        )?;
        Ok(poll)
    })?;
    let mut dispatched = 0;
    // Claim just before dispatch. A slow peer or failed first item must not
    // strand later messages behind a ten-minute lease they never used.
    for _ in 0..4 {
        let lease = {
            let operation = operation_lock(root, app);
            let _guard = operation.lock().unwrap_or_else(|e| e.into_inner());
            if !connections(root)?
                .active(app)
                .is_some_and(|c| c.handle == connection)
            {
                return Err("Incoming Gmail account changed before dispatch".into());
            }
            EventStore::open(root, app, connection)?
                .claim(unix_now(), 1)?
                .pop()
        };
        let Some(lease) = lease else {
            break;
        };
        let id = lease.message_id().to_string();
        let completion = EventCompletion {
            root: root.into(),
            app: app.into(),
            connection: connection.into(),
            lease: Some(lease),
        };
        let message = with_provider_api(root, app, |api| {
            api.inbox_message_if_present(app, connection, &id)
        });
        match message {
            Ok(Some(message)) => {
                // If the person already moved/deleted this from Inbox, do not
                // interrupt them with a stale card. Mark this event processed.
                if !message.labels.iter().any(|label| label == "INBOX") {
                    {
                        let operation = operation_lock(root, app);
                        let _guard = operation.lock().unwrap_or_else(|e| e.into_inner());
                        EventStore::open(root, app, connection)?.decide(
                            &id,
                            "quiet",
                            "Message is no longer in Inbox",
                            false,
                            unix_now(),
                        )?;
                    }
                    completion.complete(true)?;
                    continue;
                }
                hook(
                    IncomingEvent {
                        app: app.into(),
                        connection: connection.into(),
                        event_id: format!("gmail.{id}"),
                        message,
                    },
                    completion,
                )?;
                dispatched += 1;
            }
            Ok(None) => {
                {
                    let operation = operation_lock(root, app);
                    let _guard = operation.lock().unwrap_or_else(|e| e.into_inner());
                    EventStore::open(root, app, connection)?.decide(
                        &id,
                        "quiet",
                        "Message was deleted before processing",
                        false,
                        unix_now(),
                    )?;
                }
                completion.complete(true)?;
            }
            Err(error) => {
                completion.complete(false)?;
                return Err(error);
            }
        }
    }
    Ok(BackgroundReport {
        initialized: poll.initialized,
        added: poll.added,
        dispatched,
        pending: pending_count(root, app, connection)?,
        recovered_history: poll.recovered_history,
    })
}

/// Exact durable queue length for native scheduler status; no provider request.
pub fn pending_count(root: &Path, app: &str, connection: &str) -> Result<usize, String> {
    let operation = operation_lock(root, app);
    let _guard = operation.lock().unwrap_or_else(|e| e.into_inner());
    Ok(crate::inbox_events::EventStore::open(root, app, connection)?.pending_count())
}

#[cfg(test)]
mod sheet_tests {
    use super::*;

    #[test]
    fn only_the_originating_host_sheet_can_close_gmail_review() {
        const FAMILY: &str = "gmail_close_test";
        const HEAP: usize = 981117;
        struct TestService(InboxService);
        impl HostService for TestService {
            fn family(&self) -> &'static str {
                FAMILY
            }
            fn call(&mut self, call: ServiceCall, reply: Replier, host: &mut dyn ServiceHost) {
                self.0.call(call, reply, host)
            }
        }
        #[derive(Default)]
        struct Sheet(usize);
        impl ServiceHost for Sheet {
            fn open_sheet(&mut self, _: String) {
                panic!("close cannot open a sheet")
            }
            fn close_sheet(&mut self) {
                self.0 += 1;
            }
        }
        services::register_host_service(Box::new(TestService(InboxService { review: None })));
        let mut sheet = Sheet::default();
        for (id, from_sheet) in [(1, false), (2, true)] {
            services::dispatch(
                ServiceCall {
                    app_id: "org.octosense.samples.inbox".into(),
                    service: format!("{FAMILY}.sheet.close"),
                    args: json!({}),
                    from_sheet,
                    may_prompt: true,
                    host_dir: PathBuf::from("synthetic-unused-root"),
                },
                HEAP,
                id,
                &mut sheet,
            );
            let responses = services::take_replies_for(&[HEAP]);
            assert_eq!(responses.len(), 1);
            assert_eq!(responses[0].2.is_ok(), from_sheet);
            assert_eq!(sheet.0, usize::from(from_sheet));
        }
    }
}
