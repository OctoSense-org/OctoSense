//! A tool's declared JSON Schema, checked on what crosses the relay (ADR
//! 0004 §3, G8): every call's arguments against its `input_schema`, every
//! result against its `output_schema`.
//!
//! The subset `tools.json` may use (App Hub's gate refuses anything else):
//! `type` (one or a list), `properties`, `required`, `additionalProperties`
//! (a bool or a schema), `items`, `enum`, `const`, `minimum`, `maximum`,
//! `minLength`, `maxLength`, `minItems`, `maxItems`. `pattern` and
//! `format` are descriptive here (not enforced); `title`, `description`
//! and `default` never constrain. An absent or empty schema accepts
//! anything.

use serde_json::Value;

/// How deep a value may nest before it is refused outright.
const MAX_DEPTH: usize = 32;

/// Check `value` against `schema`; the first violation, with its path.
pub fn check(schema: &Value, value: &Value) -> Result<(), String> {
    node(schema, value, "$", 0)
}

fn type_matches(t: &str, value: &Value) -> bool {
    match t {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some() || value.as_f64().is_some_and(|f| f.fract() == 0.0),
        _ => true,
    }
}

fn node(schema: &Value, value: &Value, at: &str, depth: usize) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!("{at}: nested deeper than {MAX_DEPTH}"));
    }
    let Some(schema) = schema.as_object() else { return Ok(()) };
    match schema.get("type") {
        Some(Value::String(t)) if !type_matches(t, value) => return Err(format!("{at}: expected {t}")),
        Some(Value::Array(types)) if !types.iter().filter_map(Value::as_str).any(|t| type_matches(t, value)) => {
            let names: Vec<&str> = types.iter().filter_map(Value::as_str).collect();
            return Err(format!("{at}: expected one of {}", names.join(", ")));
        }
        _ => {}
    }
    if let Some(Value::Array(allowed)) = schema.get("enum") {
        if !allowed.contains(value) {
            return Err(format!("{at}: not one of the allowed values"));
        }
    }
    if let Some(expected) = schema.get("const") {
        if expected != value {
            return Err(format!("{at}: must be {expected}"));
        }
    }
    if let Some(n) = value.as_f64() {
        if schema.get("minimum").and_then(Value::as_f64).is_some_and(|min| n < min) {
            return Err(format!("{at}: below the minimum {}", schema["minimum"]));
        }
        if schema.get("maximum").and_then(Value::as_f64).is_some_and(|max| n > max) {
            return Err(format!("{at}: above the maximum {}", schema["maximum"]));
        }
    }
    if let Some(s) = value.as_str() {
        let len = s.chars().count() as u64;
        if schema.get("minLength").and_then(Value::as_u64).is_some_and(|min| len < min) {
            return Err(format!("{at}: shorter than {}", schema["minLength"]));
        }
        if schema.get("maxLength").and_then(Value::as_u64).is_some_and(|max| len > max) {
            return Err(format!("{at}: longer than {}", schema["maxLength"]));
        }
    }
    if let Some(items) = value.as_array() {
        let len = items.len() as u64;
        if schema.get("minItems").and_then(Value::as_u64).is_some_and(|min| len < min) {
            return Err(format!("{at}: fewer than {} items", schema["minItems"]));
        }
        if schema.get("maxItems").and_then(Value::as_u64).is_some_and(|max| len > max) {
            return Err(format!("{at}: more than {} items", schema["maxItems"]));
        }
        if let Some(item) = schema.get("items") {
            for (i, v) in items.iter().enumerate() {
                node(item, v, &format!("{at}[{i}]"), depth + 1)?;
            }
        }
    }
    if let Some(object) = value.as_object() {
        let properties = schema.get("properties").and_then(Value::as_object);
        if let Some(Value::Array(required)) = schema.get("required") {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return Err(format!("{at}: {key} is required"));
                }
            }
        }
        for (key, v) in object {
            let path = format!("{at}.{key}");
            match properties.and_then(|p| p.get(key)) {
                Some(sub) => node(sub, v, &path, depth + 1)?,
                None => match schema.get("additionalProperties") {
                    Some(Value::Bool(false)) => return Err(format!("{at}: {key} is not a declared field")),
                    Some(extra @ Value::Object(_)) => node(extra, v, &path, depth + 1)?,
                    _ => {}
                },
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check;
    use serde_json::json;

    #[test]
    fn the_subset_is_checked_with_a_path() {
        let schema = json!({"type": "object", "properties": {
            "command": {"type": "string", "minLength": 1, "maxLength": 5},
            "lines": {"type": "integer", "minimum": 1, "maximum": 2000},
            "to": {"type": "array", "items": {"type": "string"}, "maxItems": 2},
            "mode": {"enum": ["a", "b"]}},
            "required": ["command"], "additionalProperties": false});
        assert!(check(&schema, &json!({"command": "ls"})).is_ok());
        assert!(check(&schema, &json!({"command": "ls", "lines": 5, "to": ["x"], "mode": "a"})).is_ok());
        assert!(check(&schema, &json!({})).unwrap_err().contains("command is required"));
        assert!(check(&schema, &json!({"command": 3})).unwrap_err().contains("$.command: expected string"));
        assert!(check(&schema, &json!({"command": "toolong"})).unwrap_err().contains("longer than 5"));
        assert!(check(&schema, &json!({"command": "ls", "lines": 0})).unwrap_err().contains("below the minimum"));
        assert!(check(&schema, &json!({"command": "ls", "lines": 1.5})).unwrap_err().contains("expected integer"));
        assert!(check(&schema, &json!({"command": "ls", "to": ["a", 2]})).unwrap_err().contains("$.to[1]"));
        assert!(check(&schema, &json!({"command": "ls", "to": ["a", "b", "c"]})).unwrap_err().contains("more than 2 items"));
        assert!(check(&schema, &json!({"command": "ls", "mode": "c"})).is_err());
        assert!(check(&schema, &json!({"command": "ls", "sudo": true})).unwrap_err().contains("sudo is not a declared field"));
        assert!(check(&json!({"type": ["string", "null"]}), &json!(null)).is_ok());
        assert!(check(&json!({}), &json!({"anything": [1, 2]})).is_ok(), "no schema, no constraint");
        assert!(check(&json!({"type": "object"}), &json!("x")).is_err());
    }
}
