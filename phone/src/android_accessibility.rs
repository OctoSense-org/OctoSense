//! Read-only renderer preferences from the native Activity, independent of Settings authority.
use makepad_strict_json::Value;
use makepad_widgets::makepad_platform::accessibility::AccessibilityPreferences;

pub(crate) fn observe(value: &Value, previous: AccessibilityPreferences) -> AccessibilityPreferences {
    if value.get("schema").and_then(Value::as_i64) != Some(1) { return previous; }
    let high = value.get("high_contrast_text").and_then(Value::as_bool).unwrap_or(previous.high_contrast_text);
    let weight = value.get("font_weight_adjustment").and_then(Value::as_i64)
        .filter(|v| (-1000..=1000).contains(v)).map(|v| v as i32).unwrap_or(previous.font_weight_adjustment);
    let scale = match value.get("animator_scale") {
        Some(Value::F64(v)) if v.is_finite() && *v >= 0.0 => *v,
        Some(Value::Int(v)) if *v >= 0 => *v as f64,
        _ => previous.animator_scale(),
    };
    AccessibilityPreferences::observed(high, weight, scale).unwrap_or(previous)
}
pub(crate) fn timeout(value: &Value, key: &str) -> Option<u32> {
    if value.get("schema").and_then(Value::as_i64) != Some(1) { return None; }
    u32::try_from(value.get(key)?.as_i64()?).ok()
}
#[cfg(test)] mod tests {
    use super::*;
    fn json(s: &str)->Value { makepad_strict_json::parse(s.as_bytes()).unwrap() }
    #[test] fn accessibility_observations_apply_independently_and_unknowns_preserve_state() {
        let first=observe(&json(r#"{"schema":1,"high_contrast_text":true,"font_weight_adjustment":300,"animator_scale":0}"#),AccessibilityPreferences::default());
        assert!(first.high_contrast_text && first.reduce_motion());assert_eq!(first.font_weight_adjustment,300);
        let malformed=json(r#"{"schema":1,"high_contrast_text":null,"font_weight_adjustment":2147483647,"animator_scale":-1}"#);
        assert_eq!(observe(&malformed,first),first);
        assert_eq!(observe(&json(r#"{"schema":2,"high_contrast_text":false}"#),first),first);
        let custom=observe(&json(r#"{"schema":1,"font_weight_adjustment":-100,"animator_scale":0.5}"#),first);
        assert!(custom.high_contrast_text);assert!(!custom.reduce_motion());assert_eq!(custom.font_weight_adjustment,-100);
        let reset=observe(&json(r#"{"schema":1,"high_contrast_text":false,"font_weight_adjustment":0,"animator_scale":1.0}"#),custom);
        assert_eq!(reset,AccessibilityPreferences::default());
    }
    #[test] fn recommended_timeouts_do_not_accept_unknown_or_negative_values() {
        for (value,expected) in [("null",None),("-1",None),("120000",Some(120000)),("0",Some(0))] {
            assert_eq!(timeout(&json(&format!("{{\"schema\":1,\"interactive_timeout_ms\":{value}}}")),"interactive_timeout_ms"),expected);
        }
    }
}
