# Native frame budget work — 15 Sep 2026

Status: phone benchmarks in progress. Native parity across the shell is NOT demonstrated. See the latest session record below.

## Source and builds

- Worktree: `~/home/OctoSense-native-perf`
- Branch: `perf/native-frame-budget`, based on PR #28 commit `efc153cb6caf63759334a0810adbb35f3e2c376f`.
- Framework remains pinned to makepad `73dfc62a8358c7a6cce33e50a612ced28e5c7411`.
- `baseline-efc153c.apk`: release APK built from the unchanged base. SHA-256 `968d7c945ee8b94d7ad276a134682b272b453c31ec662aae6ea469609235d8bd`.
- `wallpaper-cache.apk`: candidate release APK. SHA-256 `3c5789440f100babad0c44bc838af8e03353986085a083d3647fdbe7f6c7384c`.
- Both bundle the existing phone kernel. Neither APK was installed during this task.

## Changes

The procedural wallpaper renders into a reusable full-resolution texture. Navigation no longer advances its decorative drift. Size, position, DPI, style, dark mode, and phase invalidate the cache. Its producer dependency is renewed without recording or painting the wallpaper again during normal navigation.

`scripts/measure_android_frames.py` records raw SurfaceFlinger timestamps, handles Android 11/15 layer listings, reports both legacy burst statistics and an inferred activity span, and retains long stalls inside that span. It rejects disconnected/no-data runs. Inferred boundaries do not measure touch-to-display latency and are not sufficient by themselves to certify native parity.

## Verification

- Android release build passed; existing packaging warnings include missing launcher icons. No framework pins or buildtool were changed.
- macOS release `mobile-only` build passed on the actual Metal backend.
- 61 mobile-focused unit tests passed; the existing dock-warp test passed.
- Measurement parser checks passed: sentinel/duplicate frames, Android 15 wrappers, an interior 500 ms stall retained, idle-only samples rejected as insufficient activity.
- Native Metal runtime: Home, shade pull, shade close, controls, and dark-mode change inspected. The cache re-recorded when appearance changed, and its pixels remained correct.
- With `gpu.pass` tracing enabled, one shade pull had 36 main-window paints and zero wallpaper paints between input start and settle, before any screenshot request. `/g` explicitly repaints child passes and its capture timings are NOT animation frame timings.
- Full unit suite has an unrelated catalog failure: `clock points at the wrong package` when default catalog paths resolve to the existing `~/home/makepad` tree. An isolated catalog pointing at the pinned fork then exposes the existing Route binary-name mismatch (`makepad-app-route` vs `route`). No catalog source or tests were changed to hide these failures.
- All owned desktop test instances were closed.

## Phone status and remaining work

OnePlus 6 `cfb7c9e3`, Android 15, 1080×2280, density 450, battery 100%, 27°C when briefly accessible. Original screen timeout 1800000 ms and stay-awake setting 15. No phone settings were modified.

The phone became authorized long enough to inspect its configuration, then disconnected and returned as unauthorized. Repeated subsequent checks remained unauthorized. A second request to accept USB debugging with “Always allow from this computer” is pending.

Once accessible: install baseline, calibrate single-gesture coordinates on this phone, collect repeated independent shade/page/Recents/app-open/group/island runs with monitor off, capture native SystemUI reference, install candidate, repeat, and profile remaining misses. Keep first-response latency and long stalls separate from idle gaps. Require ≥55 fps and p95 ≤20 ms before claiming the target is met; compare only measurements from this same phone. Optimize remaining glass and first-frame work based on those measurements.

## Latest session: OnePlus 6 connected, measured iterations

Authorization now survives USB reconnects. Large device-to-host transfers fail with USB read errors (native and libusb backends); host ADB restored to native backend. Small chunked base64 transfers work and PNG CRCs are verified by `target/perf-artifacts/adb_capture.py`. No phone settings changed. Baseline and each candidate were installed using `adb install -r` and fresh launched. Frame monitor was off for all JSON benchmark files; separate profiler runs are not benchmarks.

The Android 15 parser was corrected using actual output: the leading hex token inside RequestedLayerState is PART OF the surface name. Native NotificationShade destroys/recreates its buffer surface, so `--layer-match` resolves it during sampling.

All following are this phone, 1080x2280, 60 Hz, slow 800 ms single gestures, raw JSON under `target/perf-artifacts/op6-*`. Native shade starts at y20, OctoSense shade at y130 (below native status bar).

- PR28 baseline shade opens: 29.3, 31.6, 30.5 fps; p95 50.2, 33.5, 50.2 ms. Closes: 49.2, 49.5, 48.2 fps; p95 33.5 ms. Battery temperature 28.4 C after the set.
- Wallpaper cache alone: warm opens 31.0, 30.4 fps; no convincing gain. First open retained an interior 1054 ms gap, not hidden from the result.
- Native Android shade opens: 58.5, 58.5, 57.9 fps, p95 16.9 ms or better; closes 59.8 fps, p95 16.9 ms or better. `op6-native/`.
- Added Home scene + Gaussian snapshot cache during shade movement. Metal trace confirms one scene/pyramid capture followed by only main-window paints. GL still missed deadlines with the generic glass shader (`op6-shade-cache/`).
- Added specialized flat shade shader: fixed level-3 bicubic blur; retains tint, rim, inner/outer shadow and static dither. Removes liquid-lens edge blur/ripple/refraction work unused by the flat sheet. One temporary build failed script scope lookup; fixed with qualified mod.widgets.PhoneShadeGlass. Failed runs produced no benchmark file.
- Flat shader + cache: warm opens 58.56 and 58.62 fps, p95 16.82 and 16.79 ms. First open 46.9 fps, max 234 ms (p95 16.87). Closes 50.5/57.9/55.1 fps; p95 33.5/16.9/33.5. Cold spike and close misses remain. Metal settled shade visually checked; owned desktop instances closed. `op6-flat-shade/`.
- First page-to-Glance swipe 58.12 fps, p95 16.75 ms. Other shell transitions still pending.

Current next candidate (building, not measured): prewarm the sheet shader with the existing off-screen glyph warmup, and refresh the cached Home capture during fully-open idle shade frames so closing reuses a fresh capture immediately. APK will be saved as warm-shade.apk. Existing broad unit suite must be rerun after final source changes.

Latest source also applies the flat material to Recents (uniform animated blur), and prewarms both shader variants. This revision is currently building, not yet measured. Page return from Glance: 58.10 fps, p95 16.81 ms. Recents hold with no open apps: 47.79 fps, p95 33.49 ms, max83.68 ms with the previous generic overview shader; screenshot confirms correct Recents state. Source uses internal Home band y2180 on OP6 (y2260 would hit native Android navigation).

## Final local validation block — 15 Sep 2026, 20:37 UTC

**Status:** shade pacing is much closer to the same-device native UI; full-shell native parity is **not achieved**. This block supersedes the interim build and authorization notes above. The OnePlus 6 was authorized during the valid captures. ADB USB transfer failures were rejected and never counted as frame misses. No device settings were left changed, and the original light appearance was restored after the dark-style screenshots.

The final APK remains installed. Five owned temporary trace files under `/data/local/tmp` (`octosense-perf-atrace*`, `octosense-perf-trace-first.txt`, `octosense-perf-shade-close.trace*`) were removed after their local captures and CRC/checks; an ADB listing confirmed they no longer exist. The local raw JSONs, screenshots, build logs, source patch, and trace captures remain under this artifact directory.

### Exact candidate and environment

- Source: branch `perf/native-frame-budget`, PR #28 base `efc153cb6caf63759334a0810adbb35f3e2c376f`, pinned Makepad `73dfc62a8358c7a6cce33e50a612ced28e5c7411`. The seven tracked source files remain an uncommitted diff; the new `scripts/measure_android_frames.py` is untracked. `pointer-loop-final-source.patch` is the exact `git diff --binary` for the tracked changes, SHA-256 `8532e093f6173ec97e7b7a64206fceca59e5ceaefd0d14989eae9bd9b44621ae`. Include the untracked measurement script separately when reviewing or committing.
- Release `op6-pointer-loop-final.apk`, SHA-256 `d355ef1dc999e3e99c3ec9c6a409af5a95c2fa1606275a35922c04b372c32252`, installed for all `op6-pointer-loop-final-*` runs. Android and macOS Metal release builds passed; logs are `pointer-loop-final-android-build.log` and `pointer-loop-final-metal-build.log`.
- OnePlus 6 `cfb7c9e3`, Android 15, 1080×2280, density 450, 60 Hz, Snapdragon 845/Adreno 630, release OpenGL ES. Battery was 100% on AC; temperature was about 30°C. Frame monitor off. Same source candidate in an isolated build was measured for the earlier `op6-pointer-loop-*` blocks; the final main APK was rebuilt after a comment-only source correction, so retain the blocks' separate identities.
- Mobile tests: `cargo test --locked --offline --features mobile-only mobile -- --test-threads=1` passed 61/61 on the final source. The full suite's pre-existing catalog-path/Route binary-name failures described above remain. `git diff --check` passed.

### Valid raw run blocks and target gate

Each named JSON directory below retains SurfaceFlinger timestamps and, for OctoSense, `phone.frames` and `phone.input` joins. The gate is **at least 55 fps and p95 no more than 20 ms in each run**. The fresh-process runs wait six seconds after each `am start -S`; they are not immediate-launch or fresh-install measurements.

| Interaction / raw directory | Gate passing | Main finding |
|---|---:|---|
| Final warm shade open / close, `op6-pointer-loop-final-shade-warm` | **6/6 / 6/6** | openings 57.36–58.56 fps, closes 57.91–57.97; all p95 ≤16.83 ms. Each close still skipped two early refreshes, with no later mid-span misses. |
| Final fresh-process shade open / close, `op6-pointer-loop-final-shade-fresh` | **4/5 / 5/5** | one opening p95 ≈33.49 ms; all closes ~57.91–57.96 fps and p95 ≤16.78 ms. |
| Earlier isolated same-code shade warm / fresh, `op6-pointer-loop-shade-warm`, `op6-pointer-loop-shade-fresh` | 5/6 / 6/6 warm; **5/5 / 5/5** fresh | These are separate sample blocks, not evidence that the final APK passes every first use. |
| Home → Glance / return, `op6-pointer-loop-pages-warm` | **6/6 / 4/4** | ~57.3–58.9 fps, p95 ≤16.9 ms; two return USB captures rejected. |
| Home → Recents hold / empty Recents → Home, `op6-pointer-loop-recents-warm` | 4/5 / **0/5** | entry has multi-process input latency of 50–100 ms; valid 250 ms return misses early refreshes with p95 ≈33.5 ms. |
| Group open / close, `op6-pointer-loop-final-group-warm` | **0/6 / 0/5** | openings 38.85–44.04 fps; closes 44.80–50.33; one close USB capture rejected. Initial open present gaps repeatedly ~50, 33, 50 ms. |
| AppCard open / App → Home, `op6-pointer-loop-final-app-warm` | **0/6 / 5/5** | openings 46.77–51.96 fps, with a 67–84 ms last-active settle/capture gap; returns ~59.74–59.76 fps. |
| Populated Home → Recents / return, same AppCard block | 2/5 / 1/5 | most returns have p95 ≈33.5 ms. |
| Native SystemUI shade close, `op6-native-refresh-final` | **5/5** | ~59.75 fps, p95 ≤16.77 ms, no skipped refreshes. Native inferred openings passed 3/5; one had a 167 ms first interval without a native input marker. |
| Island triple-tap demo, `op6-pointer-loop-final-island-fresh` | 2/5 | five valid 21-present fresh-process spans; three one-skip failures at frame #1 twice and #4 once, p95 ≈33.5 ms among 20 intervals. |

The old PR #28 baseline on this phone opened the shade at ~29–33 fps and closed at ~48–50 fps. Do not combine these OnePlus 6 measurements with the historic OnePlus 6T/Android 11 findings. Native and OctoSense shade gestures use different starting coordinates; the native inferred span has no matched touch-to-pixel boundary. The target here measures animation frame pacing, not full input responsiveness parity.

### Correctness, idle, and rejected experiments

- Final Android screenshots of notification and control sheets in light and dark appearance had correct tint, blur, borders, labels and clipping; the original light appearance was restored (`op6-final-controls-light-restored.png`). Home idle presented five times in five seconds both before and after a held static shade; the static shade itself presented 118 frames in two seconds. This checks the earlier GL retired-allocation idle repaint regression and the shade frame-loop re-arm. The pinned GL completion fence is polled before retirement (`platform/src/os/linux/opengl.rs:389-395`).
- The retained one-backdrop shade cache and fixed-level flat shaders reduced repeated moving-sheet work. The frame-loop fix re-arms vsync while `phone.gesture.is_some()` even after the sheet cancels the general recognizer. An atrace before the fix showed callbacks and MOVE events during seven 28–33 ms shade-close queue gaps with no app submission; after the fix six warm closes had no later mid-span skips. The trace is diagnostic and its failed paired SurfaceFlinger transfer is not a benchmark.
- Visually correct final-glass overview routing failed empty Recents return in 0/5; an overview full-resolution cache made the screenshot almost black and failed 0/6. A flat rounded Group material made the panel more opaque and failed 0/6 opens; final-glass Group routing was still 0/6 opens and 0/3 valid closes. Neither was retained.
- A clean AppCard settled-capture skip made opening 3/5 but App → Home 0/5. A dirty-only settled capture made opening 6/6 but App → Home 0/6 and populated Recents return 0/5. Both were rejected; current source still refreshes the settled capture to preserve content freshness and reverse-transition pacing.
- Separate read-only GPU-clock probes started at 257 MHz on both retained and settled-capture builds; the retained diagnostic reached 710 MHz by ~0.19 s, the ablation by ~0.33 s. These are two swipes sampled about every 60 ms, not causal GPU-time proof. The phone rejected a write to the advertised `performance` governor and still reported its original `msm-adreno-tz` policy afterward. No pinned-clock performance run exists. A same-process `dumpsys meminfo` spot check around twelve shade open/close pairs reported Graphics PSS 93,400 → 94,136 → 94,024 KB (before, immediately after, ten seconds settled) and total PSS 194,941 → 199,048 → 197,568 KB. Raw snapshots are `op6-final-meminfo-{before,after,settled}.txt`. This suggests no large immediate growth in that short block; it does not measure direct GPU allocation, peak use or long-session stability.

### Reproducibility and remaining work

`target/perf-artifacts/drop_analysis.py RUN_DIRECTORY` accepts a run directory relative to `target/perf-artifacts` or an absolute path, errors on a missing directory, and skips `.invalid.json` samples. `atrace_timeline.py --trace TRACE.txt.gz --run RUN.json` accepts a paired plain or gzip trace and raw run with a clock-sync marker. Both copied tools are beside the run directories, compile with `py_compile`, and replay the original Recents trace (including a gzip copy) or current AppCard runs; the gzip replay log is `recents-atrace-gzip-replay.txt`. The `run_cases.py` driver now rejects samples with fewer than ten active frame intervals as `.invalid.json`; the five old `op6-pointer-loop-final-island-warm` JSONs were relabeled this way and excluded. A manual marker log proved the triple tap animates, and the fresh-process block produced five valid short spans. Two passed; three skipped one early refresh, so Island remains a gate failure.

The [gap analysis](../../../octosense-org/docs/octosense-android-perf-gap-analysis.md) ranks the remaining source clues and verification steps. The current Vulkan off-screen path waits on CPU fences before and after each pass (`platform/src/os/linux/vulkan.rs:3557,3946`); this is a source concern, and the per-pass wait cost remains unmeasured. Next decisions need paired pass/drop traces and visually correct Recents/Group captures, AppCard capture freshness tests that pass both directions, first-use and live-tile checks, and memory/lifecycle evidence. Full native parity remains a release gate failure until the failing scenarios pass repeated same-device runs.

**Later Vulkan probe, 21:37 UTC:** an isolated release `MAKEPAD=vulkan` build of the same optimized UI created a Vulkan device/swapchain on the OnePlus 6. Its usable warm shade runs were 21.27–22.02 fps opening (0/4 passing) and 36.78–38.28 fps closing (0/5 passing), so the unchanged backend was rejected as a full-app replacement. The pinned per-pass CPU wait cost remains unmeasured. The [separate probe record](../../../OctoSense-frame-baseline/target/perf-artifacts/vulkan-probe-validation.md) has raw runs, driver data, APK hash, invalid first-use samples and visual caveats. The final GLES APK was reinstalled and the light Home screenshot was confirmed.

## Scene-cache candidate — 15 Sep 2026, 22:00–23:00 UTC

**Status:** `scene-cache-v3` passes Group open/close 6/6 and Recents → Home 2/3 on warm blocks; shade and pages unchanged. Regression blocks below. Source is still the uncommitted diff on `perf/native-frame-budget` (now 8 tracked files plus the untracked measurement script); `scene-cache-v3-source.patch` (SHA-256 `c5e12d36704b9e6ce5f0a6634ff7f90fd1c44ae27d6e0303a7e3bcf805cde9db`) is the exact `git diff --binary`, `scene-cache-v3.apk` (`7a4b4cb4c3d563818dc84392234670a1b85845907419688df2019a236ca12ab6`) the release APK measured, built with the same cargo-makepad and pinned makepad `73dfc62`. v1 (`scene-cache*.{patch,apk}`) and v2 (`scene-cache-v2*`) are kept for the record; see the gap analysis §11.3 for what each changed. 61/61 mobile tests pass on the v3 source; the pre-existing full-suite catalog/Route failures are unchanged.

### New measurements this block

- **kgsl GPU traces** (`kgsl-*.txt` + `.markers`, `kgsl_gpu_timeline.py`, `kgsl_frames_summary.py`): per-submission GPU execution time, clock level and CPU waits, joined to `phone.frames`/`phone.input`. `kgsl-gl-recents-home` is a home swipe, not a return (its markers show `screen=Home overview=0`; the preceding hold was lost to a USB drop). `kgsl-gl-shade-open/close` were captured on the Vulkan probe (failed GL reinstall) and duplicate `kgsl-vk-shade-*`. `kgsl-gl2-shade-*` are the verified GLES final APK; `kgsl-gl2-shade-close` has no markers. `kgsl-scene-cache-*` are v1, `kgsl-scene-cache-v2-*` v2 (the group-close and shade traces there were cut by USB drops), `kgsl-scene-cache-v3-*` v3.
- **GPU clock A/B** (`op6-gpu-pin710-{recents,group}-warm`, `op6-gpu-ctrl257-*`): Adreno devfreq `min_freq` written to 710000000 via `su` for the pinned block, read back at 710 MHz, restored to 257000000 afterwards and confirmed; no other phone setting changed. One control Recents run rejected over USB.
- **Screenshots** via `adb_capture.py` (chunked, CRC-checked): `op6-scene-cache-v2-{home,group,recents,shade}.png`, `op6-scene-cache-v3-*.png`. The earlier `op6-scene-cache-*.png` and `op6-gl-restored-after-kgsl.png` are 62-byte failed `exec-out` captures, not images.
- Vulkan probe traces and their reading: `../../OctoSense-frame-baseline/target/perf-artifacts/vulkan-probe-validation.md`.

### Warm blocks, v3 (`op6-scene-cache-v3-*-warm`, unpinned clocks, valid runs)

| Interaction | Gate passing | fps / p95 / missed refreshes |
|---|---:|---|
| Recents → Home (250 ms) | **2/3** | 59.80 / 16.9 ms / 0; 59.77 / 16.7 / 0; 56.48 / 33.4 / 2 |
| Home → Recents hold | 3/3 | 56.7–57.5 fps, p95 ≤16.8 ms (first interval 67 ms is the multi-command injector) |
| Group open | **3/3** | 59.77–59.80 fps, p95 ≤16.8 ms, 0 missed |
| Group close | **3/3** | 59.75–59.79 fps, p95 ≤16.8 ms, 0 missed |
| Shade open | 3/3 | 57.38–57.40 fps, p95 ≤16.8 ms, 2 early skips each |
| Shade close | 3/3 | 57.99–58.87 fps, p95 ≤16.8 ms, 1–2 early skips each |

Before, on the final GLES APK: Group 0/6 and 0/5, Recents → Home 0/5. Battery 100 % on AC, 30 °C.

### Regression and fresh-process blocks, v3 (`op6-scene-cache-v3-{pages,app,island}-warm`, `op6-scene-cache-v3-{shade,recents,group}-fresh`)

Fresh-process blocks restart the process before each round and wait six seconds, as before.

| Interaction | Gate passing | fps / p95 / missed | Before |
|---|---:|---|---|
| Home → Glance / return, warm | **6/6** | 57.3–58.9 fps, p95 ≤16.8 ms | 6/6, 4/4 |
| AppCard open, warm | 0/3 | 47.3–49.8 fps, p95 67–84 ms — the settled-capture gap on the last active frame, unchanged | 0/6 |
| App → Home, warm | **3/3** | 59.79–59.80 fps, p95 ≤16.8 ms, 0 missed | 5/5 |
| Populated Home → Recents, warm | 2/2 valid | 56.7 / 57.4 fps, p95 ≤16.8 ms (one run rejected by the harness) | 2/5 |
| Populated Recents → Home, warm | 0/2 | 56.6 / 54.9 fps, p95 33.4 ms — draws live (`openness` animates), unchanged | 1/5 |
| Island triple tap, warm | invalid | all three rounds recorded no active span, as the earlier warm island driver did; not scored | — |
| Shade open, fresh | 2/3 | 56.2–56.3 fps; one p95 33.4 ms | 4/5 |
| Shade close, fresh | **3/3** | 57.97–58.01 fps, p95 ≤16.8 ms | 5/5 |
| Home → Recents hold, fresh | 3/3 | 55.9–56.7 fps, p95 ≤16.9 ms | — |
| Recents → Home, fresh | **3/3** | **59.77 / 58.12 / 58.12 fps, p95 ≤16.9 ms, missed 0 / 1 / 1** | 0/5 (warm) |
| Group open, fresh | 2/2 valid | 59.78 / 59.79 fps, 0 missed (third rejected by the harness) | 0/6 (warm) |
| Group close, fresh | **2/2** | 59.80 / 57.06 fps, p95 ≤16.8 ms | 0/5 (warm) |

Graphics PSS after the blocks: 92,464 KB (93,400–94,024 KB in the earlier same-process check); total PSS 194,711 KB. Battery 30.9 °C. The kept scene frame is one full-resolution colour + depth target more than the earlier shade-only cache; no growth is visible in this short check, and long-session growth remains unmeasured.

Still failing on v3: AppCard opening (the retained settled-capture refresh), populated Recents → Home (live scene while a card zooms), and one of three fresh shade openings. The GPU floor at 257 MHz is the constraint for all of them: a passing transition needs ≤ ~4.5 ms of GPU per frame at 710 MHz.

## Sheet content, deferred app capture, fade overlay — 15 Sep 2026, 23:00–23:35 UTC

Measured APK `sheet-capture-v3.apk` (SHA-256 `8ac4a623…`, full hash beside it), source = `OctoSense-mobile` `main` (PR #1 merge `0e8534d`) plus the four commits of PR #2; `sheet-capture.apk` (`94bd23a7…`) and `sheet-capture-v2.apk` (`20d12335…`) are the two earlier iterations, each with its build log. 61/61 mobile tests pass at every step. Raw blocks: `op6-sheet-capture-v3-{app,shade,recents,group}-warm`, `op6-sheet-capture-v3-app-fresh`, and for v2 `op6-sheet-capture-v2-*` (warm, regression and fresh blocks; the `recents-empty-3` retry in the v2 warm block is a harness artefact after a USB drop — 30.8 fps with a 552 ms gap — and is not a result). kgsl traces `kgsl-sheet-capture{,-v2}-*` and `kgsl-sheet2-app-home` (the App → Home diagnosis, clock already at 710 MHz). Screenshots `op6-sheet-capture-v2-*.png`.

| Interaction | Gate passing | fps / p95 / missed |
|---|---:|---|
| AppCard open, warm + fresh | **5/5** | 59.7–59.8 fps, p95 ≤16.8 ms, 0 missed |
| App → Home, warm + fresh | 4/5 | 59.7 / 58.1 / 58.1 / 58.1 fps; one 56.6 fps, p95 33.4 ms (2 missed) |
| Populated Home → Recents | 5/5 | 56.6–57.4 fps, p95 ≤16.7 ms |
| Populated Recents → Home | **5/5** | 58.1–59.7 fps, p95 ≤16.8 ms |
| Shade open, warm | 2/3 | 56.2 / 58.5 fps; one 53.9 fps, p95 33.4 ms (5 missed) |
| Shade close, warm | 3/3 | 58.8 / 59.7 / 59.7 fps; 0 missed in two |
| Empty Recents → Home / hold, warm | 3/3 / 3/3 | 58.1 fps, p95 ≤16.8; 57.4–58.2 fps |
| Group open / close, warm | 6/6 | 57.0–59.7 fps, p95 ≤16.7 ms |
| v2 fresh: shade open / close | 3/3 / 3/3 | 57.3–58.6 / 58.8–59.7 fps |
| v2 fresh: Recents → Home / hold | 3/3 / 3/3 | 58.1–59.7 / 55.9–57.4 fps |
| v2 fresh: Group open / close | 3/3 / 3/3 | 59.7 / 57.0–59.7 fps |
| v2 warm: pages | 6/6 | 57.3–58.9 fps |

Idle check: 0 presents in 3 s with an app open (the deferred record does not loop). Cold-install first use: two attempts rejected by the harness (no input marker for the pull); unmeasured. Battery 100 % on AC, ≈31 °C. GPU `min_freq` 257 MHz throughout; no device settings changed. The phone has `sheet-capture-v3.apk` installed.

## Home role — 16 Sep 2026, 00:00 UTC

`home-role.apk` (SHA-256 `6cbab5e0…`), source = `OctoSense-mobile` `feat/android-home-role` (PR #3) on `main` after PR #2, makepad pin `a359e165c` (fork branch `feat/android-home-intent`) and the fork's `cargo_makepad` Java from `feat/android-home-intent-java` (the build tool at `bt-1b11c4a` carries it). 61/61 mobile tests pass. Script: the session's `home_role_test.sh`; screenshots `op6-home-role-*.png`. Results: OctoSense set as Home; Home from Settings → OctoSense with the intent seen by Rust (gesture and 3-button); Home with an OctoSense app open → home page (3-button always; gesture: not on the first press after selection, then yes); Home with the sheet open closes it; reboot → OctoSense on top. Device state left changed on purpose: default Home = OctoSense, navigation mode = 3-button (`navigation_mode` 0). Revert: `cmd package set-home-activity com.android.launcher3/.uioverrides.QuickstepLauncher`; `cmd overlay enable-exclusive --category com.android.internal.systemui.navbar.gestural`.

## Merged tip: Home role + home pulls + thinking octopus — 16 Sep 2026, ~01:30 UTC

`merged2.apk` (SHA-256 `3a209341…`), source = `OctoSense-mobile` `main` at the merge of PRs #3, #4 and #5 (`7f7286b`), i.e. the sheet-capture work plus the Home role, the home-page pulls and the island's octopus, makepad pin `c7346fef` (fork `main`). `cargo check` clean, 64/64 mobile tests pass. On the phone: draws after a force-stop and a Home press, one `ActivityRecord` (the `singleInstance` fix holds), shade pull and close by hand. Two-round warm blocks `op6-merged4-{shade,app,group}-warm`: shade open 56.1 / 57.3 fps, close 59.7 fps; AppCard open 56.7 fps, App → Home 59.7 / 56.3 fps, populated Recents → App 55.1 / 56.2 fps, Recents → Home 54.7 fps; Group open 57.0 fps, close 54.2 fps — the same range as the `sheet-capture-v3` block, with GPU `min_freq` 257 MHz (not pinned) and the DVFS ramp's early misses still in place. Five runs discarded for adb `CalledProcessError`/lost markers (USB drops), not app faults.

Bench caveat once OctoSense is the Home app: `am start -S … --es makepad.TRACE phone.frames` no longer enables the markers — the force-stop makes the system relaunch the Home app at once, and the traced intent is "delivered to currently running top-most instance", where the extra is not read. Every run is then discarded with "No pre-action phone.frames marker". Hand Home back to `com.android.launcher3/.uioverrides.QuickstepLauncher` (`cmd package set-home-activity …`) for the bench and restore OctoSense afterwards; the `op6-merged2-*`/`op6-merged3-*` blocks are this failure, not results. The phone has `merged2.apk` installed, OctoSense as Home, 3-button navigation.
