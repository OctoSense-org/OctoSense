//! Reading a call's arguments. Every helper refuses with `invalid:` (a cap
//! with `too_large:`) and names the key. Integers may arrive as whole
//! floats (`3.0`): a script's numbers are doubles. The values these helpers
//! return are what the service builds every engine argument from: a method
//! never hands the engine a caller's object.

use serde_json::Value as Json;

use crate::codes::{invalid, too_large};

/// Refuse any argument `method` does not take. The engine commands behind a
/// method only ever get the parameters reviewed for them, which the service
/// builds itself; a key it was not reviewed for (a `picture` on a fill, a
/// `path` on a background) is refused here, before any engine work. A key
/// set to `null` counts as absent.
pub(crate) fn only(args: &Json, keys: &[&str], method: &str) -> Result<(), String> {
    match args {
        Json::Null => Ok(()),
        Json::Object(map) => match map.iter().find(|(k, v)| !v.is_null() && !keys.contains(&k.as_str())) {
            Some((k, _)) => Err(invalid(format!("{method} takes {}; `{k}` is not one of them", keys.join(", ")))),
            None => Ok(()),
        },
        _ => Err(invalid(format!("{method}'s arguments are an object"))),
    }
}

pub(crate) fn present<'a>(args: &'a Json, key: &str) -> Option<&'a Json> {
    args.get(key).filter(|v| !v.is_null())
}

/// A required, non-empty string.
pub(crate) fn need_str<'a>(args: &'a Json, key: &str, m: &str) -> Result<&'a str, String> {
    present(args, key).and_then(Json::as_str).filter(|s| !s.is_empty()).ok_or_else(|| invalid(format!("{m} needs `{key}`, a string")))
}

/// An optional string of at most `max` characters (it may be empty).
pub(crate) fn opt_str<'a>(args: &'a Json, key: &str, m: &str, max: usize) -> Result<Option<&'a str>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    let s = v.as_str().ok_or_else(|| invalid(format!("{m}: `{key}` is a string")))?;
    if s.chars().count() > max {
        return Err(too_large(format!("{m}: `{key}` is at most {max} characters")));
    }
    Ok(Some(s))
}

/// A required string of at most `max` characters (it may be empty).
pub(crate) fn need_text<'a>(args: &'a Json, key: &str, m: &str, max: usize) -> Result<&'a str, String> {
    opt_str(args, key, m, max)?.ok_or_else(|| invalid(format!("{m} needs `{key}`, a string")))
}

/// A whole number, from an integer or a whole float.
pub(crate) fn whole(v: &Json) -> Option<i64> {
    v.as_i64().or_else(|| v.as_f64().filter(|f| f.fract() == 0.0 && f.abs() < 9.0e15).map(|f| f as i64))
}

/// An optional whole number.
pub(crate) fn opt_int(args: &Json, key: &str, m: &str) -> Result<Option<i64>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    whole(v).map(Some).ok_or_else(|| invalid(format!("{m}: `{key}` is a whole number")))
}

/// An optional whole number in `lo..=hi`.
pub(crate) fn opt_int_in(args: &Json, key: &str, m: &str, lo: i64, hi: i64) -> Result<Option<i64>, String> {
    match opt_int(args, key, m)? {
        Some(n) if !(lo..=hi).contains(&n) => Err(invalid(format!("{m}: `{key}` is {lo} to {hi}"))),
        n => Ok(n),
    }
}

pub(crate) fn opt_bool(args: &Json, key: &str, m: &str) -> Result<Option<bool>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    v.as_bool().map(Some).ok_or_else(|| invalid(format!("{m}: `{key}` is true or false")))
}

pub(crate) fn need_bool(args: &Json, key: &str, m: &str) -> Result<bool, String> {
    opt_bool(args, key, m)?.ok_or_else(|| invalid(format!("{m} needs `{key}`, true or false")))
}

/// An optional finite number in `lo..=hi`.
pub(crate) fn opt_num(args: &Json, key: &str, m: &str, lo: f64, hi: f64) -> Result<Option<f64>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    match v.as_f64().filter(|f| f.is_finite()) {
        Some(f) if f >= lo && f <= hi => Ok(Some(f)),
        Some(_) => Err(invalid(format!("{m}: `{key}` is {lo} to {hi}"))),
        None => Err(invalid(format!("{m}: `{key}` is a number"))),
    }
}

pub(crate) fn need_num(args: &Json, key: &str, m: &str, lo: f64, hi: f64) -> Result<f64, String> {
    opt_num(args, key, m, lo, hi)?.ok_or_else(|| invalid(format!("{m} needs `{key}`, a number")))
}

/// An engine id (a slide's or a shape's): a whole number the engine's ids
/// can be.
fn engine_id(v: &Json) -> Option<u32> {
    whole(v).and_then(|n| u32::try_from(n).ok())
}

/// A required slide or shape id.
pub(crate) fn id(args: &Json, key: &str, m: &str) -> Result<u32, String> {
    present(args, key).and_then(engine_id).ok_or_else(|| invalid(format!("{m} needs `{key}`, an id the deck gave")))
}

/// Ids: 1 to `max` of them, distinct (in the order given).
pub(crate) fn ids(args: &Json, key: &str, m: &str, max: usize) -> Result<Vec<u32>, String> {
    let list = present(args, key).and_then(Json::as_array).ok_or_else(|| invalid(format!("{m} needs `{key}`, a list of ids")))?;
    if list.is_empty() {
        return Err(invalid(format!("{m}: `{key}` names at least one")));
    }
    if list.len() > max {
        return Err(too_large(format!("{m}: `{key}` names at most {max} in one call")));
    }
    let mut out: Vec<u32> = Vec::with_capacity(list.len());
    for v in list {
        let id = engine_id(v).ok_or_else(|| invalid(format!("{m}: `{key}` holds ids the deck gave")))?;
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Ok(out)
}

/// An optional list of ids, as [`ids`].
pub(crate) fn opt_ids(args: &Json, key: &str, m: &str, max: usize) -> Result<Option<Vec<u32>>, String> {
    if present(args, key).is_none() {
        return Ok(None);
    }
    ids(args, key, m, max).map(Some)
}

/// An optional 0-based position.
pub(crate) fn opt_index(args: &Json, key: &str, m: &str) -> Result<Option<usize>, String> {
    match opt_int(args, key, m)? {
        Some(n) if n >= 0 => Ok(Some(n as usize)),
        Some(_) => Err(invalid(format!("{m}: `{key}` is a position from 0"))),
        None => Ok(None),
    }
}

pub(crate) fn need_index(args: &Json, key: &str, m: &str) -> Result<usize, String> {
    opt_index(args, key, m)?.ok_or_else(|| invalid(format!("{m} needs `{key}`, a position from 0")))
}

fn finite(v: &Json) -> Option<f64> {
    v.as_f64().filter(|f| f.is_finite())
}

/// A box `[x, y, w, h]` in points, with a width and height of at least 0.
/// (How far it may reach is the door's limit on the command's geometry.)
pub(crate) fn opt_rect(args: &Json, key: &str, m: &str) -> Result<Option<[f64; 4]>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    match v.as_array().map(|a| a.iter().map(finite).collect::<Option<Vec<f64>>>()) {
        Some(Some(r)) if r.len() == 4 && r[2] >= 0.0 && r[3] >= 0.0 => Ok(Some([r[0], r[1], r[2], r[3]])),
        _ => Err(invalid(format!("{m}: `{key}` is [x, y, w, h] in points"))),
    }
}

/// A point `[x, y]` in points.
pub(crate) fn opt_point(args: &Json, key: &str, m: &str) -> Result<Option<[f64; 2]>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    match v.as_array().map(|a| a.iter().map(finite).collect::<Option<Vec<f64>>>()) {
        Some(Some(p)) if p.len() == 2 => Ok(Some([p[0], p[1]])),
        _ => Err(invalid(format!("{m}: `{key}` is [x, y] in points"))),
    }
}

/// A table cell `[row, column]`, 0-based.
pub(crate) fn opt_cell(args: &Json, key: &str, m: &str) -> Result<Option<[usize; 2]>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    cell_of(v).map(Some).ok_or_else(|| invalid(format!("{m}: `{key}` is [row, column], from 0")))
}

pub(crate) fn cell_of(v: &Json) -> Option<[usize; 2]> {
    let a = v.as_array().filter(|a| a.len() == 2)?;
    let n = |i: usize| whole(&a[i]).and_then(|n| usize::try_from(n).ok());
    Some([n(0)?, n(1)?])
}

/// The theme's colour slots a colour may name.
const SLOTS: [&str; 16] = ["accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "tx1", "tx2", "bg1", "bg2", "dk1", "dk2", "lt1", "lt2", "hlink", "folHlink"];

/// A colour as the service hands it to the engine: `#RRGGBB`, written from
/// the three bytes it parsed, or one of the theme's colour slots.
pub(crate) fn color_value(v: &Json) -> Option<String> {
    let s = v.as_str()?;
    if let Some(slot) = SLOTS.iter().find(|slot| **slot == s) {
        return Some((*slot).to_string());
    }
    let hex = s.strip_prefix('#').filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()))?;
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(format!("#{:02X}{:02X}{:02X}", byte(0)?, byte(2)?, byte(4)?))
}

pub(crate) fn opt_color(args: &Json, key: &str, m: &str) -> Result<Option<String>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    color_value(v).map(Some).ok_or_else(|| invalid(format!("{m}: `{key}` is #RRGGBB or a theme colour ({})", SLOTS.join(", "))))
}

/// One of `choices`, exactly.
pub(crate) fn opt_choice<'a>(args: &'a Json, key: &str, m: &str, choices: &[&str]) -> Result<Option<&'a str>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    match v.as_str() {
        Some(s) if choices.contains(&s) => Ok(Some(s)),
        _ => Err(invalid(format!("{m}: `{key}` is {}", choices.join(", ")))),
    }
}

pub(crate) fn need_choice<'a>(args: &'a Json, key: &str, m: &str, choices: &[&str]) -> Result<&'a str, String> {
    opt_choice(args, key, m, choices)?.ok_or_else(|| invalid(format!("{m} needs `{key}`: {}", choices.join(", "))))
}

/// A slide size: `"16:9"`, `"4:3"` or `[w, h]` in points.
pub(crate) fn opt_size(args: &Json, key: &str, m: &str) -> Result<Option<[f64; 2]>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    let wrong = || invalid(format!("{m}: `{key}` is \"16:9\", \"4:3\" or [w, h] in points"));
    match v {
        Json::String(s) if s == "16:9" => Ok(Some([960.0, 540.0])),
        Json::String(s) if s == "4:3" => Ok(Some([720.0, 540.0])),
        Json::Array(a) if a.len() == 2 => {
            let (w, h) = (finite(&a[0]).ok_or_else(wrong)?, finite(&a[1]).ok_or_else(wrong)?);
            // The engine's own bounds on a slide side; the door bounds the area.
            if !(72.0..=4032.0).contains(&w) || !(72.0..=4032.0).contains(&h) {
                return Err(invalid(format!("{m}: `{key}`'s sides are 72 to 4032 points")));
            }
            Ok(Some([w, h]))
        }
        _ => Err(wrong()),
    }
}
