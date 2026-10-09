//! Bounded foreground sampling. No subscription or background-location contract.
use super::*;
use makepad_widgets::makepad_platform::event::{LocationErrorEvent, LocationUpdateEvent};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) const TIMEOUT_ERROR: &str = "location_timeout: No location meeting the requested age and accuracy arrived before the deadline";

#[derive(Clone, Copy, Debug)]
pub(super) struct Options {
    pub timeout_ms: u64,
    pub max_age_ms: u64,
    pub max_accuracy_m: Option<f64>,
}
impl Options {
    pub fn parse(value: &Value) -> Result<Self, String> {
        let invalid = || {
            "invalid_arguments: location.sample accepts timeout_ms (1..30000), max_age_ms (1..60000) and max_accuracy_m (0..100000, exclusive of zero)".to_string()
        };
        let args = value.as_object().ok_or_else(invalid)?;
        if args
            .keys()
            .any(|key| !matches!(key.as_str(), "timeout_ms" | "max_age_ms" | "max_accuracy_m"))
        {
            return Err(invalid());
        }
        let integer = |name: &str, default, max| -> Result<u64, String> {
            match args.get(name) {
                None => Ok(default),
                Some(v) => v
                    .as_u64()
                    .filter(|v| *v > 0 && *v <= max)
                    .ok_or_else(invalid),
            }
        };
        let max_accuracy_m = match args.get("max_accuracy_m") {
            None => None,
            Some(v) => Some(
                v.as_f64()
                    .filter(|v| v.is_finite() && *v > 0.0 && *v <= 100_000.0)
                    .ok_or_else(invalid)?,
            ),
        };
        Ok(Self {
            timeout_ms: integer("timeout_ms", 10_000, 30_000)?,
            max_age_ms: integer("max_age_ms", 5_000, 60_000)?,
            max_accuracy_m,
        })
    }
    pub fn value(&self, fix: &LocationUpdateEvent, now: f64) -> Option<Value> {
        if !now.is_finite()
            || !fix.time.is_finite()
            || fix.time <= 0.0
            || !fix.lat.is_finite()
            || fix.lat.abs() > 90.0
            || !fix.lon.is_finite()
            || fix.lon.abs() > 180.0
            || !fix.accuracy_m.is_finite()
            || fix.accuracy_m < 0.0
            || self.max_accuracy_m.is_some_and(|max| fix.accuracy_m > max)
        {
            return None;
        }
        // Clock adjustments cannot turn an arbitrary future fix into fresh data.
        let age_ms = (now - fix.time) * 1000.0;
        if age_ms < -1000.0 || age_ms > self.max_age_ms as f64 {
            return None;
        }
        Some(
            json!({"latitude":fix.lat,"longitude":fix.lon,"accuracy_m":fix.accuracy_m,
            "timestamp":fix.time,"age_ms":age_ms.max(0.0).ceil() as u64,"source":"platform","freshness":"fresh"}),
        )
    }
}
#[derive(Clone, Debug)]
pub(super) struct Sample {
    pub options: Options,
    pub fix: Option<LocationUpdateEvent>,
}
fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs_f64())
        .unwrap_or(f64::NAN)
}
pub(super) fn methods() -> Vec<services::HostApiMethod> {
    use services::{AgentAccess, HostApiMethod};
    vec![
        HostApiMethod::new("location.sample", 1, "location",
            "Wait for a bounded fresh foreground location fix after existing app consent and OS authorization; never opens permission UI",
            json!({"type":"object","additionalProperties":false,"properties":{
                "timeout_ms":{"type":"integer","minimum":1,"maximum":30000,"default":10000},
                "max_age_ms":{"type":"integer","minimum":1,"maximum":60000,"default":5000},
                "max_accuracy_m":{"type":"number","exclusiveMinimum":0,"maximum":100000}}}),
            json!({"type":"object","required":["latitude","longitude","accuracy_m","timestamp","age_ms","source","freshness"],"properties":{
                "latitude":{"type":"number","minimum":-90,"maximum":90},"longitude":{"type":"number","minimum":-180,"maximum":180},
                "accuracy_m":{"type":"number","minimum":0},"timestamp":{"type":"number","description":"Unix seconds"},
                "age_ms":{"type":"integer","minimum":0},"source":{"const":"platform"},"freshness":{"const":"fresh"}}}))
            .with_platforms(&["android","macos"]).with_agent_access(AgentAccess::ForegroundOnly),
        HostApiMethod::new("location.sample.cancel", 1, "location", "Cancel this app's pending location samples",
            json!({"type":"object","additionalProperties":false}),
            json!({"type":"object","required":["cancelled"],"properties":{"cancelled":{"type":"integer","minimum":0}}}))
            .with_platforms(&["android","macos"]).with_agent_access(AgentAccess::Allowed),
    ]
}
fn is_sample(work: &Work) -> bool {
    work.operation == Operation::Sample
}
fn keep(work: &Work, check_authority: bool) -> bool {
    if !work.alive() {
        return false;
    }
    let failure = if check_authority && (!work.policy_allows() || !work.consent_still_valid()) {
        Some("permission_denied: Location consent or the app's capability changed")
    } else {
        None
    };
    if let Some(error) = failure {
        work.reply.clone().send(Err(error.into()));
        false
    } else {
        true
    }
}
pub(super) fn cancel(state: &mut State, app: &str, root: &Path) -> usize {
    let mut cancelled = 0;
    let mut retain = |work: &Work| {
        if is_sample(work) && work.call.app_id == app && work.call.host_dir == root {
            work.reply
                .clone()
                .send(Err("cancelled: Location sampling was cancelled".into()));
            cancelled += 1;
            false
        } else {
            true
        }
    };
    state.queued.retain(&mut retain);
    state.pending.retain(|_, work| retain(work));
    state.samples.retain(retain);
    SignalToUI::set_ui_signal();
    cancelled
}
/// Permission is checked before starting, and again immediately before delivery.
pub(super) fn granted(mut work: Work, state: &mut State) {
    if !keep(&work, false) {
        return;
    }
    let sample = work.sample.as_mut().unwrap();
    if let Some(value) = sample
        .fix
        .take()
        .and_then(|fix| sample.options.value(&fix, now()))
    {
        work.reply.send(Ok(value));
    } else {
        state.samples.push(work);
    }
}
pub(super) fn maintain(state: &mut State, cx: &mut Cx, event: &Event) {
    let authority = state.sample_timer.is_event(event).is_some()
        || matches!(
            event,
            Event::Signal | Event::LocationUpdate(_) | Event::LocationError(_)
        );
    state
        .queued
        .retain(|work| !is_sample(work) || keep(work, authority));
    state
        .pending
        .retain(|_, work| !is_sample(work) || keep(work, authority));
    state.samples.retain(|work| keep(work, authority));
    match event {
        Event::Pause | Event::Background => {
            for work in state.samples.drain(..) {
                work.fail(
                    "cancelled",
                    "Location sampling stops when the host leaves the foreground",
                );
            }
        }
        Event::LocationUpdate(fix) if !state.background => {
            let time = now();
            for mut work in std::mem::take(&mut state.samples) {
                let sample = work.sample.as_mut().unwrap();
                if sample.options.value(fix, time).is_some() {
                    sample.fix = Some(fix.clone());
                    let id = cx.check_permission(Permission::Location);
                    state.pending.insert(id, work);
                } else {
                    state.samples.push(work);
                }
            }
        }
        Event::LocationError(error) => {
            let (code, message) = match error {
                LocationErrorEvent::PermissionDenied => (
                    "authorization_required",
                    "OS location access is not granted; open the app to authorize it",
                ),
                LocationErrorEvent::Unavailable(_) => (
                    "location_unavailable",
                    "The OS could not provide a location fix",
                ),
            };
            for work in state.samples.drain(..) {
                work.fail(code, message);
            }
            state.pending.retain(|_, work| {
                if is_sample(work) {
                    work.reply.clone().send(Err(format!("{code}: {message}")));
                    false
                } else {
                    true
                }
            });
        }
        _ => {}
    }
}
pub(super) fn sync(state: &mut State, cx: &mut Cx) {
    let wanted = !state.background && !state.samples.is_empty();
    if wanted != state.location_running {
        if wanted {
            cx.start_location_updates_no_prompt();
        } else {
            cx.stop_location_updates_no_prompt();
        }
        state.location_running = wanted;
    }
    let pending = !state.samples.is_empty()
        || state.queued.iter().any(is_sample)
        || state.pending.values().any(is_sample);
    if pending && state.sample_timer.0 == 0 {
        state.sample_timer = cx.start_interval(0.25);
    } else if !pending && state.sample_timer.0 != 0 {
        cx.stop_timer(state.sample_timer);
        state.sample_timer = Default::default();
    }
}
