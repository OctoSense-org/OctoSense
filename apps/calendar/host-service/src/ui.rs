//! Calendar's month/day projection and item navigation. It reads the same
//! events as the agent tools; no UI-owned copy of an event is authoritative.
use super::*;
use chrono::Datelike;
use std::collections::BTreeMap;
use std::path::PathBuf;

static FOCUS: Mutex<BTreeMap<PathBuf, String>> = Mutex::new(BTreeMap::new());

/// A trusted shell navigation request. The target must exist in this store.
pub fn focus_event(host_dir: &Path, id: &str) -> Result<(), String> {
    if !load(host_dir).iter().any(|event| event.id == id) {
        return Err("This event is no longer in Calendar.".into());
    }
    FOCUS
        .lock()
        .unwrap()
        .insert(host_dir.to_path_buf(), id.into());
    Ok(())
}

fn first_of_month(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(&format!("{value}-01"), "%Y-%m-%d")
        .map_err(|_| "Use a month like 2026-10.".into())
}

fn event_on_day(event: &Event, day: NaiveDate) -> bool {
    let Ok(start) = parse_time(&event.start) else {
        return false;
    };
    let end = event
        .end
        .as_deref()
        .and_then(|s| parse_time(s).ok())
        .unwrap_or(start);
    start.date() <= day && end.date() >= day
}

fn event_row(event: &Event) -> Value {
    let mut row = serde_json::to_value(event).unwrap();
    row["timezone"] = json!(event.timezone);
    row["when"] = json!(format!(
        "{}{}",
        when_text(&event.start, event.end.as_deref()),
        event.zone_label()
    ));
    row["time"] = json!(format!(
        "{}{}",
        day_and_time(&event.start, event.end.as_deref()).1,
        event.zone_label()
    ));
    row
}

pub fn view(host_dir: &Path, args: &Value, now: NaiveDateTime) -> Result<Value, String> {
    let events = load(host_dir);
    let zone = events
        .iter()
        .find(|e| !e.timezone.is_empty())
        .map(|e| e.timezone.clone())
        .unwrap_or_default();
    let today = zone
        .parse::<chrono_tz::Tz>()
        .ok()
        .and_then(|tz| {
            Local
                .from_local_datetime(&now)
                .single()
                .map(|n| n.with_timezone(&tz).date_naive())
        })
        .unwrap_or(now.date());
    // Polling reads must not take a route while the person is editing.
    let focus = if args["take_focus"].as_bool().unwrap_or(true) {
        FOCUS.lock().unwrap().remove(host_dir)
    } else {
        None
    };
    let focused = focus
        .as_ref()
        .and_then(|id| events.iter().find(|e| &e.id == id));
    let selected = if let Some(event) = focused {
        parse_time(&event.start)?.date()
    } else if text(args, "day").is_empty() {
        today
    } else {
        NaiveDate::parse_from_str(text(args, "day"), "%Y-%m-%d")
            .map_err(|_| "Use a date like 2026-10-06.")?
    };
    let mut first = if focused.is_some() || text(args, "month").is_empty() {
        selected.with_day(1).unwrap()
    } else {
        first_of_month(text(args, "month"))?
    };
    let direction = args["direction"].as_i64().unwrap_or(0).clamp(-1, 1);
    if direction != 0 {
        first = (first + Duration::days(if direction < 0 { -1 } else { 32 }))
            .with_day(1)
            .unwrap();
    }
    let selected = if first.year() == selected.year() && first.month() == selected.month() {
        selected
    } else {
        first
    };
    let offset = first.weekday().num_days_from_monday() as i64;
    let grid_start = first - Duration::days(offset);
    let mut weeks = Vec::new();
    for week in 0..6 {
        let mut days = Vec::new();
        for column in 0..7 {
            let day = grid_start + Duration::days(week * 7 + column);
            let count = events.iter().filter(|e| event_on_day(e, day)).count();
            days.push(
                json!({"date":day.to_string(), "label":day.day().to_string(),
                "count":count, "marked":count > 0, "selected":day == selected,
                "today":day == today, "in_month":day.month() == first.month()}),
            );
        }
        weeks.push(json!({"days":days}));
    }
    let rows: Vec<Value> = events
        .iter()
        .filter(|e| event_on_day(e, selected))
        .map(event_row)
        .collect();
    use std::hash::{Hash, Hasher};
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(&(&weeks, &rows, &focus, &zone))
        .unwrap()
        .hash(&mut fingerprint);
    Ok(
        json!({"fingerprint":fingerprint.finish().to_string(),"month":first.format("%Y-%m").to_string(), "month_label":first.format("%B %Y").to_string(),
        "day":selected.to_string(), "day_label":selected.format("%A, %-d %B").to_string(),
        "today":today.to_string(), "timezone":zone, "weeks":weeks, "events":rows,
        "focused":focused.map(event_row), "focus_id":focus.unwrap_or_default()}),
    )
}

pub fn update(host_dir: &Path, args: &Value) -> Result<Value, String> {
    let mut events = load(host_dir);
    let event = events
        .iter_mut()
        .find(|e| e.id == text(args, "id"))
        .ok_or("This event is no longer in Calendar.")?;
    let expected: Event = serde_json::from_value(args["expected"].clone())
        .map_err(|_| "Read the event before editing it.")?;
    if *event != expected {
        return Err("This event changed elsewhere. Reload it before saving your changes.".into());
    }
    let mut updated = event.clone();
    updated.title = bounded(text(args, "title"), TITLE_MAX, "A title")?;
    if updated.title.is_empty() {
        return Err("Give the event a title.".into());
    }
    updated.timezone = bounded(text(args, "timezone"), 64, "A timezone")?;
    updated.start = event_stamp(text(args, "start"), &updated.timezone)?;
    updated.end = match text(args, "end") {
        "" => None,
        end => Some(event_stamp(end, &updated.timezone)?),
    };
    if updated.end.as_ref().is_some_and(|end| end < &updated.start) {
        return Err("The event ends before it starts.".into());
    }
    updated.location = bounded(text(args, "location"), PLACE_MAX, "A place")?;
    updated.notes = bounded(text(args, "notes"), NOTES_MAX, "Notes")?;
    *event = updated.clone();
    save(host_dir, &events)?;
    Ok(json!({"event":updated,"card_warning":null}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn booking_marks_the_day_and_navigation_selects_the_same_event() {
        let dir = tests_dir("ui");
        let now = parse_time("2026-10-05T10:00").unwrap();
        let added = handle(APP, "add_event", &json!({"title":"Delivery","start":"2026-10-06T09:00","timezone":"America/Los_Angeles"}), &dir, now, None).unwrap();
        focus_event(&dir, added["id"].as_str().unwrap()).unwrap();
        let screen = view(&dir, &json!({"month":"2026-09","day":"2026-09-01"}), now).unwrap();
        assert_eq!(screen["day"], "2026-10-06");
        assert_eq!(screen["events"][0]["time"], "09:00 PDT");
        assert_eq!(screen["focused"]["id"], added["id"]);
        assert!(screen["weeks"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|w| w["days"].as_array().unwrap())
            .any(|d| d["date"] == "2026-10-06" && d["marked"] == true));
        let expected = screen["focused"].clone();
        let edit = json!({"id":added["id"],"expected":expected,"title":"Delivery","start":"2026-10-06T10:30","timezone":"America/Los_Angeles","notes":"Updated"});
        handle(APP, "update_event", &edit, &dir, now, None).unwrap();
        assert_eq!(load(&dir)[0].start, "2026-10-06T10:30");
        assert!(handle(APP, "update_event", &edit, &dir, now, None)
            .unwrap_err()
            .contains("changed elsewhere"));
        assert!(handle("os.mail", "view", &json!({}), &dir, now, None).is_err());
        assert!(focus_event(&dir, "missing").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    fn tests_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("calendar-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }
}
