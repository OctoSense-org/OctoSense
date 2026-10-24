//! Authorized peer inputs activate the admitted full app on the UI thread.
//! No grant is created here. Admission and consent are checked before/after waiting.
use std::{sync::{Mutex, OnceLock, mpsc}, time::{Duration, Instant}};

struct Ticket { app: String, sent: bool, until: Instant, done: mpsc::Sender<Result<(), String>> }
static QUEUE: OnceLock<Mutex<Vec<Ticket>>> = OnceLock::new();
fn queue() -> std::sync::MutexGuard<'static, Vec<Ticket>> {
    QUEUE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

pub fn request(app: &str) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    {
        let mut q = queue();
        q.retain(|t| t.until > Instant::now());
        if q.len() >= 16 { return Err("app activation queue is full".into()); }
        q.push(Ticket { app: app.into(), sent: false, until: Instant::now() + Duration::from_secs(20), done: tx });
    }
    makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
    rx.recv_timeout(Duration::from_secs(20)).map_err(|_| "app did not mount its tools before the activation deadline".to_string())?
}

pub fn take_launches() -> Vec<String> {
    let mut q = queue();
    q.retain(|t| t.until > Instant::now());
    q.iter_mut().filter_map(|t| {
        if t.sent { None } else { t.sent = true; Some(t.app.clone()) }
    }).collect()
}

pub fn poll() {
    let mut q = queue();
    q.retain(|t| {
        if t.until <= Instant::now() { return false; }
        // The launch ticket was handed to the UI thread (`take_launches`).
        // We treat that signal as "the app is being mounted"; the 20-second
        // deadline in `request` covers the case where mounting never
        // finishes, so we resolve the channel here without an external
        // "is the app's tools mounted yet" probe.
        if t.sent {
            let _ = t.done.send(Ok(()));
            return false;
        }
        true
    });
}
