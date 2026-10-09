//! Components (ADR 0014): typed exports called with JSON, and the WASI 0.2
//! subset an app's component reaches. The fixtures are built from
//! `tests/component-guest` with plain `cargo build --target wasm32-wasip2`
//! (`build.sh` there).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use octosense_wasm_host::component::{is_component, Grants, HostCalls};
use octosense_wasm_host::{CallError, Limits, LoadError, Runtime};
use serde_json::json;

const HOSTCALL: &[u8] = include_bytes!("fixtures/hostcall.component.wasm");
const FETCH: &[u8] = include_bytes!("fixtures/fetch.component.wasm");
const NOTES: &[u8] = include_bytes!("fixtures/notes.component.wasm");
const NETPROBE: &[u8] = include_bytes!("fixtures/netprobe.component.wasm");

fn runtime() -> Runtime {
    Runtime::new(
        Limits {
            deadline: Duration::from_millis(500),
            memory_bytes: 64 << 20,
            ..Limits::default()
        },
        None,
    )
    .unwrap()
}

/// A fresh folder standing in for the app's storage.
fn storage() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "wasm-host-component-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_component_is_told_apart_from_a_module() {
    assert!(is_component(NOTES));
    assert!(!is_component(&wat::parse_str("(module)").unwrap()));
}

#[test]
fn exports_are_listed_with_their_wit_signatures() {
    let program = runtime().load_component(NOTES).unwrap();
    let names: Vec<&str> = program.exports().iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "analyze",
            "append_text",
            "count",
            "delete_file",
            "echo_bytes",
            "file_info",
            "grow",
            "now_ms",
            "random_u64",
            "read_file",
            "save_html",
            "set_len",
            "spin",
            "to_html"
        ]
    );
    let analyze = &program.exports()[0];
    assert_eq!(
        analyze.params,
        [("markdown".to_string(), "string".to_string())]
    );
    assert_eq!(
        analyze.result.as_deref(),
        Some("record { words: u32, lines: u32, headings: list<string> }")
    );
}

#[test]
fn an_unmodified_crate_runs_and_records_come_back_as_objects() {
    let rt = runtime();
    let program = rt.load_component(NOTES).unwrap();
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    let html = instance
        .call_json("to_html", &json!({"markdown": "# Hi\n\nThere *you*"}))
        .unwrap();
    assert_eq!(html, json!("<h1>Hi</h1>\n<p>There <em>you</em></p>\n"));
    // A one-parameter function also takes the bare value.
    let stats = instance
        .call_json("analyze", &json!("# Title\n## Part\nsome words here"))
        .unwrap();
    assert_eq!(
        stats,
        json!({"words": 7, "lines": 3, "headings": ["Title", "Part"]})
    );
    // And an array in parameter order, as every other function does.
    let stats = instance.call_json("analyze", &json!(["# One"])).unwrap();
    assert_eq!(stats, json!({"words": 2, "lines": 1, "headings": ["One"]}));
}

#[test]
fn state_persists_across_calls_on_one_instance() {
    let rt = runtime();
    let program = rt.load_component(NOTES).unwrap();
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    assert_eq!(instance.call_json("count", &json!(null)).unwrap(), json!(1));
    assert_eq!(instance.call_json("count", &json!({})).unwrap(), json!(2));
    let mut fresh = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    assert_eq!(fresh.call_json("count", &json!([])).unwrap(), json!(1));
}

#[test]
fn clocks_randomness_and_bytes_work() {
    let rt = runtime();
    let program = rt.load_component(NOTES).unwrap();
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    let now = instance.call_json("now_ms", &json!(null)).unwrap();
    assert!(now.as_u64().unwrap() > 1_700_000_000_000, "{now}");
    let a = instance.call_json("random_u64", &json!(null)).unwrap();
    let b = instance.call_json("random_u64", &json!(null)).unwrap();
    assert_ne!(a, b);
    // list<u8> travels as base64.
    let echoed = instance
        .call_json("echo_bytes", &json!("AAEC/w=="))
        .unwrap();
    assert_eq!(echoed, json!("AAEC/w=="));
    // An array is the list itself, unless only as its one argument.
    for args in [json!([0, 1, 2, 255]), json!([[0, 1, 2, 255]])] {
        let echoed = instance.call_json("echo_bytes", &args).unwrap();
        assert_eq!(echoed, json!("AAEC/w=="), "{args}");
    }
    assert_eq!(
        instance
            .call_json("echo_bytes", &json!([0, 1, 2, 255]))
            .unwrap(),
        json!("AAEC/w==")
    );
}

#[test]
fn files_live_only_in_the_granted_storage_folder() {
    let rt = runtime();
    let program = rt.load_component(NOTES).unwrap();
    let dir = storage();
    let grants = Grants {
        storage_dir: Some(dir.clone()),
        read_only: false,
        http_hosts: Vec::new(),
    };
    let mut instance = rt.instantiate_component(&program, &grants, None).unwrap();
    let written = instance
        .call_json(
            "save_html",
            &json!({"markdown": "# Saved", "path": "out.html"}),
        )
        .unwrap();
    assert_eq!(written, json!(15));
    assert_eq!(
        std::fs::read_to_string(dir.join("out.html")).unwrap(),
        "<h1>Saved</h1>\n"
    );
    assert_eq!(
        instance
            .call_json("read_file", &json!("/out.html"))
            .unwrap(),
        json!("<h1>Saved</h1>\n")
    );
    // Nothing outside the folder: escaping it fails inside the guest.
    for path in ["../../../../etc/hosts", "/../etc/hosts"] {
        match instance.call_json("read_file", &json!(path)) {
            Err(CallError::Guest(_)) => {}
            other => panic!("{path}: {other:?}"),
        }
    }
    // Read-only (a used-up quota): its files read, nothing writes.
    let read_only = Grants {
        storage_dir: Some(dir.clone()),
        read_only: true,
        http_hosts: Vec::new(),
    };
    let mut reader = rt
        .instantiate_component(&program, &read_only, None)
        .unwrap();
    assert_eq!(
        reader.call_json("read_file", &json!("out.html")).unwrap(),
        json!("<h1>Saved</h1>\n")
    );
    for path in ["out.html", "new.html"] {
        assert!(matches!(
            reader.call_json("save_html", &json!({"markdown": "x", "path": path})),
            Err(CallError::Guest(_))
        ));
    }
    assert!(!dir.join("new.html").exists());
    assert_eq!(
        std::fs::read_to_string(dir.join("out.html")).unwrap(),
        "<h1>Saved</h1>\n"
    );
    // Without a storage grant there is no filesystem at all.
    let mut bare = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    assert!(matches!(
        bare.call_json("save_html", &json!({"markdown": "x", "path": "x.html"})),
        Err(CallError::Guest(_))
    ));
    let _ = std::fs::remove_dir_all(dir);
}

/// The storage budget: growth past it fails in the guest as a full disk,
/// while rewriting (truncating first), shrinking and deleting give bytes
/// back. A guest's error does not spend the instance.
#[test]
fn the_storage_budget_refuses_growth_and_returns_freed_bytes() {
    let rt = runtime();
    let program = rt.load_component(NOTES).unwrap();
    let dir = storage();
    let grants = Grants {
        storage_dir: Some(dir.clone()),
        read_only: false,
        http_hosts: Vec::new(),
    };
    let mut instance = rt.instantiate_component(&program, &grants, None).unwrap();
    assert_eq!(instance.storage_budget(), None);
    let full = |result: Result<serde_json::Value, CallError>| match result {
        Err(CallError::Guest(why)) => {
            assert!(why.ends_with("the storage budget is used up"), "{why}")
        }
        other => panic!("expected a full disk, got {other:?}"),
    };
    // "# Saved" renders to 15 bytes.
    let save = |path: &str| json!({"markdown": "# Saved", "path": path});
    instance.set_storage_budget(Some(20));
    assert_eq!(
        instance.call_json("save_html", &save("a.html")).unwrap(),
        json!(15)
    );
    assert_eq!(instance.storage_budget(), Some(5));
    // Another 15 bytes do not fit (write-via-stream); the file stays empty.
    full(instance.call_json("save_html", &save("b.html")));
    assert_eq!(std::fs::metadata(dir.join("b.html")).unwrap().len(), 0);
    assert!(!instance.spent());
    // Rewriting a.html truncates it first: its 15 bytes come back.
    assert_eq!(
        instance.call_json("save_html", &save("a.html")).unwrap(),
        json!(15)
    );
    assert_eq!(instance.storage_budget(), Some(5));
    // Appending (append-via-stream) charges every byte.
    assert_eq!(
        instance
            .call_json("append_text", &json!(["a.html", "12345"]))
            .unwrap(),
        json!(20)
    );
    full(instance.call_json("append_text", &json!(["a.html", "x"])));
    // Growing a file's length (set-size) is growth; shrinking gives back.
    full(instance.call_json("set_len", &json!(["a.html", 1 << 30])));
    assert_eq!(std::fs::metadata(dir.join("a.html")).unwrap().len(), 20);
    instance
        .call_json("set_len", &json!(["a.html", 10]))
        .unwrap();
    assert_eq!(instance.storage_budget(), Some(10));
    // Deleting (unlink-file-at) gives a file's bytes back.
    instance.call_json("delete_file", &json!("a.html")).unwrap();
    assert_eq!(instance.storage_budget(), Some(20));
    assert_eq!(
        instance.call_json("file_info", &json!("b.html")).unwrap(),
        json!({"byte_count": 0, "kind": "regular_file"})
    );
    // Without a ceiling, anything.
    instance.set_storage_budget(None);
    assert_eq!(
        instance
            .call_json("append_text", &json!(["big.txt", "x".repeat(100_000)]))
            .unwrap(),
        json!(100_000)
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// A local HTTP server: every connection gets `reply` (or, with `None`, an
/// accepted connection that never answers). Its `host:port`.
fn serve(reply: Option<&'static str>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming().flatten() {
            let Some(body) = reply else {
                held.push(stream);
                continue;
            };
            let mut stream = stream;
            let mut request = Vec::new();
            let mut byte = [0u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                if std::io::Read::read(&mut stream, &mut byte).unwrap_or(0) == 0 {
                    break;
                }
                request.push(byte[0]);
            }
            let answer = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = std::io::Write::write_all(&mut stream, answer.as_bytes());
        }
    });
    address
}

/// `wasi:http` reaches exactly the app's hosts, by a script's rule; anything
/// else is refused inside the component and logged.
#[test]
fn http_reaches_only_the_apps_hosts() {
    let rt = runtime();
    let program = rt.load_component(FETCH).unwrap();
    let server = serve(Some("hello"));
    let port = server.rsplit(':').next().unwrap().to_string();
    let grants = Grants {
        http_hosts: vec![server.clone()],
        ..Grants::default()
    };
    let mut instance = rt.instantiate_component(&program, &grants, None).unwrap();
    assert_eq!(
        instance
            .call_json("get", &json!(format!("http://{server}/hi")))
            .unwrap(),
        json!("200 hello")
    );
    // The same server under another name is another host.
    match instance.call_json("get", &json!(format!("http://localhost:{port}/hi"))) {
        Err(CallError::Guest(why)) => assert!(why.contains("HttpRequestDenied"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(
        instance.take_logs().iter().any(|l| l
            == &format!(
            "a request to localhost:{port} was refused: it is not one of the app's network hosts"
        )),
        "the refusal is logged"
    );
    // No hosts, no network.
    let mut offline = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    match offline.call_json("get", &json!(format!("http://{server}/hi"))) {
        Err(CallError::Guest(why)) => assert!(why.contains("HttpRequestDenied"), "{why}"),
        other => panic!("{other:?}"),
    }
}

/// A request waits outside the guest, where the epoch check cannot end it:
/// its timeouts are clamped to the call's deadline, so a server that never
/// answers ends the call instead of holding the worker.
#[test]
fn a_request_that_never_answers_ends_at_the_deadline() {
    let rt = Runtime::new(
        Limits {
            network_deadline: Duration::from_millis(800),
            ..Limits::default()
        },
        None,
    )
    .unwrap();
    let program = rt.load_component(FETCH).unwrap();
    let server = serve(None);
    let grants = Grants {
        http_hosts: vec![server.clone()],
        ..Grants::default()
    };
    let mut instance = rt.instantiate_component(&program, &grants, None).unwrap();
    let started = std::time::Instant::now();
    let result = instance.call_json("get", &json!(format!("http://{server}/")));
    assert!(result.is_err(), "{result:?}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "ended after {:?}",
        started.elapsed()
    );
}

/// An embedder's host services for the `hostcall` component: it records
/// each call, answers `notes.get`, refuses the rest, and makes `slow.wait`
/// wait out the call's deadline.
#[derive(Default)]
struct Services(std::sync::Mutex<Vec<(String, String)>>);

impl HostCalls for Services {
    fn request(&self, service: &str, args: &str, deadline: Instant) -> Result<String, String> {
        self.0.lock().unwrap().push((service.into(), args.into()));
        match service {
            "notes.get" => Ok(format!(r#"{{"echo":{args}}}"#)),
            "slow.wait" => {
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                Err("past the call's deadline".into())
            }
            _ => Err(format!("this app was not granted {service}")),
        }
    }
}

/// `octosense:host`: a component calls its app's host services through the
/// embedder, which decides what it may call; without one it reaches none,
/// and a call's deadline reaches the embedder.
#[test]
fn a_component_calls_its_apps_host_services_through_the_embedder() {
    let rt = runtime();
    let program = rt.load_component(HOSTCALL).unwrap();
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    match instance.call_json("call", &json!(["notes.get", "{}"])) {
        Err(CallError::Guest(why)) => assert!(why.contains("no host services"), "{why}"),
        other => panic!("{other:?}"),
    }
    let services = Arc::new(Services::default());
    instance.set_host_calls(Some(services.clone()));
    assert_eq!(
        instance
            .call_json("call", &json!(["notes.get", r#"{"id":1}"#]))
            .unwrap(),
        json!(r#"{"echo":{"id":1}}"#)
    );
    match instance.call_json("call", &json!(["mail.send", "{}"])) {
        Err(CallError::Guest(why)) => assert!(why.contains("not granted mail.send"), "{why}"),
        other => panic!("{other:?}"),
    }
    let started = Instant::now();
    assert!(instance
        .call_json("call", &json!(["slow.wait", "{}"]))
        .is_err());
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(services.0.lock().unwrap().len(), 3);
}

#[test]
fn the_deadline_and_the_memory_cap_end_a_call_and_spend_the_instance() {
    let rt = runtime();
    let program = rt.load_component(NOTES).unwrap();
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    assert_eq!(
        instance.call_json("spin", &json!(null)),
        Err(CallError::Deadline)
    );
    assert!(instance.spent());
    assert_eq!(
        instance.call_json("count", &json!(null)),
        Err(CallError::Spent)
    );
    let mut other = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    match other.call_json("grow", &json!(128)) {
        Err(CallError::Trap(why)) => assert!(why.contains("memory"), "{why}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn arguments_that_do_not_fit_are_refused_with_the_parameter_name() {
    let rt = runtime();
    let program = rt.load_component(NOTES).unwrap();
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    match instance.call_json("save_html", &json!({"markdown": 3, "path": "x"})) {
        Err(CallError::Guest(why)) => assert!(why.starts_with("markdown:"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        instance.call_json("nope", &json!(null)),
        Err(CallError::NoSuchFunction(_))
    ));
}

#[test]
fn a_component_that_imports_sockets_is_refused() {
    match runtime().load_component(NETPROBE) {
        Err(LoadError::Import(name)) => assert!(name.starts_with("wasi:sockets/"), "{name}"),
        Err(other) => panic!("{other:?}"),
        Ok(_) => panic!("loaded a component that imports sockets"),
    }
}
