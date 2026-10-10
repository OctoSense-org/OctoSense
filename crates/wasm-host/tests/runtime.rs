//! The runtime against a hand-written guest that follows the ABI: a bump
//! allocator, a function that works, and functions that misbehave in each
//! way the containment has to stop.

use std::time::{Duration, Instant};

use octosense_wasm_host::{CallError, Instance, Limits, LoadError, Runtime};

const GUEST: &str = r#"
(module
  (import "octo" "log" (func $log (param i32 i32)))
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 1024))
  (global $count (mut i32) (i32.const 0))
  (data (i32.const 16) "\01nope")
  (data (i32.const 32) "\00")

  (func $alloc (export "octo_alloc") (param $len i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $heap))
    (block $fits
      (br_if $fits (i32.le_u (i32.add (local.get $p) (local.get $len))
                             (i32.mul (memory.size) (i32.const 65536))))
      (drop (memory.grow (i32.add (i32.shr_u (local.get $len) (i32.const 16)) (i32.const 1)))))
    (global.set $heap (i32.and (i32.add (i32.add (local.get $p) (local.get $len)) (i32.const 7))
                               (i32.const -8)))
    (local.get $p))

  (func (export "octo_free") (param i32 i32))

  (func $pack (param $p i32) (param $n i32) (result i64)
    (i64.or (i64.shl (i64.extend_i32_u (local.get $p)) (i64.const 32))
            (i64.extend_i32_u (local.get $n))))

  ;; The input in upper case.
  (func (export "upper") (param $ptr i32) (param $len i32) (result i64)
    (local $out i32) (local $i i32) (local $c i32)
    (local.set $out (call $alloc (i32.add (local.get $len) (i32.const 1))))
    (i32.store8 (local.get $out) (i32.const 0))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $len)))
        (local.set $c (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
        (if (i32.and (i32.ge_u (local.get $c) (i32.const 97)) (i32.le_u (local.get $c) (i32.const 122)))
          (then (local.set $c (i32.sub (local.get $c) (i32.const 32)))))
        (i32.store8 (i32.add (i32.add (local.get $out) (i32.const 1)) (local.get $i)) (local.get $c))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (call $pack (local.get $out) (i32.add (local.get $len) (i32.const 1))))

  ;; An error of the guest's own.
  (func (export "fail") (param i32 i32) (result i64)
    (call $pack (i32.const 16) (i32.const 5)))

  ;; An endless loop.
  (func (export "spin") (param i32 i32) (result i64)
    (loop $forever (br $forever))
    (unreachable))

  ;; A panic.
  (func (export "boom") (param i32 i32) (result i64)
    (unreachable))

  ;; Runaway recursion.
  (func $deep (export "deep") (param i32 i32) (result i64)
    (call $deep (local.get 0) (local.get 1)))

  ;; A gigabyte more memory.
  (func (export "hog") (param i32 i32) (result i64)
    (drop (memory.grow (i32.const 16384)))
    (call $pack (i32.const 32) (i32.const 1)))

  ;; Logs its input.
  (func (export "say") (param $ptr i32) (param $len i32) (result i64)
    (call $log (local.get $ptr) (local.get $len))
    (call $pack (i32.const 32) (i32.const 1)))

  ;; How many times it was called, as one digit.
  (func (export "count") (param i32 i32) (result i64)
    (global.set $count (i32.add (global.get $count) (i32.const 1)))
    (i32.store8 (i32.const 40) (i32.const 0))
    (i32.store8 (i32.const 41) (i32.add (i32.const 48) (global.get $count)))
    (call $pack (i32.const 40) (i32.const 2))))
"#;

fn limits() -> Limits {
    Limits {
        deadline: Duration::from_millis(200),
        memory_bytes: 64 << 20,
        ..Limits::default()
    }
}

fn instance(runtime: &Runtime) -> Instance {
    let program = runtime.load(&wat::parse_str(GUEST).unwrap()).unwrap();
    runtime.instantiate(&program).unwrap()
}

#[test]
fn a_function_gets_its_input_and_returns_its_output() {
    let runtime = Runtime::new(limits(), None).unwrap();
    let mut guest = instance(&runtime);
    assert_eq!(
        guest.functions(),
        ["boom", "count", "deep", "fail", "hog", "say", "spin", "upper"]
    );
    assert_eq!(guest.call("upper", b"hello, Octo").unwrap(), b"HELLO, OCTO");
    assert_eq!(guest.call("upper", b"").unwrap(), b"");
    assert_eq!(
        guest.call("nope", b""),
        Err(CallError::NoSuchFunction("nope".into()))
    );
}

#[test]
fn a_guest_error_comes_back_as_its_text_and_the_instance_goes_on() {
    let runtime = Runtime::new(limits(), None).unwrap();
    let mut guest = instance(&runtime);
    assert_eq!(
        guest.call("fail", b""),
        Err(CallError::Guest("nope".into()))
    );
    assert!(!guest.spent());
    assert_eq!(guest.call("upper", b"still here").unwrap(), b"STILL HERE");
}

#[test]
fn an_endless_loop_stops_at_its_deadline_and_spends_the_instance() {
    let runtime = Runtime::new(limits(), None).unwrap();
    let program = runtime.load(&wat::parse_str(GUEST).unwrap()).unwrap();
    let mut guest = runtime.instantiate(&program).unwrap();
    let started = Instant::now();
    assert_eq!(guest.call("spin", b""), Err(CallError::Deadline));
    let took = started.elapsed();
    // The deadline is 200 ms of wall-clock time, give or take a tick.
    assert!(
        took >= Duration::from_millis(190) && took < Duration::from_millis(400),
        "stopped after {took:?}"
    );
    // Its state may be half-updated: it takes no more calls, and a fresh
    // instance of the same program does.
    assert!(guest.spent());
    assert_eq!(guest.call("upper", b"x"), Err(CallError::Spent));
    let mut fresh = runtime.instantiate(&program).unwrap();
    assert_eq!(fresh.call("upper", b"still here").unwrap(), b"STILL HERE");
}

#[test]
fn memory_beyond_the_cap_ends_the_call() {
    let runtime = Runtime::new(limits(), None).unwrap();
    let mut guest = instance(&runtime);
    assert!(matches!(guest.call("hog", b""), Err(CallError::Trap(_))));
    assert!(guest.memory_bytes() <= 64 << 20);
}

#[test]
fn a_trap_is_an_error_not_a_crash() {
    let runtime = Runtime::new(limits(), None).unwrap();
    let mut guest = instance(&runtime);
    let error = guest.call("boom", b"").unwrap_err();
    assert!(
        matches!(&error, CallError::Trap(why) if why.contains("unreachable")),
        "{error:?}"
    );
    assert_eq!(guest.call("upper", b"x"), Err(CallError::Spent));
}

#[test]
fn runaway_recursion_traps_at_the_stack_cap() {
    let runtime = Runtime::new(limits(), None).unwrap();
    let error = instance(&runtime).call("deep", b"").unwrap_err();
    assert!(
        matches!(&error, CallError::Trap(why) if why.contains("stack")),
        "{error:?}"
    );
}

#[test]
fn log_is_the_only_import() {
    let runtime = Runtime::new(limits(), None).unwrap();
    let mut guest = instance(&runtime);
    guest.call("say", b"hi from the guest").unwrap();
    assert_eq!(guest.take_logs(), ["hi from the guest"]);
    let wasi = r#"(module
        (import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (func (export "octo_alloc") (param i32) (result i32) (i32.const 0))
        (func (export "octo_free") (param i32 i32)))"#;
    assert_eq!(
        runtime.load(&wat::parse_str(wasi).unwrap()).err(),
        Some(LoadError::Import("wasi_snapshot_preview1.fd_write".into()))
    );
    let no_alloc =
        r#"(module (memory (export "memory") 1) (func (export "octo_free") (param i32 i32)))"#;
    assert!(matches!(
        runtime.load(&wat::parse_str(no_alloc).unwrap()),
        Err(LoadError::Export(_))
    ));
}

#[test]
fn instances_keep_their_own_state() {
    let runtime = Runtime::new(limits(), None).unwrap();
    let (mut a, mut b) = (instance(&runtime), instance(&runtime));
    for _ in 0..3 {
        a.call("count", b"").unwrap();
    }
    assert_eq!(a.call("count", b"").unwrap(), b"4");
    assert_eq!(b.call("count", b"").unwrap(), b"1");
}

#[test]
fn an_instance_can_run_on_another_thread() {
    let runtime = Runtime::new(limits(), None).unwrap();
    let mut guest = instance(&runtime);
    let out = std::thread::spawn(move || guest.call("upper", b"worker").unwrap())
        .join()
        .unwrap();
    assert_eq!(out, b"WORKER");
}

#[test]
fn input_over_the_limit_is_refused() {
    let runtime = Runtime::new(
        Limits {
            io_bytes: 16,
            ..limits()
        },
        None,
    )
    .unwrap();
    assert_eq!(
        instance(&runtime).call("upper", &[b'a'; 17]),
        Err(CallError::TooLarge(17))
    );
}

#[test]
fn module_executes_after_runtime_restart_with_optional_cache() {
    let dir =
        std::env::temp_dir().join(format!("octosense-wasm-host-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let bytes = wat::parse_str(GUEST).unwrap();
    let first = Runtime::new(limits(), Some(dir.clone())).unwrap();
    assert!(!first.load(&bytes).unwrap().from_cache());
    drop(first);
    let second = Runtime::new(limits(), Some(dir.clone())).unwrap();
    let program = second.load(&bytes).unwrap();
    // A busy reader may intentionally compile instead. Strict artifact-hit
    // and deserialization coverage lives in cache::tests without I/O timing.
    assert_eq!(
        second
            .instantiate(&program)
            .unwrap()
            .call("upper", b"cached")
            .unwrap(),
        b"CACHED"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn table_initial_size_and_growth_have_an_element_cap() {
    let runtime = Runtime::new(
        Limits {
            table_elements: 8,
            ..limits()
        },
        None,
    )
    .unwrap();
    let guest = |initial: usize| {
        wat::parse_str(format!(
            r#"(module
        (memory (export "memory") 1) (table {initial} funcref)
        (func (export "octo_alloc") (param i32) (result i32) (i32.const 0))
        (func (export "octo_free") (param i32 i32))
        (func (export "grow") (param i32 i32) (result i64)
            (drop (table.grow (ref.null func) (i32.const 9))) (i64.const 1)))"#
        ))
        .unwrap()
    };
    let program = runtime.load(&guest(9)).unwrap();
    assert!(
        runtime.instantiate(&program).is_err(),
        "initial table must respect the cap"
    );
    let program = runtime.load(&guest(1)).unwrap();
    let error = runtime
        .instantiate(&program)
        .unwrap()
        .call("grow", b"")
        .unwrap_err();
    assert!(matches!(error, CallError::Trap(_)), "{error}");
}

#[test]
fn guarded_invocations_observe_cancellation_and_the_absolute_deadline() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let runtime = Runtime::new(
        Limits {
            deadline: Duration::from_secs(2),
            ..limits()
        },
        None,
    )
    .unwrap();
    let program = runtime.load(&wat::parse_str(GUEST).unwrap()).unwrap();
    assert!(runtime
        .instantiate_guarded(&program, Instant::now(), || true)
        .is_err());
    assert!(runtime
        .instantiate_guarded(&program, Instant::now() + Duration::from_secs(2), || false)
        .is_err());
    let pending = Arc::new(AtomicBool::new(true));
    let check = pending.clone();
    let mut instance = runtime
        .instantiate_guarded(
            &program,
            Instant::now() + Duration::from_secs(2),
            move || check.load(Ordering::Acquire),
        )
        .unwrap();
    let cancel = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(40));
        pending.store(false, Ordering::Release);
    });
    let started = Instant::now();
    assert_eq!(instance.call("spin", b""), Err(CallError::Deadline));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "cancellation interrupts active guest code"
    );
    cancel.join().unwrap();
    let mut instance = runtime
        .instantiate_guarded(&program, Instant::now() + Duration::from_millis(30), || {
            true
        })
        .unwrap();
    std::thread::sleep(Duration::from_millis(40));
    assert_eq!(
        instance.call("spin", b""),
        Err(CallError::Deadline),
        "call does not restart an expired budget"
    );
}

#[test]
fn concurrent_compilation_publishes_only_complete_cache_entries() {
    use std::sync::{Arc, Barrier};
    let dir =
        std::env::temp_dir().join(format!("octosense-wasm-cache-race-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let runtime = Arc::new(Runtime::new(limits(), Some(dir.clone())).unwrap());
    let bytes = Arc::new(wat::parse_str(GUEST).unwrap());
    let start = Arc::new(Barrier::new(4));
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let (runtime, bytes, start) = (runtime.clone(), bytes.clone(), start.clone());
            std::thread::spawn(move || {
                start.wait();
                let program = runtime.load(&bytes).unwrap();
                assert_eq!(
                    runtime
                        .instantiate(&program)
                        .unwrap()
                        .call("upper", b"parallel")
                        .unwrap(),
                    b"PARALLEL"
                );
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let program = runtime.load(&bytes).unwrap();
    assert_eq!(
        runtime
            .instantiate(&program)
            .unwrap()
            .call("upper", b"cached")
            .unwrap(),
        b"CACHED"
    );
    assert!(std::fs::read_dir(&dir).unwrap().all(|entry| entry
        .unwrap()
        .path()
        .extension()
        .unwrap()
        == "cwasm"));
    let _ = std::fs::remove_dir_all(dir);
}
