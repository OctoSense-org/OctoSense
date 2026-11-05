# relax: a large program with an early proc-macro world dies at startup with an illegal instruction in `core::hash::random::next_keys` (the `match` on its `#[thread_local]` KEYS)

## What happens
`relax run --allow-c octosense …` (our desktop shell, 382 crates, serde_derive and friends expanded by the in-process proc-macro world; type-check clean) dies the same way on eight consecutive launches, cold and warm, before the first frame:

```
makepad-run: proc-macro world started beside the native build
…
makepad-run: fatal fault in program code: the process ends (signal 4) … illegal instruction at pc in JIT core::hash::random::next_keys +0x8c (slot 181, baseline fresh)
  frame 0 … core::collections::hash_map::HashMap::new +0x7f      (std/core/hash/fast.rs:134)
  frame 1 … makepad_hal::backend::apple::shell::Shared::new +0x2f (platform/hal/src/backend/apple/shell.rs:233)
  frame 2 … Once::call / OnceLock::initialize / expect_main_loop / makepad_platform::app_main::run_app
makepad: illegal instruction … at core::hash::random::next_keys (libs/relax/compiler/std/core/hash/random.rs:29:20)
```

random.rs:29 is `let (k0, k1) = match KEYS.get() {` on `#[thread_local] static KEYS: Cell<Option<(u64, u64)>>`: the first HashMap of the program reads back a discriminant that is neither None nor Some, and the match's unreachable arm (`udf`) is what traps. Nothing is printed before the fault (no panic, no missing-foreign report).

## What it is not
- Not the runner build: the stock calculator runs from the same tree under both `target/release/relax` and `target/host/makepad`, with and without `--allow-c`.
- Not `--allow-c`'s native library: once it loads (see the two issues on that), the fault is unchanged; without `--allow-c` the program is refused (blake3's `cc` build-dependency), so that cannot be compared.
- Not an optimisation pass: `MAKEPAD_RUN_OPT=<every -token the backend knows>` gives the identical fault at the identical offset.
- `MAKEPAD_RUN_OPT=-statictls` changes nothing (static TLS is Linux x86-64 only; macOS places everything in the block).

## What is different from the calculator
This program needs the proc-macro world, and `--allow-c` starts it early ("proc-macro world started beside the native build"). `MAKEPAD_RUN_TLS_TRACE=1` prints one sequence of placements in the main thread's block:

```
thread_local proc_macro::CALL_SITE::…: 28 bytes align 4, block at 0x20, copied on first access
thread_local proc_macro::IN_MACRO::…: 1 bytes align 1, block at 0x3d
thread_local std::thread::CURRENT::…: 8 bytes align 8, block at 0x40
thread_local std::panic::DEPTH::…: 4 bytes align 4, block at 0x48
thread_local core::hash::random::next_keys::KEYS: 24 bytes align 8, block at 0x50
thread_local std::panic::PAYLOAD::…: 16 bytes align 8, block at 0x68
```

The world is its own `jit::Unit` (pmhost.rs:595) and `Unit::tls_next` starts at `TLS_FIRST_FREE` per unit, while the thread-local block is one per thread for the process (one OS key). I cannot tell from outside whether the program's `KEYS` and a world variable overlap in the main thread's block, or whether `KEYS` was placed by the world's unit (the sequence above is continuous) and read by the program unit under a different scheme; either way the value at 0x50 is not what `next_keys` wrote. The calculator, which has no world, is fine.

## Repro
Our tree is not small (OctoSense-org/OctoSense branch `jit3-port`, resolved against work 60abec8ba + #1383, run script in the notes), but the shape is: any makepad app whose graph needs the proc-macro world AND `--allow-c`, started cold or warm. Happy to run any diagnostic you want on it (MAKEPAD_RUN_TLS_TRACE output above; `makepad inspect` is not reachable since the process dies before the first event).

macOS 15 arm64 (Apple silicon), Rust stable, relax built from work 60abec8ba.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
