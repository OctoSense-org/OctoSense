# OctoSense Android: closing the gap to native — gap analysis

Original analysis snapshot: **15 September 2026, 12:30 PDT (19:30 UTC)**. The table and trace diagnosis below describe the candidate at that time. The measured follow-up in §10 records the implemented loop fix and later ablations. This extends the [Android performance plan](performance-plan.md) with a per-scenario diagnosis and a ranked list of remaining costs.

Sources: the uncommitted candidate on branch `perf/native-frame-budget` (worktree `OctoSense-native-perf`, base `efc153c`, makepad pin `73dfc62`); the run sets `op6-marker-baseline-warm`, `op6-shader-only-warm`, `op6-mip-overview-v2-warm`, `op6-one-cache-warm`, `op6-native`, `op6-baseline` under `target/perf-artifacts/`; the Recents `atrace` capture `recents-atrace-diagnostic.txt` (4.06 s, 107 813 events); and a source inspection of makepad `73dfc62` (`platform/src/os/linux/opengl.rs`, `android/android.rs`, `widgets/src/gauss_*.rs`, `backdrop.rs`). Analysis scripts: `target/perf-artifacts/drop_analysis.py` and `atrace_timeline.py`.

The copied scripts now accept new captures beside the run JSONs: `python3 target/perf-artifacts/drop_analysis.py RUN_DIRECTORY` prints each drop's position, and `python3 target/perf-artifacts/atrace_timeline.py --trace TRACE.txt.gz --run RUN.json` joins a paired plain or gzip trace and run using the capture's clock-sync marker and detected thread IDs. The former takes a run directory relative to the script or an absolute path and skips `.invalid.json` samples; it errors on a missing directory. The latter requires a paired JSON with `phone.frames` markers; input markers make the boundary more reliable. A trace with no paired SurfaceFlinger sample is diagnostic only. The original Recents trace, its gzip replay, and a newer AppCard run-directory replay passed after the path change.

Device: OnePlus 6 `cfb7c9e3`, Android 15, 1080×2280, 60 Hz, GLES. Target: ≥55 fps and p95 ≤20 ms.

## 1. Scenario names, corrected

`target/perf-artifacts/run_cases.py` defines:

| Case | Gesture | Meaning |
|---|---|---|
| `recents-empty` | hold from the Home band | **Home → Recents** (enter and hold) |
| `recents-home-empty` | 250 ms `input swipe` up | **Recents → Home** (return; `overview` eases 1 → 0) |

Neither starts from the Android drawer. Earlier summaries that called `recents-empty` "Recents from the drawer" were wrong. The scenario that still fails every run is the **return to Home**.

## 2. Where the drops fall

`rdy` = `frame_ready − desired_present` (how late the GPU finished relative to the frame's vsync). Marked spans, one-cache build unless noted.

| Scenario | Steady `rdy` p50 / p95 | Drops |
|---|---|---|
| Native shade open/close (5 runs) | 5–7 / 8–10 ms | none mid-gesture; 2 single skips at gesture end |
| OctoSense shade open (6) | 11–14 / 23–27 ms | only frames #1–2 (33–50 ms); mid-span clean |
| OctoSense shade close (6) | 4–8 / 30–40 ms | frames #1, #3 always (start ramp `rdy` 28→48→42→30→23→18→11 over 8 frames), then 2–4 isolated mid-span skips |
| Recents → Home (4) | 11–12 / 50–60 ms | frames #1–#4 all 33 ms (`rdy` 30, 48, 60, 61, 57), a 4-frame plateau at exactly 16.7 ms/frame, then clean. n≈35, so 2 drops flip p95 |
| Home → Recents (4) | 10–12 / 25–29 ms | 1–3 drops at ACTION_UP/settle (`rdy` 23→36→43→40). The "67–100 ms first frame" is harness latency, not the app (see §6) |

Native's GPU finishes 5–7 ms after its vsync with a 3 ms tail; OctoSense shade open finishes at 11–14 ms with a 12–15 ms tail — about twice the per-frame cost, still inside the two-vsync slot.

Three distinct failure classes:

- **Class A — first-frame double work.** `cache_shade` requires `shade.open > 0.001` (`src/desk/phone.rs:273`), so the backdrop cache cannot be valid before the first pull frame. The current one-cache path records the backdrop, copies the scene, builds a 3-level pyramid, and draws the window: roughly 4–5 full-screen fills plus smaller mip work. The rejected three-cache variant also reconstructed a full-resolution blur, raising that estimate to ≈5–6 fills. Several shade-open runs contain a 50 ms maximum, although others do not. Recents has no scene cache, so its first frames record the Home scene again.
- **Class B — steady-state GPU throughput ≈1 ms over budget.** In the atrace, Thread-2 (the render thread, nice −10, cores 4/6) queues its buffer 7–9 ms after vsync, but the `GPU completion` fence wait runs 13 ms, so the GPU finishes 17–21 ms after vsync. `dequeueBuffer` blocks 1–2.5 ms on normal frames and **4.9–7.6 ms on exactly the dropped frames** (t = 238, 488, 723 ms; every ~14–15 frames): triple-buffer exhaustion from a GPU running just over 16.7 ms/frame. SurfaceFlinger is not missing (`composeSurfaces` every 16.7 ms; `PrevHwcFrameMissed` non-zero only outside the span). After UP the GPU runs ≈25 ms/frame for ~6 frames.
- **Class C — skipped vsyncs, not slow frames.** All 39 mid-span shade-close drops across the one-cache and mip-v2 sets have `desired_present` spacing of 33 ms with `rdy` 4–8 ms: the app submitted no frame for that vsync. Spacing is irregular (67–400 ms), so not a periodic tick. Shade open in the same builds has zero such skips.

The atrace's fence wait and BufferQueue blocking support GPU queue pressure, but they do not directly measure GPU execution time. The “≈1 ms over budget” estimate in Class B remains an inference until a GPU counter or paired timing experiment confirms it.

Clock state: the big cores sat at **826 MHz (minimum)** for nearly the whole gesture; the governor sees ~30 % utilisation and never boosts. GPU clock was not captured.

## 3. Per-frame work in the current candidate

W = one full-resolution 1080×2280 RGBA8 fill. The Gaussian scene and mip passes request `ClearWith` (`widgets/src/gauss_stack.rs:38-43`, `gauss_chain.rs:264-284`); GL issues `glClear` when those flags are set (`opengl.rs:1334-1342`). A clear may be cheap on a tiler, so W counts are source-level work estimates, not GPU bandwidth measurements.

| Scenario | Off-screen passes | Fills | Notes |
|---|---|---|---|
| Shade open, first pull frame | backdrop frame → gauss scene → 3 downsample mips → window; no full-resolution blur reconstruction in the retained patch | ≈4–5 W + pyramid | Class A |
| Shade open/close, moving | none repainted | cached scene quad (W) + dim SDF quad (≤W) + sheet glass (≤W) + sheet content | compositor skipped; sheet samples one full-resolution preblur texture in the three-cache build, or four bicubic mip taps in the one-cache build; content re-laid-out each frame |
| Recents ↔ Home | gauss scene seg0 → checkpoint → seg0→seg1 copy (`backdrop.rs:86-88`) → overview glass → cards → pyramid (ceil(blur) levels) → seg1→window copy (`backdrop.rs:204`) | ≈**4 W** + pyramid + 3 full-res clears | double the cached shade path; fractional levels cost 8 taps; the return's tail is ~20 frames of near-invisible glass |
| Idle fully-open shade | same backdrop capture path | ≈4–5 W + pyramid | re-recorded on every 1 Hz clock tick because `moving` is false (`phone.rs:289-299`) |

Verified not to be per-frame problems: tile captures freeze once ready (`phone.rs:186-199`); the foreground app capture is not re-recorded during zoom unless dirty, arriving or stale (`phone.rs:379-386`); icons are cached; cached `WindowFrame` passes really do not repaint on cache-hit frames (`compute_pass_repaint_order`, `cx_shared.rs:135-225`).

## 4. Ranked clues

### 4.1 Make the overview glass the compositor's final glass (Class B, pass-count lead)

Recents pays two full-screen scene copies the shade path already eliminated (`phone.rs:335-340`, `421-423`; `widgets/src/backdrop.rs:86-88`, `204`). Try what commit `0b68c89` did for the shade: `finish(cx, screen, Some((screen, blur)))` with `overview_glass` as final glass, and draw cards and overlay directly in the window pass. Fall back to segments only when the shade is open over Recents. This may remove a copy, but the exact pass reduction and performance benefit need a GPU-pass trace.

Verify: compare GPU pass counts and at least five input-aware Recents → Home runs against the same build without this routing. The `one-cache-final-overview` ablation preserved the screenshot but still failed return p95 in every valid run (§10); routing alone is insufficient.

### 4.2 Cache the static Home scene and its mips under Recents or Group glass (Class A + B)

Generalise `PhoneShadeBackdrop` (`phone.rs:66-69`, `273-299`) to "glass over a static Home scene". Under the Recents glass there is only wallpaper + home + frozen tiles (cards draw after the glass, `phone.rs:343-409`). Group glass also grows over Home (`mobile_groups.rs:379-407`); its underlying page should not need to be recaptured on every tween frame. Cache the scene and needed mip textures once, keyed on (screen, dpi, style, dark, `pages.position`, tile-capture generation), then draw live cards or Group panel content after it. Recents' fractional-level return needs two adjacent mips; Group requests level 4. Keep all requested mip slots. The earlier three-cache ablation cleared them; the current candidate does not. Do not assume the panel's scrim can move across a blur checkpoint without a pixel check.

Verify: trace producer pass paints on moving Recents returns and Group openings, compare both directions' p95 and light/dark screenshots, and inspect live tile changes and memory. The first `one-cache-overview-cache` ablation produced an almost-black Recents backdrop and failed the timing gate (§10); its compositor/pass nesting is rejected. Group caching has not yet been A/B measured.

### 4.3 Record the shade backdrop on settled idle Home frames (Class A)

Let the key be valid when `home_settled()` (`mobile.rs:131-136`) and the key changed (`phone.rs:273-278`) — the 1 Hz tick already draws those frames — and invalidate on `note_client_frame` (`phone.rs:104-110`), appearance and geometry changes. If the capture is ready before input, the first pull can hit the cache. Check whether this moves a stall to startup or the idle tick; it may reduce the early gap, but does not explain every Recents return drop.

Verify: record cache hit/miss and GPU-pass markers on the first pull after both a fresh launch and a later Home idle tick, with a fresh-process timing block and a live-tile visual check.

### 4.4 Cut the invisible tail and the settle spike (Class B)

Return-to-Home eases with k = 19 (`mobile.rs:163-177`), so 1 → 0.001 takes ≈0.36 s: ~20 frames of near-invisible glass still paying 4 W and fractional 8-tap sampling. Treat `overview < ~0.02` as done for `scene_plan.compose` and the glass (`mobile.rs:234-251`, `phone.rs:335-340`), or snap the easing. The 25 ms/frame settle spike after UP is the same tail.

Verify: compare the transition's frame count, return drops and a slow-motion light/dark capture at the snap point. Do not count an fps rise from merely shortening the measured span as a pacing improvement.

### 4.5 Missed frame requests on close (Class C — diagnose first)

The general recognizer sets `tracking = true` and requests another frame on each callback while it owns a finger (`mobile_app.rs:353-387`). An open-shade sheet drag takes a different route: `phone_pointer_at` cancels that recognizer (`mobile_app.rs:700`), leaving `tracking = false` while `phone.gesture` still holds the finger. The loop counts `phone.gesture.is_some()` in `active` but previously re-armed only when `moving || tracking`. It can therefore stop asking for frames during a shade-owned drag between changes in `ShadeState`.

**Diagnostic, 19:43 UTC:** a separate shade-close atrace captured 47 ACTION_MOVE events over an 801 ms pull. All seven observed 28–33 ms app queue gaps contained two Choreographer callbacks and MOVE events. On several skipped callbacks Thread-2 woke, ran only about 0.1–0.3 ms, and slept without `dequeueBuffer` or `eglSwapBuffers`. The matching SurfaceFlinger read failed over USB, so this trace locates missing submissions but is not a passing benchmark. The small loop change (`if active` for `new_next_frame`) is now implemented and passed the shade-close timing suite (§10).

Verify: rerun input-aware shade-close samples, split early and mid-span misses, and inspect static holds and post-release idle. If skips return, trace callback delivery, post-paint GC (`android.rs:1634-1648`), and render-thread blocking.

### 4.6 Framework: `glInvalidateFramebuffer` is never called (Class B, cheap, cross-cutting)

No invalidate exists in `opengl.rs`. On Adreno (a tiler), unused depth attachments and retained colour targets may incur load/store traffic; the source alone does not establish what the driver actually stores. `InitWith` colour targets clear only on their first use, so later binds are worth checking (`opengl.rs:1211-1229`, `1334-1342`).

Candidate: test invalidation of `DEPTH_ATTACHMENT` after off-screen passes and unused default-framebuffer depth before `eglSwapBuffers`. Only invalidate `COLOR_ATTACHMENT0` where full overwrite is proven. Shell-side companion: `WindowFrame::new_with_name` allocates a `DepthD32` per cache frame (`dock_warp.rs:99-111`) that pure-quad caches may not need. Three full-resolution colour/depth pairs have a nominal footprint near 59 MB; `Gfx dev`/Graphics PSS is not a direct measurement of those attachments.

Verify: Adreno GMEM load/store counters if the ROM exposes them; otherwise A/B fps on shade close and Recents → Home.

### 4.7 Framework: mid-frame `glFlush` and a fence per pass

`poll_texture_lifetimes` runs from the render-view path (`opengl.rs:395`) and, when `submitted > completed`, can create a fence and flush (`opengl.rs:4538-4543`). That may split a frame into extra driver submissions; the trace has not established their count or cost. A once-per-frame fence is worth an isolated framework experiment only if retirement still advances correctly.

Verify with `atrace gfx`: count kgsl submissions per frame and compare fence polling/retirement debt, idle presents, and pacing before and after.

### 4.8 Framework: unconditional per-draw `glBufferData`

`draw_call_uniforms` is uploaded for every draw item every frame (`opengl.rs:650-653`), again if `uniforms_dirty` (`737-746`), plus per draw list and per pass, through `glBufferData(STATIC_DRAW)`. The driver cost is unknown. `uniforms_gen` / `consumed_uniforms_gen` already exist (`opengl.rs:1071`); an isolated experiment can gate uploads and test `glBufferSubData` with safe in-flight storage.

Verify with `simpleperf record -p <pid> -g` during a gesture; look at the `glBufferData` share in `libGLESv2_adreno.so`, then compare render-thread CPU and return pacing with the shader/scene unchanged.

### 4.9 DVFS: big CPU cores stayed at minimum in the Recents trace

The atrace showed big cores at 826 MHz during the Recents gesture. Separate App → Home diagnostics later saw the GPU start at 257 MHz and reach 710 MHz by ~0.19–0.33 s; they do not capture its execution time in a failing Group or Recents frame (§10). Experiment on this rooted phone: poll `/sys/class/kgsl/kgsl-3d0/gpuclk` and `gpubusy` during a failing run, then test a supported faster policy only if the driver accepts it, restoring the original values immediately. The advertised GPU `performance` governor was rejected in the first probe, so no pinned-clock run exists. If misses disappear under a supported policy, clock policy is a contributor, though native PowerHAL behavior would still need direct evidence. Product-side options include an ADPF `APerformanceHint` session on Thread-2 with a 16.7 ms target (API 31+), or enough headroom from the scene and backend changes for the normal governor.

Verify: record available sysfs paths and original policies, GPU busy time/clock and missed-refresh counts in paired normal/pinned runs; check restored policies before leaving the device. The clock trace is in `atrace_timeline.py`'s source data; Makepad's serial draw/swap path is `android.rs:1606-1660`.

### 4.10 Smaller items

- The sheet dim (`mobile_shade.rs:432`) is an SDF `rounded` quad that grows to nearly full screen during close — the same class of full-screen layer that `2eca43c` removed under the drawer for a measured gain. Fold the dim into the cached-scene quad (draw the scene only in the uncovered band with a dim multiplier on `DrawPhoneApp`, `phone.rs:8-19`), or at least use `d.solid`.
- Sheet content (labels, cards, toggles, `d.wrap`, `format!`, hit rects; `mobile_shade.rs:454-604`) is re-laid-out on every moving frame although nothing inside changes. Record it into a `WindowFrame` keyed by (notifications, toggles, page, clock, battery, dark) and draw one clipped quad during movement.
- The idle fully-open shade re-records the retained ≈4–5 W capture path on every 1 Hz clock tick, and the idle Recents clock tick costs a 33 ms GPU frame; a close gesture that lands right after a tick starts one vsync behind.
- The `makepad-log` thread spends 21 ms per gesture on the little cores writing per-frame logcat markers; keep it out of timing runs.
- Per-draw state thrash (`glUseProgram(0)`, `glBindVertexArray(0)`, blend re-enable after every draw, `opengl.rs:1080-1085`; per-slot `glBindSampler`/`glUniform1i`, `1018-1038`) is minor individually and cheap to gate on change.
- Program binaries are read (`PROGRAM_BINARY_LENGTH`, `opengl.rs:1869`); confirm they are actually persisted to the app's cache directory on Android. If not, that is the cheapest first-use win. While a variant compiles asynchronously the draw is skipped and a repaint scheduled (`opengl.rs:567-573`), so first use of any variant costs a missing draw plus a frame.
- `sh.mapping.uses_time` forces display-rate repaint (`opengl.rs:551-553`); grep the phone shaders for `time` if idle presents ever creep back up.

Verify these smaller items separately: compare pass markers and shade-close pacing for dim/content changes; inspect program-cache files plus first-use draws for the shader-binary claim; compare idle present counts for any shader-time change. Keep light/dark screenshots and hit targets in the checks.

## 5. Android frame pipeline as it is (for reference)

- The Choreographer callback runs on the Java main thread (`android_jni.rs:826-854`, `CALLBACK_ANIMATION` on pid 14100 in the trace, little cores) and only sends `RenderLoop` to the Rust thread. Event drain, draw event, GL encode and `eglSwapBuffers` all run serially on one Rust thread (`android.rs:387-467`, `1606-1660`): Thread-2. Nothing overlaps.
- SurfaceFlinger phases on this phone: app phase = SF phase = 1 ms, work duration 16.67 ms. The whole callback → thread wake → draw → encode → queue chain has one refresh period; any overrun slips exactly one refresh, which is the p95 = 33 ms signature.
- EGL config RGB888, depth 16; no `eglSwapInterval`; no `ANativeWindow_setFrameRate`; default (triple) BufferQueue. The OctoSense layer composes as DEVICE (HWC overlay) at idle; SF did 22 RenderEngine compositions during the Recents gesture (region sampling), a small extra GPU contention.
- Vsync frame-timeline deadline data is ignored (`android_jni.rs:848`); animation time is `time_now()` at draw. Reading `AChoreographerFrameCallbackData_getFrameTimelineDeadlineNanos` would give the real deadline.
- Emitting ATrace sections around draw-event, encode and swap (dlsym `ATrace_beginSection` from `libandroid.so`) and naming Thread-2 would make a perfetto trace show which stage overruns; today only Java's callback and a `sched` line are visible.

## 6. Measurement caveats to fix before the next comparison

- Recents → Home is judged on ~35 frames: two drops flip p95. Use an 800 ms swipe like the shade cases, or report drop counts alongside p95.
- Drive Home → Recents from one continuous input stream that can move and then hold near the switcher threshold. The present four-process `input motionevent` sequence adds 50–100 ms of harness latency (DOWN dispatched at 53 ms, first MOVE at 105 ms in the atrace) that is counted as a first-frame gap. A simple `input swipe` does not reproduce the required hold; use a controlled injector or report this boundary separately. The 83.7 ms "first gap" in the diagnostic is harness latency, not an app stall.
- Native's span is inferred, OctoSense's is marked. Native presents at desired + 37 ms (two-vsync pipeline), OctoSense close at desired + 25 ms, so cross-app `rdy` comparisons carry ~5 ms ambiguity; within-OctoSense comparisons are exact.
- Two `recents-empty` runs in `op6-one-cache-warm` failed with `No phone.frames markers; relaunch with the trace extra`. They are harness failures; record them as rejected.

## 7. Correctness risks in the current diff

- `PhoneShadeBackdrop.key` (`phone.rs:278`) ignores tile-capture dirtiness, `phone.clock` and island state; a tile whose client draws during a pull appears only once the sheet settles. Include a capture generation in the key if 4.3 is adopted, since the cache then lives across idle time.
- The earlier three-cache ablation cleared `mip_textures` and required a `shade_preblurred` shader branch. Those are absent from the current one-cache candidate; if a reconstructed full-resolution blur is reintroduced, verify every snapshot consumer before clearing any mip slots.
- The cache-hit early return (`phone.rs:290-299`) skips `phone_content`, the exclusion/keyboard updates and the perf spans. Safe under its current guards (`keyboard < 0.5`, Home only); revisit if `cache_shade` is relaxed.
- `wm_phone_wallpaper` is re-parented by `attach` to whichever pass is current; ordering holds because it is never dirty, but a future `repaint_pass_and_child_passes` on the window would repaint it under GL.
- Memory: the current diff retains one full-resolution colour + depth cache frame plus the compositor's stacks. The three-cache ablation added two more full-resolution targets without a repeatable timing gain. Watch Graphics PSS and growth across repeated open/close, but do not treat it as a direct GPU allocation census.

## 8. Vulkan notes (for the later experiment)

Revision `73dfc62` waits `in_flight_fence` at the top of every frame (`vulkan.rs:2911-2914`) and before and after every off-screen pass (`3557-3559`, `3946-3948`): potentially many CPU waits per frame on this workload. Swapchain is FIFO with `min_image_count + 1` (`7590-7601`). A same-phone release probe using `MAKEPAD=vulkan` confirmed a Vulkan device and swapchain, then failed every **usable** warm shade pair: 4 openings at 21.27–22.02 fps and 5 closes at 36.78–38.28 fps, against six passing GLES pairs near 58 fps. One warm opening and three fresh-process openings had too few active presents and were excluded. The first immediate screenshot was black during startup, while the later warm shade rendered correctly; startup and full-shell visual equivalence need separate checks. The Vulkan slowdown is measured, but the CPU wait cost is not. The [Vulkan probe record](../../target/perf-artifacts/ (frame-baseline worktree) vulkan-probe-validation.md) preserves driver properties, hashes, raw runs, and restored GLES state. A useful follow-up would trace waits and GPU execution, then test grouped command buffers and GPU barriers with at least two safe frames in flight, rather than simply removing fence waits.

## 9. Suggested order

1. Keep the measured Class C fix (4.5), subject to the idle/static-hold checks, and rerun the same input-aware suite on the final APK.
2. Correct Recents input timing (§6), then measure clock policy (4.9) and a visually correct static Home cache under Recents and Group glass (4.2). Group opens 0/6 and shows three early 33–50 ms gaps; caching that producer has not yet been isolated. The first final-glass routing (4.1) did not pass, and the first overview-cache prototype was visually wrong.
3. Test first-pull pre-recording (4.3) against its startup/idle cost and transition-tail trimming (4.4) against actual missed-refresh counts.
4. Use paired traces to select framework experiments (4.6–4.8); preserve resource ordering and idle retirement behavior.

## 10. Measured follow-up — 15 September 2026, after the analysis snapshot

The source candidate now retains one shade backdrop frame and its existing Gaussian mips, uses the flat sheet and overview shaders, and re-arms `new_next_frame` while `phone.gesture.is_some()` (`mobile_app.rs:377-386`). The phone remained on GLES. Counts below are **valid input-aware** 60 Hz runs; disconnected or missing-marker samples were rejected. The target is ≥55 fps and p95 ≤20 ms in each run.

| Scenario / run block | Valid runs passing both targets | Observed fps / p95 result |
|---|---:|---|
| Shade open, warm (`op6-pointer-loop-shade-warm`) | 5/6 | 55.04–58.51 fps; one p95 ≈33.5 ms |
| Shade close, warm (same block) | **6/6** | 57.00–57.93 fps; p95 ≈16.8 ms in all six |
| Shade open and close, fresh-process (`op6-pointer-loop-shade-fresh`) | **5/5 each** | opens 57.33–58.53 fps; closes 56.99–57.92 fps; all p95 ≤16.9 ms |
| Shade open / close, **final main APK**, warm (`op6-pointer-loop-final-shade-warm`) | **6/6 each** | opens 57.36–58.56 fps; closes 57.91–57.97 fps; all p95 ≤16.83 ms |
| Shade open / close, **final main APK**, fresh-process (`op6-pointer-loop-final-shade-fresh`) | **4/5 / 5/5** | opens 56.13–58.53 fps, with one p95 ≈33.5 ms; closes 57.91–57.96 fps, all p95 ≤16.8 ms |
| Home → Glance page / return (`op6-pointer-loop-pages-warm`) | **6/6 / 4/4** | ~57.3–58.9 fps; all valid p95 ≤16.9 ms; two returns lost over USB |
| Home → Recents hold / Recents → Home 250 ms (`op6-pointer-loop-recents-warm`) | 4/5 / **0/5** | enter 53.61–56.61 fps; return 49.76–56.42 fps, p95 ≈33.5 ms on every return |
| Group open / close (`op6-pointer-loop-final-group-warm`) | **0/6 / 0/5** | open 38.85–44.04 fps; close 44.80–50.33 fps; one close rejected over USB |
| AppCard open / App → Home (`op6-pointer-loop-final-app-warm`) | **0/6 / 5/5** | opening 46.77–51.96 fps, with a 67–84 ms last active interval; return ~59.74–59.76 fps |
| Populated Home → Recents / Recents → Home (same AppCard block) | **2/5 / 1/5** | enter ~55.07–56.64 fps; return ~52.90–59.76 fps; most returns p95 ≈33.5 ms |
| Island triple-tap demo, final APK, fresh process (`op6-pointer-loop-final-island-fresh`) | **2/5** | 56.89–59.76 fps; each of the three failures had one 33.5 ms gap and p95 ≈33.5 ms among 20 intervals |

On both warm shade-close blocks, the remaining missed refreshes were confined to the first one to three frames; the earlier candidate had two to four additional mid-span misses. The final APK/source patch hashes are recorded in `target/perf-artifacts/validation.md`. Fresh-process suites wait six seconds after each launch, so they are neither fresh-install nor immediate-launch tests. The final APK lost one fresh opening, although the earlier same-code isolated APK passed five; report both run blocks rather than treating either as deterministic. A new native refresh block (`op6-native-refresh-final`) had **5/5 closing** runs at ~59.75 fps and p95 ~16.7–16.8 ms with no skipped refresh; its inferred opening spans passed 3/5, including a 167 ms first interval in one run whose input boundary cannot be verified. OctoSense's final six warm shade closes meet the pacing gate at ~57.9 fps but still skip two early refreshes each. Pixel-level response latency and full-shell native parity remain unmeasured/unmet.

Two rejected Recents optimizations narrow the next step. `one-cache-final-overview` moved overview glass to the compositor's final-glass path and preserved the Android screenshot, yet Recents → Home passed **0/5** valid returns. `one-cache-overview-cache` added a full-resolution Home/blur cache but made the Recents background almost black and passed **0/6** valid returns; its source was reverted. Do not promote either prototype. The return's three early 33 ms gaps persist in the visually correct candidate, so a correct cache must be judged with both frame counts and screenshots.

Group open has a repeatable first three present-gap sequence of about 50, 33 and 50 ms, with GPU-ready lateness initially ≈49→84→79 ms before it clears. A fixed-level rounded flat group shader raised opening into roughly 43–49 fps but passed **0/6** valid opens and closes; its settled panel was visibly more opaque. Moving group glass to the compositor's final consumer still passed **0/6** opens and **0/3** valid closes, despite a correct screenshot. Neither ablation was kept in the main patch. The rooted phone reports GPU governor `msm-adreno-tz` and a 257 MHz idle clock, but rejects writing the advertised `performance` governor; the diagnostic made no pinned-clock run and confirmed the original governor afterward. GPU execution time and clock policy during the failing first frames remain unresolved.

AppCard opening's repeatable late 67–84 ms gap falls on the last active frame, when the settled full-screen capture is re-recorded (`phone.rs:375-382`). Two isolated freshness ablations avoided that unconditional capture and raised opening to 3/5 and 6/6 passes, respectively, but **both failed every valid App → Home return** and worsened populated Recents return. Both were rejected; the retained capture is intentionally still refreshed at the settled frame until visual freshness and transition pacing can pass together. Separate read-only clock captures around App → Home found 257 MHz at gesture start on both builds; the retained build reached 710 MHz by the ~0.19 s sample and the settled-capture ablation by ~0.33 s. Those are two diagnostic swipes with ~60 ms sampling, not proof that clock policy causes the benchmark difference. `gpubusy` did not provide usable per-frame execution time on this ROM.

The Island action is three quick taps on the status clock. The first warm-block driver attempts (`op6-pointer-loop-final-island-warm`) recorded no valid active span and are **invalid**, not pacing results; a manual `phone.frames` log confirmed the demo does animate. Restarting the process before each tap triple produced five valid 21-present spans of ~335–352 ms. The three single skips fell at frame #1 twice and #4 once. With only 20 intervals, one skip changes p95 to ≈33.5 ms; two clean runs passed at ~59.7 fps. This remains below the repeatability gate and is not a native-equivalent island comparison. A same-process `dumpsys meminfo` check around twelve shade open/close pairs changed settled Graphics PSS from 93,400 to 94,024 KB after ten seconds and total PSS from 194,941 to 197,568 KB. The short check does not establish long-session growth or direct GPU allocation size.

## 11. Measured GPU time and clock — 15 September 2026, 22:00–23:00 UTC

Everything in §2–§4 inferred GPU cost from presentation timestamps. This section measures it. The OnePlus 6 kernel exposes the Adreno (kgsl) tracepoints, so `adreno_cmdbatch_submitted/retired` give each GPU submission's execution time (`retire − start` in 19.2 MHz ticks), `kgsl_pwrlevel` gives every clock change, and `kgsl_waittimestamp_*` gives CPU-side waits on the GPU. `target/perf-artifacts/kgsl_gpu_timeline.py` prints the per-submission timeline joined to the app's `phone.frames`/`phone.input` markers (same `CLOCK_MONOTONIC`), and `kgsl_frames_summary.py` prints each frame's main submission with the clock it ran at and the same work scaled to 710 MHz. The capture script is the session's `kgsl_trace.sh` (tracefs via `su`, `trace_clock mono`, kgsl events only). USB drops interrupted several captures; a run whose markers file is empty or whose first heavy submission is not preceded by an input marker is diagnostic only.

### 11.1 The GPU clock, not the first frame, decides the short transitions

Paired A/B on the final GLES APK (`d355ef1d…`), same session, warm blocks, `min_freq` of the Adreno devfreq pinned at 710 MHz and then restored to 257 MHz (`op6-gpu-pin710-*`, `op6-gpu-ctrl257-*`):

| Scenario (3 runs each) | Pinned 710 MHz | Unpinned control |
|---|---|---|
| Recents → Home (250 ms) | **59.8 / 58.2 / 56.6 fps**, missed 0 / 1 / 2 | 54.9 / 54.9 fps, missed 3 / 3 (third run rejected) |
| Home → Recents hold | 56.6 / 55.8 / 56.6 fps, p95 ≤16.8 ms | 55.2 / 54.4 fps, p95 33.5 ms |
| Group open | 52.0 / 53.8 / 53.8 fps, missed 3 / 2 / 2 | 41.8 / 41.9 / 44.8 fps, missed 6 / 6 / 5 |
| Group close | 56.9 / **59.8** / 56.9 fps, missed 1 / 0 / 1 | 50.8 / 47.8 / 47.8 fps, missed 3 / 4 / 4 |

The kgsl traces explain it. The GPU idles at 257 MHz (`msm-adreno-tz`, 7 levels, 257–710 MHz), and the first governor step comes **80–125 ms after the first heavy submission** in every capture, with 710 MHz reached at 180–240 ms. A 250 ms return or a 15-frame group animation therefore runs half or more of its frames at the floor clock:

| Trace (final GLES APK) | Main submission per frame at the floor clock | Same work at 710 MHz | Clock steps after the gesture |
|---|---|---|---|
| Recents → Home (`kgsl-gl-recents-home`, see caveat) | 14.6–14.9 ms | ≈5.3 ms | 342 MHz at 125 ms, no further step |
| Group open (`kgsl-gl-group-open`) | **46–47 ms**, then 18 ms at 710 MHz | **≈17–18 ms** | 342 at 120 ms … 710 at 237 ms |
| Shade open (`kgsl-gl2-shade-open`) | 15–20 ms | 5.5–7.4 ms early, ≈10 ms steady | 342 at 80 ms, 414 at 90 ms |
| Shade close (`kgsl-gl2-shade-close`) | 19 ms, then 15, 12, 10.7, 9.1 as the clock rises | ≈7–8 ms | 342 at 25 ms after the first heavy frame |

Caveat: the `kgsl-gl-recents-home` markers show `screen=Home overview=0` throughout, so that capture is a home swipe with nothing to return from (the preceding hold was lost to a USB drop); its 14.7 ms is the cost of the live wallpaper + home page, not of the overview return.

Consequences. At 257 MHz a frame has ≈16.7 ms of GPU; a shade-close frame costs 19 ms there and a group frame 46 ms, so both miss until the clock ramps, whatever their first frame does. A transition passes at the floor clock only if its frame costs **≤ ~4.5 ms at 710 MHz**. Native SystemUI's frames are far below that (no per-pixel blur on this device), which is why the ramp never shows in its numbers. The PowerHAL/ADPF path (`performance_hint` service, `HAL Support: true`, preferred rate 16.67 ms) boosts CPU for a hint session; it does not raise the GPU clock, and the app cannot write the devfreq floor without root. So the product-side fix is per-frame GPU cost, and the measurement to optimise against is **GPU ms per frame at the floor clock** (or at a pinned clock), not fps at whatever clock the governor chose.

### 11.2 Why the Vulkan build is slower

Same kgsl capture on the isolated `MAKEPAD=vulkan` probe APK (`op6-vulkan-probe.apk`, `863b2894…`, Adreno Vulkan 1.1.128), shade open and close (`kgsl-vk-shade-open`, `kgsl-vk-shade-close`; the two `kgsl-gl-shade-*` files were captured while the Vulkan probe was still installed, after a failed GL reinstall, and are Vulkan data too):

1. **No CPU/GPU overlap.** Every app submission has `inflight = 0`: the GPU never has a second batch queued, and 2–5 ms of GPU idle separate every batch (the CPU recording the next frame). The GLES traces reach 3–4 batches in flight. This is the source structure in §8: one command buffer and one fence, waited before acquiring the next image (`vulkan.rs:2911-2914`) and before and after every off-screen pass (`3557-3559`, `3946-3948`).
2. **The GPU clock never leaves 257 MHz.** Zero `kgsl_pwrlevel` events in ~1 s of continuous gesture on Vulkan, against 5–9 in each GLES capture. With the serialised pipeline the governor's busy fraction stays below its ramp threshold, so the same work also runs at the floor clock for the whole gesture — which the GLES build escapes after ~120 ms.
3. **Per-frame GPU work is somewhat higher, not lower.** Shade close: 32.8 → 19.6 ms per frame at 257 MHz (≈11.9 → 7.1 ms at 710) on Vulkan against 19 → 9 ms (≈7 ms at 710) on GLES; shade open: 11–17 ms (4.1–6.1 at 710) against 15–20 ms (5.5–7.4) — similar. Vulkan does not reduce the pixel work; it adds the synchronisation cost.

Together these give the measured 21–22 fps opening (frame ≈ CPU record + GPU execute + image acquire, three refreshes at the floor clock) and 37–38 fps closing (alternating one and two refreshes). A Vulkan backend that helps would need all passes of a frame in one command buffer with image barriers, at least two frames in flight with their own fences and semaphores, and the acquire moved off the critical path — after which the GPU clock would ramp as it does under GLES. Until then the unchanged backend is rejected, and Vulkan is not a route to the target.

### 11.3 Candidate: one kept scene for the shade, Recents and Group, and flat fixed-level overlays

Three iterations on the final source, all with 61/61 mobile tests, `cargo check` and the Android release build passing; each is an exact `git diff --binary` beside its APK in `target/perf-artifacts/`:

| Build | Source patch | APK | Adds |
|---|---|---|---|
| v1 `scene-cache` | `19d0f0e1…` | `e00b45f1…` | the scene cache, sheet bands, flat group panel, solid scrim, group prewarm |
| v2 `scene-cache-v2` | `c242d47e…` | `39310036…` | keeps the cache frame across non-cacheable frames; an empty return no longer forces `openness = 1`; `[phone.scene]` diagnostics |
| **v3 `scene-cache-v3`** | `c5e12d36…` | `7a4b4cb4…` | overview glass samples a fixed level 3; the scene quad is skipped under a settled (opaque) overview |

What changed (`src/desk/phone.rs`, `src/mobile_groups.rs`, `src/mobile_surface.rs`, `src/mobile_app.rs`, `src/mobile_perf.rs`):

- `PhoneShadeBackdrop` became `PhoneSceneBackdrop`: the still home scene and a **four-level** pyramid, recorded on idle home/Recents frames and on the first overlay frame whose key does not fit, read on every moving frame under the shade, the overview glass or a group window. A home animation without an overlay (island, settling tile) draws live and drops the key. The overview glass and the group panel take the kept snapshot instead of opening a compositor checkpoint; the shade behaves as before but records on idle Home frames too, so its first pull frame finds the cache.
- Under an opaque sheet only the rows beside it draw the cached scene (`DrawPhoneApp` gained `uv_pos`/`uv_size`; 48-unit overlap for the sheet's shadow); under a settled overview glass the scene quad is skipped.
- `PhoneGroupGlass`: the flat material at fixed level 4, keeping the liquid panel's `surface_alpha` 0.9 and hairline border, no lens/gradient/chroma; the scrim is a flat fill; the variant is prewarmed with the others. `PhoneOverviewGlass`: fixed level 3 — the blur no longer grows with `overview`, only the opacity does (the same cross-fade to frosted, at four taps per pixel instead of eight).
- `drive_gesture` sets `openness = 1` for a home-up from Recents only when an app is open; an empty return keeps the recorded scene under the fading glass instead of fading the home page in.
- Diagnostics: `[phone.scene] hit|record|live …` in logcat under the `phone.frames` trace. On the v3 return: 36 hit, 2 record (the idle frames before), 1 live (the settle frame).

Measured, warm blocks, unpinned clocks, valid input-aware runs (`op6-scene-cache-v3-*-warm`, with v1 in `op6-scene-cache-*-warm`):

| Scenario | v3 | v1 | Before (final GLES APK) |
|---|---|---|---|
| Recents → Home (250 ms) | **59.8 / 59.8 / 56.5 fps, missed 0 / 0 / 2** — 2/3 pass | 51.7 / 55.0 / 50.4, 0/3 | 49.8–56.4 fps, 0/5 |
| Home → Recents hold | 56.7 / 57.4 / 57.5 fps, p95 ≤16.8 ms — 3/3 | 3/3 | 4/5 |
| Group open | **59.8 / 59.8 / 59.8 fps, 0 missed — 3/3** | 59.8 / 59.8, 2/2 | 38.9–44.0 fps, 0/6 |
| Group close | **59.8 / 59.8 / 59.7 fps, 0 missed — 3/3** | 2/2 | 44.8–50.3 fps, 0/5 |
| Shade open | 57.4 / 57.4 / 57.4 fps, p95 ≤16.8 ms — 3/3 | 3/3 | 6/6 |
| Shade close | 58.0 / 58.0 / 58.9 fps, p95 ≤16.8 ms — 3/3 | 3/3 | 6/6 |

GPU per frame at the 257 MHz floor (kgsl, main submission): Group open **11–12 ms** on every frame (≈4.3 ms at 710; the clock never ramped and it still passed) against 46 ms before; Recents → Home 7–12 ms (≈4–5 at 710); shade close still 25 ms on its first four frames (≈9 at 710) — the two early skips every close keeps come from there, and the sheet's re-laid-out content (clue 4.10) is the next cost to take out. Screenshots of Home, the group window, Recents and the shade on v2/v3 (`op6-scene-cache-v2-*.png`, `op6-scene-cache-v3-*.png`, chunked transfer) show the frosted flat panel with its border and scrim, and unchanged sheet and overview appearance.

v1 taught two things worth keeping. Its return did not improve because `drive_gesture` set `openness = 1` for an empty return, so the home page faded in and every frame drew live — the `[phone.scene]` line, not the frame timings, showed it. And v1 assigned `None` back to the cache slot on every non-cacheable frame, freeing and re-allocating ~20 MB of textures around every app or keyboard frame; the old shade code had left the slot in place.

Populated Recents (an open app zooming) and AppCard opening still draw live: their scene changes with `openness` every frame. The next lever there is to make the home page's fade an overlay property rather than per-element opacity, so the scene stays cacheable while a card zooms.

Regression and fresh-process blocks on v3 (`op6-scene-cache-v3-*`, details in the validation record): pages 6/6; App → Home 3/3 at 59.8 fps; populated Home → Recents 2/2 valid; **fresh-process Recents → Home 3/3 (59.8 / 58.1 / 58.1 fps)**, fresh Group open 2/2 valid and close 2/2, fresh shade close 3/3 and open 2/3. Unchanged failures: AppCard open (0/3, the 67–84 ms settled-capture frame) and populated Recents → Home (0/2, draws live). The warm island driver again recorded no active span and is not scored. Graphics PSS after the blocks 92,464 KB, below the earlier 93,400–94,024 KB check.

## 12. Second round — 15 September 2026, 23:00–23:35 UTC: sheet content, deferred app capture, fade as overlay

Four changes on top of `OctoSense-mobile` `main` (PR #1), measured as `sheet-capture-v3` (APK `8ac4a623…`; v1 `94bd23a7…` and v2 `20d12335…` are the two earlier iterations kept beside it):

1. **The sheet's content is recorded once and shown as one quad while the sheet moves** (`mobile_shade.rs`, `ShadeContentCache`). Recorded at the settled content rect on idle frames — with the shade closed too, on the kept-scene frames, hits discarded, so the first pull already finds it — and keyed on everything it shows (cards, toggles, sliders, page, clock, battery, appearance, size; card ages by the minute). A moving frame whose key does not fit draws live rather than recording inside the gesture (v1 recorded there and made fresh-process openings worse). A settled sheet records and presents, so its cards and sliders keep responding.
2. **The app's settled capture is recorded on the idle frame after the zoom**, not on the zoom's last frame (`phone.rs`): the settle frame shows the zoom's capture at full size and asks for one more frame; the 67–84 ms record moves to a frame where nothing moves. Idle presents with an app open stayed at 0 over 3 s (no repaint loop).
3. **The home page's fade is an overlay**: recorded at full opacity into the kept scene (`draw_home(.., still)`) and dimmed by a flat fill (black, 0.35 × `openness`) over the cached quad. The kept scene therefore also serves the populated Recents transitions and the home-up drag on an open app (`screen == App && overview > 0`), whose live frames had cost ~11 ms at 710 MHz (30 ms at the floor) and were the App → Home misses.
4. **Every phone material is prewarmed** (the iOS dock and keyboard panels joined the sheet/overview/group variants), so no program meets the driver on first use.

Warm and fresh blocks, unpinned clocks (`op6-sheet-capture-v3-*`, plus `op6-sheet-capture-v2-*` for the group/pages/fresh blocks that did not change between v2 and v3):

| Scenario | v3 (PR #1) | This round |
|---|---|---|
| AppCard open | 0/6, 47–50 fps, 67–84 ms last frame | **5/5, 59.7–59.8 fps, 0 missed** (3 warm + 2 fresh) |
| App → Home | 3/3 | 4/5 (59.7 / 58.1 / 58.1 / 58.1; one 56.6 with p95 33 ms) |
| Populated Recents → Home | 0/2 | **5/5, 58.1–59.7 fps** |
| Populated Home → Recents | 2/2 valid | 5/5, 56.6–57.4 fps |
| Shade close | 3/3, 1–2 early skips each | 3/3 warm + 3/3 fresh; **0 skipped refreshes in 4 of 6** (moving frames 12 ms at the floor, were 25) |
| Shade open | 3/3 | 2/3 warm (one 53.9 fps, 5 skips), 3/3 fresh — the sheet glass growing over the whole width still costs ~15 ms at the floor for its first frames |
| Empty Recents → Home / Home → Recents | 3/3 / 3/3 | 3/3 / 3/3 (warm and fresh) |
| Group open / close | 6/6 | 6/6 warm + 6/6 fresh (v2), 59.7 fps |
| Pages | 6/6 | 6/6 |

Diagnosis of the App → Home regression that v1/v2 of this round introduced, for the record: its markers showed eight return frames 30 ms apart, the kgsl trace showed the drag on the App screen at ~11 ms/frame at 710 MHz — the live path with the overview glass — which the earlier build had also paid but whose late frames the buffer queue happened to absorb (its `ready − desired` p95 was 79–93 ms with no skips). Keeping the scene during that drag (change 3, v3) removed it.

Cold-install first use remains unmeasured: two attempts (uninstall, install, launch, 14 s, pull) produced no `phone.input` marker for the pull, so the harness rejected them; the process was up and a later launch of the same install measured normally. First-launch behaviour needs a separate look.
