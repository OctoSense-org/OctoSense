//! The `sys.*` helpers a lowered L0 card calls, installed into every Splash
//! isolate through `register_splash_isolate_mod` (the host mods run last, so
//! they survive the isolate's ambient-authority strip).
//!
//! Lifted from `register_agent_module` in the AppCard port framework's
//! `widgets/src/splash.rs` — the weather/text subset only: the open-meteo
//! forecast helpers (`weather`, `weathernum`, `weathercond`, `weatherword`,
//! `dayname`, `weekmin`, `weekmax`, `daylight`), the clock-only moon helpers
//! (`moonphase`, `moonnum`), place lookup (`geocode`, `geocodenum`), the raw
//! `fetch` primitive, and the pure L0 helpers the framework already ships
//! (`splash_l0::install`: `l0_ratio`, `convert`, `num`, `json_string`,
//! `l0_math`). Live values go through the platform's fetch layer
//! (`Cx::script_data_fetch`): the first call issues the request and answers
//! "—", the module re-evaluates the card when the fetch lands.
//!
//! Stubbed for Phase A, with the placeholder the cards already tolerate:
//! `locale` (no per-device locale plumbing yet: "en"/"c"), `prefs` and
//! `cities` (the durable store is Phase B: empty), `gps` (no location
//! permission in this host yet: an empty geocode name resolves to nothing
//! instead of the device fix), and `agent.notify` (the tap channel is Phase
//! B: logged, not dispatched). Nothing here touches a file, a process or a
//! thread.

use makepad_widgets::makepad_script::*;
use makepad_widgets::*;

/// The cached open-meteo forecast for a place: one URL, so every helper
/// that reads a different field of it shares a single request.
fn forecast_url(lat: f64, lon: f64) -> String {
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat:.4}&longitude={lon:.4}\
&current=temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m,surface_pressure,is_day\
&daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset,uv_index_max,precipitation_probability_max\
&timezone=auto&forecast_days=7"
    )
}

/// The value a live helper shows while its fetch is in flight or after it
/// failed: an em dash, which every card's layout already tolerates.
const PLACEHOLDER: &str = "—";

fn string_arg(vm: &mut ScriptVm, value: ScriptValue) -> String {
    let mut out = String::new();
    vm.bx.heap.cast_to_string(value, &mut out);
    out
}

/// Pluck a dotted path (`daily.temperature_2m_max.0`) out of a JSON body.
/// open-meteo ISO local datetimes reduce to `HH:MM`, which is what a card
/// prints for sunrise and sunset.
pub fn json_pluck(bytes: &[u8], path: &str) -> Option<String> {
    let root: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let mut cur = &root;
    for seg in path.split('.') {
        cur = if let Ok(idx) = seg.parse::<usize>() { cur.get(idx)? } else { cur.get(seg)? };
    }
    let s = match cur {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        _ => return None,
    };
    if s.len() >= 16 && s.as_bytes().get(10) == Some(&b'T') {
        return Some(s[11..16].to_string());
    }
    Some(s)
}

/// Whole degrees, whole UV index, whole wind speed: what a card displays.
fn round_display(path: &str, value: String) -> String {
    let rounds = path.contains("temperature") || path.contains("uv_index") || path.contains("wind_speed");
    if !rounds {
        return value;
    }
    match value.parse::<f64>() {
        Ok(n) => format!("{}", n.round() as i64),
        Err(_) => value,
    }
}

fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Howard Hinnant's `days_from_civil`: (year, month, day) -> days since the epoch.
fn days_from_civil(y: i64, m: u64, d: u64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

/// Weekday for a days-since-epoch count, 0 = Sunday (1970-01-01 was a Thursday).
fn weekday_from_days(z: i64) -> usize {
    (((z + 4) % 7 + 7) % 7) as usize
}

const DAY_EN: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const DAY_ZH: [&str; 7] = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];

/// The lowest low or highest high across the cached 7-day forecast.
fn week_extreme(vm: &mut ScriptVm, lat: f64, lon: f64, path: &str, want_max: bool) -> Option<f64> {
    let bytes = vm.host.cx_mut().script_data_fetch(&forecast_url(lat, lon))?;
    let mut acc: Option<f64> = None;
    for i in 0..7 {
        let Some(v) = json_pluck(&bytes, &format!("{path}.{i}")) else { continue };
        let Ok(n) = v.parse::<f64>() else { continue };
        acc = Some(match acc {
            None => n,
            Some(a) if want_max => a.max(n),
            Some(a) => a.min(n),
        });
    }
    acc
}

/// Mean synodic month in seconds, and a known new moon (2000-01-06 18:14 UTC).
const SYNODIC_SECS: f64 = 29.530_588_853 * 86_400.0;
const NEW_MOON_EPOCH: f64 = 947_182_440.0;

/// Position in the synodic cycle, 0..1: 0 new, 0.5 full. The mean cycle,
/// which is within half a day of the true phase — invisible on a disc.
fn moon_phase_fraction() -> f64 {
    let elapsed = now_unix_secs() as f64 - NEW_MOON_EPOCH;
    let f = (elapsed % SYNODIC_SECS) / SYNODIC_SECS;
    if f < 0.0 { f + 1.0 } else { f }
}

fn moon_phase_name(f: f64) -> &'static str {
    if f < 0.0335 || f >= 0.9665 { "New Moon" }
    else if f < 0.2165 { "Waxing Crescent" }
    else if f < 0.2835 { "First Quarter" }
    else if f < 0.4665 { "Waxing Gibbous" }
    else if f < 0.5335 { "Full Moon" }
    else if f < 0.7165 { "Waning Gibbous" }
    else if f < 0.7835 { "Last Quarter" }
    else { "Waning Crescent" }
}

fn moon_phase_name_zh(f: f64) -> &'static str {
    if f < 0.0335 || f >= 0.9665 { "新月" }
    else if f < 0.2165 { "蛾眉月" }
    else if f < 0.2835 { "上弦月" }
    else if f < 0.4665 { "盈凸月" }
    else if f < 0.5335 { "满月" }
    else if f < 0.7165 { "亏凸月" }
    else if f < 0.7835 { "下弦月" }
    else { "残月" }
}

/// "HH:MM" -> minutes since midnight.
fn hhmm_to_minutes(s: &str) -> Option<f64> {
    let (h, m) = s.trim().split_once(':')?;
    Some(h.trim().parse::<f64>().ok()? * 60.0 + m.trim().parse::<f64>().ok()?)
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x3040..=0x30FF | 0xAC00..=0xD7AF)
}

fn percent_encode_query(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The open-meteo gazetteer lookup for a named place. Phase A has no GPS:
/// an empty name (the exemplar's "device location") searches for nothing
/// and keeps the placeholder — never an invented place.
fn geocode_url(name: &str) -> String {
    let name = name.trim();
    let lang = if name.chars().any(is_cjk) { "zh" } else { "en" };
    format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language={lang}&format=json",
        percent_encode_query(name)
    )
}

fn geocode_pluck(bytes: &[u8], field: &str) -> Option<String> {
    let candidates: &[&str] = match field {
        "lat" => &["results.0.latitude"],
        "lon" => &["results.0.longitude"],
        "name" => &["results.0.name"],
        "country" => &["results.0.country"],
        "admin" | "admin1" => &["results.0.admin1"],
        "timezone" => &["results.0.timezone"],
        other => return json_pluck(bytes, &format!("results.0.{other}")),
    };
    candidates.iter().find_map(|path| json_pluck(bytes, path))
}

/// The WMO weather code at `path` of the cached forecast, as a number.
fn weather_code(vm: &mut ScriptVm, lat: f64, lon: f64, path: &str) -> Option<i64> {
    vm.host
        .cx_mut()
        .script_data_fetch(&forecast_url(lat, lon))
        .and_then(|bytes| json_pluck(&bytes, path.trim()))
        .and_then(|s| s.parse::<f64>().ok())
        .map(|n| n as i64)
}

/// Install `sys` and `agent` into a Splash isolate. Registered with
/// `register_splash_isolate_mod`, so it runs for every isolate the host
/// allocates; the names are namespaced under `sys`, and no other module
/// in this host defines one.
pub fn install(vm: &mut ScriptVm) {
    // The card's own text roles, so the body's `crate_resource("octosense_appcard:...")`
    // font references resolve in this isolate (loading a crate's script mod
    // records its manifest path in the VM).
    crate::roles::script_mod(vm);

    // `agent.notify(event, payload)`: the tap channel. Phase B routes it to
    // the card session; Phase A records that the circuit was closed.
    let agent = vm.new_module(id!(agent));
    vm.add_method(agent, id_lut!(notify), script_args_def!(event = NIL, payload = NIL), |vm, args| {
        let event = script_value!(vm, args.event);
        let event = string_arg(vm, event);
        log!("appcard: agent.notify({event}) — no card session in Phase A");
        NIL
    });
    vm.set_injected_global(id!(agent), agent.into());

    let sys = vm.new_module(id!(sys));
    makepad_widgets::splash_l0::install(vm, sys);

    // sys.fetch(url) -> the raw response body, or "" while it loads.
    vm.add_method(sys, id_lut!(fetch), script_args_def!(url = NIL), |vm, args| {
        let url = script_value!(vm, args.url);
        let url = string_arg(vm, url);
        let url = url.trim();
        if url.is_empty() {
            return vm.bx.heap.new_string_from_str("");
        }
        let out = match vm.host.cx_mut().script_data_fetch(url) {
            Some(bytes) => String::from_utf8_lossy(&bytes[..]).into_owned(),
            None => String::new(),
        };
        vm.bx.heap.new_string_from_str(&out)
    });

    // sys.geocode(name, "name"|"lat"|"lon"|"country"|...) -> the field as text.
    vm.add_method(sys, id_lut!(geocode), script_args_def!(name = NIL, field = NIL), |vm, args| {
        let name = script_value!(vm, args.name);
        let name = string_arg(vm, name);
        let field = script_value!(vm, args.field);
        let field = string_arg(vm, field);
        let out = if name.trim().is_empty() {
            PLACEHOLDER.to_string()
        } else {
            match vm.host.cx_mut().script_data_fetch(&geocode_url(&name)) {
                Some(bytes) => geocode_pluck(&bytes, field.trim()).unwrap_or_else(|| PLACEHOLDER.to_string()),
                None => PLACEHOLDER.to_string(),
            }
        };
        vm.bx.heap.new_string_from_str(&out)
    });

    // sys.geocodenum(name, "lat"|"lon") -> the coordinate, -9999 while the
    // lookup loads or when the place is unknown: guard the card on `>= -9998`.
    vm.add_method(sys, id_lut!(geocodenum), script_args_def!(name = NIL, field = NIL), |vm, args| {
        let name = script_value!(vm, args.name);
        let name = string_arg(vm, name);
        let field = script_value!(vm, args.field);
        let field = string_arg(vm, field);
        if name.trim().is_empty() {
            return ScriptValue::from_f64(-9999.0);
        }
        let key = if field.trim() == "lon" { "lon" } else { "lat" };
        let n = vm
            .host
            .cx_mut()
            .script_data_fetch(&geocode_url(&name))
            .and_then(|bytes| geocode_pluck(&bytes, key))
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(-9999.0);
        ScriptValue::from_f64(n)
    });

    // sys.weather(lat, lon, "path") -> a live open-meteo value as display text.
    vm.add_method(sys, id_lut!(weather), script_args_def!(lat = NIL, lon = NIL, path = NIL), |vm, args| {
        let lat = script_value!(vm, args.lat).as_number().unwrap_or(0.0);
        let lon = script_value!(vm, args.lon).as_number().unwrap_or(0.0);
        let path = script_value!(vm, args.path);
        let path = string_arg(vm, path);
        let value = match vm.host.cx_mut().script_data_fetch(&forecast_url(lat, lon)) {
            Some(bytes) => json_pluck(&bytes, path.trim())
                .map(|v| round_display(path.trim(), v))
                .unwrap_or_else(|| PLACEHOLDER.to_string()),
            None => PLACEHOLDER.to_string(),
        };
        vm.bx.heap.new_string_from_str(&value)
    });

    // sys.weathernum(lat, lon, "path") -> the same value as a NUMBER, -9999
    // while it loads, for widget uniforms and guards.
    vm.add_method(sys, id_lut!(weathernum), script_args_def!(lat = NIL, lon = NIL, path = NIL), |vm, args| {
        let lat = script_value!(vm, args.lat).as_number().unwrap_or(0.0);
        let lon = script_value!(vm, args.lon).as_number().unwrap_or(0.0);
        let path = script_value!(vm, args.path);
        let path = string_arg(vm, path);
        let n = vm
            .host
            .cx_mut()
            .script_data_fetch(&forecast_url(lat, lon))
            .and_then(|bytes| json_pluck(&bytes, path.trim()))
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(-9999.0);
        ScriptValue::from_f64(n)
    });

    // sys.weathercond(lat, lon, "path") -> the WeatherIcon `cond` index for the
    // WMO code at `path`: 0 clear, 1 partly, 2 overcast, 3 rain, 4 storm,
    // 5 snow, 7 fog. Partly cloudy while the fetch is in flight.
    vm.add_method(sys, id_lut!(weathercond), script_args_def!(lat = NIL, lon = NIL, path = NIL), |vm, args| {
        let lat = script_value!(vm, args.lat).as_number().unwrap_or(0.0);
        let lon = script_value!(vm, args.lon).as_number().unwrap_or(0.0);
        let path = script_value!(vm, args.path);
        let path = string_arg(vm, path);
        let idx = match weather_code(vm, lat, lon, &path) {
            Some(0) => 0,
            Some(1) | Some(2) => 1,
            Some(3) => 2,
            Some(45) | Some(48) => 7,
            Some(51..=57) | Some(61..=67) | Some(80..=82) => 3,
            Some(71..=77) | Some(85) | Some(86) => 5,
            Some(95..=99) => 4,
            _ => 1,
        };
        ScriptValue::from_f64(idx as f64)
    });

    // sys.weatherword(lat, lon, "path", locale) -> the condition as text.
    vm.add_method(
        sys,
        id_lut!(weatherword),
        script_args_def!(lat = NIL, lon = NIL, path = NIL, locale = NIL),
        |vm, args| {
            let lat = script_value!(vm, args.lat).as_number().unwrap_or(0.0);
            let lon = script_value!(vm, args.lon).as_number().unwrap_or(0.0);
            let path = script_value!(vm, args.path);
            let path = string_arg(vm, path);
            let locale = script_value!(vm, args.locale);
            let zh = string_arg(vm, locale).trim().to_ascii_lowercase().starts_with("zh");
            let (en, cn) = match weather_code(vm, lat, lon, &path) {
                Some(0) => ("Clear", "晴"),
                Some(1) => ("Mainly Clear", "晴间多云"),
                Some(2) => ("Partly Cloudy", "局部多云"),
                Some(3) => ("Overcast", "阴"),
                Some(45) | Some(48) => ("Fog", "雾"),
                Some(51..=57) => ("Drizzle", "小雨"),
                Some(61..=67) => ("Rain", "雨"),
                Some(71..=77) => ("Snow", "雪"),
                Some(80..=82) => ("Showers", "阵雨"),
                Some(85) | Some(86) => ("Snow Showers", "阵雪"),
                Some(95..=99) => ("Thunderstorm", "雷暴"),
                _ => (PLACEHOLDER, PLACEHOLDER),
            };
            vm.bx.heap.new_string_from_str(if zh { cn } else { en })
        },
    );

    // sys.dayname(lat, lon, n, locale) -> the weekday label of forecast row n
    // ("Now" for 0), from the forecast's own local date once it has landed.
    vm.add_method(sys, id_lut!(dayname), script_args_def!(lat = NIL, lon = NIL, n = NIL, locale = NIL), |vm, args| {
        let lat = script_value!(vm, args.lat).as_number().unwrap_or(0.0);
        let lon = script_value!(vm, args.lon).as_number().unwrap_or(0.0);
        let n = script_value!(vm, args.n).as_number().unwrap_or(0.0).max(0.0) as usize;
        let locale = script_value!(vm, args.locale);
        let zh = string_arg(vm, locale).trim().to_ascii_lowercase().starts_with("zh");
        if n == 0 {
            return vm.bx.heap.new_string_from_str(if zh { "现在" } else { "Now" });
        }
        let from_api = vm.host.cx_mut().script_data_fetch(&forecast_url(lat, lon)).and_then(|bytes| {
            let date = json_pluck(&bytes, &format!("daily.time.{n}"))?;
            let mut it = date.trim().split('-');
            let y = it.next()?.parse::<i64>().ok()?;
            let m = it.next()?.parse::<u64>().ok()?;
            let d = it.next()?.parse::<u64>().ok()?;
            Some(weekday_from_days(days_from_civil(y, m, d)))
        });
        let wd = from_api.unwrap_or_else(|| weekday_from_days((now_unix_secs() / 86_400) as i64 + n as i64));
        vm.bx.heap.new_string_from_str(if zh { DAY_ZH[wd] } else { DAY_EN[wd] })
    });

    // sys.weekmin(lat, lon) / sys.weekmax(lat, lon) -> the week's range for a
    // TempBar's wmin/wmax. A temperate fallback while the fetch is in flight,
    // never 0/0 (a zero span collapses every bar to one colour).
    vm.add_method(sys, id_lut!(weekmin), script_args_def!(lat = NIL, lon = NIL), |vm, args| {
        let lat = script_value!(vm, args.lat).as_number().unwrap_or(0.0);
        let lon = script_value!(vm, args.lon).as_number().unwrap_or(0.0);
        ScriptValue::from_f64(week_extreme(vm, lat, lon, "daily.temperature_2m_min", false).unwrap_or(0.0))
    });
    vm.add_method(sys, id_lut!(weekmax), script_args_def!(lat = NIL, lon = NIL), |vm, args| {
        let lat = script_value!(vm, args.lat).as_number().unwrap_or(0.0);
        let lon = script_value!(vm, args.lon).as_number().unwrap_or(0.0);
        ScriptValue::from_f64(week_extreme(vm, lat, lon, "daily.temperature_2m_max", true).unwrap_or(30.0))
    });

    // sys.moonphase("name"|"name_zh"|"illumination"|"phase") -> from the clock.
    vm.add_method(sys, id_lut!(moonphase), script_args_def!(field = NIL), |vm, args| {
        let field = script_value!(vm, args.field);
        let field = string_arg(vm, field);
        let f = moon_phase_fraction();
        let value = match field.trim().to_ascii_lowercase().as_str() {
            "name" => moon_phase_name(f).to_string(),
            "name_zh" | "name_cn" => moon_phase_name_zh(f).to_string(),
            "illumination" | "illum" => format!("{}", ((1.0 - (std::f64::consts::TAU * f).cos()) * 50.0).round() as i64),
            _ => format!("{f:.2}"),
        };
        vm.bx.heap.new_string_from_str(&value)
    });
    // sys.moonnum("phase"|"illumination") -> the same as a number, for the
    // MoonPhase widget's `draw_bg.phase`.
    vm.add_method(sys, id_lut!(moonnum), script_args_def!(field = NIL), |vm, args| {
        let field = script_value!(vm, args.field);
        let field = string_arg(vm, field);
        let f = moon_phase_fraction();
        let n = match field.trim().to_ascii_lowercase().as_str() {
            "illumination" | "illum" => (1.0 - (std::f64::consts::TAU * f).cos()) * 50.0,
            _ => f,
        };
        ScriptValue::from_f64(n)
    });

    // sys.daylight(lat, lon) -> fraction of daylight elapsed (0 sunrise, 1
    // sunset; outside that range is night) for SunArc's `draw_bg.progress`.
    // 0.5 while the forecast is in flight.
    vm.add_method(sys, id_lut!(daylight), script_args_def!(lat = NIL, lon = NIL), |vm, args| {
        let lat = script_value!(vm, args.lat).as_number().unwrap_or(0.0);
        let lon = script_value!(vm, args.lon).as_number().unwrap_or(0.0);
        let progress = vm
            .host
            .cx_mut()
            .script_data_fetch(&forecast_url(lat, lon))
            .and_then(|bytes| {
                let rise = hhmm_to_minutes(&json_pluck(&bytes, "daily.sunrise.0")?)?;
                let set = hhmm_to_minutes(&json_pluck(&bytes, "daily.sunset.0")?)?;
                let offset = json_pluck(&bytes, "utc_offset_seconds")?.parse::<f64>().ok()?;
                let local = (now_unix_secs() as f64 + offset).rem_euclid(86_400.0);
                let now = local / 60.0;
                let span = set - rise;
                if span <= 0.0 {
                    return None;
                }
                Some((now - rise) / span)
            })
            .unwrap_or(0.5);
        ScriptValue::from_f64(progress)
    });

    // ---- Phase A stubs: what needs the device, the kernel or the store ----

    // sys.locale("lang"|"temp_unit") -> the host locale. No per-device locale
    // plumbing reaches the module yet: English, Celsius.
    vm.add_method(sys, id_lut!(locale), script_args_def!(field = NIL), |vm, args| {
        let field = script_value!(vm, args.field);
        let field = string_arg(vm, field);
        let out = match field.trim() {
            "temp_unit" | "units" => "c",
            _ => "en",
        };
        vm.bx.heap.new_string_from_str(out)
    });
    // sys.prefs(field) -> the user's stored choice. The store is Phase B.
    vm.add_method(sys, id_lut!(prefs), script_args_def!(field = NIL), |vm, _args| {
        vm.bx.heap.new_string_from_str("")
    });
    // sys.cities(index, field) / sys.citiesnum() -> the saved cities. Phase B.
    vm.add_method(sys, id_lut!(cities), script_args_def!(index = NIL, field = NIL), |vm, _args| {
        vm.bx.heap.new_string_from_str(PLACEHOLDER)
    });
    vm.add_method(sys, id_lut!(citiesnum), script_args_def!(), |_vm, _args| ScriptValue::from_f64(0.0));
    // sys.gps("lat"|"lon") -> the device fix. No location permission in this
    // host yet: -9999, the same "not yet" every numeric helper answers.
    vm.add_method(sys, id_lut!(gps), script_args_def!(field = NIL), |_vm, _args| ScriptValue::from_f64(-9999.0));

    vm.set_injected_global(id!(sys), sys.into());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_pluck_walks_paths_and_reduces_iso_times() {
        let body = br#"{"daily":{"temperature_2m_max":[21.6,19.2],"sunrise":["2026-09-13T06:51"]},"current":{"is_day":true}}"#;
        assert_eq!(json_pluck(body, "daily.temperature_2m_max.1").as_deref(), Some("19.2"));
        assert_eq!(json_pluck(body, "daily.sunrise.0").as_deref(), Some("06:51"));
        assert_eq!(json_pluck(body, "current.is_day").as_deref(), Some("true"));
        assert_eq!(json_pluck(body, "daily.missing.0"), None);
        assert_eq!(round_display("daily.temperature_2m_max.0", "21.6".into()), "22");
        assert_eq!(round_display("current.surface_pressure", "1013.2".into()), "1013.2");
    }

    #[test]
    fn calendar_and_moon_helpers_agree_with_known_dates() {
        // 1970-01-01 was a Thursday; 2026-09-13 is a Sunday.
        assert_eq!(weekday_from_days(0), 4);
        assert_eq!(weekday_from_days(days_from_civil(2026, 9, 13)), 0);
        assert_eq!(hhmm_to_minutes("06:51"), Some(411.0));
        let f = moon_phase_fraction();
        assert!((0.0..1.0).contains(&f));
        assert!(!moon_phase_name(f).is_empty());
        assert_eq!(geocode_url("San José"), "https://geocoding-api.open-meteo.com/v1/search?name=San+Jos%C3%A9&count=1&language=en&format=json");
    }
}
