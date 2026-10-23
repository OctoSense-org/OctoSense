//! `studio.image`: an image from the host's `model` service, written into
//! the system agent's workspace (ADR 0006, slice 2 of the accepted route).
//!
//! The call runs `model.image` under the system agent's own identity, so the
//! person's configured image provider, the daily pixel budget and the
//! service's byte limits all apply as they do to any app; developer mode
//! admits the system agent to that service (`apps.rs`) and, as for every
//! studio tool, answers the approval itself. This slice offers the tool to
//! the system agent only: an app's agent would spend its app's budget.
//!
//! A generation can outlast one tool call, so the tool returns a job id when
//! the provider has not answered by the call's deadline, and the same tool
//! with `job` polls it; the image is written when a call sees the job done,
//! into that call's workspace, as `.studio-<uuid>.png` (or `.jpg`), the way
//! `studio.inspect` writes its PNG. Every job carries the developer-mode tag
//! of the call that started it: once the tag no longer stands, a queued call
//! is not sent, a call in flight is cancelled, an answer is dropped, a
//! finished image is forgotten, and nothing is written.
//!
//! Threads: the service call is queued for the UI thread ([`pump`]), like a
//! component's host calls, and its reply comes back through App Hub's reply
//! queue. The UI thread owns only that queue, the waits and the cancel
//! list; it never takes the job table or the developer-mode lock. Each job
//! has a watcher thread that parses the answer, notices developer mode
//! moving, and ends the job when it expires; the executor's thread only
//! polls the job table.
use super::studio::scoped;
use crate::ai_host::app_peers::host_tools::{CallerKind, HostToolCall, ToolOutcome, ToolReply};
use crate::dev_mode::DevTag;
use base64::Engine as _;
use octosense_appstore::services::{ServiceCall, ServiceHost};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const TOOL: &str = "studio.image";
/// Generations in flight at once: the model service's own bound per app
/// (`media::dispatch`), which the system agent is here.
const MAX_RUNNING: usize = 2;
/// Jobs of any state held at once, on the device and per conversation: a
/// finished, uncollected image is up to 2 MiB.
const MAX_JOBS: usize = 4;
const MAX_JOBS_PER_OWNER: usize = 2;
/// A finished job waits this long to be collected.
const JOB_TTL: Duration = Duration::from_secs(600);
/// How long a job waits for the service at most; the service's own timeout
/// covers every provider attempt and is shorter in practice.
const SERVICE_WAIT: Duration = Duration::from_secs(240);
/// A queued call the UI thread picks up with less than this left is not
/// sent: a provider may need two minutes, and a late answer would be thrown
/// away after it was paid for.
const MIN_SERVICE_TIME: Duration = Duration::from_secs(130);
/// Poll interval while a call waits for its job.
const POLL: Duration = Duration::from_millis(150);
/// How often a job's watcher looks for developer mode moving while it
/// waits for the answer, and then while it holds the finished image.
const WATCH_TICK: Duration = Duration::from_millis(250);
const HOLD_TICK: Duration = Duration::from_secs(1);
/// The sizes `model.image` accepts.
pub const SIZES: [&str; 4] = ["512x512", "1024x1024", "1536x1024", "1024x1536"];
const SERVICE: &str = "model";
const REVOKED: &str = "developer mode ended; the image job was dropped and nothing was written";
const CHANGED: &str = "developer mode changed while the call waited; call again";
const CANCELLED: &str = "the generation was cancelled";
const TIMEOUT: &str = "the image provider did not answer in time";
const NO_SUCH_JOB: &str = "no such image job in this conversation";

pub fn declaration(app: &str) -> Value {
    json!({
        "name": TOOL, "app": app,
        "description": "Developer mode only; the system agent's tool. Generate one image with the person's configured image provider and write it into your workspace, for an app's artwork or a design to build from. To start, give prompt (and optionally size, seed, class) and nothing else: the tool assigns the job id itself. The reply has path, width, height and format, or, when the provider needs longer than this call, {job, state: \"running\"}: then call again with only that job value, repeating until state is done. A call with a prompt always starts a new generation, whatever else it carries. Each generation spends the daily image budget and is approved by developer mode without asking; some providers refuse seed or 512x512; the image never leaves the workspace by itself.",
        "input_schema": {"type":"object","properties":{
            "prompt":{"type":"string","minLength":1,"maxLength":4096,"description":"What to generate. Starts a new generation; leave job out."},
            "size":{"type":"string","enum":SIZES,"description":"Pixel size; default 1024x1024."},
            "seed":{"type":"integer","minimum":0,"maximum":4294967295u64,"description":"Optional seed; some providers refuse it."},
            "class":{"type":"string","enum":["fast","strong"],"description":"Model class; default fast."},
            "job":{"type":"string","minLength":1,"maxLength":100,"description":"Only to poll: the job value a previous reply returned. Never invent one, and do not send it with a prompt."}
        },"additionalProperties":false},
        "output_schema": {"type":"object","properties":{
            "job":{"type":"string"},"state":{"type":"string","enum":["running","done"]},
            "path":{"type":"string"},"width":{"type":"integer","minimum":1},
            "height":{"type":"integer","minimum":1},"format":{"type":"string"}
        },"required":["job","state"],"additionalProperties":false},
        "risk":"act","background":false,"shareable":false
    })
}

/// Whether `tag` still stands and developer mode still covers `app`. Takes
/// the developer-mode lock: never called on the UI thread.
fn authorized(app: &str, tag: &DevTag) -> bool {
    #[cfg(test)]
    if let Some(answer) = tests::auth_override() {
        return answer(app, tag);
    }
    super::studio::authorized(app, tag)
}

/// The developer-mode generation, without its lock: it moves whenever a
/// grant may have changed.
fn generation() -> u64 {
    #[cfg(test)]
    if let Some(generation) = tests::generation_override() {
        return generation;
    }
    crate::dev_mode::generation_relaxed()
}

/// The host service family the generation is asked of.
fn family() -> &'static str {
    #[cfg(test)]
    if let Some(family) = tests::family_override() {
        return family;
    }
    SERVICE
}

// ---------------------------------------------------------------- the UI thread's state

struct Queued {
    key: usize,
    call: ServiceCall,
    /// The developer-mode generation when the call was queued.
    generation: u64,
    deadline: Instant,
    done: Sender<Result<String, String>>,
}
struct Waiting {
    done: Sender<Result<String, String>>,
    deadline: Instant,
}
static QUEUE: Mutex<Vec<Queued>> = Mutex::new(Vec::new());
static WAITING: Mutex<Option<HashMap<usize, Waiting>>> = Mutex::new(None);
/// Keys of calls to cancel: queued ones are not sent, sent ones are
/// cancelled at App Hub, by the UI thread, which owns App Hub's listeners.
static CANCELS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
/// Heap keys for these calls, in a range no isolate, script tool or
/// component uses.
static NEXT_KEY: AtomicUsize = AtomicUsize::new(1 << 52);

/// A tool call never raises a sheet: the person is not in the app.
struct NoSheet;
impl ServiceHost for NoSheet {
    fn open_sheet(&mut self, _body: String) {}
    fn close_sheet(&mut self) {}
}

/// Asks the UI thread to cancel the call with `key`, wherever it is.
fn request_cancel(key: usize) {
    CANCELS.lock().unwrap_or_else(|e| e.into_inner()).push(key);
    makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
}

/// Whether a queued call may still be sent at `now`, with developer mode at
/// `generation`: a move since it was queued may have ended its grant, and
/// the job's watcher, not this thread, is the one that can check.
fn admit(queued: &Queued, now: Instant, generation: u64) -> Result<(), String> {
    if queued.generation != generation {
        return Err(CHANGED.into());
    }
    if queued.deadline.saturating_duration_since(now) < MIN_SERVICE_TIME {
        return Err("the host was not ready in time to start the generation".into());
    }
    Ok(())
}

/// Answers every wait past its deadline and returns their keys, for the
/// caller to cancel at App Hub.
fn expire(waiting: &mut HashMap<usize, Waiting>, now: Instant) -> Vec<usize> {
    let expired: Vec<usize> = waiting
        .iter()
        .filter(|(_, wait)| wait.deadline <= now)
        .map(|(key, _)| *key)
        .collect();
    for key in &expired {
        if let Some(wait) = waiting.remove(key) {
            let _ = wait.done.send(Err(TIMEOUT.into()));
        }
    }
    expired
}

/// Dispatch the queued `model.image` calls, cancel what was asked, and
/// deliver the answers; on the UI thread ([`super::pump`]). The answer is
/// passed on as the text App Hub returned; the job's own thread parses it.
pub fn pump() {
    let generation = generation();
    let queued = std::mem::take(&mut *QUEUE.lock().unwrap_or_else(|e| e.into_inner()));
    let cancels = std::mem::take(&mut *CANCELS.lock().unwrap_or_else(|e| e.into_inner()));
    let mut waiting = WAITING.lock().unwrap_or_else(|e| e.into_inner());
    let waiting = waiting.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    for queued in queued {
        if cancels.contains(&queued.key) {
            let _ = queued.done.send(Err(CANCELLED.into()));
            continue;
        }
        if let Err(why) = admit(&queued, now, generation) {
            let _ = queued.done.send(Err(why));
            continue;
        }
        waiting.insert(
            queued.key,
            Waiting {
                done: queued.done,
                deadline: queued.deadline,
            },
        );
        octosense_appstore::services::dispatch(queued.call, queued.key, 1, &mut NoSheet);
    }
    for key in cancels {
        if let Some(wait) = waiting.remove(&key) {
            let _ = wait.done.send(Err(CANCELLED.into()));
            octosense_appstore::services::cancel_heap(key);
        }
    }
    if waiting.is_empty() {
        return;
    }
    let keys: Vec<usize> = waiting.keys().copied().collect();
    for (key, _, result) in octosense_appstore::services::take_replies_for(&keys) {
        if let Some(wait) = waiting.remove(&key) {
            let _ = wait.done.send(result);
        }
    }
    for key in expire(waiting, Instant::now()) {
        octosense_appstore::services::cancel_heap(key);
    }
}

// ---------------------------------------------------------------- jobs

/// The agent and conversation a job belongs to: another conversation never
/// sees it, as with the other studio tools' instances.
#[derive(Clone, PartialEq, Eq)]
struct Owner {
    app: String,
    account: Option<String>,
    session: String,
    context: Option<String>,
}
impl Owner {
    fn of(app: &str, call: &HostToolCall) -> Owner {
        Owner {
            app: app.to_string(),
            account: call.account.clone(),
            session: call.session_id.clone(),
            context: call.context_id.clone(),
        }
    }
}

enum State {
    Running,
    /// The decoded image, kept until a call writes it into its workspace.
    Done {
        bytes: Vec<u8>,
        width: u64,
        height: u64,
        format: String,
    },
    /// A call is writing the image now.
    Collecting,
    Failed(String),
}
struct Job {
    owner: Owner,
    app: String,
    tag: DevTag,
    /// The heap key of its service call, to cancel it at App Hub.
    key: usize,
    started: Instant,
    finished: Option<Instant>,
    state: State,
}
static JOBS: Mutex<Option<HashMap<String, Job>>> = Mutex::new(None);

fn with_jobs<R>(f: impl FnOnce(&mut HashMap<String, Job>) -> R) -> R {
    let mut jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
    let jobs = jobs.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    // Each job's watcher ends it on time; this is the backstop, and the only
    // end of a job whose watcher never answered.
    jobs.retain(|_, job| match job.finished {
        Some(at) => now.duration_since(at) < JOB_TTL + HOLD_TICK,
        None => now.duration_since(job.started) < SERVICE_WAIT + Duration::from_secs(60),
    });
    f(jobs)
}

/// Admits `job` into `jobs` under the bounds and returns its id, or why not.
fn register(jobs: &mut HashMap<String, Job>, job: Job) -> Result<String, String> {
    if jobs.values().filter(|j| matches!(j.state, State::Running)).count() >= MAX_RUNNING {
        return Err("the studio is already generating two images; wait for one to finish".into());
    }
    if jobs.len() >= MAX_JOBS {
        return Err("the studio holds as many images as it can; collect one with its job id, or let one expire".into());
    }
    if jobs.values().filter(|j| j.owner == job.owner).count() >= MAX_JOBS_PER_OWNER {
        return Err("this conversation already holds two image jobs; collect them with their job ids first".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    jobs.insert(id.clone(), job);
    Ok(id)
}

/// What `model.image` answered, decoded.
fn decode(reply: Result<String, String>) -> State {
    let value = match reply.and_then(|text| {
        serde_json::from_str::<Value>(&text).map_err(|e| format!("invalid service reply: {e}"))
    }) {
        Ok(value) => value,
        Err(why) => return State::Failed(why),
    };
    let encoded = value["b64_json"].as_str().unwrap_or("");
    let format = value["format"].as_str().unwrap_or("png").to_string();
    let (width, height) = (
        value["width"].as_u64().unwrap_or(0),
        value["height"].as_u64().unwrap_or(0),
    );
    match base64::engine::general_purpose::STANDARD.decode(encoded) {
        Ok(bytes) if !bytes.is_empty() && width > 0 && height > 0 => State::Done {
            bytes,
            width,
            height,
            format,
        },
        _ => State::Failed("the image service returned no usable image".into()),
    }
}

/// Records a job's outcome, once; false when the job is already gone.
fn finish(id: &str, outcome: State) -> bool {
    with_jobs(|jobs| match jobs.get_mut(id).filter(|job| job.finished.is_none()) {
        Some(job) => {
            job.state = outcome;
            job.finished = Some(Instant::now());
            true
        }
        None => false,
    })
}

/// Whether developer mode moved since `seen` and no longer covers the job.
fn revoked(app: &str, tag: &DevTag, seen: &mut u64) -> bool {
    let now = generation();
    if now == *seen {
        return false;
    }
    *seen = now;
    !authorized(app, tag)
}

/// Waits for the service's answer on a thread of the job's own, records
/// it, then holds the job until it is collected, abandoned, revoked or
/// expired. Started before the call is queued, so a request nobody waits
/// for is never sent; `generation` is the developer-mode generation the
/// queued call carries too, so a move in between fails one or the other.
/// An answer after developer mode ended is dropped with the job: nothing
/// is written later, and no thread waits on.
fn watch(
    id: String,
    app: String,
    tag: DevTag,
    key: usize,
    generation: u64,
    answer: Receiver<Result<String, String>>,
) -> Result<(), String> {
    std::thread::Builder::new()
        .name("studio-image".into())
        .spawn(move || {
            let deadline = Instant::now() + SERVICE_WAIT;
            let mut seen = generation;
            let outcome = loop {
                match answer.recv_timeout(WATCH_TICK) {
                    Ok(reply) => break decode(reply),
                    Err(RecvTimeoutError::Timeout) => {
                        if Instant::now() >= deadline {
                            break State::Failed(TIMEOUT.into());
                        }
                        if revoked(&app, &tag, &mut seen) {
                            request_cancel(key);
                            break State::Failed(REVOKED.into());
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        break State::Failed("the image service went away".into())
                    }
                }
            };
            let revoked = matches!(&outcome, State::Failed(why) if why == REVOKED) || !authorized(&app, &tag);
            if revoked {
                with_jobs(|jobs| {
                    jobs.remove(&id);
                });
            } else if finish(&id, outcome) {
                hold(&id, &app, &tag, seen);
            }
        })
        .map(|_| ())
        .map_err(|e| format!("the studio could not start a thread to wait for the image: {e}"))
}

/// Keeps a finished job for its collection window, and drops it when
/// developer mode no longer covers it.
fn hold(id: &str, app: &str, tag: &DevTag, mut seen: u64) {
    let expiry = Instant::now() + JOB_TTL;
    loop {
        std::thread::sleep(HOLD_TICK);
        if with_jobs(|jobs| !jobs.contains_key(id)) {
            return;
        }
        if Instant::now() >= expiry || revoked(app, tag, &mut seen) {
            with_jobs(|jobs| {
                jobs.remove(id);
            });
            return;
        }
    }
}

/// Forgets a job whose first call was cancelled before the agent learned
/// its id; the UI thread keeps its call from going out, or cancels it.
fn abandon(id: &str, key: usize) {
    request_cancel(key);
    with_jobs(|jobs| {
        jobs.remove(id);
    });
}

/// The directory App Hub hands every host service, where the `model`
/// service keeps its budgets.
fn host_dir() -> Result<PathBuf, String> {
    Ok(octosense_appstore::data_root_if_set()
        .ok_or("App Hub has no apps root yet")?
        .join(".host"))
}

/// The service call for `args`, under the calling agent's identity.
fn service_call(app: &str, args: &Value) -> Result<ServiceCall, String> {
    Ok(service_call_in(app, args, host_dir()?, family()))
}

/// The `<family>.image` request `args` ask for, one image, as an agent's
/// tool call rather than the app in the foreground.
fn service_call_in(app: &str, args: &Value, host_dir: PathBuf, family: &str) -> ServiceCall {
    let mut request = json!({"prompt": args["prompt"], "n": 1});
    for key in ["size", "seed", "class"] {
        if !args[key].is_null() {
            request[key] = args[key].clone();
        }
    }
    ServiceCall {
        app_id: app.to_string(),
        service: format!("{family}.image"),
        args: request,
        from_sheet: false,
        may_prompt: false,
        host_dir,
    }
}

/// One look at a job by the call polling it.
enum Step {
    Wait,
    Busy,
    Failed(String),
    Revoked(usize),
    Image {
        bytes: Vec<u8>,
        width: u64,
        height: u64,
        format: String,
    },
}

/// The job's state for `owner`, taking a finished image out for writing.
fn step(id: &str, owner: &Owner) -> Result<Step, String> {
    let (app, tag, key) = with_jobs(|jobs| {
        jobs.get(id)
            .filter(|job| &job.owner == owner)
            .map(|job| (job.app.clone(), job.tag.clone(), job.key))
    })
    .ok_or_else(|| NO_SUCH_JOB.to_string())?;
    // Outside the job lock: this takes the developer-mode lock.
    if !authorized(&app, &tag) {
        with_jobs(|jobs| {
            jobs.remove(id);
        });
        return Ok(Step::Revoked(key));
    }
    with_jobs(|jobs| {
        let Some(job) = jobs.get_mut(id) else {
            return Err(NO_SUCH_JOB.to_string());
        };
        Ok(match std::mem::replace(&mut job.state, State::Collecting) {
            State::Running => {
                job.state = State::Running;
                Step::Wait
            }
            State::Collecting => Step::Busy,
            State::Failed(why) => {
                jobs.remove(id);
                Step::Failed(why)
            }
            State::Done {
                bytes,
                width,
                height,
                format,
            } => Step::Image {
                bytes,
                width,
                height,
                format,
            },
        })
    })
}

/// Writes a finished job's image into the call's workspace and describes
/// it; on failure the image goes back to the job for a later call.
fn collect(
    scope: &scoped::Workspace,
    root: &Path,
    call: &HostToolCall,
    id: &str,
    bytes: Vec<u8>,
    width: u64,
    height: u64,
    format: String,
) -> Result<Value, String> {
    match write_image(scope, root, call, &bytes, &format) {
        Ok(path) => {
            with_jobs(|jobs| {
                jobs.remove(id);
            });
            Ok(json!({"job": id, "state": "done", "path": path, "width": width, "height": height, "format": format}))
        }
        Err(why) => {
            with_jobs(|jobs| {
                if let Some(job) = jobs.get_mut(id) {
                    job.state = State::Done {
                        bytes,
                        width,
                        height,
                        format,
                    };
                }
            });
            Err(why)
        }
    }
}

fn write_image(
    scope: &scoped::Workspace,
    root: &Path,
    call: &HostToolCall,
    bytes: &[u8],
    format: &str,
) -> Result<String, String> {
    use std::io::Write;
    if call.caller_kind == CallerKind::AppPeer {
        super::studio::app_available(call)?;
    }
    scope.verify(root, call.context_id.as_deref())?;
    let output = scope.output_image(format)?;
    (&output.file)
        .write_all(bytes)
        .and_then(|()| output.file.sync_all())
        .map_err(|e| format!("could not write the image: {e}"))?;
    Ok(output.keep())
}

/// `studio.image`, on the executor's thread: `root` is the caller's
/// kernel-confirmed workspace, `app` the calling agent's app and `tag` the
/// developer-mode tag that admitted the call.
pub fn execute(
    call: &HostToolCall,
    root: &Path,
    app: &str,
    tag: &DevTag,
    cancel: &AtomicBool,
    reply: &ToolReply,
) -> ToolOutcome {
    let run = || -> Result<Value, String> {
        let scope = scoped::Workspace::open(root, call.context_id.as_deref())?;
        let owner = Owner::of(app, call);
        // A prompt starts a generation, whatever else the call carries: a
        // model that fills every field sends a `job` of its own making with
        // its prompt, and a refusal here taught it nothing (observed on the
        // first live runs). Only a call without a prompt polls.
        let (id, started) = match (call.args["job"].as_str(), call.args["prompt"].as_str()) {
            (Some(job), None) => (job.to_string(), None),
            (_, Some(_)) => {
                // Built before a slot is taken: an invalid request holds none.
                let service_call = service_call(app, &call.args)?;
                let key = NEXT_KEY.fetch_add(1, Ordering::Relaxed);
                let id = with_jobs(|jobs| {
                    register(
                        jobs,
                        Job {
                            owner: owner.clone(),
                            app: app.to_string(),
                            tag: tag.clone(),
                            key,
                            started: Instant::now(),
                            finished: None,
                            state: State::Running,
                        },
                    )
                })?;
                let (done, answer) = channel();
                let generation = generation();
                if let Err(why) = watch(id.clone(), app.to_string(), tag.clone(), key, generation, answer) {
                    with_jobs(|jobs| {
                        jobs.remove(&id);
                    });
                    return Err(why);
                }
                QUEUE.lock().unwrap_or_else(|e| e.into_inner()).push(Queued {
                    key,
                    call: service_call,
                    generation,
                    deadline: Instant::now() + SERVICE_WAIT,
                    done,
                });
                makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
                (id, Some(key))
            }
            (None, None) => return Err("give prompt to start an image, or the job value a reply returned to poll one".into()),
        };
        // Wait for this call's share of the deadline; a running job outlives
        // the call and is polled with its id.
        let deadline = Instant::now()
            + Duration::from_millis(call.timeout_ms.saturating_sub(1_500).max(500)).min(SERVICE_WAIT);
        loop {
            if cancel.load(Ordering::SeqCst) || !reply.is_open() {
                // The agent never learns a cancelled first call's job id, so
                // the job is not left to run for nobody.
                return Err(match started {
                    Some(key) => {
                        abandon(&id, key);
                        "cancelled; the generation was abandoned".into()
                    }
                    None => "cancelled".into(),
                });
            }
            match step(&id, &owner)? {
                Step::Wait => {}
                Step::Busy => return Err("this image is being written by another call".into()),
                Step::Failed(why) => return Err(why),
                Step::Revoked(key) => {
                    request_cancel(key);
                    return Err(REVOKED.into());
                }
                Step::Image {
                    bytes,
                    width,
                    height,
                    format,
                } => return collect(&scope, root, call, &id, bytes, width, height, format),
            }
            if Instant::now() >= deadline {
                return Ok(json!({"job": id, "state": "running"}));
            }
            std::thread::sleep(POLL);
        }
    };
    match run() {
        Ok(value) => ToolOutcome::Ok(value),
        Err(why) => ToolOutcome::error("studio_image_failed", why),
    }
}

#[cfg(test)]
mod tests {
    use super::super::relay::SYSTEM;
    use super::super::studio;
    use super::*;
    use crate::ai_host::app_peers::host_tools::ToolExecutor;
    use octosense_appstore::services::{HostService, Replier};
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    // ------------------------------------------------ seams for these tests
    //
    // Developer mode is a process-wide controller, and the `model` family
    // may be the real service in this test process, so most tests answer
    // the authorization and the generation themselves and register their
    // fake under a family of their own. The two tests that need the real
    // controller run in a child process of this test binary.
    static DEV_ON: AtomicBool = AtomicBool::new(true);
    static GENERATION: AtomicU64 = AtomicU64::new(1);
    static SEAMS: AtomicBool = AtomicBool::new(false);
    pub(super) fn auth_override() -> Option<fn(&str, &DevTag) -> bool> {
        SEAMS.load(Ordering::SeqCst).then_some(dev_flag as fn(&str, &DevTag) -> bool)
    }
    pub(super) fn generation_override() -> Option<u64> {
        SEAMS.load(Ordering::SeqCst).then(|| GENERATION.load(Ordering::SeqCst))
    }
    pub(super) fn family_override() -> Option<&'static str> {
        SEAMS.load(Ordering::SeqCst).then_some(TEST_FAMILY)
    }
    fn dev_flag(_app: &str, _tag: &DevTag) -> bool {
        DEV_ON.load(Ordering::SeqCst)
    }
    /// Developer mode "moves": the next pump refuses what was queued before,
    /// and a watcher re-checks its job.
    fn move_generation() {
        GENERATION.fetch_add(1, Ordering::SeqCst);
    }
    const TEST_FAMILY: &str = "studio_image_test";

    static CALLS: AtomicUsize = AtomicUsize::new(0);
    static DELAY_MS: AtomicUsize = AtomicUsize::new(1500);
    static REPLIERS: Mutex<Vec<Replier>> = Mutex::new(Vec::new());
    static SETUP: std::sync::Once = std::sync::Once::new();
    /// The fake service and the job table are shared: one test at a time.
    static SERIAL: Mutex<()> = Mutex::new(());

    /// Answers `<family>.image` like the real service, from a thread, after
    /// `DELAY_MS`; keeps each replier so a test can see whether App Hub still
    /// has the request pending.
    struct FakeModel(&'static str);
    impl HostService for FakeModel {
        fn family(&self) -> &'static str {
            self.0
        }
        fn timeout(&self, _call: &ServiceCall) -> Duration {
            Duration::from_secs(270)
        }
        fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
            CALLS.fetch_add(1, Ordering::SeqCst);
            assert!(call.service.ends_with(".image"), "{}", call.service);
            assert!(!call.may_prompt && !call.from_sheet);
            assert_eq!(call.app_id, SYSTEM);
            REPLIERS.lock().unwrap().push(reply.clone());
            let delay = Duration::from_millis(DELAY_MS.load(Ordering::SeqCst) as u64);
            std::thread::spawn(move || {
                std::thread::sleep(delay);
                reply.send(Ok(png_reply(1024, 1024)));
            });
        }
    }

    fn png_bytes() -> Vec<u8> {
        vec![0x89, b'P', b'N', b'G', 1, 2, 3, 4]
    }
    fn png_reply(width: u64, height: u64) -> Value {
        json!({"b64_json": base64::engine::general_purpose::STANDARD.encode(png_bytes()),
               "format": "png", "mime": "image/png", "width": width, "height": height})
    }
    fn tag() -> DevTag {
        DevTag {
            profile_id: "test-profile".into(),
            since: 1,
        }
    }
    fn job(owner: &str, state: State) -> Job {
        Job {
            owner: Owner {
                app: SYSTEM.into(),
                account: None,
                session: owner.into(),
                context: None,
            },
            app: SYSTEM.into(),
            tag: tag(),
            key: 0,
            started: Instant::now(),
            finished: if matches!(state, State::Running) { None } else { Some(Instant::now()) },
            state,
        }
    }

    /// A workspace of its own for one test, under a data root the fake
    /// service never reads; the seams on.
    fn setup(test: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("studio-image-{}", std::process::id()));
        SETUP.call_once(|| {
            std::fs::create_dir_all(root.join("apps/.host")).unwrap();
            octosense_appstore::set_data_root(root.join("apps"));
            octosense_appstore::services::register_host_service(Box::new(FakeModel(TEST_FAMILY)));
            SEAMS.store(true, Ordering::SeqCst);
        });
        let workspace = root.join(test);
        let _ = std::fs::remove_dir_all(&workspace);
        std::fs::create_dir_all(&workspace).unwrap();
        DEV_ON.store(true, Ordering::SeqCst);
        with_jobs(|jobs| jobs.clear());
        workspace
    }

    fn call(id: &str, session: &str, args: Value, timeout_ms: u64) -> HostToolCall {
        let mut call = HostToolCall::parse(&json!({"call_id": id, "name": TOOL, "app": SYSTEM,
            "session_id": session, "caller": {"kind": "system"}, "args": args,
            "timeout_ms": timeout_ms, "risk": "act"}))
        .unwrap();
        call.calling_app = SYSTEM.into();
        call
    }

    fn data(outcome: ToolOutcome) -> Result<Value, String> {
        match outcome {
            ToolOutcome::Ok(value) => Ok(value),
            ToolOutcome::Error { kind, message } => Err(format!("{kind}: {message}")),
        }
    }

    /// Runs `execute` as the executor does, on its own thread, while this
    /// thread pumps as the UI thread would; `between` runs once the call
    /// is queued, before the first pump.
    fn run(
        call: HostToolCall,
        workspace: &Path,
        cancel: Arc<AtomicBool>,
        between: impl FnOnce(),
    ) -> Result<Value, String> {
        let workspace = workspace.to_path_buf();
        let worker = std::thread::spawn(move || {
            let reply = ToolReply::new(call.call_id.clone(), |_| {});
            let outcome = execute(&call, &workspace, SYSTEM, &tag(), &cancel, &reply);
            reply.finish(outcome.clone());
            outcome
        });
        std::thread::sleep(Duration::from_millis(50));
        between();
        while !worker.is_finished() {
            pump();
            std::thread::sleep(Duration::from_millis(10));
        }
        data(worker.join().unwrap())
    }

    /// Pumps for `for_` so late answers and cancels are handled.
    fn pump_for(for_: Duration) {
        let until = Instant::now() + for_;
        while Instant::now() < until {
            pump();
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn jobs_of(session: &str) -> usize {
        with_jobs(|jobs| jobs.values().filter(|job| job.owner.session == session).count())
    }

    fn state_of(job: &str) -> Option<String> {
        with_jobs(|jobs| {
            jobs.get(job).map(|job| match &job.state {
                State::Running => "running".to_string(),
                State::Done { .. } => "done".to_string(),
                State::Collecting => "collecting".to_string(),
                State::Failed(why) => format!("failed: {why}"),
            })
        })
    }

    fn files_in(workspace: &Path) -> usize {
        std::fs::read_dir(workspace).unwrap().count()
    }

    fn last_replier() -> Replier {
        REPLIERS.lock().unwrap().last().cloned().expect("a service call reached the fake")
    }

    // ------------------------------------------------ the real executor

    #[test]
    fn a_start_returns_running_and_a_poll_collects_the_image_once() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let workspace = setup("happy");
        DELAY_MS.store(1500, Ordering::SeqCst);
        let calls = CALLS.load(Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        // The job a model invents beside its prompt is ignored: the call
        // starts a generation and the reply names the real job.
        let first = run(call("h1", "sys", json!({"prompt": "a red square", "job": "new"}), 2000), &workspace, cancel.clone(), || {}).unwrap();
        assert_eq!(first["state"], "running");
        let job = first["job"].as_str().unwrap().to_string();
        assert_ne!(job, "new");
        // Another conversation cannot collect it.
        let other = run(call("o1", "other", json!({"job": job}), 2000), &workspace, cancel.clone(), || {});
        assert!(other.unwrap_err().contains(NO_SUCH_JOB));
        let second = run(call("h2", "sys", json!({"job": job}), 6000), &workspace, cancel.clone(), || {}).unwrap();
        assert_eq!(second["state"], "done");
        assert_eq!((second["width"].as_u64(), second["format"].as_str()), (Some(1024), Some("png")));
        let path = second["path"].as_str().unwrap();
        assert!(path.starts_with(".studio-") && path.ends_with(".png"), "{path}");
        assert_eq!(std::fs::read(workspace.join(path)).unwrap(), png_bytes());
        assert_eq!(CALLS.load(Ordering::SeqCst), calls + 1);
        // Collected once: a second poll finds nothing.
        let again = run(call("h3", "sys", json!({"job": job}), 2000), &workspace, cancel, || {});
        assert!(again.is_err());
        assert_eq!(files_in(&workspace), 1);
    }

    #[test]
    fn a_cancel_before_the_first_reply_abandons_the_job_and_cancels_it_at_app_hub() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let workspace = setup("cancel");
        DELAY_MS.store(800, Ordering::SeqCst);
        let calls = CALLS.load(Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let outcome = run(call("c1", "sys-cancel", json!({"prompt": "a blue square"}), 30_000), &workspace, cancel, move || {
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(150));
                flag.store(true, Ordering::SeqCst);
            });
        });
        assert!(outcome.unwrap_err().contains("abandoned"));
        assert_eq!(jobs_of("sys-cancel"), 0);
        // The UI thread cancels the sent request at App Hub; a late answer
        // finds no job and writes nothing.
        pump_for(Duration::from_millis(100));
        assert_eq!(CALLS.load(Ordering::SeqCst), calls + 1, "it had been dispatched before the cancel");
        assert!(!last_replier().is_pending(), "cancelled at App Hub");
        pump_for(Duration::from_millis(1200));
        assert_eq!(jobs_of("sys-cancel"), 0);
        assert_eq!(files_in(&workspace), 0);
    }

    #[test]
    fn ending_developer_mode_drops_a_finished_job_on_its_poll_and_writes_nothing() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let workspace = setup("revoke");
        DELAY_MS.store(1500, Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        let first = run(call("r1", "sys-revoke", json!({"prompt": "a green square"}), 2000), &workspace, cancel.clone(), || {}).unwrap();
        let job = first["job"].as_str().unwrap().to_string();
        // The grant ends without the generation moving (the watcher cannot
        // tell); the poll re-checks the tag before it writes.
        DEV_ON.store(false, Ordering::SeqCst);
        let poll = run(call("r2", "sys-revoke", json!({"job": job}), 6000), &workspace, cancel.clone(), || {});
        assert!(poll.unwrap_err().contains("developer mode ended"));
        assert_eq!(jobs_of("sys-revoke"), 0);
        pump_for(Duration::from_millis(1500));
        assert_eq!(files_in(&workspace), 0);
        // Turned on again, the old job is still gone.
        DEV_ON.store(true, Ordering::SeqCst);
        let later = run(call("r3", "sys-revoke", json!({"job": job}), 2000), &workspace, cancel, || {});
        assert!(later.unwrap_err().contains(NO_SUCH_JOB));
    }

    #[test]
    fn a_watcher_ends_a_running_job_when_developer_mode_moves_and_no_longer_covers_it() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let workspace = setup("watch");
        DELAY_MS.store(3000, Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        let first = run(call("w1", "sys-watch", json!({"prompt": "a yellow square"}), 2000), &workspace, cancel.clone(), || {}).unwrap();
        let job = first["job"].as_str().unwrap().to_string();
        let replier = last_replier();
        assert!(replier.is_pending());
        DEV_ON.store(false, Ordering::SeqCst);
        move_generation();
        // Within a watch tick the watcher drops the job and the UI thread
        // cancels the request at App Hub; no poll was needed, and no
        // thread waits on.
        pump_for(Duration::from_millis(700));
        assert_eq!(state_of(&job), None, "dropped at once");
        assert!(!replier.is_pending(), "cancelled at App Hub");
        pump_for(Duration::from_millis(2600));
        assert_eq!(files_in(&workspace), 0, "nothing written");
        // Back on, under a new generation: the old job cannot be collected.
        DEV_ON.store(true, Ordering::SeqCst);
        move_generation();
        let poll = run(call("w2", "sys-watch", json!({"job": job}), 2000), &workspace, cancel, || {});
        assert!(poll.is_err());
        assert_eq!(files_in(&workspace), 0);
    }

    #[test]
    fn a_queued_call_is_not_sent_once_developer_mode_has_moved() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let workspace = setup("queued");
        let calls = CALLS.load(Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        let outcome = run(call("q1", "sys-queued", json!({"prompt": "never sent"}), 30_000), &workspace, cancel, move_generation);
        assert!(outcome.unwrap_err().contains("developer mode changed"));
        assert_eq!(CALLS.load(Ordering::SeqCst), calls, "the paid call must not go out");
        assert_eq!(jobs_of("sys-queued"), 0);
    }

    // ------------------------------------------------ the real developer-mode controller
    //
    // Two checks need developer mode on for real: that the executor refuses
    // an app's agent for the system-only reason, and that turning the mode
    // off ends a job in flight. The controller is process-wide, so each
    // runs in a child of this test binary launched with `OCTOSENSE_DEV_MODE=all`.

    /// In the parent: runs `test` in a child with the real controller on and
    /// returns true once it passed. In the child: false, run the body.
    fn in_child_with_developer_mode(test: &str) -> bool {
        const CHILD: &str = "OCTOSENSE_STUDIO_IMAGE_TEST_CHILD";
        if std::env::var(CHILD).as_deref() == Ok(test) {
            return false;
        }
        let output = std::process::Command::new(std::env::current_exe().expect("the shell test binary"))
            .args(["--exact", &format!("host_tools::studio_image::tests::{test}"), "--nocapture"])
            .env(CHILD, test)
            .env(crate::dev_mode::ENV, "all")
            .output()
            .expect("start the child test");
        let (stdout, stderr) = (String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        assert!(output.status.success(), "{stdout}\n{stderr}");
        assert!(stdout.contains("test result: ok. 1 passed;"), "the child must run the one test:\n{stdout}");
        true
    }

    /// The child's world: a fresh data root, the fake under the real
    /// `model` family, the real controller on, and a UI-thread stand-in.
    fn real_setup(test: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("studio-image-real-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("apps/.host")).unwrap();
        std::fs::create_dir_all(root.join("home")).unwrap();
        std::fs::create_dir_all(root.join("ws")).unwrap();
        octosense_appstore::set_data_root(root.join("apps"));
        octosense_appstore::services::register_host_service(Box::new(FakeModel(SERVICE)));
        crate::dev_mode::init(&root.join("home"));
        assert!(crate::dev_mode::grants_all(SYSTEM), "developer mode is on for the system agent");
        std::thread::Builder::new()
            .name("ui".into())
            .spawn(|| loop {
                pump();
                std::thread::sleep(Duration::from_millis(10));
            })
            .unwrap();
        root
    }

    /// Through the real executor: developer-mode tag, grants, schema and
    /// the system-only rule; the one reply it sends.
    fn via_executor(call: HostToolCall) -> Value {
        let (tx, rx) = channel();
        let reply = ToolReply::new(call.call_id.clone(), move |value| {
            let _ = tx.send(value);
        });
        studio::StudioExecutor::default().execute(call, reply);
        rx.recv_timeout(Duration::from_secs(20)).expect("one reply")
    }

    #[test]
    fn an_app_agent_naming_the_tool_is_refused_before_any_service_call() {
        if !studio::SUPPORTED || in_child_with_developer_mode("an_app_agent_naming_the_tool_is_refused_before_any_service_call") {
            return;
        }
        let root = real_setup("app");
        let workspace = std::fs::canonicalize(root.join("ws")).unwrap();
        let calls = CALLS.load(Ordering::SeqCst);
        // Each of these owns the tool it names, as far as the relay's
        // ownership check goes; only the system agent gets this one.
        for (calling, owner) in [("card.os.news", "os.news"), ("card.org.example.notes", "org.example.notes"), ("card.os.news", SYSTEM)] {
            let mut call = HostToolCall::parse(&json!({"peer": "p1", "session_id": "s#peer-p1", "call_id": format!("app-{owner}"),
                "name": TOOL, "app": owner, "caller": {"kind": "app_peer"}, "args": {"prompt": "x"},
                "timeout_ms": 2000, "risk": "act"}))
            .unwrap();
            call.calling_app = calling.into();
            call.account = Some("device".into());
            call.peer_workspace = Some(workspace.clone());
            let reply = via_executor(call);
            assert_eq!(reply["ok"], false, "{reply}");
            assert_eq!(reply["error"]["kind"], "not_granted", "{reply}");
        }
        assert_eq!(CALLS.load(Ordering::SeqCst), calls, "no service call for an app's agent");
        assert_eq!(files_in(&workspace), 0);
        // The system agent itself is admitted by the same executor.
        studio::system_workspace_opened(Some(workspace.to_str().unwrap()));
        DELAY_MS.store(100, Ordering::SeqCst);
        let mut call = call("sys-1", crate::system_chat::session::SYSTEM_SESSION, json!({"prompt": "a red square"}), 6000);
        call.calling_app = SYSTEM.into();
        let reply = via_executor(call);
        assert_eq!(reply["data"]["state"], "done", "{reply}");
        assert_eq!(files_in(&workspace), 1);
    }

    #[test]
    fn turning_developer_mode_off_ends_a_job_in_flight_without_a_poll() {
        if !studio::SUPPORTED || in_child_with_developer_mode("turning_developer_mode_off_ends_a_job_in_flight_without_a_poll") {
            return;
        }
        let root = real_setup("off");
        let workspace = root.join("ws");
        DELAY_MS.store(3000, Ordering::SeqCst);
        let live = crate::dev_mode::tag().expect("a live developer-mode tag");
        let cancel = AtomicBool::new(false);
        let reply = ToolReply::new("off-1", |_| {});
        let first = data(execute(&call("off-1", "sys", json!({"prompt": "a green square"}), 2000), &workspace, SYSTEM, &live, &cancel, &reply)).unwrap();
        let job = first["job"].as_str().unwrap().to_string();
        let replier = last_replier();
        assert!(replier.is_pending());
        assert!(crate::apps::model_grant(SYSTEM, &root.join("apps/.host")));
        crate::dev_mode::turn_off("studio.image test");
        assert!(!crate::apps::model_grant(SYSTEM, &root.join("apps/.host")), "the model service refuses the system agent now");
        std::thread::sleep(Duration::from_millis(700));
        assert_eq!(state_of(&job), None, "the watcher dropped it at once");
        assert!(!replier.is_pending(), "the watcher had it cancelled at App Hub");
        std::thread::sleep(Duration::from_millis(3000));
        assert_eq!(files_in(&workspace), 0, "nothing written");
        // Back on, with a new tag: the old job cannot be collected.
        std::thread::sleep(Duration::from_millis(1100));
        crate::dev_mode::init(&root.join("home"));
        let tag = crate::dev_mode::tag().expect("a new tag");
        assert_ne!(tag, live);
        let poll = data(execute(&call("off-2", "sys", json!({"job": job}), 2000), &workspace, SYSTEM, &tag, &cancel, &reply));
        assert!(poll.is_err());
        assert_eq!(files_in(&workspace), 0);
    }

    // ------------------------------------------------ the pieces

    #[test]
    fn jobs_are_bounded_running_held_and_per_conversation() {
        let mut jobs = HashMap::new();
        let a1 = register(&mut jobs, job("a", State::Running)).unwrap();
        let b1 = register(&mut jobs, job("b", State::Running)).unwrap();
        let third = register(&mut jobs, job("c", State::Running)).unwrap_err();
        assert!(third.contains("two images"), "{third}");
        // A finished image frees its running slot but is still held: it
        // counts per conversation and in all until collected or expired.
        jobs.get_mut(&a1).unwrap().state = State::Failed("x".into());
        jobs.get_mut(&b1).unwrap().state = State::Failed("x".into());
        register(&mut jobs, job("a", State::Running)).unwrap();
        let a_third = register(&mut jobs, job("a", State::Running)).unwrap_err();
        assert!(a_third.contains("this conversation"), "{a_third}");
        let d = register(&mut jobs, job("d", State::Running)).unwrap();
        jobs.get_mut(&d).unwrap().state = State::Done {
            bytes: png_bytes(),
            width: 1,
            height: 1,
            format: "png".into(),
        };
        let fifth = register(&mut jobs, job("e", State::Running)).unwrap_err();
        assert!(fifth.contains("holds as many"), "{fifth}");
        jobs.remove(&a1);
        register(&mut jobs, job("e", State::Running)).unwrap();
    }

    #[test]
    fn a_service_reply_is_decoded_or_refused() {
        match decode(Ok(png_reply(512, 512).to_string())) {
            State::Done { bytes, width, height, format } => {
                assert_eq!((bytes.len(), width, height, format.as_str()), (8, 512, 512, "png"));
            }
            _ => panic!("decoded"),
        }
        assert!(matches!(decode(Ok(json!({"format":"png"}).to_string())), State::Failed(_)));
        assert!(matches!(decode(Ok(json!({"b64_json":"!!","width":1,"height":1}).to_string())), State::Failed(_)));
        assert!(matches!(decode(Ok("not json".into())), State::Failed(w) if w.contains("invalid service reply")));
        assert!(matches!(decode(Err("budget".into())), State::Failed(w) if w == "budget"));
    }

    #[test]
    fn the_ui_thread_refuses_a_late_or_outdated_call_and_answers_an_expired_wait() {
        let queued = |deadline: Instant, generation: u64| {
            let (done, answer) = channel();
            let queued = Queued {
                key: 7,
                call: service_call_in(SYSTEM, &json!({"prompt": "p"}), std::env::temp_dir(), TEST_FAMILY),
                generation,
                deadline,
                done,
            };
            (queued, answer)
        };
        let now = Instant::now();
        let (fresh, _) = queued(now + SERVICE_WAIT, 3);
        assert!(admit(&fresh, now, 3).is_ok());
        assert!(admit(&fresh, now, 4).unwrap_err().contains("developer mode changed"));
        let (late, _) = queued(now + MIN_SERVICE_TIME - Duration::from_secs(1), 3);
        assert!(admit(&late, now, 3).unwrap_err().contains("not ready in time"));

        let (done, answer) = channel();
        let mut waiting = HashMap::new();
        waiting.insert(
            9usize,
            Waiting {
                done,
                deadline: now - Duration::from_secs(1),
            },
        );
        let (kept, _keep) = channel();
        waiting.insert(
            10usize,
            Waiting {
                done: kept,
                deadline: now + Duration::from_secs(1),
            },
        );
        assert_eq!(expire(&mut waiting, now), vec![9]);
        assert_eq!(answer.try_recv().unwrap().unwrap_err(), TIMEOUT);
        assert!(waiting.contains_key(&10) && !waiting.contains_key(&9));
    }

    #[test]
    fn the_declaration_takes_a_prompt_or_a_job_and_nothing_else() {
        let decl = declaration(SYSTEM);
        let schema = &decl["input_schema"];
        assert!(super::super::schema::check(schema, &json!({"prompt":"a red square"})).is_ok());
        assert!(super::super::schema::check(schema, &json!({"prompt":"x","size":"512x512","seed":7,"class":"fast"})).is_ok());
        assert!(super::super::schema::check(schema, &json!({"job":"j"})).is_ok());
        assert!(super::super::schema::check(schema, &json!({"prompt":"x","size":"640x480"})).is_err());
        assert!(super::super::schema::check(schema, &json!({"prompt":"x","output_path":"a.png"})).is_err());
        // A prompt with a made-up job is a start, not a refusal (see execute).
        assert!(super::super::schema::check(schema, &json!({"prompt":"x","job":"new"})).is_ok());
        assert_eq!(decl["risk"], "act");
        let request = service_call_in(SYSTEM, &json!({"prompt":"p","size":"512x512","seed":3}), std::env::temp_dir(), SERVICE);
        assert_eq!(request.service, "model.image");
        assert_eq!(request.args, json!({"prompt":"p","n":1,"size":"512x512","seed":3}));
        assert!(!request.may_prompt && !request.from_sheet);
    }

    #[test]
    fn the_tool_is_the_system_agents_and_the_model_service_admits_it_under_developer_mode_only() {
        if !studio::SUPPORTED {
            return;
        }
        assert!(studio::declarations(SYSTEM).iter().any(|d| d["name"] == TOOL));
        assert!(!studio::declarations("os.news").iter().any(|d| d["name"] == TOOL));
        assert!(studio::declarations("os.news").iter().any(|d| d["name"] == "studio.install"));
        // What the relay grants, by owner and tool.
        assert!(studio::granted_to(SYSTEM, TOOL));
        assert!(!studio::granted_to("os.news", TOOL));
        assert!(studio::granted_to("os.news", "studio.install"));
        assert!(!studio::granted_to(SYSTEM, "studio.nothing"));
        // The admission the model service asks `apps.rs` for: the system
        // agent only while developer mode grants it everything, and only in
        // a build that has the studio.
        assert!(studio::system_caller_admitted_given(SYSTEM, true));
        assert!(!studio::system_caller_admitted_given(SYSTEM, false));
        assert!(!studio::system_caller_admitted_given("os.news", true));
        assert_eq!(studio::system_caller_admitted(SYSTEM), crate::dev_mode::grants_all(SYSTEM));
        assert_eq!(crate::apps::model_scope(SYSTEM).as_deref(), Some("device"));
        // An app's scope is its contained account's hash, if it has one;
        // never the device.
        assert_ne!(crate::apps::model_scope("os.news").as_deref(), Some("device"));
        assert!(!crate::apps::model_grant("os.news", &std::env::temp_dir()));
    }
}
