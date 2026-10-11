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
