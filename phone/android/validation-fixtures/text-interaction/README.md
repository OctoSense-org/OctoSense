# Text and interaction consumer fixture

Ordinary public-SDK Activity using real Android TextView/Button behavior. It has
no setting-write permission or hidden native setters. `InteractionFixture` is a
DUMP-gated explicit receiver accepting only `state` or `reset_counters` for its
own in-memory test counters. Reads do not force-stop/restart the consumer.

The state reports actual Configuration weight, TextView typeface/size, framework
long-press duration, AccessibilityManager recommendations, rendered frame state,
and actual View click/long-click/mouse events with monotonic timing. The target
rectangle is measured from the real screen, so a driver need not guess pixels.

The raw provider map is separate from effective getters. High-contrast rendering
must be checked in a screenshot and native service dump: unchanged TextView paint
color does not prove the compositor ignored contrast. WMS scales need an
independent shell invocation of `IWindowManager.getAnimationScales()`; the API35
text dump omits these fields. `WindowAnimationObserver` exposes only that
read-only getter and runs from the disposable probe APK without changing either
app's permissions. Typeface weight and configuration
are independent observations, not assumptions that every canvas obeys a setting.

Long press: reset counters, wait for a focused drawn target, inject DOWN and
retain it until the real long-click callback (or a bounded timeout), then inject
UP. Compare elapsed callback time with the selected native duration. A short tap
must remain a click. Automatic click: register a temporary `uinput` kernel mouse, use relative motion
to reach the measured target, then wait for the generated mouse DOWN/click; never
inject a button event. UiAutomation motion injection bypasses the accessibility
input filter, so it cannot exercise AutoclickController. The mouse is closed and
its removal verified in a finally block. Off must not generate clicks during the same bounded window.

Run only on the assigned disposable emulator. Restore exact raw rows (including
absent/empty distinction) and remove this fixture after validation. The fixture
itself never writes those rows, assigns roles or requests permissions.

The shared Settings probe offers `text_interaction` (only disposable 5560) and
`text_interaction_unavailable` (only ordinary 5556). The runner refuses an existing
consumer package and restores all eleven exact secure/global rows in an outer
Python finally as well as the instrumentation finally. The DUMP receiver returns
base64 JSON with ordered-broadcast result code 1. Nothing has been device-accepted
merely by compiling these probes.

Android TextView.getTypeface() returns mOriginalTypeface. The rendered weight
comes from TextView.getPaint().getTypeface(); the fixture records both values.
Configuration changes call setTypeface, which adds fontWeightAdjustment to the
original weight before assigning the text paint.
