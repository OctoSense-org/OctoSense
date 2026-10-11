//! Reading a call's arguments. Every helper refuses with `invalid:` (a cap
//! with `too_large:`) and names the key. Integers may arrive as whole
//! floats (`3.0`): a script's numbers are doubles.

use serde_json::Value as Json;

use crate::codes::{fail, invalid, Code};

/// The most pages one call names (SERVICE.md: "at most 512 pages named in
/// one call").
pub(crate) const MAX_PAGES: usize = 512;
/// The most rectangles one comment marks.
pub(crate) const MAX_RECTS: usize = 256;

/// Refuse any argument `method` does not take. The engine command behind
/// a method only ever gets the parameters reviewed for it, which the
/// service builds itself; a key it was not reviewed for (a file `path` on
/// a comment) is refused here, before any engine work. A key set to `null`
/// counts as absent.
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

fn present<'a>(args: &'a Json, key: &str) -> Option<&'a Json> {
    args.get(key).filter(|v| !v.is_null())
}

/// A required, non-empty string.
pub(crate) fn need_str<'a>(args: &'a Json, key: &str, m: &str) -> Result<&'a str, String> {
    present(args, key).and_then(Json::as_str).filter(|s| !s.is_empty()).ok_or_else(|| invalid(format!("{m} needs `{key}`, a string")))
}

/// An optional string of at most `max` characters.
pub(crate) fn opt_str<'a>(args: &'a Json, key: &str, m: &str, max: usize) -> Result<Option<&'a str>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    let s = v.as_str().ok_or_else(|| invalid(format!("{m}: `{key}` is a string")))?;
    if s.chars().count() > max {
        return Err(fail(Code::TooLarge, format!("{m}: `{key}` is at most {max} characters")));
    }
    Ok(Some(s))
}

/// A whole number, from an integer or a whole float.
fn whole(v: &Json) -> Option<i64> {
    v.as_i64().or_else(|| v.as_f64().filter(|f| f.fract() == 0.0 && f.abs() < 9.0e15).map(|f| f as i64))
}

/// An optional whole number.
pub(crate) fn opt_int(args: &Json, key: &str, m: &str) -> Result<Option<i64>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    whole(v).map(Some).ok_or_else(|| invalid(format!("{m}: `{key}` is a whole number")))
}

/// A required whole number ≥ 1 (a 1-based page, a position).
pub(crate) fn positive(args: &Json, key: &str, m: &str) -> Result<u64, String> {
    match opt_int(args, key, m)? {
        Some(n) if n >= 1 => Ok(n as u64),
        _ => Err(invalid(format!("{m} needs `{key}`, a whole number from 1"))),
    }
}

pub(crate) fn opt_bool(args: &Json, key: &str, m: &str) -> Result<Option<bool>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    v.as_bool().map(Some).ok_or_else(|| invalid(format!("{m}: `{key}` is true or false")))
}

/// 1-based page numbers: 1 to [`MAX_PAGES`] of them.
pub(crate) fn opt_pages(args: &Json, key: &str, m: &str) -> Result<Option<Vec<u64>>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    let list = v.as_array().ok_or_else(|| invalid(format!("{m}: `{key}` is a list of 1-based page numbers")))?;
    if list.is_empty() {
        return Err(invalid(format!("{m}: `{key}` names at least one page")));
    }
    if list.len() > MAX_PAGES {
        return Err(fail(Code::TooLarge, format!("{m}: `{key}` names at most {MAX_PAGES} pages in one call")));
    }
    list.iter()
        .map(|p| whole(p).filter(|n| *n >= 1).map(|n| n as u64).ok_or_else(|| invalid(format!("{m}: `{key}` holds 1-based page numbers"))))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

pub(crate) fn pages(args: &Json, key: &str, m: &str) -> Result<Vec<u64>, String> {
    opt_pages(args, key, m)?.ok_or_else(|| invalid(format!("{m} needs `{key}`, a list of 1-based page numbers")))
}

fn finite(v: &Json) -> Option<f64> {
    v.as_f64().filter(|f| f.is_finite() && f.abs() <= 1.0e6)
}

/// A point `[x, y]` in points from the top-left of the displayed page.
pub(crate) fn opt_point(args: &Json, key: &str, m: &str) -> Result<Option<[f64; 2]>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    match v.as_array().map(|a| a.iter().map(finite).collect::<Option<Vec<f64>>>()) {
        Some(Some(p)) if p.len() == 2 => Ok(Some([p[0], p[1]])),
        _ => Err(invalid(format!("{m}: `{key}` is [x, y] in points"))),
    }
}

/// Rectangles `[[x, y, w, h], …]` in points from the top-left of the
/// displayed page, each with a positive size: 1 to [`MAX_RECTS`] of them.
pub(crate) fn opt_rects(args: &Json, key: &str, m: &str) -> Result<Option<Vec<[f64; 4]>>, String> {
    let Some(v) = present(args, key) else { return Ok(None) };
    let wrong = || invalid(format!("{m}: `{key}` is a list of [x, y, w, h] in points, each with a positive width and height"));
    let list = v.as_array().filter(|l| !l.is_empty()).ok_or_else(wrong)?;
    if list.len() > MAX_RECTS {
        return Err(fail(Code::TooLarge, format!("{m}: `{key}` holds at most {MAX_RECTS} rectangles")));
    }
    list.iter()
        .map(|r| match r.as_array().map(|a| a.iter().map(finite).collect::<Option<Vec<f64>>>()) {
            Some(Some(r)) if r.len() == 4 && r[2] > 0.0 && r[3] > 0.0 => Ok([r[0], r[1], r[2], r[3]]),
            _ => Err(wrong()),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

/// A resolution in dots per inch, `lo..=hi`, rounded to a whole number.
pub(crate) fn dpi(args: &Json, key: &str, m: &str, default: u32, (lo, hi): (u32, u32)) -> Result<u32, String> {
    let Some(v) = present(args, key) else { return Ok(default) };
    let d = v.as_f64().filter(|f| f.is_finite()).ok_or_else(|| invalid(format!("{m}: `{key}` is a number")))?.round();
    if d < f64::from(lo) || d > f64::from(hi) {
        return Err(invalid(format!("{m}: `{key}` is {lo} to {hi}")));
    }
    Ok(d as u32)
}

/// Engine geometry `[x0, y0, x1, y1]` (top-left origin) as SERVICE.md's
/// `[x, y, w, h]`, to 1/100 pt; `null` when it is not four numbers.
pub(crate) fn xywh(r: &Json) -> Json {
    let n: Option<Vec<f64>> = r.as_array().map(|a| a.iter().filter_map(Json::as_f64).collect());
    match n {
        Some(v) if v.len() == 4 => {
            let round = |x: f64| (x * 100.0).round() / 100.0;
            serde_json::json!([round(v[0].min(v[2])), round(v[1].min(v[3])), round((v[2] - v[0]).abs()), round((v[3] - v[1]).abs())])
        }
        _ => Json::Null,
    }
}
