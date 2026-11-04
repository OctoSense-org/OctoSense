//! Reading a manifest written for a newer `1.x` (ADR 0005 §2).
//!
//! The manifest's types refuse unknown fields (`deny_unknown_fields`), and
//! that stays the rule. Only for a manifest whose `schema_minor` is newer
//! than this build's does [`from_value`] read it through a deserializer that
//! drops, at every level, the object keys the target struct does not name,
//! and records them (path and value). Each struct tells serde its field
//! names when it asks to be deserialized (`deserialize_struct`), so what is
//! "unknown" is exactly what the strict path would have refused: nothing
//! else about the types or their checks changes.
use serde::de::{self, DeserializeSeed, Deserializer, IntoDeserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};
use std::cell::RefCell;

/// A field this build did not know, with where it was and what it held.
pub(crate) type Ignored = (Vec<String>, Value);

/// Deserialize `value`, ignoring and recording unknown struct fields.
pub(crate) fn from_value<T: de::DeserializeOwned>(value: Value) -> Result<(T, Vec<Ignored>), serde_json::Error> {
    let ignored = RefCell::new(Vec::new());
    let parsed = T::deserialize(Lenient { value, path: Vec::new(), ignored: &ignored })?;
    Ok((parsed, ignored.into_inner()))
}

/// Put ignored fields back into a serialised manifest, so a newer
/// manifest's signing bytes are the ones its signer computed.
pub(crate) fn restore(target: &mut Value, ignored: &[Ignored]) {
    for (path, value) in ignored {
        insert(target, path, value);
    }
}

fn insert(at: &mut Value, path: &[String], value: &Value) {
    match (path, at) {
        ([last], Value::Object(map)) => {
            map.insert(last.clone(), value.clone());
        }
        ([head, rest @ ..], Value::Object(map)) => {
            if let Some(next) = map.get_mut(head.as_str()) {
                insert(next, rest, value);
            }
        }
        ([head, rest @ ..], Value::Array(items)) if !rest.is_empty() => {
            if let Some(next) = head.parse::<usize>().ok().and_then(|i| items.get_mut(i)) {
                insert(next, rest, value);
            }
        }
        _ => {}
    }
}

struct Lenient<'a> {
    value: Value,
    path: Vec<String>,
    ignored: &'a RefCell<Vec<Ignored>>,
}

impl<'a> Lenient<'a> {
    fn child(&self, segment: String, value: Value) -> Lenient<'a> {
        let mut path = self.path.clone();
        path.push(segment);
        Lenient { value, path, ignored: self.ignored }
    }

    fn visit_object<'de, V: Visitor<'de>>(self, map: Map<String, Value>, visitor: V) -> Result<V::Value, serde_json::Error> {
        let entries: Vec<Lenient<'a>> = map.into_iter().map(|(k, v)| self.child(k, v)).collect();
        visitor.visit_map(Entries { entries: entries.into_iter(), pending: None })
    }

    fn visit_array<'de, V: Visitor<'de>>(self, items: Vec<Value>, visitor: V) -> Result<V::Value, serde_json::Error> {
        let items: Vec<Lenient<'a>> = items.into_iter().enumerate().map(|(i, v)| self.child(i.to_string(), v)).collect();
        visitor.visit_seq(Items { items: items.into_iter() })
    }
}

impl<'de, 'a> Deserializer<'de> for Lenient<'a> {
    type Error = serde_json::Error;

    fn deserialize_any<V: Visitor<'de>>(mut self, visitor: V) -> Result<V::Value, Self::Error> {
        match std::mem::take(&mut self.value) {
            Value::Object(map) => self.visit_object(map, visitor),
            Value::Array(items) => self.visit_array(items, visitor),
            other => other.deserialize_any(visitor),
        }
    }

    fn deserialize_struct<V: Visitor<'de>>(
        mut self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let Value::Object(mut map) = std::mem::take(&mut self.value) else {
            return self.value.deserialize_any(visitor);
        };
        let unknown: Vec<String> = map.keys().filter(|k| !fields.contains(&k.as_str())).cloned().collect();
        for key in unknown {
            let value = map.remove(&key).unwrap_or(Value::Null);
            let mut path = self.path.clone();
            path.push(key);
            self.ignored.borrow_mut().push((path, value));
        }
        self.visit_object(map, visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(mut self, visitor: V) -> Result<V::Value, Self::Error> {
        match std::mem::take(&mut self.value) {
            Value::Object(map) => self.visit_object(map, visitor),
            other => other.deserialize_map(visitor),
        }
    }

    fn deserialize_seq<V: Visitor<'de>>(mut self, visitor: V) -> Result<V::Value, Self::Error> {
        match std::mem::take(&mut self.value) {
            Value::Array(items) => self.visit_array(items, visitor),
            other => other.deserialize_seq(visitor),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _name: &'static str, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        // The manifest's enums are unit variants spelled as strings: no
        // struct inside them to be lenient about.
        self.value.deserialize_enum(name, variants, visitor)
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(self, _name: &'static str, _len: usize, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct identifier ignored_any
    }
}

struct Entries<'a> {
    entries: std::vec::IntoIter<Lenient<'a>>,
    pending: Option<Lenient<'a>>,
}

impl<'de, 'a> MapAccess<'de> for Entries<'a> {
    type Error = serde_json::Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error> {
        let Some(entry) = self.entries.next() else { return Ok(None) };
        let key = entry.path.last().cloned().unwrap_or_default();
        self.pending = Some(entry);
        seed.deserialize(IntoDeserializer::<serde_json::Error>::into_deserializer(key)).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Self::Error> {
        let entry = self.pending.take().ok_or_else(|| de::Error::custom("value without a key"))?;
        seed.deserialize(entry)
    }
}

struct Items<'a> {
    items: std::vec::IntoIter<Lenient<'a>>,
}

impl<'de, 'a> SeqAccess<'de> for Items<'a> {
    type Error = serde_json::Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error> {
        self.items.next().map(|item| seed.deserialize(item)).transpose()
    }
}
