//! Bounded media APIs. Credentials and routes remain host-owned. Provider calls
//! are never retried automatically: an uncertain POST may already be billable.
use super::{Candidate, Class, Code, Grants, ModelHost, Options, Refusal};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use octosense_appstore::services::{AgentAccess, HostApiMethod, Replier, ServiceCall};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use url::Url;

mod wire;

pub const RESPONSE_MAX: usize = 4 * 1024 * 1024;
pub const ASSET_MAX: usize = 2 * 1024 * 1024;
const JOB_TTL_MS: u64 = 24 * 3600 * 1000;
const POLL_MS: u64 = 5_000;
const DAY_MS: u64 = 24 * 3600 * 1000;
const WORKERS: usize = 4;
const JOBS: usize = 64;

/// Hard resource quotas, separate from chat token accounting. Reservations are
/// charged before submission and retained on failure/cancellation: remote work
/// may have been accepted. A host can lower these, including to zero.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub image_pixels_per_day: u64,
    pub audio_characters_per_day: u64,
    pub video_seconds_per_day: u64,
    pub embedding_bytes_per_day: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            image_pixels_per_day: 8 * 1024 * 1024,
            audio_characters_per_day: 10_000,
            video_seconds_per_day: 12,
            embedding_bytes_per_day: 100_000,
        }
    }
}
impl Limits {
    fn for_kind(self, kind: &str) -> u64 {
        match kind {
            "image" => self.image_pixels_per_day,
            "audio" => self.audio_characters_per_day,
            "video" => self.video_seconds_per_day,
            "embeddings" => self.embedding_bytes_per_day,
            _ => 0,
        }
    }
}

/// Injectable protocol boundary. Implementations must not log headers or body.
/// The service independently checks the returned byte limit, including mocks.
pub trait Transport: Send + Sync {
    fn request(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<&str>,
    ) -> Result<(u16, Vec<u8>), String>;
}
pub struct Http;
impl Transport for Http {
    fn request(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<&str>,
    ) -> Result<(u16, Vec<u8>), String> {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(120))
            .redirects(0)
            .build();
        let mut request = agent.request(method, url);
        for (name, value) in headers {
            request = request.set(name, value);
        }
        let response = match body {
            Some(body) => request.send_string(body),
            None => request.call(),
        };
        let response = match response { Ok(r) => r, Err(ureq::Error::Status(_, r)) => r,
            Err(_) => return Err("The media provider could not be reached; the request may already have been accepted.".into()) };
        let status = response.status();
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(RESPONSE_MAX as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "The media provider response was interrupted.".to_string())?;
        if bytes.len() > RESPONSE_MAX {
            return Err("The media provider response exceeded the host byte limit.".into());
        }
        Ok((status, bytes))
    }
}

#[derive(Clone)]
struct Job {
    app: String,
    root: PathBuf,
    scope: String,
    binding: String,
    provider_id: String,
    created: u64,
    polled: u64,
    cancelled_at: Option<u64>,
    busy: bool,
    duration: u64,
    status: String,
    result: Option<Value>,
}
#[derive(Default)]
struct Data {
    active: BTreeMap<(PathBuf, String), usize>,
    jobs: BTreeMap<String, Job>,
}
pub(super) struct State {
    transport: Arc<dyn Transport>,
    limits: Limits,
    data: Mutex<Data>,
    // One lock serializes bounded on-disk quota read/modify/write. Each profile
    // has a distinct ledger; switching host profiles never merges their data.
    budget_lock: Mutex<()>,
}
impl State {
    pub(super) fn new(options: &Options) -> Self {
        Self {
            transport: options
                .media_transport
                .clone()
                .unwrap_or_else(|| Arc::new(Http)),
            limits: options.media_limits.unwrap_or_default(),
            data: Mutex::new(Data::default()),
            budget_lock: Mutex::new(()),
        }
    }
    fn load_budget(root: &Path, now: u64) -> Result<Value, Refusal> {
        let path = root.join("model/media-ledger.json");
        let mut value = match std::fs::File::open(path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(256 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| budget_error())?;
                if bytes.len() > 256 * 1024 {
                    return Err(budget_error());
                }
                serde_json::from_slice::<Value>(&bytes).map_err(|_| budget_error())?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                json!({"day":now/DAY_MS,"apps":{}})
            }
            Err(_) => return Err(budget_error()),
        };
        if !value["day"].is_u64() || !value["apps"].is_object() {
            return Err(budget_error());
        }
        if value["apps"].as_object().unwrap().values().any(|app| {
            app.as_object().is_none_or(|counts| {
                counts.iter().any(|(kind, count)| {
                    !matches!(kind.as_str(), "image" | "audio" | "video" | "embeddings")
                        || !count.is_u64()
                })
            })
        }) {
            return Err(budget_error());
        }
        if value["day"] != now / DAY_MS {
            value = json!({"day":now/DAY_MS,"apps":{}});
        }
        Ok(value)
    }
    pub(super) fn budget(&self, root: &Path, app: &str, now: u64) -> Value {
        let _lock = self.budget_lock.lock().unwrap();
        match Self::load_budget(root, now) {
            Ok(value) => {
                let mut result = json!({"resets_at":(now/DAY_MS+1)*DAY_MS/1000});
                for kind in ["image", "audio", "video", "embeddings"] {
                    result[kind] = json!({"used":value["apps"][app][kind].as_u64().unwrap_or(0),"limit":self.limits.for_kind(kind)});
                }
                result
            }
            Err(_) => {
                json!({"available":false,"reason":"The media quota ledger could not be read."})
            }
        }
    }
    fn reserve(
        &self,
        root: &Path,
        app: &str,
        now: u64,
        kind: &str,
        units: u64,
    ) -> Result<(), Refusal> {
        let _lock = self.budget_lock.lock().unwrap();
        let mut value = Self::load_budget(root, now)?;
        let used = value["apps"][app][kind].as_u64().unwrap_or(0);
        let next = used.checked_add(units).ok_or_else(budget_error)?;
        if next > self.limits.for_kind(kind) {
            return Err(Refusal::new(
                Code::Budget,
                format!("This app has insufficient {kind} quota today."),
            ));
        }
        if value["apps"].as_object().unwrap().len() >= 1024 && value["apps"].get(app).is_none() {
            return Err(budget_error());
        }
        value["apps"][app][kind] = json!(next);
        let dir = root.join("model");
        std::fs::create_dir_all(&dir).map_err(|_| budget_error())?;
        let path = dir.join(format!(".media-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&path)?;
            file.write_all(value.to_string().as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&path, dir.join("media-ledger.json"))
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(path);
            return Err(budget_error());
        }
        Ok(())
    }
}
fn budget_error() -> Refusal {
    Refusal::new(
        Code::Budget,
        "The host media quota ledger is unavailable; no provider request was sent.",
    )
}
fn bad(message: &str) -> Refusal {
    Refusal::new(Code::BadRequest, message)
}
fn invalid() -> Refusal {
    Refusal::new(
        Code::InvalidOutput,
        "The media provider returned an invalid or unsupported result.",
    )
}
fn cancelled() -> Refusal {
    Refusal::new(
        Code::Capability,
        "The request ended or this app no longer has model access.",
    )
}

pub(super) fn handles(method: &str) -> bool {
    matches!(
        method,
        "image"
            | "audio"
            | "embeddings"
            | "video"
            | "video.status"
            | "video.cancel"
            | "capabilities"
    )
}

struct Permit {
    host: Arc<ModelHost>,
    key: (PathBuf, String),
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut data = self.host.media.data.lock().unwrap();
        if let Some(count) = data.active.get_mut(&self.key) {
            *count -= 1;
            if *count == 0 {
                data.active.remove(&self.key);
            }
        }
    }
}
struct Context {
    host: Arc<ModelHost>,
    grants: Grants,
    call: ServiceCall,
    reply: Replier,
    scope: String,
}
impl Context {
    fn check(&self) -> Result<(), Refusal> {
        if self.reply.is_pending()
            && (self.grants)(&self.call.app_id, &self.call.host_dir)
            && (self.host.scope)(&self.call.app_id, &self.call.host_dir).as_deref()
                == Some(self.scope.as_str())
        {
            Ok(())
        } else {
            Err(cancelled())
        }
    }
    fn providers(&self) -> Result<Vec<Candidate>, Refusal> {
        match &self.host.providers {
            Some(providers) => providers.candidates().map_err(|_| {
                Refusal::new(
                    Code::NoProvider,
                    "The configured AI providers could not be read.",
                )
            }),
            None => Ok(Vec::new()),
        }
    }
    fn call_provider(
        &self,
        route: &wire::Route,
        method: &str,
        suffix: &str,
        body: Option<Value>,
    ) -> Result<Vec<u8>, Refusal> {
        self.check()?;
        // A removed/changed credential or route must not remain usable from a
        // pending job. Resolve on every operation; keep no key in the job table.
        if !self
            .providers()?
            .iter()
            .any(|c| wire::binding(c) == route.binding)
        {
            return Err(cancelled());
        }
        let body = body.map(|v| v.to_string());
        let headers = vec![
            ("Authorization".into(), format!("Bearer {}", route.key)),
            ("Content-Type".into(), "application/json".into()),
        ];
        let (status, bytes) = self.host.media.transport.request(method, &format!("{}{}", route.base, suffix), &headers, body.as_deref())
            .map_err(|_| Refusal::new(Code::Provider, "The media provider request failed; it may already have been accepted. It was not retried."))?;
        self.check()?;
        if !self
            .providers()?
            .iter()
            .any(|c| wire::binding(c) == route.binding)
        {
            return Err(cancelled());
        }
        if bytes.len() > RESPONSE_MAX {
            return Err(Refusal::new(
                Code::TooLarge,
                "The media response exceeds the host byte limit.",
            ));
        }
        if !(200..300).contains(&status) {
            return Err(Refusal::new(
                if status == 429 {
                    Code::Rate
                } else {
                    Code::Provider
                },
                if status == 401 || status == 403 {
                    "The configured provider does not authorize this media operation. Check its account and API entitlement."
                } else {
                    "The media provider refused the request. No automatic retry was made."
                },
            ));
        }
        // Never return the raw provider body/error: it can contain routes, input,
        // account identifiers and arbitrary HTML.
        Ok(bytes)
    }
    fn json(
        &self,
        route: &wire::Route,
        method: &str,
        suffix: &str,
        body: Option<Value>,
    ) -> Result<Value, Refusal> {
        let value: Value =
            serde_json::from_slice(&self.call_provider(route, method, suffix, body)?)
                .map_err(|_| invalid())?;
        if value.get("error").is_some()
            || value
                .get("base_resp")
                .is_some_and(|v| v["status_code"] != 0)
        {
            return Err(Refusal::new(Code::Provider, "The configured provider refused this media operation. Check its API entitlement and quota."));
        }
        Ok(value)
    }
}

pub(super) fn dispatch(host: Arc<ModelHost>, grants: Grants, call: ServiceCall, reply: Replier) {
    let request = match wire::Request::parse(call.method(), &call.args) {
        Ok(r) => r,
        Err(e) => return reply.send(Err(e.to_string())),
    };
    let Some(scope) = (host.scope)(&call.app_id, &call.host_dir) else {
        return reply.send(Err(cancelled().to_string()));
    };
    let key = (call.host_dir.clone(), call.app_id.clone());
    {
        let mut data = host.media.data.lock().unwrap();
        if data.active.values().sum::<usize>() >= WORKERS
            || data.active.get(&key).copied().unwrap_or(0) >= 2
        {
            return reply.send(Err(Refusal::new(
                Code::Rate,
                "Media workers are busy; retry later.",
            )
            .to_string()));
        }
        *data.active.entry(key.clone()).or_default() += 1;
    }
    let permit = Permit {
        host: host.clone(),
        key,
    };
    let failed_reply = reply.clone();
    let context = Context {
        host,
        grants,
        call,
        reply,
        scope,
    };
    if std::thread::Builder::new()
        .name("model-media".into())
        .spawn(move || {
            let _permit = permit;
            let answer = run(&context, request);
            let answer = context.check().and(answer);
            context.reply.send(answer.map_err(|e| e.to_string()));
        })
        .is_err()
    {
        failed_reply.send(Err(
            "provider: The host could not start a media worker.".into()
        ));
    }
}

fn run(context: &Context, request: wire::Request) -> Result<Value, Refusal> {
    context.check()?;
    let now = (context.host.clock)();
    let providers = context.providers()?;
    if matches!(request, wire::Request::Capabilities) {
        let mut result = json!({"version":1,"configured":{},"limits":{"asset_bytes":ASSET_MAX,"response_bytes":RESPONSE_MAX,
            "workers":WORKERS,"video_job_ttl_s":JOB_TTL_MS/1000,"video_poll_after_ms":POLL_MS},
            "note":"Configured route availability is not provider account entitlement. Video jobs last only for this host process."});
        for kind in ["image", "audio", "video", "embeddings"] {
            result["configured"][kind] = json!(wire::route(&providers, kind, Class::Fast).is_ok());
        }
        return Ok(result);
    }
    if let wire::Request::Job { id, cancel } = request {
        return video_poll(context, &providers, &id, cancel, now);
    }
    let kind = request.kind();
    let route = wire::route(&providers, kind, request.class())?;
    let (suffix, body) = wire::prepare(&route, &request)?;
    if let wire::Request::Video { duration, .. } = request {
        return video_start(context, route, suffix, body, duration, now);
    }
    reserve(context, kind, request.units(), now)?;
    let output = if kind == "audio" && route.vendor == wire::Vendor::OpenAi {
        wire::audio(
            &context.call_provider(&route, "POST", suffix, Some(body))?,
            None,
        )?
    } else {
        let response = context.json(&route, "POST", suffix, Some(body))?;
        wire::decode(&request, &route, response)?
    };
    let mut output = output;
    output["meta"] = json!({"class":request.class().as_str(),"budget":context.host.media.budget(&context.call.host_dir,&context.call.app_id,now)});
    Ok(output)
}

fn reserve(context: &Context, kind: &str, units: u64, now: u64) -> Result<(), Refusal> {
    // Both shared call/rate policy and modality resources must admit before any
    // billable request. Failed reservations are conservative, never refunded.
    {
        let mut ledger = context.host.ledger.lock().unwrap();
        ledger.attach(context.call.host_dir.join("model/ledger.json"));
        ledger.admit(&context.call.app_id, now, 0).map_err(|r| {
            Refusal::new(
                if matches!(r, super::ledger::Refused::Rate { .. }) {
                    Code::Rate
                } else {
                    Code::Budget
                },
                "The shared model call budget refused this request.",
            )
        })?;
    }
    context.host.media.reserve(
        &context.call.host_dir,
        &context.call.app_id,
        now,
        kind,
        units,
    )
}

fn video_start(
    context: &Context,
    route: wire::Route,
    suffix: &str,
    body: Value,
    duration: u64,
    now: u64,
) -> Result<Value, Refusal> {
    let id = uuid::Uuid::new_v4().to_string();
    {
        let mut data = context.host.media.data.lock().unwrap();
        data.jobs
            .retain(|_, j| now.saturating_sub(j.created) < JOB_TTL_MS);
        if data.jobs.len() >= JOBS {
            return Err(Refusal::new(
                Code::Rate,
                "The host video job limit is reached; retry after old jobs expire.",
            ));
        }
        data.jobs.insert(
            id.clone(),
            Job {
                app: context.call.app_id.clone(),
                root: context.call.host_dir.clone(),
                scope: context.scope.clone(),
                binding: route.binding.clone(),
                provider_id: String::new(),
                created: now,
                polled: 0,
                cancelled_at: None,
                busy: true,
                duration,
                status: "submitting".into(),
                result: None,
            },
        );
    }
    let result = reserve(context, "video", duration, now)
        .and_then(|_| context.json(&route, "POST", suffix, Some(body)))
        .and_then(|v| {
            let provider_id = v["task_id"]
                .as_str()
                .filter(|id| {
                    !id.is_empty()
                        && id.len() <= 128
                        && id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                })
                .ok_or_else(invalid)?
                .to_string();
            Ok(provider_id)
        });
    let mut data = context.host.media.data.lock().unwrap();
    match result {
        Ok(provider_id) => {
            let job = data.jobs.get_mut(&id).unwrap();
            job.provider_id = provider_id;
            job.status = "queued".into();
            job.busy = false;
            Ok(job_reply(&id, job))
        }
        Err(e) => {
            data.jobs.remove(&id);
            Err(e)
        }
    }
}
fn job_reply(id: &str, job: &Job) -> Value {
    let mut value = json!({"job":id,"status":job.status,"poll_after_ms":POLL_MS,"expires_at":(job.created+JOB_TTL_MS)/1000});
    if let Some(result) = &job.result {
        value["result"] = result.clone();
    }
    value
}
fn video_poll(
    context: &Context,
    providers: &[Candidate],
    id: &str,
    cancel: bool,
    now: u64,
) -> Result<Value, Refusal> {
    let job = {
        let mut data = context.host.media.data.lock().unwrap();
        let job = data
            .jobs
            .get_mut(id)
            .filter(|j| {
                j.app == context.call.app_id
                    && j.root == context.call.host_dir
                    && j.scope == context.scope
                    && now.saturating_sub(j.created) < JOB_TTL_MS
            })
            .ok_or_else(|| bad("Unknown or expired video job for this app."))?;
        if !providers.iter().any(|p| wire::binding(p) == job.binding) {
            job.status = "expired".into();
            job.result = None;
            return Err(cancelled());
        }
        if job.busy {
            return Err(Refusal::new(
                Code::Rate,
                "This video job already has a request pending.",
            ));
        }
        if matches!(
            job.status.as_str(),
            "succeeded" | "failed" | "cancelled" | "expired"
        ) {
            if cancel && job.status != "cancelled" {
                return Err(bad(
                    "This video job has already finished and was not cancelled.",
                ));
            }
            return Ok(job_reply(id, job));
        }
        if cancel
            && job
                .cancelled_at
                .is_some_and(|at| now.saturating_sub(at) < POLL_MS)
        {
            return Err(Refusal::new(
                Code::Rate,
                "Wait before retrying video cancellation.",
            ));
        }
        if !cancel && job.polled != 0 && now.saturating_sub(job.polled) < POLL_MS {
            return Ok(job_reply(id, job));
        }
        job.busy = true;
        if cancel {
            job.cancelled_at = Some(now);
        }
        job.polled = now;
        job.clone()
    };
    let result = (|| {
        let candidate = providers
            .iter()
            .find(|p| wire::binding(p) == job.binding)
            .ok_or_else(cancelled)?;
        let route = wire::route(std::slice::from_ref(candidate), "video", Class::Fast)?;
        let suffix = if cancel {
            format!("/v2/video_generation/{}", job.provider_id)
        } else {
            format!("/v2/query/video_generation/{}", job.provider_id)
        };
        let value = context.json(&route, if cancel { "DELETE" } else { "GET" }, &suffix, None)?;
        if cancel {
            if value["task_id"].as_str() != Some(job.provider_id.as_str())
                || value["action"] != "cancelled"
                || value["status"] != "cancelled"
            {
                return Err(invalid());
            }
            return Ok(("cancelled".to_string(), None));
        }
        let task = &value["task"];
        if task["id"].as_str() != Some(job.provider_id.as_str()) {
            return Err(invalid());
        }
        let status = task["status"]
            .as_str()
            .filter(|s| {
                matches!(
                    *s,
                    "queued" | "running" | "succeeded" | "failed" | "cancelled" | "expired"
                )
            })
            .ok_or_else(invalid)?;
        let output = if status == "succeeded" {
            let url = public_url(task["content"]["url"].as_str().ok_or_else(invalid)?)?;
            let duration = task["duration"]
                .as_u64()
                .filter(|d| *d > 0 && *d <= 15)
                .ok_or_else(invalid)?;
            if duration != job.duration {
                return Err(invalid());
            }
            Some(
                json!({"url":url,"format":"mp4","duration_s":duration,"resolution":task["resolution"].as_str().filter(|s| matches!(*s,"480P"|"768P"|"2K")).ok_or_else(invalid)?}),
            )
        } else {
            None
        };
        Ok((status.to_string(), output))
    })();
    let mut data = context.host.media.data.lock().unwrap();
    let current = data.jobs.get_mut(id).ok_or_else(cancelled)?;
    current.busy = false;
    if let Ok((status, output)) = result.as_ref() {
        current.status = status.clone();
        current.result = output.clone();
    }
    result.map(|_| job_reply(id, current))
}

/// Returned media URLs are data, never navigated with provider credentials or
/// followed by this service. Reject active schemes, local names and IP literals.
fn public_url(raw: &str) -> Result<String, Refusal> {
    if raw.len() > 2048 {
        return Err(invalid());
    }
    let url = Url::parse(raw).map_err(|_| invalid())?;
    let host = url.host_str().ok_or_else(invalid)?.trim_end_matches('.');
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || host.parse::<std::net::IpAddr>().is_ok()
        || host.contains(':')
        || !host.contains('.')
        || host.ends_with(".local")
        || host.ends_with(".localhost")
        || host.ends_with(".internal")
    {
        return Err(invalid());
    }
    Ok(url.into())
}

pub(super) fn catalog() -> Vec<HostApiMethod> {
    wire::schemas()
        .into_iter()
        .map(|(name, summary, args, result)| {
            HostApiMethod::new(name, 1, "model", summary, args, result)
                .with_platforms(&["macos", "android", "windows", "linux", "ios", "openharmony"])
                .with_agent_access(AgentAccess::Allowed)
        })
        .collect()
}
