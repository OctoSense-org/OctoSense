//! The isolate owner captures a worker-safe storage lifetime on submission.
//! App Hub can cancel a heap before Makepad reclaims it. Give the replacement
//! a new flag even when its address, tag and storage are identical.
use octosense_appstore::makepad_widgets::splash_storage::{
    storage_lifetime_for_heap, StorageLifetime,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Weak};

static HEAPS: LazyLock<Mutex<HashMap<usize, Weak<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
pub struct Isolate {
    pub heap: usize,
    storage: Option<StorageLifetime>,
    live: Arc<AtomicBool>,
}

impl Isolate {
    /// Called on the submitting thread. An agent/component has no Makepad
    /// storage lifetime and its open documents retain their idle expiry.
    pub fn capture(heap: usize, app: &str) -> Self {
        let storage = storage_lifetime_for_heap(heap, app);
        let mut heaps = HEAPS.lock().unwrap_or_else(|e| e.into_inner());
        heaps.retain(|_, flag| flag.strong_count() != 0);
        let entry = heaps.entry(heap).or_default();
        let live = entry.upgrade().unwrap_or_else(|| {
            let live = Arc::new(AtomicBool::new(true));
            *entry = Arc::downgrade(&live);
            live
        });
        Self {
            heap,
            storage,
            live,
        }
    }

    pub fn is_bound(&self) -> bool {
        self.storage.is_some()
    }

    pub fn is_live(&self) -> bool {
        self.live.load(Ordering::Acquire)
            && self.storage.as_ref().is_none_or(StorageLifetime::is_live)
    }
}

pub(crate) fn close(heap: usize) -> bool {
    let entry = HEAPS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&heap);
    if let Some(live) = entry.and_then(|e| e.upgrade()) {
        live.store(false, Ordering::Release);
        true
    } else {
        false
    }
}
