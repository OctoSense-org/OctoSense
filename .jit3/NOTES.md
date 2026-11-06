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

## STEPS 1-5 (5 Nov afternoon, user "do 1-5")
1. Fork `jit` = OctoSense-org/makepad jit (aee111593: upstream work 60abec8ba + 15 port commits + compat/ crates + libs/wm_theme);
   upstream PRs #1391 (TLS allocator, fixes #1386), #1392 (MAKEPAD_RUN_TRUST_MACROS) + #1383 earlier.
2. OctoSense `jit` branch (e54315fd): pins → fork jit, Octoscript-Makepad jit 468becf1, App Hub jit eb4fbe15; [patch] → .sources/makepad
   (upstream layout + compat/); no runtime patches; Rinx/AppCard/OctosCode/Task + markdown editor off the line (native-apps.json);
   setup.py --check --cargo + native_apps.py --check pass; relax/makepad check 0 errors; shell tests build, touched tests pass.
   PROVIDED-CRATE TRAP: the runner must be built FROM the checkout's own `.sources/makepad` (`cargo build --profile host -p makepad-run
   --bin makepad` there). A runner built in another worktree of the same commit (jit3-work) binds the program's makepad_script by name
   as a provided crate while also compiling it → "a field of ScriptTypeObject is reached from JIT-compiled code … lives in a provided
   crate" in traits::register_type_inner → no frame in 180 s (run14; report ~/.makepad/devhub/captures/20261105T210556Z-*). Hypothesis
   under test (run15 with .sources/makepad/target/host/makepad).
3. devhub master runs under the JIT (`makepad run makepad-app-devhub` from the tree; menu-bar app; control ~/.makepad/devhub/devhub.sock);
   it says "no fleet identity: ~/.makepad/fleet/id.key … this machine's JITs only" → tools/fleet-identity.md (fleet-id tool, root key on
   the admin Mac, node credentials for the minis) + the private-LAN rule = the overlay question. `makepad devhub ls` hung (rc 142) with no
   JIT up; `makepad inspect <pid> status` works against the running shell. autosync wants a branch named `work`.
4. Security gaps, port branch jit-port-policy (.worktrees/jit3-port2, on top of aee111593): splash policy layer = fork 03a0362fe picked
   (path-rewritten) + dd1961a69 (granted isolate mods, register_agent_module → SplashAction::Notify defined here), 541216e62 (vm/res
   hunks only), d7a834c74, b0f98fb63, 52160dec0; splash_policy.rs/gate.rs = fork d653bc44 final; gps.rs ported (no data-fetch epoch);
   device consent = makepad-device-consent.patch minus camera_preview. Tests: widgets-core splash/policy/host/isolate 47 + script-std gate 5
   pass before consent (consent run pending). Left: trusted-input evidence hooks (macOS: HAL apple backend), then merge → fork jit → pins.
5. `main` decision waits on Home (upstream Android HAL has no GLES; OpenHarmony dropped upstream).
- PROVIDED-CRATE BINDING, resolved (5 Nov evening): the loader binds makepad_script etc. as provided crates when the program's makepad
  sources hash-match the loader's build (run14/15/16: bound → parked compile errors: traits::register_type_inner (fixed by #[relax_api] on
  it, jit3-work + .sources/makepad), then lib.rs:6691 `ScriptMod built in JIT reaches rustc code`, and the audit's list below). When they
  MISMATCH the loader runs everything from source (unbound) and offers a loader update ("The JIT itself changed (makepad_script) --
  Restart to update"); run13 and run17 ran that way. `makepad check --boundary makepad-script …` lists what OctoSense must fix for the
  bound mode: octoscript-render eval.rs:36 (ScriptVmBase.injected_globals field) and :303 (ScriptArrayStorage matched by variant;
  → arr_view op), shell glance_card.rs:287 install_nav (ScriptCode.native field), plus makepad_script internals reached from JIT
  (array_storage, set_fn, temp_string_with). Upstream's own calculator audit shows 26 HAL static reads too (upstream's problem).
  Decision for now: run unbound (loader built from a different commit than the program's makepad) until those sites use ops.
- run18 (bare relax runner, jit branch, .sources layout): first frame 30.5 s cold, 6 modules, the `ime` note only → the jit branch is
  validated end-to-end in unbound mode. Fork jit → 7407e3c5a (policy layer + device consent merged, register_type_inner bound op);
  Octoscript-Makepad jit 6b448eec; App Hub jit 537fd7c9 (policy/agent/consent calls restored); OctoSense pins moved (setup --update),
  PORT GAP marks in the shell down to 3 (WebReader, isolate memory cap stays enforced now through set_memory_bytes), App Hub 1
  (camera_preview). The bare runner (`relax`, Policy::Open, provides no crates) is the jit line's runner until the boundary sites use ops;
  `makepad run` binds its provided makepad_script and parks on the audit's violations.

## BOUND MODE (official `makepad run`, provided crates) — where it stands, 5 Nov night
- OctoSense/Octoscript-Makepad/App Hub are boundary-clean for makepad-script per the audit (0 violations in audited crates): injected
  globals via vm.injected_global, strings via vm.cast_to_string, arrays via heap.arr_view, global fns via the new ScriptVm::new_fn
  (REG_FN, fork jit c36bf46d), ScriptMod via ScriptMod::new (19 literals). Marked ops: traits::register_type_inner, ScriptLoc::new (+ impl).
- Still parked at startup: `ScriptLoc::new` (vm.rs:574) "a field of ScriptLoc (file: String) is reached from JIT code … rustc's alone" even
  though ScriptLoc is `#[relax_api(plain)]` and new is marked: the 35 "provided type … rustc lays it out in N bytes, makepad run in N+8:
  opaque" lines show every plain type with a String/Vec field is demoted to opaque in this build (relax's String is 8 bytes larger than
  rustc's), so plain-typed APIs cannot bind. That is upstream's to answer (issue to file: provided plain types with String fields).
- Runner rule stays: the bare `relax` runner (provides no crates) runs the jit line; OCTOSENSE_JIT_RUNNER=<runner> makes process apps
  launch from source through it (clients.rs launch_plan, 790ab4fd).

## 5 Nov, afternoon: process apps launch from source through the runner and are admitted
- Why the first JIT child died: upstream has no launch-secret handshake. The shell hands every process launch a secret
  (STUDIO_HANDSHAKE_STDIN=1 + first stdin line) and the hub refuses any studio socket without the X-Studio-Token header
  (`wm-hub: refused a socket (BadToken(1))` in jit3-run25.log), so the child was removed after one frame. Ported the fork's
  handshake (studio_launch_token, the two consts, header on start_studio_websocket) = fork jit f6dee0a7f, cherry-picked
  (uncommitted) into .sources/makepad. Verified with a fake hub (scratchpad hdr.py): the JIT child sends the header.
- jit3-run26 (MAKEPAD_WM_TEST_APP=finance): shell first frame 21.7 s; `wm: finance client 1 first frame in 14527 ms (cold)`
  (child main at 4.6 s = relax compile from source, cache cold), 1561 demo transactions loaded; no [E]/[W] after it.
- Status of "其余 app jit 化": every catalog process app (Task, Finance, Route, Image, PDF, Files, Browser, aichat, …) now
  launches from source through OCTOSENSE_JIT_RUNNER; in-process native apps are JIT'd inside the shell image. Still out:
  Rinx, AppCard, OctosCode, markdown editor (need fork widgets `serde`/`appcard` features → ports).

## 5 Nov, evening ("can we fix here"): the async/crypto stack compiles and runs under relax
- Reproducer `.d1-scratch/jit3-deps` (tokio rt-multi-thread+net+time+sync, httparse, ring 0.17, zeroize): run1 = 12 distinct
  type errors in tokio/mio/socket2/ring/httparse/cc, all missing relax std surface → fixed in fork jit 3641529b5 (std,
  read from the checkout at run time; MAKEPAD_RUN_STD) + differential test tests/std/core_dep_gaps_async_crypto.rs (16/16 SAME).
- Then ring panicked "foreign function `concat` is not available": `#[link_name = concat!(prefix!(), stringify!(x))]` was read
  as the token `concat` → parser now literalises link_name/export_name/link_section macro values (6402b7ed3).
- Then "`ring_core_0_17_14__sha256_block_data_order_hw` is not available": libnative.dylib HAS it but local (`t`) because ring
  builds with -fvisibility=hidden / .private_extern → new compiler/src/os/hidden.rs reads the dylib's LC_SYMTAB (ELF .symtab)
  and adds the image slide; hooked after find_system in jit.rs foreign_addr (9240af3a8). allow_c fixture gained cn_hidden_mul.
- run4: httparse, ring sha256 (2cf24dba…), zeroize, tokio all print the rustc output. tokio-only variant (jit3-deps2) also OK.
- Upstream PRs (jit3-pr, on upstream/main, clean cherry-picks, cargo check green): #1394 std gaps, #1395 attr value macros,
  #1396 hidden native symbols. Fork jit pushed at 9240af3a8. relax-front tests 79/79; relax-base has one pre-existing failing
  test (zone_alloc::domain a_sparse_detached…, base untouched by us). Next probe running: jit3-deps3 = reqwest(rustls)+keyring+chrono+serde_json+url (AppCard's stack).
- The std generator (std/check, `relax-std-gen num`) does not build with stable rustc here (internal attributes); the NonZero
  TryFrom lines were emitted by hand in the generator's exact format, and the generator source changed to match.
- jit3-deps3 (reqwest 0.12 rustls+json+blocking, tokio, serde derive, serde_json, chrono+serde, url, keyring apple-native):
  relax run2 = 0 type errors, output identical to rustc (client build, refused connect, keyring entry, serde round trip).
  AppCard's third-party stack is clear; what is left is makepad-side: makepad-ai (compat is EMPTY), code_editor (exists upstream),
  streaming-markdown-kit + makepad-diagram-kit (Robius kits, patched onto upstream widgets). Measuring: Cargo.toml (uncommitted)
  adds member apps/appcard/module + workspace deps/patches makepad-code-editor → .sources/makepad/code_editor, makepad-ai →
  compat; `relax check --allow-c octosense-appcard` → .jit3/check-appcard1.log.

## 5 Nov, late: AppCard measured on the jit line
- Fork jit now 900357ab3: + svg text/edges port (a333e31a3), makepad-ai real crate (661a836ea), relax lint guard (0cbe066e4),
  zmij NEON intrinsics (98351cb3e), blake3-pure build-script gate (900357ab3). Upstream PRs #1397 (lint), #1398 (NEON), #1399 (blake3).
- OctoSense jit: pins → 661a836e (makepad-ai patched to .sources/makepad/libs/makepad_ai, makepad-code-editor → code_editor),
  member apps/appcard/module, appcard build-dep blake3 pure, `::serde_json::json!` in AppCard (relax #1384 workaround).
- `relax check --allow-c octosense-appcard`: 161 → 47 → 45 → 35 errors, ALL in AppCard's own code, all fork-only makepad APIs:
  splash::set_link/set_collections/set_prefs/set_locale (5), Cx::system_browser + web_card_browser_id + WidgetRef::web_card +
  NativeSystemBrowserNavigation (8, the native web-card overlay), NativeComposer*/NativeDeepLink events (6), Cx::script_data_fetch_epoch
  (4), ScriptVm::eval_checked + ScriptHostContext (4), Cx::lock_script_sources (1), WidgetTree::geometry_json (1), SplashAction cast (1).
- THE REAL GAP (not visible in the type check): the fork's widgets/src/splash.rs is 6292 lines with 70 `sys.*` L0 capability methods
  (news, weather, maps/tiles, routes, stocks, quakes, locale, prefs, collections, link, …); the jit splash (upstream + our policy port)
  is 1515 lines with 1 (`notify`). AppCard's cards call those at run time. Decision for the user: port the fork `sys` module onto
  upstream splash, or move it out of makepad into AppCard/OctoSense as an app-registered module.

## 5 Nov, night: option 2 = the `sys` vocabulary moves out of makepad (user: "2")
- Fork jit bcce6142 + 40f50bec: generic Splash live-data pump (widget_async::{bump_live_data_epoch, live_data_epoch,
  register_live_data_predicate, sim_clock_secs}; Splash fields anim_next_frame/animating/last_data_epoch/last_sim_tick/tick_timer,
  1 Hz `fn tick()`, per-frame re-eval on epoch change / sim second), splash_policy::device_prompt_allowed pub, platform::locale,
  SplashAction::None default, Cx::lock_script_sources, WidgetTree::geometry_json.
- OctoSense jit 446ecd6f: NEW crates/l0-sys (octosense-l0-sys): sys.rs = fork splash.rs 1340-4136 + helpers 4150-6292 + splash_l0/
  dataset/news (generated by a rewrite script: vm.bx.heap.* → upstream VM API, crate:: → makepad_widgets::/crate::geo/crate::fetch),
  fetch.rs (URL cache, lazy retries, placeholders, response routing; bumps the live-data epoch), geo.rs (polyline5, haversine,
  worldbank_url). ensure_registered() = register_splash_isolate_mod(install) + register_live_data_predicate(body_binds_live_data).
  AppCard: ensure_registered at startup + module register; set_* from l0-sys; live_data_epoch(); host_events.rs (Native* types);
  web_card_shim.rs behind `web-card` feature (off); l0_eval on upstream VM API. `cargo check -p octosense-appcard` = 0 errors.
- Run-time contract: the host app must route Event::NetworkResponses to octosense_l0_sys::handle_network_responses (AppCard does;
  the shell's glance cards do NOT yet — add when the shell hosts L0 cards that fetch).
- Pending: relax check7, octos-app unit tests, a real run of AppCard under the JIT shell (fetch → epoch → re-eval path unverified).

## 5 Nov, later: AppCard runs in the JIT shell; live data flows through the shell as broker
- run27–30 (JIT3_FEATURES=…,app-appcard,dev-mode; MAKEPAD_WM_TEST_APP=appcard MAKEPAD_SEED_CARD_FILE=.d1-scratch/jit3-seed-weather.splash):
  AppCard = in-process module in the JIT shell, seed card evaluated, `sys.weather` → l0-sys queues → the SHELL pumps the HTTP
  (octosense_l0_sys::pump in its AppMain::handle_event; Cx::http_request is refused inside an isolate: host-IO-only) → response
  matched by the variant's request_id (not metadata_id) → "sys: data fetch loaded 1462 bytes (status 200)" → epoch bump → Splash pump.
  run30 first frame 230 s (cache warm; the module build dominates). Fixed on the way: relax build-script nesting (depth rule →
  chain rule, beca1727f), blake3-pure gate, std batch 2 (ae4a1db5a: PhantomData Ord, u8::escape_ascii, str::from_utf8* assoc,
  split_at_unchecked, Seek for Box, OsString==OsStr, vld1q_u8_x2), PopupMenuPosition in the DSL (02f9f0700), WebCard DSL placeholder.
- Remaining parked relax errors in deps (do not block the run): chrono E0034 (method resolution: Debug::fmt applies without the trait in
  scope — relax bug), serde_json map.rs Debug of iterators (`&&IterMut` vs `&dyn Debug`), tracing Pin::map_unchecked, url `self`
  module, tokio Command::uid/gid, security-framework `impl Trait` in a path + collect, sha2 sha512 NEON (vextq_u64, vsha512hq_u64,
  vsha512h2q_u64), icu/image/indexmap/jiff items partly fixed in batch 2 (indexmap Bound indexing still open).
- Splash isolate theme errors (`themes/macos/theme.splash: property widgets not found in prototype chain`) appear when the card isolate
  is restyled — pre-existing jit-line issue, to look at. `BUG: update_global_ui_handle while isolate installed; deferred` once per run.
- Visual confirmation of the re-evaluated card is still pending (needs a screenshot/instrument run); fork jit 0b3ffae05 adds the
  "splash: live re-eval" log so the next run shows it.
