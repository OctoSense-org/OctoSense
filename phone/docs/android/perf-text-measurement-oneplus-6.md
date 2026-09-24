# Text measurement and first-search performance — 24 September 2026

Device: ONEPLUS A6003, Android 15, ROM `octosense-202609240442`, slot `_a`,
1080 × 2280, approximately 59.8 Hz. These results follow the
[Home swipe fixes](perf-home-swipes-oneplus-6.md). The earlier drag-distance fix
remains in place.

## Cause and implementation

Makepad's `Cx::perf_monitor` and opt-in `phone.work` timings isolated the
remaining first-search CPU stall to result labels. Individual first-use label
draws cost 5.839, 4.215, 2.665, 2.491 and 1.003 ms. Matching and icon sections
did not cross the 1 ms logging threshold.

`ShellDraw::measure` called `DrawText::prepare_single_line_run`. That prepares
rasterized glyphs even though measurement needs only advances. Android uses
Makepad's vector glyph rendering path, so this generated unnecessary SDF glyphs,
including the sizes and candidate strings tested during elision. Measurement
now uses text layout and preserves the first-row width and `font_scale` behavior.

The shell also skips prewarming its own notification shade when floating
navigation or Android's system panel makes that shade unavailable. Overview
and group material preparation remains. This avoids doing unused work at launch.

New optional timings separate search matching, icons and labels. Input markers
now include floating-navigation touches before that handler returns, allowing
Recents tests to require a received Down event. Both kinds of instrumentation
remain off by default.

## Method and measured results

Use the existing `home/scripts/measure_android_frames.py` harness with
`--frame-markers --input-markers --seconds 2.1`, graph off, and
`makepad.TRACE=phone.frames`. Match submitted frames to the shell's active scene
markers and SurfaceFlinger's actual presentation times. Retain gaps within active
spans. Validate the expected scene and reject missing input markers or a changed
process. No screenshot capture or CPU sampling ran during pacing measurements.

Home swipes were 800 ms, `(800,1050) → (220,1050)` and the reverse. Search opened
with `(540,650) → (540,1100)`. Final Home samples comprise three fresh-process
left/right pairs and five warm pairs. First search was sampled in three separate
processes. Recents used the floating navigation panel: open panel, settle, then
tap Recents or Home; three pairs. These are short, approximately 19-interval
transitions, so one missed refresh materially changes their average and p95.

| Scenario | Runs | FPS range | Highest run p95 | Longest interval | Missed refreshes / intervals |
| --- | ---: | ---: | ---: | ---: | ---: |
| Baseline Home swipes | 6 | 57.98–59.82 | 16.92 ms | 33.63 ms | 7 / 390 |
| Final Home swipes | 16 | 58.00–59.81 | 16.89 ms | 33.48 ms | 15 / 1046 |
| Baseline first search | 3 | 57.50–57.72 | 16.78 ms | 33.45 ms | 6 / 160 |
| Final first search | 3 | 58.71–59.79 | 16.85 ms | 33.46 ms | 2 / 164 |
| Final Recents open/return | 6 | 56.80–59.81 | 33.47 ms | 33.47 ms | 3 / 113 |

Separate Makepad diagnostic captures, with the monitor enabled, show:

| CPU section | Before | After |
| --- | ---: | ---: |
| First search result draw | 21.357 ms | 3.896 ms |
| First search editor draw | 3.622 ms | 3.631 ms |
| Home channel launch-window maximum | 35.53 ms | 9.78 ms |
| Shade channel launch-window maximum | 23.81 ms | 6.60 ms |

The first result draw used approximately 82% less CPU time. This is not an 82%
FPS increase: Home was already close to the display's refresh limit. The channel
peaks include multiple kinds of preparation; the shade change was not benchmarked
in isolation. These are sequential before/after captures, not randomized trials.
Battery was 100%, USB powered, approximately 28.4–29.2 °C during measurement.

Occasional single-refresh misses remain. Some have approximately 16 ms between
submissions but approximately 25–28 ms from submission to frame-ready; these
timestamps alone cannot isolate GPU execution, queueing, compositor scheduling
or power-state transitions. Whole-session startup draw maximum was still about
39.7 ms. This change does not establish uninterrupted 60 FPS during startup,
Recents, every hosted application, or every thermal condition. Scene timing also
does not measure hardware touch latency or verify changed pixels.

## Validation and provenance

- Release APK built and installed in place; app data retained. Only Home was
  installed from this build, preserving the separately updated OTA Bridge v2.
- All 308 app tests passed with `cargo test --locked --offline --bin octosense
  --features mobile-apps` from `home/` in the prepared build checkout.
- The new text regression uses real font layout at 2× DPI, two font scales,
  accented text, ellipsis, multiline text and empty input. It compares widths
  against the previous preparation path and verifies no glyph-atlas pixels are
  written during width-only measurement.
- Phone captures verify Home labels, filtered search, Cancel closing the keyboard,
  catalog labels with elision, no search bar in the catalog, and a right swipe
  returning from the catalog to Home.
- A separate idle check recorded six presents over six seconds. A control tap
  produced new frames on the same verified surface, ruling out a missing timeline.
- The phone was returned to Home with the graph and trace extras off. System
  update intent resolution still targets the new OctoSense update screen.

Installed Home version: `2026092403`. SHA-256:
`dea5e458e9109587a6f9bc16ffe425426dc7f003e447454ff29cc5805d2e5cc7`.
Platform certificate SHA-256:
`dd6926fe1599c6edbf4100c518c36652e90d4cd6b3c347bc1433f1ad3f8e787a`.

The installed APK was built from the existing development checkout so prior
Mail, Octos and camera fixes stayed on the phone. Its receipt explicitly records
`source_dirty=true`, source revision `b31cbe53b7725422763e9685076aa7ba33d50aa9`,
dependency revisions and patch hashes. It is not an APK built solely from the
performance commit. The isolated performance commit contains the four shell
source changes and this record; the other development changes were not staged.

Local evidence is retained under `out/home/perf-20260924/`: baseline/final frame
JSON, `diagnostic-detail.log`, `final/makepad-diagnostic.log`, test logs, captures
and `final/verification.json`. The APK and original build receipt are under
the build checkout's `out/home/perf-20260924-layout/`.
Early attempts using disabled edge gestures and an incorrect idle-layer name
are explicitly rejected (`*.invalid.json`) and excluded. Intermediate candidate
data is also excluded from the final results.
