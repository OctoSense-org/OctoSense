//! The JSON Schema subset `model.complete` checks a reply against: a small,
//! bounded validator.
//!
//! The keywords are Octoscript's (`octoscript-schema`, which the toolbox's
//! templates use): `type`, `properties`, `required`, `additionalProperties`
//! (boolean, default true), `items`, `minItems`, `maxItems`, `minLength`,
//! `maxLength`, `minimum`, `maximum` and `enum`, with the annotations
//! `title`, `description`, `default`, `examples`, `$schema` and `$id`.
//! Anything else is refused when the schema is compiled, so an app never
//! believes a keyword is enforced when it is not. The schema is capped in
//! bytes, depth, properties and enum values.
//!
//! It is not `octoscript-schema` itself because that crate turns on
//! serde_json's `arbitrary_precision`, which would change how every crate in
//! the shell's build reads numbers. Numbers here compare as `f64`.
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The largest schema an app may send, as compact JSON.
pub const MAX_SCHEMA_BYTES: usize = 8 * 1024;
pub const MAX_DEPTH: usize = 16;
pub const MAX_PROPERTIES: usize = 64;
pub const MAX_ENUM_VALUES: usize = 64;

const KEYWORDS: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "minItems",
    "maxItems",
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
    "enum",
    "title",
    "description",
    "default",
    "examples",
    "$schema",
    "$id",
];

/// A compiled schema.
#[derive(Clone, Debug, PartialEq)]
pub struct Schema {
    node: Node,
}

#[derive(Clone, Debug, PartialEq)]
struct Node {
    kind: Kind,
    enum_values: Option<Vec<Value>>,
}

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Any,
    Null,
    Boolean,
    Number { integer: bool, minimum: Option<f64>, maximum: Option<f64> },
    String { min: Option<usize>, max: Option<usize> },
    Array { items: Option<Box<Node>>, min: Option<usize>, max: Option<usize> },
    Object { properties: BTreeMap<String, Node>, required: BTreeSet<String>, additional: bool },
}

impl Schema {
    /// Compile `source`, or say what is wrong with it.
    pub fn compile(source: &Value) -> Result<Schema, String> {
        let size = source.to_string().len();
        if size > MAX_SCHEMA_BYTES {
            return Err(format!("the schema is {size} bytes; the most is {MAX_SCHEMA_BYTES}"));
        }
        Ok(Schema { node: Node::compile(source, "$", 0)? })
    }

    /// Check `value`; the error names the first place it fails.
    pub fn validate(&self, value: &Value) -> Result<(), String> {
        self.node.validate(value, "$")
    }
}

fn usize_of(object: &Map<String, Value>, key: &str, path: &str) -> Result<Option<usize>, String> {
    match object.get(key) {
        None => Ok(None),
        Some(v) => v.as_u64().map(|n| Some(n as usize)).ok_or_else(|| format!("{path}: {key} must be a whole number")),
    }
}

fn f64_of(object: &Map<String, Value>, key: &str, path: &str) -> Result<Option<f64>, String> {
    match object.get(key) {
        None => Ok(None),
        Some(v) => v.as_f64().map(Some).ok_or_else(|| format!("{path}: {key} must be a number")),
    }
}

impl Node {
    fn compile(source: &Value, path: &str, depth: usize) -> Result<Node, String> {
        if depth > MAX_DEPTH {
            return Err(format!("the schema is nested deeper than {MAX_DEPTH}"));
        }
        let object = source.as_object().ok_or_else(|| format!("{path}: a schema must be an object"))?;
        if let Some(key) = object.keys().find(|k| !KEYWORDS.contains(&k.as_str())) {
            return Err(format!("{path}: the keyword {key:?} is not supported"));
        }
        let has = |k: &str| object.contains_key(k);
        let inferred = if has("properties") || has("required") || has("additionalProperties") {
            "object"
        } else if has("items") || has("minItems") || has("maxItems") {
            "array"
        } else if has("minLength") || has("maxLength") {
            "string"
        } else if has("minimum") || has("maximum") {
            "number"
        } else {
            "any"
        };
        let kind = match object.get("type") {
            None => inferred,
            Some(Value::String(t)) => t.as_str(),
            Some(_) => return Err(format!("{path}: type must be one string")),
        };
        // A keyword for another type would be silently ignored: refuse it.
        let allowed: &[&str] = match kind {
            "object" => &["properties", "required", "additionalProperties"],
            "array" => &["items", "minItems", "maxItems"],
            "string" => &["minLength", "maxLength"],
            "number" | "integer" => &["minimum", "maximum"],
            _ => &[],
        };
        let typed = ["properties", "required", "additionalProperties", "items", "minItems", "maxItems", "minLength", "maxLength", "minimum", "maximum"];
        if let Some(k) = typed.iter().find(|k| has(k) && !allowed.contains(k)) {
            return Err(format!("{path}: {k} does not apply to type {kind:?}"));
        }
        let kind = match kind {
            "any" => Kind::Any,
            "null" => Kind::Null,
            "boolean" => Kind::Boolean,
            "number" | "integer" => Kind::Number {
                integer: kind == "integer",
                minimum: f64_of(object, "minimum", path)?,
                maximum: f64_of(object, "maximum", path)?,
            },
            "string" => Kind::String { min: usize_of(object, "minLength", path)?, max: usize_of(object, "maxLength", path)? },
            "array" => Kind::Array {
                items: match object.get("items") {
                    None => None,
                    Some(items) => Some(Box::new(Node::compile(items, &format!("{path}[]"), depth + 1)?)),
                },
                min: usize_of(object, "minItems", path)?,
                max: usize_of(object, "maxItems", path)?,
            },
            "object" => {
                let mut properties = BTreeMap::new();
                if let Some(props) = object.get("properties") {
                    let props = props.as_object().ok_or_else(|| format!("{path}: properties must be an object"))?;
                    if props.len() > MAX_PROPERTIES {
                        return Err(format!("{path}: more than {MAX_PROPERTIES} properties"));
                    }
                    for (name, sub) in props {
                        properties.insert(name.clone(), Node::compile(sub, &format!("{path}.{name}"), depth + 1)?);
                    }
                }
                let mut required = BTreeSet::new();
                if let Some(list) = object.get("required") {
                    let list = list.as_array().ok_or_else(|| format!("{path}: required must be a list"))?;
                    for name in list {
                        let name = name.as_str().ok_or_else(|| format!("{path}: required names must be strings"))?;
                        required.insert(name.to_string());
                    }
                }
                let additional = match object.get("additionalProperties") {
                    None => true,
                    Some(Value::Bool(b)) => *b,
                    Some(_) => return Err(format!("{path}: additionalProperties must be true or false")),
                };
                Kind::Object { properties, required, additional }
            }
            other => return Err(format!("{path}: the type {other:?} is not supported")),
        };
        let enum_values = match object.get("enum") {
            None => None,
            Some(Value::Array(values)) if !values.is_empty() && values.len() <= MAX_ENUM_VALUES => Some(values.clone()),
            Some(_) => return Err(format!("{path}: enum must be a list of 1 to {MAX_ENUM_VALUES} values")),
        };
        Ok(Node { kind, enum_values })
    }

    fn validate(&self, value: &Value, path: &str) -> Result<(), String> {
        let fail = |why: &str| Err(format!("{path}: {why}"));
        match &self.kind {
            Kind::Any => {}
            Kind::Null if !value.is_null() => return fail("expected null"),
            Kind::Boolean if !value.is_boolean() => return fail("expected a boolean"),
            Kind::Null | Kind::Boolean => {}
            Kind::Number { integer, minimum, maximum } => {
                let Some(n) = value.as_f64() else {
                    return fail(if *integer { "expected an integer" } else { "expected a number" });
                };
                if *integer && !(value.is_i64() || value.is_u64() || n.fract() == 0.0) {
                    return fail("expected an integer");
                }
                if minimum.is_some_and(|m| n < m) {
                    return fail("below minimum");
                }
                if maximum.is_some_and(|m| n > m) {
                    return fail("above maximum");
                }
            }
            Kind::String { min, max } => {
                let Some(s) = value.as_str() else { return fail("expected a string") };
                let n = s.chars().count();
                if min.is_some_and(|m| n < m) {
                    return fail("shorter than minLength");
                }
                if max.is_some_and(|m| n > m) {
                    return fail("longer than maxLength");
                }
            }
            Kind::Array { items, min, max } => {
                let Some(list) = value.as_array() else { return fail("expected a list") };
                if min.is_some_and(|m| list.len() < m) {
                    return fail("fewer items than minItems");
                }
                if max.is_some_and(|m| list.len() > m) {
                    return fail("more items than maxItems");
                }
                if let Some(items) = items {
                    for (i, item) in list.iter().enumerate() {
                        items.validate(item, &format!("{path}[{i}]"))?;
                    }
                }
            }
            Kind::Object { properties, required, additional } => {
                let Some(map) = value.as_object() else { return fail("expected an object") };
                if let Some(missing) = required.iter().find(|k| !map.contains_key(k.as_str())) {
                    return fail(&format!("missing {missing:?}"));
                }
                for (key, v) in map {
                    match properties.get(key) {
                        Some(node) => node.validate(v, &format!("{path}.{key}"))?,
                        None if !additional => return fail(&format!("unexpected property {key:?}")),
                        None => {}
                    }
                }
            }
        }
        if let Some(values) = &self.enum_values {
            if !values.iter().any(|v| same(v, value)) {
                return fail("not one of the enum values");
            }
        }
        Ok(())
    }
}

/// JSON equality with numbers by value (`1` equals `1.0`).
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w))),
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_typical_reply_schema() {
        let s = Schema::compile(&json!({
            "type": "object", "additionalProperties": false, "required": ["title", "tags"],
            "properties": {
                "title": {"type": "string", "maxLength": 20},
                "tags": {"type": "array", "maxItems": 3, "items": {"type": "string", "enum": ["a", "b"]}},
                "score": {"type": "integer", "minimum": 0, "maximum": 10}
            }
        }))
        .unwrap();
        assert!(s.validate(&json!({"title": "x", "tags": ["a"], "score": 3})).is_ok());
        assert!(s.validate(&json!({"title": "x", "tags": ["a"], "score": 3.0})).is_ok());
        assert!(s.validate(&json!({"title": "x"})).unwrap_err().contains("missing \"tags\""));
        assert!(s.validate(&json!({"title": "x", "tags": ["c"]})).unwrap_err().contains("$.tags[0]"));
        assert!(s.validate(&json!({"title": "x", "tags": [], "more": 1})).unwrap_err().contains("unexpected"));
        assert!(s.validate(&json!({"title": "x", "tags": [], "score": 2.5})).unwrap_err().contains("integer"));
        assert!(s.validate(&json!({"title": "x".repeat(21), "tags": []})).is_err());
    }

    #[test]
    fn unsupported_or_misplaced_keywords_are_refused() {
        assert!(Schema::compile(&json!({"type": "string", "pattern": "^a"})).unwrap_err().contains("pattern"));
        assert!(Schema::compile(&json!({"anyOf": []})).is_err());
        assert!(Schema::compile(&json!({"type": "string", "minimum": 1})).is_err());
        assert!(Schema::compile(&json!({"type": ["string", "null"]})).is_err());
        assert!(Schema::compile(&json!("string")).is_err());
    }

    #[test]
    fn size_and_depth_are_capped() {
        let big = json!({"type": "string", "description": "x".repeat(MAX_SCHEMA_BYTES)});
        assert!(Schema::compile(&big).unwrap_err().contains("bytes"));
        let mut deep = json!({"type": "string"});
        for _ in 0..=MAX_DEPTH + 1 {
            deep = json!({"type": "array", "items": deep});
        }
        assert!(Schema::compile(&deep).unwrap_err().contains("deeper"));
    }
}
