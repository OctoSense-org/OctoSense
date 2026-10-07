//! Scoped provider operations and review of immutable remote-write snapshots.
use crate::{
    api::{Api, CalendarEvent, GithubFile},
    host::{clients, connections, provider_transport, unix_now, STORE_LOCK},
    providers::ClientRegistration,
};
use octosense_appstore::services::{self, HostService, Replier, ServiceCall, ServiceHost};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

enum Change {
    Github(GithubFile),
    Calendar {
        calendar: String,
        event: CalendarEvent,
        existing: Option<(String, String)>,
        create_id: String,
    },
}
struct Review {
    app: String,
    root: PathBuf,
    connection: String,
    change: Change,
    reply: Replier,
    deadline: Instant,
    save: Arc<SaveState>,
    account: String,
}
#[derive(Default)]
struct SaveState {
    phase: AtomicU8, // 0 pending, 1 submitted, 2 cancelled
    result: Mutex<Option<Result<Value, String>>>,
}
impl SaveState {
    fn start(&self) -> bool {
        self.phase
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    fn started(&self) -> bool {
        self.phase.load(Ordering::Acquire) == 1
    }
    fn cancel(&self) -> bool {
        self.phase
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    fn result(&self) -> Option<Result<Value, String>> {
        self.result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

/// A native-only, single-use capability for an immutable reviewed write.
/// No service method or serializable field can construct or approve it.
pub struct ReviewRequest {
    family: &'static str,
    ticket: String,
    review: Arc<Review>,
}
impl ReviewRequest {
    pub fn snapshot(&self) -> Value {
        review_snapshot(&self.review)
    }
    pub fn close_request(&self) -> (String, String) {
        (format!("{}.sheet.cancel", self.family), self.ticket.clone())
    }
    pub fn result(&self) -> Option<Result<Value, String>> {
        self.review.save.result()
    }
    fn claim_approval(&self, down: bool, up: bool) -> Result<(), String> {
        if !down || !up {
            return Err(
                "Saving requires a physical activation of the native Approve & Save control".into(),
            );
        }
        if Instant::now() >= self.review.deadline {
            self.cancel()?;
            return Err("Review expired; check the current draft again".into());
        }
        if !self.review.save.start() {
            return Err(
                "This review was already submitted or cancelled; review the draft again".into(),
            );
        }
        Ok(())
    }
    /// Capture native down/up provenance synchronously before crossing into
    /// the worker. Rejected automation does not consume the pending review.
    pub fn approve(&mut self, down: bool, up: bool) -> Result<(), String> {
        self.claim_approval(down, up)?;
        let review = self.review.clone();
        let fallback = review.clone();
        if std::thread::Builder::new()
            .name("connector-reviewed-save".into())
            .spawn(move || {
                let result = with_api(&review.root, |api| {
                    if !api
                        .connections
                        .active(&review.app)
                        .is_some_and(|c| c.handle == review.connection)
                    {
                        return Err(
                            "The selected account changed; review again under the original account"
                                .into(),
                        );
                    }
                    match &review.change {
                        Change::Github(file) => {
                            api.github_save(&review.app, &review.connection, file)
                        }
                        Change::Calendar {
                            calendar,
                            event,
                            existing,
                            create_id,
                        } => api.calendar_save(
                            &review.app,
                            &review.connection,
                            calendar,
                            event,
                            existing
                                .as_ref()
                                .map(|(id, etag)| (id.as_str(), etag.as_str())),
                            create_id,
                        ),
                    }
                });
                *review.save.result.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(result.clone());
                review.reply.clone().send(result);
            })
            .is_err()
        {
            let error =
                "Could not start the save worker; nothing was submitted. Review again.".to_owned();
            *fallback
                .save
                .result
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some(Err(error.clone()));
            fallback.reply.clone().send(Err(error.clone()));
            return Err(error);
        }
        Ok(())
    }
    pub fn cancel(&self) -> Result<(), String> {
        if self.review.save.cancel() {
            self.review
                .reply
                .clone()
                .send(Err("Save cancelled; your local draft is unchanged".into()));
        } else if self.review.save.started() && self.review.save.result().is_none() {
            return Err("The approved save is still in progress".into());
        }
        Ok(())
    }
}
impl Drop for ReviewRequest {
    fn drop(&mut self) {
        let _ = self.cancel();
    }
}
struct Connector {
    family: &'static str,
    review_hook: Option<ReviewHook>,
    reviews: HashMap<String, Arc<Review>>,
    // This small owner record outlives an expired private snapshot so its
    // host sheet retains a scoped Back action without retaining the draft.
    current: HashMap<(String, PathBuf), String>,
}

pub fn register() {
    register_review(None);
}
pub type ReviewHook = Arc<dyn Fn(ReviewRequest) -> Result<String, String> + Send + Sync>;
pub fn register_with_review_hook(
    hook: impl Fn(ReviewRequest) -> Result<String, String> + Send + Sync + 'static,
) {
    register_review(Some(Arc::new(hook)));
}
fn register_review(review_hook: Option<ReviewHook>) {
    for family in ["github", "gcalendar"] {
        services::register_host_service(Box::new(Connector {
            family,
            review_hook: review_hook.clone(),
            reviews: HashMap::new(),
            current: HashMap::new(),
        }));
    }
}
fn field<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    value[name]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Missing {name}"))
}

impl HostService for Connector {
    fn family(&self) -> &'static str {
        self.family
    }
    fn call(&mut self, call: ServiceCall, reply: Replier, host: &mut dyn ServiceHost) {
        self.reviews.retain(|ticket, review| {
            // Keep the visible ticket available for Back to editing even if it
            // expires. An approved write must settle with its actual outcome.
            let current = call.from_sheet
                && call.args["ticket"].as_str() == Some(ticket)
                && review.app == call.app_id
                && review.root == call.host_dir;
            if Instant::now() >= review.deadline && !current {
                if review.save.cancel() {
                    review
                        .reply
                        .clone()
                        .send(Err("Review expired; check the current draft again".into()));
                }
                false
            } else {
                true
            }
        });
        match call.method() {
            "prepare" if self.family == "gcalendar" => {
                reply.send(crate::calendar_cache::prepare_draft(call.args));
            }
            "review_save" if self.family != "gmail" => {
                if !call.may_prompt {
                    reply.send(Err("Open the app to review this change".into()));
                    return;
                }
                let Some(hook) = self.review_hook.clone() else {
                    reply.send(Err("Native trusted save review is unavailable on this host; your local draft is unchanged".into()));
                    return;
                };
                let parsed = (|| {
                    let connection = field(&call.args, "connection")?.to_string();
                    let change = if self.family == "github" {
                        let file: GithubFile = serde_json::from_value(call.args["file"].clone())
                            .map_err(|_| "Invalid Markdown save request")?;
                        file.validate()?;
                        Change::Github(file)
                    } else {
                        let calendar = field(&call.args, "calendar")?.to_string();
                        let event: CalendarEvent =
                            serde_json::from_value(call.args["event"].clone())
                                .map_err(|_| "Invalid event draft")?;
                        event.validate()?;
                        let existing = if call.args["event_id"].is_null() {
                            None
                        } else {
                            Some((
                                field(&call.args, "event_id")?.into(),
                                field(&call.args, "etag")?.into(),
                            ))
                        };
                        Change::Calendar {
                            calendar,
                            event,
                            existing,
                            create_id: Uuid::new_v4().simple().to_string(),
                        }
                    };
                    let _guard = STORE_LOCK
                        .try_lock()
                        .map_err(|_| "Account storage is busy; try review again")?;
                    let store = connections(&call.host_dir)?;
                    let account = store
                        .active(&call.app_id)
                        .filter(|account| account.handle == connection)
                        .ok_or(
                            "The selected account changed; choose the account before reviewing",
                        )?;
                    let (provider, scope) = if self.family == "github" {
                        (
                            crate::Provider::Github,
                            if account.scopes.contains("repo") {
                                "repo"
                            } else {
                                "public_repo"
                            },
                        )
                    } else {
                        (crate::Provider::Google, crate::api::CALENDAR_SCOPE)
                    };
                    store.authorized(&call.app_id, &connection, provider, scope)?;
                    Ok((connection, change, account.label))
                })();
                let (connection, change, account) = match parsed {
                    Ok(v) => v,
                    Err(e) => {
                        reply.send(Err(e));
                        return;
                    }
                };
                // A replaced sheet must settle the request whose controls disappeared.
                let previous: Vec<_> = self
                    .reviews
                    .iter()
                    .filter(|(_, r)| r.app == call.app_id && r.root == call.host_dir)
                    .map(|(id, _)| id.clone())
                    .collect();
                for id in previous {
                    if let Some(old) = self.reviews.remove(&id) {
                        if old.save.cancel() {
                            old.reply
                                .clone()
                                .send(Err("A newer draft replaced this review".into()));
                        }
                    }
                }
                if self.reviews.len() >= 32 {
                    reply.send(Err(
                        "Too many pending reviews; close an earlier review".into()
                    ));
                    return;
                }
                let ticket = Uuid::new_v4().to_string();
                let review = Arc::new(Review {
                    app: call.app_id,
                    root: call.host_dir,
                    connection,
                    change,
                    reply,
                    deadline: Instant::now() + Duration::from_secs(600),
                    save: Arc::new(SaveState::default()),
                    account,
                });
                let request = ReviewRequest {
                    family: self.family,
                    ticket: ticket.clone(),
                    review: review.clone(),
                };
                let source = match hook(request) {
                    Ok(source) => source,
                    Err(error) => {
                        review.reply.clone().send(Err(error));
                        return;
                    }
                };
                host.open_sheet(source);
                self.current
                    .insert((review.app.clone(), review.root.clone()), ticket.clone());
                self.reviews.insert(ticket, review);
            }
            "sheet.save" => {
                reply.send(Err("Saving requires a physical activation of the native host review. Script and agent requests cannot approve it.".into()));
            }
            "sheet.cancel" | "sheet.status" => {
                if !call.from_sheet {
                    reply.send(Err("Approval belongs to the host review screen".into()));
                    return;
                }
                let ticket = call.args["ticket"].as_str().unwrap_or("");
                let owner = (call.app_id.clone(), call.host_dir.clone());
                if self.current.get(&owner).map(String::as_str) != Some(ticket) {
                    reply.send(Err("Review expired or belongs to another app".into()));
                    return;
                }
                if !self.reviews.contains_key(ticket) {
                    if call.method() == "sheet.cancel" {
                        self.current.remove(&owner);
                        host.close_sheet();
                        reply.send(Ok(json!({"cancelled":true})));
                    } else {
                        reply.send(Err(
                            "Review expired; return to editing and review again".into()
                        ));
                    }
                    return;
                }
                if !self
                    .reviews
                    .get(ticket)
                    .is_some_and(|r| r.app == call.app_id && r.root == call.host_dir)
                {
                    reply.send(Err("Review expired or belongs to another app".into()));
                    return;
                }
                let review = self.reviews[ticket].clone();
                if call.method() == "sheet.status" {
                    let status = match review.save.result() {
                        Some(Ok(result)) => {
                            // Only this ticket's visible sheet closes itself.
                            // A worker finishing never queues an app-wide close.
                            self.reviews.remove(ticket);
                            self.current.remove(&owner);
                            host.close_sheet();
                            json!({"phase":"saved", "result":result})
                        }
                        Some(Err(message)) => json!({"phase":"error", "message":message}),
                        None if review.save.started() => json!({"phase":"saving"}),
                        None if Instant::now() >= review.deadline => {
                            let message = "Review expired; check the current draft again";
                            review.reply.clone().send(Err(message.into()));
                            json!({"phase":"error", "message":message})
                        }
                        None => json!({"phase":"ready"}),
                    };
                    reply.send(Ok(status));
                    return;
                }
                if call.method() == "sheet.cancel" {
                    if review.save.started() && review.save.result().is_none() {
                        reply.send(Err("The approved save is still in progress".into()));
                        return;
                    }
                    self.reviews.remove(ticket);
                    self.current.remove(&owner);
                    if review.save.cancel() {
                        review
                            .reply
                            .clone()
                            .send(Err("Save cancelled; your local draft is unchanged".into()));
                    }
                    host.close_sheet();
                    reply.send(Ok(json!({"cancelled":true})));
                    return;
                }
            }
            _ => {
                let family = self.family;
                std::thread::spawn(move || {
                    let result = (|| {
                        let connection = field(&call.args, "connection")?;
                        with_api(&call.host_dir, |api| match (family, call.method()) {
                            ("github", "repositories") => api.github_repositories(
                                &call.app_id,
                                connection,
                                call.args["page"].as_u64().unwrap_or(1).min(1000) as u32,
                            ),
                            ("github", "read") => api.github_read(
                                &call.app_id,
                                connection,
                                field(&call.args, "owner")?,
                                field(&call.args, "repo")?,
                                field(&call.args, "branch")?,
                                field(&call.args, "path")?,
                            ),
                            ("github", "files") => api.github_files(
                                &call.app_id,
                                connection,
                                field(&call.args, "owner")?,
                                field(&call.args, "repo")?,
                                field(&call.args, "branch")?,
                                call.args["path"].as_str().unwrap_or(""),
                            ),
                            ("gcalendar", "calendars") => api.calendars(
                                &call.app_id,
                                connection,
                                call.args["page_token"].as_str(),
                            ),
                            ("gcalendar", "cached" | "refresh") => {
                                api.connections.authorized(
                                    &call.app_id,
                                    connection,
                                    crate::Provider::Google,
                                    crate::api::CALENDAR_SCOPE,
                                )?;
                                let calendar = field(&call.args, "calendar")?;
                                if call.method() == "cached" {
                                    crate::calendar_cache::cached(
                                        &call.host_dir,
                                        &call.app_id,
                                        connection,
                                        calendar,
                                    )
                                } else {
                                    let deadline = Instant::now() + Duration::from_secs(35);
                                    crate::calendar_cache::refresh(
                                        &call.host_dir,
                                        &call.app_id,
                                        connection,
                                        calendar,
                                        api.now,
                                        |sync, page| {
                                            if Instant::now() >= deadline {
                                                return Err("Calendar refresh took too long; the previous snapshot is unchanged".into());
                                            }
                                            let result = api.calendar_sync(
                                                &call.app_id,
                                                connection,
                                                calendar,
                                                sync,
                                                page,
                                            )?;
                                            if Instant::now() >= deadline {
                                                return Err("Calendar refresh took too long; the previous snapshot is unchanged".into());
                                            }
                                            Ok(result)
                                        },
                                    )
                                }
                            }
                            ("gcalendar", "sync") => api.calendar_sync(
                                &call.app_id,
                                connection,
                                field(&call.args, "calendar")?,
                                call.args["sync_token"].as_str(),
                                call.args["page_token"].as_str(),
                            ),
                            ("gcalendar", "get") => api.calendar_get(
                                &call.app_id,
                                connection,
                                field(&call.args, "calendar")?,
                                field(&call.args, "event_id")?,
                            ),
                            ("gmail", "messages") => api.gmail_messages(
                                &call.app_id,
                                connection,
                                call.args["page_token"].as_str(),
                            ),
                            ("gmail", "message") => api.gmail_message(
                                &call.app_id,
                                connection,
                                field(&call.args, "message_id")?,
                            ),
                            _ => Err("Unknown connector operation".into()),
                        })
                    })();
                    reply.send(result);
                });
            }
        }
    }
}

fn with_api(
    root: &std::path::Path,
    run: impl FnOnce(&mut Api<'_>) -> Result<Value, String>,
) -> Result<Value, String> {
    let _guard = STORE_LOCK.lock().unwrap();
    let mut store = connections(root)?;
    let config = clients(root)?;
    let client = config.google.as_ref().map(|client| ClientRegistration {
        client_id: client.client_id.clone(),
    });
    let transport = provider_transport(root)?;
    run(&mut Api {
        connections: &mut store,
        transport: transport.as_ref(),
        google_client: client.as_ref(),
        google_client_secret: config
            .google
            .as_ref()
            .and_then(|c| c.client_secret.as_deref()),
        now: unix_now(),
    })
}
fn review_snapshot(review: &Review) -> Value {
    let (title, details, body) = match &review.change {
        Change::Github(file) => (
            "Save Markdown to GitHub",
            format!(
                "{}/{} · {}\n{}\nCommit: {}",
                file.owner, file.repo, file.branch, file.path, file.message
            ),
            file.content.clone(),
        ),
        Change::Calendar {
            calendar,
            event,
            existing,
            ..
        } => (
            if existing.is_some() {
                "Update Google Calendar event"
            } else {
                "Create Google Calendar event"
            },
            format!(
                "Calendar: {}\n{}\nStart: {} ({})\nEnd: {} ({})",
                calendar,
                event.summary,
                event
                    .start
                    .date_time
                    .as_ref()
                    .or(event.start.date.as_ref())
                    .map(String::as_str)
                    .unwrap_or(""),
                event.start.time_zone.as_deref().unwrap_or("all-day date"),
                event
                    .end
                    .date_time
                    .as_ref()
                    .or(event.end.date.as_ref())
                    .map(String::as_str)
                    .unwrap_or(""),
                event.end.time_zone.as_deref().unwrap_or("all-day date")
            ),
            format!("{}\n{}", event.location, event.description),
        ),
    };
    json!({"app":review.app,"connection":review.connection,"account":review.account,"title":title,"details":details,"body":body})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[derive(Default)]
    struct Sheet {
        closed: usize,
    }
    impl ServiceHost for Sheet {
        fn open_sheet(&mut self, _: String) {}
        fn close_sheet(&mut self) {
            self.closed += 1;
        }
    }
    fn fixture() -> (ReviewRequest, usize) {
        static NEXT: AtomicUsize = AtomicUsize::new(992000);
        let heap = NEXT.fetch_add(1, Ordering::SeqCst);
        let family: &'static str = Box::leak(format!("connector_capture_{heap}").into_boxed_str());
        struct Capture {
            family: &'static str,
            reply: Arc<Mutex<Option<Replier>>>,
        }
        impl HostService for Capture {
            fn family(&self) -> &'static str {
                self.family
            }
            fn call(&mut self, _: ServiceCall, reply: Replier, _: &mut dyn ServiceHost) {
                *self.reply.lock().unwrap() = Some(reply);
            }
        }
        let captured = Arc::new(Mutex::new(None));
        services::register_host_service(Box::new(Capture {
            family,
            reply: captured.clone(),
        }));
        services::dispatch(
            ServiceCall {
                app_id: "org.example.notes".into(),
                service: format!("{family}.hold"),
                args: Value::Null,
                from_sheet: false,
                may_prompt: true,
                host_dir: "synthetic-root".into(),
            },
            heap,
            1,
            &mut Sheet::default(),
        );
        let reply = captured.lock().unwrap().take().unwrap();
        let review = Arc::new(Review {
            app: "org.example.notes".into(),
            root: "synthetic-root".into(),
            connection: "synthetic-connection".into(),
            account: "Fictional account".into(),
            change: Change::Github(GithubFile {
                owner: "fictional".into(),
                repo: "notes".into(),
                branch: "main".into(),
                path: "note.md".into(),
                content: "Exact reviewed UTF-8 — 保留\n".into(),
                message: "Save note".into(),
                sha: Some("old-sha".into()),
            }),
            reply,
            deadline: Instant::now() + Duration::from_secs(60),
            save: Arc::new(SaveState::default()),
        });
        (
            ReviewRequest {
                family: "github",
                ticket: Uuid::new_v4().to_string(),
                review,
            },
            heap,
        )
    }

    #[test]
    fn native_provenance_is_required_on_both_edges_and_approval_is_one_use() {
        let (mut request, heap) = fixture();
        for (down, up) in [(false, false), (true, false), (false, true)] {
            assert!(request.approve(down, up).is_err());
            assert!(
                !request.review.save.started(),
                "automation must not consume physical approval"
            );
        }
        assert_eq!(request.snapshot()["body"], "Exact reviewed UTF-8 — 保留\n");
        assert!(request.claim_approval(true, true).is_ok());
        assert!(request.claim_approval(true, true).is_err());
        *request.review.save.result.lock().unwrap() = Some(Err("Synthetic conflict".into()));
        assert!(
            request.claim_approval(true, true).is_err(),
            "failed writes require a new review"
        );
        services::cancel_heap(heap);
    }

    #[test]
    fn dropped_expired_or_cancelled_native_review_cannot_start_a_write() {
        let (request, heap) = fixture();
        let state = request.review.save.clone();
        drop(request);
        assert!(!state.start());
        assert!(
            services::take_replies_for(&[heap])[0].2.is_err(),
            "drop settles the original app promise"
        );
        let (mut expired, heap) = fixture();
        Arc::get_mut(&mut expired.review).unwrap().deadline =
            Instant::now() - Duration::from_secs(1);
        assert!(expired.claim_approval(true, true).is_err());
        assert!(!expired.review.save.start());
        services::cancel_heap(heap);
    }

    #[test]
    fn script_save_never_approves_and_only_the_scoped_current_sheet_can_close() {
        let (request, original_heap) = fixture();
        let family: &'static str =
            Box::leak(format!("connector_review_{original_heap}").into_boxed_str());
        let connector = Arc::new(Mutex::new(Connector {
            family,
            review_hook: None,
            reviews: HashMap::from([(request.ticket.clone(), request.review.clone())]),
            current: HashMap::from([(
                (request.review.app.clone(), request.review.root.clone()),
                request.ticket.clone(),
            )]),
        }));
        struct Shared {
            family: &'static str,
            connector: Arc<Mutex<Connector>>,
        }
        impl HostService for Shared {
            fn family(&self) -> &'static str {
                self.family
            }
            fn call(&mut self, call: ServiceCall, reply: Replier, host: &mut dyn ServiceHost) {
                self.connector.lock().unwrap().call(call, reply, host)
            }
        }
        services::register_host_service(Box::new(Shared {
            family,
            connector: connector.clone(),
        }));
        let mut sheet = Sheet::default();
        let mut id = 10;
        let heap = original_heap + 100_000;
        let mut dispatch =
            |method: &str, from_sheet: bool, app: &str, ticket: &str, sheet: &mut Sheet| {
                id += 1;
                services::dispatch(
                    ServiceCall {
                        app_id: app.into(),
                        service: format!("{family}.{method}"),
                        args: json!({"ticket":ticket}),
                        from_sheet,
                        may_prompt: true,
                        host_dir: "synthetic-root".into(),
                    },
                    heap,
                    id,
                    sheet,
                );
                services::take_replies_for(&[heap])
            };
        for from_sheet in [false, true] {
            let replies = dispatch(
                "sheet.save",
                from_sheet,
                &request.review.app,
                &request.ticket,
                &mut sheet,
            );
            assert!(replies[0].2.is_err());
            assert!(!request.review.save.started());
        }
        for (from_sheet, app, ticket) in [
            (false, request.review.app.as_str(), request.ticket.as_str()),
            (true, "org.other.app", request.ticket.as_str()),
            (true, request.review.app.as_str(), "stale-ticket"),
        ] {
            let replies = dispatch("sheet.cancel", from_sheet, app, ticket, &mut sheet);
            assert!(replies[0].2.is_err());
            assert_eq!(sheet.closed, 0);
        }
        assert!(request.claim_approval(true, true).is_ok());
        *request.review.save.result.lock().unwrap() = Some(Err("Synthetic conflict".into()));
        let replies = dispatch(
            "sheet.status",
            true,
            &request.review.app,
            &request.ticket,
            &mut sheet,
        );
        assert!(replies[0]
            .2
            .as_ref()
            .unwrap()
            .contains("\"phase\":\"error\""));
        assert_eq!(sheet.closed, 0, "failed save remains visible");
        dispatch(
            "sheet.cancel",
            true,
            &request.review.app,
            &request.ticket,
            &mut sheet,
        );
        assert_eq!(sheet.closed, 1);
        services::cancel_heap(heap);
        services::cancel_heap(original_heap);
    }

    #[test]
    fn approval_and_cancellation_race_has_exactly_one_winner() {
        let state = Arc::new(SaveState::default());
        let tasks: Vec<_> = (0..16)
            .map(|i| {
                let state = state.clone();
                std::thread::spawn(move || {
                    if i % 2 == 0 {
                        state.start()
                    } else {
                        state.cancel()
                    }
                })
            })
            .collect();
        assert_eq!(
            tasks
                .into_iter()
                .filter_map(|t| t.join().ok())
                .filter(|won| *won)
                .count(),
            1
        );
    }
}
