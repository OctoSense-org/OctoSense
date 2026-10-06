//! Scoped provider operations and review of immutable remote-write snapshots.
use crate::{
    api::{Api, CalendarEvent, GithubFile},
    host::{clients, connections, unix_now, STORE_LOCK},
    providers::ClientRegistration,
    transport::HttpsTransport,
};
use octosense_appstore::services::{self, HostService, Replier, ServiceCall, ServiceHost};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
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
}
#[derive(Default)]
struct SaveState {
    started: AtomicBool,
    result: Mutex<Option<Result<Value, String>>>,
}
impl SaveState {
    fn start(&self) -> bool {
        self.started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    fn started(&self) -> bool {
        self.started.load(Ordering::Acquire)
    }
    fn result(&self) -> Option<Result<Value, String>> {
        self.result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}
struct Connector {
    family: &'static str,
    reviews: HashMap<String, Arc<Review>>,
    // This small owner record outlives an expired private snapshot so its
    // host sheet retains a scoped Back action without retaining the draft.
    current: HashMap<(String, PathBuf), String>,
}

pub fn register() {
    for family in ["github", "gcalendar"] {
        services::register_host_service(Box::new(Connector {
            family,
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
                if !review.save.started() {
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
                    Ok((connection, change))
                })();
                let (connection, change) = match parsed {
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
                        if !old.save.started() {
                            old.reply
                                .clone()
                                .send(Err("A newer draft replaced this review".into()));
                        }
                    }
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
                });
                host.open_sheet(review_sheet(self.family, &ticket, &review));
                self.current
                    .insert((review.app.clone(), review.root.clone()), ticket.clone());
                self.reviews.insert(ticket, review);
            }
            "sheet.save" | "sheet.cancel" | "sheet.status" => {
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
                    if !review.save.started() {
                        review
                            .reply
                            .clone()
                            .send(Err("Save cancelled; your local draft is unchanged".into()));
                    }
                    host.close_sheet();
                    reply.send(Ok(json!({"cancelled":true})));
                    return;
                }
                if Instant::now() >= review.deadline {
                    let message = "Review expired; check the current draft again";
                    review.reply.clone().send(Err(message.into()));
                    reply.send(Err(message.into()));
                    return;
                }
                if !review.save.start() {
                    reply.send(Err(
                        "This review was already submitted; return to editing for a new review"
                            .into(),
                    ));
                    return;
                }
                reply.send(Ok(json!({"started":true})));
                std::thread::spawn(move || {
                    let result = with_api(&review.root, |api| {
                        if !api
                            .connections
                            .active(&review.app)
                            .is_some_and(|c| c.handle == review.connection)
                        {
                            return Err("The selected account changed; review again under the original account".into());
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
                                existing.as_ref().map(|(a, b)| (a.as_str(), b.as_str())),
                                create_id,
                            ),
                        }
                    });
                    *review.save.result.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(result.clone());
                    review.reply.clone().send(result);
                });
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
    let transport = HttpsTransport::new()?;
    run(&mut Api {
        connections: &mut store,
        transport: &transport,
        google_client: client.as_ref(),
        google_client_secret: config
            .google
            .as_ref()
            .and_then(|c| c.client_secret.as_deref()),
        now: unix_now(),
    })
}
fn review_sheet(family: &str, ticket: &str, review: &Review) -> String {
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
                "{}\n{}\n{} → {}",
                calendar,
                event.summary,
                event
                    .start
                    .date_time
                    .as_ref()
                    .or(event.start.date.as_ref())
                    .map(String::as_str)
                    .unwrap_or(""),
                event
                    .end
                    .date_time
                    .as_ref()
                    .or(event.end.date.as_ref())
                    .map(String::as_str)
                    .unwrap_or("")
            ),
            format!("{}\n{}", event.location, event.description),
        ),
    };
    let title = json!(title);
    let details = json!(details);
    let body = json!(body);
    let ticket = json!(ticket);
    let save = json!(format!("{family}.sheet.save"));
    let cancel = json!(format!("{family}.sheet.cancel"));
    let status = json!(format!("{family}.sheet.status"));
    format!(
        r#"
let ticket = {ticket}
let sending = false
let attempted = false
fn poll() {{
    if !sending {{ return }}
    host.request({status}, {{ticket: ticket}}, fn(r) {{
        if r.is_ok {{
            if r.data.phase == "saved" {{ sending = false }}
            if r.data.phase == "error" {{
                sending = false
                ui.connector_review_status.set_text(r.data.message)
            }}
        }} else {{
            sending = false
            ui.connector_review_status.set_text(r.error)
        }}
        if sending {{ start_timeout(0.5, || poll()) }}
    }})
}}
fn save() {{
    if sending || attempted {{ return }}
    attempted = true
    sending = true
    ui.connector_review_status.set_text("Saving the reviewed content…")
    host.request({save}, {{ticket: ticket}}, fn(r) {{
        if r.is_ok {{ poll() }} else {{ sending = false ui.connector_review_status.set_text(r.error) }}
    }})
}}
SolidView {{width: Fill height: Fill flow: Down padding: 16 spacing: 12 draw_bg.color: #fff
    Label {{width: Fill text: {title} draw_text.color: #222 draw_text.text_style.font_size: 20}}
    ScrollYView {{width: Fill height: Fill flow: Down spacing: 12
        Label {{width: Fill text: {details} draw_text.color: #444}}
        Label {{width: Fill text: {body} draw_text.color: #222}}
    }}
    connector_review_status := Label {{width: Fill text: "Review this exact version before saving." draw_text.color: #444}}
    View {{width: Fill height: Fit spacing: 12
        ButtonFlat {{width: Fill height: 48 text: "Back to editing" on_click: || {{if !sending {{host.request({cancel}, {{ticket: ticket}}, fn(r) {{}})}}}}}}
        Button {{width: Fill height: 48 text: "Approve & Save" on_click: || save()}}
    }}
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_approval_starts_once_even_after_a_failed_save() {
        let state = Arc::new(SaveState::default());
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let state = state.clone();
                std::thread::spawn(move || state.start())
            })
            .collect();
        let winners = workers
            .into_iter()
            .filter_map(|worker| worker.join().ok())
            .filter(|won| *won)
            .count();
        assert_eq!(winners, 1);
        *state.result.lock().unwrap() = Some(Err("Synthetic conflict".into()));
        assert!(!state.start(), "retry requires a new reviewed snapshot");
    }

    #[test]
    fn only_current_scoped_review_status_closes_and_errors_remain_visible() {
        const FAMILY: &str = "connector_review_test";
        const HEAP: usize = 981004;
        struct Shared(Arc<Mutex<Connector>>);
        impl HostService for Shared {
            fn family(&self) -> &'static str {
                FAMILY
            }
            fn call(&mut self, call: ServiceCall, reply: Replier, host: &mut dyn ServiceHost) {
                self.0.lock().unwrap().call(call, reply, host)
            }
        }
        #[derive(Default)]
        struct Sheet {
            opened: usize,
            closed: usize,
        }
        impl ServiceHost for Sheet {
            fn open_sheet(&mut self, _: String) {
                self.opened += 1;
            }
            fn close_sheet(&mut self) {
                self.closed += 1;
            }
        }
        let connector = Arc::new(Mutex::new(Connector {
            family: FAMILY,
            reviews: HashMap::new(),
            current: HashMap::new(),
        }));
        services::register_host_service(Box::new(Shared(connector.clone())));
        let mut sheet = Sheet::default();
        let mut id = 0;
        let mut call = |method: &str, args: Value, app: &str, root: &str, sheet: &mut Sheet| {
            id += 1;
            services::dispatch(
                ServiceCall {
                    app_id: app.into(),
                    service: format!("{FAMILY}.{method}"),
                    args,
                    from_sheet: method.starts_with("sheet."),
                    may_prompt: true,
                    host_dir: PathBuf::from(root),
                },
                HEAP,
                id,
                sheet,
            );
            services::take_replies_for(&[HEAP])
        };
        let draft = json!({"connection":"synthetic", "calendar":"primary", "event": {
            "summary":"Synthetic review", "description":"", "location":"",
            "start":{"date":"2026-10-06"}, "end":{"date":"2026-10-07"}
        }});
        call(
            "review_save",
            draft.clone(),
            "org.example.calendar",
            "test-root",
            &mut sheet,
        );
        let (old_ticket, old) = connector
            .lock()
            .unwrap()
            .reviews
            .iter()
            .map(|(id, review)| (id.clone(), review.clone()))
            .next()
            .unwrap();
        assert!(old.save.start());
        call(
            "review_save",
            draft.clone(),
            "org.example.calendar",
            "test-root",
            &mut sheet,
        );
        let (ticket, review) = connector
            .lock()
            .unwrap()
            .reviews
            .iter()
            .map(|(id, review)| (id.clone(), review.clone()))
            .next()
            .unwrap();
        *old.save.result.lock().unwrap() = Some(Ok(json!({"saved":true})));
        old.reply.clone().send(Ok(json!({"saved":true})));
        assert_eq!(
            sheet.closed, 0,
            "an old worker cannot close the replacement"
        );
        let replies = call(
            "sheet.status",
            json!({"ticket":old_ticket}),
            "org.example.calendar",
            "test-root",
            &mut sheet,
        );
        assert!(replies.iter().any(|(_, _, result)| result.is_err()));
        for (app, root) in [
            ("org.other.calendar", "test-root"),
            ("org.example.calendar", "other-root"),
        ] {
            let replies = call(
                "sheet.status",
                json!({"ticket":ticket}),
                app,
                root,
                &mut sheet,
            );
            assert!(replies[0].2.is_err());
        }
        assert!(review.save.start());
        *review.save.result.lock().unwrap() = Some(Err("Synthetic conflict; review again".into()));
        review
            .reply
            .clone()
            .send(Err("Synthetic conflict; review again".into()));
        let replies = call(
            "sheet.status",
            json!({"ticket":ticket}),
            "org.example.calendar",
            "test-root",
            &mut sheet,
        );
        assert!(replies.iter().any(|(_, _, result)| result
            .as_ref()
            .is_ok_and(|v| v.contains("\"phase\":\"error\""))));
        assert_eq!(sheet.closed, 0, "failure remains visible");
        call(
            "sheet.cancel",
            json!({"ticket":ticket}),
            "org.example.calendar",
            "test-root",
            &mut sheet,
        );
        assert_eq!(sheet.closed, 1);
        call(
            "review_save",
            draft.clone(),
            "org.example.calendar",
            "test-root",
            &mut sheet,
        );
        let ticket = connector
            .lock()
            .unwrap()
            .reviews
            .keys()
            .next()
            .unwrap()
            .clone();
        {
            let mut connector = connector.lock().unwrap();
            Arc::get_mut(connector.reviews.get_mut(&ticket).unwrap())
                .unwrap()
                .deadline = Instant::now() - Duration::from_secs(1);
        }
        // A different app prunes the private snapshot before the person
        // returns. Only the original sheet can still dismiss its owner record.
        call(
            "sheet.status",
            json!({"ticket":"unknown"}),
            "org.other.calendar",
            "test-root",
            &mut sheet,
        );
        let replies = call(
            "sheet.save",
            json!({"ticket":ticket}),
            "org.example.calendar",
            "test-root",
            &mut sheet,
        );
        assert!(replies.iter().all(|(_, _, result)| result.is_err()));
        call(
            "sheet.cancel",
            json!({"ticket":ticket}),
            "org.example.calendar",
            "test-root",
            &mut sheet,
        );
        assert_eq!(
            sheet.closed, 2,
            "expired review still has a working Back action"
        );
        call(
            "review_save",
            draft,
            "org.example.calendar",
            "test-root",
            &mut sheet,
        );
        let (ticket, review) = connector
            .lock()
            .unwrap()
            .reviews
            .iter()
            .map(|(id, review)| (id.clone(), review.clone()))
            .next()
            .unwrap();
        assert!(review.save.start());
        *review.save.result.lock().unwrap() = Some(Ok(json!({"saved":true})));
        review.reply.clone().send(Ok(json!({"saved":true})));
        call(
            "sheet.status",
            json!({"ticket":ticket}),
            "org.example.calendar",
            "test-root",
            &mut sheet,
        );
        assert_eq!(sheet.closed, 3);
        assert!(connector.lock().unwrap().reviews.is_empty());
        services::cancel_heap(HEAP);
    }
}
