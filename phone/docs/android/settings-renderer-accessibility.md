# Native accessibility preferences in the shared renderer

The trusted Settings service changes Android preferences. Home separately observes
those preferences through a read-only Activity adapter. This also covers changes
made outside OctoSense Settings, including ordinary APK installations that have
no authority to write the preferences themselves.

The observer watches high contrast and animation scale, receives Configuration
changes for font weight, and reads AccessibilityManager's recommended timeouts.
Unknown observations retain the last valid renderer value. Home handles font
weight Configuration changes without recreating the Activity. Preferences live
in the shared Makepad Cx, so built-in modules use the same renderer state without
rebuilding their views, styles, text editors or documents.

## Text

DrawText uses its existing distance-field glyph path when high contrast or weight
adjustment is enabled. A bounded, size/density-adjusted synthetic stroke supports
static fonts as well as variable fonts; it preserves shaping, glyph advances and
caret positions. This is a renderer weight adjustment, not replacement of each
app's font family with Android's typeface. Color emoji keep their original path.
High contrast uses black or white ink plus a contrasting outline while preserving
text alpha. Turning both options off restores the original analytic glyph path.

The default analytic curve shader stays unchanged. An initial experimental
outline implementation rescanned curves, and emulator rendering stalled during
first launch; Home2534 is not an accepted build. Home2535 removes those rescans.
Its first launches still exceeded the ordinary10s acceptance wait while new
shader programs were being cached. This initial-cache cost remains a performance
follow-up, rather than being counted as a successful cold-install test.

On disposable5560, a subsequent ready-checked2535 test observed the Display page
in2.587s. Bold, contrast and both options visibly changed the screenshot, retained
the same process and Activity identity, and restored a byte-identical PNG when
turned off. Settings rows were restored exactly. The visual samples and measured
results are in `out/home/settings-text-interaction-r2/validation/renderer-ready5560`.
No OnePlus device was used.

## Motion and interaction

Makepad Animator uses the observed native animator scale. Scale zero snaps new
transitions and finishes active finite and looping tracks at their target. The
launcher also settles autonomous navigation/group/page/shade/island transitions
without advancing scroll physics, gesture clocks, notification age or countdowns.
Finger-driven gestures still follow the finger. Scene crossfades and the thinking
indicator stop animating with reduced motion.

Android's existing View long-click path supplies Makepad LongPress events. The
new controls do not introduce a second gesture timer. Recommended timeouts are
observed separately; Settings results and launcher onboarding hints already stay
visible until user action, so they do not need a fabricated dismissal timer.

## Reproducibility and acceptance

`runtime-patches.lock.json` pins the combined
`patches/runtime/makepad-contained-apps.patch` against Makepad `1d3d383`. It
combines the contained-app runtime with the Settings accessibility and IME changes. An independent Git index reconstruction
verifies the complete patched tree before APK packaging.

Home2534's full510 app tests,20 relevant Java contract/IME tests, and three focused
platform/draw/animator tests passed before the shader-only2535 correction.
Further2535 native, ordinary-app, navigation and final regression results belong
in the text/interaction audit and emulator validation ledger. None of these
checks closes unrelated Settings parity rows or substitutes for hardware tests.

The2535 native consumer scenario passed555 checks and its final full app suite
passed510 tests. A privileged cold-navigation run subsequently exposed50 rejected
program cache entries and missed the Search deadline. An independent GLES30 test
reproduced a failed binary reload immediately after successful export; this is
not attributed to the text shader without evidence. With only Makepad's app cache
set aside, Display became ready in4.668s (host driver cache retained). The2536
candidate validates a freshly exported program and bypasses an unusable cache
for that GL loader instance. Candidate performance acceptance is pending.

The corrected2537 build passes privileged45 cold-entry checks over11 fresh
processes and33 editing/accessibility/navigation checks. The previous2536 isolated
build omitted the pre-existing manifest template from its source snapshot and
could not resolve the custom Settings entry; it is not accepted.2537 includes
that template, and its compiled manifest matches2535 apart from version/build
metadata. It was built from an isolated285-file capture to exclude in-progress
System Languages code.

On5560,2537 logs one rejected old binary and one failed capability probe per
process, then compiles from source. Bold/contrast still update with the same
process/Activity and restore a byte-identical image. With all58 app shader files
temporarily set aside, Display reached the ready state in4.763s; no new unusable
program files were written. All58 originals were restored afterward. The host
GPU/driver cache was retained, so this measurement is not a first-ever driver
launch. Phone/hardware cache behavior has not been tested.
