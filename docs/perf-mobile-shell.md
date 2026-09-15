# Mobile shell perf — baseline vs perf (OnePlus 6T)

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

This is **upstream** (the makepad fork) and unresolved here.

### What the shell controls
**The cost of each forced repaint.**
- **Baseline:** three full-screen passes: the window, the `WmScene` framebuffer cache, and the desktop wallpaper `CachedView`.
- **Perf:** the window pass alone. The census's hot list on idle Home is `pass#15/window` only.

### Why the 66 ms frames on the App screen remain
The idle App screen (17–18 presents/s, every interval ≥ 33 ms) is the hosted AppCard presenting on its own. Its frames are in makepad's repaint path, not the shell's (1 scene/s on both builds).

## Visual check
Home, shade (pulled open) and Recents were captured on both builds with the monitor off:
- baseline: `scratchpad/regression-baseline/{home,shade,recents}.png`
- perf: `scratchpad/regression/{home,shade,recents}.png`

They match, apart from the wallpaper's drift position.

**A flip that is not a regression.** On both builds the frosted backdrop under the shade and the Recents overview samples the scene **upside down**: a mirrored clock and mirrored tiles show through at the bottom. It is identical on the baseline, so it predates these commits. It is the GL snapshot orientation the desk code already notes ("the final-glass snapshot is upside down on GL"), and it is left for a separate fix.

## Device housekeeping
- **What the run changed.** It set `svc power stayon true` and `settings put system screen_off_timeout 1800000`.
- **Restored at the end.** `settings put system screen_off_timeout 30000` (the original value) and `svc power stayon false`.
