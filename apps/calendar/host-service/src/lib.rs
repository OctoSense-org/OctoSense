//! The `calendar` host service: Calendar's events, kept by the host, and
//! the cards Calendar's agent puts on the glance screen.
//!
//! | method | args | answer |
//! |---|---|---|
//! | `calendar.events` | `{from?, to?, limit?}` | `{events: [{id, title, start, end, location, notes}]}`, soonest first |
//! | `calendar.add_event` | `{title, start, end?, location?, notes?}` | `{id, start}` |
//! | `calendar.remove_event` | `{id}` | `{removed}` |
//! | `calendar.notify` | `{event}` or `{title, when, location?, notes?}`, and `{card_id?, priority?}` | `{card_id, replaced, expires_at}` once an event card is on the glance screen, with a notification |
//! | `calendar.agenda` | `{days?}` | the same, for an agenda card: the next three events within `days` (7) |
//!
//! Times are local: `2026-10-02T15:00`, `2026-10-02 15:00`, RFC 3339 (taken
//! to local time) or a date alone (the day, all day). Events live in the
//! host's own directory (`<host_dir>/calendar/events.json`), outside every
//! app's jail.
//!
//! **Only Calendar.** Calendar (`os.calendar`) is the one caller: App Hub's
//! capability list names no `calendar`, so no other app can be granted it,
//! and the shell runs Calendar's agent's `calendar.*` tools here as the
//! system app's own service (`host_tools::script_apps`).
//!
//! **Cards.** Each card is a fixed L0 card (`resources/event.card`,
//! `resources/agenda.card`) filled from the events, published through the
//! shell's glance service as the calling app ([`on_publish_card`]): the
//! model only names the event, it never writes card code.

use chrono::{Duration, Local, NaiveDate, NaiveDateTime};
use octosense_appstore::services::{HostService, Replier, ServiceCall, ServiceHost};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// The one app this service answers.
pub const APP: &str = "os.calendar";

/// Calendar's event card (L0): one event's title, time, place and notes.
pub const EVENT_CARD: &str = include_str!("../resources/event.card");
/// Calendar's agenda card (L0): the next three events.
pub const AGENDA_CARD: &str = include_str!("../resources/agenda.card");

const TITLE_MAX: usize = 80;
const PLACE_MAX: usize = 120;
const NOTES_MAX: usize = 600;
/// Events kept at most (the oldest past ones go first).
const EVENTS_MAX: usize = 500;

/// Publishes one glance card as the calling app: `(app id, glance.publish
/// arguments)`. The shell installs it once at startup; it decides whether
/// the app may publish (the `glance` grant).
pub type CardPublisher = Arc<dyn Fn(&str, Value) -> Result<Value, String> + Send + Sync>;

fn card_publisher() -> &'static Mutex<Option<CardPublisher>> {
    static PUBLISHER: std::sync::OnceLock<Mutex<Option<CardPublisher>>> = std::sync::OnceLock::new();
    PUBLISHER.get_or_init(Default::default)
}

/// Install (or with `None` remove) what the cards are published through.
pub fn on_publish_card(publisher: Option<CardPublisher>) {
    *card_publisher().lock().unwrap_or_else(|e| e.into_inner()) = publisher;
}

/// One event.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub title: String,
    /// Local, `YYYY-MM-DDTHH:MM` (sorts as it reads).
    pub start: String,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub notes: String,
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("").trim()
}

/// A local time from what a person or a model writes.
pub fn parse_time(s: &str) -> Result<NaiveDateTime, String> {
    let s = s.trim();
    for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(t) = NaiveDateTime::parse_from_str(s, format) {
            return Ok(t);
        }
    }
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(t.with_timezone(&Local).naive_local());
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(d.and_hms_opt(0, 0, 0).expect("midnight"));
    }
    Err(format!("{s:?} is not a time: write 2026-10-02T15:00 or 2026-10-02"))
}

fn stamp(t: NaiveDateTime) -> String {
    t.format("%Y-%m-%dT%H:%M").to_string()
}

/// How an event's time reads on a card: `Fri 2 Oct · 15:00–16:00`, a day
/// alone for an event at midnight with no end.
pub fn when_text(start: &str, end: Option<&str>) -> String {
    let Ok(s) = parse_time(start) else { return start.to_string() };
    let day = s.format("%a %-d %b").to_string();
    let all_day = s.format("%H:%M").to_string() == "00:00" && end.is_none();
    if all_day {
        return day;
    }
    match end.and_then(|e| parse_time(e).ok()) {
        Some(e) if e.date() == s.date() => format!("{day} \u{00b7} {}\u{2013}{}", s.format("%H:%M"), e.format("%H:%M")),
        Some(e) => format!("{day} \u{00b7} {} \u{2013} {}", s.format("%H:%M"), e.format("%a %-d %b %H:%M")),
        None => format!("{day} \u{00b7} {}", s.format("%H:%M")),
    }
}

/// An event's day and time for the card's two tiles: `Fri 2 Oct` and
/// `15:00–16:00` (`All day` for a day alone).
pub fn day_and_time(start: &str, end: Option<&str>) -> (String, String) {
    let Ok(s) = parse_time(start) else { return (start.to_string(), String::new()) };
    let day = s.format("%a %-d %b").to_string();
    if s.format("%H:%M").to_string() == "00:00" && end.is_none() {
        return (day, "All day".into());
    }
    let time = match end.and_then(|e| parse_time(e).ok()) {
        Some(e) if e.date() == s.date() => format!("{}\u{2013}{}", s.format("%H:%M"), e.format("%H:%M")),
        Some(e) => format!("{} \u{2013} {}", s.format("%H:%M"), e.format("%-d %b %H:%M")),
        None => s.format("%H:%M").to_string(),
    };
    (day, time)
}

fn store_path(host_dir: &Path) -> std::path::PathBuf {
    host_dir.join("calendar").join("events.json")
}

/// The events kept under `host_dir`, soonest first.
pub fn load(host_dir: &Path) -> Vec<Event> {
    let mut events: Vec<Event> = std::fs::read(store_path(host_dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    events.sort_by(|a, b| a.start.cmp(&b.start).then(a.id.cmp(&b.id)));
    events
}

fn save(host_dir: &Path, events: &[Event]) -> Result<(), String> {
    let path = store_path(host_dir);
    let dir = path.parent().expect("a parent");
    std::fs::create_dir_all(dir).map_err(|e| format!("Calendar cannot keep events: {e}"))?;
    let tmp = dir.join(format!(".events-{}.json", std::process::id()));
    std::fs::write(&tmp, serde_json::to_vec_pretty(events).expect("events serialize")).map_err(|e| format!("Calendar cannot keep events: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("Calendar cannot keep events: {e}"))
}

fn bounded(s: &str, max: usize, what: &str) -> Result<String, String> {
    if s.chars().count() > max {
        return Err(format!("{what} takes at most {max} characters."));
    }
    Ok(s.to_string())
}

/// `calendar.notify`'s `glance.publish` arguments: the event card filled
/// from an event (its day and time, place and notes), opening Calendar,
/// with a notification.
pub fn event_card_args(title: &str, day: &str, time: &str, location: &str, notes: &str, card_id: &str, priority: i64) -> Value {
    json!({
        "card_id": card_id, "title": format!("Calendar \u{00b7} {title}"), "source": EVENT_CARD,
        "data": {"ev": {
            "title": title, "as_of": "Event",
            "metric1_label": "Day", "metric1_value": day,
            "metric2_label": "Time", "metric2_value": if time.is_empty() { "\u{2014}" } else { time },
            "subtitle": if location.is_empty() { "No place given" } else { location },
            "summary": notes,
        }},
        "priority": priority.clamp(0, 100), "open": {"app": "calendar"}, "notify": true,
    })
}

/// `calendar.agenda`'s `glance.publish` arguments: the next three of
/// `events` (soonest first) on the agenda card.
pub fn agenda_card_args(events: &[Event], days: i64, now: NaiveDateTime) -> Value {
    let mut day = json!({"title": "", "as_of": now.format("%a %-d %b").to_string(), "summary": ""});
    for i in 1..=3 {
        let (title, body) = match events.get(i - 1) {
            Some(e) => {
                let when = when_text(&e.start, e.end.as_deref());
                let body = if e.location.is_empty() { when } else { format!("{when} \u{00b7} {}", e.location) };
                (e.title.clone(), body)
            }
            None => (String::new(), String::new()),
        };
        day[format!("pick{i}_title")] = json!(title);
        day[format!("pick{i}_body")] = json!(body);
    }
    day["title"] = json!(match events.len() {
        0 => format!("Nothing in the next {days} days"),
        1 => "Your next event".to_string(),
        n => format!("Your next {} events", n.min(3)),
    });
    if events.len() > 3 {
        day["summary"] = json!(format!("and {} more in the next {days} days", events.len() - 3));
    }
    json!({
        "card_id": "agenda", "title": "Calendar \u{00b7} Agenda", "source": AGENDA_CARD,
        "data": {"day": day}, "priority": 60, "open": {"app": "calendar"}, "notify": true,
    })
}

/// One call, answered: the events under `host_dir`, cards through
/// `publish`. `now` is local time.
pub fn handle(app: &str, method: &str, args: &Value, host_dir: &Path, now: NaiveDateTime, publish: Option<&CardPublisher>) -> Result<Value, String> {
    if app != APP {
        return Err("calendar is Calendar's own service".into());
    }
    let publish_card = |card: Value| match publish {
        Some(p) => p(app, card),
        None => Err("This device shows no glance cards.".into()),
    };
    match method {
        "events" => {
            let from = match text(args, "from") {
                "" => None,
                s => Some(stamp(parse_time(s)?)),
            };
            let to = match text(args, "to") {
                "" => None,
                s => Some(stamp(parse_time(s)?)),
            };
            let limit = args["limit"].as_u64().unwrap_or(50).clamp(1, 200) as usize;
            let events: Vec<Event> = load(host_dir)
                .into_iter()
                .filter(|e| from.as_ref().map_or(true, |f| e.end.as_ref().unwrap_or(&e.start) >= f))
                .filter(|e| to.as_ref().map_or(true, |t| &e.start <= t))
                .take(limit)
                .collect();
            Ok(json!({"events": events}))
        }
        "add_event" => {
            let title = bounded(text(args, "title"), TITLE_MAX, "A title")?;
            if title.is_empty() {
                return Err("Give the event a title.".into());
            }
            let start = stamp(parse_time(text(args, "start"))?);
            let end = match text(args, "end") {
                "" => None,
                s => {
                    let end = stamp(parse_time(s)?);
                    if end < start {
                        return Err("The event ends before it starts.".into());
                    }
                    Some(end)
                }
            };
            let event = Event {
                id: format!("ev-{}", now.and_utc().timestamp_millis()),
                title,
                start: start.clone(),
                end,
                location: bounded(text(args, "location"), PLACE_MAX, "A place")?,
                notes: bounded(text(args, "notes"), NOTES_MAX, "Notes")?,
            };
            let mut events = load(host_dir);
            let id = if events.iter().any(|e| e.id == event.id) { format!("{}-{}", event.id, events.len()) } else { event.id.clone() };
            events.push(Event { id: id.clone(), ..event });
            // Kept bounded: the oldest past events go first.
            while events.len() > EVENTS_MAX {
                events.remove(0);
            }
            save(host_dir, &events)?;
            Ok(json!({"id": id, "start": start}))
        }
        "remove_event" => {
            let id = text(args, "id");
            let mut events = load(host_dir);
            let before = events.len();
            events.retain(|e| e.id != id);
            let removed = events.len() != before;
            if removed {
                save(host_dir, &events)?;
            }
            Ok(json!({"removed": removed}))
        }
        "notify" => {
            let priority = args["priority"].as_i64().unwrap_or(70);
            let (title, (day, time), location, notes, card_id) = match text(args, "event") {
                "" => {
                    let title = bounded(text(args, "title"), TITLE_MAX, "A title")?;
                    let when = bounded(text(args, "when"), TITLE_MAX, "When")?;
                    if title.is_empty() || when.is_empty() {
                        return Err("Name an event (`event`), or give a title and when.".into());
                    }
                    // A time the service reads goes on the tiles; words as written.
                    let day_time = if parse_time(&when).is_ok() { day_and_time(&when, None) } else { (when, String::new()) };
                    let id = format!("event-{}", now.and_utc().timestamp_millis());
                    (title, day_time, bounded(text(args, "location"), PLACE_MAX, "A place")?, bounded(text(args, "notes"), NOTES_MAX, "Notes")?, id)
                }
                id => {
                    let event = load(host_dir).into_iter().find(|e| e.id == id).ok_or_else(|| format!("There is no event {id:?}."))?;
                    let day_time = day_and_time(&event.start, event.end.as_deref());
                    (event.title, day_time, event.location, event.notes, event.id)
                }
            };
            let card_id = match text(args, "card_id") {
                "" => card_id,
                own => own.to_string(),
            };
            publish_card(event_card_args(&title, &day, &time, &location, &notes, &card_id, priority))
        }
        "agenda" => {
            let days = args["days"].as_i64().unwrap_or(7).clamp(1, 60);
            let from = stamp(now);
            let to = stamp(now + Duration::days(days));
            let events: Vec<Event> = load(host_dir).into_iter().filter(|e| e.end.as_ref().unwrap_or(&e.start) >= &from && e.start <= to).collect();
            publish_card(agenda_card_args(&events, days, now))
        }
        other => Err(format!("calendar has no method {other:?}")),
    }
}

/// The service the Card runner (and the agent's tools) call.
pub struct CalendarService;

impl HostService for CalendarService {
    fn family(&self) -> &'static str {
        "calendar"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        let publisher = card_publisher().lock().unwrap_or_else(|e| e.into_inner()).clone();
        let now = Local::now().naive_local();
        let answer = handle(&call.app_id, call.method(), &call.args, &call.host_dir, now, publisher.as_ref());
        reply.send(answer);
    }
}

/// Offer the service (the shell, once at startup).
pub fn register() {
    octosense_appstore::services::register_host_service(Box::new(CalendarService));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("calendar-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn at(s: &str) -> NaiveDateTime {
        parse_time(s).unwrap()
    }

    #[test]
    fn times_read_as_people_and_models_write_them() {
        assert_eq!(stamp(at("2026-10-02T15:00")), "2026-10-02T15:00");
        assert_eq!(stamp(at("2026-10-02 15:00")), "2026-10-02T15:00");
        assert_eq!(stamp(at("2026-10-02")), "2026-10-02T00:00");
        assert!(parse_time("tomorrow").is_err());
        assert_eq!(when_text("2026-10-02T15:00", Some("2026-10-02T16:30")), "Fri 2 Oct \u{00b7} 15:00\u{2013}16:30");
        assert_eq!(when_text("2026-10-02", None), "Fri 2 Oct", "a day alone");
        assert_eq!(day_and_time("2026-10-02T15:00", Some("2026-10-02T16:30")), ("Fri 2 Oct".to_string(), "15:00\u{2013}16:30".to_string()));
        assert_eq!(day_and_time("2026-10-02", None).1, "All day");
    }

    #[test]
    fn events_are_kept_listed_soonest_first_and_removed() {
        let d = dir("events");
        let now = at("2026-10-01T09:00");
        let add = |title: &str, start: &str| handle(APP, "add_event", &json!({"title": title, "start": start, "location": "Room 4"}), &d, now, None).unwrap();
        let late = add("Review", "2026-10-03T10:00");
        add("Standup", "2026-10-02T09:30");
        let listed = handle(APP, "events", &json!({}), &d, now, None).unwrap();
        let titles: Vec<&str> = listed["events"].as_array().unwrap().iter().map(|e| e["title"].as_str().unwrap()).collect();
        assert_eq!(titles, ["Standup", "Review"]);
        let only = handle(APP, "events", &json!({"from": "2026-10-03"}), &d, now, None).unwrap();
        assert_eq!(only["events"].as_array().unwrap().len(), 1);
        assert_eq!(handle(APP, "remove_event", &json!({"id": late["id"]}), &d, now, None).unwrap()["removed"], true);
        assert_eq!(load(&d).len(), 1);
        assert!(handle(APP, "add_event", &json!({"title": "", "start": "2026-10-02"}), &d, now, None).is_err());
        assert!(handle(APP, "add_event", &json!({"title": "x", "start": "2026-10-02T10:00", "end": "2026-10-02T09:00"}), &d, now, None).is_err());
        assert!(handle("os.mail", "events", &json!({}), &d, now, None).is_err(), "Calendar's own service");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cards_are_the_fixed_cards_filled_from_the_events() {
        let d = dir("cards");
        let now = at("2026-10-01T09:00");
        let seen: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
        let log = seen.clone();
        let publisher: CardPublisher = Arc::new(move |app, args| {
            log.lock().unwrap().push((app.to_string(), args.clone()));
            Ok(json!({"card_id": args["card_id"], "replaced": false}))
        });
        let added = handle(APP, "add_event", &json!({"title": "Dentist", "start": "2026-10-02T15:00", "end": "2026-10-02T16:00", "location": "Main St"}), &d, now, None).unwrap();
        handle(APP, "notify", &json!({"event": added["id"]}), &d, now, Some(&publisher)).unwrap();
        handle(APP, "notify", &json!({"title": "Lunch", "when": "2026-10-02T12:30"}), &d, now, Some(&publisher)).unwrap();
        handle(APP, "agenda", &json!({}), &d, now, Some(&publisher)).unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 3);
        assert!(seen.iter().all(|(app, args)| app == APP && args["notify"] == true && args["open"]["app"] == "calendar"));
        let ev = &seen[0].1["data"]["ev"];
        assert_eq!((ev["title"].as_str(), ev["metric1_value"].as_str(), ev["metric2_value"].as_str(), ev["subtitle"].as_str()), (Some("Dentist"), Some("Fri 2 Oct"), Some("15:00\u{2013}16:00"), Some("Main St")));
        assert_eq!(seen[0].1["source"], EVENT_CARD);
        assert_eq!((seen[1].1["data"]["ev"]["metric2_value"].as_str(), seen[1].1["data"]["ev"]["subtitle"].as_str()), (Some("12:30"), Some("No place given")));
        let day = &seen[2].1["data"]["day"];
        assert_eq!((day["title"].as_str(), day["pick1_title"].as_str(), day["pick2_title"].as_str()), (Some("Your next event"), Some("Dentist"), Some("")));
        assert!(handle(APP, "notify", &json!({}), &d, now, Some(&publisher)).is_err(), "names no event");
        let _ = std::fs::remove_dir_all(&d);
    }
}
