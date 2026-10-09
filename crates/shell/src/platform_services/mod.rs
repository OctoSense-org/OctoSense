//! Device APIs for installed apps, backed by Makepad's native permission bridge.
//!
//! Manifest capability, retained per-app consent, and OS authorization are three
//! different checks. An OS grant to the shell never authorizes another app.
//! All native requests run on the UI thread; agents cannot open permission UI.
use makepad_widgets::{
    makepad_platform::{
        permission::{Permission, PermissionStatus},
        SignalToUI,
    },
    Cx, Event,
};
use octosense_appstore::services::{self, HostService, Replier, ServiceCall, ServiceHost};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

mod consent;
mod location;
mod prompt;
#[cfg(test)]
mod tests;

const DEADLINE: Duration = Duration::from_secs(60);
const REVIEW_TTL: Duration = Duration::from_secs(300);
const MAX_REQUESTS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Operation {
    Status,
    Request,
    Location,
    Sample,
}
struct Work {
    call: ServiceCall,
    reply: Replier,
    family: &'static str,
    permission: Permission,
    operation: Operation,
    grant: consent::Grant,
    deadline: Instant,
    requesting: bool,
    sample: Option<location::Sample>,
}
impl Work {
    fn alive(&self) -> bool {
        if !self.reply.is_pending() {
            return false;
        }
        if Instant::now() >= self.deadline {
            // A deadline can cross after the sample maintenance pass but before
            // generic dequeue/pending cleanup. Reply before that cleanup drops
            // the final owner, otherwise the caller waits the broker's timeout.
            if self.operation == Operation::Sample {
                self.reply.clone().send(Err(location::TIMEOUT_ERROR.into()));
            }
            return false;
        }
        true
    }
    fn policy_allows(&self) -> bool {
        crate::host_tools::script_apps::grants(&self.call.app_id, self.family)
    }
    fn current_grant(&self) -> Result<consent::Grant, String> {
        consent::get(&self.call.host_dir, &self.call.app_id, self.family)
    }
    fn consent_still_valid(&self) -> bool {
        self.current_grant()
            .is_ok_and(|g| g.allowed && g.revision == self.grant.revision)
    }
    fn fail(self, code: &str, message: &str) {
        self.reply.send(Err(format!("{code}: {message}")));
    }
}
struct Review {
    work: Work,
    created: Instant,
}
#[derive(Default)]
struct State {
    queued: VecDeque<Work>,
    pending: HashMap<i32, Work>,
    reviews: HashMap<String, Review>,
    current: HashMap<(String, PathBuf, &'static str), (String, Instant)>,
    background: bool,
    samples: Vec<Work>,
    location_running: bool,
    sample_timer: makepad_widgets::Timer,
}
fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State::default()))
}
fn permission(family: &str) -> Option<Permission> {
    match family {
        "camera" => Some(Permission::Camera),
        "microphone" => Some(Permission::AudioInput),
        "location" => Some(Permission::Location),
        _ => None,
    }
}
fn permission_supported() -> bool {
    cfg!(any(target_os = "android", target_os = "macos"))
}
fn prompt_supported() -> bool {
    cfg!(any(target_os = "android", target_os = "macos"))
}
fn status_name(status: PermissionStatus) -> &'static str {
    match status {
        PermissionStatus::Granted => "granted",
        PermissionStatus::NotDetermined => "not_determined",
        PermissionStatus::DeniedCanRetry => "denied",
        PermissionStatus::DeniedPermanent => "settings_required",
    }
}
fn status(work: &Work, os: &str) -> Value {
    json!({"capability": work.family, "supported": permission_supported(), "app_policy_granted": work.policy_allows(),
        "app_consent": work.current_grant().map(|g| g.allowed).unwrap_or(false), "os_permission": os,
        "location_read_supported": work.family == "location" && cfg!(target_os = "android"),
        "location_sample_supported": work.family == "location" && permission_supported(),
        "scope": "host_api_v1_device_access", "background_permission": false})
}
fn queue(work: Work) {
    let mut state = state().lock().unwrap_or_else(|e| e.into_inner());
    if state.queued.len() + state.pending.len() + state.samples.len() >= MAX_REQUESTS {
        drop(state);
        work.fail("busy", "Too many device requests are pending");
        return;
    }
    state.queued.push_back(work);
    SignalToUI::set_ui_signal();
}
pub fn register() {
    prompt::register();
    makepad_widgets::splash_policy::set_device_permission_gate(|app, family| {
        octosense_appstore::data_root_if_set()
            .is_some_and(|root| consent::cached(&root.join(".host"), app, family))
    });
    for family in ["camera", "microphone", "location"] {
        services::register_host_service(Box::new(DeviceService { family }));
    }
}
struct DeviceService {
    family: &'static str,
}
impl HostService for DeviceService {
    fn family(&self) -> &'static str {
        self.family
    }
    fn api_methods(&self) -> Vec<services::HostApiMethod> {
        use services::{AgentAccess, HostApiMethod};
        if !permission_supported() {
            return vec![];
        }
        let input = json!({"type":"object","additionalProperties":false});
        let output = json!({"type":"object","required":["capability","supported","app_policy_granted","app_consent","os_permission"],
            "properties":{"capability":{"type":"string"},"supported":{"type":"boolean"},"app_policy_granted":{"type":"boolean"},
                "app_consent":{"type":"boolean"},"os_permission":{"enum":["granted","not_determined","denied","settings_required","unsupported"]},
                "location_read_supported":{"type":"boolean"},"location_sample_supported":{"type":"boolean"},"scope":{"const":"host_api_v1_device_access"},"background_permission":{"const":false}}});
        let mut methods = vec![
            HostApiMethod::new(
                format!("{}.permission.status", self.family),
                1,
                self.family,
                "Read this app's device consent and the OS permission without prompting",
                input.clone(),
                output.clone(),
            )
            .with_platforms(&["android", "macos"])
            .with_agent_access(AgentAccess::Allowed),
            HostApiMethod::new(
                format!("{}.permission.request", self.family),
                1,
                self.family,
                "Request app-scoped device consent in a native host sheet, then OS permission",
                input.clone(),
                output.clone(),
            )
            .with_platforms(&["android", "macos"])
            .with_agent_access(AgentAccess::ForegroundOnly),
            HostApiMethod::new(
                format!("{}.permission.revoke", self.family),
                1,
                self.family,
                "Revoke this app's host-api-v1 device access without changing the OS package grant",
                input.clone(),
                json!({"type":"object","required":["app_consent","os_permission_unchanged","scope"],"properties":{
                    "app_consent":{"const":false},"os_permission_unchanged":{"const":true},"scope":{"const":"host_api_v1_device_access"}}}),
            )
            .with_platforms(&["android", "macos"])
            .with_agent_access(AgentAccess::Allowed),
        ];
        if self.family == "location" && cfg!(target_os = "android") {
            methods.push(HostApiMethod::new("location.get",1,"location",
                "Read Android's last-known location after app consent and a fresh OS permission check; fix age is unknown",input,
                json!({"type":"object","required":["latitude","longitude","accuracy_m","source","timestamp","freshness"],"properties":{
                    "latitude":{"type":"number","minimum":-90,"maximum":90},"longitude":{"type":"number","minimum":-180,"maximum":180},
                    "accuracy_m":{"type":"number"},"source":{"const":"last_known"},"timestamp":{"type":"null"},"freshness":{"const":"unknown"}}}))
                .with_platforms(&["android"]).with_agent_access(AgentAccess::Allowed));
        }
        if self.family == "location" {
            methods.extend(location::methods());
        }
        methods
    }
    fn call(&mut self, call: ServiceCall, reply: Replier, host: &mut dyn ServiceHost) {
        if call.method() == "sheet.close" {
            let ticket = call.args["ticket"].as_str().unwrap_or("");
            let key = (call.app_id.clone(), call.host_dir.clone(), self.family);
            let mut state = state().lock().unwrap_or_else(|e| e.into_inner());
            if !call.from_sheet {
                reply.send(Err(
                    "invalid_review: Only the host's device permission sheet may close itself"
                        .into(),
                ));
                return;
            }
            // The broker has already authenticated this exact live sheet heap.
            // Expiry/replacement revokes approval, not the person's ability to
            // dismiss its stale UI. Never cancel a newer review on another
            // surface merely because this older sheet is closing.
            if state.current.get(&key).map(|(ticket, _)| ticket.as_str()) == Some(ticket) {
                state.current.remove(&key);
                if let Some(review) = state.reviews.remove(ticket) {
                    review
                        .work
                        .fail("cancelled", "Device consent was cancelled");
                }
            }
            drop(state);
            host.close_sheet();
            reply.send(Ok(json!({"closed":true})));
            return;
        }
        let sample_options = if self.family == "location" && call.method() == "sample" {
            match location::Options::parse(&call.args) {
                Ok(options) => Some(options),
                Err(error) => {
                    reply.send(Err(error));
                    return;
                }
            }
        } else {
            None
        };
        if sample_options.is_none()
            && (!call.args.is_object()
                || call.args.as_object().is_some_and(|args| !args.is_empty()))
        {
            reply.send(Err(
                "invalid_arguments: Device methods require an empty object".into(),
            ));
            return;
        }
        if !crate::host_tools::script_apps::grants(&call.app_id, self.family) {
            reply.send(Err(
                "permission_denied: The installed app has no grant for this capability".into(),
            ));
            return;
        }
        let opted_in = crate::host_tools::script_apps::guidance(&call.app_id).is_ok_and(|loaded| {
            loaded.manifest["requires"]
                .as_array()
                .is_some_and(|features| features.iter().any(|f| f == "host-api-v1"))
        });
        if !opted_in {
            reply.send(Err("host_requirement_missing: Declare requires: [\"host-api-v1\"] to use device APIs with unified app consent".into()));
            return;
        }
        if self.family == "location" && call.method() == "sample.cancel" {
            if !permission_supported() {
                reply.send(Err(
                    "unsupported_platform: Location sampling is unavailable".into(),
                ));
                return;
            }
            let cancelled = location::cancel(
                &mut state().lock().unwrap_or_else(|e| e.into_inner()),
                &call.app_id,
                &call.host_dir,
            );
            reply.send(Ok(json!({"cancelled":cancelled})));
            return;
        }
        let operation = match call.method() {
            "permission.status" => Operation::Status,
            "permission.request" => Operation::Request,
            "permission.revoke" => {
                reply.send(consent::set(&call.host_dir, &call.app_id, self.family, false, None)
                    .map(|_| json!({"app_consent":false,"os_permission_unchanged":true,"scope":"host_api_v1_device_access"})));
                return;
            }
            "get" if self.family == "location" => Operation::Location,
            "sample" if self.family == "location" => Operation::Sample,
            _ => {
                reply.send(Err(
                    "method_unavailable: This device method is not implemented".into(),
                ));
                return;
            }
        };
        let grant = match consent::get(&call.host_dir, &call.app_id, self.family) {
            Ok(g) => g,
            Err(e) => {
                reply.send(Err(e));
                return;
            }
        };
        let work = Work {
            call,
            reply,
            family: self.family,
            permission: permission(self.family).unwrap(),
            operation,
            grant,
            deadline: Instant::now()
                + sample_options
                    .map(|v| Duration::from_millis(v.timeout_ms))
                    .unwrap_or(DEADLINE),
            requesting: false,
            sample: sample_options.map(|options| location::Sample { options, fix: None }),
        };
        if !permission_supported() {
            if operation == Operation::Status {
                work.reply.clone().send(Ok(status(&work, "unsupported")));
            } else {
                work.fail(
                    "unsupported_platform",
                    "Device APIs are currently implemented on Android and macOS",
                );
            }
            return;
        }
        if operation == Operation::Sample && !work.call.may_prompt {
            work.fail(
                "authorization_required",
                "Location sampling requires the foreground app",
            );
            return;
        }
        if matches!(operation, Operation::Location | Operation::Sample) {
            if operation == Operation::Location && !cfg!(target_os = "android") {
                work.fail(
                    "unsupported_platform",
                    "This host has no admitted location reader on this platform",
                );
                return;
            }
            if !grant.allowed {
                work.fail(
                    "authorization_required",
                    "Open the app and request location permission",
                );
                return;
            }
        }
        if operation == Operation::Request {
            if !work.call.may_prompt {
                work.fail(
                    "authorization_required",
                    "Open the app to request device permission",
                );
                return;
            }
            if !prompt_supported() {
                work.fail(
                    "unsupported_platform",
                    "Trusted device consent is unavailable on this platform",
                );
                return;
            }
            if !grant.allowed {
                let ticket = uuid::Uuid::new_v4().to_string();
                let key = (
                    work.call.app_id.clone(),
                    work.call.host_dir.clone(),
                    self.family,
                );
                let mut state = state().lock().unwrap_or_else(|e| e.into_inner());
                state.reviews.retain(|_, review| {
                    if review.created.elapsed() >= REVIEW_TTL {
                        review
                            .work
                            .reply
                            .clone()
                            .send(Err("timeout: Device consent expired".into()));
                        return false;
                    }
                    review.work.reply.is_pending()
                });
                state
                    .current
                    .retain(|_, (_, created)| created.elapsed() < REVIEW_TTL);
                if state.reviews.len() >= MAX_REQUESTS || state.current.len() >= MAX_REQUESTS {
                    drop(state);
                    work.fail("busy", "Close an earlier permission request");
                    return;
                }
                if let Some(old) = state.current.insert(key, (ticket.clone(), Instant::now())) {
                    if let Some(old) = state.reviews.remove(&old.0) {
                        old.work
                            .fail("cancelled", "A newer permission request replaced this one");
                    }
                }
                state.reviews.insert(
                    ticket.clone(),
                    Review {
                        work,
                        created: Instant::now(),
                    },
                );
                drop(state);
                host.open_sheet(format!(
                    "DevicePermissionPrompt {{ width: Fill height: Fill ticket: {} family: {} }}",
                    json!(ticket),
                    json!(self.family)
                ));
                return;
            }
        }
        queue(work);
    }
}

/// Called by the host's event loop, never by a worker thread or the script VM.
pub fn handle_event(cx: &mut Cx, event: &Event) {
    let mut state = state().lock().unwrap_or_else(|e| e.into_inner());
    location::maintain(&mut state, cx, event);
    state
        .current
        .retain(|_, (_, created)| created.elapsed() < REVIEW_TTL);
    state.reviews.retain(|_, review| {
        if review.created.elapsed() >= REVIEW_TTL {
            review
                .work
                .reply
                .clone()
                .send(Err("timeout: Device consent expired".into()));
            return false;
        }
        review.work.reply.is_pending()
    });
    state.pending.retain(|_, work| work.alive());
    if matches!(event, Event::Pause | Event::Background) {
        for work in state.queued.drain(..) {
            work.fail(
                "cancelled",
                "Device requests stop when the host leaves the foreground",
            );
        }
        // Android's own permission dialog can pause the activity. Preserve
        // that bounded in-flight result, but do not start another OS prompt.
        state.pending.retain(|_, work| {
            if work.requesting {
                return true;
            }
            work.reply
                .clone()
                .send(Err("cancelled: Device request left the foreground".into()));
            false
        });
        state.background = true;
        for (_, review) in state.reviews.drain() {
            review
                .work
                .fail("cancelled", "Device consent was interrupted");
        }
        location::sync(&mut state, cx);
        return;
    }
    if matches!(event, Event::Resume | Event::Foreground) {
        state.background = false;
    }
    if let Event::PermissionResult(result) = event {
        if let Some(mut work) = state.pending.remove(&result.request_id) {
            if work.permission != result.permission {
                work.fail(
                    "permission_mismatch",
                    "The OS returned a different permission",
                );
            } else if !work.policy_allows() {
                work.fail(
                    "permission_denied",
                    "The app was removed or its policy changed",
                );
            } else if work.operation == Operation::Status {
                work.reply
                    .clone()
                    .send(Ok(status(&work, status_name(result.status))));
            } else if !work.consent_still_valid() {
                work.fail("permission_denied", "The app's device consent changed");
            } else if result.status == PermissionStatus::Granted {
                if work.operation == Operation::Sample {
                    if state.background {
                        work.fail("cancelled", "Location sampling left the foreground");
                    } else {
                        location::granted(work, &mut state);
                    }
                } else if work.operation == Operation::Location {
                    let value = makepad_widgets::makepad_platform::gps::last_gps_fix().map(|fix| json!({"latitude":fix.lat,"longitude":fix.lon,"accuracy_m":fix.acc,"source":"last_known","timestamp":null,"freshness":"unknown"}));
                    work.reply.send(value.ok_or_else(|| {
                        "location_unavailable: No last-known device location is available".into()
                    }));
                } else {
                    #[cfg(target_os = "android")]
                    if work.permission == Permission::Location {
                        cx.request_gps_location();
                    }
                    work.reply.clone().send(Ok(status(&work, "granted")));
                }
            } else if work.operation == Operation::Request
                && !work.requesting
                && result.status != PermissionStatus::DeniedPermanent
            {
                work.requesting = true;
                let id = cx.request_permission(work.permission);
                state.pending.insert(id, work);
            } else if matches!(work.operation, Operation::Location | Operation::Sample) {
                work.fail(
                    "authorization_required",
                    "OS location access is not granted; open the app to continue",
                );
            } else {
                work.reply
                    .clone()
                    .send(Ok(status(&work, status_name(result.status))));
            }
        }
    }
    while let Some(work) = state.queued.pop_front() {
        if !work.alive() {
            continue;
        }
        if state.background && matches!(work.operation, Operation::Request | Operation::Sample) {
            work.fail(
                "authorization_required",
                "Return to the foreground app to request permission",
            );
            continue;
        }
        if !work.policy_allows() {
            work.fail(
                "permission_denied",
                "The app was removed or its policy changed",
            );
            continue;
        }
        if work.operation != Operation::Status && !work.consent_still_valid() {
            work.fail("permission_denied", "The app's device consent changed");
            continue;
        }
        let id = cx.check_permission(work.permission);
        state.pending.insert(id, work);
    }
    location::sync(&mut state, cx);
}
