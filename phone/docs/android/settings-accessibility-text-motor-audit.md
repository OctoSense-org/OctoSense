# Accessibility text and interaction controls

This slice is integrated in Home2535. Ordinary unavailable-capability and
navigation acceptance passed after shader-cache warm-up. Privileged preference
and independent-consumer acceptance passed555 checks on disposable5560. First-install shader
performance is explicitly not accepted by those warm-cache results.
The source is pinned Settings `0f0669fc699f70adb20fe6ed4b2ff1c600da6f86`
and framework `ff7620a38e54c5f7ec14a5b8ccc5be1ba41e2b1b`. Validation is emulator-only;
no physical phone is involved. The controls reuse the finite Controls boundary and the Agent's
existing secure-settings authority, with current owner/unlocked/caller checks.
No accessibility service consent, package grant, arbitrary provider key or new
permission is needed for the audited preference writes.

## Native semantics

| Control | Native source and stored state | Finite choices and qualifications |
| --- | --- | --- |
| High contrast text | `HighTextContrastPreferenceController`; Secure `high_text_contrast_enabled`, default 0, checked exactly 1; AccessibilityManagerService observes the same value. | Off/On. Preserve custom/malformed integers as unavailable; do not call them working On. Native Settings-only statistics logging is outside the adapter. |
| Bold text | `FontWeightAdjustmentPreferenceController`; Secure `font_weight_adjustment`; normal 0, bold `FontStyle.FONT_WEIGHT_BOLD - FONT_WEIGHT_NORMAL` = 300. | Off/On writes 0/300. A different numeric adjustment remains a custom observation. `ActivityTaskManagerService.updateFontWeightAdjustmentForUser` updates Configuration; a missing row uses `FONT_WEIGHT_ADJUSTMENT_UNDEFINED`, which means no requested adjustment, while the native Settings switch defaults Off. |
| Remove animations | `DisableAnimationsPreferenceController` and the synchronized Catalyst `RemoveAnimationsPreference`; Global window, transition and animator-duration scales. | On writes all three 0; Off writes all three 1. Missing defaults are 1. Mixed/custom nonnegative finite scales remain visible and unchanged until an explicit selection. Turning animations back on intentionally does not restore earlier custom scales, matching native behavior. Native read treats all non-positive values as disabled; negative/nonfinite malformed values should be unavailable in the bounded writer rather than silently normalized. |
| Touch & hold delay | `SelectLongPressTimeoutPreferenceController`, native `long_press_timeout_selector_values`; Secure `long_press_timeout`. | Short 400, Medium 1000, Long 1500 ms; missing native first choice and `ViewConfiguration.DEFAULT_LONG_PRESS_TIMEOUT` are 400. Preserve other nonnegative durations as custom; compare `ViewConfiguration.getLongPressTimeout()` after writes, allowing asynchronous core-setting propagation. |
| Time to take action | `AccessibilityTimeoutController`, `AccessibilityTimeoutUtils`; Secure interactive and non-interactive timeout rows. | Default 0, 10/30/60/120 seconds. Native writes **both** rows. Native radio selection reads interactive only; the built-in observation must also retain a differing non-interactive value. Service recommendations additionally consider enabled accessibility services; a saved value does not promise every application will dismiss content at exactly that time. |
| Automatic click | `ToggleAutoclickPreferenceController`, `ToggleAutoclickCustomSeekbarController`, `AutoclickUtils`; Secure enabled and delay rows. | Off or an explicit 200–1000 ms delay in 100 ms steps. Missing enabled is 0 and framework delay is 600. Choosing a delay enables automatic clicks; Off writes enabled=0 and delay=0, as the native Off radio does. Selection order is enabled then delay. |
| Large pointer | `LargePointerIconPreferenceController`, `InputSettingsObserver`. | Off/On only when `android.view.flags.Flags.enableVectorCursorA11ySettings()` is false. Unknown flag support is unavailable, not guessed. Native Settings reads any nonzero as checked but InputSettingsObserver applies only exactly 1; custom integers must not be represented as a demonstrated large pointer. No mouse is needed to save a preference; actual pointer behavior requires an input-device test. |

The native automatic-click radio array contains 0, 200, 600, 1000 and **2000**.
The last value is the **Custom editor sentinel**, not a 2000 ms action delay.
Its actual seekbar is bounded to 200–1000 ms. Native Settings stores selected
radio mode and its remembered custom delay in its own private preferences.
The adapter must not read or write those private editor records or advertise
them as global system state. A single finite Off/delay chooser can expose all
actual delays with authoritative enabled/delay observations. Native recovery
may retain its own prior editor-radio memory, independent of the actual delay.

There is no additional administrator restriction in these pinned controllers.
Do not invent policy checks that disable valid preferences because DND, an
external keyboard/mouse, or a particular accessibility service is absent.
Respect the existing current-owner/unlock guard and provider enforcement.
The pointer feature flag is a real native capability gate.

## Isolated backend and integration proposal

The shared `vendor/octosense/settings/src/dev/makepad/octosense/controls/AccessibilityTextMotorSettings.java`
has seven finite field identifiers,
a closed set of provider keys, and explicit finite choice enums. Keep raw
observations separate from write values: custom font weight, animation triples,
long-press duration, timeout pairs and automatic-click delay are observations,
not a route to arbitrary writes. A missing verified default is not a provider
read failure. Return unknown/unavailable for malformed data and retain the raw
rows without normalization.

Every action reads current state, verifies its capability and writable owner,
then re-reads before the first write. Recheck authority and expected raw state
before each subsequent write. The animation, timeout and autoclick sequences
are not atomic; a failure after one accepted write reports partial, and no
automatic mutation retry is allowed. Re-read all linked rows afterwards and
report requested while a native service observation is still catching up.
The consumer-facing preference outcome must distinguish a saved setting from
proven renderer/input behavior. Its standalone Java regression currently passes
600 assertions covering all finite choices, exact defaults/custom observations,
capability and owner changes, each linked-write failure position, preservation,
delayed service observations and no automatic retries. These files were excluded from the accepted caption-editor Home 2533 and are
now integrated for 2534. The native adapter also requires the actual service observations for contrast,
weight, animation scales and both timeout controls. Missing or lagging getter
results do not become a demonstrated On/Off value or an applied result.

The existing AOSP 35 framework capture (SHA-256
`f3a7bca4fadf18b9454d004f7aec9dea9a44cb197e61df80ed545a97e87b107c`)
has `AccessibilityManager.isHighTextContrastEnabled()`, whereas the pinned
framework uses `isHighContrastTextEnabled()`. The adapter recognizes
only these two native spellings; absent methods remain unavailable. Actual
DEX class definitions also confirm WMS animation scales, Configuration weight,
ViewConfiguration timeout, recommendation getters and the vector-cursor flag.
This verifies runtime descriptors, not native execution or rendering behavior.
Receipt: `/tmp/octosense-text-motor-runtime-framework.json`.

The built-in page is **Accessibility: text and interaction**, under System; its
finite external route is `accessibility_text_interaction`. Generic Android
Accessibility continues to open the native page. Seven fields use the existing
Controls command/event, with no new permissions or Binder transactions.
Four toggles and finite timeout/hold controls can reuse compiled Controls.
Automatic click uses a bounded chooser with 20 stable slots and all ten reachable
choices (Off plus nine delays). Touch/hold values use `hold:short|medium|long`;
timeouts use `timeout:default|seconds10|seconds30|minute1|minutes2`; automatic
click choices use `off` or `autoclick_delay:200` through `autoclick_delay:1000`.
Read-only custom observations are separately typed and never valid mutations. Dedicated
native recovery and public action routing need separate reviewed mappings;
do not redirect the entire generic Accessibility action to this subset.

## Required behavior acceptance

- Pure backend tests: exact missing defaults; all finite choices; malformed and
  custom raw values without writes; legacy pointer flag true/false/unknown;
  read-only and locked/owner denial; changed raw/capability between read and
  write; each position in multi-row failure; delayed native readback; preservation
  of unrelated rows and explicit partial results.
- Native descriptor/resource check before Agent capture (runtime descriptors checked): high-contrast getter,
  Configuration weight, WMS animation-scale getter, ViewConfiguration timeout,
  AccessibilityManager recommendation APIs, and the vector-cursor flag. Resolve
  any native resource arrays by trusted resource name rather than an inlined
  cross-build resource ID.
- Privileged emulator baseline/restore must preserve all exact raw rows,
  global animation settings, role holders, existing grants and package identity.
  Synthetic consumers should observe configuration/core-setting changes,
  recommended timeouts and actual automatic mouse click / long press timing.
  No physical device or existing app data should be modified for tests.
- High contrast and bold affect framework text rendering; a Makepad OpenGL
  canvas does not inherit those effects automatically. Home2535 observes native animation scale in the shared Animator and launcher
  transitions; finite/looping/reduced-motion regressions pass. Long press already travels through Android View's
  `onLongClick` and JNI to Makepad `Event::LongPress`; native View scheduling may
  already honor the global timeout, and needs an actual event-timing test before
  changing that path. The shared text renderer has screenshot proof and native input consumers pass
  on5560. First-install performance, full assistive-service interaction and
  physical hardware remain separate gates.

Read-only source receipts: `/tmp/octosense-accessibility-text-motor-native.txt`,
`-values.txt`, `-service.txt`, and `-effective.txt`. The first service search used
an obsolete framework path; the corrected audit reads
`frameworks/base/services/accessibility/java/com/android/server/accessibility`.

## Current source validation

The 600-assertion Java policy regression and 11 contract/backend tests pass.
Five new Rust model/UI tests, seven existing Controls/host tests, the actual
button-start touch swipe/up/tap regression including the new chooser, and the
150% narrow-page regression pass. The additional chooser accessibility-identity
and gutter regression also passes after correcting its test root-clip setup.
The coordinated full Home suite passed510 tests, relevant Java/IME20 tests and
three targeted renderer/runtime tests. Native Agent compilation against pinned
Android15 passed in6m03s and was installed on the disposable5560 clone with
identity/permission preservation. Home2534 and2535 artifacts retain their
separate failed-first-launch evidence and subsequent warm-cache receipts below.
Privileged preference/consumer execution passed555 checks; its evidence is
recorded below separately from the renderer and first-install performance gates.

## Acceptance harness

The shared public-API probe now offers `text_interaction` on the disposable 5560
clone and `text_interaction_unavailable` on ordinary 5556. Its independently
signed SDK-only consumer records actual Configuration/TextView weight, framework
long-press callbacks and elapsed time, AccessibilityManager recommendations, and
mouse events. A temporary kernel `uinput` mouse sends only relative motion when
testing automatic clicks; the generated click must come from Android. Ordinary
UiAutomation injection bypasses the accessibility input filter and cannot prove
this behavior. A shell-only, read-only `IWindowManager.getAnimationScales()` helper provides
independent service readback; the runtime35 text dump omits those fields. All eleven provider rows are restored exactly in
both the instrumentation and outer runner finally blocks, including null versus
empty. No existing app data is cleared.

The scenarios cover every finite offered choice, disabled capability behavior,
external custom/malformed values, retired picker nodes, nested Back and process
restart. Cold-entry coverage adds this page (11 launches / 45 expected checks);
caller-boundary coverage adds finite Controls snapshot/set requests for this page
(two extra checks, 60 with all existing selectors). Probe, consumer and boundary
sources compile against public SDK35. Native and renderer evidence must be kept
distinct: a saved pointer preference does not establish a physical mouse cursor's
appearance, and TextView paint color alone cannot establish high-contrast canvas
compositing. Root owns separate renderer screenshots and Makepad behavior checks.

### Ordinary Home2535 receipt

The ordinary5556 path passed27 unavailable-capability/native-recovery checks,
45 cold-entry checks across11 process launches, and33 accessibility/IME/navigation
smoke checks. The same signer, UID10208, data inode589826 and first-install time
were preserved; all eleven raw preferences are unchanged, fixtures removed and
crash buffer empty. Receipt:
`out/home/settings-text-interaction-r2/validation/ordinary2535-5556.json`.

First-install shader-cache performance is **not accepted** by those receipts.
The initial2534 and2535 launch attempts failed their10s pane deadline and showed a
translucent opening panel. Their failure logs/screenshots remain archived. A
separate repeated-launch readiness probe, after shader compilation, reached the
native pane in2406ms and passed23 read-only checks; only then were the ordinary
regressions run. No setting mutation was retried. Native seven-control/consumer acceptance is recorded below, separately from
renderer measurements and the open first-install performance issue.

### Privileged Home2535 receipt

The complete text/interaction scenario passed555 checks on disposable5560. It
verified all finite choices, actual TextView paint weight and Configuration,
three native long-press callbacks at400/1000/1500ms, short taps, both recommended
timeouts, all three WindowManager scales, automatic click Off/600ms through a
real kernel input source, stale/malformed/custom observations, restart and Back.
Large-pointer preference/readback is supported on this runtime; this alone is
not a physical cursor appearance test. All eleven exact rows were restored.
The temporary kernel mouse and both fixture APKs were removed. Both ordinary
and platform-signed unauthorized callers passed60 boundary checks each.

Earlier attempts are retained: r1 read TextView's original typeface instead of
its rendered paint; r2 lost a toggle during asynchronous readback and scrolling;
r3 expected fields removed from the WindowManager text dump; r4 injected mouse
events downstream of the accessibility filter. The final harness fixes those
observations without changing production settings code or replaying mutations.
Native success: `octosense-text2535-text_interaction5560-r5.log`.

### Renderer/startup follow-up2537

Corrected2537 passes45 cold-entry checks/11 launches and33 editing/accessibility
smoke checks on5560. Live bold/contrast preserve the process/Activity and restore
an identical PNG. A broken emulator shader-binary roundtrip is detected once per
GL loader, after which source compilation is used; working binary caches retain
the normal path. App-cache-empty Display readiness is4.763s with host driver
cache retained. The incomplete2536 manifest packaging attempt is retained as a
failed build, not a rendering result. See the renderer audit for source capture,
independent GLES proof and remaining physical/hardware limits.
