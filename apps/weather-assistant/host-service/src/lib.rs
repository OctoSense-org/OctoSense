//! Daycast's host-owned app storage for its schedule. Keeping it here lets Daycast's UI and its
//! agent use the same private data, while the shell can ask for approval
//! before an agent writes an event.

use chrono::{Local, NaiveDate, NaiveDateTime};
use octosense_appstore::services::{HostService, Replier, ServiceCall, ServiceHost};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const APP: &str = "os.weather-assistant";
pub const AGENT_ADD_TOOL: &str = "daycast.schedule.add";
const EVENTS_MAX: usize = 500;

/// A narrow write tool for Daycast's own agent and for other agents Daycast
/// explicitly grants. The shell's relay always asks before this act runs.
pub fn agent_tool_declaration() -> Value {
    json!({
        "name": AGENT_ADD_TOOL,
        "description": "Add one event to Daycast's private schedule. Require a title, local start and local end in YYYY-MM-DDTHH:mm format. Ask for missing date, time or duration. The host will show the exact event and ask the person before saving; do not claim it was added until this tool returns success.",
        "input_schema": {"type":"object","properties":{
            "title":{"type":"string","minLength":1,"maxLength":80},
            "start":{"type":"string","minLength":1,"maxLength":32},
            "end":{"type":"string","minLength":1,"maxLength":32}
        },"required":["title","start","end"],"additionalProperties":false},
        // Schedule writes are treated as approval-required actions even
        // though they remain private and reversible.
        "risk":"destructive",
        "outward":true,
        "confirm":"host",
        "shareable":true,
        "auto_approvable":false
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub title: String,
    pub start: String,
    pub end: String,
}

fn store_path(host_dir: &Path) -> PathBuf {
    host_dir.join("daycast").join("schedule.json")
}

fn load(host_dir: &Path) -> Result<Vec<Event>, String> {
    let path = store_path(host_dir);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Daycast cannot read its schedule: {error}")),
    };
    let mut events: Vec<Event> = serde_json::from_slice(&bytes).map_err(|error| format!("Daycast's schedule is invalid: {error}"))?;
    events.sort_by(|a, b| a.start.cmp(&b.start).then(a.id.cmp(&b.id)));
    Ok(events)
}

fn save(host_dir: &Path, events: &[Event]) -> Result<(), String> {
    let path = store_path(host_dir);
    let dir = path.parent().expect("schedule path has parent");
    std::fs::create_dir_all(dir).map_err(|e| format!("Daycast cannot save its schedule: {e}"))?;
    let temp = dir.join(format!(".schedule-{}.tmp", std::process::id()));
    std::fs::write(&temp, serde_json::to_vec_pretty(events).expect("events serialize"))
        .map_err(|e| format!("Daycast cannot save its schedule: {e}"))?;
    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("Daycast cannot replace its schedule: {e}"))?;
    }
    std::fs::rename(&temp, &path).map_err(|e| format!("Daycast cannot save its schedule: {e}"))
}

fn text<'a>(args: &'a Value, key: &str) -> &'a str {
    args.get(key).and_then(Value::as_str).unwrap_or("").trim()
}

fn parse_time(value: &str) -> Result<NaiveDateTime, String> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M"))
        .or_else(|_| NaiveDate::parse_from_str(value, "%Y-%m-%d").map(|d| d.and_hms_opt(0, 0, 0).unwrap()))
        .map_err(|_| format!("{value:?} is not a local date/time; use YYYY-MM-DDTHH:mm"))
}

fn stamp(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M").to_string()
}

fn overlaps(a: &Event, start: &str, end: &str) -> bool {
    a.start < end.to_string() && start < a.end.as_str()
}

/// Calls from Daycast's UI and from its contained agent share this store.
/// `schedule.list` may include the app's old local schedule once; it is
/// merged by title and time, making migration safe to repeat.
pub fn handle(app: &str, method: &str, args: &Value, host_dir: &Path) -> Result<Value, String> {
    if app != APP {
        return Err("the Daycast schedule belongs to Daycast".into());
    }
    static STORE: Mutex<()> = Mutex::new(());
    let _guard = STORE.lock().unwrap_or_else(|error| error.into_inner());
    let mut events = load(host_dir)?;
    match method {
        "daycast.schedule.list" => {
            if let Some(legacy) = args.get("legacy").and_then(Value::as_array) {
                for row in legacy {
                    let title = row.get("title").and_then(Value::as_str).unwrap_or("").trim();
                    let start = row.get("start").and_then(Value::as_str).unwrap_or("").trim();
                    let end = row.get("end").and_then(Value::as_str).unwrap_or("").trim();
                    let (Ok(start_time), Ok(end_time)) = (parse_time(start), parse_time(end)) else { continue };
                    let (start, end) = (stamp(start_time), stamp(end_time));
                    if title.is_empty() || start_time >= end_time || events.iter().any(|e| e.title == title && e.start == start && e.end == end) { continue }
                    events.push(Event { id: format!("daycast-{}", Local::now().timestamp_nanos_opt().unwrap_or_default()), title: title.chars().take(80).collect(), start, end });
                }
                if events.len() > EVENTS_MAX { events.truncate(EVENTS_MAX); }
                save(host_dir, &events)?;
            }
            events.sort_by(|a, b| a.start.cmp(&b.start).then(a.id.cmp(&b.id)));
            Ok(json!({"events": events}))
        }
        "daycast.schedule.add" => {
            let title = text(args, "title");
            if title.is_empty() || title.chars().count() > 80 { return Err("A Daycast event title must be 1–80 characters.".into()) }
            if events.len() >= EVENTS_MAX { return Err("Daycast's schedule is full.".into()) }
            let start = parse_time(text(args, "start"))?;
            let end = parse_time(text(args, "end"))?;
            if end <= start { return Err("The event end must be later than its start.".into()) }
            let (start, end) = (stamp(start), stamp(end));
            if events.iter().any(|event| overlaps(event, &start, &end)) {
                return Err("That time overlaps an existing Daycast event. Choose another time.".into());
            }
            let event = Event { id: format!("daycast-{}", Local::now().timestamp_nanos_opt().unwrap_or_default()), title: title.to_string(), start, end };
            events.push(event.clone());
            save(host_dir, &events)?;
            Ok(json!({"event": event}))
        }
        "daycast.schedule.remove" => {
            let id = text(args, "id");
            let before = events.len();
            events.retain(|event| event.id != id);
            if events.len() == before { return Err("That Daycast event no longer exists.".into()) }
            save(host_dir, &events)?;
            Ok(json!({"removed": true}))
        }
        other => Err(format!("Daycast has no method {other:?}")),
    }
}

pub struct DaycastService;

// The shell supplies a narrow bridge to its live Rinx module. This service
// never reads Matrix credentials or takes them from an app.
type RinxBridge = fn(ServiceCall, Replier);
static RINX_BRIDGE: std::sync::OnceLock<RinxBridge> = std::sync::OnceLock::new();

pub fn register_rinx_bridge(bridge: RinxBridge) {
    let _ = RINX_BRIDGE.set(bridge);
}

impl HostService for DaycastService {
    fn family(&self) -> &'static str { "storage" }

    fn timeout(&self, call: &ServiceCall) -> std::time::Duration {
        // Rinx's confirmation is on a different native surface, so App Hub
        // does not see that sheet and would otherwise time out after 60 s.
        std::time::Duration::from_secs(if call.method() == "daycast.rinx.send" { 130 } else { 60 })
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        if call.method().starts_with("daycast.rinx.") {
            match RINX_BRIDGE.get() {
                Some(bridge) => bridge(call, reply),
                None => reply.send(Err("此宿主尚未接入 Daycast 的 Rinx 提醒桥接。".into())),
            }
            return;
        }
        reply.send(handle(&call.app_id, call.method(), &call.args, &call.host_dir));
    }
}

pub fn register() {
    octosense_appstore::services::register_host_service(Box::new(DaycastService));
}
