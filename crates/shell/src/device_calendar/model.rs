//! Bounded, provider-independent calendar contract. No OS calls in validation.
use serde::{Deserialize, Serialize};
#[cfg(any(test, target_os = "android"))]
use serde_json::json;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) const MAX_ITEMS: usize = 200;
pub(super) const MAX_WINDOW: i64 = 93 * 86_400_000;
pub(super) const MAX_TIME: i64 = 4_102_444_800_000; // 2100-01-01 UTC

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(super) struct Calendar {
    pub id: String,
    pub account: String,
    pub name: String,
    pub account_name: String,
    pub writable: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(super) struct EventData {
    pub id: String,
    pub title: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub timezone: String,
    pub all_day: bool,
    pub location: String,
    pub notes: String,
    pub recurring: bool,
    pub has_attendees: bool,
}
impl EventData {
    pub fn revision(&self) -> String {
        format!("{:x}", Sha256::digest(serde_json::to_vec(self).unwrap()))
    }
    pub fn public(&self) -> Value {
        let mut value = serde_json::to_value(self).unwrap();
        value["revision"] = self.revision().into();
        value
    }
    pub fn editable(&self) -> Result<(), String> {
        if self.recurring || self.has_attendees {
            return Err(
                "unsupported_event: Recurring events and invitations are read-only in version 1"
                    .into(),
            );
        }
        Ok(())
    }
}
// EventKit represents floating all-day dates in the device zone, sometimes
// ending at 23:59:59. The public API represents dates at UTC midnight with an
// exclusive end. Timed events must never pass through these conversions.
pub(super) fn all_day_read_range(start: i64, end: i64, zone: &str) -> Result<(i64, i64), String> {
    use chrono::Timelike;
    let zone: chrono_tz::Tz = zone
        .parse()
        .map_err(|_| "platform_error: Unknown calendar timezone")?;
    let start = chrono::DateTime::from_timestamp_millis(start)
        .ok_or("platform_error: Invalid event date")?
        .with_timezone(&zone);
    let end = chrono::DateTime::from_timestamp_millis(end)
        .ok_or("platform_error: Invalid event date")?
        .with_timezone(&zone);
    let last = if end.time().num_seconds_from_midnight() == 0 && end.time().nanosecond() == 0 {
        end.date_naive()
    } else {
        end.date_naive()
            .succ_opt()
            .ok_or("platform_error: Invalid event end")?
    };
    let first = start
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_millis();
    let last = last
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_millis();
    if last <= first {
        return Err("platform_error: Invalid all-day date range".into());
    }
    Ok((first, last))
}
pub(super) fn all_day_write_range(start: i64, end: i64, zone: &str) -> Result<(i64, i64), String> {
    use chrono::TimeZone;
    let zone: chrono_tz::Tz = zone
        .parse()
        .map_err(|_| "platform_error: Unknown calendar timezone")?;
    let convert = |value| -> Result<i64, String> {
        let date = chrono::DateTime::from_timestamp_millis(value)
            .ok_or("platform_error: Invalid event date")?
            .date_naive();
        zone.from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .single()
            .map(|d| d.timestamp_millis())
            .ok_or_else(|| {
                "unsupported_event: All-day boundary is ambiguous or missing in the device timezone"
                    .into()
            })
    };
    Ok((convert(start)?, convert(end)?))
}
pub(super) fn fields(value: &Value, allowed: &[&str]) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or("invalid_arguments: Expected an object")?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("invalid_arguments: Unknown field".into());
    }
    Ok(())
}
pub(super) fn text(value: &Value, key: &str, max: usize) -> Result<String, String> {
    let text = value[key]
        .as_str()
        .ok_or_else(|| format!("invalid_arguments: {key} must be text"))?;
    if text.is_empty()
        || text.len() > max
        || text
            .chars()
            .any(|c| c == '\0' || (c.is_control() && c != '\n' && c != '\t'))
    {
        return Err(format!("invalid_arguments: Invalid {key}"));
    }
    Ok(text.into())
}
fn optional_text(value: &Value, key: &str, max: usize) -> Result<String, String> {
    match value.get(key) {
        None => Ok(String::new()),
        Some(Value::String(s)) if s.is_empty() => Ok(String::new()),
        _ => text(value, key, max),
    }
}
pub(super) fn event(value: &Value) -> Result<EventData, String> {
    fields(
        value,
        &[
            "title", "start_ms", "end_ms", "timezone", "all_day", "location", "notes",
        ],
    )?;
    let start = value["start_ms"]
        .as_i64()
        .ok_or("invalid_arguments: Invalid start_ms")?;
    let end = value["end_ms"]
        .as_i64()
        .ok_or("invalid_arguments: Invalid end_ms")?;
    window(start, end)?;
    let all_day = match value.get("all_day") {
        None => false,
        Some(Value::Bool(b)) => *b,
        _ => return Err("invalid_arguments: Invalid all_day".into()),
    };
    let timezone = text(value, "timezone", 128)?;
    timezone
        .parse::<chrono_tz::Tz>()
        .map_err(|_| "invalid_arguments: timezone must be a known IANA timezone")?;
    if all_day && (timezone != "UTC" || start % 86_400_000 != 0 || end % 86_400_000 != 0) {
        return Err(
            "invalid_arguments: All-day dates use exclusive UTC midnight boundaries".into(),
        );
    }
    Ok(EventData {
        id: String::new(),
        title: text(value, "title", 512)?,
        start_ms: start,
        end_ms: end,
        timezone,
        all_day,
        location: optional_text(value, "location", 2048)?,
        notes: optional_text(value, "notes", 8192)?,
        recurring: false,
        has_attendees: false,
    })
}
pub(super) fn window(start: i64, end: i64) -> Result<(), String> {
    if start < 0 || end > MAX_TIME || end <= start || end - start > MAX_WINDOW {
        return Err("invalid_arguments: Time window must be positive, within 1970–2100, and at most 93 days".into());
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub(super) enum Command {
    LoadConsent,
    Status,
    Permission,
    Calendars,
    Calendar {
        id: String,
    },
    List {
        calendar: Calendar,
        start: i64,
        end: i64,
        limit: usize,
    },
    Get {
        calendar: Calendar,
        id: String,
    },
    Save {
        calendar: Calendar,
        event: EventData,
        previous: Option<EventData>,
    },
    Delete {
        calendar: Calendar,
        previous: EventData,
    },
    Persist(ConsentChange),
}
#[derive(Clone, Debug)]
pub(super) enum ConsentChange {
    Grant,
    Revoke,
    Select { handle: String, calendar: Calendar },
}
impl Command {
    pub fn needs_consent(&self) -> bool {
        !matches!(
            self,
            Self::LoadConsent
                | Self::Status
                | Self::Permission
                | Self::Persist(ConsentChange::Grant | ConsentChange::Revoke)
        )
    }
    pub fn foreground(&self) -> bool {
        matches!(
            self,
            Self::Permission | Self::Save { .. } | Self::Delete { .. } | Self::Persist(_)
        )
    }
    #[cfg(target_os = "android")]
    pub fn wire(&self) -> Value {
        match self {
            Self::LoadConsent | Self::Persist(_) => {
                unreachable!("consent never leaves the Rust host")
            }
            Self::Status => json!({"operation":"status"}),
            Self::Permission => json!({"operation":"permission"}),
            Self::Calendars => json!({"operation":"calendars"}),
            Self::Calendar { id } => json!({"operation":"calendar","calendar_id":id}),
            Self::List {
                calendar,
                start,
                end,
                limit,
            } => {
                json!({"operation":"list","calendar":calendar,"start_ms":start,"end_ms":end,"limit":limit})
            }
            Self::Get { calendar, id } => {
                json!({"operation":"get","calendar":calendar,"event_id":id})
            }
            Self::Save {
                calendar,
                event,
                previous,
            } => json!({"operation":"save","calendar":calendar,"event":event,"previous":previous}),
            Self::Delete { calendar, previous } => {
                json!({"operation":"delete","calendar":calendar,"previous":previous})
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unbounded_and_invented_event_fields() {
        let mut e = json!({"title":"Synthetic visit","start_ms":1_792_000_000_000i64,"end_ms":1_792_003_600_000i64,"timezone":"America/Los_Angeles"});
        assert!(event(&e).is_ok());
        e["timezone"] = "Mars/Olympus".into();
        assert!(event(&e).is_err());
        e["timezone"] = "America/Los_Angeles".into();
        e["attendees"] = json!(["synthetic@example.invalid"]);
        assert!(event(&e).is_err());
        assert!(window(0, MAX_WINDOW + 1).is_err());
        assert!(window(1, 0).is_err());
        assert!(window(-1, 100).is_err());
    }
    #[test]
    fn all_day_has_unambiguous_exclusive_utc_bounds() {
        let mut e = json!({"title":"Synthetic day","start_ms":86400000,"end_ms":172800000,"timezone":"UTC","all_day":true});
        assert!(event(&e).is_ok());
        e["timezone"] = "America/Los_Angeles".into();
        assert!(event(&e).is_err());
    }
    #[test]
    fn eventkit_inclusive_end_preserves_observed_civil_date() {
        assert_eq!(
            all_day_read_range(1792166400000, 1792252799000, "Asia/Shanghai").unwrap(),
            (1792195200000, 1792281600000)
        );
        // Providers may already return the exclusive midnight instead.
        assert_eq!(
            all_day_read_range(1792166400000, 1792252800000, "Asia/Shanghai").unwrap(),
            (1792195200000, 1792281600000)
        );
    }
    #[test]
    fn all_day_write_and_read_roundtrip_east_west_and_dst() {
        use chrono::NaiveDate;
        for (zone, date, days) in [
            ("Asia/Shanghai", "2026-10-17", 1),
            ("America/Los_Angeles", "2026-03-08", 1),
            ("America/Los_Angeles", "2026-11-01", 1),
            ("Europe/Berlin", "2026-03-28", 3),
            ("Pacific/Auckland", "2026-09-27", 1),
            ("UTC", "2026-10-17", 1),
        ] {
            let first = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc()
                .timestamp_millis();
            let last = first + days * 86_400_000;
            let raw = all_day_write_range(first, last, zone).unwrap();
            assert_eq!(
                all_day_read_range(raw.0, raw.1, zone).unwrap(),
                (first, last),
                "{zone} {date}"
            );
            assert_eq!(
                all_day_read_range(raw.0, raw.1 - 1000, zone).unwrap(),
                (first, last),
                "inclusive {zone} {date}"
            );
        }
    }
    #[test]
    fn existing_long_all_day_event_is_not_rejected_by_draft_duration_limit() {
        let first = 1792195200000;
        let last = first + 120 * 86_400_000;
        assert_eq!(
            all_day_read_range(first, last, "UTC").unwrap(),
            (first, last)
        );
    }
    #[test]
    fn unsafe_or_missing_local_midnight_is_rejected() {
        assert!(all_day_read_range(0, 1000, "Mars/Olympus").is_err());
        assert!(all_day_read_range(1792166400000, 1792166400000, "Asia/Shanghai").is_err());
        let first = chrono::NaiveDate::from_ymd_opt(2011, 12, 30)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        assert!(all_day_write_range(first, first + 86_400_000, "Pacific/Apia").is_err());
    }
    #[test]
    fn revision_detects_changes_and_complex_events_are_read_only() {
        let mut e =
            event(&json!({"title":"Synthetic","start_ms":1000,"end_ms":2000,"timezone":"UTC"}))
                .unwrap();
        let revision = e.revision();
        e.notes = "Updated by another calendar app".into();
        assert_ne!(revision, e.revision());
        e.recurring = true;
        assert!(e.editable().is_err());
        e.recurring = false;
        e.has_attendees = true;
        assert!(e.editable().is_err());
    }
}
