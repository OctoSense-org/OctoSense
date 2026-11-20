use octosense_appstore::services::{self, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::Area as EngineArea;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

struct NoSheet;

impl ServiceHost for NoSheet {
    fn open_sheet(&mut self, _: String) {
        panic!("an engine raised a sheet")
    }
    fn close_sheet(&mut self) {
        panic!("an engine closed a sheet")
    }
}

struct Ping;

impl HostService for Ping {
    fn family(&self) -> &'static str {
        "probe"
    }

    fn call(&mut self, _: ServiceCall, reply: Replier, _: &mut dyn ServiceHost) {
        reply.send(Ok(json!("pong")));
    }
}

fn dispatch(root: &Path, method: &str, heap: usize, marker: usize) {
    let call = ServiceCall {
        app_id: "os.sheets".into(),
        service: method.into(),
        args: json!({"marker": marker}),
        from_sheet: false,
        may_prompt: true,
        host_dir: root.to_owned(),
    };
    services::dispatch(call, heap, 1, &mut NoSheet);
}

fn answer(heap: usize) -> Result<Value, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some((_, id, result)) = services::take_replies_for(&[heap]).pop() {
            assert_eq!(id, 1);
            return result.map(|s| serde_json::from_str(&s).unwrap());
        }
        assert!(Instant::now() < deadline, "no reply for {heap}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn blocked_engine_leaves_dispatch_and_registry_available() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().to_owned();
    let (entered, started) = mpsc::channel();
    let (release, hold) = mpsc::channel();
    let hold = Mutex::new(hold);
    let executed = Arc::new(Mutex::new(Vec::new()));
    let recorded = executed.clone();
    octosense_sheets_service::set_area_resolver(Some(Arc::new(move |call| {
        let marker = call.args["marker"].as_u64().unwrap();
        recorded.lock().unwrap().push(marker);
        if marker == 1 {
            entered.send(()).unwrap();
            hold.lock().unwrap().recv().unwrap();
        }
        Ok(EngineArea::new(root.clone(), None, true))
    })));
    octosense_sheets_service::register();
    services::register_host_service(Box::new(Ping));
    let (returned, available) = mpsc::channel();
    let root = folder.path().to_owned();
    let caller = std::thread::spawn(move || {
        dispatch(&root, "sheet.new", 100, 1);
        dispatch(&root, "probe.ping", 101, 0);
        returned.send(answer(101)).unwrap();
    });
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    // An unrelated host service can complete while the engine is blocked.
    let responsive = available.recv_timeout(Duration::from_secs(3));
    if responsive != Ok(Ok(json!("pong"))) {
        release.send(()).unwrap();
        caller.join().unwrap();
        panic!("the engine held the caller or the registry lock: {responsive:?}");
    }
    caller.join().unwrap();

    dispatch(folder.path(), "sheet.new", 102, 2);
    services::cancel_heap(102);
    // Reusing both isolate and request ids must not revive the old job.
    dispatch(folder.path(), "sheet.new", 102, 3);
    dispatch(folder.path(), "sheet.new", 103, 4);

    // Capture a real Card scope, then rebind it while its engine call waits.
    use octosense_appstore::makepad_widgets::{self, *};
    let mut cx = Cx::new(Box::new(|_, _| {}));
    let mut card = cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        let value = vm.eval(script! {use mod.widgets.* Splash{}});
        Splash::script_from_value(vm, value)
    });
    card.set_host_tag(&mut cx, Some("os.sheets".into()));
    card.set_sandbox_dir(&mut cx, Some(folder.path().to_owned()));
    card.set_text(&mut cx, "View {}");
    let real_heap = card.isolate_heap_key(&mut cx).unwrap();
    dispatch(folder.path(), "sheet.new", real_heap, 5);
    // A blocked engine only accepts a bounded backlog. Refusals cannot
    // strand the caller or execute later when the backlog clears.
    for heap in 200..260 {
        dispatch(folder.path(), "sheet.new", heap, heap);
    }
    assert!(answer(259).unwrap_err().contains("queue is full"));
    card.set_sandbox_dir(&mut cx, Some(folder.path().join("replacement")));
    // No other call is submitted between the rebind and the old job running.
    release.send(()).unwrap();
    assert!(answer(real_heap)
        .unwrap_err()
        .contains("storage scope changed"));
    for heap in [100, 102, 103] {
        answer(heap).unwrap();
    }
    for heap in 200..259 {
        let _ = answer(heap);
    }
    let markers = executed.lock().unwrap();
    assert_eq!(&markers[..3], &[1, 3, 4]);
    assert!(!markers.contains(&5));
    assert!(!markers.contains(&259));
}
