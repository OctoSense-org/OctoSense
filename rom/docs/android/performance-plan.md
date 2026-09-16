# OctoSense Android rendering improvement plan

Original table snapshot: **15 September 2026, 18:15 UTC**. Later validated run blocks are dated under the objective and in the [gap analysis](perf-gap-analysis.md#10-measured-follow-up--15-september-2026-after-the-analysis-snapshot).

## Objective and current conclusion

Bring common Android shell interactions to **at least 55 fps with p95 frame intervals at or below 20 ms**, while preserving visual quality, responsiveness, and low idle activity. At 60 Hz, the display has a new refresh every 16.7 ms. p95 is the interval below which 95% of measured frame intervals fall.

The original “about half native” result described shade opening in the baseline. On the **final Android APK**, the flat shade shader, one retained backdrop and corrected frame loop pass the pacing target in **6/6 warm opening/closing pairs, 4/5 fresh-process openings, and 5/5 fresh-process closings**. Page swipes pass in every valid newer run. Recents return, group transitions and AppCard opening still miss; first visible-pixel response latency needs a paired native measurement. Native parity across the shell is not established.

Prioritize the remaining shader, cache, and first-frame costs on the measured OpenGL ES renderer. Evaluate Vulkan as a separate, controlled experiment; do not assume an API switch supplies the missing performance.

**Update, 15 September 2026, 19:30 UTC:** the [gap analysis](perf-gap-analysis.md) diagnoses the remaining misses from the `op6-one-cache-warm` runs and the Recents atrace. Shade open now passes 6/6; shade close passes 3/6 (skipped vsyncs, not slow frames); Recents → Home fails 0/4 on its first four frames and a GPU-bound plateau. It ranks the remaining costs and notes that `recents-empty` is Home → Recents and `recents-home-empty` is Recents → Home; neither starts from the Android drawer.

**Follow-up, 15 September 2026, 20:29 UTC:** a shade-close atrace exposed a missing `new_next_frame` request while the shade owned a finger. Re-arming while `phone.gesture.is_some()` removed the later mid-span skips. The exact final APK passes 6/6 warm shade pairs and 5/5 fresh-process closes; one of five fresh openings still has p95 ≈33.5 ms. Home idle presented five frames in five seconds before and after a two-second static shade hold (118 presents), and light/dark sheets were visually checked on the phone. Native SystemUI closed at ~59.75 fps with no skipped refresh in five valid runs; OctoSense closed at ~57.9 fps with two early skips in each warm run. Empty Recents → Home remains 0/5; group open/close and AppCard open remain below target. Two Recents, two group and two app-capture ablations were visually or statistically insufficient and were rejected; their evidence and risks are in the [gap analysis](perf-gap-analysis.md#10-measured-follow-up--15-september-2026-after-the-analysis-snapshot).

**Follow-up, 15 September 2026, 20:37 UTC:** a fresh-process triple-tap Island block produced five valid short animation spans; 2/5 passed the pacing gate, and each failure skipped one early refresh. Twelve same-process shade open/close pairs changed settled Graphics PSS from 93,400 to 94,024 KB after ten seconds; this small readout does not prove GPU memory stability over longer use. Invalid warm Island samples were relabeled and excluded. See the [gap analysis](perf-gap-analysis.md#10-measured-follow-up--15-september-2026-after-the-analysis-snapshot) and [validation record](../../target/perf-artifacts/validation.md) for raw runs, exact source/APK hashes, and remaining checks.

**GPU measurement and scene-cache follow-up, 15 September 2026, 22:35 UTC:** kgsl tracing on the phone ([gap analysis §11](perf-gap-analysis.md#11-measured-gpu-time-and-clock--15-september-2026-22002300-utc)) established that the remaining short-transition misses come from the Adreno clock sitting at its 257 MHz floor for the first ~120 ms of every gesture — pinning it to 710 MHz made Recents → Home and Group close pass at once — and that the Vulkan probe is slow because the backend serialises CPU and GPU and never lets the clock ramp. The `scene-cache-v3` candidate (one kept home scene and pyramid under the shade, Recents and Group; flat fixed-level group and overview materials) passes **Group open/close 6/6 at 59.8 fps with no missed refresh** (was 0/11) and **Recents → Home 2/3 at 59.8 fps** (was 0/5), with shade, pages and Home → Recents unchanged; the GPU cost of a group frame fell from 46 ms to 12 ms at the floor clock. Regression blocks and the fresh-process runs are recorded in the gap analysis and [validation record](../../target/perf-artifacts/validation.md). Populated Recents and AppCard opening still draw live.

**Second round, 15 September 2026, 23:35 UTC:** `OctoSense-mobile` PR #2 ([gap analysis §12](perf-gap-analysis.md#12-second-round--15-september-2026-23002335-utc-sheet-content-deferred-app-capture-fade-as-overlay)) records the sheet's content once, defers the app's settled capture to the idle frame after the zoom, makes the home fade an overlay so a zooming card keeps the recorded scene, and prewarms every material. AppCard opening passes **5/5 at 59.7 fps with no missed refresh** (was 0/6), populated Recents → Home 5/5 (was 0/2), shade close skips no refresh in four of six runs; shade open (2/3 warm) and one App → Home run in five remain the visible variance. Cold-install first use is still unmeasured.

**Vulkan follow-up, 15 September 2026, 21:37 UTC:** the OnePlus 6 supports Adreno Vulkan 1.1.128, and a release `MAKEPAD=vulkan` build of the same optimized UI created a Vulkan device and swapchain. The unchanged pinned backend failed **0/4 usable warm shade openings** at 21.27–22.02 fps and **0/5 closings** at 36.78–38.28 fps; the saved GLES build passed 6/6 warm pairs near 58 fps. One warm opening and three fresh-process openings had too few active presents and were excluded. Vulkan's per-off-screen CPU waits are still an unmeasured cause candidate, so the unchanged backend is rejected as a full-app replacement on this phone. Exact APK hash, device properties, raw runs, visual caveats, and the restored GLES installation are in the [Vulkan probe record](../../target/perf-artifacts/ (frame-baseline worktree) vulkan-probe-validation.md).

## 1. Evidence and limits

### Same-device measurements

Device: OnePlus 6 `cfb7c9e3`, Android 15, 1080×2280, 60 Hz. These are release builds with the frame monitor off. Shade measurements use single 800 ms gestures. OctoSense pulls start below Android's status bar; native pulls start at the top edge. Native SystemUI is a frame-pacing reference, not an identical rendering workload.

| Scenario / build | Runs | fps | p95 interval | Interpretation |
|---|---:|---:|---:|---|
| PR #28 baseline: shade open | 3 | 29.3–31.6 | 33.5–50.2 ms | Misses both targets |
| PR #28 baseline: shade close | 3 | 48.2–49.5 | 33.5 ms | Misses both targets |
| Native Android: shade open | 3 | 57.9–58.5 | 16.9 ms or less | Same-phone reference |
| Native Android: shade close | 2 | 59.8 | 16.9 ms or less | Same-phone reference |
| Flat shader + cache: warm shade open | 2 | 58.6 | 16.8 ms | Meets pacing targets in these runs |
| Flat shader + cache: first shade open after launch | 1 | 46.9 | 16.9 ms | **234 ms gap after the pull**; inferred boundary may include an idle redraw, so this does not establish an animation or compilation stall |
| Flat shader + cache: shade close | 3 | 50.5–57.9 | 16.9–33.5 ms | Not consistently passing |
| Page to Glance / return, same candidate | 1 each | 58.1 | 16.8–16.9 ms | Promising; repetition and paired baseline pending |
| Recents hold, no open apps, generic overview shader | 1 | 47.8 | 33.5 ms | 83.7 ms maximum gap; improvement pending |

The first-use sample is not a fresh-install or cold OS-cache measurement. More repetitions are required before treating any result as a release gate pass.

Raw timestamps are in [the local performance artifacts](../../target/perf-artifacts/), particularly `op6-baseline/`, `op6-native/`, and `op6-flat-shade/`. Build context and ongoing results are in [validation.md](../../target/perf-artifacts/validation.md); its latest-session section supersedes its older interim notes.

### What the evidence supports

- Wallpaper caching alone showed no convincing Android frame-rate gain.
- A retained Home scene and blur snapshot remove repeated background passes during shade movement; Metal tracing verified that work reduction.
- Specializing the flat shade shader, together with caching, produced a large warm-opening improvement on Android. The available runs do not isolate each change's individual contribution.
- GPU/display waits and shader complexity remain useful profiling leads. Swap wait alone does not separate GPU execution, driver work, scheduling, and presentation backpressure.
- The [earlier findings](perf-findings-oneplus-6t.md) and [HTML report](perf-findings-oneplus-6t.md) concern a OnePlus 6T on Android 11. Their timings must not be mixed with this OnePlus 6 comparison. The earlier GPU-only diagnosis is stronger than the current evidence warrants.

## 2. Measurement contract

Before evaluating another optimization:

1. Record source revision, uncommitted patch identity, framework revision, APK hash, actual graphics backend, device/driver version, resolution, refresh rate, battery temperature, and power state.
2. Use release builds, identical app data and starting screens, and the same gesture duration and coordinates for each OctoSense variant. Preserve app data when installing candidates.
3. Collect at least **five warm repetitions per scenario per build** and **three fresh-process first-use repetitions**. Alternate baseline and candidate blocks to expose thermal/order effects. Repeat native reference runs under comparable conditions.
4. Record fps, p50, p95, maximum interval, frame count, and missed-refresh distribution for every run. Show ranges and failing runs, not just an aggregate or best result.
5. Preserve raw presentation timestamps. Reject disconnected, empty, or wrong-layer runs and record why they were rejected. USB failures are not app performance failures.
6. Separate first-response latency from animation pacing. Add input and first-changed-frame markers using available tracing/instrumentation. Until then, label the current SurfaceFlinger activity boundaries as inferred.
7. Keep profiler overlays, screenshot requests, image transfers, and unrelated device work out of timing runs. Use separate diagnostic runs for CPU/GPU traces.

The sampler still provides an inferred span, but the newer `phone.frames` and `phone.input` markers join changed shell-state frames to SurfaceFlinger submissions and exclude work before input. Those markers define active pacing more reliably. The first-state-response proxy is not a pixel readback, and no matched native touch-to-pixel comparison exists; do not certify responsiveness parity from it.

**Review correction, 15 September 2026:** the old first-use sample's 234 ms gap follows the pull's active frames, rather than appearing on its first frame. A newer optional `phone.frames` trace distinguishes recorded animation scenes from idle clock redraws. On one traced opening, inferred boundaries reported 49.1 fps while marked activity reported 58.6 fps. The trace's final-frame flag also needed correction so later idle redraws were not left marked active. Preserve the old raw data, but do not attribute its post-pull gap to shader compilation. Scene markers measure activity, not input-to-first-changed-pixel latency; apply the same instrumentation to baseline and candidate before comparing marked results.

## 3. Prioritized implementation sequence

### P0 — Finish consistent shade performance

**Implemented candidates:** fixed-phase wallpaper texture caching; Home scene/blur reuse during shade movement; a specialized fixed-level bicubic shader for the flat sheet. The material keeps tint, shadows, rim, and dither, while removing liquid-lens calculations and edge-dependent blur. The edge appearance therefore needs comparison, not an assumption of pixel identity.

**Current follow-up work:**

- Evaluate shade shader prewarming with the existing glyph warmup if first-interaction traces establish a compilation cost. Verify that the driver actually compiles the variant before interaction; a transparent off-screen draw alone is not proof. The old 234 ms post-pull gap does not establish that cost.
- Refresh the retained Home backdrop on fully-open idle shade frames so closing can reuse a fresh capture without rebuilding its blur on the first moving frame; the current one-cache patch does this, but the exact first-frame benefit still needs a pass trace.
- Check cache invalidation for geometry, safe areas, DPI, appearance, navigation, and underlying content. Limit freezing to the intended transition; live content must refresh after settling.
- Keep prewarming bounded and on demand. Measure startup and idle behavior so removing an interaction stall does not merely move an unacceptable stall elsewhere.
- Compare the simplest passing combination. The extra full-resolution preblur/wallpaper frames in the three-cache ablation showed no repeatable advantage; the final shade candidate retains one backdrop frame and its existing mips.
- Keep the frame-loop fix that re-arms while the shade owns a finger. Its six warm and five fresh-process closing samples pass, and a static hold plus post-release idle check found no continuous idle repaint.

**Exit evidence:** repeated first-use and warm opening/closing measurements; light/dark visual checks; no empty, stale, inverted, or incorrectly clipped captures; no startup or idle regression.

### P1 — Bring Recents and other shell transitions within budget

- Validate the flat overview material in each Recents state. It is in the final shade candidate, but empty Recents → Home still has three early missed refreshes and failed all five input-aware return samples.
- Measure Recents from Home, the drawer, and a foreground app separately. Include empty and populated Recents, card scrolling, dismissal, and return to Home.
- Retain settled app captures during card transforms. Refresh them when content or viewport changes; avoid re-recording a hosted app and its own blur merely because its containing card moves.
- Inspect remaining full-screen copies, redundant fills, and unnecessary blur levels. Remove work only where ordering, sampling footprint, and opacity make it redundant.
- Page swipes and App → Home now pass their valid run blocks. Repeat Group open/close, Island expansion, and their reverse transitions after targeted changes. Extend flat-material specialization only to surfaces whose intended appearance is flat; the rounded Group variant changed translucency and still failed its gate.
- The first final-glass Recents routing retained correct pixels but did not pass return pacing. An overview-cache prototype darkened the background almost to black and was reverted. A flat group shader and final-glass group routing each improved some fps but still failed; their group material also changed translucency. Revisit these paths only with a correct capture and paired pass/drop counts.
- The final AppCard opening still pays a 67–84 ms interval on its settled full-screen capture; App → Home passes 5/5 valid returns. Deferring a clean capture made AppCard opening pass but caused App → Home and populated Recents return regressions. Preserve the refresh until a paired timing and content-freshness experiment passes both directions.

**Exit evidence:** each scenario passes independently, with representative app content and no visual/input regressions. A passing empty Recents run does not cover populated Recents.

### P2 — Remove first-use and app-opening stalls

- Capture timelines for shader/pipeline creation, glyph rasterization, icon tessellation, font decompression, script initialization, resource uploads, and first app capture. Optimize the costs actually observed.
- Reuse compiled shaders and immutable drawing resources where supported. Prewarm only resources expected on the next interaction.
- Evaluate uncompressed packaging or other font-loading changes only if decompression is confirmed as a significant first-open cost. Preserve required language coverage.
- Move suitable preparation off the UI thread through existing worker facilities; publish ready resources without synchronous waits on the render thread.
- Measure both immediate interaction after launch and interaction after the warmup window. Report time to a responsive Home and cold AppCard launch separately from steady animation fps.

**Exit evidence:** repeatable first-use stalls are eliminated or explicitly remain blockers. A good p95 cannot hide a long pause inside a verified interaction interval.

### P3 — Evaluate Vulkan after establishing the optimized GL reference

Vulkan can reduce CPU-side driver overhead, especially with many draw calls. It does not automatically reduce the shader's pixel work or texture traffic. Android also recommends evaluating performance and driver reliability on older devices. [Android Vulkan guidance](https://developer.android.com/games/develop/vulkan/native-engine-support)

**Concrete backend finding:** the retained OctoSense APK uses OpenGL ES; a separate release APK was verified as Vulkan on the same phone. The pinned Makepad GL renderer polls its completion fence before collecting retired allocations (`platform/src/os/linux/opengl.rs`, around line 389), addressing the earlier idle repaint bug. The same Makepad revision `73dfc62a8358c7a6cce33e50a612ced28e5c7411` retains CPU fence waits before and after Vulkan off-screen submissions in `platform/src/os/linux/vulkan.rs` (around lines 3557 and 3946), and an Android wait for the previous in-flight Vulkan submission in the main draw path (around line 2914). The off-screen path can serialize a multi-pass blur pipeline. The unchanged Vulkan backend was measured at only 21–22 fps opening and 37–38 fps closing the shade in valid warm runs, but the wait cost itself was not measured. Attribute the slowdown only after a paired CPU/GPU timeline.

Sequence:

1. Device-support check completed: Adreno 630 reports Vulkan 1.1.128, `VK_KHR_swapchain`, and Android baseline 2021/2022 profile support. Its driver is a 2021 build; the Android 15 minimum profile was not reported supported. Keep a supported-device check for any additional phones.
2. Unchanged-backend baseline completed: release Vulkan was active (`createdVulkanDevice=1`, `createdVulkanSwapchain=1`) and the notification shade rendered after warmup, but all usable warm shade runs missed the gate. Cold/startup spans and full-shell visual equivalence remain unresolved.
3. Trace CPU submission, GPU execution, and waits. If per-pass waits are material, batch dependent passes or order submissions with GPU synchronization. Preserve image-layout transitions and resource lifetimes; do not simply delete fence waits.
4. Use bounded per-frame resources so CPU preparation and GPU work can overlap where safe. Retain necessary waits for resource reuse, CPU readback, resizing, and teardown. Measure input latency as well as throughput.
5. Reduce render-pass boundaries and attachment load/store traffic where the dependency graph permits. These boundaries can be expensive on mobile GPUs. [Android Vulkan design guidelines](https://developer.android.com/ndk/guides/graphics/design-notes)
6. Compare optimized GL, unchanged Vulkan, and improved Vulkan with identical visuals and the same benchmark suite. Include cold pipeline creation, memory use, idle activity, resize/resume, and sustained performance.

**Adoption gate:** the unchanged Vulkan backend is rejected as a full-app switch on the measured OnePlus 6 because it regressed the now-passing shade. A synchronization change would need a paired wait/GPU trace, visual parity and repeated same-device frame runs before reconsideration. GL remains the measured reference.

## 4. Validation matrix and completion gates

| Area | Required coverage |
|---|---|
| Shade | First-use/warm opening and closing; notifications/controls; drag reversal; holds; light/dark; Home and foreground app backgrounds |
| Recents | From Home/drawer/app; empty/populated; scroll/dismiss; return to Home |
| Home | Page/Glance swipes both directions; groups; island; idle behavior |
| Hosted app | Cold/warm open; app-to-Home; capture updates; keyboard and app-owned gestures |
| Lifecycle and graphics | Rotation, safe-area/size changes, resume, cache invalidation, blur orientation, text sharpness, transparency and clipping |
| Resource behavior | Startup cost, steady idle CPU/presents, peak texture memory, and repeated-transition memory growth |

Completion requires:

- Every scoped animation scenario meets **≥55 fps and p95 ≤20 ms** across the repeated run set. Report cold/first-use results separately and retain their full stall intervals.
- No unresolved repeatable long stall is hidden by percentiles or averaging. Establish input-to-first-changed-frame latency against the same-device native reference for comparable actions before claiming responsiveness parity.
- Visual and interaction checks pass on the real Android GPU. Metal checks support cross-platform correctness but do not replace Android evidence.
- Existing relevant tests and release builds run on the final source. The full suite's existing catalog-path/Route binary-name failures remain disclosed until separately resolved; do not weaken tests to obtain a pass.
- Idle behavior does not return to continuous repainting, and memory stabilizes after repeated navigation.
- Final results identify the exact build/patch, device, conditions, raw measurements, remaining limitations, and which scenarios actually pass.

## 5. Deliverables

1. A small, reviewable rendering patch with measured reasons for each retained optimization.
2. Reproducible baseline/candidate/native runs and a per-scenario results table, including first-use failures.
3. A Vulkan decision record covering the unchanged backend and any synchronization improvements, if that experiment is pursued.
4. A dated update to the findings reports once the final source is validated, keeping the OnePlus 6 and OnePlus 6T evidence distinct.

Implementation locations: [phone composition and caches](../../OctoSense-native-perf/src/desk/phone.rs), [phone materials and warmup](../../OctoSense-native-perf/src/mobile_surface.rs), [shade drawing](../../OctoSense-native-perf/src/mobile_shade.rs), and [frame measurement](../../OctoSense-native-perf/scripts/measure_android_frames.py).
