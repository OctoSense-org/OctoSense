//! Finite default-policy and native time-schedule observations. No Android rule IDs or URIs.
use makepad_strict_json::Value;

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub struct DndKey(String);
impl DndKey {
    pub fn wire(&self) -> &str {
        &self.0
    }
    pub(crate) fn decode(v: &Value) -> Option<Self> {
        let s = v.as_str()?;
        (s.len() == 64
            && s.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)))
        .then(|| Self(s.into()))
    }
}
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyField {
    Calls,
    Messages,
    Conversations,
    RepeatCallers,
    Alarms,
    Media,
    System,
    Reminders,
    Events,
}
impl PolicyField {
    pub const ALL: [Self; 9] = [
        Self::Calls,
        Self::Messages,
        Self::Conversations,
        Self::RepeatCallers,
        Self::Alarms,
        Self::Media,
        Self::System,
        Self::Reminders,
        Self::Events,
    ];
    pub fn wire(self) -> &'static str {
        match self {
            Self::Calls => "calls",
            Self::Messages => "messages",
            Self::Conversations => "conversations",
            Self::RepeatCallers => "repeat_callers",
            Self::Alarms => "alarms",
            Self::Media => "media",
            Self::System => "system",
            Self::Reminders => "reminders",
            Self::Events => "events",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Calls => "Calls",
            Self::Messages => "Messages",
            Self::Conversations => "Conversations",
            Self::RepeatCallers => "Repeated callers",
            Self::Alarms => "Alarms",
            Self::Media => "Media",
            Self::System => "System sounds",
            Self::Reminders => "Reminders",
            Self::Events => "Calendar events",
        }
    }
    fn decode(v: &Value) -> Option<Self> {
        Self::ALL.into_iter().find(|f| Some(f.wire()) == v.as_str())
    }
    pub fn choices(self) -> &'static [PolicyValue] {
        use PolicyValue::*;
        match self {
            Self::Calls | Self::Messages => &[Anyone, Contacts, Starred, None],
            Self::Conversations => &[All, Important, None],
            _ => &[Off, On],
        }
    }
    pub fn label_value(self, value: PolicyValue) -> &'static str {
        if value == PolicyValue::None && matches!(self, Self::Calls | Self::Messages) {
            "No one"
        } else {
            value.label()
        }
    }
}
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyValue {
    Anyone,
    Contacts,
    Starred,
    None,
    All,
    Important,
    Off,
    On,
}
impl PolicyValue {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Anyone => "anyone",
            Self::Contacts => "contacts",
            Self::Starred => "starred",
            Self::None => "none",
            Self::All => "all",
            Self::Important => "important",
            Self::Off => "off",
            Self::On => "on",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Anyone => "Anyone",
            Self::Contacts => "Contacts",
            Self::Starred => "Starred contacts",
            Self::None => "None",
            Self::All => "All",
            Self::Important => "Important",
            Self::Off => "Off",
            Self::On => "On",
        }
    }
}
#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub struct DndRead {
    pub offset: u32,
    pub generation: Option<DndKey>,
}
impl DndRead {
    pub fn valid(&self) -> bool {
        self.offset < 256
            && self.offset % 20 == 0
            && (self.offset == 0 || self.generation.is_some())
    }
    pub fn accepts(&self, state: &DndSnapshot) -> bool {
        (state.stale && state.offset == 0)
            || (!state.stale
                && self
                    .generation
                    .as_ref()
                    .is_none_or(|g| g == &state.generation)
                && (state.offset == self.offset || state.offset == 0 && self.offset >= state.total))
    }
}
fn optional<T>(v: Option<&Value>, f: impl FnOnce(&Value) -> Option<T>) -> Option<Option<T>> {
    match v {
        None | Some(Value::Null) => Some(None),
        Some(v) => f(v).map(Some),
    }
}
fn text(v: &Value, max: usize) -> Option<String> {
    let s = v.as_str()?;
    (!s.trim().is_empty() && s.chars().count() <= max && !s.chars().any(char::is_control))
        .then(|| s.into())
}
fn uint(v: &Value, max: u32) -> Option<u32> {
    v.as_i64()
        .filter(|n| *n >= 0 && *n <= max as i64)
        .map(|n| n as u32)
}
pub fn time(value: &str) -> Option<u16> {
    let b = value.as_bytes();
    if b.len() != 5 || b[2] != b':' || ![0, 1, 3, 4].iter().all(|i| b[*i].is_ascii_digit()) {
        return None;
    }
    let h = (b[0] - b'0') as u16 * 10 + (b[1] - b'0') as u16;
    let m = (b[3] - b'0') as u16 * 10 + (b[4] - b'0') as u16;
    (h < 24 && m < 60).then_some(h * 60 + m)
}
pub fn format_time(value: u16) -> String {
    format!("{:02}:{:02}", value / 60, value % 60)
}
#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Schedule {
    pub name: String,
    pub days: Vec<u8>,
    pub start: u16,
    pub end: u16,
    pub exit_at_alarm: bool,
    pub enabled: bool,
}
impl Schedule {
    pub fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.chars().count() <= 100
            && !self.name.chars().any(char::is_control)
            && self.start < 1440
            && self.end < 1440
            && self.days.len() <= 7
            && self.days.iter().all(|d| (1..=7).contains(d))
            && self.days.windows(2).all(|w| w[0] < w[1])
            && (!self.enabled || !self.days.is_empty())
    }
}
#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub enum DndRequest {
    Snapshot(DndRead),
    Policy {
        key: DndKey,
        field: PolicyField,
        value: PolicyValue,
    },
    Save {
        key: DndKey,
        target: Option<DndKey>,
        schedule: Schedule,
    },
    Enabled {
        key: DndKey,
        target: DndKey,
        enabled: bool,
    },
    Delete {
        key: DndKey,
        target: DndKey,
    },
}
impl DndRequest {
    pub fn valid(&self) -> bool {
        match self {
            Self::Snapshot(r) => r.valid(),
            Self::Policy { field, value, .. } => field.choices().contains(value),
            Self::Save { schedule, .. } => schedule.valid(),
            _ => true,
        }
    }
    pub fn key(&self) -> Option<&DndKey> {
        match self {
            Self::Snapshot(_) => None,
            Self::Policy { key, .. }
            | Self::Save { key, .. }
            | Self::Enabled { key, .. }
            | Self::Delete { key, .. } => Some(key),
        }
    }
}
#[derive(serde::Serialize, Clone, Debug)]
pub struct PolicyRow {
    pub field: PolicyField,
    pub value: Option<PolicyValue>,
    pub can_set: bool,
}
fn policy(v: &Value) -> Option<Vec<PolicyRow>> {
    let rows = v.as_arr()?;
    if rows.len() != 9 {
        return None;
    }
    let mut result = Vec::new();
    for row in rows {
        let field = PolicyField::decode(row.get("field")?)?;
        if result.iter().any(|r: &PolicyRow| r.field == field) {
            return None;
        }
        let value = optional(row.get("value"), |v| {
            field
                .choices()
                .iter()
                .find(|x| Some(x.wire()) == v.as_str())
                .copied()
        })?;
        let can_set = row.get("can_set")?.as_bool()?;
        if can_set && value.is_none() {
            return None;
        }
        result.push(PolicyRow {
            field,
            value,
            can_set,
        });
    }
    Some(result)
}
#[derive(serde::Serialize, Clone, Debug)]
pub struct DndRule {
    pub target: DndKey,
    pub name: String,
    pub enabled: bool,
    pub active: Option<bool>,
    pub can_edit: bool,
    pub can_delete: bool,
    pub schedule: Option<Schedule>,
}
#[derive(serde::Serialize, Clone, Debug)]
pub struct DndSnapshot {
    pub id: i64,
    pub available: bool,
    pub modes_api: bool,
    pub modes_ui: bool,
    pub manual_mode: Option<String>,
    pub current_mode: Option<String>,
    pub key: DndKey,
    pub policy: Vec<PolicyRow>,
    pub effective: Vec<PolicyRow>,
    pub repeat_minutes: Option<u32>,
    pub generation: DndKey,
    pub offset: u32,
    pub total: u32,
    pub stale: bool,
    pub truncated: bool,
    pub can_create: bool,
    pub rules: Vec<DndRule>,
}
impl DndSnapshot {
    pub fn decode(v: &Value) -> Option<Self> {
        if v.get("schema")?.as_i64()? != 1 {
            return None;
        }
        let available = match v.get("availability")?.as_str()? {
            "available" => true,
            "unavailable" => false,
            _ => return None,
        };
        let offset = uint(v.get("offset")?, 255)?;
        let total = uint(v.get("total")?, 256)?;
        let rows = v.get("rules")?.as_arr()?;
        if offset % 20 != 0
            || (total == 0 && offset != 0)
            || (total > 0 && offset >= total)
            || rows.len() != 20.min(total.saturating_sub(offset)) as usize
        {
            return None;
        }
        let mut rules = Vec::new();
        for row in rows {
            let target = DndKey::decode(row.get("target")?)?;
            if rules.iter().any(|r: &DndRule| r.target == target) {
                return None;
            }
            let name = text(row.get("name")?, 100)?;
            let enabled = row.get("enabled")?.as_bool()?;
            let schedule = optional(row.get("schedule"), |s| {
                let days = s
                    .get("days")?
                    .as_arr()?
                    .iter()
                    .map(|v| uint(v, 7).filter(|n| *n > 0).map(|n| n as u8))
                    .collect::<Option<Vec<_>>>()?;
                if days.len() > 7 || !days.windows(2).all(|w| w[0] < w[1]) {
                    return None;
                }
                Some(Schedule {
                    name: name.clone(),
                    days,
                    start: uint(s.get("start_minute")?, 1439)? as u16,
                    end: uint(s.get("end_minute")?, 1439)? as u16,
                    exit_at_alarm: s.get("exit_at_alarm")?.as_bool()?,
                    enabled,
                })
            })?;
            if row.get("kind")?.as_str()? != if schedule.is_some() { "time" } else { "other" } {
                return None;
            }
            let can_edit = row.get("can_edit")?.as_bool()?;
            let can_delete = row.get("can_delete")?.as_bool()?;
            if (can_edit || can_delete) && (!available || schedule.is_none()) {
                return None;
            }
            rules.push(DndRule {
                target,
                name,
                enabled,
                active: optional(row.get("active"), Value::as_bool)?,
                can_edit,
                can_delete,
                schedule,
            });
        }
        let mode = |v: &Value| {
            let s = v.as_str()?;
            ["off", "priority", "alarms", "silence"]
                .contains(&s)
                .then(|| s.to_owned())
        };
        let result = Self {
            id: v.get("request_id")?.as_i64().filter(|n| *n > 0)?,
            available,
            modes_api: v.get("modes_api")?.as_bool()?,
            modes_ui: v.get("modes_ui")?.as_bool()?,
            manual_mode: optional(v.get("manual_mode"), mode)?,
            current_mode: optional(v.get("current_mode"), mode)?,
            key: DndKey::decode(v.get("key")?)?,
            policy: policy(v.get("policy")?)?,
            effective: policy(v.get("effective_policy")?)?,
            repeat_minutes: optional(v.get("repeat_callers_minutes"), |v| {
                uint(v, 1440).filter(|n| *n > 0)
            })?,
            generation: DndKey::decode(v.get("generation")?)?,
            offset,
            total,
            stale: v.get("stale")?.as_bool()?,
            truncated: v.get("truncated")?.as_bool()?,
            can_create: v.get("can_create")?.as_bool()?,
            rules,
        };
        if result.effective.iter().any(|r| r.can_set)
            || !available
                && (result.can_create || result.policy.iter().any(|r| r.can_set) || total > 0)
        {
            return None;
        }
        Some(result)
    }
    pub fn rule(&self, target: &DndKey) -> Option<&DndRule> {
        self.rules.iter().find(|r| &r.target == target)
    }
    pub fn permits(&self, request: &DndRequest) -> bool {
        if !self.available || !request.valid() || request.key() != Some(&self.key) {
            return false;
        }
        match request {
            DndRequest::Policy { field, value, .. } => self
                .policy
                .iter()
                .any(|r| r.field == *field && r.can_set && r.value != Some(*value)),
            DndRequest::Save { target, .. } => target
                .as_ref()
                .map(|t| self.rule(t).is_some_and(|r| r.can_edit))
                .unwrap_or(self.can_create),
            DndRequest::Enabled {
                target, enabled, ..
            } => self.rule(target).is_some_and(|r| {
                r.can_edit
                    && r.enabled != *enabled
                    && (!enabled || r.schedule.as_ref().is_some_and(|s| !s.days.is_empty()))
            }),
            DndRequest::Delete { target, .. } => self.rule(target).is_some_and(|r| r.can_delete),
            _ => false,
        }
    }
    pub fn clear_actions(&mut self) {
        self.can_create = false;
        for row in &mut self.policy {
            row.can_set = false;
        }
        for row in &mut self.rules {
            row.can_edit = false;
            row.can_delete = false;
        }
    }
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use makepad_strict_json::{obj, s};
    pub fn snapshot(id: i64) -> Value {
        let policy = |writable| {
            Value::Arr(
                PolicyField::ALL
                    .into_iter()
                    .map(|field| {
                        obj(vec![
                            ("field", s(field.wire())),
                            ("value", s(field.choices()[0].wire())),
                            ("can_set", Value::Bool(writable)),
                        ])
                    })
                    .collect(),
            )
        };
        obj(vec![
            ("schema", Value::Int(1)),
            ("request_id", Value::Int(id)),
            ("availability", s("available")),
            ("modes_api", Value::Bool(true)),
            ("modes_ui", Value::Bool(false)),
            ("manual_mode", s("off")),
            ("current_mode", s("off")),
            ("key", s(format!("{:064x}", 1))),
            ("policy", policy(true)),
            ("effective_policy", policy(false)),
            ("repeat_callers_minutes", Value::Int(15)),
            ("generation", s(format!("{:064x}", 2))),
            ("offset", Value::Int(0)),
            ("total", Value::Int(1)),
            ("stale", Value::Bool(false)),
            ("truncated", Value::Bool(false)),
            ("can_create", Value::Bool(true)),
            (
                "rules",
                Value::Arr(vec![obj(vec![
                    ("target", s(format!("{:064x}", 3))),
                    ("name", s("Night")),
                    ("kind", s("time")),
                    ("enabled", Value::Bool(false)),
                    ("active", Value::Bool(false)),
                    ("can_edit", Value::Bool(true)),
                    ("can_delete", Value::Bool(true)),
                    (
                        "schedule",
                        obj(vec![
                            ("days", Value::Arr(vec![Value::Int(1), Value::Int(7)])),
                            ("start_minute", Value::Int(1320)),
                            ("end_minute", Value::Int(420)),
                            ("exit_at_alarm", Value::Bool(true)),
                        ]),
                    ),
                ])]),
            ),
        ])
    }
    #[test]
    fn policy_and_schedule_requests_require_fresh_observed_capabilities() {
        let mut state = DndSnapshot::decode(&snapshot(1)).unwrap();
        let request = DndRequest::Policy {
            key: state.key.clone(),
            field: PolicyField::Calls,
            value: PolicyValue::Starred,
        };
        assert!(state.permits(&request));
        let mut wrong = request.clone();
        if let DndRequest::Policy { value, .. } = &mut wrong {
            *value = PolicyValue::On;
        }
        assert!(!state.permits(&wrong));
        state.key = DndKey(format!("{:064x}", 12));
        assert!(!state.permits(&request));
        state.clear_actions();
        let row = &state.rules[0];
        assert!(!row.can_edit);
        assert_eq!(row.schedule.as_ref().unwrap().start, 1320);
        assert!(!state.permits(&DndRequest::Delete {
            key: state.key.clone(),
            target: row.target.clone()
        }));
    }
    #[test]
    fn schedules_validate_days_and_civil_time_without_invented_defaults() {
        for value in ["24:00", "12:60", "9:30", "12:00:00", " 12:00"] {
            assert!(time(value).is_none());
        }
        assert_eq!(time("00:00"), Some(0));
        assert_eq!(time("23:59"), Some(1439));
        let mut schedule = Schedule {
            name: "Reviewed".into(),
            days: vec![],
            start: 1320,
            end: 420,
            exit_at_alarm: true,
            enabled: false,
        };
        assert!(schedule.valid());
        schedule.enabled = true;
        assert!(!schedule.valid());
        schedule.days = vec![1, 1];
        assert!(!schedule.valid());
        schedule.days = vec![1, 7];
        schedule.end = schedule.start;
        assert!(
            schedule.valid(),
            "equal endpoint uses native next-day semantics"
        );
    }
    #[test]
    fn malformed_catalog_policy_and_unavailable_authority_are_rejected() {
        let mut v = snapshot(1);
        crate::settings_updates::tests::field(&mut v, "total", Value::Int(2));
        assert!(DndSnapshot::decode(&v).is_none());
        let mut v = snapshot(1);
        crate::settings_updates::tests::field(&mut v, "availability", s("unavailable"));
        assert!(DndSnapshot::decode(&v).is_none());
        let mut v = snapshot(1);
        let policy = v.get("policy").unwrap().clone();
        crate::settings_updates::tests::field(&mut v, "effective_policy", policy);
        assert!(DndSnapshot::decode(&v).is_none());
        assert!(!DndRead {
            offset: 20,
            generation: None
        }
        .valid());
        let mut state = DndSnapshot::decode(&snapshot(1)).unwrap();
        let read = DndRead {
            offset: 0,
            generation: Some(DndKey("f".repeat(64))),
        };
        assert!(
            !read.accepts(&state),
            "a changed catalog must explicitly retire the old generation"
        );
        state.stale = true;
        assert!(read.accepts(&state));
    }
}
