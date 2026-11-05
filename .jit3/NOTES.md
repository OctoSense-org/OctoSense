# jit3 spike: OctoSense desktop shell against upstream makepad work (60abec8ba)

## Resolution fixes (cargo metadata)
- makepad-widgets feature `appcard` is fork-only: dropped from root + crates/shell
- fork-only makepad-widgets features removed: phone/Cargo.toml: dropped ['appcard']
- fork-only makepad-widgets features removed: apps/appcard/module/Cargo.toml: dropped ['appcard']
- spike scope: members dropped (phone, appcard, smoke tests, process-apps): ['apps/appcard/app/app', 'apps/appcard/app/crates/octos-app-render', 'apps/appcard/app/crates/octos-app-store', 'apps/appcard/app/crates/octos-app-transport', 'apps/appcard/module', 'crates/browser-smoke', 'crates/host-api-smoke', 'crates/process-apps', 'crates/video-api-smoke', 'crates/wasm-phone-smoke', 'phone']
- shell optional dep removed for the spike (third-party makepad consumer or dropped member): rinx
- shell optional dep removed for the spike (third-party makepad consumer or dropped member): octoscode-module
- shell optional dep removed for the spike (third-party makepad consumer or dropped member): octosense-appcard
- shell features app-rinx / app-octoscode / app-appcard emptied (Rinx, OctosCode and AppCard are separate port items)
- markdown-editor (GitHub Notes editor on Rinx article-makepad, needs fork widgets feature `serde`) dropped from the spike

## check1 (default features, --allow-c): exit 1 in 97 s, two build-script walls
- makepad check: /Users/ychen/.cargo/git/checkouts/octosense-app-hub-3222614192ecfbf8/3863591/crates/app-hub-app/build.rs: build script failed (exit 1): makepad-run: `blake3` compiles C (blake3: build-dependency cc): it is built only with `--allow-c` (or MAKEPAD_RUN_ALLOW_C=1): cargo and the system C compiler build it, and its library is linked into the run (its crate is checked without the script's cfgs)
- makepad check: /Users/ychen/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/cranelift-codegen-0.136.2/build.rs: build script failed (exit 101): compile error in cranelift_isle::trie_again::<RuleSetBuilder as sema::PatternVisitor>::add_match_bool: /Users/ychen/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/cranelift-isle-0.136.2/src/trie_again.rs:636:9: assert_eq of `Vec<BindingId>` and `&[?0; 0]` (its crate is checked without the script's cfgs)

## check2 (no wasm/engines; --allow-c + MAKEPAD_RUN_ALLOW_C=1): exit 1 in 18 s, 1 error
- relax PARSE gap in a dependency: tls_codec 0.4.2 tls_vec.rs:897 "expected `>` (found Plus `+`)". Cause: `impl_tls_vec_generic!`
  expands `$name<T: $($bounds + )*>` → `T: Zeroize + >` (trailing `+` in a bound list, legal Rust). Repro: .jit3/repro/trailing_plus.rs.
  Chain: octosense → shell → ai-host → octosense-appstore → octosense-app-hub → sigstore-verify → tls_codec. Not optional for the shell.
- Lesson: one parse gap anywhere in the 1,690-package graph stops the whole check; the shell's own code has not been reached yet.
- BISECTED (relax dev runner, `relax check tls_codec` in a scratch copy): the arm is `impl_tls_vec_generic!(…, Zeroize)` and inside it
  the FIRST chunk, `#[derive(Eq, Debug)] pub struct $name<T: $($bounds + )*> {…}`. Plain `struct S<T: Clone + >` and the same macro
  WITHOUT a derive both pass; with `#[derive(...)]` relax's derive expansion re-parses the generics and fails on the trailing `+`.
  10-line repro: .jit3/repro/trailing_plus_derive.rs (relax: "expected `>` (found Plus `+`)"; rustc: ok). Issue draft: .jit3/issue-derive-trailing-plus.md
- Sweep of the 88 direct third-party deps of the workspace crates through `relax check` (each in its own registry dir): .jit3/sweep-results.tsv

## rustc layer 1 (cargo check --keep-going, 13 errors in 3 crates) and the spike stubs applied
- octoscript-render/eval.rs: ScriptVm.host = host.host_context(); eval_checked → local helper without the instruction limit (1 call)
- octoscript-widgets: Widget::selected_value/checked (fork trait methods) renamed to inherent jit3_*; View.selected (fork field) writes dropped, reads None; TouchState::Cancel arm added
- App Hub app-policy copied to .jit3/app-hub; splash_adapter's set_policy/set_memory_bytes (fork-only SplashRef API) dropped

## rustc layer 2 (cargo-check4.log): App Hub appstore, 8 errors, ALL fork-only Splash security/host API
camera_preview::release_isolate_devices, splash_policy::may_run, splash_host::take_splash_host_requests_for, splash::register_agent_module,
Event::TextInputStateQuery, ScriptHeap::object_data, ScriptVm::eval_checked, SplashRef::set_device_consent. Upstream has NO splash_host /
splash_policy / splash_storage / widget_async modules. Reference counts (OctoSense crates): splash_host 20, splash_policy 25, policy
setters 18, register_agent_module 6, TextInputStateQuery 3, eval_checked 3 (+3 in Octoscript-Makepad). STOPPED PEELING HERE: the next
stubs would be the shell's own security model (inventory L items "prompt provenance + device consent + per-app policy", "isolate host
extensions", "Splash sys.* engine/fetch") — that is the port, not a spike.

## relax sweep of the 88 direct third-party deps (sweep-results.tsv), partial at 41: 29 OK, 8 TYPE (soft), 4 LOAD (hard)
LOAD = parse gaps: ndarray 0.17.2 slice.rs:247 (effectcraft x3), time 0.3.55 parsed.rs:133 "expected `>` (found Minus `-`)". Plus
tls_codec 0.4.2 (derive + trailing `+`). TYPE (only matter if reached): flate2 22, image 4, base64 2, chrono 2, deckcraft/filmcraft 2 each.

## relax sweep FINAL (88 direct third-party deps, each `relax check` in its own registry dir): 54 OK, 20 TYPE, 11 LOAD, 3 cargo-native fail
Distinct hard walls (LOAD): ndarray 0.17.2 slice.rs:247 parse; time 0.3.55 parsed.rs:133 parse "expected `>` (found Minus)"; serde
1.0.197/1.0.219 de/mod.rs:495 parse "expected OpenBrace (found Plus)" (serde 1.0.228 loads: version-specific); tls_codec 0.4.2 derive +
trailing `+`; cranelift-isle 0.136.2 inference `assert_eq!(Vec<_>, &[])` (wasmtime); zip → relax PANIC types/src/mir/build/place.rs:106.
RC1 = the --allow-c cargo pass failed on thiserror build script / foldhash / hashbrown (vectorcraft-engine, wasmtime-wasi, -http).
Biggest soft counts: sha2 112, flate2 22, photocraft-automation 12, lightcraft-engine 6, image 4, serde_json 4.
- upstream issue OPENED: makepad/makepad#1382 (derive + trailing `+`), 4 Nov 2026

## "patch and proceed" (user, 4 Nov): relax derive fix + the program now loads end to end
- Fix: libs/relax/front/src/derive.rs derive_generics drops a trailing `+` from each param's copied bounds; test
  compiler/tests/derive_trailing_plus_bound.rs. Branch `relax-derive-trailing-plus` in jit3-work (commit aa3b48089), not pushed.
  Verified: repro passes, tls_codec standalone 0 errors, relax-front suite 79/79. relax-base 1 failure (zone_alloc domain test, deterministic)
  and relax-compiler 1 failure (domain teardown test) are in crates the fix does not touch; pristine-tree run in jit3-pristine to confirm.
- check3 (makepad check, sandboxed policy): 634 errors in 69 s, the WHOLE graph loads now. 166 are relax's proc-macro SANDBOX refusing
  serde_derive (ctxt.rs:4 `use std::thread`) and proc-macro2, which cascade into 159 derive(Serialize/Deserialize) errors in our crates:
  under `makepad run` any registry proc macro naming fs/net/process/env/thread/time is refused; only the bare `relax` runner (Policy::Open)
  trusts them. Upstream's own apps use no serde derive. → upstream question: how is serde_derive meant to run under `makepad run`?
- Ours after removing sandbox noise ≈ 300: shell 235 (incl. sandbox), makepad-d3 135, ai-providers/config 30, l0-chat 20, kernel 17,
  ai-host 15, octoscript-ui-l0 11, app-peers 5. Top missing fork APIs: android_integration, accessibility_preferences,
  with_script_vm_id_trusted, thread::Lane, splash_host, ai_services peer/ui_port links, NativeQr events, ModalBounds, CloseDecision,
  set_policy. Also relax macro gaps: 15 "macro not supported", 14 concat!/compile_error!, 5 tokio select Mask.
- check4 = same under bare `relax` (Open policy) → the clean fork-API number.
- check4 RESULT (bare `relax`, Open policy, 44 s): 182 errors, ALL in our code (shell 167, ai-host 15); no third-party error at all.
  29 are relax macro gaps (15× `json!` unsupported, 14× `include_str!` inside concat!/compile_error!). 153 are fork-only APIs in 26 files
  (shell lib.rs 26, module_host.rs 26, studio/mod.rs 15, android_integration.rs 12, ai-host module_peer.rs 9, glance_card.rs 7, qr.rs 6…):
  android_integration, accessibility_preferences, with_script_vm_id_trusted, thread::Lane, CloseDecision, splash_host, WindowRequest,
  ai_services peer/ui_port links, set_policy, TextInputStateQuery, ModuleCloseAction/ModuleWindows, ModalBounds, Readback*, terminal::ai.
  This is the shell's first-layer port list (resolution errors hide the type errors behind them; expect 2–3× once resolved).

## PORT LOOP (started 4 Nov, user "push and start the 153"); PR #1383 opened for the relax fix
Recipe: fork commits cherry-pick directly into jit3-work (shared object store; branch `jit3-port-makepad` = upstream work + relax fix);
files moved upstream (widgets/src → widgets/core/src) are re-applied with a path-rewritten `git apply --3way`; check with
`cargo check -p <crates>` + crate tests; then `relax check --json --allow-c octosense --no-default-features --features octos-core,app-*`
from jit3-octosense (bare runner, ~35–65 s) and `python3 .jit3/worklist.py` (edit the log name inside).
Progress: 182 → 157 (AI1: 08118a09d af0d0fc26 b9cbb6c17 45d677e67, resolved against upstream's settings-card engine; 99 tests)
→ 139 (W4: cc3e9a618 1e6bf7cea; area.rs part dropped as superseded; modal on widgets/core).
Remaining map: close veto 2370dd25b (module-host 9); PendingServiceLinks ← 74fec2160 (aichat overlay, big; port only the type);
Readback* ← b174762ef (Studio code atlas, big); Lane ← 99a7ad641?; NativeQr ← a68fd2063; with_script_vm_id_trusted ← ed8452f12;
take_splash_host_requests_for ← ddfb25b96. NO fork commit (OctoSense runtime patches): WindowRequest, AccessibilityPreferences,
TextInputStateQuery, screen_origin. relax gaps to file upstream: json! (15), include_str! in concat!/compile_error! (14).
→ 130 (close veto 2370dd25b, clean pick). Runtime-patch trial (.jit3/patch-trial.tsv): of OctoSense's 41 makepad patches only 3 apply to
upstream work (android-enter-composition, android-video-seek, finance-seed); 38 fail, nearly all because their files moved/changed
(platform/src/os → platform/hal, widgets/src → widgets/core). The settings patch (18 files, 123 hunks: AccessibilityPreferences,
TextInputStateQuery, scroll_pos, screen_origin, WindowRequest?) is a hand re-port.

## PORT LOOP, round 2 (4 Nov, user "next"): 130 → 66 → 29 → 15, every fork-API family gone
Makepad side (jit3-work `jit3-port-makepad`, now upstream/work + 13 commits): c82d7c318 splash-host drain (ddfb25b96, path-rewritten);
b13d30484 terminal TERMINAL_MODULE (53d66972e; crate renamed makepad_app_terminal, `caller` in its test); 371a2dd44 Cx::show_qr_scanner +
Cx::android_integration + NativeQr events + Event::AndroidIntegration (id 96) (hand port of 81fd7371c, desktop half); ebfe04864 the
runtime-patch surface: accessibility.rs + Cx::accessibility_preferences, Event::TextInputStateQuery (98) answered by TextInput,
View/ViewRef::scroll_pos, input_provenance (trusted_user_input, always false here: no HAL evidence hooks), Event::HomeIntent (97),
home::platform_data_dir; 08e26b83a Cx::handle_script_tasks pub.
OctoSense side (jit3-port 50897a0c, 9470e688, 1b916eb0): upstream differences the SHELL follows: normalised readbacks (packed RGBA
top-first), task Class instead of Lane (Heavy→Background, Light→Visible, no reserve), no isolate time budget (trusted = plain entry),
ServiceRegistry::process() instead of PendingServiceLinks, ServiceCall.caller, MpTerm module root (TermTabs needs the fork's settings
chain 3d6637552→b5a945e29, not ported), WindowGeomChange.screen_origin, Swapchain Option, MacosMenu dock, RemoteScroll phase/is_mouse,
Rect dpi_snap helper, StyleSheet::load(style.sheet(dark)) + family/resources, BlackOrange arms, TouchState::Cancel, point_to_index shift.
PORT GAPS (marked in code): Splash set_policy/set_memory_bytes/set_device_consent (isolate memory cap LOST), splash_policy::may_run,
splash::register_agent_module (agent.notify in isolates), camera_preview release, WebReader (auth webview), Video show_controls (fork
video-controls patch), TermTabs, provenance evidence hooks (macOS/Android), Android HAL halves of QR/AndroidIntegration/HomeIntent.
rustc: `cargo test -p octosense-shell --features octos-core,app-terminal --no-run` builds (App Hub crates all [patch]ed to .jit3/app-hub);
touched modules: 145 passed, 1 ignored, 4 failed = upstream behaviour (VM number rounding 14.5/14.6 MB, 990 m/1 km; Video
show_controls). relax: check9 = 29 (json! 15 + include_str 14); with OCTOSENSE_WORKSPACE exported check10 = 15 (relax ignores cargo
[env] → .jit3/issue-cargo-env-config.md). json! ROOT CAUSE found with .jit3/repro/json_glob_shadow: a glob `pub use crate::serde_json::*`
over a PRIVATE `mod serde_json` (makepad_micro_serde) makes relax resolve `serde_json::json!` to that module via `use makepad_widgets::*`;
`::serde_json::json!` works → .jit3/issue-json-glob-shadow.md (not opened yet). Workaround if needed: `::serde_json::json!` at the 15 sites.
Next: open the two issues (user's call), then `makepad run octosense` (bare relax run) for the first real JIT launch; then the gaps above.

## FIRST RUN ATTEMPTS (4 Nov evening, user "open octosense"): relax check clean (bc7d3a6c, `::serde_json::json!` around #1384), runs die at startup
Launch: `.d1-scratch/jit3-run.sh <log> [ENV=..]` (herdr tab via macos-look/tab.sh; scratch OCTOSENSE_HOME .d1-scratch/jit3-home; logs .d1-scratch/jit3-run*.log).
Every run (1-7) dies identically before the first frame: `fatal fault in program code (signal 4) … illegal instruction at pc in JIT core::hash::random::next_keys +0x8c`
= relax std shim random.rs:29:20, the `match KEYS.get()` on the `#[thread_local] static KEYS: Cell<Option<(u64,u64)>>` → an unreachable arm (udf), i.e. the
thread-local read back a bad discriminant. Frames: HashMap::new ← makepad_hal::backend::apple::shell::Shared::new ← OnceLock ← expect_main_loop ← run_app.
Ruled out along the way (each a real finding):
- run1-3: `libraries not found: [… libnative.dylib]` = dlopen RTLD_NOW failed: `symbol not found in flat namespace '_blake3_compress_in_place_portable'`
  (blake3's C NEON code calls back into its Rust side; the JIT exports nothing to dyld) → .jit3/issue-allow-c-native-backref.md. Workaround IN TREE:
  desktop/Cargo.toml `blake3 = { features = ["pure"] }` (blake3 still needs --allow-c: its `cc` build-dependency).
- the native key changed every run: cargo rewrites Cargo.lock on each `--allow-c` build (only the ORDER of [[patch.unused]] blocks) and native.rs keys on
  the lock bytes → full C rebuild per launch → .jit3/issue-allow-c-lock-order.md. Workaround IN TREE: removed every unused [patch] entry (code_editor ×2,
  makepad-ai/markdown/markdown-widgets/task shims, ruma section, app-contract path) → lock stable (md5 66b718…), warm boots.
- calculator runs fine under both runners, with and without --allow-c (hidden window reached first present) → not the runner build, not --allow-c as such.
- OctoSense without --allow-c is refused (blake3's cc build-dep). MAKEPAD_RUN_OPT=-statictls changes nothing (static TLS is Linux x86-64 only).
- MAKEPAD_RUN_TLS_TRACE=1: KEYS is "block at 0x50"; the trace also lists proc_macro::CALL_SITE (0x20, copied on first access), IN_MACRO, std CURRENT,
  panic DEPTH/PAYLOAD → the proc-macro WORLD (pmhost.rs: its own Program/Tcx/JIT Unit, in process, for the life of the run; started early beside the
  native build = `proc-macro world started beside the native build`, absent in the calculator) runs on the same main thread. Suspect: two units
  placing thread-locals in one per-thread block (each Unit starts at TLS_FIRST_FREE) — not proven: the trace shows one sequence of offsets.
- run8: MAKEPAD_RUN_OPT with EVERY backend -token off → identical fault at the identical offset: not an optimisation pass; runtime/TLS.
STOPPED HERE (user's call). Upstream issues opened 4 Nov: #1384 json! glob shadow, #1385 cargo [env], #1386 the startup trap
(next_keys/KEYS, world + program units), #1387 native library back-reference (blake3), #1388 lock rewrite per run. In-tree
workarounds kept: pure blake3, unused [patch] entries removed, `::serde_json::json!`. Next when upstream answers #1386: rerun
`.d1-scratch/jit3-run.sh jit3-run9`.

## FIRST LIGHT (5 Nov 2026 morning, user "keep pushing without them"): OctoSense RUNS under the JIT
- The trap reproduced with the smallest app + one serde derive, no --allow-c (.d1-scratch/jit3-mini, `relax run jit3-mini --features serde`);
  with blake3 + --allow-c (early world) it did not → posted on #1386. Cause: each jit::Unit allocated `#[thread_local]`s from TLS_FIRST_FREE
  of the one per-thread block; the proc-macro world (own Unit) and the program aliased. FIX in jit3-work 3538cf789 (`Unit::tls_alloc` bumps
  one process-wide AtomicU32); relax rebuilt (`cargo build --release -p relax-runner --bin relax`, 51 s). Mini app: two KEYS placements, runs.
- run10 (patched runner): no trap, exited at main+3 s after `[makepad-remote] MAKEPAD_REMOTE=47640: a port or address is not supported any more`
  → jit3-run.sh no longer sets MAKEPAD_REMOTE (relax opens its own Unix socket: `makepad inspect <pid>`).
- run11: UP. Modules linked (terminal, calculator, clock, notes, reminders, weather), ai-host up (no packaged octos kernel → stub), glance etc.
  Shader flood: `sample_as_bgra` unknown upstream (fork: identical to `sample` except WebGL) → 26 sites in crates/shell → `sample(` (e3e5929d);
  the LIVE session patched the running app in 70 ms ("makepad live: icon_frame.rs, ui.rs: patched"), glass shader compiles, flood gone.
  Left: (a) once: lazy module `isolate ui`: `property ime not found in prototype chain` (widgets/core lib.rs widgets_header `..mod.ime` in an
  isolate); (b) one contained panic on makepad-app: `GenVec<ScriptObjectData> value from heap 3 used in heap 1` (a script value crossing
  isolates; flightrec /Users/ychen/.makepad-run/flightrec/auto-octosense-51076-20261105-184314-panic.mkpf; `makepad inspect 51076 panics`);
  (c) relax background typeck panics `anonymous const type: NoSolution` (typeck.rs:850, thread gandalf; harmless); (d) prebuild.rs runs cargo
  for 10 catalog apps in the background under the JIT (exit 101) — disable in the spike.
- 5 Nov midday: process apps = the first-open cargo release build (minutes) + a PROTOCOL MISMATCH: launches built from `.sources/makepad`
  (the FORK checkout; no `MPB1` studio batch header) while the JIT shell speaks upstream's → `.sources/makepad` is now a symlink to
  jit3-work (fork worktree kept as `.sources/makepad.fork`), desktop/config/apps*.json makepad rows renamed to upstream's
  `makepad-app-<id>` package + bin; route/image prebuilt (`cargo build --release -p makepad-app-route -p makepad-app-image`, 45 s).
- `makepad run` (tools/run, target/host/makepad): added MAKEPAD_RUN_TRUST_MACROS=1 (jit3-work 2b346b873: Policy::Open with a notice) because
  the sandbox refuses serde_derive. OctoSense now runs under it: `JIT3_RUNNER=<jit3-work>/target/host/makepad jit3-run.sh <log>
  MAKEPAD_RUN_TRUST_MACROS=1` → first present main+5.3 s cold, app check 0 errors, modules linked (run13).
