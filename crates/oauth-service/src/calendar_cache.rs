//! Durable, app-bound Calendar sync. A partial page sequence never replaces the
//! last usable snapshot or advances its sync token.
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
    sync_token: String,
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
    if cache.schema != 1 || cache.connection != connection || cache.calendar != calendar {
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
    json!({"calendar":cache.calendar,"connection":cache.connection,"synced_at":cache.synced_at,"events":events})
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

/// Fetch pages with the original sync token; atomically install the result only
/// after Google supplies nextSyncToken. One HTTP 410 restarts a complete sync.
/// The caller serializes refreshes for this identity and authorizes each read.
pub fn refresh(
    root: &Path,
    app: &str,
    connection: &str,
    calendar: &str,
    now: u64,
    mut fetch: impl FnMut(Option<&str>, Option<&str>) -> Result<Value, String>,
) -> Result<Value, String> {
    let path = cache_path(root, app, connection, calendar);
    let old = load(&path, connection, calendar)?;
    let mut timezone = old.timezone;
    let mut sync = if old.sync_token.is_empty() {
        None
    } else {
        Some(old.sync_token.clone())
    };
    let mut staged = if sync.is_some() {
        old.events
    } else {
        BTreeMap::new()
    };
    let mut page: Option<String> = None;
    let mut visited = BTreeSet::new();
    let mut reset = false;
    for _ in 0..MAX_PAGES {
        let response = fetch(sync.as_deref(), page.as_deref())?;
        if response["reset_required"] == true {
            if reset || sync.is_none() {
                return Err("Google rejected a full Calendar sync; try again later".into());
            }
            sync = None;
            page = None;
            staged.clear();
            visited.clear();
            reset = true;
            continue;
        }
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
            if event["status"] == "cancelled" && !event["recurringEventId"].is_string() {
                staged.remove(id);
            } else {
                // Retain recurring-instance tombstones in the raw cache. They
                // must remain excluded if a future view expands recurrence.
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
        if let Some(next) = response["nextPageToken"].as_str().filter(|s| !s.is_empty()) {
            if next.len() > 4096 || !visited.insert(next.to_owned()) {
                return Err("Google repeated an invalid Calendar page token".into());
            }
            page = Some(next.into());
            continue;
        }
        let next_sync = response["nextSyncToken"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 4096)
            .ok_or("Google did not complete Calendar synchronization")?;
        let cache = Cache {
            schema: 1,
            connection: connection.into(),
            calendar: calendar.into(),
            sync_token: next_sync.into(),
            synced_at: now,
            timezone,
            events: staged,
        };
        persist(&path, &cache)?;
        return Ok(snapshot(&cache));
    }
    Err("Calendar synchronization exceeded its page limit; previous data is unchanged".into())
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
    fn seed(root: &Path) {
        refresh(root, "sample", "account", "primary", 1, |_, _| {
            Ok(json!({"items":[event("one","Original")],"nextSyncToken":"s1"}))
        })
        .unwrap();
    }
    #[test]
    fn uninstall_purge_erases_disconnected_calendars_only_for_owner() {
        let root = TestRoot::new();
        seed(&root.0);
        for (app, connection, calendar) in [
            ("sample", "disconnected-account", "second-calendar"),
            ("other.app", "account", "primary"),
        ] {
            refresh(&root.0, app, connection, calendar, 1, |_, _| {
                Ok(json!({"items":[event("one","Retained")],"nextSyncToken":"s1"}))
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
    fn pages_commit_together_and_next_sync_reuses_original_token() {
        let root = TestRoot::new();
        seed(&root.0);
        let mut calls = 0;
        let result = refresh(&root.0, "sample", "account", "primary", 2, |sync, page| {
            assert_eq!(sync, Some("s1"));
            calls += 1;
            if calls == 1 {
                assert_eq!(page, None);
                Ok(json!({"items":[event("one","Changed")],"nextPageToken":"p2"}))
            } else {
                assert_eq!(page, Some("p2"));
                Ok(json!({"items":[event("two","Added")],"nextSyncToken":"s2"}))
            }
        })
        .unwrap();
        assert_eq!(result["events"].as_array().unwrap().len(), 2);
        assert_eq!(result["events"][0]["summary"], "Changed");
        refresh(&root.0, "sample", "account", "primary", 3, |sync, _| {
            assert_eq!(sync, Some("s2"));
            Ok(json!({"items":[],"nextSyncToken":"s3"}))
        })
        .unwrap();
    }
    #[test]
    fn failed_second_page_preserves_previous_snapshot_and_token() {
        let root = TestRoot::new();
        seed(&root.0);
        let before = cached(&root.0, "sample", "account", "primary").unwrap();
        assert!(
            refresh(&root.0, "sample", "account", "primary", 2, |_, page| {
                if page.is_none() {
                    Ok(json!({"items":[event("one","Incomplete")],"nextPageToken":"p2"}))
                } else {
                    Err("offline".into())
                }
            })
            .is_err()
        );
        assert_eq!(
            cached(&root.0, "sample", "account", "primary").unwrap(),
            before
        );
        assert_eq!(
            load(
                &cache_path(&root.0, "sample", "account", "primary"),
                "account",
                "primary"
            )
            .unwrap()
            .sync_token,
            "s1"
        );
    }
    #[test]
    fn expired_sync_replaces_cache_only_after_full_success() {
        let root = TestRoot::new();
        seed(&root.0);
        let result = refresh(&root.0, "sample", "account", "primary", 2, |sync, page| {
            assert_eq!(page, None);
            if sync.is_some() {
                Ok(json!({"reset_required":true}))
            } else {
                Ok(json!({"items":[event("two","Full replacement")],"nextSyncToken":"fresh"}))
            }
        })
        .unwrap();
        assert_eq!(result["events"].as_array().unwrap().len(), 1);
        assert_eq!(result["events"][0]["id"], "two");
    }
    #[test]
    fn cancellation_removes_event_and_identities_are_separate() {
        let root = TestRoot::new();
        seed(&root.0);
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
        let result = refresh(&root.0, "sample", "account", "primary", 2, |_, _| {
            Ok(json!({"items":[{"id":"one","status":"cancelled"}],"nextSyncToken":"s2"}))
        })
        .unwrap();
        assert!(result["events"].as_array().unwrap().is_empty());
    }
    #[test]
    fn incomplete_or_cyclic_sequences_do_not_publish() {
        let root = TestRoot::new();
        seed(&root.0);
        assert!(
            refresh(&root.0, "sample", "account", "primary", 2, |_, _| Ok(
                json!({"items":[]})
            ))
            .is_err()
        );
        assert!(
            refresh(&root.0, "sample", "account", "primary", 2, |_, _| Ok(
                json!({"items":[],"nextPageToken":"again"})
            ))
            .is_err()
        );
        assert_eq!(
            cached(&root.0, "sample", "account", "primary").unwrap()["synced_at"],
            1
        );
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
        let result = refresh(&root.0,"sample","account","primary",1,|_,_|Ok(json!({"items":[later,earlier],"timeZone":"America/Los_Angeles","nextSyncToken":"s1"}))).unwrap();
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
