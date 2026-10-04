# Platform app icon shapes implementation plan

**Goal:** Keep each app identity while rendering one outer shape per selected shell style, including bundled and store icons.

**Architecture:** The shared shell owns icon presentation. Keep the framework's existing style-specific assets. Frame the App Hub and Assistant foreground artwork using the same style geometry. Pass the selected style into the installed-icon renderer and mask PNG and cached SVG textures with the same frame. A neutral backing gives transparent store artwork a consistent silhouette. Native Android package icons retain the mask supplied by Android. No runtime pin or icon-artwork redesign is needed.

**Tech stack:** Rust, Makepad SVG and image shaders, existing shell tests.

## Investigation

Assistant commit b6e554f3 intentionally introduced distinct speech-bubble artwork; it does not describe a platform shape exception. The shell reuses that SVG and App Hub's SVG unchanged for every style. Bundled Camera/AI providers and store icons bypass the styled catalog entirely. These are missing adaptations, not documented exceptions.

Android's launcher mask may vary by OEM. OctoSense's Android style already uses circles; native Android app icons remain system-rendered. Apple's iOS/macOS style uses rounded squares. Other styles keep their existing conventions (Windows soft corners, NextStep square tiles, Omarchy and Windows 2000 freeform).

Alternatives considered: drawing separate assets for every app cannot enforce future store icons; a GPU texture pass per icon adds avoidable work. Use shared geometry and direct shader masking. Visual review showed that SVG backing/fade composition requires flattening the layers first; SVGs therefore render to a bounded, size-bucketed texture once and reuse it across frames and styles. PNGs need no extra pass.

## Tasks

1. Run existing style tests as baseline; add regression coverage for App Hub/Assistant per-style silhouettes and watch it fail.
2. Add shared icon-frame policy in `crates/shell/src/octosense/icon_frame.rs`; reuse it for SVG backgrounds and the installed PNG/SVG renderer. Preserve the art, opacity, aspect ratio, cache invalidation and style switching.
3. Add rendering tests for square PNG/SVG, transparent artwork, corners and style changes. Verify the actual shader registration and rendered Android/macOS examples.
4. Add contributor rules to root/apps AGENTS.md and bilingual app docs. Explain artwork safe area, shell ownership and native icon exceptions.
5. Run the phone shell tests, both desktop checks, phone check, graph guards and source/native-app checks. Record real device/render evidence separately from unverified platforms. Review final diff.

## Validation results

Completed on macOS and the connected Pixel 7 Pro. All five tasks above are complete.

- From `phone/`, `cargo test --locked -p octosense-shell --features mobile-apps`: 867 passed, zero failures. The new App Hub/Assistant shape regression first failed against the previous implementation. Frame geometry, bounded SVG raster sizes and real shader input packing have regression coverage.
- From the repository root, `cargo check --locked -p octosense` and `cargo check --locked -p octosense --features mobile-apps`: passed.
- From `phone/`, `cargo check --locked -p octosense-home --features mobile-apps`: passed.
- Root `bash tools/check-shell-graph.sh -p octosense` and phone `bash ../tools/check-shell-graph.sh -p octosense-home`: passed.
- `python3 tools/setup.py --check --cargo`, `python3 tools/native_apps.py --check` and `git diff --check`: passed.
- Built the `icon_shapes` example from `phone/` with `mobile-apps`; captured its actual Metal output with hidden windows via `/g?scale=1`. All seven styles were visually inspected at 64- and 24-point sizes, including transparent artwork and opacity 0.5. `python3 tools/check_icon_shape_preview.py target/icon-shapes/platform-sheet-fixed.png`: 34 passed. The checker also rejected an earlier preview with unclipped SVG corners. The preview was stopped via `/quit`.
- Built the release Android APK with the pinned Makepad packager and installed it as the separate test package `dev.makepad.octosense.shapetest`, still named **OctoSense**. Verified App Hub, Assistant, Camera and AI providers on Home, plus YouTube's SVG on the second page. The final startup log had no fatal or shader-error markers. The existing normal package and default Home were preserved.
- Final code review found no remaining actionable issues.

iOS, Windows, Linux and OpenHarmony native builds/devices are **unverified**. Rendering their shell styles in the macOS preview is not native-platform validation. No packaging names, icon source artwork or runtime dependency pins changed.

## Android Back follow-up — 2026-10-04

The first custom-package APK rendered the icons correctly but did not load the Android application extension. The pinned activity looked only for `<application id>.MakepadAppExtension`, ignoring the explicit `dev.makepad.android.APPLICATION_EXTENSION` already in Home's manifest. In News, an edge Back swipe therefore left the test app for Pixel Launcher; the same gesture in the old normal package returned to OctoSense Home.

A small locked runtime patch now honors the manifest class and retains the conventional package fallback. A JVM test executes the real loader with Android metadata doubles; it failed with the missing integration before the fix and passes after it. It covers explicit selection, precedence, empty/absent metadata and ordinary apps without an extension.

- `python3 -m unittest discover -s rom/tests -p test_android_application_extension.py -v`: passed.
- From `rom/`, `python3 -m unittest discover -s tests` with the prepared Android 33 SDK selected: all 96 tests passed.
- Rebuilt the patched packager and release APK. Updated `dev.makepad.octosense.shapetest` to version code `2026100409`, still named OctoSense, preserving its data and the older normal installation.
- Pixel 7 Pro: left- and right-edge Back from both News and Photos passed (four cases). Captured screens confirmed the app opened and Home returned; pixel checks confirmed the Home icon grid and Android reported the test app still in the foreground. Pixel Launcher remains the default Home.
- Runtime setup/Cargo graph check and whitespace check passed; review found no actionable issues. The base dependency revisions remain unchanged; the runtime patch lock records the added loader fix.

This follow-up was not run on the OnePlus or other devices; those results remain **unverified**.
