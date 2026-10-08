//! Startup and per-call latency of Wasm Lab's functions in the runtime,
//! against the same functions called natively.
//!
//!     cargo run --release -p octosense-wasm-host --example measure

use std::time::{Duration, Instant};

use octosense_wasm_host::{Limits, Runtime};
use serde_json::{json, Value};

const MODULE: &[u8] = include_bytes!("../../../apps/wasmlab/bundle/fns/wasmlab.wasm");

/// Median of `runs` timings of `f`, in microseconds.
fn median_us(runs: usize, mut f: impl FnMut()) -> f64 {
    let mut times: Vec<Duration> = (0..runs)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .collect();
    times.sort();
    times[runs / 2].as_secs_f64() * 1e6
}

fn markdown() -> String {
    let mut text = String::new();
    for i in 0..200 {
        text.push_str(&format!(
            "## Section {i}\n\nSome *emphasis*, **strong** text, `code` and a [link](https://example.org/{i}).\n\n- [x] done {i}\n- [ ] todo {i}\n\n| key | value |\n|---|---|\n| a{i} | {i} |\n\n"
        ));
    }
    text
}

fn day() -> Value {
    let busy: Vec<[String; 2]> = (0..30)
        .map(|i| {
            let start = 8 * 60 + i * 19;
            [format!("{:02}:{:02}", start / 60, start % 60), format!("{:02}:{:02}", (start + 11) / 60, (start + 11) % 60)]
        })
        .collect();
    json!({"day_start": "08:00", "day_end": "19:00", "duration": 30, "step": 5, "busy": busy, "max": 50})
}

fn items() -> Value {
    let words = ["calendar", "calculator", "camera", "clock", "mail", "maps", "notes", "news", "photos", "weather"];
    let items: Vec<String> = (0..500).map(|i| format!("{} {} {i}", words[i % 10], words[(i * 7) % 10])).collect();
    json!({"query": "cal not", "items": items, "limit": 20})
}

fn texts() -> Value {
    let old: Vec<String> = (0..1000).map(|i| format!("line {i}: the quick brown fox")).collect();
    let mut new = old.clone();
    for i in (0..1000).step_by(50) {
        new[i] = format!("line {i}: the quick red fox");
    }
    new.insert(500, "an inserted line".into());
    json!({"old": old.join("\n"), "new": new.join("\n"), "context": 3})
}

fn main() {
    println!("module: {} KiB", MODULE.len() / 1024);
    let cache = std::env::temp_dir().join(format!("octosense-wasm-measure-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);

    let t = Instant::now();
    let runtime = Runtime::new(Limits::default(), Some(cache.clone())).unwrap();
    let program = runtime.load(MODULE).unwrap();
    println!("compile (Cranelift, first load): {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    let again = Runtime::new(Limits::default(), Some(cache.clone())).unwrap();
    let t = Instant::now();
    let cached = again.load(MODULE).unwrap();
    println!("load from the cache: {:.2} ms (from cache: {})", t.elapsed().as_secs_f64() * 1e3, cached.from_cache());
    let t = Instant::now();
    let mut guest = runtime.instantiate(&program).unwrap();
    println!("instantiate: {:.3} ms", t.elapsed().as_secs_f64() * 1e3);
    let _ = std::fs::remove_dir_all(&cache);

    let md = markdown();
    let (day, items, texts) = (day(), items(), texts());
    let json_in = |v: &Value| serde_json::to_vec(v).unwrap();

    println!("\n{:<44} {:>10} {:>10} {:>7}", "per call (median of 200)", "wasm us", "native us", "ratio");
    let rows: Vec<(String, f64, f64)> = vec![
        (
            format!("md_to_html ({} KiB of Markdown)", md.len() / 1024),
            median_us(200, || drop(guest.call("md_to_html", md.as_bytes()).unwrap())),
            median_us(200, || drop(wasmlab_functions::md_to_html(md.as_bytes()).unwrap())),
        ),
        (
            "find_slots (30 busy intervals, 5-min grid)".into(),
            median_us(200, || drop(guest.call("find_slots", &json_in(&day)).unwrap())),
            median_us(200, || drop(serde_json::to_vec(&wasmlab_functions::find_slots(serde_json::from_slice(&json_in(&day)).unwrap()).unwrap()).unwrap())),
        ),
        (
            "fuzzy_rank (500 items)".into(),
            median_us(200, || drop(guest.call("fuzzy_rank", &json_in(&items)).unwrap())),
            median_us(200, || drop(serde_json::to_vec(&wasmlab_functions::fuzzy_rank(serde_json::from_slice(&json_in(&items)).unwrap()).unwrap()).unwrap())),
        ),
        (
            "text_diff (1000 lines, 21 changes)".into(),
            median_us(200, || drop(guest.call("text_diff", &json_in(&texts)).unwrap())),
            median_us(200, || drop(serde_json::to_vec(&wasmlab_functions::text_diff(serde_json::from_slice(&json_in(&texts)).unwrap()).unwrap()).unwrap())),
        ),
    ];
    for (name, wasm, native) in rows {
        println!("{name:<44} {wasm:>10.1} {native:>10.1} {:>6.1}x", wasm / native);
    }
    let empty = json!({"query": "x", "items": [], "limit": 1});
    let call = median_us(2000, || drop(guest.call("fuzzy_rank", &json_in(&empty)).unwrap()));
    println!("\nsmallest call (fuzzy_rank over no items, JSON both ways): {call:.1} us");
}
