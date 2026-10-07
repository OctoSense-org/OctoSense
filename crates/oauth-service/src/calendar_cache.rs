//! Durable, app-bound Calendar agenda snapshots. Every refresh expands a finite
//! window and atomically replaces it; incremental sync tokens are never reused.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_PAGES: usize = 100;
const MAX_EVENTS: usize = 25_000;

/// UTC day boundaries: 30 days before today through 366 days after today.
/// Google expands recurring series inside this finite interval, including
/// exceptions; instance IDs and ETags remain the provider's own identities.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgendaWindow {
    time_min: String,
    time_max: String,
}
impl AgendaWindow {
    pub fn around(now: u64) -> Result<Self, String> {
        let now = i64::try_from(now).map_err(|_| "Invalid Calendar refresh time")?;
        let today = chrono::DateTime::from_timestamp(now, 0)
            .ok_or("Invalid Calendar refresh time")?
            .date_naive();
        let boundary = |days| {
            today
                .checked_add_signed(chrono::Duration::days(days))
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|time| {
                    time.and_utc()
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                })
                .ok_or("Calendar window is outside the supported date range")
        };
        Ok(Self {
            time_min: boundary(-30)?,
            time_max: boundary(366)?,
        })
    }
    pub fn time_min(&self) -> &str {
        &self.time_min
    }
    pub fn time_max(&self) -> &str {
        &self.time_max
    }
    fn includes(&self, event: &Value, calendar_zone: &str) -> Result<bool, String> {
        if event["recurrence"]
            .as_array()
            .is_some_and(|rules| !rules.is_empty())
        {
            return Err(
                "Google returned an unexpanded recurring series; previous agenda is unchanged"
                    .into(),
            );
        }
        let time = |key: &str| -> Result<i64, String> {
            let value = &event[key];
            if let Some(value) = value["dateTime"].as_str() {
                return chrono::DateTime::parse_from_rfc3339(value)
                    .map(|time| time.timestamp())
                    .map_err(|_| "Google returned an invalid event time".into());
            }
            let date = value["date"]
                .as_str()
                .and_then(|value| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
                .ok_or("Google returned an event without a valid time")?;
            let zone: chrono_tz::Tz = value["timeZone"]
                .as_str()
                .unwrap_or(calendar_zone)
                .parse()
                .map_err(|_| "Google returned an invalid all-day event timezone")?;
            use chrono::TimeZone;
            // Some zones advance their clock at midnight. An all-day date
            // starts at that date's first existing minute, unlike a timed
            // appointment whose nonexistent wall-clock time must be rejected.
            let midnight = date.and_hms_opt(0, 0, 0).unwrap();
            (0..24 * 60)
                .find_map(|minute| {
                    zone.from_local_datetime(&(midnight + chrono::Duration::minutes(minute)))
                        .earliest()
                })
                .map(|time| time.timestamp())
                .ok_or_else(|| "Google returned a nonexistent all-day boundary".into())
        };
        let start = time("start")?;
        let end = time("end")?;
        if end <= start {
            return Err("Google returned an invalid event interval".into());
        }
        let min = chrono::DateTime::parse_from_rfc3339(&self.time_min)
            .map_err(|_| "Invalid Calendar window")?
            .timestamp();
        let max = chrono::DateTime::parse_from_rfc3339(&self.time_max)
            .map_err(|_| "Invalid Calendar window")?
            .timestamp();
        Ok(end > min && start < max)
    }
}

/// Convert ordinary editor fields into provider timestamps with an explicit
/// offset. Ambiguous clock changes require a person to choose a clear time;
/// the service never silently chooses one of two possible appointments.
pub fn prepare_draft(input: Value) -> Result<Value, String> {
    use crate::api::{CalendarEvent, EventTime};
    use chrono::{LocalResult, NaiveDate, NaiveTime, TimeZone};
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        summary: String,
        description: String,
        location: String,
        start_date: String,
        start_time: String,
        end_date: String,
        end_time: String,
        timezone: String,
        all_day: bool,
    }
    let fields: Fields =
        serde_json::from_value(input).map_err(|_| "Invalid Calendar editor fields")?;
    let start = NaiveDate::parse_from_str(&fields.start_date, "%Y-%m-%d")
        .map_err(|_| "Enter the start date as YYYY-MM-DD")?;
    let end = NaiveDate::parse_from_str(&fields.end_date, "%Y-%m-%d")
        .map_err(|_| "Enter the end date as YYYY-MM-DD")?;
    let (start, end) = if fields.all_day {
        if end < start {
            return Err("The last day must not be before the first day".into());
        }
        // The editor asks for the last included day; Google uses an exclusive end.
        let exclusive_end = end
            .succ_opt()
            .ok_or("The last day is outside the supported date range")?;
        (
            EventTime {
                date: Some(start.to_string()),
                date_time: None,
                time_zone: None,
            },
            EventTime {
                date: Some(exclusive_end.to_string()),
                date_time: None,
                time_zone: None,
            },
        )
    } else {
        let zone: chrono_tz::Tz = fields.timezone.parse().map_err(|_| {
            "Choose an IANA timezone, for example America/Los_Angeles or Europe/London"
        })?;
        let convert = |date: NaiveDate, time: &str| -> Result<EventTime, String> {
            let time = NaiveTime::parse_from_str(time, "%H:%M")
                .map_err(|_| "Enter times as HH:mm using the 24-hour clock")?;
            let value = match zone.from_local_datetime(&date.and_time(time)) {
                LocalResult::Single(value) => value,
                LocalResult::None => return Err("This local time does not exist because the clocks move forward. Choose a time before or after the clock change.".into()),
                LocalResult::Ambiguous(_,_) => return Err("This local time occurs twice because the clocks move back. Choose a time outside the repeated hour before saving.".into()),
            };
            Ok(EventTime {
                date: None,
                date_time: Some(value.to_rfc3339()),
                time_zone: Some(fields.timezone.clone()),
            })
        };
        (
            convert(start, &fields.start_time)?,
            convert(end, &fields.end_time)?,
        )
    };
    let event = CalendarEvent {
        summary: fields.summary.trim().into(),
        description: fields.description,
        location: fields.location,
        start,
        end,
    };
    event.validate()?;
    serde_json::to_value(event).map_err(|_| "Cannot prepare Calendar event".into())
}

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    schema: u32,
    connection: String,
    calendar: String,
    #[serde(default)]
    window: Option<AgendaWindow>,
    synced_at: u64,
    #[serde(default)]
    timezone: String,
    events: BTreeMap<String, Value>,
}

fn cache_path(root: &Path, app: &str, connection: &str, calendar: &str) -> PathBuf {
    let mut hash = Sha256::new();
    // Length framing prevents different identities from sharing a digest input.
    for value in [app, connection, calendar] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value.as_bytes());
    }
    crate::inbox::app_directory(root, "calendar-cache", app)
        .join(format!("{:x}.json", hash.finalize()))
}

/// Each app owns a separate cache directory, including disconnected accounts.
/// Uninstall can erase it without parsing or trusting cached provider JSON.
pub(crate) fn purge_app(root: &Path, app: &str) -> Result<(), String> {
    crate::inbox::purge_private_app(root, "calendar-cache", app)
}

fn load(path: &Path, connection: &str, calendar: &str) -> Result<Cache, String> {
    if !path.exists() {
        return Ok(Cache {
            schema: 1,
            connection: connection.into(),
            calendar: calendar.into(),
            ..Cache::default()
        });
    }
    if fs::metadata(path)
        .map_err(|_| "Cannot inspect Calendar cache")?
        .len()
        > MAX_BYTES as u64
    {
        return Err("Calendar cache exceeds its size limit".into());
    }
    let cache: Cache =
        serde_json::from_slice(&fs::read(path).map_err(|_| "Cannot read Calendar cache")?)
            .map_err(|_| "Calendar cache is damaged; reconnect or refresh from Google")?;
    if !matches!(cache.schema, 1 | 2)
        || cache.connection != connection
        || cache.calendar != calendar
    {
        return Err("Calendar cache identity does not match this connection".into());
    }
    Ok(cache)
}

fn persist(path: &Path, cache: &Cache) -> Result<(), String> {
    let bytes = serde_json::to_vec(cache).map_err(|_| "Cannot encode Calendar cache")?;
    if bytes.len() > MAX_BYTES {
        return Err("Calendar cache exceeds its size limit".into());
    }
    let parent = path.parent().ok_or("Invalid Calendar cache path")?;
    fs::create_dir_all(parent).map_err(|_| "Cannot create Calendar cache")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Cannot protect Calendar cache")?;
    }
    let temporary = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|_| "Cannot write Calendar cache")?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "Cannot finish Calendar cache")?;
        drop(file);
        fs::rename(&temporary, path).map_err(|_| "Cannot commit Calendar cache")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn string(value: &Value, name: &str) -> String {
    value[name].as_str().unwrap_or("").to_owned()
}

fn snapshot(cache: &Cache) -> Value {
    let mut events: Vec<_> = cache
        .events
        .values()
        .filter(|v| v["status"] != "cancelled")
        .collect();
    events.sort_by_key(|v| {
        let instant = v["start"]["dateTime"]
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|time| time.timestamp())
            .or_else(|| {
                v["start"]["date"]
                    .as_str()
                    .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                    .and_then(|date| date.and_hms_opt(0, 0, 0))
                    .map(|time| time.and_utc().timestamp())
            })
            .unwrap_or(i64::MAX);
        (instant, v["id"].as_str().unwrap_or(""))
    });
    let events: Vec<_> = events.into_iter().map(|event| {
        let zone = event["start"]["timeZone"].as_str().filter(|s| !s.is_empty()).unwrap_or(&cache.timezone).parse::<chrono_tz::Tz>().unwrap_or(chrono_tz::UTC);
        let fields = |key: &str| -> (String,String) {
            if let Some(date) = event[key]["date"].as_str() {
                let date = if key == "end" { chrono::NaiveDate::parse_from_str(date,"%Y-%m-%d").ok().and_then(|date| date.pred_opt()).map(|date| date.to_string()).unwrap_or_default() } else {date.to_owned()};
                return (date,String::new());
            }
            event[key]["dateTime"].as_str().and_then(|time| chrono::DateTime::parse_from_rfc3339(time).ok()).map(|time| {
                let time = time.with_timezone(&zone);
                (time.format("%Y-%m-%d").to_string(),time.format("%H:%M").to_string())
            }).unwrap_or_default()
        };
        let (start_date,start_time) = fields("start");
        let (end_date,end_time) = fields("end");
        json!({
        "id": string(event, "id"), "etag": string(event, "etag"),
        "card_id": format!("event-{:x}", Sha256::digest(format!("{}\0{}\0{}",cache.connection,cache.calendar,string(event,"id"))))[..62].to_owned(),
        "card_title": event["summary"].as_str().filter(|s| !s.is_empty()).unwrap_or("Calendar event").chars().take(80).collect::<String>(),
        "card_summary": format!("{start_date} · {} · {}", if event["start"]["date"].is_string() { "All day" } else { &start_time },string(event,"location")).chars().take(200).collect::<String>(),
        "summary": event["summary"].as_str().unwrap_or("Untitled event"),
        "description": string(event, "description"), "location": string(event, "location"),
        "start": event["start"]["dateTime"].as_str().or(event["start"]["date"].as_str()).unwrap_or(""),
        "end": event["end"]["dateTime"].as_str().or(event["end"]["date"].as_str()).unwrap_or(""),
        "timezone": zone.name(), "start_date":start_date,"start_time":start_time,"end_date":end_date,"end_time":end_time,
        "all_day": event["start"]["date"].is_string(),
        "recurring": event["recurrence"].is_array() || event["recurringEventId"].is_string(),
        "html_url": string(event, "htmlLink"), "updated": string(event, "updated"),
    })}).collect();
    json!({"calendar":cache.calendar,"connection":cache.connection,"synced_at":cache.synced_at,"window":cache.window,"events":events})
}

/// Call only after verifying the caller still owns the connection and Calendar
/// scope. A revoked connection must not expose cached private event data.
pub fn cached(root: &Path, app: &str, connection: &str, calendar: &str) -> Result<Value, String> {
    Ok(snapshot(&load(
        &cache_path(root, app, connection, calendar),
        connection,
        calendar,
    )?))
}

/// Fetch the same finite window on every page. Google's incremental tokens
/// forbid timeMin/timeMax; reusing them here would not preserve this agenda's
/// membership or safely bound recurrence expansion. A failed page leaves the
/// last complete window unchanged. The host serializes operations for this app.
pub fn refresh(
    root: &Path,
    app: &str,
    connection: &str,
    calendar: &str,
    now: u64,
    mut fetch: impl FnMut(&AgendaWindow, Option<&str>) -> Result<Value, String>,
) -> Result<Value, String> {
    let path = cache_path(root, app, connection, calendar);
    let old = load(&path, connection, calendar)?;
    let window = AgendaWindow::around(now)?;
    let mut timezone = old.timezone;
    let mut staged = BTreeMap::new();
    let mut page: Option<String> = None;
    let mut visited = BTreeSet::new();
    for _ in 0..MAX_PAGES {
        let response = fetch(&window, page.as_deref())?;
        if let Some(zone) = response["timeZone"].as_str() {
            timezone = zone.to_owned();
        }
        let items = response["items"]
            .as_array()
            .ok_or("Google returned an invalid Calendar page")?;
        for event in items {
            let id = event["id"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 1024)
                .ok_or("Calendar event has no valid ID")?;
            if event["status"] == "cancelled" {
                continue;
            }
            if window.includes(event, &timezone)? {
                staged.insert(id.into(), event.clone());
            }
        }
        if staged.len() > MAX_EVENTS
            || serde_json::to_vec(&staged)
                .map_err(|_| "Invalid Calendar page")?
                .len()
                > MAX_BYTES
        {
            return Err("Calendar exceeds the local cache limit".into());
        }
        if let Some(next) = response["nextPageToken"].as_str() {
            if next.is_empty() || next.len() > 4096 || !visited.insert(next.to_owned()) {
                return Err("Google repeated an invalid Calendar page token".into());
            }
            page = Some(next.into());
            continue;
        }
        let cache = Cache {
            schema: 2,
            connection: connection.into(),
            calendar: calendar.into(),
            window: Some(window),
            synced_at: now,
            timezone,
            events: staged,
        };
        persist(&path, &cache)?;
        return Ok(snapshot(&cache));
    }
    Err("Calendar refresh exceeded its page limit; previous data is unchanged".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestRoot(PathBuf);
    impl TestRoot {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("octosense-calendar-{}", uuid::Uuid::new_v4())))
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn event(id: &str, name: &str) -> Value {
        json!({"id":id,"etag":"v1","summary":name,"start":{"dateTime":"2026-10-08T09:00:00-07:00","timeZone":"America/Los_Angeles"},"end":{"dateTime":"2026-10-08T10:00:00-07:00"}})
    }
    fn now() -> u64 {
        chrono::DateTime::parse_from_rfc3339("2026-10-06T12:00:00Z")
            .unwrap()
            .timestamp() as u64
    }
    fn seed(root: &Path) {
        refresh(root, "sample", "account", "primary", now(), |_, _| {
            Ok(json!({"items":[event("one","Original")],"timeZone":"America/Los_Angeles","nextSyncToken":"ignored-token"}))
        }).unwrap();
    }
    #[test]
    fn uninstall_purge_erases_disconnected_calendars_only_for_owner() {
        let root = TestRoot::new();
        seed(&root.0);
        for (app, connection, calendar) in [
            ("sample", "disconnected", "second"),
            ("other.app", "account", "primary"),
        ] {
            refresh(&root.0, app, connection, calendar, now(), |_, _| {
                Ok(json!({"items":[event("one","Retained")]}))
            })
            .unwrap();
        }
        purge_app(&root.0, "sample").unwrap();
        assert!(!crate::inbox::app_directory(&root.0, "calendar-cache", "sample").exists());
        assert!(
            cached(&root.0, "sample", "account", "primary").unwrap()["events"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            cached(&root.0, "other.app", "account", "primary").unwrap()["events"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        purge_app(&root.0, "sample").unwrap();
    }
    #[test]
    fn pages_share_exact_window_and_refresh_replaces_instead_of_reusing_tokens() {
        let root = TestRoot::new();
        seed(&root.0);
        let expected = AgendaWindow::around(now()).unwrap();
        let mut calls = 0;
        let result = refresh(
            &root.0,
            "sample",
            "account",
            "primary",
            now() + 1,
            |window, page| {
                assert_eq!(window, &expected);
                calls += 1;
                if calls == 1 {
                    assert_eq!(page, None);
                    Ok(json!({"items":[event("one","Changed")],"nextPageToken":"page2"}))
                } else {
                    assert_eq!(page, Some("page2"));
                    Ok(json!({"items":[event("two","Added")],"nextSyncToken":"must-not-reuse"}))
                }
            },
        )
        .unwrap();
        assert_eq!(result["events"].as_array().unwrap().len(), 2);
        let empty = refresh(
            &root.0,
            "sample",
            "account",
            "primary",
            now() + 2,
            |window, page| {
                assert_eq!(window, &expected);
                assert_eq!(page, None);
                Ok(json!({"items":[]}))
            },
        )
        .unwrap();
        assert!(empty["events"].as_array().unwrap().is_empty());
        let bytes =
            fs::read_to_string(cache_path(&root.0, "sample", "account", "primary")).unwrap();
        assert!(!bytes.contains("sync_token") && !bytes.contains("must-not-reuse"));
    }
    #[test]
    fn failed_later_page_preserves_complete_previous_window() {
        let root = TestRoot::new();
        seed(&root.0);
        let before = cached(&root.0, "sample", "account", "primary").unwrap();
        assert!(refresh(
            &root.0,
            "sample",
            "account",
            "primary",
            now() + 86_400,
            |_, page| {
                if page.is_none() {
                    Ok(json!({"items":[event("one","Incomplete")],"nextPageToken":"page2"}))
                } else {
                    Err("Synthetic offline provider".into())
                }
            }
        )
        .is_err());
        assert_eq!(
            cached(&root.0, "sample", "account", "primary").unwrap(),
            before
        );
    }
    #[test]
    fn rollover_requeries_and_filters_half_open_boundaries_and_recurrent_exceptions() {
        let root = TestRoot::new();
        seed(&root.0);
        let next = now() + 86_400;
        let expected = AgendaWindow::around(next).unwrap();
        assert_eq!(expected.time_min(), "2026-09-07T00:00:00Z");
        assert_eq!(expected.time_max(), "2027-10-08T00:00:00Z");
        let timed = |id: &str, start: &str, end: &str| json!({"id":id,"etag":"v1","start":{"dateTime":start},"end":{"dateTime":end}});
        let mut occurrence = timed(
            "series_20261008T160000Z",
            "2026-10-08T16:00:00Z",
            "2026-10-08T17:00:00Z",
        );
        occurrence["recurringEventId"] = json!("old-series");
        occurrence["summary"] = json!("Upcoming actual instance");
        let result=refresh(&root.0,"sample","account","primary",next,|window,page| {
            assert_eq!(window,&expected); assert!(page.is_none());
            Ok(json!({"timeZone":"UTC","items":[
                timed("past","2026-09-06T20:00:00Z",window.time_min()),
                timed("future",window.time_max(),"2027-10-08T01:00:00Z"),
                timed("overlap","2026-09-06T23:00:00Z","2026-09-07T01:00:00Z"),
                occurrence,
                {"id":"cancelled-instance","recurringEventId":"old-series","status":"cancelled"},
                {"id":"all-day","start":{"date":"2026-10-08"},"end":{"date":"2026-10-09"}}
            ]}))
        }).unwrap();
        let events = result["events"].as_array().unwrap();
        assert_eq!(events.len(), 3);
        assert!(events
            .iter()
            .any(|e| e["id"] == "series_20261008T160000Z" && e["recurring"] == true));
        assert!(!events.iter().any(|e| e["id"] == "one"));
        assert_eq!(result["window"], json!(expected));
        assert_eq!(
            cached(&root.0, "sample", "account", "primary").unwrap(),
            result,
            "restart retains the exact window"
        );
    }
    #[test]
    fn all_day_dates_survive_a_midnight_daylight_saving_gap() {
        use chrono::TimeZone;
        let date = chrono::NaiveDate::from_ymd_opt(2027, 4, 30).unwrap();
        assert!(chrono_tz::Africa::Cairo
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .is_none());
        assert!(AgendaWindow::around(now())
            .unwrap()
            .includes(
                &json!({"start":{"date":"2027-04-30"},"end":{"date":"2027-05-01"}}),
                "Africa/Cairo"
            )
            .unwrap());
    }
    #[test]
    fn unexpanded_series_invalid_pages_and_cycles_never_replace_the_agenda() {
        let root = TestRoot::new();
        seed(&root.0);
        let before = cached(&root.0, "sample", "account", "primary").unwrap();
        let mut master = event(
            "master",
            "Old master must not impersonate an upcoming instance",
        );
        master["recurrence"] = json!(["RRULE:FREQ=DAILY"]);
        assert!(
            refresh(&root.0, "sample", "account", "primary", now(), |_, _| Ok(
                json!({"items":[master]})
            ))
            .is_err()
        );
        assert!(
            refresh(&root.0, "sample", "account", "primary", now(), |_, _| Ok(
                json!({"items":{}})
            ))
            .is_err()
        );
        assert!(
            refresh(&root.0, "sample", "account", "primary", now(), |_, _| Ok(
                json!({"items":[],"nextPageToken":"again"})
            ))
            .is_err()
        );
        assert_eq!(
            cached(&root.0, "sample", "account", "primary").unwrap(),
            before
        );
        for (app, connection, calendar) in [
            ("other", "account", "primary"),
            ("sample", "other", "primary"),
            ("sample", "account", "other"),
        ] {
            assert!(
                cached(&root.0, app, connection, calendar).unwrap()["events"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }
    }

    fn fields(date: &str, start: &str, end: &str) -> Value {
        json!({"summary":"Fixture appointment","description":"Synthetic only","location":"","start_date":date,"start_time":start,"end_date":date,"end_time":end,"timezone":"America/Los_Angeles","all_day":false})
    }
    #[test]
    fn editor_times_use_timezone_offset_and_reject_clock_gaps() {
        let event = prepare_draft(fields("2026-10-08", "09:00", "10:00")).unwrap();
        assert_eq!(event["start"]["dateTime"], "2026-10-08T09:00:00-07:00");
        assert!(prepare_draft(fields("2026-03-08", "02:30", "04:00"))
            .unwrap_err()
            .contains("does not exist"));
        assert!(prepare_draft(fields("2026-11-01", "01:30", "03:00"))
            .unwrap_err()
            .contains("occurs twice"));
        assert!(prepare_draft(fields("2026-10-08", "10:00", "09:00")).is_err());
    }
    #[test]
    fn all_day_editor_last_day_is_inclusive() {
        let mut input = fields("2026-10-08", "", "");
        input["all_day"] = json!(true);
        let event = prepare_draft(input).unwrap();
        assert_eq!(event["start"]["date"], "2026-10-08");
        assert_eq!(event["end"]["date"], "2026-10-09");
    }
    #[test]
    fn snapshots_preserve_timezones_and_bound_glance_metadata() {
        let root = TestRoot::new();
        let mut later = event("later", &"日".repeat(100));
        later["location"] = json!("A".repeat(300));
        later["start"] = json!({"dateTime":"2026-10-08T09:00:00-07:00"});
        let mut earlier = event("earlier", "Earlier instant");
        earlier["start"] = json!({"dateTime":"2026-10-08T10:00:00+02:00"});
        let result = refresh(&root.0,"sample","account","primary",now(),|_,_|Ok(json!({"items":[later,earlier],"timeZone":"America/Los_Angeles","nextSyncToken":"s1"}))).unwrap();
        assert_eq!(result["events"][0]["id"], "earlier");
        assert_eq!(result["events"][1]["start_time"], "09:00");
        assert_eq!(result["events"][1]["timezone"], "America/Los_Angeles");
        assert_eq!(
            result["events"][1]["card_title"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            80
        );
        assert_eq!(
            result["events"][1]["card_summary"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            200
        );
        assert!(result["events"][1]["card_id"].as_str().unwrap().len() <= 64);
    }
}
