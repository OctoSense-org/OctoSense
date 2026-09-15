# Mobile shell perf — baseline, perf, the idle-repaint fix and the transition fixes (OnePlus 6T)

Device: OnePlus 6T (ONEPLUS A6013), Snapdragon 845, 1080×2340 @ 60 Hz, over USB (`bf0a4730`).
Both builds sit on `fix/hosted-appcard-mobile` @ fedd802 (PR #24):
- **baseline**: `perf/mobile-shell-baseline` @ 7b253a8. The frame monitor, PerfGraph and `[perf]` census only, no fixes.
- **perf**: `perf/mobile-shell` @ 57a3134. The same instrument plus four fix commits.

## Method
**Two instruments per scenario.**
1. **SurfaceFlinger** (what the display got). `dumpsys SurfaceFlinger --latency 'SurfaceView - dev.makepad.octosense/…#0'`, intervals between actual present times (`sf_sample.py`).
2. **makepad's frame monitor** (`Cx::perf_monitor`), switched on with a battery-icon triple tap. The `[perf]` line every 2 s gives:
   - the main-thread gap;
   - per-channel ms per shell frame;
   - what asked for the frame;
   - a renderer census: repaints against scenes drawn.

   Plus one PerfGraph screenshot per scenario (`scratchpad/perf-baseline/`, `scratchpad/perf-perf/`).

**Procedure.** Driven by `perf_scenarios2.sh <baseline|perf>`, with gestures injected through `input tap/swipe/motionevent`. The raw logs are `perf-baseline.txt` and `perf-perf.txt`, and the tables come from `perf_tables.py`.

**Before, for reference.** `perf-before.md` holds the first measurement on the installed feat/mobile-standalone @ 8197abf, SurfaceFlinger only. It reproduces within noise in the baseline column below.

## Table 1 — SurfaceFlinger (display), baseline vs perf

| scenario | baseline fps | baseline p50/p95/max ms | baseline >16.7 / >33 | perf fps | perf p50/p95/max ms | perf >16.7 / >33 |
|---|---|---|---|---|---|---|
| idle-home | 56.9 | 16.6 / 17.3 / 66.5 | 22/454 · 3/454 | 56.2 | 16.6 / 33.1 / 66.4 | 24/448 · 5/448 |
| open-app(tap AppCard icon) | 24.3 | 34.6 / 66.7 / 515.9 | 49/96 · 48/96 | 27.3 | 16.7 / 66.6 / 549.3 | 46/108 · 43/108 |
| idle-app-screen | 17.5 | 50.2 / 66.8 / 83.3 | 138/138 · 138/138 | 18.6 | 49.9 / 66.9 / 116.7 | 147/147 · 146/147 |
| swipe-up-home | 42.8 | 16.6 / 66.4 / 83.2 | 32/170 · 25/170 | 40.6 | 16.6 / 66.4 / 83.3 | 36/161 · 30/161 |
| shade-pull+close | 40.1 | 16.6 / 50.1 / 99.8 | 61/199 · 34/199 | 41.1 | 16.7 / 50.0 / 83.1 | 66/204 · 27/204 |
| page-swipe(right then back) | 56.1 | 16.6 / 32.7 / 66.4 | 16/279 · 4/279 | 55.3 | 16.6 / 16.8 / 116.5 | 9/275 · 7/275 |
| recents(swipe-up-hold) | 38.7 | 16.7 / 49.5 / 199.7 | 82/192 · 10/192 | 39.9 | 16.7 / 33.6 / 182.8 | 81/198 · 7/198 |
| recents-idle | 30.4 | 33.3 / 66.6 / 116.9 | 94/120 · 8/120 | 33.7 | 33.3 / 49.9 / 99.8 | 92/133 · 10/133 |
| recents-to-home | 46.1 | 16.6 / 33.5 / 182.9 | 44/183 · 5/183 | 48.6 | 16.6 / 33.5 / 99.7 | 36/193 · 6/193 |
| island-demo(triple-tap clock) | 53.6 | 16.6 / 33.3 / 116.5 | 26/320 · 6/320 | 52.9 | 16.6 / 33.3 / 116.0 | 22/316 · 9/316 |
| group-open(tap Work) | 46.1 | 16.6 / 33.5 / 99.8 | 41/183 · 6/183 | 47.3 | 16.6 / 33.5 / 116.3 | 38/188 · 6/188 |
| group-idle | 41.8 | 16.6 / 33.7 / 149.4 | 51/166 · 7/166 | 44.1 | 16.6 / 49.9 / 83.2 | 46/175 · 11/175 |
| group-close(tap scrim) | 51.1 | 16.6 / 33.5 / 133.5 | 17/152 · 4/152 | 53.7 | 16.6 / 33.3 / 66.6 | 18/160 · 1/160 |

CPU after *idle-home*: baseline 78.0 % · perf 43.0 %  
CPU after *idle-app-screen*: baseline 41.0 % · perf 34.0 %  

## Table 2 — makepad frame monitor (`[perf]`), baseline vs perf

Gap = main-thread paint-to-paint interval (p50 / p95 / max ms, with (>16.7·>33) counts). ms/frame columns: event · home · module · glass · overlay · shade · groups. Asked by: a=animation g=gesture x=action e=external (nothing in the shell asked).

| scenario | baseline scenes/s | baseline repaints/s | baseline gap | baseline ms/frame | baseline asked by | perf scenes/s | perf repaints/s | perf gap | perf ms/frame | perf asked by |
|---|---|---|---|---|---|---|---|---|---|---|
| idle-home | 57.0 | 57.0 | 16.5 / 26.2 / 81.9 (102·15) | 4.48 · 4.76 · 0.00 · 0.02 · 0.03 · 0.00 · 0.00 | a513 g0 x0 e0 | 1.0 | 56.3 | 0.0 / 0.0 / 0.0 (0·0) | 20.61 · 18.75 · 0.00 · 0.00 · 0.12 · 0.00 · 0.00 | a0 g0 x0 e10 |
| open-app(tap AppCard icon) | 26.3 | 33.1 | 16.6 / 264.6 / 264.6 (26·18) | 6.10 · 4.62 · 0.78 · 0.02 · 0.03 · 0.00 · 0.00 | a177 g0 x0 e7 | 2.6 | 30.4 | 0.0 / 348.6 / 348.6 (11·11) | 26.00 · 6.57 · 8.21 · 0.00 · 0.11 · 0.00 · 0.00 | a7 g0 x0 e11 |
| idle-app-screen | 1.0 | 17.3 | 0.0 / 0.0 / 0.0 (0·0) | 49.08 · 1.31 · 0.18 · 0.03 · 0.16 · 0.00 · 0.00 | a0 g0 x0 e12 | 1.0 | 18.5 | 0.0 / 0.0 / 0.0 (0·0) | 26.19 · 0.03 · 0.23 · 0.00 · 0.22 · 0.00 · 0.00 | a0 g0 x0 e11 |
| swipe-up-home | 34.8 | 40.2 | 17.3 / 76.4 / 94.5 (57·11) | 5.05 · 4.46 · 0.01 · 0.02 · 0.10 · 0.00 · 0.00 | a203 g3 x0 e3 | 2.9 | 35.0 | 0.0 / 478.0 / 478.0 (15·7) | 15.43 · 6.81 · 0.12 · 0.04 · 0.54 · 0.00 · 0.00 | a11 g3 x1 e8 |
| shade-pull+close | 43.3 | 43.3 | 16.8 / 66.9 / 102.4 (97·55) | 5.32 · 4.96 · 0.00 · 0.02 · 0.04 · 0.40 · 0.00 | a298 g5 x0 e0 | 6.1 | 46.1 | 17.3 / 380.5 / 380.5 (28·20) | 9.30 · 5.70 · 0.00 · 0.04 · 0.06 · 1.83 · 0.00 | a18 g4 x8 e19 |
| page-swipe(right then back) | 55.2 | 55.2 | 16.5 / 28.2 / 103.1 (81·10) | 6.34 · 1.39 · 0.00 · 0.02 · 0.04 · 0.00 · 0.00 | a319 g12 x0 e0 | 4.0 | 55.1 | 16.6 / 262.4 / 262.4 (10·5) | 18.14 · 8.52 · 0.00 · 0.00 · 0.06 · 0.00 · 0.00 | a1 g9 x8 e10 |
| recents(swipe-up-hold) | 37.9 | 37.9 | 29.0 / 69.9 / 144.8 (199·18) | 9.34 · 2.02 · 0.00 · 0.05 · 0.47 · 0.00 · 0.07 | a193 g110 x0 e0 | 8.7 | 43.0 | 27.6 / 92.8 / 490.9 (69·13) | 15.34 · 1.53 · 0.00 · 0.09 · 0.84 · 0.00 · 0.24 | a1 g67 x0 e10 |
| recents-idle | 30.7 | 30.7 | 29.6 / 96.9 / 115.2 (210·13) | 11.44 · 4.12 · 0.00 · 0.08 · 0.74 · 0.00 · 0.03 | a215 g0 x0 e0 | 1.2 | 33.2 | 26.1 / 26.1 / 26.1 (1·0) | 40.74 · 9.00 · 0.00 · 0.14 · 1.89 · 0.00 · 0.07 | a0 g0 x0 e7 |
| recents-to-home | 43.3 | 43.3 | 16.8 / 32.2 / 142.4 (154·12) | 6.65 · 4.07 · 0.00 · 0.04 · 0.24 · 0.00 · 0.01 | a345 g6 x0 e0 | 3.2 | 45.9 | 0.0 / 93.0 / 371.0 (20·4) | 26.39 · 5.89 · 0.00 · 0.06 · 0.61 · 0.00 · 0.02 | a15 g3 x0 e11 |
| island-demo(triple-tap clock) | 52.5 | 52.5 | 16.6 / 34.0 / 115.1 (99·16) | 4.81 · 4.65 · 0.00 · 0.02 · 0.30 · 0.00 · 0.00 | a425 g0 x0 e0 | 4.4 | 52.7 | 16.3 / 75.0 / 166.6 (5·4) | 19.10 · 7.33 · 0.00 · 0.00 · 1.21 · 0.00 · 0.00 | a20 g0 x6 e5 |
| group-open(tap Work) | 47.4 | 47.4 | 20.7 / 26.0 / 124.5 (248·16) | 5.50 · 4.49 · 0.00 · 0.03 · 0.39 · 0.00 · 0.50 | a422 g0 x0 e0 | 26.0 | 46.5 | 20.4 / 44.0 / 112.5 (82·7) | 4.83 · 4.88 · 0.00 · 0.01 · 0.42 · 0.00 · 0.50 | a153 g0 x2 e1 |
| group-idle | 41.2 | 41.2 | 21.1 / 73.6 / 109.3 (240·11) | 5.85 · 4.52 · 0.00 · 0.03 · 0.39 · 0.00 · 0.95 | a247 g0 x0 e0 | 1.0 | 44.7 | 0.0 / 0.0 / 0.0 (0·0) | 71.17 · 16.11 · 0.00 · 0.09 · 1.12 · 0.00 · 2.60 | a0 g0 x6 e0 |
| group-close(tap scrim) | 49.0 | 49.0 | 20.3 / 30.7 / 110.6 (89·8) | 5.00 · 4.34 · 0.00 · 0.03 · 0.37 · 0.00 · 0.32 | a250 g0 x0 e0 | 4.5 | 52.5 | 15.3 / 307.7 / 343.4 (4·3) | 19.24 · 8.53 · 0.00 · 0.03 · 0.47 · 0.00 · 0.68 | a20 g0 x6 e1 |

scenes/s = phone scenes the shell drew; repaints/s = window repaints the renderer did (census).

**How to read Table 2's ms/frame columns.** They are per *shell* frame. When the shell draws only 1 scene/s, the time of every event in that second lands on that one frame, so a larger number there is not a slower frame.

## What the numbers say
### The shell now draws on demand
- **Idle screens.** The phone scene went from 57 → 1.0 draws/s on idle Home, 30.7 → 1.2 on idle Recents, and 41.2 → 1.0 on the idle group window.
- **Why they were busy.** On baseline every idle frame was asked for by the animation loop (`asked by: anim`), which re-armed only to drift the wallpaper. On perf nothing in the shell asks.
- **Transitions.** Frames are drawn only while something moves: the swipe home 34.8 → 2.9 scenes/s, the page swipe 55.2 → 4.0, the island demo 52.5 → 4.4.

### CPU
- **Idle Home:** 78 % → 43 %.
- **Idle App screen:** 41 % → 34 %.

### What the display still gets
**SurfaceFlinger** is essentially unchanged: ~56 presents/s on idle Home on both builds, and the same 33–66 ms spikes during transitions.

**The cause is makepad, not the shell.** The census shows the renderer repainting the window at display rate on both builds (repaints/s ≈ presents) while the perf shell draws 1 scene/s. Sampled directly on the idle phone:
- `demo_time_repaint` is set on every tick;
- no pass is dirty and no repaint is requested;
- `retirement-debt: allocations` every time.

**The mechanism.** makepad's retained-upload allocation ledger never settles on the GL backend, so `retire_free_items` sets `demo_time_repaint` on every render (`opengl.rs:409`). That makes `compute_pass_repaint_order` repaint every pass recorded in the latest redraw.

**What was ruled out.**
- The worker pool is healthy.
- Dropping the only per-frame timer before the app doesn't change the repaints.
- Neither does drawing a single flat quad for the whole scene (desktop repro).
- No drawn shader reads the pass time.

This is **upstream** (the makepad fork). Resolved in the third column below.

### What the shell controls
**The cost of each forced repaint.**
- **Baseline:** three full-screen passes: the window, the `WmScene` framebuffer cache, and the desktop wallpaper `CachedView`.
- **Perf:** the window pass alone. The census's hot list on idle Home is `pass#15/window` only.

### Why the 66 ms frames on the App screen remain
The idle App screen (17–18 presents/s, every interval ≥ 33 ms) looked like the hosted AppCard presenting on its own. The shell drew 1 scene/s on both builds.

**Corrected by the third column.** These frames were makepad's forced repaint, not the module:
- **Every forced repaint re-encoded the whole window's gauss pyramid.** The module's kit glass had requested it.
- **Idle App screen with the idle fix:** 1.0 presents/s at 3 % CPU.

## Visual check
Home, shade (pulled open) and Recents were captured on both builds with the monitor off:
- baseline: `scratchpad/regression-baseline/{home,shade,recents}.png`
- perf: `scratchpad/regression/{home,shade,recents}.png`

They match, apart from the wallpaper's drift position.

**A flip that is not a regression.** On both builds the frosted backdrop under the shade and the Recents overview samples the scene **upside down**: a mirrored clock and mirrored tiles show through at the bottom. It is identical on the baseline, so it predates these commits. It is the GL snapshot orientation the desk code already notes ("the final-glass snapshot is upside down on GL"). Fixed in the third column below.

## Third column — the makepad idle fix and the transition fixes (`perf/android-idle-repaint`)

The third build is `perf/mobile-shell` plus:
- the makepad fork at `fix/android-idle-repaint` (OctoSense-org/makepad#6);
- five shell commits: the in-process clock, the fork-side flip replacing the GL override, a module's glass scoped to its capture, the capture freeze while an app zooms open, and a one-branch wallpaper shader.

It is measured with `perf_scenarios3.sh round3`, the same 13 scenarios with the monitor verified on. Raw data is in `scratchpad/perf-round3.txt` and `perf-round3/`, and the tables come from `perf_tables3.py`.

### Why the display kept getting ~56 frames/s: the unpolled GL completion fence
The completed-serial counter on GL only moves when `poll_texture_lifetimes` polls the completion fence, and the GL render path never called it. Its only callers were `frame_completion_serial()` and texture release.

**The effect, step by step.**
1. Every released retained-upload allocation record (`submitted > completed`) stayed in the ledger forever.
2. `has_pending_instance_retirements()` never went false.
3. `render_view` set `demo_time_repaint` on every render (`platform/src/os/linux/opengl.rs:409` at ad8f372).
4. Every live pass repainted on the next vsync.

**Why macOS settles.** Metal command-buffer completion handlers advance the counter on their own.

**A diagnostic build on the phone.** After 953 submissions of an idle Home: `completed=0 records=598 terms=allocations fence_fns=false`. The fence functions had never been resolved.

**The fix.**
- `render_view` polls the fence.
- `eglGetCurrentContext` comes from libEGL, because Android's `eglGetProcAddress` resolves extension entry points only.
- On Android, retirement debt is served on an idle vsync beat without painting, like the macOS maintenance beat.

**Result.**
| screen | before | after |
|---|---|---|
| idle Home | 56 presents/s | 1.0 presents/s (the clock tick) |
| idle hosted-AppCard screen (17.5 fps with every frame > 33 ms before; not the module's own cost) | 18.6 presents/s | 1.0 presents/s |
| idle Recents | 33.7 presents/s | 1.2 presents/s |
| idle group | 44.1 presents/s | 1.0 presents/s |

### Idle CPU: a forked `date`
**After the renderer fix, idle Home still used 19 % of a core.** Per-thread `/proc` ticks put 11 % on `wm-status`: the status sampler forked `date` twice a second. `localtime_r` + `strftime` replace it.

**Idle Home CPU:**
| stage | CPU |
|---|---|
| perf | 43 % |
| fork fix alone | 19 % |
| with the clock fix | 3–8 % |

The rest is the main loop waking on every Choreographer vsync to find nothing to paint (~5 %) and the Java UI thread (~2 %).

### The frosted backdrop was upside down
The fork's GL backend has rendered offscreen passes top-left since d1a0eb1cb. `gauss_render_texture_y_flip_for_os` still returned a V flip of 1.0 on Android, so every glass shader sampled the pyramid mirrored.

**Fixed in the fork.** The flip is 0 on every backend. The shell's `DrawGaussScene` override (`octosense/android_rendering.rs`) is gone.

**Verified.** The clock behind the shade and Recents reads upright (`scratchpad/perf-round2/visual-{shade,recents}.png`).

### What the transition frames were made of
Android had no platform channels, so the fork adds `draw` (pass encode), `wait` (`eglSwapBuffers`) and `gc` to the frame monitor. SurfaceFlinger's third `--latency` column (the acquire fence) gives a GPU-ready latency per frame.

**On the idle-fix build, transitions are GPU-bound, not CPU-bound.**
| scenario | burst fps | CPU draw (ms, mean) | CPU shell (ms, mean) | eglSwapBuffers wait (ms, mean / worst) | GPU-ready p50 |
|---|---|---|---|---|---|
| open-app | 17–20 | 9.8 | home 11.6, module 14 | 25.5 / 124 | 94 ms |
| shade | 25–30 | 5.9 | shade 1.9 | 15.3 / 92 | — |
| Recents hold | 33 | — | — | — | 41 ms |

`draw` is the pass encode and the shell columns are its own channels; all of them are CPU time.

**Two costs the census exposed.**
- **The hosted module's kit glass requested the whole WM window's gauss pyramid.** `gauss_mip_*` on the window appeared in the hot list on open-app and on every idle App tick: a full-screen scene pass, a copy and six levels. `WindowFrame` now carries a `CaptureGauss`, as the fork's own WM does.
- **The foreground app's full capture was re-recorded on every frame of the zoom-open animation.** It now zooms its last capture and refreshes once settled.
- **Also:** the wallpaper shader computed both styles per pixel, full screen, and now branches.

**Open-app improved from 17–20 fps to 31.5** (p50 16.6 ms). Every other transition is still short of the ≥ 55 fps / p95 ≤ 20 ms target; see below.

## Table 3 — SurfaceFlinger, perf (Table 1) vs round3

Burst stats exclude intervals of 250 ms or more (idle gaps and the 1 s clock tick): once an idle screen presents ~1/s, whole-window stats measure the idle gap, not the animation. GPU-ready = queue → acquire-fence signal, per burst frame.

| scenario | perf presents/s | perf p50/p95 ms | perf >16.7 / >33 | round3 presents/s | round3 burst fps | round3 burst p50/p95/max ms | round3 >16.7 / >33 | round3 GPU-ready p50/p95 ms |
|---|---|---|---|---|---|---|---|---|
| idle-home | 56.2 | 16.6 / 33.1 | 24/448 · 5/448 | 1.0 | — | no burst | — | — |
| open-app(tap AppCard icon) | 27.3 | 16.7 / 66.6 | 46/108 · 43/108 | 6.0 | 31.5 | 16.6 / 49.9 / 199.6 | 6/22 · 5/22 | 17.2 / 121.4 |
| idle-app-screen | 18.6 | 49.9 / 66.9 | 147/147 · 146/147 | 1.0 | — | no burst | — | — |
| swipe-up-home | 40.6 | 16.6 / 66.4 | 36/161 · 30/161 | 7.5 | 40.1 | 16.7 / 33.3 / 66.5 | 11/26 · 1/26 | 37.5 / 44.7 |
| shade-pull+close | 41.1 | 16.7 / 50.0 | 66/204 · 27/204 | 10.2 | 33.7 | 33.3 / 49.9 / 99.8 | 24/46 · 8/46 | 39.8 / 123.7 |
| page-swipe(right then back) | 55.3 | 16.6 / 16.8 | 9/275 · 7/275 | 7.0 | 50.1 | 16.6 / 16.8 / 116.4 | 1/30 · 1/30 | 17.3 / 21.6 |
| recents(swipe-up-hold) | 39.9 | 16.7 / 33.6 | 81/198 · 7/198 | 15.6 | 36.0 | 33.3 / 33.3 / 83.2 | 46/73 · 1/73 | 37.2 / 48.4 |
| recents-idle | 33.7 | 33.3 / 49.9 | 92/133 · 10/133 | 1.2 | 60.1 | 16.6 / 16.6 / 16.6 | 0/1 · 0/1 | 77.4 / 77.4 |
| recents-to-home | 48.6 | 16.6 / 33.5 | 36/193 · 6/193 | 6.2 | 34.1 | 16.6 / 33.3 / 149.7 | 9/21 · 1/21 | 30.1 / 81.2 |
| island-demo(triple-tap clock) | 52.9 | 16.6 / 33.3 | 22/316 · 9/316 | 5.5 | 55.7 | 16.6 / 33.2 / 33.3 | 2/25 · 0/25 | 18.3 / 24.3 |
| group-open(tap Work) | 47.3 | 16.6 / 33.5 | 38/188 · 6/188 | 26.8 | 52.6 | 16.6 / 33.3 / 49.9 | 13/105 · 2/105 | 22.5 / 39.2 |
| group-idle | 44.1 | 16.6 / 49.9 | 46/175 · 11/175 | 1.0 | — | no burst | — | — |
| group-close(tap scrim) | 53.7 | 16.6 / 33.3 | 18/160 · 1/160 | 6.0 | 37.6 | 16.6 / 83.2 / 83.2 | 5/15 · 2/15 | 17.0 / 56.6 |

CPU after *idle-home*: perf 43.0 % · round3 3.0 %  
CPU after *idle-app-screen*: perf 34.0 % · round3 3.0 %  

## Table 4 — frame monitor channels on round3 (busy windows, mean/worst ms per shell frame)

| scenario | gap p50/p95/max ms (>16.7·>33) | channels |
|---|---|---|
| idle-home | idle | idle |
| open-app(tap AppCard icon) | 16.7 / 168.2 / 176.5 (8·5) | event 13.0/247, draw 9.0/15, wait 11.0/122, home 1.8/5, module 6.1/139, overlay 0.1/1 |
| idle-app-screen | idle | idle |
| swipe-up-home | 23.5 / 142.2 / 293.5 (24·2) | event 3.1/24, draw 3.1/6, wait 13.2/21, home 2.2/4, module 0.0/0, glass 0.0/0, overlay 0.3/3 |
| shade-pull+close | 39.0 / 45.5 / 104.2 (27·23) | event 6.4/48, draw 5.7/10, wait 14.9/92, home 3.9/10, glass 0.0/0, overlay 0.1/0, shade 1.6/32 |
| page-swipe(right then back) | 15.6 / 28.8 / 120.4 (9·1) | event 8.9/61, draw 3.9/8, wait 2.9/9, home 2.8/20, overlay 0.0/0 |
| recents(swipe-up-hold) | 27.4 / 233.1 / 466.8 (75·3) | event 11.3/53, draw 6.1/15, wait 10.5/63, home 1.0/4, glass 0.1/1, overlay 0.6/2, groups 0.1/10 |
| recents-idle | 29.9 / 29.9 / 29.9 (1·0) | event 38.6/53, draw 7.0/9, wait 1.4/2, home 6.4/12, glass 0.2/0, overlay 1.5/3, groups 0.1/0 |
| recents-to-home | 23.9 / 78.9 / 166.1 (20·2) | event 12.7/52, draw 4.4/9, wait 8.5/64, home 4.1/7, glass 0.1/0, overlay 0.7/4, groups 0.0/0 |
| island-demo(triple-tap clock) | 18.5 / 313.2 / 364.0 (9·2) | event 13.4/56, draw 4.3/7, wait 2.7/14, home 4.6/10, overlay 0.6/11 |
| group-open(tap Work) | 20.6 / 58.7 / 58.7 (59·4) | event 5.0/53, draw 3.6/8, wait 5.6/42, home 3.6/9, glass 0.0/0, overlay 0.4/1, groups 0.3/11 |
| group-idle | idle | idle |
| group-close(tap scrim) | 17.8 / 99.2 / 99.2 (8·5) | event 13.2/60, draw 6.5/14, wait 4.0/19, home 5.5/10, glass 0.0/0, overlay 0.5/1, groups 1.0/2 |

### What still blocks 55 fps on shade, Recents and the swipe home
**The A/B test.** A temporary build read \`debug.octosense.ab\` on every scene draw, so parts of the frame could be switched off from adb:
- \`flatwall\`: a flat fill instead of the wallpaper shader;
- \`noglass\`: no compositor, so no gauss scene, copy or pyramid;
- \`nohome\`: no home page.

**Results.** Fresh launch, empty Recents, two repetitions per set; burst fps with p95 in ms.

| switches | shade | Recents hold | Recents → Home |
|---|---|---|---|
| none | 25–43 (50–150) | 48–56 (17–33) | 35–44 (33–50) |
| flatwall | 41–43 (50–67) | 53–56 (33) | 46 (33) |
| noglass | 32–42 (50–133) | 52–56 (33) | 44–46 (33) |
| nohome | 42–43 (50) | 55–57 (17–33) | 28–48 (33–216) |
| flatwall + noglass | 37–44 (50–83) | 51–56 (33) | 41–46 (33–50) |

**Full-screen GPU content is not what holds these frames.** Removing the wallpaper shader, the whole backdrop compositor, or the home page moves nothing beyond run-to-run noise.

**Second run, with the AppCard card in Recents** (as in the harness), monitor off:

| switches | Recents hold | Recents → Home | shade |
|---|---|---|---|
| none (twice) | 52–56 (33) | 23–46 (33–216) | 33–44 (50) |
| flatwall | 51–56 (33) | 44–45 (33) | 38–41 (50–67) |
| noglass | 51–54 (33) | 34–44 (33–50) | 27–43 (33–233) |
| nohome | 53–53 (33) | 47–48 (33) | 44 (33–50) |
| flatwall + noglass | 54 (33) | 34–44 (33–50) | 31–42 (67–133) |

The card changes nothing either.

**The monitor distorts one scenario.** With the monitor off, the Recents hold runs at 52–56 fps, against 36 fps in round 3, where the monitor and PerfGraph were on. PerfGraph asks for a next frame on every draw and adds its own full-width graph, so the round-3 Recents-hold column understates the monitor-off build.

**The shade doesn't have that excuse.** It stays at 33–44 fps with p95 50 ms, monitor on or off, with or without GPU content.

**Where the slow frames sit.** Per-frame intervals from \`perf-round3/*.presents\`:
- **Animation-driven phases already present at 16.6 ms:** the shade's settle, Home after a swipe, the island, the group window.
- **The first frame of a transition** is 67–150 ms, sometimes two frames.
- **The finger phase** runs 33/33/50 ms on the shade pull, or alternating 16.7/33 ms on the swipe home and the Recents hold.

**Each alternating frame misses the vsync on the main thread, not on the GPU.** The frame monitor on round 3 during the Recents hold:

| channel | ms per frame (mean) |
|---|---|
| \`event\` (the shell's event and draw recording) | 11.3 |
| \`draw\` (pass encode) | 6.1 |
| \`wait\` (\`eglSwapBuffers\`) | 10.5 |
| **total** | **≈ 28**, against a 16.7 ms budget |

The shell's own draw channels account for only ~2 ms of \`event\` (home 1.0, overlay 0.6, glass 0.1), so ~9 ms of per-frame scene recording is unattributed.

**Next step:** a CPU profile of that recording plus the swap, which needs a debuggable build for \`simpleperf\`; the release APK's perf events are denied. The first-frame spikes line up with the monitor's worst \`event\` (48–80 ms) and \`wait\` (63–122 ms) frames at a transition's start.

**Targets met on round 3:**
- idle Home, Recents and group windows: ≈ 1 present/s;
- open-app: p50 16.6 ms (31.5 fps, p95 50 ms);
- page swipe: 50 fps, p95 16.8 ms.

**Targets not met:**
- shade: 34 fps, p95 50 ms;
- Recents hold: 36 fps, p95 33 ms, with the AppCard card present;
- swipe home: 40 fps, p95 33 ms;
- Recents → Home: 34 fps, p95 33 ms.

## Fourth column — main-thread and fill-rate fixes (`perf/transition-main-thread`)

The fourth build is `perf/android-idle-repaint` plus:
- the makepad fork at `perf/android-main-thread` @ e596207 (six commits on #6);
- five shell commits: the run view's tick, the Android drawer's wallpaper and fills, the shade as the compositor's final glass, and the shade as one glass layer with its glyphs warmed.

Measured with `perf_scenarios3.sh`, monitor **off**, two fresh launches per build. Raw data is in `scratchpad/perf-t-{before,before2,final1,final2}` (SurfaceFlinger presents) and `perf-t-finalmon` (the attribution run with the monitor on). The tables come from `ttable.py`.

### How it was profiled
- **CPU.** `simpleperf record --call-graph dwarf -f 4000` as root on the release app, per scenario, both whole-process and render-thread only (`scratchpad/tprof/`). Symbols come from the APK's unstripped `libmakepad.so`.
- **Scheduling.** `atrace gfx view input sched freq idle` over a shade pull, a Recents hold and a return home.
- **GPU attribution.** A temporary `debug.octosense.ab` switch, not committed, turned parts of the shade and the drawer off one at a time.

### Main-thread hotspots on PR #27's build (share of render-thread samples)
| scenario | App event dispatch | hosted AppCard (`MpModuleView`) | `draw_phone_scene` | `AppIconDraw` → `render_svg` | `render_view` (GL encode) |
|---|---|---|---|---|---|
| shade | 34 % | 22 % (`update_connection_indicator` 10 %) | 33 % | 27 % → 23 % | 20 % |
| open-app | 56 % | 35 % (15 %) | 16 % | 12 % → 11 % | 15 % |
| swipe home | 49 % | 33 % (15 %) | 16 % | 14 % → 12 % | 18 % |
| page swipe | 52 % | 34 % (15 %) | 21 % | 17 % → 15 % | 12 % |
| Recents hold | 34 % | 23 % (11 %) | 18 % | 13 % → 11 % | 33 % |
| Recents → Home | 50 % | 32 % (13 %) | 24 % | 20 % → 17 % | 12 % |
| group open | 50 % | 34 % (16 %) | 24 % | 21 % → 18 % | 15 % |

**Icon tessellation.** `AppIconDraw` re-tessellated icons on every frame; `add_gradient_row` and `sample_stops` were the top self symbols. In the heaviest Recents frames, icon tessellation was 54 % of the frame.

**The hosted AppCard.** Its cost was `Event::Signal` and `Event::Timer` dispatched from `handle_other_events`. The sources were a worker signal once per animation frame (Recents hold: 110 next-frames against 109 signals in 2 s) and a 125 Hz run-view timer. Each event ran the AppCard's `update_connection_indicator` script eval.

**After the fixes:**
- `AppIconDraw` 0.8–1.8 %;
- `render_svg` 0–0.6 %;
- `glTexImage2D` 0 %;
- the module path 5–12 %.

### Scheduling
- **Priority and cores.** The render thread ran at **nice 0**, while HWUI's RenderThread runs at −10. **21–28 % of its run time was on the little cores**, mostly between 576 MHz and 1.4 GHz, because `schedutil` ramps up from the idle gap before each transition.
- **No contention.** Run-queue wait was short (p50 0.09 ms, max 8 ms) and there were no lock waits.
- **Swap back-pressure.** `eglSwapBuffers` waits on the GPU, not on CPU work. In slow frames the render thread sits in D state while `queueBuffer` takes 22–73 ms.
- **Now:** nice −10, pinned to cpus 4–7. The first boost was undone by `warm_task_pool` (`UserInteractive` = nice 0), so the boost now runs after Startup. Android 11 has no ADPF hint API.
- **GPU clock.** The GPU (`msm-adreno-tz`) idles at 257 MHz and reaches 710 MHz within ~150 ms of a pull. It was not thermally limited (35 °C).

### What the shade's frames were made of: the A/B
Shade pull and close, monitor off, two pulls per variant.

| variant | fps | GPU-ready p95 ms | monitor on: `wait` mean / worst ms |
|---|---|---|---|
| everything drawn (4 runs) | 34–42 | 91–108 | 7–21 / 57–85 |
| no dim | 30–35 | 105–119 | — |
| no sheet glass | 39–50 | 63–68 | 4–6 / 16–34 |
| no sheet content | 41–43 | 70–94 | 12 / 79 |
| no cards | 34–41 | 100–121 | — |
| no text | 26–43 | 100–121 | — |
| no compositor at all | 44–46 | 84–106 | 4–20 / 31–64 |
| no dim, glass or content | 34 / **60.1** | 30 | **1.5–3.7 / 8–11** |

**This is fill rate, not GL driver overhead.** CPU `draw` (the GL encode) stayed at 4–5.5 ms in every variant, including the one at 60 fps. The swap wait fell from 7–21 ms to 1.5 ms only once the stacked full-screen blends were gone: the sheet glass, its 55–62 % tint layer, the dim and the content. A Vulkan path would not remove this cost; drawing fewer full-screen blended pixels does.

**After the shade fixes** (final glass, one merged glass layer, dim clipped, one fewer mip), with the monitor on, `wait` averages 4.9–16 ms with a worst case of 36–47 ms. Before the fixes it averaged 7–21 ms, worst 57–85 ms.

### The Recents hold depends on where it starts
The harness reaches Recents from the Android drawer, because its page swipe ends there.
- **Over Home:** the hold already ran at 52–56 fps (GPU-ready p50 12 ms).
- **Over the drawer:** 38–40 fps (GPU-ready p50 42 ms).

A/B over the drawer:

| variant | fps |
|---|---|
| drawer's icons, labels or search pill removed | no change |
| either full-screen fill removed (the system-bar fill under the drawer, or the drawer's own sheet) | 42–44 |
| both fills removed | 48–49 |
| overview glass removed | 57–58 (p95 16.7 ms) |

The committed fix draws only the system-bar strips under the drawer and a flat fill for the sheet. It was measured through the A/B switch only: the phone disconnected before a harness run on the final commit.

### Fixes
| fix | where |
|---|---|
| a worker's UI wake dispatches but does not paint between vsync beats | fork `platform/src/os/linux/android/android.rs` (`FromJavaMessage::Wake`), `android_jni.rs`, `os/linux/mod.rs` |
| draw-storage retirement no longer raises `Event::Signal` on Android | fork `platform/src/draw_list.rs`, `thread.rs` (`submit_detached_silent`) |
| icon tessellation cached per 4 % size bucket | fork `widgets/src/app_icon.rs` |
| render thread at nice −10 on the fast cluster, after Startup | fork `platform/src/os/linux/android/android.rs` (`boost_render_thread`) |
| integer blur level: one bicubic read, `ceil(level)` mips | fork `widgets/src/gauss_view.rs` (`sample_blur`), `backdrop.rs` |
| run view ticks only with a process attached | `src/run_view.rs` |
| Android drawer: no wallpaper, no full-screen fill under it | `src/mobile.rs` (`scene_plan`), `src/desk/phone.rs`, `src/mobile_surface.rs` |
| shade sheet = the compositor's final glass | `src/desk/phone.rs` |
| shade = one merged-tint glass, dim below the sheet only, glyphs warmed once | `src/mobile_shade.rs`, `src/mobile_surface.rs` |

### Table 5 — SurfaceFlinger, PR #27's build vs this build, monitor off
Cells: burst fps · p95 ms · intervals > 33 ms / burst intervals · worst first frame of a burst (ms). The build measured here predates the drawer fill commit.

| scenario | before run 1 | before run 2 | after run 1 | after run 2 |
|---|---|---|---|---|
| open-app-tap-AppCard-icon | 41.4 · 49.9 · 4/20 · 49.9 | 30.1 · 50.0 · 5/21 · 16.6 | 40.1 · 66.5 · 4/20 · 49.9 | 33.4 · 166.3 · 4/20 · 16.6 |
| swipe-up-home | 43.7 · 33.3 · 0/24 · 33.3 | 42.1 · 33.3 · 1/21 · 49.9 | 45.6 · 33.3 · 0/22 · 16.6 | 41.3 · 33.3 · 1/22 · 49.9 |
| shade-pull-close | 33.7 · 49.9 · 9/46 · 66.5 | 33.9 · 49.9 · 8/48 · 83.2 | 45.1 · 49.9 · 3/33 · 49.9 | 45.7 · 49.9 · 2/35 · 33.3 |
| page-swipe-right-then-back | 41.8 · 66.5 · 2/32 · 199.6 | 35.1 · 133.1 · 2/28 · 133.1 | 37.1 · 83.0 · 2/29 · 199.7 | 47.0 · 33.3 · 1/25 · 33.3 |
| recents-swipe-up-hold | 34.9 · 33.3 · 1/75 · 149.7 | 34.5 · 49.9 · 4/74 · 83.2 | 38.8 · 33.5 · 0/84 · 33.5 | 36.3 · 33.3 · 1/84 · 149.6 |
| recents-to-home | 30.8 · 66.5 · 2/21 · 199.6 | 41.5 · 66.3 · 1/20 · 66.3 | 41.5 · 66.5 · 1/20 · 66.5 | 45.1 · 33.3 · 0/24 · 16.6 |
| island-demo-triple-tap-clock | 36.7 · 16.8 · 1/22 · 249.5 | 49.0 · 49.9 · 2/22 · 66.5 | 40.7 · 16.8 · 1/23 · 199.6 | 42.7 · 16.6 · 1/22 · 166.3 |
| group-open-tap-Work | 53.6 · 33.3 · 1/115 · 49.9 | 52.4 · 33.4 · 2/115 · 49.8 | 52.3 · 33.4 · 2/100 · 33.1 | 53.9 · 33.3 · 1/130 · 49.9 |
| group-close-tap-scrim | 48.1 · 33.3 · 0/16 · 33.3 | 36.1 · 99.8 · 2/15 · 50.0 | 48.1 · 49.9 · 1/16 · 49.9 | 48.1 · 50.0 · 1/16 · 50.0 |

- **Shade:** 34 → 45–46 fps; >33 ms frames 8–9 → 2–3; first frame 67–83 → 33–50 ms.
- **Recents → Home:** 31–42 → 42–45 fps.
- **Recents hold:** 35 → 36–39 fps. The harness starts it from the drawer; see the section above.
- **Page swipe, island demo, open-app:** within run-to-run noise. Their first frames are still 133–250 ms.

### Table 6 — frame monitor on this build (attribution run, busy windows, mean ms per frame)
| scenario | frames | gap p95 ms | event | draw | wait |
|---|---|---|---|---|---|
| open-app | 28 | 289.3 | 13.1 | 3.7 | 12.6 |
| swipe-up-home | 23 | 66.4 | 4.0 | 4.8 | 15.4 |
| shade-pull+close | 40 | 450.2 | 5.0 | 4.9 | 8.2 |
| page-swipe | 33 | 431.9 | 5.9 | 3.3 | 1.8 |
| recents (swipe-up-hold, from the drawer) | 89 | 58.4 | 8.2 | 4.7 | 12.0 |
| recents-to-home | 28 | 44.0 | 9.7 | 4.2 | 10.2 |
| island-demo | 27 | 297.9 | 7.1 | 2.9 | 2.3 |
| group-open | 228 | 26.8 | 7.2 | 4.0 | 3.0 |
| group-close | 19 | 346.9 | 8.3 | 5.2 | 4.7 |

### Not met, and why
- **No transition reaches ≥ 55 fps with p95 ≤ 20 ms in the harness.** Group open is closest: 52–54 fps, p95 33 ms. The remaining cost is GPU fill in the glass and full-screen layers; Recents over Home reaches 52–56 fps.
- **First frames of 133–250 ms remain.**
  - *Open-app:* 213 ms is the AppCard's script VM on first open. That covers parsing plus a failed property lookup that builds a `suggest_property` Levenshtein list.
  - *Open-app, continued:* a further 190 ms is `inflate_fast`: a compressed font asset (`to_java_load_asset` → `ensure_fonts_loaded`, likely the 19 MB CJK face) decompressed on the render thread.
  - *Page swipe and island:* the 150–250 ms first frames were not profiled.
  - *Candidate fixes:* store fonts uncompressed in the APK (cargo-makepad's `aapt add`) or preload them off the main thread, and fix the AppCard's failed script lookup.
- **A 136×128 palette PNG is decoded every ~50 ms** during a DeepSeek streaming turn. This runs on the AppCard's backend thread, not the render thread, and belongs to Octoscript-AppCard.

## Device housekeeping
- **What the run changed.** It set `svc power stayon true` and `settings put system screen_off_timeout 1800000`.
- **Restored at the end.** `settings put system screen_off_timeout 30000` (the original value) and `svc power stayon false`.
