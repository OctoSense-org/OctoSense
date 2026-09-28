//! JSON helpers shared by the runner and evaluation.

use serde_json::Value;

/// Recursive data equality. OctoScript round-trips integral floats as JSON
/// integers, so numbers compare by value; every key and string compares
/// exactly.
pub fn equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => x == y,
            _ => a == b,
        },
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, a)| b.get(k).is_some_and(|b| equivalent(a, b)))
        }
        _ => a == b,
    }
}

/// The first path where `a` and `b` differ, for diagnostics.
pub fn first_difference(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .find_map(|(i, (x, y))| first_difference(x, y, &format!("{path}[{i}]"))),
        (Value::Object(x), Value::Object(y)) => {
            for (k, xv) in x {
                match y.get(k) {
                    Some(yv) => {
                        if let Some(d) = first_difference(xv, yv, &format!("{path}.{k}")) {
                            return Some(d);
                        }
                    }
                    None => return Some(format!("{path}.{k} (missing)")),
                }
            }
            y.keys()
                .find(|k| !x.contains_key(*k))
                .map(|k| format!("{path}.{k} (unexpected)"))
        }
        _ => (!equivalent(a, b)).then(|| path.to_owned()),
    }
}

/// Every string in `value`, recursively (object keys excluded).
pub fn strings(value: &Value) -> Vec<&str> {
    let mut out = Vec::new();
    fn walk<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
        match value {
            Value::String(s) => out.push(s),
            Value::Array(items) => items.iter().for_each(|v| walk(v, out)),
            Value::Object(fields) => fields.values().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    walk(value, &mut out);
    out
}

/// Whether a string carries a URL anywhere in it.
pub fn contains_url(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("http://") || lower.contains("https://") || lower.contains("www.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numbers_by_value() {
        assert!(equivalent(&json!({"a": [1, 2.0]}), &json!({"a": [1.0, 2]})));
        assert!(!equivalent(&json!({"a": 1}), &json!({"a": 1, "b": 2})));
        assert_eq!(
            first_difference(&json!({"a": [1, 2]}), &json!({"a": [1, 3]}), "$").as_deref(),
            Some("$.a[1]")
        );
    }
}
