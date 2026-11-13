//! Scoped provider operations and review of immutable remote-write snapshots.
use crate::{
    api::{CalendarEvent, EventTime, GithubFile},
    host::{connections, operation_lock, with_provider_api},
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
    Backend(crate::host::backend_host::PreparedRequest),
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
                let result = match &review.change {
                    Change::Backend(request) => request.execute(),
                    _ => with_provider_api(&review.root, &review.app, |api| {
                        execute_review(&review, api)
                    }),
                };
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
static REVIEW_HOOK: Mutex<Option<ReviewHook>> = Mutex::new(None);
pub type ReviewHook = Arc<dyn Fn(ReviewRequest) -> Result<String, String> + Send + Sync>;
pub fn register_with_review_hook(
    hook: impl Fn(ReviewRequest) -> Result<String, String> + Send + Sync + 'static,
) {
    register_review(Some(Arc::new(hook)));
}
fn register_review(review_hook: Option<ReviewHook>) {
    *REVIEW_HOOK.lock().unwrap_or_else(|e| e.into_inner()) = review_hook.clone();
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

impl Connector {
    fn open_review(
        &mut self,
        call: ServiceCall,
        reply: Replier,
        host: &mut dyn ServiceHost,
        connection: String,
        change: Change,
        account: String,
    ) {
        let Some(hook) = self.review_hook.clone() else {
            reply.send(Err(
                "Native trusted save review is unavailable on this host".into(),
            ));
            return;
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
}

/// Backend writes share the exact native-only review capability used for
/// GitHub/Calendar. No script method can claim physical approval.
pub(crate) struct BackendReviews(Connector);
impl Default for BackendReviews {
    fn default() -> Self {
        Self(Connector {
            family: "auth.backend",
            review_hook: None,
            reviews: HashMap::new(),
            current: HashMap::new(),
        })
    }
}
impl BackendReviews {
    pub(crate) fn call(
        &mut self,
        mut call: ServiceCall,
        reply: Replier,
        host: &mut dyn ServiceHost,
        scope_check: crate::host::ScopeCheck,
    ) {
        self.0.review_hook = REVIEW_HOOK
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if call.method() != "backend.request" {
            call.service = call.service.replacen("auth.backend.", "auth.", 1);
            self.0.call(call, reply, host);
            return;
        }
        self.0.reviews.retain(|_, review| {
            if Instant::now() < review.deadline {
                return true;
            }
            if review.save.cancel() {
                review
                    .reply
                    .clone()
                    .send(Err("Review expired; check the current draft again".into()));
            }
            review.save.started() && review.save.result().is_none()
        });
        let prepared = match crate::host::backend_host::prepare_request(&call, scope_check) {
            Ok(request) => request,
            Err(error) => {
                reply.send(Err(error));
                return;
            }
        };
        if !prepared.mutates() {
            std::thread::spawn(move || {
                reply.send(prepared.execute());
            });
            return;
        }
        if !call.may_prompt {
            reply.send(Err("Open the app to review this backend change".into()));
            return;
        }
        self.0.open_review(
            call,
            reply,
            host,
            prepared.request.connection.clone(),
            Change::Backend(prepared.clone()),
            prepared.account.clone(),
        );
    }
}

impl HostService for Connector {
    fn api_methods(&self) -> Vec<services::HostApiMethod> {
        crate::host_catalog::connector(self.family, self.review_hook.is_some())
    }
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
                if self.review_hook.is_none() {
                    reply.send(Err("Native trusted save review is unavailable on this host; your local draft is unchanged".into()));
                    return;
                }
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
                    let operation = operation_lock(&call.host_dir, &call.app_id);
                    let _guard = operation
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
                self.open_review(call, reply, host, connection, change, account);
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
                        with_provider_api(&call.host_dir, &call.app_id, |api| {
                            match (family, call.method()) {
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
                                            |window, page| {
                                                if Instant::now() >= deadline {
                                                    return Err("Calendar refresh took too long; the previous snapshot is unchanged".into());
                                                }
                                                let result = api.calendar_window(
                                                    &call.app_id,
                                                    connection,
                                                    calendar,
                                                    window,
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
                                ("gcalendar", "sync") => Err("Use gcalendar.refresh for the bounded agenda; raw history synchronization is not exposed".into()),
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
                            }
                        })
                    })();
                    reply.send(result);
                });
            }
        }
    }
}

fn execute_review(review: &Review, api: &mut crate::api::Api<'_>) -> Result<Value, String> {
    if !api
        .connections
        .active(&review.app)
        .is_some_and(|c| c.handle == review.connection)
    {
        return Err("The selected account changed; review again under the original account".into());
    }
    match &review.change {
        Change::Backend(_) => Err("Backend writes require their scoped executor".into()),
        Change::Github(file) => api.github_save(&review.app, &review.connection, file),
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
}

fn review_snapshot(review: &Review) -> Value {
    change_snapshot(&review.app, &review.connection, &review.account, &review.change)
}

/// What a review of a change says. Nothing here can approve it.
fn change_snapshot(app: &str, connection: &str, account: &str, change: &Change) -> Value {
    let (title, details, body) = match change {
        Change::Backend(request) => (
            "Submit backend change",
            format!(
                "{}\n{} {}\nOperation: {}\nAccount: {}",
                request.origin,
                request.method,
                request.path,
                request.request.operation,
                request.account
            ),
            serde_json::to_string_pretty(
                &serde_json::json!({"query":request.request.query,"body":request.request.body}),
            )
            .unwrap_or_default(),
        ),
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
    json!({"app":app,"connection":connection,"account":account,"title":title,"details":details,"body":body,"plain":plain_review(change)})
}

/// What a review of this request would say, with nothing behind it: no store is
/// opened, no account is looked up, and these words are all it is. A hidden test
/// shell's vault is a file, so it cannot reach the OAuth store; its sample sheets
/// (`--test-action sample-reviews`, `connected_review.rs`) are drawn from this.
pub fn sample_snapshot(family: &str, app: &str, args: &Value) -> Result<Value, String> {
    const ACCOUNT: &str = "Sample account";
    if let Some(backend) = args.get("backend") {
        let text = |name: &str| backend[name].as_str().unwrap_or("").to_string();
        let data = json!({"query": backend["query"], "body": backend["body"]});
        let body = serde_json::to_string_pretty(&data).unwrap_or_default();
        let plain = backend_plain(&text("origin"), &text("method"), &text("path"), &text("operation"), data);
        return Ok(json!({"app":app,"connection":"","account":ACCOUNT,"title":"Submit backend change","details":"","body":body,"plain":plain}));
    }
    let change = match family {
        "github" => {
            let file: GithubFile = serde_json::from_value(args["file"].clone()).map_err(|_| "Invalid Markdown save request")?;
            file.validate()?;
            Change::Github(file)
        }
        "gcalendar" => {
            let event: CalendarEvent = serde_json::from_value(args["event"].clone()).map_err(|_| "Invalid event draft")?;
            event.validate()?;
            Change::Calendar {
                calendar: field(args, "calendar")?.to_string(),
                event,
                existing: args["event_id"].as_str().map(|id| (id.to_string(), String::new())),
                create_id: String::new(),
            }
        }
        _ => return Err("No sample of this review".into()),
    };
    Ok(change_snapshot(app, "", ACCOUNT, &change))
}

/// The same change as labelled rows, for a sheet that shows them: what it does,
/// each exact value under its own label, and the body when it differs. Built
/// from the fields, so no value can pass for another row.
fn plain_review(change: &Change) -> Value {
    match change {
        Change::Backend(request) => backend_plain(
            &request.origin,
            &request.method,
            &request.path,
            &request.request.operation,
            json!({"query": request.request.query, "body": request.request.body}),
        ),
        Change::Github(file) => json!({
            "wants": "save this file to GitHub",
            "facts": [
                ["Repository", format!("{}/{}", file.owner, file.repo)],
                ["Branch", file.branch],
                ["File", file.path],
                ["Commit", file.message],
            ],
        }),
        Change::Calendar { calendar, event, existing, .. } => {
            let mut facts = vec![
                json!(["Calendar", calendar]),
                json!(["Event", event.summary]),
            ];
            facts.extend(plain_when(&event.start, &event.end).into_iter().map(|(k, v)| json!([k, v])));
            facts.push(json!(["Location", event.location]));
            facts.push(json!(["Notes", event.description]));
            json!({
                "title": if existing.is_some() { "Update Google Calendar Event" } else { "Create Google Calendar Event" },
                "wants": if existing.is_some() { "change this event in your Google Calendar" } else { "add this event to your Google Calendar" },
                "facts": facts,
                "body": "",
            })
        }
    }
}

fn backend_plain(origin: &str, method: &str, path: &str, operation: &str, data: Value) -> Value {
    // What the request does to the address it names, and the whole address.
    let does = match method {
        "POST" => "Sends to",
        "PUT" => "Replaces",
        "PATCH" => "Changes",
        "DELETE" => "Deletes",
        _ => "Requests",
    };
    // It undoes something: the sheet says so, and the default is to back out.
    let careful = method == "DELETE";
    json!({
        "title": if careful { "Delete this?" } else { "Send This Change" },
        "wants": if careful { "delete this from its website" } else { "send this change to its website" },
        "approve": if careful { "Approve & Delete" } else { "Approve & Send" },
        "note": if careful { "Review the account and the address before deleting." } else { "Review the account, destination and exact content before sending." },
        "facts": [
            [does, format!("{origin}{path}")],
            ["Action", operation_words(operation)],
        ],
        "data": data,
        "careful": careful,
    })
}

/// An app's name for an operation as a person says it, every word of it there: what
/// it does, then what to. `notes.add` reads "Add notes".
fn operation_words(operation: &str) -> String {
    let mut parts: Vec<String> = operation.split('.').map(|part| part.replace(['_', '-'], " ")).collect();
    let does = parts.pop().unwrap_or_default();
    let words = if parts.is_empty() { does } else { format!("{does} {}", parts.join(" ").to_lowercase()) };
    let mut letters = words.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => String::new(),
    }
}

/// A UTC offset as a person says it: "UTC-8", "UTC+5:30".
fn offset_words(seconds: i32) -> String {
    let (hours, minutes) = (seconds.abs() / 3600, seconds.abs() % 3600 / 60);
    let sign = if seconds < 0 { '-' } else { '+' };
    match minutes {
        0 => format!("UTC{sign}{hours}"),
        _ => format!("UTC{sign}{hours}:{minutes:02}"),
    }
}

/// An event's times as a person reads them: "Nov 2, 2026 at 3:45 PM PST", in the
/// event's time zone. An instant is shown there whatever offset it came with: it is
/// the same instant. With no zone its own offset stays in sight. An all-day event's
/// end date is exclusive (Google's), so its last day is the day before. A value that
/// does not parse is shown as it is.
fn plain_when(start: &EventTime, end: &EventTime) -> Vec<(&'static str, String)> {
    use chrono::NaiveDate;
    if let (Some(first), Some(after)) = (start.date.as_deref(), end.date.as_deref()) {
        let day = |d: NaiveDate| d.format("%b %-d, %Y").to_string();
        let parsed = NaiveDate::parse_from_str(first, "%Y-%m-%d").ok().zip(NaiveDate::parse_from_str(after, "%Y-%m-%d").ok());
        return vec![(
            "When",
            match parsed {
                Some((first, after)) if after > first => match after.pred_opt().filter(|last| *last > first) {
                    Some(last) => format!("All day, {} through {}", day(first), day(last)),
                    None => format!("All day, {}", day(first)),
                },
                _ => format!("All day, {first} until {after}"),
            },
        )];
    }
    let wall = |t: chrono::NaiveDateTime| {
        use chrono::Timelike;
        t.format(if t.second() == 0 { "%b %-d, %Y at %-I:%M %p" } else { "%b %-d, %Y at %-I:%M:%S %p" }).to_string()
    };
    let at = |time: &EventTime| {
        use chrono::TimeZone;
        let raw = time.date_time.as_deref().or(time.date.as_deref()).unwrap_or("");
        let named = time.time_zone.as_deref().filter(|zone| !zone.is_empty());
        let zone = named.and_then(|zone| zone.parse::<chrono_tz::Tz>().ok());
        // A zone nothing here knows is still said, in its own words.
        let unknown = named.filter(|_| zone.is_none()).map(|zone| format!(" ({})", zone.replace('_', " "))).unwrap_or_default();
        if let Ok(t) = chrono::DateTime::parse_from_rfc3339(raw) {
            return match zone {
                Some(zone) => {
                    let t = t.with_timezone(&zone);
                    format!("{} {}", wall(t.naive_local()), t.format("%Z"))
                }
                None => format!("{} {}{unknown}", wall(t.naive_local()), offset_words(t.offset().local_minus_utc())),
            };
        }
        match chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S") {
            Ok(t) => match zone.and_then(|zone| zone.from_local_datetime(&t).earliest()) {
                Some(local) => format!("{} {}", wall(t), local.format("%Z")),
                None => format!("{}{unknown}", wall(t)),
            },
            Err(_) => format!("{raw}{unknown}"),
        }
    };
    vec![("Starts", at(start)), ("Ends", at(end))]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// A test shell's sample reads as the real review does, and is only words.
    #[test]
    fn a_sample_review_reads_like_the_real_one() {
        let file = json!({"owner":"fictional","repo":"notes","branch":"main","path":"note.md","content":"Exact","message":"Save note","sha":null});
        let parsed: GithubFile = serde_json::from_value(file.clone()).unwrap();
        let sample = sample_snapshot("github", "os.writer", &json!({"file": file})).unwrap();
        let real = change_snapshot("os.writer", "", "Sample account", &Change::Github(parsed));
        assert_eq!(sample, real);
        assert_eq!(sample["plain"]["wants"], "save this file to GitHub");
        let backend = json!({"origin":"https://example.test","method":"POST","path":"/notes","operation":"notes.add","query":{},"body":{"api_key":"x"}});
        let sample = sample_snapshot("github", "os.writer", &json!({"backend": backend})).unwrap();
        assert_eq!(sample["plain"]["facts"][0], json!(["Sends to", "https://example.test/notes"]));
        assert_eq!(sample["plain"]["facts"][1], json!(["Action", "Add notes"]));
        assert_eq!(sample["plain"]["careful"], false);
        assert_eq!(sample["plain"]["approve"], "Approve & Send");
        assert_eq!(operation_words("order_items.bulk-delete"), "Bulk delete order items");
        assert_eq!(operation_words("refresh"), "Refresh");
        // What undoes something is flagged, and only that.
        let removing = json!({"origin":"https://example.test","method":"DELETE","path":"/notes/1","operation":"notes.remove","query":{},"body":{}});
        assert_eq!(sample_snapshot("github", "os.writer", &json!({"backend": removing})).unwrap()["plain"]["careful"], true);
        assert_eq!(sample["plain"]["data"]["body"]["api_key"], "x", "the words hide it when they are drawn");
        assert!(sample_snapshot("mail", "os.writer", &json!({})).is_err());
    }

    #[test]
    fn a_reviews_times_read_as_dates_and_keep_their_zone() {
        let timed = |raw: &str, zone: Option<&str>| EventTime { date: None, date_time: Some(raw.into()), time_zone: zone.map(str::to_string) };
        let day = |raw: &str| EventTime { date: Some(raw.into()), date_time: None, time_zone: None };
        let one = |start: &EventTime, end: &EventTime| plain_when(start, end).into_iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("; ");
        assert_eq!(
            one(&timed("2026-11-02T15:45:00", Some("America/Los_Angeles")), &timed("2026-11-02T16:45:30", Some("America/Los_Angeles"))),
            "Starts: Nov 2, 2026 at 3:45 PM PST; Ends: Nov 2, 2026 at 4:45:30 PM PST"
        );
        // An offset is never dropped: alone it is said, and with a zone the same
        // instant is shown in that zone.
        assert_eq!(
            one(&timed("2026-11-02T15:45:00-08:00", None), &timed("2026-11-02T16:45:00-08:00", Some("Asia/Tokyo"))),
            "Starts: Nov 2, 2026 at 3:45 PM UTC-8; Ends: Nov 3, 2026 at 9:45 AM JST"
        );
        assert_eq!(
            one(&timed("2026-11-02T15:45:00+05:30", Some("Nowhere/Some_Place")), &timed("2026-11-02T16:45:00", Some("Nowhere/Some_Place"))),
            "Starts: Nov 2, 2026 at 3:45 PM UTC+5:30 (Nowhere/Some Place); Ends: Nov 2, 2026 at 4:45 PM (Nowhere/Some Place)"
        );
        // What does not parse is shown as it is.
        assert_eq!(one(&timed("soon", None), &timed("later", None)), "Starts: soon; Ends: later");
        // An all-day event's end date is exclusive.
        assert_eq!(one(&day("2026-11-02"), &day("2026-11-03")), "When: All day, Nov 2, 2026");
        assert_eq!(one(&day("2026-11-02"), &day("2026-11-05")), "When: All day, Nov 2, 2026 through Nov 4, 2026");
        assert_eq!(one(&day("2026-11-02"), &day("2026-11-02")), "When: All day, 2026-11-02 until 2026-11-02");
    }

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

    #[test]
    fn account_switch_before_the_save_worker_causes_no_provider_request() {
        use crate::{
            oauth::Tokens,
            transport::{Request, Response, Transport},
            Connections, CredentialStore,
        };
        #[derive(Default)]
        struct Vault(Mutex<HashMap<String, String>>);
        impl CredentialStore for Vault {
            fn put(&self, k: &str, v: &str) -> Result<(), String> {
                self.0.lock().unwrap().insert(k.into(), v.into());
                Ok(())
            }
            fn get(&self, k: &str) -> Result<String, String> {
                self.0
                    .lock()
                    .unwrap()
                    .get(k)
                    .cloned()
                    .ok_or("missing".into())
            }
            fn remove(&self, k: &str) -> Result<(), String> {
                self.0.lock().unwrap().remove(k);
                Ok(())
            }
        }
        struct NeverSend(AtomicUsize);
        impl Transport for NeverSend {
            fn send(&self, _: Request) -> Result<Response, String> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Err("Unexpected provider call".into())
            }
        }
        let root = std::env::temp_dir().join(format!("review-account-race-{}", Uuid::new_v4()));
        let mut store = Connections::open(&root, Arc::new(Vault::default())).unwrap();
        let tokens = || Tokens {
            access: "synthetic".into(),
            refresh: None,
            expires_at: None,
            scopes: ["public_repo".into()].into_iter().collect(),
        };
        let (mut request, heap) = fixture();
        let first = store
            .connect(
                &request.review.app,
                crate::Provider::Github,
                "first",
                "First",
                tokens(),
            )
            .unwrap();
        Arc::get_mut(&mut request.review).unwrap().connection = first.handle;
        store
            .connect(
                &request.review.app,
                crate::Provider::Github,
                "second",
                "Second",
                tokens(),
            )
            .unwrap();
        request.claim_approval(true, true).unwrap();
        let review = request.review.clone();
        let transport = Arc::new(NeverSend(AtomicUsize::new(0)));
        let worker_transport = transport.clone();
        let result = std::thread::spawn(move || {
            execute_review(
                &review,
                &mut crate::api::Api {
                    connections: &mut store,
                    transport: worker_transport.as_ref(),
                    google_client: None,
                    google_client_secret: None,
                    now: 1,
                },
            )
        })
        .join()
        .unwrap();
        assert!(result.unwrap_err().contains("selected account changed"));
        assert_eq!(transport.0.load(Ordering::SeqCst), 0);
        services::cancel_heap(heap);
        std::fs::remove_dir_all(root).unwrap();
    }
}
