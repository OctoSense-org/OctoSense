//! A component's (ADR 0014) startup and per-call latency in the runtime,
//! against Wasm Lab's core module (ADR 0011) and native code doing the same
//! work, and the cost of the storage quota check the shell's `wasm` service
//! makes after a component's call.
//!
//!     cargo run --release -p octosense-wasm-host --example measure_component

use std::path::Path;
use std::time::{Duration, Instant};

use octosense_wasm_host::component::Grants;
use octosense_wasm_host::{Limits, Runtime};
use serde_json::{json, Value};

const COMPONENT: &[u8] = include_bytes!("../tests/fixtures/notes.component.wasm");
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

/// The walk the shell's `Storage::usage` makes over an app's jail: every
/// entry's metadata, never following a symlink.
fn tree_bytes(dir: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                total += meta.len();
            }
        }
    }
    total
}

fn main() {
    println!(
        "component: {} KiB (module: {} KiB)",
        COMPONENT.len() / 1024,
        MODULE.len() / 1024
    );
    let scratch = std::env::temp_dir().join(format!(
        "octosense-wasm-measure-component-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    let cache = scratch.join("cache");

    let t = Instant::now();
    let runtime = Runtime::new(Limits::default(), Some(cache.clone())).unwrap();
    let program = runtime.load_component(COMPONENT).unwrap();
    println!(
        "compile (Cranelift, first load): {:.1} ms",
        t.elapsed().as_secs_f64() * 1e3
    );
    let again = Runtime::new(Limits::default(), Some(cache.clone())).unwrap();
    let t = Instant::now();
    let cached = again.load_component(COMPONENT).unwrap();
    println!(
        "load from the cache: {:.2} ms (from cache: {})",
        t.elapsed().as_secs_f64() * 1e3,
        cached.from_cache()
    );
    let bare = median_us(50, || {
        drop(
            runtime
                .instantiate_component(&program, &Grants::default(), None)
                .unwrap(),
        )
    });
    let storage = scratch.join("jail");
    std::fs::create_dir_all(&storage).unwrap();
    let grants = Grants {
        storage_dir: Some(storage.clone()),
        read_only: false,
    };
    let with_files = median_us(50, || {
        drop(
            runtime
                .instantiate_component(&program, &grants, None)
                .unwrap(),
        )
    });
    println!(
        "instantiate: {:.3} ms; with the storage folder: {:.3} ms",
        bare / 1e3,
        with_files / 1e3
    );

    let mut component = runtime
        .instantiate_component(&program, &grants, None)
        .unwrap();
    let module = runtime.load(MODULE).unwrap();
    let mut guest = runtime.instantiate(&module).unwrap();
    let md = markdown();
    let md_json = json!({"markdown": md});
    println!(
        "\n{:<46} {:>10} {:>10} {:>10}",
        "per call (median of 200)", "component", "module", "native us"
    );
    let to_html = median_us(200, || {
        drop(component.call_json("to_html", &md_json).unwrap())
    });
    let md_to_html = median_us(200, || {
        drop(guest.call("md_to_html", md.as_bytes()).unwrap())
    });
    let native = median_us(200, || {
        drop(wasmlab_functions::md_to_html(md.as_bytes()).unwrap())
    });
    println!(
        "{:<46} {to_html:>10.1} {md_to_html:>10.1} {native:>10.1}",
        format!("Markdown to HTML ({} KiB, pulldown-cmark)", md.len() / 1024)
    );
    let analyze = median_us(200, || {
        drop(component.call_json("analyze", &md_json).unwrap())
    });
    println!(
        "{:<46} {analyze:>10.1} {:>10} {:>10}",
        "analyze (a record back)", "-", "-"
    );
    // A new file every time: rewriting one file over and over measures the
    // filesystem (APFS takes about 6 ms a rewrite, natively too).
    let mut n = 0;
    let mut saved = || {
        n += 1;
        json!({"markdown": md, "path": format!("out{n}.html")})
    };
    let save = median_us(200, || {
        drop(component.call_json("save_html", &saved()).unwrap())
    });
    println!(
        "{:<46} {save:>10.1} {:>10} {:>10}",
        "save_html (render, write 49 KiB)", "-", "-"
    );
    let html = component.call_json("to_html", &md_json).unwrap();
    let html = html.as_str().unwrap().to_string();
    let mut m = 0;
    let native_save = median_us(200, || {
        m += 1;
        std::fs::write(storage.join(format!("native{m}.html")), &html).unwrap()
    });
    println!(
        "{:<46} {:>10} {:>10} {native_save:>10.1}",
        "write 49 KiB natively", "-", "-"
    );
    let mut k = 0;
    let save_tiny = median_us(200, || {
        k += 1;
        drop(
            component
                .call_json(
                    "save_html",
                    &json!({"markdown": "# Hi", "path": format!("tiny{k}.html")}),
                )
                .unwrap(),
        )
    });
    println!(
        "{:<46} {save_tiny:>10.1} {:>10} {:>10}",
        "save_html (write 12 bytes)", "-", "-"
    );
    let read = json!("out1.html");
    let read_file = median_us(200, || {
        drop(component.call_json("read_file", &read).unwrap())
    });
    println!(
        "{:<46} {read_file:>10.1} {:>10} {:>10}",
        "read_file (49 KiB back)", "-", "-"
    );
    let mib: Value = json!(base64_of(&vec![7u8; 1 << 20]));
    let echo = median_us(50, || {
        drop(component.call_json("echo_bytes", &mib).unwrap())
    });
    println!(
        "{:<46} {echo:>10.1} {:>10} {:>10}",
        "echo_bytes (1 MiB, base64 both ways)", "-", "-"
    );
    let empty = json!({"query": "x", "items": [], "limit": 1});
    let smallest_module = median_us(2000, || {
        drop(
            guest
                .call("fuzzy_rank", &serde_json::to_vec(&empty).unwrap())
                .unwrap(),
        )
    });
    let smallest = median_us(2000, || {
        drop(component.call_json("count", &Value::Null).unwrap())
    });
    println!(
        "\nsmallest call: component `count` {smallest:.1} us; module `fuzzy_rank` over no items {smallest_module:.1} us"
    );

    // The quota check after a call that had the storage folder: one walk of
    // the app's jail, as `Storage::usage` makes it.
    println!("\nquota check (one walk of the jail, median of 50):");
    for files in [10usize, 1_000, 10_000] {
        let jail = scratch.join(format!("jail-{files}"));
        for i in 0..files {
            let dir = jail.join(format!("d{}", i / 100));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("f{i}")), b"x").unwrap();
        }
        let walk = median_us(50, || {
            std::hint::black_box(tree_bytes(&jail));
        });
        println!("  {files:>6} files: {:.2} ms", walk / 1e3);
    }
    let _ = std::fs::remove_dir_all(&scratch);
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
