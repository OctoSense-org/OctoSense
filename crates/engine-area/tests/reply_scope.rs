use octosense_appstore::services::{self, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::{worker, Area, Slot};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

struct Reader {
    area: Arc<Slot>,
    has_data: mpsc::Sender<()>,
    finish: Arc<Mutex<mpsc::Receiver<()>>>,
}

impl HostService for Reader {
    fn family(&self) -> &'static str {
        "readprobe"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _: &mut dyn ServiceHost) {
        let area = self.area.clone();
        let has_data = self.has_data.clone();
        let finish = self.finish.clone();
        worker::call(call, reply, move |call, _| {
            let file = area.area(&call, "readprobe")?.root.join("old-account");
            let data = std::fs::read_to_string(file).unwrap();
            has_data.send(()).unwrap();
            finish.lock().unwrap().recv().unwrap();
            Ok(json!(data))
        });
    }
}

struct NoSheet;
impl ServiceHost for NoSheet {
    fn open_sheet(&mut self, _: String) {
        unreachable!()
    }
    fn close_sheet(&mut self) {
        unreachable!()
    }
}

#[test]
fn a_read_that_finishes_after_the_account_changes_cannot_reply_with_the_old_data() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_owned();
    std::fs::write(root.join("old-account"), "old account data").unwrap();
    let signed_in = Arc::new(AtomicBool::new(true));
    let check = signed_in.clone();
    let areas = Arc::new(Slot::new());
    areas.set(Some(Arc::new(move |_| {
        let check = check.clone();
        Ok(
            Area::new(root.clone(), None, false).with_scope_check(Arc::new(move || {
                if check.load(Ordering::Acquire) {
                    Ok(())
                } else {
                    Err("signed out".into())
                }
            })),
        )
    })));
    let (has_data, started) = mpsc::channel();
    let (release, finish) = mpsc::channel();
    services::register_host_service(Box::new(Reader {
        area: areas,
        has_data,
        finish: Arc::new(Mutex::new(finish)),
    }));
    services::dispatch(
        ServiceCall {
            app_id: "os.fixture".into(),
            service: "readprobe.read".into(),
            args: json!({}),
            from_sheet: false,
            may_prompt: false,
            host_dir: dir.path().to_owned(),
        },
        601,
        1,
        &mut NoSheet,
    );
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    signed_in.store(false, Ordering::Release);
    release.send(()).unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some((_, _, answer)) = services::take_replies_for(&[601]).pop() {
            assert_eq!(answer, Err("signed out".into()));
            break;
        }
        assert!(Instant::now() < deadline, "no worker reply");
        std::thread::sleep(Duration::from_millis(1));
    }
}
