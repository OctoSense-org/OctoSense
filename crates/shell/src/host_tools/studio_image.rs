//! `studio.image`: an image from the host's `model` service, written into
//! the caller's workspace (ADR 0006, slice 2 of the accepted route).
//!
//! The call runs `model.image` under the calling agent's own identity, so the
//! person's configured image provider, the daily pixel budget and the
//! service's byte limits all apply as they do to any app. The system agent
//! is admitted only while developer mode covers it (`apps.rs`); an app's
//! agent is admitted as its app. A generation can outlast one tool call, so
//! the tool returns a job id when the provider has not answered by the
//! call's deadline, and the same tool with `job` polls it; the image is
//! written when a call sees the job done, into that call's workspace, as
//! `.studio-<uuid>.png` (or `.jpg`), the way `studio.inspect` writes its PNG.
//!
//! The service call itself is queued for the UI thread ([`pump`]), like a
//! component's host calls, and its reply comes back through App Hub's reply
//! queue; this executor's thread only waits.
use super::relay::SYSTEM;
use super::studio::scoped;
use crate::ai_host::app_peers::host_tools::{HostToolCall, ToolOutcome, ToolReply};
use base64::Engine as _;
use octosense_appstore::services::{ServiceCall, ServiceHost};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const TOOL: &str = "studio.image";
/// Open jobs per calling agent and conversation, and in all.
const MAX_JOBS_PER_OWNER: usize = 2;
const MAX_JOBS: usize = 8;
/// A finished or abandoned job is forgotten after this.
const JOB_TTL: Duration = Duration::from_secs(600);
/// How long a job waits for the service at most; the service's own timeout
/// covers every provider attempt and is shorter in practice.
const SERVICE_WAIT: Duration = Duration::from_secs(240);
/// Poll interval while a call waits for its job.
const POLL: Duration = Duration::from_millis(150);
/// The sizes `model.image` accepts.
pub const SIZES: [&str; 4] = ["512x512", "1024x1024", "1536x1024", "1024x1536"];

pub fn declaration(app: &str) -> Value {
    json!({
        "name": TOOL, "app": app,
        "description": "Developer mode only. Generate one image with the person's configured image provider and write it into your workspace, for an app's artwork or a design to build from. Give prompt (and optionally size, seed, class) to start; the reply has path, width, height and format, or, when the provider needs longer than this call, a job id: call again with only that job until state is done. Each generation spends the daily image budget; the image never leaves the workspace by itself.",
        "input_schema": {"type":"object","properties":{
            "prompt":{"type":"string","minLength":1,"maxLength":4096},
            "size":{"type":"string","enum":SIZES},
            "seed":{"type":"integer","minimum":0,"maximum":4294967295u64},
            "class":{"type":"string","enum":["fast","strong"]},
            "job":{"type":"string","minLength":1,"maxLength":100}
        },"additionalProperties":false},
        "output_schema": {"type":"object","properties":{
            "job":{"type":"string"},"state":{"type":"string","enum":["running","done"]},
            "path":{"type":"string"},"width":{"type":"integer","minimum":1},
            "height":{"type":"integer","minimum":1},"format":{"type":"string"}
        },"required":["job","state"],"additionalProperties":false},
        "risk":"act","background":false,"shareable":false
    })
}

// ---------------------------------------------------------------- the service call queue

struct Queued {
    call: ServiceCall,
    deadline: Instant,
    done: Sender<Result<Value, String>>,
}
static QUEUE: Mutex<Vec<Queued>> = Mutex::new(Vec::new());
static WAITING: Mutex<Option<HashMap<usize, (Sender<Result<Value, String>>, Instant)>>> =
    Mutex::new(None);
/// Heap keys for these calls, in a range no isolate, script tool or
/// component uses.
static NEXT_KEY: AtomicUsize = AtomicUsize::new(1 << 52);

/// A tool call never raises a sheet: the person is not in the app.
struct NoSheet;
impl ServiceHost for NoSheet {
    fn open_sheet(&mut self, _body: String) {}
    fn close_sheet(&mut self) {}
}

/// Dispatch the queued `model.image` calls and deliver their answers; on the
/// UI thread ([`super::pump`]).
pub fn pump() {
    let queued = std::mem::take(&mut *QUEUE.lock().unwrap_or_else(|e| e.into_inner()));
    let mut waiting = WAITING.lock().unwrap_or_else(|e| e.into_inner());
    let waiting = waiting.get_or_insert_with(HashMap::new);
    for Queued { call, deadline, done } in queued {
        if Instant::now() >= deadline {
            continue;
        }
        let key = NEXT_KEY.fetch_add(1, Ordering::Relaxed);
        waiting.insert(key, (done, deadline));
        octosense_appstore::services::dispatch(call, key, 1, &mut NoSheet);
    }
    if waiting.is_empty() {
        return;
    }
    let keys: Vec<usize> = waiting.keys().copied().collect();
    for (key, _, result) in octosense_appstore::services::take_replies_for(&keys) {
        if let Some((done, _)) = waiting.remove(&key) {
            let parsed = result.and_then(|text| {
                serde_json::from_str::<Value>(&text).map_err(|e| format!("invalid service reply: {e}"))
            });
            let _ = done.send(parsed);
        }
    }
    let now = Instant::now();
    waiting.retain(|_, (_, deadline)| *deadline > now);
}

// ---------------------------------------------------------------- jobs

#[derive(Clone)]
enum State {
    Running,
    /// The decoded image, kept until a call writes it into its workspace.
    Done {
        bytes: Vec<u8>,
        width: u64,
        height: u64,
        format: String,
    },
    Failed(String),
}
struct Job {
    owner: String,
    started: Instant,
    state: State,
}
static JOBS: Mutex<Option<HashMap<String, Job>>> = Mutex::new(None);

fn with_jobs<R>(f: impl FnOnce(&mut HashMap<String, Job>) -> R) -> R {
    let mut jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
    let jobs = jobs.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    jobs.retain(|_, job| now.duration_since(job.started) < JOB_TTL + SERVICE_WAIT);
    f(jobs)
}

/// The agent and conversation a job belongs to: another conversation never
/// sees it, as with the other studio tools' instances.
fn owner_of(app: &str, call: &HostToolCall) -> String {
    format!("{app}\n{}\n{}", call.session_id, call.context_id.as_deref().unwrap_or(""))
}

/// Registers a job for `owner` and returns its id, or why not.
fn start_job(owner: &str) -> Result<String, String> {
    with_jobs(|jobs| register(jobs, owner))
}

/// Admits one more job for `owner` into `jobs`, under the two bounds.
fn register(jobs: &mut HashMap<String, Job>, owner: &str) -> Result<String, String> {
    let open = |job: &Job| matches!(job.state, State::Running);
    if jobs.values().filter(|job| open(job)).count() >= MAX_JOBS {
        return Err("the studio is already generating as many images as it can at once; wait for a job to finish".into());
    }
    if jobs.values().filter(|job| job.owner == owner && open(job)).count() >= MAX_JOBS_PER_OWNER {
        return Err("you already have two image generations running; poll them first".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    jobs.insert(
        id.clone(),
        Job {
            owner: owner.to_string(),
            started: Instant::now(),
            state: State::Running,
        },
    );
    Ok(id)
}

/// What `model.image` answered, decoded.
fn decode(reply: Result<Value, String>) -> State {
    match reply {
        Err(why) => State::Failed(why),
        Ok(value) => {
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
    }
}

/// Waits for the service's answer on its own thread and records it.
fn watch(id: String, answer: Receiver<Result<Value, String>>) {
    std::thread::spawn(move || {
        let outcome = match answer.recv_timeout(SERVICE_WAIT) {
            Ok(reply) => decode(reply),
            Err(RecvTimeoutError::Timeout) => {
                State::Failed("the image provider did not answer in time".into())
            }
            Err(RecvTimeoutError::Disconnected) => {
                State::Failed("the image service went away".into())
            }
        };
        with_jobs(|jobs| {
            if let Some(job) = jobs.get_mut(&id) {
                job.state = outcome;
            }
        });
    });
}

/// The job's state, if it belongs to `owner`.
fn state_of(id: &str, owner: &str) -> Result<State, String> {
    with_jobs(|jobs| lookup(jobs, id, owner))
}

fn lookup(jobs: &HashMap<String, Job>, id: &str, owner: &str) -> Result<State, String> {
    match jobs.get(id) {
        Some(job) if job.owner == owner => Ok(job.state.clone()),
        _ => Err("no such image job in this conversation".into()),
    }
}

/// The directory App Hub hands every host service, where the `model`
/// service keeps its budgets.
fn host_dir() -> Result<std::path::PathBuf, String> {
    Ok(octosense_appstore::data_root_if_set()
        .ok_or("App Hub has no apps root yet")?
        .join(".host"))
}

/// The service call for `args`, under the calling agent's identity.
fn service_call(app: &str, args: &Value) -> Result<ServiceCall, String> {
    Ok(service_call_in(app, args, host_dir()?))
}

/// The `model.image` request `args` ask for, one image, as an agent's tool
/// call rather than the app in the foreground: the service raises no sheet
/// and refuses what would need one.
fn service_call_in(app: &str, args: &Value, host_dir: std::path::PathBuf) -> ServiceCall {
    let mut request = json!({"prompt": args["prompt"], "n": 1});
    for key in ["size", "seed", "class"] {
        if !args[key].is_null() {
            request[key] = args[key].clone();
        }
    }
    ServiceCall {
        app_id: app.to_string(),
        service: "model.image".into(),
        args: request,
        from_sheet: false,
        may_prompt: false,
        host_dir,
    }
}

/// Writes a finished job's image into `scope` and describes it.
fn deliver(
    scope: &scoped::Workspace,
    id: &str,
    bytes: &[u8],
    width: u64,
    height: u64,
    format: &str,
) -> Result<Value, String> {
    use std::io::Write;
    let output = scope.output_image(format)?;
    (&output.file)
        .write_all(bytes)
        .and_then(|()| output.file.sync_all())
        .map_err(|e| format!("could not write the image: {e}"))?;
    let path = output.keep();
    with_jobs(|jobs| {
        jobs.remove(id);
    });
    Ok(json!({"job": id, "state": "done", "path": path, "width": width, "height": height, "format": format}))
}

/// `studio.image`, on the executor's thread: `root` is the caller's
/// kernel-confirmed workspace and `app` the calling agent's app.
pub fn execute(
    call: &HostToolCall,
    root: &Path,
    app: &str,
    cancel: &AtomicBool,
    reply: &ToolReply,
) -> ToolOutcome {
    let run = || -> Result<Value, String> {
        let scope = scoped::Workspace::open(root, call.context_id.as_deref())?;
        let owner = owner_of(app, call);
        let (id, started_now) = match (call.args["job"].as_str(), call.args["prompt"].as_str()) {
            (Some(job), None) => (job.to_string(), false),
            (None, Some(_)) => {
                let id = start_job(&owner)?;
                let (done, answer) = channel();
                let queued = Queued {
                    call: service_call(app, &call.args)?,
                    deadline: Instant::now() + SERVICE_WAIT,
                    done,
                };
                QUEUE.lock().unwrap_or_else(|e| e.into_inner()).push(queued);
                makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
                watch(id.clone(), answer);
                (id, true)
            }
            _ => return Err("give prompt to start an image, or job alone to poll one".into()),
        };
        // Wait for this call's share of the deadline; a running job outlives
        // the call and is polled with its id.
        let deadline = Instant::now()
            + Duration::from_millis(call.timeout_ms.saturating_sub(1_500).max(500)).min(SERVICE_WAIT);
        loop {
            if cancel.load(Ordering::SeqCst) || !reply.is_open() {
                return Err(if started_now {
                    format!("cancelled; the generation continues as job {id}")
                } else {
                    "cancelled".into()
                });
            }
            match state_of(&id, &owner)? {
                State::Done {
                    bytes,
                    width,
                    height,
                    format,
                } => {
                    scope.verify(root, call.context_id.as_deref())?;
                    return deliver(&scope, &id, &bytes, width, height, &format);
                }
                State::Failed(why) => {
                    with_jobs(|jobs| {
                        jobs.remove(&id);
                    });
                    return Err(why);
                }
                State::Running => {}
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
    use super::*;

    fn png_reply(width: u64, height: u64) -> Value {
        let bytes = vec![0x89, b'P', b'N', b'G', 1, 2, 3, 4];
        json!({"b64_json": base64::engine::general_purpose::STANDARD.encode(&bytes),
               "format": "png", "mime": "image/png", "width": width, "height": height})
    }

    #[test]
    fn jobs_are_bounded_per_owner_and_in_all() {
        // On a table of its own: the process-wide one is shared with the
        // other tests of this crate.
        let mut jobs = HashMap::new();
        let a = register(&mut jobs, "a").unwrap();
        register(&mut jobs, "a").unwrap();
        let third = register(&mut jobs, "a").unwrap_err();
        assert!(third.contains("two image generations"), "{third}");
        // Other owners still fit, up to the total of eight.
        for owner in ["b", "b", "c", "c", "d", "d"] {
            register(&mut jobs, owner).unwrap();
        }
        let ninth = register(&mut jobs, "e").unwrap_err();
        assert!(ninth.contains("as many images"), "{ninth}");
        // A finished job frees its slot, and is not another owner's to see.
        jobs.get_mut(&a).unwrap().state = State::Failed("x".into());
        assert!(matches!(lookup(&jobs, &a, "a").unwrap(), State::Failed(_)));
        assert!(lookup(&jobs, &a, "b").is_err());
        register(&mut jobs, "e").unwrap();
    }

    #[test]
    fn a_service_reply_is_decoded_or_refused() {
        match decode(Ok(png_reply(512, 512))) {
            State::Done { bytes, width, height, format } => {
                assert_eq!((bytes.len(), width, height, format.as_str()), (8, 512, 512, "png"));
            }
            _ => panic!("decoded"),
        }
        assert!(matches!(decode(Ok(json!({"format":"png"}))), State::Failed(_)));
        assert!(matches!(decode(Ok(json!({"b64_json":"!!","width":1,"height":1}))), State::Failed(_)));
        assert!(matches!(decode(Err("budget".into())), State::Failed(w) if w == "budget"));
    }

    #[test]
    fn the_declaration_takes_a_prompt_or_a_job_and_nothing_else() {
        let decl = declaration("system");
        let schema = &decl["input_schema"];
        assert!(super::super::schema::check(schema, &json!({"prompt":"a red square"})).is_ok());
        assert!(super::super::schema::check(schema, &json!({"prompt":"x","size":"512x512","seed":7,"class":"fast"})).is_ok());
        assert!(super::super::schema::check(schema, &json!({"job":"j"})).is_ok());
        assert!(super::super::schema::check(schema, &json!({"prompt":"x","size":"640x480"})).is_err());
        assert!(super::super::schema::check(schema, &json!({"prompt":"x","output_path":"a.png"})).is_err());
        assert_eq!(decl["risk"], "act");
        let request = service_call_in("system", &json!({"prompt":"p","size":"512x512","seed":3}), std::env::temp_dir());
        assert_eq!(request.service, "model.image");
        assert_eq!(request.args, json!({"prompt":"p","n":1,"size":"512x512","seed":3}));
        assert!(!request.may_prompt && !request.from_sheet);
    }

    #[test]
    fn the_system_agent_is_admitted_only_under_developer_mode() {
        use super::super::studio::system_caller_admitted;
        assert!(!system_caller_admitted("os.news"));
        // Without developer mode on in this test process, the system agent
        // is refused too; the grant is read live from the controller.
        assert_eq!(system_caller_admitted(SYSTEM), crate::dev_mode::grants_all(SYSTEM));
    }
}
