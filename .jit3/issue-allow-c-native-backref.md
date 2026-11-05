# makepad run --allow-c: a native library whose C code calls back into Rust (`blake3_compress_in_place_portable`) cannot be dlopen'ed, and the run dies in libc's first call instead of saying so

## What happens
A program whose `--allow-c` set contains blake3 1.8.7 (NEON: `static=blake3_neon`) gets a `libnative.dylib` that links fine (`-undefined dynamic_lookup`) but cannot be loaded:

```
dlopen(…/native/<key>/libnative.dylib, RTLD_NOW|RTLD_GLOBAL):
  symbol not found in flat namespace '_blake3_compress_in_place_portable'
```

`blake3_compress_in_place_portable` is a `#[no_mangle] extern "C"` fn of blake3's Rust side that its C NEON code calls; under rustc the linker finds it in the same binary, under the JIT nothing exports it when dyld binds the library. `run.rs link_libraries` only notes the failure (`--verbose`: "libraries not found: [… (--allow-c native library of aws-lc-rs, aws-lc-sys, blake3, libc, …)]") and the program starts anyway. Since libc's externs are also routed through that library, the first libc call traps:

```
makepad-run: fatal fault in program code: the process ends (signal 4) … illegal instruction at pc in JIT core::hash::random::next_keys +0x8c
  frame 0 … core::collections::hash_map::HashMap::new   frame 1 … makepad_hal::backend::apple::shell::Shared::new
```

which points nowhere near the cause. Three runs, same fault; the stock calculator (no `--allow-c`) runs fine from the same tree (work 60abec8ba + #1383).

## Suggestions
1. When `load_path` fails, print `dlerror()` and stop before the program starts (a missing native library is fatal for every extern it carries; the trap in `next_keys` cost an evening).
2. For the back-reference itself: link a stub object into libnative that defines every symbol the archives import and no library provides (`nm -u` minus libSystem) as a trampoline the JIT patches once the `#[no_mangle]` fn is compiled; or document that crates with C→Rust back-references need their pure feature (`blake3 = { features = ["pure"] }` is our workaround).

## Environment
macOS 15 arm64, `relax run --allow-c octosense …` and `makepad run` alike; the library's full undefined list is 89 symbols, 88 of them libSystem's, one blake3's.
