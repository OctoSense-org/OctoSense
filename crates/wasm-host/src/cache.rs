//! Optional compiled-code reads must not hold a Wasm request behind filesystem I/O.

use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::time::Duration;

const MAX_READERS: usize = 2;
const READ_WAIT: Duration = Duration::from_millis(50);
// A larger valid compiled artifact still works by compiling the verified source.
const MAX_BYTES: u64 = 64 << 20;

pub(super) enum ReadResult {
    Hit(Vec<u8>),
    Missing,
    Unavailable,
}

#[derive(Default)]
pub(super) struct Readers {
    active: AtomicUsize,
    // Only strict cache-format tests use this per-runtime seam. Production
    // always takes the bounded worker path, including integration tests.
    #[cfg(test)]
    synchronous_for_test: bool,
}

struct Slot(Arc<Readers>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Readers {
    pub(super) fn shared() -> Arc<Self> {
        static READERS: OnceLock<Arc<Readers>> = OnceLock::new();
        READERS.get_or_init(|| Arc::new(Self::default())).clone()
    }

    pub(super) fn read(self: &Arc<Self>, path: PathBuf) -> ReadResult {
        #[cfg(test)]
        if self.synchronous_for_test {
            return match read_file(path) {
                Ok(bytes) => ReadResult::Hit(bytes),
                Err(error) if error.kind() == io::ErrorKind::NotFound => ReadResult::Missing,
                Err(_) => ReadResult::Unavailable,
            };
        }
        self.read_with(move || read_file(path), READ_WAIT)
    }

    fn read_with(
        self: &Arc<Self>,
        read: impl FnOnce() -> io::Result<Vec<u8>> + Send + 'static,
        wait: Duration,
    ) -> ReadResult {
        // There is no queue and no waiting for a slot. A stuck filesystem can
        // retain at most MAX_READERS threads, including after callers time out.
        if self
            .active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < MAX_READERS).then_some(n + 1)
            })
            .is_err()
        {
            return ReadResult::Unavailable;
        }
        let slot = Slot(self.clone());
        let (send, receive) = mpsc::sync_channel(1);
        if std::thread::Builder::new()
            .name("wasm-cache-read".into())
            .spawn(move || {
                let _slot = slot;
                // If the caller has timed out, send drops the late bytes.
                // The slot remains held until the actual read finishes (or panics).
                let _ = send.send(read());
            })
            .is_err()
        {
            return ReadResult::Unavailable;
        }
        match receive.recv_timeout(wait) {
            Ok(Ok(bytes)) => ReadResult::Hit(bytes),
            Ok(Err(error)) if error.kind() == io::ErrorKind::NotFound => ReadResult::Missing,
            _ => ReadResult::Unavailable,
        }
    }
}

fn read_file(path: PathBuf) -> io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err(io::Error::other("compiled cache exceeds its read limit"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(io::Error::other("compiled cache exceeds its read limit"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{component::Grants, Limits, Runtime};
    use std::sync::mpsc::{Receiver, Sender};
    use std::time::Instant;

    fn wait_until_released(readers: &Readers) {
        let until = Instant::now() + Duration::from_secs(2);
        while readers.active.load(Ordering::Acquire) != 0 {
            assert!(
                Instant::now() < until,
                "cache reader did not release its slot"
            );
            std::thread::yield_now();
        }
    }

    // The read cannot finish until the test permits it: no filesystem timing
    // assumptions and no sleeping worker which might finish before the assertion.
    fn block_reader(readers: &Arc<Readers>) -> (Sender<()>, Receiver<()>) {
        let (release, released) = mpsc::channel();
        let (started, start) = mpsc::channel();
        let (finished, finish) = mpsc::channel();
        let began = Instant::now();
        assert!(matches!(
            readers.read_with(
                move || {
                    started.send(()).unwrap();
                    released.recv().unwrap();
                    finished.send(()).unwrap();
                    Ok(vec![7; 4096])
                },
                Duration::from_millis(10),
            ),
            ReadResult::Unavailable
        ));
        assert!(began.elapsed() < Duration::from_secs(2));
        start.recv_timeout(Duration::from_secs(2)).unwrap();
        (release, finish)
    }

    #[test]
    fn timed_out_reads_keep_slots_bounded_and_release_after_late_completion() {
        let readers = Arc::new(Readers::default());
        let blocked: Vec<_> = (0..MAX_READERS).map(|_| block_reader(&readers)).collect();
        assert_eq!(readers.active.load(Ordering::Acquire), MAX_READERS);
        let unexpected_reads = Arc::new(AtomicUsize::new(0));
        for _ in 0..100 {
            let started = unexpected_reads.clone();
            assert!(matches!(
                readers.read_with(
                    move || {
                        started.fetch_add(1, Ordering::AcqRel);
                        Ok(Vec::new())
                    },
                    READ_WAIT
                ),
                ReadResult::Unavailable
            ));
        }
        assert_eq!(unexpected_reads.load(Ordering::Acquire), 0);
        assert_eq!(readers.active.load(Ordering::Acquire), MAX_READERS);
        for (release, finished) in blocked {
            release.send(()).unwrap();
            finished.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        // A late result must not leave either a blocked send or an occupied slot.
        wait_until_released(&readers);
        assert_eq!(unexpected_reads.load(Ordering::Acquire), 0);
        assert!(matches!(
            readers.read_with(|| Ok(vec![1, 2, 3]), Duration::from_secs(2)),
            ReadResult::Hit(bytes) if bytes == [1, 2, 3]
        ));
        wait_until_released(&readers);
    }

    #[test]
    fn failure_and_panic_release_the_slot_but_only_missing_allows_publication() {
        let readers = Arc::new(Readers::default());
        for kind in [io::ErrorKind::PermissionDenied, io::ErrorKind::NotFound] {
            let result = readers.read_with(move || Err(kind.into()), Duration::from_secs(2));
            assert!(matches!(
                (kind, result),
                (io::ErrorKind::NotFound, ReadResult::Missing)
                    | (io::ErrorKind::PermissionDenied, ReadResult::Unavailable)
            ));
            wait_until_released(&readers);
        }
        assert!(matches!(
            readers.read_with(|| panic!("injected reader panic"), Duration::from_secs(2)),
            ReadResult::Unavailable
        ));
        wait_until_released(&readers);
    }

    fn cache_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wasm-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn format_test_runtime(dir: &std::path::Path) -> Runtime {
        let mut runtime = Runtime::new(Limits::default(), Some(dir.into())).unwrap();
        runtime.cache_readers = Arc::new(Readers {
            synchronous_for_test: true,
            ..Readers::default()
        });
        runtime
    }

    fn echo_module() -> Vec<u8> {
        wat::parse_str(
            r#"(module
            (memory (export "memory") 1)
            (data (i32.const 0) "\00ok")
            (func (export "octo_alloc") (param i32) (result i32) i32.const 32)
            (func (export "octo_free") (param i32 i32))
            (func (export "echo") (param i32 i32) (result i64) i64.const 3))"#,
        )
        .unwrap()
    }

    // The returned hit flag belongs to the program actually invoked, not a
    // separate successful probe. This exercises Wasmtime deserialization.
    fn execute(runtime: &Runtime, bytes: &[u8]) -> bool {
        if crate::component::is_component(bytes) {
            let program = runtime.load_component(bytes).unwrap();
            let mut instance = runtime
                .instantiate_component(&program, &Grants::default(), None)
                .unwrap();
            assert_eq!(
                instance
                    .call_json("to_html", &serde_json::json!("# Cached"))
                    .unwrap(),
                serde_json::json!("<h1>Cached</h1>\n")
            );
            program.from_cache()
        } else {
            let program = runtime.load(bytes).unwrap();
            assert_eq!(
                runtime
                    .instantiate(&program)
                    .unwrap()
                    .call("echo", b"")
                    .unwrap(),
                b"ok"
            );
            program.from_cache()
        }
    }

    #[test]
    fn module_and_component_cache_hits_execute_after_restart_and_corruption_repairs() {
        let component = include_bytes!("../tests/fixtures/notes.component.wasm").to_vec();
        for (name, bytes) in [
            ("module-format", echo_module()),
            ("component-format", component),
        ] {
            let dir = cache_dir(name);
            let first = format_test_runtime(&dir);
            assert!(first.precompile(&bytes).unwrap());
            let path = first.cache_path(&bytes).unwrap();
            assert!(path.is_file(), "a true cache-miss must publish an artifact");
            let original = read_file(path.clone()).unwrap();
            assert!(!original.is_empty());
            drop(first);

            let second = format_test_runtime(&dir);
            assert!(!second.precompile(&bytes).unwrap());
            assert!(
                execute(&second, &bytes),
                "the invoked program must deserialize from cache"
            );
            assert_eq!(read_file(path.clone()).unwrap(), original);

            std::fs::write(&path, b"invalid compiled artifact").unwrap();
            assert!(
                !execute(&second, &bytes),
                "corruption must fall back to source compilation"
            );
            let repaired = read_file(path.clone()).unwrap();
            assert!(!repaired.is_empty());
            assert_ne!(repaired, b"invalid compiled artifact");
            assert!(
                execute(&second, &bytes),
                "the repaired artifact must deserialize and execute"
            );
            std::fs::remove_file(path).unwrap();
            assert!(
                !execute(&second, &bytes),
                "a removed artifact is a genuine miss"
            );
            assert!(
                execute(&second, &bytes),
                "the newly published artifact must execute"
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn concurrent_publication_produces_a_real_deserializable_cache_entry() {
        let dir = cache_dir("concurrent-format");
        let runtime = Arc::new(format_test_runtime(&dir));
        let bytes = Arc::new(echo_module());
        let start = Arc::new(std::sync::Barrier::new(4));
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let (runtime, bytes, start) = (runtime.clone(), bytes.clone(), start.clone());
                std::thread::spawn(move || {
                    start.wait();
                    execute(&runtime, &bytes);
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let path = runtime.cache_path(&bytes).unwrap();
        assert!(path.is_file());
        assert!(!read_file(path).unwrap().is_empty());
        drop(runtime);
        let restarted = format_test_runtime(&dir);
        assert!(execute(&restarted, &bytes));
        let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert_eq!(entries.len(), 1, "no staging files may remain");
        assert_eq!(
            entries[0].as_ref().unwrap().path().extension().unwrap(),
            "cwasm"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn oversized_cache_is_unavailable_instead_of_allocated() {
        let dir = cache_dir("oversized");
        let path = dir.join("large.cwasm");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(MAX_BYTES + 1)
            .unwrap();
        let readers = Arc::new(Readers::default());
        assert!(matches!(
            readers.read_with(move || read_file(path), Duration::from_secs(2)),
            ReadResult::Unavailable
        ));
        wait_until_released(&readers);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn blocked_readers_fall_back_for_module_and_component_without_cache_writes() {
        let dir = cache_dir("fallback");
        let mut runtime = Runtime::new(Limits::default(), Some(dir.clone())).unwrap();
        // Isolate the gate so parallel cache tests cannot occupy these slots.
        runtime.cache_readers = Arc::new(Readers::default());
        let blocked: Vec<_> = (0..MAX_READERS)
            .map(|_| block_reader(&runtime.cache_readers))
            .collect();
        let module = wat::parse_str(
            r#"(module
            (memory (export "memory") 1)
            (data (i32.const 0) "\00ok")
            (func (export "octo_alloc") (param i32) (result i32) i32.const 32)
            (func (export "octo_free") (param i32 i32))
            (func (export "echo") (param i32 i32) (result i64) i64.const 3))"#,
        )
        .unwrap();
        let component = include_bytes!("../tests/fixtures/notes.component.wasm");
        let paths = [
            runtime.cache_path(&module).unwrap(),
            runtime.cache_path(component).unwrap(),
        ];
        for path in &paths {
            std::fs::write(path, b"existing cache must stay untouched").unwrap();
        }
        let program = runtime.load(&module).unwrap();
        assert!(!program.from_cache());
        assert_eq!(
            runtime
                .instantiate(&program)
                .unwrap()
                .call("echo", b"")
                .unwrap(),
            b"ok"
        );
        let program = runtime.load_component(component).unwrap();
        assert!(!program.from_cache());
        let mut instance = runtime
            .instantiate_component(&program, &Grants::default(), None)
            .unwrap();
        assert_eq!(
            instance
                .call_json("to_html", &serde_json::json!("# Fallback"))
                .unwrap(),
            serde_json::json!("<h1>Fallback</h1>\n")
        );
        for path in &paths {
            assert_eq!(
                std::fs::read(path).unwrap(),
                b"existing cache must stay untouched"
            );
        }
        // Missing entries must not be published either while I/O is unavailable.
        for path in &paths {
            std::fs::remove_file(path).unwrap();
        }
        assert!(!runtime.load(&module).unwrap().from_cache());
        assert!(!runtime.load_component(component).unwrap().from_cache());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        for (release, finished) in blocked {
            release.send(()).unwrap();
            finished.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        wait_until_released(&runtime.cache_readers);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
