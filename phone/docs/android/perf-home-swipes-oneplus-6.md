# Home swipe measurements — OnePlus 6, 22 September 2026

Device: ONEPLUS A6003, Android 15 OctoSense ROM, 1080 × 2280,
approximately 59.8 Hz. Release, platform-signed Home; existing app data retained.
The reported problem was horizontal Home page swipes.

## Cause and change

The recognizer capped movement at its 120-point commit distance. The pager
then mapped that capped value to 35% of a page. An 800 ms swipe therefore
continued presenting frames while its content stopped at page position 0.35.
The retained baseline trace contains a 318 ms stretch of identical positions
after movement had started. FPS alone did not expose this freeze.

`PageSwipe.progress` now carries signed distance divided by viewport width.
The pager follows the finger throughout the drag, retaining resistance at
the outer boundaries and the existing release threshold. Release chooses
the adjacent target before incorporating the drag, so crossing half a page
does not accidentally skip a page. Tests cover both directions, multiple
viewport sizes, reversal, cancellation, and settling.

An additional first-search stall was isolated with Makepad's frame monitor,
the optional `phone.work` timings, and a render-thread CPU sample. Opening an
empty editor synchronously inflated the 19 MB CJK and 10.6 MB emoji fonts.
The editor retains these fallback fonts but requests them on a glyph miss.
This avoids loading both during an empty search; the first use of those
scripts can still incur their loading cost.

## Measurement method

Use Makepad's `Cx::perf_monitor` and `PerfGraph` for diagnosis, enabled by
`makepad.APP_CONFIG='{"test_actions":["perf:on"]}'`. The retained diagnostic
log separates `home`, `event`, `draw`, and `wait` costs. Whole-session monitor
FPS includes deliberate idle periods and is not the swipe FPS.

Measure release pacing separately with the graph off and
`makepad.TRACE=phone.frames`. Join those scene markers to SurfaceFlinger's
actual presentation timestamps. Require a received touch Down; reject
intercepted gestures, missing markers, and process changes. Retain gaps
inside active spans, including stalls. No screenshots or CPU profiler run
during these samples.

The frame harness also saves scene positions and Down/Up markers. This
allows checking continuous movement alongside presentation cadence. Position
markers describe shell state, not pixel readback or hardware touch latency.

Reproduce from the repository root, with the phone awake and unlocked:

```sh
adb -s SERIAL shell am start -W -a android.settings.SETTINGS
adb -s SERIAL shell am force-stop dev.makepad.octosense
adb -s SERIAL shell am start -W -n dev.makepad.octosense/.MakepadApp \
  --es makepad.TRACE phone.frames
python3 home/scripts/measure_android_frames.py --serial SERIAL \
  --frame-markers --input-markers --seconds 2.1 \
  --command 'input swipe 800 1050 220 1050 800' --output page-left.json
python3 home/scripts/measure_android_frames.py --serial SERIAL \
  --frame-markers --input-markers --seconds 2.1 \
  --command 'input swipe 220 1050 800 1050 800' --output page-right.json
```

Starting Settings before force-stop prevents the ROM from restarting Home
without the requested trace extras. Allow startup to settle before sampling.
Repeat five pairs in one process and three pairs after fresh process launches.
Use `phone.frames,phone.work` in a separate diagnostic run to time search
styling, editor draw, and results.

## Results and retained evidence

Six accepted baseline Home swipes measured 58.0–59.8 FPS, p95 at most
16.79 ms, and maximum interval 33.49 ms. These apparently healthy numbers
coexisted with the 318 ms frozen page position.

Ten initial fixed Home swipes measured 58.0–59.8 FPS, p95 at most
16.79 ms, and maximum interval 33.47 ms. After movement began, no repeated
position lasted more than 17.7 ms. The page continued past the former 0.35
cap to approximately 0.53 for the same 580-pixel drag.

The final ROM APK (version 2026092219, SHA-256
`4e44846d635cee39dfe162c5f3f74b3f7ded0573a32c4b27a02e77154ab52b88`)
was installed and measured again: three fresh-process left/right pairs and
five warm pairs, all accepted. Across these 16 Home swipes, FPS was
57.1–59.8, every run's p95 was at most 16.80 ms, and the worst interval was
33.60 ms. Occasional missed refreshes remain; the long drag plateau does not.

Three first-search opens in separate processes measured 56.5–57.6 FPS with
maximum intervals of 33.50 ms, versus the baseline's 150.52 ms maximum.
One search run had p95 33.47 ms, so first-search pacing still misses the
20 ms p95 target even though the font-loading stall is removed.
In a separate Makepad diagnostic run, first editor draw fell from 143.4 ms
to 2.4 ms. First result rendering still cost 22.3 ms; this is separate from
the horizontal Home paging fix.

The measured APK and its 87 passing mobile tests include the preceding
catalog/search navigation changes alongside these performance fixes. The
APK receipt identifies that combined working-tree build. Both new pager
regressions pass; the frame harness compiles and `git diff --check` passes.
Before committing the performance changes independently, all 84 mobile
tests also passed in an isolated snapshot of the staged source.
On-device captures confirm the catalog has no search bar, right swipe returns
Home, pull-down search accepts text and filters results, and swiping back
closes the native keyboard. The phone was left running Home with diagnostic
traces/graph disabled; ADB was restored to its original non-root mode.

Raw baseline/candidate/final JSON, Makepad monitor logs, diagnostic stacks,
APK/build receipts, and validation captures are retained locally under
`out/home/swipe-perf/` (ignored build artifacts). Failed baseline attempts
intercepted outside Home were discarded, not counted as passing samples.
This is a measurement of these Home gestures on this device, not a blanket
60 FPS claim for every ROM surface.
