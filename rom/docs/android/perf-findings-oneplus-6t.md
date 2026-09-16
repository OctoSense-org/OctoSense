# OctoSense mobile shell: frame budget findings

OnePlus 6T (Snapdragon 845, Adreno 630, 1080×2340 at 60 Hz, Android 11, rooted) · measured 14–15 Sep 2026
Published report: https://claude.ai/code/artifact/a9523197-2aa6-4164-8372-9e8251475786
Full tables: `docs/perf-mobile-shell.md` on OctoSense branch `perf/transition-main-thread` (PR #28)

## Review update — 15 September 2026

The tables below retain the original **OnePlus 6T / Android 11** measurements. Current **OnePlus 6 / Android 15** experiments are separate: baseline shade opening was about 29–32 fps. On the final GLES candidate, six warm shade opening/closing pairs pass the 55 fps / p95 ≤20 ms pacing gate; five fresh-process closes and four of five fresh-process openings pass. Native SystemUI shade closes ran about 59.75 fps with no missed refresh in five valid same-phone runs, while the candidate closes ran about 57.9 fps with two early skips per warm run. Page swipes pass; Recents return, group windows and AppCard opening still miss, and a fresh-process Island demo passed 2/5 short spans. An unchanged-backend Vulkan probe on this phone rendered the warm shade but opened it at 21–22 fps and closed at 37–38 fps; it is rejected as a full-app switch. Native parity across the shell is not established; see the [Android performance plan](performance-plan.md) and [gap analysis](perf-gap-analysis.md) for the current run matrix and limits.

The old burst filter can hide long active stalls or include short idle gaps. In the newer OnePlus 6 trials, a 234 ms gap previously called a first-use stall occurs after the pull and does not establish shader-compilation cost. Input-to-first-changed-frame latency remains unverified.

## Original OnePlus 6T verdict

- **Idle screens are fixed.** An idle screen presents about 1 frame a second, down from 56. Idle Home CPU fell from 78% to 3–8%.
- **Transitions are faster but not native.** The shade went from 34 to 45 fps. No transition reaches the 55 fps target yet.
- **Full-screen glass is a major profiling lead.** Removing glass, dim and content reached 60 fps in one variant. These measurements do not isolate GPU execution from driver, scheduling, and presentation costs.

## Idle screens

Before the fixes, two causes stacked. The shell kept its own frame loop running. makepad's Android renderer then repainted the whole window at display rate whatever the shell did.

| Idle screen | Presents/s, shell fixed (PR #26) | Presents/s, renderer fixed (PR #27) |
|---|---|---|
| Home | 56.2 | 1.0 |
| Hosted AppCard screen | 18.6 | 1.0 |
| Recents | 33.7 | 1.2 |
| Group window | 44.1 | 1.0 |

| CPU while idle | Before | Shell fixes, PR #26 | Renderer and clock fixes, PR #27 |
|---|---|---|---|
| Home | 78% | 43% | 3–8% |
| Hosted AppCard screen | 41% | 34% | 3% |

The remaining presents are the status clock's one-second tick.

## Transitions

Burst fps over two fresh launches, frame monitor off. A range means the two runs differed. Current validation target: at least 55 fps and p95 frame intervals at or below 20 ms; these historical burst averages alone cannot establish a pass.

| Transition | Before (PR #27 build) | After (PR #28 build) |
|---|---|---|
| Shade pull and close | 33.7–33.9 | 45.1–45.7 |
| Recents to Home | 30.8–41.5 | 41.5–45.1 |
| Recents hold | 34.5–34.9 | 36.3–38.8 |
| Swipe up home | 42.1–43.7 | 41.3–45.6 |
| Page swipe | 35.1–41.8 | 37.1–47.0 |
| Open app | 30.1–41.4 | 33.4–40.1 |
| Island demo | 36.7–49.0 | 40.7–42.7 |
| Group open | 52.4–53.6 | 52.3–53.9 |
| Group close | 36.1–48.1 | 48.1 |

- **Shade:** frames over 33 ms dropped from 8–9 to 2–3 per burst. The first frame fell from 67–83 ms to 33–50 ms.
- **Page swipe, island, open app:** within run-to-run noise. Their first frames are still 133–250 ms.
- **Group open** is closest to native, at 52–54 fps with p95 33 ms.

## Causes found and fixed

Every cause was confirmed with measurements before it was fixed: SurfaceFlinger present times, makepad's frame monitor, `simpleperf` CPU samples, an `atrace` scheduling trace, and on/off switches for individual layers.

### makepad renderer, Android GL (fork PR #6)

- **Unpolled completion fence.** The GL path never polled its GPU completion fence. Freed allocations stayed pending forever, so every render requested another full repaint (`platform/src/os/linux/opengl.rs:409` at ad8f372).
- **Upside-down glass.** The frosted backdrop was sampled upside down on Android. The V flip is now 0 on every backend.

### makepad platform and widgets (fork PR #7)

- **Icon tessellation.** App icons were re-tessellated from SVG every frame. That was 11–27% of render-thread time and 54% of the heaviest Recents frames. It is now about 1%.
- **Wakes and signals.** A worker's UI wake painted between vsyncs, and freeing draw storage raised a signal every frame.
- **Render-thread priority.** The render thread ran at normal priority and spent 21–28% of its time on the slow cores. It now runs at nice −10, pinned to cores 4–7, set after Startup.
- **Blur mips.** The blur read two mip levels where one suffices at whole-number levels, and built an extra level.
- **Lookup probes.** Script lookups that expect a miss built "did you mean" suggestion lists.

### Phone shell (OctoSense PRs #26, #27, #28)

- **Wallpaper loop.** The frame loop re-armed every frame only to drift the wallpaper.
- **App list.** It re-read settings files and checked every app binary several times per frame.
- **Compositor and caches.** Every frame ran the full-screen blur compositor, plus two desktop caches the phone never shows.
- **Status clock.** It forked `date` twice a second, costing 11% of a core.
- **App open.** The opening app re-recorded its full-screen capture on every animation frame.
- **Shade layers.** The shade stacked a dim, a glass layer and a separate tint over the whole screen. It is now one glass layer, dimmed only below the sheet, with glyphs warmed before the first pull.
- **Android drawer.** No wallpaper and no full-screen fill are drawn under it.

### Hosted AppCard (OctoSense PRs #27, #28 · fork PR #7)

- **Glass scope.** Its glass requested a blur of the whole window instead of its own capture.
- **Per-frame script.** A worker signal every frame and a 125 Hz run-view timer each re-ran a script, taking 22–35% of render-thread time. That is now 5–12%.

## What is left

### GPU fill in the frosted glass

The shade's measured CPU encode stays at 4–5.5 ms per frame in these variants. Layer changes affect swap wait, but swap wait alone cannot distinguish GPU execution, driver overhead, scheduling, or presentation backpressure. Fps was measured with the frame monitor off; the swap wait comes from runs with it on.

| Shade variant | fps | Swap wait, mean / worst |
|---|---|---|
| Everything drawn, before the shade fixes | 34–42 | 7–21 / 57–85 ms |
| No sheet glass | 39–50 | 4–6 / 16–34 ms |
| No compositor at all | 44–46 | 4–20 / 31–64 ms |
| No dim, glass or content | 34 and 60.1 | 1.5–3.7 / 8–11 ms |
| After the shade fixes, everything drawn | 45–46 | 4.9–16 / 36–47 ms |

Recents shows the same pattern. Over Home it already runs at 52–56 fps. Over the Android drawer it ran at 38–40 fps, and removing the overview glass brings it to 57–58.

**Next experiments.** Specialize flat materials, reuse unchanged backgrounds during transitions, and remove redundant copies before reducing visual quality. Lower-resolution blur or a different moving-state material remain options if measurements justify them. Compare appearance and memory cost on the device.

### First-frame spikes

- **First AppCard open.** It spends 213 ms in AppCard's script VM. It then spends 190 ms decompressing a font asset on the render thread, likely the 19 MB CJK face. Fix by storing fonts uncompressed in the APK, or by loading them off the main thread.
- **Page swipe and island.** First frames of 150–250 ms, not profiled yet.
- **Repeated PNG decode.** A 136×128 PNG is decoded about every 50 ms during a streaming card turn. It runs on AppCard's own background thread, so the fix belongs in Octoscript-AppCard.

### Not yet verified on a phone

- **Drawer fill fix.** Measured only through the on/off switch: 38–40 → 48–49 fps.
- **Glyph warm-up and lookup probe fix.** Not measured on a device yet.

## Would Vulkan help?

The unchanged Vulkan backend was measured on the OnePlus 6 and was substantially slower for the shade: 21–22 fps opening and 37–38 fps closing in usable warm runs. The retained GLES build passes the shade gate. A later synchronization change would require a separate controlled test; see the [Vulkan probe record](../../target/perf-artifacts/ (frame-baseline worktree) vulkan-probe-validation.md).

- **The retained build uses OpenGL ES.** The pinned Makepad revision `73dfc62` also contains non-XR Android Vulkan initialization through `CxVulkan::new` in `android.rs`; the earlier claim that it only supported XR was incorrect. The probe's `use_vulkan` build flag and Android GPU service confirmed a Vulkan device and swapchain. Its GLES context is kept for interop, not evidence that the window rendered through GL.
- **Vulkan can reduce driver overhead.** It does not automatically reduce pixel work or texture traffic. Older-device performance and reliability need testing. [Android Vulkan guidance](https://developer.android.com/games/develop/vulkan/native-engine-support)
- **Inspect synchronization first.** This revision waits on CPU fences before and after off-screen submissions. Their cost on this phone is unmeasured; preserve resource lifetimes and ordering if changing them. See the [controlled Vulkan experiment](performance-plan.md#p3--evaluate-vulkan-after-establishing-the-optimized-gl-reference).

## How to read these numbers

- **Historical burst fps.** Intervals of 250 ms or more were excluded. This can also hide an actual long stall, and shorter idle gaps can enter the result. Use validated activity boundaries and retain active stalls for new comparisons.
- **Noise.** Open app, page swipe and Recents to Home vary by up to 10 fps between runs.
- **The frame monitor distorts Recents.** Its overlay requests frames itself: Recents runs at 36 fps with it on, 52–56 with it off. Final numbers were taken with it off.
- **Harness starting point.** The harness reaches Recents from the Android drawer, which is its slowest starting point.

## Pull requests

Historical PR snapshot from the original measurement session; current merge status is not asserted here. The Base column defines dependencies: #25 and #26 both branch from #24.

| PR | Contents | Base |
|---|---|---|
| OctoSense #22 | Gesture layer, shade, pages, live island, tile groups | `main` |
| OctoSense #23 | Standalone mobile shell, no desktop styles or switcher bar on Android | #22 |
| OctoSense #24 | Hosted AppCard drawn in its client slot; automatic card re-approval per build | #23 |
| OctoSense #25 | AppCard pin for the DeepSeek card fix | #24 |
| OctoSense #26 | On-demand frame loop, frame monitor and PerfGraph | #24 |
| OctoSense #27 | Renderer idle-fix pin, clock, capture freeze, AppCard glass scope | #26 |
| OctoSense #28 (draft) | Transition fixes, shade glass, drawer fill, perf doc | #27 |
| makepad fork #6 | GL completion fence, idle repaint, glass flip | `port/appcard-on-octoscript` |
| makepad fork #7 | Main-thread wakes, icon cache, render-thread priority, blur mips, lookup probes | #6 |
| Octoscript-AppCard #92 | A turn finishes on the kernel's saved reply, not the partial stream | `main` |

- **Pins.** OctoSense pins exact fork and AppCard commits. If a fork or AppCard PR is squash-merged, the matching OctoSense pin must move to the merge commit.
- **Merge order.** Merge #92 before #25.

## Phone housekeeping

- **OnePlus 6T** (`bf0a4730`). Left at a 30-minute screen timeout with stay-awake on, because it disconnected before the final restore. The committed perf doc wrongly says it was restored. Three trace files also remain in `/data/local/tmp`.
- **OnePlus 6** (`cfb7c9e3`). Baseline and candidate APKs were installed with app data preserved. Its settings are unchanged; its own timeout is 30 minutes. ADB reconnects and some USB transfers still fail, so incomplete samples are rejected rather than counted as performance results.

Restore the 6T once it is connected:

```
adb -s bf0a4730 shell settings put system screen_off_timeout 30000
adb -s bf0a4730 shell svc power stayon false
adb -s bf0a4730 shell rm /data/local/tmp/tperf.data /data/local/tmp/tperf.txt /data/local/tmp/tatrace.txt
```
