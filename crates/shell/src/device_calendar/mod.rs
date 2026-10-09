//! Public OS calendar access, separate from the bundled `calendar` and Google
//! `gcalendar` services. Scripts can request review; only native input approves.
use makepad_widgets::{makepad_platform::SignalToUI, Cx, Event};
use octosense_appstore::services::{
    self, AgentAccess, HostApiMethod, HostService, Replier, ServiceCall, ServiceHost,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{sync_channel, Receiver, SyncSender},
        Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
#[cfg(target_os = "macos")]
mod macos;
mod model;
mod prompt;
mod store;
use model::{Calendar, Command, ConsentChange, EventData};
const FAMILY: &str = "device_calendar";
const MAX_PENDING: usize = 16;
#[cfg(target_os = "android")]
// 0 not started, 1 probing, 2 available, 3 unsupported after bounded probe.
static ANDROID_ADAPTER: AtomicUsize = AtomicUsize::new(0);
static FOREGROUND: AtomicBool = AtomicBool::new(true);
static NATIVE_WORKERS: AtomicUsize = AtomicUsize::new(0);
struct NativeWorker;
impl Drop for NativeWorker {
    fn drop(&mut self) {
        NATIVE_WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}
const REVIEW_TTL: Duration = Duration::from_secs(300);
type AccountScope = fn(&str) -> Option<String>;
static SCOPE: OnceLock<AccountScope> = OnceLock::new();

fn supported() -> bool {
    #[cfg(target_os = "android")]
    {
        ANDROID_ADAPTER.load(Ordering::Acquire) == 2
    }
    #[cfg(not(target_os = "android"))]
    {
        cfg!(target_os = "macos")
    }
}
fn scope(app: &str) -> Result<String, String> {
    SCOPE
        .get()
        .and_then(|f| f(app))
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .ok_or_else(|| "account_unavailable: No authenticated app account scope".into())
}
fn admitted(call: &ServiceCall) -> bool {
    crate::host_tools::script_apps::admitted_bundle(&call.app_id)
        .and_then(|(root, bundle)| {
            if root.join(".host") != call.host_dir {
                return Err("Wrong host root".into());
            }
            crate::host_tools::script_apps::from_bundle(&bundle)
        })
        .is_ok_and(|bundle| {
            bundle.families.contains(FAMILY)
                && bundle.manifest["requires"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v == "host-api-v1"))
        })
}
#[derive(Clone)]
struct Context {
    call: ServiceCall,
    reply: Replier,
    scope: String,
    revision: u64,
}
impl Context {
    fn identity_valid(&self) -> Result<(), String> {
        if !self.reply.is_pending() {
            return Err("cancelled: Originating request closed or expired".into());
        }
        if !admitted(&self.call) || scope(&self.call.app_id)? != self.scope {
            return Err("account_changed: App admission or active account changed".into());
        }
        Ok(())
    }
    fn valid_revision(&self, consent: bool, revision: u64) -> Result<(), String> {
        self.identity_valid()?;
        let grant = store::get(&self.call.host_dir, &self.call.app_id, &self.scope)?;
        if grant.revision != revision || (consent && !grant.allowed) {
            return Err("stale_consent: Calendar access changed; review again".into());
        }
        Ok(())
    }
    fn valid(&self, consent: bool) -> Result<(), String> {
        self.valid_revision(consent, self.revision)
    }
    fn valid_for(&self, command: &Command) -> Result<(), String> {
        if matches!(command, Command::LoadConsent) {
            self.identity_valid()
        } else {
            self.valid(command.needs_consent())
        }
    }
    fn finish(&self, value: Result<Value, String>) {
        self.reply.clone().send(value);
    }
}
#[derive(Clone)]
enum Prepared {
    Permission,
    Select(Calendar),
    Save {
        calendar: Calendar,
        event: EventData,
        previous: Option<EventData>,
    },
    Delete {
        calendar: Calendar,
        previous: EventData,
    },
}
enum Phase {
    Loading,
    Ready(Prepared),
    Submitted,
    Finished(Result<Value, String>),
}
struct Review {
    context: Context,
    created: Instant,
    phase: Phase,
}
enum Purpose {
    Read,
    Prepare(String),
    Commit(String),
}
struct Job {
    context: Context,
    command: Command,
    purpose: Purpose,
    deadline: Instant,
}
#[derive(Default)]
struct State {
    queue: VecDeque<Job>,
    running: HashMap<String, Job>,
    reviews: HashMap<String, Review>,
    background: bool,
    timer: Option<makepad_widgets::Timer>,
    #[cfg(target_os = "android")]
    probe_started: Option<Instant>,
}
fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State::default()))
}
type Outcome = (String, Result<Value, String>);
struct Outcomes {
    sender: SyncSender<Outcome>,
    receiver: Mutex<Receiver<Outcome>>,
}
fn outcomes() -> &'static Outcomes {
    static RESULTS: OnceLock<Outcomes> = OnceLock::new();
    RESULTS.get_or_init(|| {
        let (sender, receiver) = sync_channel(MAX_PENDING * 2);
        Outcomes {
            sender,
            receiver: Mutex::new(receiver),
        }
    })
}
fn enqueue(context: Context, command: Command, purpose: Purpose) -> Result<(), String> {
    let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
    if s.queue.len() + s.running.len() >= MAX_PENDING {
        return Err("busy: Too many device calendar operations".into());
    }
    s.queue.push_back(Job {
        context,
        command,
        purpose,
        deadline: Instant::now() + Duration::from_secs(45),
    });
    SignalToUI::set_ui_signal();
    Ok(())
}
pub fn register(account_scope: AccountScope) {
    let _ = SCOPE.set(account_scope);
    prompt::register();
    services::register_host_service(Box::new(DeviceCalendarService));
}
struct DeviceCalendarService;
impl HostService for DeviceCalendarService {
    fn family(&self) -> &'static str {
        FAMILY
    }
    fn timeout(&self, _: &ServiceCall) -> Duration {
        Duration::from_secs(60)
    }
    fn api_methods(&self) -> Vec<HostApiMethod> {
        let properties = json!({"handle":{"type":"string","maxLength":64},"calendar_id":{"type":"string","maxLength":1024},
            "event_id":{"type":"string","maxLength":1024},"revision":{"type":"string","maxLength":64},
            "start_ms":{"type":"integer","minimum":0,"maximum":model::MAX_TIME},"end_ms":{"type":"integer","minimum":0,"maximum":model::MAX_TIME},
            "limit":{"type":"integer","minimum":1,"maximum":model::MAX_ITEMS},"event":{"type":"object","additionalProperties":false,"required":["title","start_ms","end_ms","timezone"],"properties":{
                "title":{"type":"string","minLength":1,"maxLength":512},"start_ms":{"type":"integer","minimum":0,"maximum":model::MAX_TIME},
                "end_ms":{"type":"integer","minimum":0,"maximum":model::MAX_TIME},"timezone":{"type":"string","maxLength":128,"description":"IANA timezone, e.g. America/Los_Angeles"},
                "all_day":{"type":"boolean","default":false},"location":{"type":"string","maxLength":2048},"notes":{"type":"string","maxLength":8192}}}});
        [("permission.status",&[][..],false,"Read app consent and OS permission without prompting"),
         ("permission.request",&[][..],true,"Review app consent, then request OS calendar permission"),
         ("permission.revoke",&[][..],true,"Revoke this app account and invalidate its selected calendar handles"),
         ("calendars.list",&[][..],false,"List at most 64 OS calendar choices after consent"),
         ("calendars.select",&["calendar_id"][..],true,"Review an OS calendar/account and return a scoped opaque handle"),
         ("events.list",&["handle","start_ms","end_ms","limit"][..],false,"List up to 200 occurrences in at most 93 days"),
         ("events.get",&["handle","event_id"][..],false,"Read one event in the selected calendar"),
         ("events.create",&["handle","event"][..],true,"Review and physically approve a new non-recurring event"),
         ("events.update",&["handle","event_id","revision","event"][..],true,"Review a replacement and reject stale or recurring events"),
         ("events.delete",&["handle","event_id","revision"][..],true,"Review and physically approve deleting one non-recurring event")]
         .into_iter().map(|(name,keys,foreground,description)|{
             let mut selected=serde_json::Map::new();for key in keys{selected.insert((*key).into(),properties[*key].clone());}
             HostApiMethod::new(format!("{FAMILY}.{name}"),1,FAMILY,description,
                 json!({"type":"object","additionalProperties":false,"required":keys,"properties":selected}),json!({"type":"object"}))
                 .with_platforms(&["macos","android"]).with_agent_access(if foreground{AgentAccess::ForegroundOnly}else{AgentAccess::Allowed})
         }).collect()
    }
    fn call(&mut self, call: ServiceCall, reply: Replier, host: &mut dyn ServiceHost) {
        if call.method() == "sheet.close" {
            if !call.from_sheet {
                reply.send(Err("invalid_review: Host sheet required".into()));
                return;
            }
            let ticket = call.args["ticket"].as_str().unwrap_or("");
            let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
            if let Some(review) = s.reviews.get(ticket) {
                if review.context.call.app_id != call.app_id
                    || review.context.call.host_dir != call.host_dir
                {
                    reply.send(Err("invalid_review: Wrong owner".into()));
                    return;
                }
                if matches!(review.phase, Phase::Submitted) {
                    reply.send(Err("busy: Approved operation is still completing".into()));
                    return;
                }
            }
            if let Some(review) = s.reviews.remove(ticket) {
                review
                    .context
                    .finish(Err("cancelled: Calendar review closed".into()));
            }
            drop(s);
            host.close_sheet();
            reply.send(Ok(json!({"closed":true})));
            return;
        }
        let result = begin(call.clone(), reply.clone(), host);
        if let Err(error) = result {
            reply.send(Err(error));
        }
    }
}
fn begin(call: ServiceCall, reply: Replier, host: &mut dyn ServiceHost) -> Result<(), String> {
    if !admitted(&call) {
        return Err("permission_denied: Declare device_calendar and requires host-api-v1".into());
    }
    let account = scope(&call.app_id)?;
    let grant = match store::get(&call.host_dir, &call.app_id, &account) {
        Ok(grant) => grant,
        Err(error)
            if error.starts_with("cache_uninitialized:")
                && call.method() == "permission.status" =>
        {
            model::fields(&call.args, &[])?;
            let context = Context {
                call,
                reply,
                scope: account,
                revision: 0,
            };
            return enqueue(context, Command::LoadConsent, Purpose::Read);
        }
        Err(error) => return Err(error),
    };
    let context = Context {
        call: call.clone(),
        reply,
        scope: account,
        revision: grant.revision,
    };
    let method = call.method();
    if method == "permission.revoke" {
        if !call.may_prompt {
            return Err("foreground_required: Open the app to revoke its calendar access".into());
        }
        model::fields(&call.args, &[])?;
        return enqueue(
            context,
            Command::Persist(ConsentChange::Revoke),
            Purpose::Read,
        );
    }
    #[cfg(target_os = "android")]
    if !supported() && ANDROID_ADAPTER.load(Ordering::Acquire) != 3 {
        if method == "permission.status" {
            model::fields(&call.args, &[])?;
            return enqueue(context, Command::Status, Purpose::Read);
        }
        return Err("initializing: Checking the Android calendar adapter; retry after permission.status completes".into());
    }
    if !supported() {
        if method == "permission.status" {
            model::fields(&call.args, &[])?;
            context.finish(Ok(json!({"supported":false,"app_consent":grant.allowed,"os_permission":"unsupported"})));
            return Ok(());
        }
        return Err(
            "unsupported_platform: Device calendars support macOS and Android Home only".into(),
        );
    }
    if method == "permission.status" {
        model::fields(&call.args, &[])?;
        return enqueue(context, Command::Status, Purpose::Read);
    }
    if method == "permission.request" {
        model::fields(&call.args, &[])?;
        return review(context, Phase::Ready(Prepared::Permission), None, host);
    }
    if !grant.allowed {
        return Err(
            "authorization_required: Request calendar permission in the foreground app".into(),
        );
    }
    if method == "calendars.list" {
        model::fields(&call.args, &[])?;
        return enqueue(context, Command::Calendars, Purpose::Read);
    }
    if method == "calendars.select" {
        model::fields(&call.args, &["calendar_id"])?;
        let id = model::text(&call.args, "calendar_id", 1024)?;
        return review(
            context,
            Phase::Loading,
            Some(Command::Calendar { id }),
            host,
        );
    }
    let handle = model::text(&call.args, "handle", 64)?;
    let calendar = grant
        .selections
        .get(&handle)
        .cloned()
        .ok_or("invalid_handle: Select a calendar for this app account")?;
    match method {
        "events.list" => {
            model::fields(&call.args, &["handle", "start_ms", "end_ms", "limit"])?;
            let start = call.args["start_ms"]
                .as_i64()
                .ok_or("invalid_arguments: Invalid start_ms")?;
            let end = call.args["end_ms"]
                .as_i64()
                .ok_or("invalid_arguments: Invalid end_ms")?;
            model::window(start, end)?;
            let limit = call.args["limit"]
                .as_u64()
                .filter(|n| *n > 0 && *n <= model::MAX_ITEMS as u64)
                .ok_or("invalid_arguments: Limit must be 1–200")? as usize;
            enqueue(
                context,
                Command::List {
                    calendar,
                    start,
                    end,
                    limit,
                },
                Purpose::Read,
            )
        }
        "events.get" => {
            model::fields(&call.args, &["handle", "event_id"])?;
            let id = model::text(&call.args, "event_id", 1024)?;
            enqueue(context, Command::Get { calendar, id }, Purpose::Read)
        }
        "events.create" => {
            model::fields(&call.args, &["handle", "event"])?;
            let event = model::event(&call.args["event"])?;
            if !calendar.writable {
                return Err("read_only: Selected calendar cannot be edited".into());
            }
            // The OS calendar is reloaded before native review and again on save.
            let _ = event;
            review(
                context,
                Phase::Loading,
                Some(Command::Calendar { id: calendar.id }),
                host,
            )
        }
        "events.update" | "events.delete" => {
            let keys = if method == "events.update" {
                &["handle", "event_id", "revision", "event"][..]
            } else {
                &["handle", "event_id", "revision"][..]
            };
            model::fields(&call.args, keys)?;
            model::text(&call.args, "revision", 64)?;
            if method == "events.update" {
                model::event(&call.args["event"])?;
            }
            if !calendar.writable {
                return Err("read_only: Selected calendar cannot be edited".into());
            }
            let id = model::text(&call.args, "event_id", 1024)?;
            review(
                context,
                Phase::Loading,
                Some(Command::Get { calendar, id }),
                host,
            )
        }
        _ => Err("method_unavailable: Unknown device calendar method".into()),
    }
}
fn review(
    context: Context,
    phase: Phase,
    command: Option<Command>,
    host: &mut dyn ServiceHost,
) -> Result<(), String> {
    if !context.call.may_prompt {
        return Err(
            "foreground_required: Open the app to review calendar access or changes".into(),
        );
    }
    let ticket = uuid::Uuid::new_v4().to_string();
    {
        let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
        if s.reviews.len() >= MAX_PENDING {
            return Err("busy: Close an earlier calendar review".into());
        }
        s.reviews.insert(
            ticket.clone(),
            Review {
                context: context.clone(),
                created: Instant::now(),
                phase,
            },
        );
    }
    if let Some(command) = command {
        if let Err(error) = enqueue(context, command, Purpose::Prepare(ticket.clone())) {
            state().lock().unwrap().reviews.remove(&ticket);
            return Err(error);
        }
    }
    host.open_sheet(format!(
        "DeviceCalendarReview {{width: Fill height: Fill ticket: {}}}",
        json!(ticket)
    ));
    Ok(())
}
fn prepared(context: &Context, value: Value) -> Result<Prepared, String> {
    let args = &context.call.args;
    if context.call.method() == "calendars.select" {
        return Ok(Prepared::Select(
            serde_json::from_value(value).map_err(|_| "invalid_response: Calendar metadata")?,
        ));
    }
    let grant = store::get(&context.call.host_dir, &context.call.app_id, &context.scope)?;
    let calendar = grant
        .selections
        .get(args["handle"].as_str().unwrap_or(""))
        .cloned()
        .ok_or("invalid_handle: Calendar selection expired")?;
    if context.call.method() == "events.create" {
        let actual: Calendar =
            serde_json::from_value(value).map_err(|_| "invalid_response: Calendar metadata")?;
        if actual.id != calendar.id || actual.account != calendar.account || !actual.writable {
            return Err("calendar_changed: Review calendar selection again".into());
        }
        return Ok(Prepared::Save {
            calendar: actual,
            event: model::event(&args["event"])?,
            previous: None,
        });
    }
    let actual: Calendar = serde_json::from_value(value["_calendar"].clone())
        .map_err(|_| "invalid_response: Calendar metadata")?;
    if actual.id != calendar.id || actual.account != calendar.account || !actual.writable {
        return Err("calendar_changed: Review calendar selection again".into());
    }
    let calendar = actual;
    let previous: EventData =
        serde_json::from_value(value).map_err(|_| "invalid_response: Calendar event")?;
    previous.editable()?;
    if previous.revision() != args["revision"].as_str().unwrap_or("") {
        return Err("conflict: Event changed; reload it before reviewing".into());
    }
    if context.call.method() == "events.delete" {
        Ok(Prepared::Delete { calendar, previous })
    } else {
        let mut event = model::event(&args["event"])?;
        event.id = previous.id.clone();
        Ok(Prepared::Save {
            calendar,
            event,
            previous: Some(previous),
        })
    }
}
fn claim_ready(phase: &mut Phase) -> Result<Prepared, String> {
    let Phase::Ready(prepared) = phase else {
        return Err("invalid_review: Not ready or already submitted".into());
    };
    let prepared = prepared.clone();
    *phase = Phase::Submitted;
    Ok(prepared)
}
fn approve(ticket: &str, contained: bool, down: bool, up: bool) -> Result<(), String> {
    if contained || !down || !up {
        return Err(
            "trusted_input_required: Physically activate the native approval button".into(),
        );
    }
    let (context, prepared) = {
        let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
        if s.background {
            return Err("foreground_required: Return to OctoSense".into());
        }
        let review = s
            .reviews
            .get_mut(ticket)
            .ok_or("expired: Calendar review ended")?;
        if review.created.elapsed() >= REVIEW_TTL {
            return Err("expired: Review calendar changes again".into());
        }
        let Phase::Ready(prepared) = &review.phase else {
            return Err("invalid_review: Not ready or already submitted".into());
        };
        review
            .context
            .valid(!matches!(prepared, Prepared::Permission))?;
        let data = claim_ready(&mut review.phase)?;
        (review.context.clone(), data)
    };
    let command = match prepared {
        Prepared::Permission => Command::Permission,
        Prepared::Save {
            calendar,
            event,
            previous,
        } => Command::Save {
            calendar,
            event,
            previous,
        },
        Prepared::Delete { calendar, previous } => Command::Delete { calendar, previous },
        Prepared::Select(calendar) => Command::Persist(ConsentChange::Select {
            handle: uuid::Uuid::new_v4().to_string(),
            calendar,
        }),
    };
    if let Err(error) = enqueue(context.clone(), command, Purpose::Commit(ticket.into())) {
        finish_review(ticket, &context, Err(error.clone()));
        return Err(error);
    }
    Ok(())
}
fn finish_review(ticket: &str, context: &Context, result: Result<Value, String>) {
    if let Some(review) = state()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .reviews
        .get_mut(ticket)
    {
        review.phase = Phase::Finished(result.clone());
    }
    context.finish(result);
}
fn fail_job(state: &mut State, job: Job, error: String) {
    if let Purpose::Prepare(ticket) | Purpose::Commit(ticket) = &job.purpose {
        if let Some(review) = state.reviews.get_mut(ticket) {
            review.phase = Phase::Finished(Err(error.clone()));
        }
    }
    job.context.finish(Err(error));
}
fn complete(id: String, result: Result<Value, String>) {
    let Some(job) = state()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .running
        .remove(&id)
    else {
        return;
    };
    if matches!(job.command, Command::LoadConsent) {
        let result = job.context.identity_valid().and(result);
        match result {
            Ok(_) => {
                let grant = store::get(
                    &job.context.call.host_dir,
                    &job.context.call.app_id,
                    &job.context.scope,
                );
                match grant {
                    Ok(grant) => {
                        let mut context = job.context;
                        context.revision = grant.revision;
                        if !cfg!(any(target_os = "macos", target_os = "android")) {
                            context.finish(Ok(json!({"supported":false,"app_consent":grant.allowed,"os_permission":"unsupported"})));
                        } else if let Err(error) =
                            enqueue(context.clone(), Command::Status, Purpose::Read)
                        {
                            context.finish(Err(error));
                        }
                    }
                    Err(error) => job.context.finish(Err(error)),
                }
            }
            Err(error) => job.context.finish(Err(error)),
        }
        return;
    }
    let revision = if result.is_ok() && matches!(job.command, Command::Persist(_)) {
        job.context.revision.saturating_add(1)
    } else {
        job.context.revision
    };
    let check = job
        .context
        .valid_revision(job.command.needs_consent(), revision);
    let result = check.and(result);
    match job.purpose {
        Purpose::Read => {
            let result = result.and_then(|mut value| {
                if matches!(job.command, Command::Status) {
                    value["app_consent"] = store::get(
                        &job.context.call.host_dir,
                        &job.context.call.app_id,
                        &job.context.scope,
                    )?
                    .allowed
                    .into();
                    value["supported"] = supported().into();
                }
                if matches!(job.command, Command::Get { .. }) {
                    value = serde_json::from_value::<EventData>(value)
                        .map_err(|_| "invalid_response: Event")?
                        .public();
                }
                if matches!(job.command, Command::List { .. }) {
                    let events = value["events"]
                        .as_array()
                        .ok_or("invalid_response: Events")?;
                    if events.len() > model::MAX_ITEMS {
                        return Err("limit: Event response too large".into());
                    }
                    let rows = events
                        .iter()
                        .map(|e| {
                            serde_json::from_value::<EventData>(e.clone())
                                .map(|e| e.public())
                                .map_err(|_| "invalid_response: Event".to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    value["events"] = json!(rows);
                }
                Ok(value)
            });
            job.context.finish(result);
        }
        Purpose::Prepare(ticket) => {
            let result = result.and_then(|value| prepared(&job.context, value));
            let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
            if let Some(review) = s.reviews.get_mut(&ticket) {
                review.phase = match result {
                    Ok(value) => Phase::Ready(value),
                    Err(error) => {
                        job.context.finish(Err(error.clone()));
                        Phase::Finished(Err(error))
                    }
                };
            }
        }
        Purpose::Commit(ticket) => {
            if matches!(job.command, Command::Permission)
                && result
                    .as_ref()
                    .is_ok_and(|value| value["os_permission"] == "granted")
            {
                if let Err(error) = enqueue(
                    job.context.clone(),
                    Command::Persist(ConsentChange::Grant),
                    Purpose::Commit(ticket.clone()),
                ) {
                    finish_review(&ticket, &job.context, Err(error));
                }
                return;
            }
            let result = result.and_then(|mut value| {
                if matches!(job.command, Command::Permission) {
                    if value["os_permission"] != "granted" {
                        return Err(
                            "authorization_required: OS calendar access was not granted".into()
                        );
                    }
                    return Err(
                        "internal_error: Permission result must be persisted before completion"
                            .into(),
                    );
                }
                if matches!(job.command, Command::Save { .. }) {
                    value["event"] = serde_json::from_value::<EventData>(value["event"].clone())
                        .map_err(|_| "invalid_response: Saved event")?
                        .public();
                }
                Ok(value)
            });
            finish_review(&ticket, &job.context, result);
        }
    }
}
/// Called on the host UI thread. Native adapters receive only host-validated
/// commands. Android replies are correlated to a fresh unpredictable job id.
pub fn handle_event(cx: &mut Cx, event: &Event) {
    #[cfg(target_os = "android")]
    if ANDROID_ADAPTER.load(Ordering::Acquire) == 0
        || (matches!(event, Event::Resume) && ANDROID_ADAPTER.load(Ordering::Acquire) == 3)
    {
        ANDROID_ADAPTER.store(1, Ordering::Release);
        state()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .probe_started = Some(Instant::now());
        cx.android_integration("device_calendar.probe", "{}");
    }
    if let Event::AndroidIntegration { channel, payload } = event {
        #[cfg(target_os = "android")]
        if channel == "device_calendar.ready" && payload == "{}" {
            ANDROID_ADAPTER.store(2, Ordering::Release);
        }
        if channel == "device_calendar.result" && payload.len() <= 1 << 20 {
            if let Ok(value) = serde_json::from_str::<Value>(payload) {
                if let Some(id) = value["id"].as_str() {
                    let result = if value["ok"] == true {
                        Ok(value["data"].clone())
                    } else {
                        Err(value["error"]
                            .as_str()
                            .unwrap_or("platform_error: Calendar operation failed")
                            .chars()
                            .take(512)
                            .collect())
                    };
                    complete(id.into(), result);
                }
            }
        }
    }
    loop {
        let outcome = outcomes()
            .receiver
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .try_recv();
        match outcome {
            Ok((id, result)) => complete(id, result),
            Err(_) => break,
        }
    }
    let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
    match event {
        Event::Pause | Event::Background | Event::WindowLostFocus(_) => s.background = true,
        Event::Resume | Event::Foreground | Event::WindowGotFocus(_) => s.background = false,
        _ => {}
    }
    FOREGROUND.store(!s.background, Ordering::Release);
    s.reviews.retain(|_, r| {
        let keep = r.created.elapsed() < REVIEW_TTL;
        if !keep {
            r.context
                .finish(Err("expired: Calendar review expired".into()));
        }
        keep
    });
    let expired: Vec<_> = s
        .running
        .iter()
        .filter(|(_, j)| Instant::now() >= j.deadline || j.context.identity_valid().is_err())
        .map(|(id, _)| id.clone())
        .collect();
    for id in expired {
        #[cfg(target_os = "android")]
        cx.android_integration(
            "device_calendar.command",
            &json!({"id":id,"operation":"cancel"}).to_string(),
        );
        if let Some(j) = s.running.remove(&id) {
            fail_job(
                &mut s,
                j,
                "timeout: Calendar operation ended; refresh to check an approved write".into(),
            );
        }
    }
    let queued = std::mem::take(&mut s.queue);
    for job in queued {
        if Instant::now() >= job.deadline {
            fail_job(
                &mut s,
                job,
                "timeout: Calendar operation expired before dispatch".into(),
            );
        } else {
            s.queue.push_back(job);
        }
    }
    let active = !s.queue.is_empty() || !s.running.is_empty() || !s.reviews.is_empty();
    if !active {
        if let Some(timer) = s.timer.take() {
            cx.stop_timer(timer);
        }
        return;
    }
    if s.timer.is_none() {
        s.timer = Some(cx.start_interval(0.25));
    }
    while s.running.len() < 2 {
        if NATIVE_WORKERS.load(Ordering::Acquire) >= 2 {
            break;
        }
        let Some(job) = s.queue.pop_front() else {
            break;
        };
        #[cfg(target_os = "android")]
        if !supported() && !matches!(job.command, Command::Persist(_) | Command::LoadConsent) {
            if s.probe_started
                .is_some_and(|start| start.elapsed() < Duration::from_secs(2))
            {
                s.queue.push_front(job);
                break;
            }
            ANDROID_ADAPTER.store(3, Ordering::Release);
            if matches!(job.command, Command::Status) {
                let result = job.context.valid(false).and_then(|_| store::get(&job.context.call.host_dir, &job.context.call.app_id, &job.context.scope))
                    .map(|grant| json!({"supported":false,"app_consent":grant.allowed,"os_permission":"unsupported"}));
                job.context.finish(result);
            } else {
                fail_job(
                    &mut s,
                    job,
                    "unsupported_platform: This Android host has no device calendar adapter".into(),
                );
            }
            continue;
        }
        if let Err(error) = job.context.valid_for(&job.command) {
            fail_job(&mut s, job, error);
            continue;
        }
        if s.background && job.command.foreground() {
            fail_job(
                &mut s,
                job,
                "foreground_required: Calendar change cancelled before dispatch".into(),
            );
            continue;
        }
        let id = uuid::Uuid::new_v4().to_string();
        let command = job.command.clone();
        let context = job.context.clone();
        let deadline = job.deadline;
        s.running.insert(id.clone(), job);
        // Android provider calls use the platform's bounded executor; consent
        // persistence uses the host pool on every platform.
        #[cfg(target_os = "android")]
        if !matches!(command, Command::Persist(_) | Command::LoadConsent) {
            let mut wire = command.wire();
            wire["id"] = id.into();
            wire["expires_after_ms"] = (deadline
                .saturating_duration_since(Instant::now())
                .as_millis() as u64)
                .into();
            cx.android_integration("device_calendar.command", &wire.to_string());
            continue;
        }
        NATIVE_WORKERS.fetch_add(1, Ordering::AcqRel);
        let fail_id = id.clone();
        let sender = outcomes().sender.clone();
        let task = cx.task_pool().submit_named(
            makepad_widgets::makepad_platform::thread::Lane::Heavy,
            "device_calendar",
            move || {
                let _worker = NativeWorker;
                let result = if Instant::now() >= deadline {
                    Err("timeout: Calendar command expired before native execution".into())
                } else {
                    context
                        .valid_for(&command)
                        .and_then(|_| execute_local(&context, &command, deadline))
                };
                let _ = sender.try_send((id, result));
                SignalToUI::set_ui_signal();
            },
        );
        match task {
            Ok(task) => task.detach(),
            Err(_) => {
                NATIVE_WORKERS.fetch_sub(1, Ordering::AcqRel);
                let _ = outcomes()
                    .sender
                    .try_send((fail_id, Err("busy: Cannot queue calendar worker".into())));
                SignalToUI::set_ui_signal();
            }
        }
    }
    let _ = cx;
}

// Called only on the bounded pool. No fsync or OS provider call runs on UI.
fn execute_local(context: &Context, command: &Command, deadline: Instant) -> Result<Value, String> {
    let guard = || -> Result<(), String> {
        if Instant::now() >= deadline {
            return Err("timeout: Calendar operation expired before commit".into());
        }
        context.valid_for(command)?;
        if command.foreground() && !FOREGROUND.load(Ordering::Acquire) {
            return Err(
                "foreground_required: Calendar operation cancelled before native execution".into(),
            );
        }
        Ok(())
    };
    guard()?;
    if matches!(command, Command::LoadConsent) {
        store::load(&context.call.host_dir)?;
        return Ok(json!({"loaded":true}));
    }
    if let Command::Persist(change) = command {
        let result = match change {
            ConsentChange::Grant => {
                json!({"supported":true,"app_consent":true,"os_permission":"granted"})
            }
            ConsentChange::Revoke => {
                json!({"app_consent":false,"handles_revoked":true,"os_permission_unchanged":true})
            }
            ConsentChange::Select { handle, calendar } => {
                json!({"handle":handle,"calendar":calendar})
            }
        };
        store::change(
            &context.call.host_dir,
            &context.call.app_id,
            &context.scope,
            Some(context.revision),
            |grant| {
                // Recheck after waiting for the disk-write lock. Cancellation or an
                // account switch cannot grant access to the new account.
                guard()?;
                match change {
                    ConsentChange::Grant => grant.allowed = true,
                    ConsentChange::Revoke => {
                        grant.allowed = false;
                        grant.selections.clear();
                    }
                    ConsentChange::Select { handle, calendar } => {
                        if grant.selections.len() >= 16 {
                            return Err(
                                "limit: At most 16 selected calendars per app account".into()
                            );
                        }
                        grant.selections.insert(handle.clone(), calendar.clone());
                    }
                }
                Ok(())
            },
        )?;
        return Ok(result);
    }
    #[cfg(target_os = "macos")]
    {
        let value = macos::execute(command, guard)?;
        if serde_json::to_vec(&value)
            .map_err(|_| "invalid_response: Calendar reply")?
            .len()
            > 1 << 20
        {
            return Err("limit: Calendar reply exceeds 1 MiB; reduce the list limit".into());
        }
        Ok(value)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("unsupported_platform: Native calendar adapter unavailable".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_review_is_single_use_and_cannot_claim_loading_or_finished() {
        let mut phase = Phase::Ready(Prepared::Permission);
        assert!(claim_ready(&mut phase).is_ok());
        assert!(claim_ready(&mut phase).is_err());
        for mut phase in [
            Phase::Loading,
            Phase::Finished(Ok(json!({}))),
            Phase::Finished(Err("cancelled".into())),
        ] {
            assert!(claim_ready(&mut phase).is_err());
        }
    }
    #[test]
    fn discovery_keeps_read_tools_separate_from_foreground_actions() {
        let methods = DeviceCalendarService.api_methods();
        assert_eq!(methods.len(), 10);
        for method in methods {
            let readonly = matches!(
                method.name.as_str(),
                "device_calendar.permission.status"
                    | "device_calendar.calendars.list"
                    | "device_calendar.events.list"
                    | "device_calendar.events.get"
            );
            assert_eq!(
                method.agent_access,
                if readonly {
                    AgentAccess::Allowed
                } else {
                    AgentAccess::ForegroundOnly
                }
            );
            assert_eq!(method.capability, FAMILY);
            assert!(!method.supports("linux"));
            assert!(method.supports("macos"));
        }
    }
}
