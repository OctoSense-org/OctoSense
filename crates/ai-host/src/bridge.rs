//! A request answered on the UI thread (the camera scanner, an image
//! picker): the service asks from any thread, the request parks its
//! completion here and wakes the UI thread, which opens the platform surface
//! on its next event and completes the request from the platform's answer.
//! One request of a kind is outstanding at a time: a newer one answers the
//! older as superseded.

use makepad_widgets::makepad_platform::thread::SignalToUI;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

pub type Done<R> = Box<dyn FnOnce(R) + Send>;

/// One outstanding request whose answer comes from the UI thread: the
/// completion, an id to match a late answer against, and whether the UI
/// thread still has to open the surface for it.
pub struct Bridge<R> {
    pending: Mutex<Option<(u64, Done<R>)>>,
    open: AtomicBool,
    next: AtomicU64,
}

impl<R> Default for Bridge<R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R> Bridge<R> {
    pub const fn new() -> Self {
        Bridge { pending: Mutex::new(None), open: AtomicBool::new(false), next: AtomicU64::new(1) }
    }

    /// Park `done` (any thread) and ask the UI thread to open the surface.
    /// An earlier request still waiting is answered with `superseded`.
    pub fn begin(&self, done: Done<R>, superseded: R) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let earlier = self.pending.lock().unwrap_or_else(|e| e.into_inner()).replace((id, done));
        self.open.store(true, Ordering::Release);
        if let Some((_, earlier)) = earlier {
            earlier(superseded);
        }
        SignalToUI::set_ui_signal();
        id
    }

    /// UI thread: the request to open a surface for, once.
    pub fn take_open(&self) -> Option<u64> {
        if !self.open.swap(false, Ordering::AcqRel) {
            return None;
        }
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|(id, _)| *id)
    }

    /// Whether a request waits for its answer.
    pub fn is_pending(&self) -> bool {
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// Take the outstanding request's completion (`id`: only that one), to
    /// answer it later, e.g. from a worker that reads a file.
    pub fn take(&self, id: Option<u64>) -> Option<Done<R>> {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        match (pending.as_ref(), id) {
            (Some((current, _)), Some(id)) if *current != id => None,
            _ => pending.take().map(|(_, done)| done),
        }
    }

    /// Answer the outstanding request (`id`: only that one). False when
    /// nothing waited, e.g. a scan another part of the app started.
    pub fn finish(&self, id: Option<u64>, result: R) -> bool {
        match self.take(id) {
            Some(done) => {
                done(result);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn recorder<R: Send + 'static>() -> (Arc<Mutex<Vec<R>>>, impl Fn() -> Done<R>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        (seen, move || {
            let sink = sink.clone();
            Box::new(move |r| sink.lock().unwrap().push(r)) as Done<R>
        })
    }

    /// A scan opens the scanner once and completes from the answer, once;
    /// an answer nobody asked for (AppCard's own composer scan) is left alone.
    #[test]
    fn a_scan_opens_once_and_completes_once() {
        let bridge: Bridge<Result<String, String>> = Bridge::new();
        let (seen, done) = recorder();
        assert!(!bridge.finish(None, Ok("stray".into())));
        let id = bridge.begin(done(), Err("interrupted".into()));
        assert_eq!(bridge.take_open(), Some(id));
        assert_eq!(bridge.take_open(), None);
        assert!(bridge.is_pending());
        assert!(bridge.finish(None, Ok("OCTOS1E:abc".into())));
        assert!(!bridge.finish(None, Err("cancelled".into())));
        assert_eq!(*seen.lock().unwrap(), vec![Ok("OCTOS1E:abc".to_string())]);
    }

    /// One outstanding request: a newer one answers the older as superseded,
    /// and a late answer for the older id does not complete the newer.
    #[test]
    fn a_newer_request_supersedes_the_older() {
        let bridge: Bridge<Result<String, String>> = Bridge::new();
        let (seen, done) = recorder();
        let first = bridge.begin(done(), Err("interrupted".into()));
        let second = bridge.begin(done(), Err("interrupted".into()));
        assert_ne!(first, second);
        assert_eq!(*seen.lock().unwrap(), vec![Err("interrupted".to_string())]);
        assert!(!bridge.finish(Some(first), Ok("late".into())));
        assert!(bridge.finish(Some(second), Err("cancelled".into())));
        assert_eq!(seen.lock().unwrap().len(), 2);
        assert_eq!(seen.lock().unwrap()[1], Err("cancelled".to_string()));
    }

    /// A completion taken out (the desktop panel's file is read on a worker)
    /// is answered exactly once, by whoever took it.
    #[test]
    fn a_taken_request_is_no_longer_pending() {
        let bridge: Bridge<Result<String, String>> = Bridge::new();
        let (seen, done) = recorder();
        bridge.begin(done(), Err("interrupted".into()));
        let taken = bridge.take(None).expect("the request waits");
        assert!(!bridge.is_pending());
        assert!(!bridge.finish(None, Ok("again".into())));
        taken(Ok("read".into()));
        assert_eq!(*seen.lock().unwrap(), vec![Ok("read".to_string())]);
    }
}
