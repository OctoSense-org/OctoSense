# ADR 0011: An app's own functions, in WebAssembly

English | [简体中文](0011-apps-own-functions-in-webassembly.zh-CN.md)

Status: Proposed. Implemented behind the shell's `wasm-lab` feature (off by
default), with the contract side in App Hub #140 (app-contract 1.7.0).
Validated on macOS in a hidden desktop shell and on an Android
phone (Redmi Note 12) in a separately packaged Home; iOS and OpenHarmony not
tried.

## Context

A script app is Splash: contained, interpreted, and limited to what its
runtime offers. Some of an app's logic is better written in Rust: a parser, a
ranking or scheduling algorithm, a diff, a format conversion, or an existing
crate that does exactly the job. Today the only ways to run Rust for an app
are a native app (linked into the shell, fully trusted: not for third
parties) or a host service (the shell's own code, written by us). Splash's
compute kernels cover numeric work, not general Rust.

We want a developer to ship their own Rust functions inside their bundle, and
the shell to run them for that app only, with no new trust: nothing the
functions can reach that the app could not already reach, and nothing at all
outside their own input and output.

## Decision

1. **Modules.** A bundle may carry core WebAssembly modules as `fns/*.wasm`
   (at most 8). A module exports `memory`, `octo_alloc(len) -> ptr`,
   `octo_free(ptr, len)` and its functions, each `(ptr, len) -> i64` returning
   `(out_ptr << 32) | out_len` of an output that starts with a status byte (0
   ok, 1 error). Its only import may be `octo.log(ptr, len)`. A small guest
   crate (`apps/wasmlab/guest/octosense-guest`) writes all of this for
   `fn(&[u8]) -> Result<Vec<u8>, String>` and for serde types. No component
   model and no WASI in this version.
2. **Runtime.** `crates/wasm-host` runs modules in Wasmtime 49, compiled by
   Cranelift when they load. Compiled code is cached in the host directory,
   keyed by the module's digest and the engine's compatibility hash. Every
   call has a deadline (2 s, epoch interruption against wall-clock time);
   memory (256 MiB), table elements (16,384), the wasm stack (512 KiB),
   the module (8 MiB) and runtime input/output (16 MiB) have caps. A trap or
   a deadline ends the call with an error, never the process, and spends the instance: the next call gets a
   fresh one, because a Rust guest's stack pointer and allocator may be
   half-updated.
3. **The `wasm` host service** (shell feature `wasm-lab`). `wasm.<function>`
   calls one of the calling app's functions; `wasm.functions` lists them, how
   they loaded and how they have run. A string argument goes in as its text,
   anything else as JSON; JSON output comes back as data, anything else as
   `{"text": …}`. Modules come only from the calling app's own admitted,
   digest-checked bundle, never from an argument, and the service checks the
   admitted manifest's grant itself. Each invocation gets a fresh instance,
   including after successful calls and guest-returned errors: memory, globals,
   tables and logs cannot carry one caller's input into another call. Only
   compiled Programs are reused. The low-level runtime's reusable `Instance`
   API remains available for benchmarks; the app service never reuses it.

   The service admits at most four active workers, four queued requests per
   app, 1 MiB serialized input per request and 16 MiB total buffered input
   (including executing calls). Full queues return an error without blocking
   the UI. Workers release Programs and exit after five idle seconds. A
   request has a ten-second budget including queueing/loading; guest start
   and function execution share at most two seconds, with cancellation
   checked at each epoch. Compilation itself is not interruptible, but an
   expired or cancelled request cannot proceed from compilation to execution.
   Admission is checked before queueing, before execution and before delivery;
   changed bundles/grants or signed withdrawal discard the result and cached
   Program references. On a valid new revision the worker loads the new code.
   The disk cache contains compiled code only, never guest state.
4. **The `wasm` capability** joins App Hub's closed capability list. A store
   says: "Runs its own functions in a sandbox on this device; they reach no
   files, network or other apps."
5. **Agent tools.** A tool may name `host_method: "wasm.<function>"`: the
   app's agent (and, if shared, other callers) runs the app's own function.
   It needs the `wasm` capability, and unlike shared host methods it has no
   risk floor and no `private_data` requirement: the function sees only the
   arguments it is given.

## Alternatives measured

CoreMark on the M5 Max (higher is faster; native Rust is about 45,000):

| Runtime | CoreMark | Notes |
| --- | --- | --- |
| Wasmtime + Cranelift (chosen) | 44,534 | 7.4 MiB in the binary; compiles when a module loads |
| WasmEdge AOT | 30,432 | 153 MiB library; 0.2–0.36 s to compile |
| stitch (interpreter) | 2,942 | makepad-friendly, no JIT; needs a host-call fix (makepad #106) |
| Pulley (Wasmtime's interpreter) | 2,587 | no JIT: the iOS option |
| WasmEdge interpreter | 374 | |

- **Native dynamic libraries** give no isolation; Theseus OS's model (a
  trusted compiler, one address space) does not apply to a phone OS.
- **Upstream Makepad's JIT** compiles Splash, not Rust.
- **Splash compute kernels** stay the answer for numeric kernels.

## Measurements (Wasm Lab, `apps/wasmlab`)

These are the original runtime measurements, before the service switched to
fresh instances for every call. They are not a new end-to-end service benchmark.

Light algorithms only: Markdown to HTML (pulldown-cmark), free slots around
busy times, fuzzy ranking (strsim), a line diff (similar).

Medians of 200 calls, release builds, three runs on the desktop and three on
the phone (`cargo run --release -p octosense-wasm-host --example measure`;
on the phone the same example, cross-compiled and run over adb):

| | Desktop (M5 Max) | Phone (Redmi Note 12, Snapdragon 685) |
| --- | --- | --- |
| Module | 433 KiB | 433 KiB |
| Compile (first load) | 27–33 ms | 378–421 ms |
| Load from the cache | 1.0–1.4 ms in a new process; 4.8–6.4 ms right after writing | 5.0–7.0 ms |
| Instantiate | 0.07 ms | 0.19–0.27 ms |
| md_to_html, 32 KiB | 405–412 µs (2.1–2.2× native) | 2,325–2,328 µs (1.7×) |
| find_slots, 30 busy intervals | 6.7 µs (1.7×) | 40 µs (1.5×) |
| fuzzy_rank, 500 items | 138–146 µs (1.6–1.7×) | 841–845 µs (1.4×) |
| text_diff, 1000 lines | 231–254 µs (1.6–1.7×) | 1,434 µs (1.9×) |
| Smallest call (JSON both ways) | 0.3 µs | 2.5–2.6 µs |

Building the guest with wasm SIMD (`+simd128`) did not narrow the Markdown
gap (2.2–2.3×): it is Cranelift's code generation against LLVM's, not
vectorisation. Wasmtime and Cranelift add about 7.5 MiB of code to the
Android library (symbol sizes; 7.4 MiB measured on the desktop).

In the shell, a script's `host.request` to its function and back takes about
2 ms on the desktop and 1.8–2 ms on the phone, almost all of it the UI event
loop; the function itself takes 18–72 µs on the desktop and 47–190 µs on the
phone. In Home on the phone the module compiled in 670 ms on the first launch
(the app was still starting) and loaded from the cache in 7.4–11 ms after.
The four misbehaving modes (an endless loop, endless allocation, a panic,
runaway recursion) each ended in an error after 2,000 ms, 140 ms, 2.2 ms and
3 ms, and Home kept running.

Two platform findings:

- **Android traps needed libc 0.2.190.** With libc 0.2.189 every trap that
  Wasmtime catches by signal (a panic's `unreachable`, an out-of-bounds load,
  a stack overflow) killed Home, and a plain binary too. Wasmtime's handler
  ran but read the faulting pc through libc's `ucontext_t`, whose aarch64
  Android layout lacked the kernel's padding after the signal mask, so no
  trap looked like wasm. 0.2.190 fixed the layout; `crates/wasm-host` now
  requires it. Explicit traps (`signals_based_traps(false)`) also worked
  but cost 19–38% per call and doubled the compile time.
- **A desktop scanner can make the cache slower than compiling.** On a
  managed Mac, Microsoft Defender holds the first open of a freshly written
  cache file for about a second, and again the first time a new build of the
  shell opens it (1,086 ms; 2.3 ms on the next launch). The writer reads the
  file back in the background, which covers the first case only. Compiling
  takes 30–60 ms there, so a desktop could skip the cache or race the two;
  on the phone the cache saves about 0.4 s.

## Consequences

- Third-party Rust runs in OctoSense apps at 1.4–2.2× native speed for these
  workloads, with no new trust and no change to the default build.
- The Android APK grows by 3.5 MiB (209.1 to 212.8 MB, with Wasm Lab's own
  bundle); the runtime is about 7.5 MiB of code.
- Compiling costs about 0.4 s per module on a mid-range phone (0.67 s while
  Home is starting). The cache makes later loads 5–11 ms; compiling at
  install time (store apps) or at build time (system apps) would remove the
  first one.
- A store bundle may carry `fns/*.wasm` under App Hub #140's gate (a core
  module, at most 8, only with the `wasm` capability). So far only a system
  app has run functions on a device.
- iOS allows no JIT for apps and no downloaded native code: there, Pulley
  (about 17× slower than Cranelift). A shipped system app's functions could
  be translated to native code when the app is built; a store app's could
  not. OpenHarmony's JIT policy is unknown (**unverified**).
- Guest memory is no longer retained across calls or across the eight modules:
  each worker holds at most one live instance, and only four workers may run.
  Continuous admitted requests can still keep those workers busy; CPU fairness
  across applications and a bounded disk-cache eviction policy remain follow-up
  work. Compiled Programs also consume host memory beyond the linear-memory cap.
- Runtime and shell service regression tests cover actual Wasm state retention,
  table growth, cancellation, input/queue bounds, revision/grant changes and
  signed withdrawal. This hardening has not been revalidated on a phone yet.

## Open questions

- Typed interfaces (the component model and WIT) instead of JSON in and out.
- Any host import beyond `octo.log` (a clock, randomness) would be a new
  capability, not a default.
- Epoch deadlines are cheap but not deterministic; fuel would be, at a cost.
