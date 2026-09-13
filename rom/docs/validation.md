# macOS application-name validation — 2026-09-11

On `fix/macos-app-name`, `.cargo/config.toml` explicitly sets
`MAKEPAD_BUNDLE_NAME=OctoSense` and
`MAKEPAD_BUNDLE_IDENTIFIER=dev.makepad.octosense`. The pinned Makepad build script
otherwise derives its generated `Info.plist` name from the checkout folder.
Here that folder is still `makeos`, producing `Makeos` despite the correct
OctoSense window and menu strings in the application source.

Verified with isolated native launches:

- Before the change, the actual macOS menu bar reported `Apple, Makeos`.
- After locked debug and release workspace builds, both generated plists report
  `OctoSense` for `CFBundleName` and `CFBundleDisplayName`, and
  `dev.makepad.octosense` for `CFBundleIdentifier`.
- Both `cargo run --locked` and `cargo run --locked --release` reported
  `Apple, OctoSense` through macOS accessibility inspection. The application
  rendered without runtime errors and each test process exited normally.

Evidence is in `target/macos-branding/`, including build logs and the captured
menu titles. `NSRunningApplication.localizedName` reports the executable name
for these bare Cargo launches, so validation reads the actual menu bar instead.
Existing legacy state-path compatibility and historical provenance remain valid.

---

# OctoSense light appearance validation — 2026-09-11

On `feat/desktop-wallpaper`, OctoSense now supports the existing Light/Dark
appearance control. The light theme uses pearl glass, dark ink text and blue
accents, with the matching light Abyssal Currents wallpaper. Shell surfaces and
hosted widgets receive the same palette; existing dark theme files are preserved.

Verified:

- Locked all-features workspace tests: **219 passed**, including repeated
  dark/light/dark stylesheet reloads in one VM, shell palette restoration,
  browser appearance, and light text/selection contrast.
- Python maintenance tests: **49 passed**. Locked release workspace build passed.
- Native all-style smoke passed, including both OctoSense appearances, repeated
  wallpaper/style switches, Reference input and retained state, separate
  instances, fullscreen/workspace operations, failure handling and shutdown.
- A focused native probe clicked the top-bar appearance control in both
  directions, launched a fresh Reference app in each appearance, and preserved
  the existing app's counter when switching from dark to light.
- Reviewed native frames of light/dark windows, menus, calendar and notifications.
  Host/client logs contain no rendering errors and all test processes stopped.
- Upstream revision, mappings and original fork theme hashes remain unchanged.

Evidence: `target/desktop-wallpaper/light-smoke/` and
`target/desktop-wallpaper/light-toggle/`; test/build logs are alongside them.
The Android style was checked on desktop; no Android device build was run for
this change. Its animated background is preserved. Both native PNGs and their
full generation/edit prompts are documented in `resources/wallpapers/README.md`.

---

# Abyssal Currents wallpaper validation — 2026-09-11

On `feat/desktop-wallpaper`, the OctoSense desktop style now embeds the original
Abyssal Currents PNG and renders it with the existing Image widget's centered
crop-to-fill mode. The replaced SVG and its custom widget were removed; their
source provenance remains recorded. Android's animated background and Omarchy's
selected theme image are unchanged.

Verified:

- Locked all-features workspace tests: **217 passed** (the removed SVG widget's
  geometry test is no longer needed).
- Python maintenance tests: **49 passed**.
- Locked release workspace build passed.
- Native all-style smoke passed, including returning to OctoSense and Omarchy,
  retained app state/input, workspace/fullscreen operations, failure handling,
  and process cleanup. Captured frames show the new wallpaper behind glass
  windows and menus, and the existing Android-style background.
- Upstream status and original import mappings/hashes remain valid. Remaining
  fork theme files match their recorded hashes; replaced wallpaper provenance
  identifies both the old source and the new asset.

The first runtime run exposed an existing smoke-helper bug: the native remote
backend reports an input-frame presentation failure after applying the input.
Retrying that request delivered a click twice. The helper now waits for a
separate read-only grab instead of replaying clicks or keys. Regression tests
cover that ordering, window selection and failed-capture handling. Framework
and application input code were not changed.

Final runtime evidence: `target/desktop-wallpaper/smoke-verified/`. Earlier
traces and the pre-wallpaper baseline comparison are retained alongside it.
This task tested the Android style on desktop, not an Android device build.
The asset dimensions, hash and full generation prompt are documented in
`resources/wallpapers/README.md`.

---

# OctoSense rename validation — 2026-09-11

The application and Reference crate now build as `octosense` and
`octosense-reference`. Shell labels, the custom desktop style, resource paths,
logs, catalog entries, packaging and active documentation use the new name.
Existing desktop settings and model links remain accessible through the
`MAKEOS_HOME` fallback and existing `~/.makeos` directory; new installations use
`OCTOSENSE_HOME` / `~/.octosense`.

Verified on macOS with Rust 1.98.1:

- Locked Cargo metadata, all-features workspace check, **218 Rust tests** and
  **48 Python maintenance tests** passed.
- Locked release and debug workspace builds passed.
- Native release smoke passed across all eight styles, including OctoSense,
  with Reference input/state retained, failed-launch handling and child cleanup.
  Captured frames show the renamed shell and Reference greeting.
- Plain `cargo run` with the shipped catalog passed startup, wallpaper, app
  launch, pointer/text input, workspace/fullscreen, instance and shutdown checks.
- `cargo makepad android build -p octosense --release` passed. The generated
  manifest has label `OctoSense` and application ID `dev.makepad.octosense`.
  APK: `target/android/makepad-android-apk/octosense/apk/octo_sense.apk`.
  No ADB device was attached, so this rename was not tested on hardware.
- Upstream status and the already-current daily-sync path passed at
  `74b63be83e101ab3a28d3604df77e9662d50a833`. Original import mappings, source
  names and hashes remain intact; all three retained theme/wallpaper asset
  hashes match their renamed local destinations. Framework pins are unchanged.

Runtime evidence is retained under `target/octosense-rename/smoke-styles/` and
`target/octosense-rename/smoke-default/`. Existing shared-entry-point, duplicate
upstream-package and missing-custom-icon warnings remain. The earlier iOS
framework build failure below was not revisited for this rename.

---

# Upstream sync validation — 2026-09-08

> Historical record from before the OctoSense rename. Original names, commands and artifact paths are retained for traceability.

Target: `ae20efc51e6db2ad083d65b40a9beec726582f56`, read from the local sibling Makepad checkout. Previous baseline: `83a00d2801e4864c42c3a40e85186a8b1743fd84`.

The three conflicts were resolved as follows:
- `clients.rs`: retain the MakeOS catalog and its tests; include upstream Cargo progress parsing and its tests, plus the process-reaping wait.
- `main.rs`: keep raw child diagnostics separately from upstream's filtered build progress; initialize MakeOS state paths before upstream desktop styles.
- `shell/menu.rs`: keep supported operations and Quit MakeOS, include desktop styles and alternate menus, and filter their app shortcuts through the catalog.

Further integration fixes use named dependency fonts in the mobile surface, gate mobile tile background launches on the existing prewarming setting, and prevent missing catalog apps from appearing in mobile home/dock UI. The pinned widgets crate lacks two View methods called by upstream WM; `scene.rs` therefore owns its framebuffer cache, and `desk.rs` redraws the wallpaper's public quad while preserving its geometry. No framework source was copied or patched.

Verified in the isolated candidate: Cargo metadata, locked workspace check, **191 Rust tests**, **40 Python tests**, and locked release/debug workspace builds. Both native smoke modes passed: release host and exact `cargo run` with the shipped catalog. They covered startup without child apps, forwarded pointer/text input, workspace movement, fullscreen geometry, independent instances, closing individual apps, and shutdown cleanup. The release mode also covered a failed app launch and shutdown during an unfinished Cargo build.

Additional native checks switched through macOS, Windows, NeXTSTEP, Omarchy, iOS and Android styles, including repeated desktop transitions and the phone-size/desktop-size changes. Captured frames showed the new rendering path, named fonts, and catalog-only mobile home correctly; the same reference process and its state survived, and no extra apps launched. NeXTSTEP and mobile use their own menu/picker controls, which the supplemental check follows.

The import remains scoped to **86 files under `apps/wm/` plus the upstream MIT license**. Makepad dependencies and the reference app pin the same revision; other application source is not copied. Optional linked apps remain disabled by default.

Retained report: `target/makepad-sync/reports/20260908T180743Z-83mualj1/` (ignored), including `verification.log`, `comparison.txt`, native frames/logs, and the reviewed conflict candidate. The upgrade is left uncommitted for review.

---

# Initial extraction validation

Validated on macOS with Rust/Cargo 1.98.1 against Makepad commit
`83a00d2801e4864c42c3a40e85186a8b1743fd84`.

## Automated checks

- `cargo build --release --locked --workspace`: host and reference app built.
- `cargo test --locked --workspace --quiet`: 163 passed.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 24 passed.
- `python3 scripts/upstream.py status --source ../makepad`: all 70 imported
  file hashes and coordinated Cargo pins verified; 11 adapted files and 59
  unchanged files at the initial baseline.

The maintenance tests use disposable local repositories to check additions,
deletions, independent edits, conflicts, dependency pinning, staged verification,
write rollback, and concurrent edits. No upgrade to a newer real Makepad
revision has been performed.

## Native runtime checks

`python3 scripts/smoke.py` passed against the release build:

- Desktop starts with no child apps, using isolated state.
- Reference launches through Cargo into a MakeOS tile.
- Pointer input sent to the host increments the child's counter.
- Text input sent to the host updates the child's echo label.
- Moving to another workspace leaves the original workspace empty.
- Fullscreen expands the child's reported size and restores it afterward.
- A second reference process starts with independent counter state.
- Closing that tile reaps its process while the first instance survives.
- An invalid Cargo package produces a desktop notification and diagnostic log;
  the host remains responsive.
- Host quit reaps both a running app and the process group of an unfinished
  Cargo build, including its build-script process.

`python3 scripts/smoke.py --cargo-run --default-catalog` also passed from this
repository and from a temporary source copy outside the sibling Makepad layout.
The latter used its own target directory, seeded with compiled artifacts to
reuse the build cache. Cargo dependency caches were shared. The host and child
ran from the temporary project; no sibling Makepad checkout was needed.

Smoke tests run Cargo offline after initial dependency fetching, and set only
test isolation/remote-control settings. App registration and normal launching
require no such settings. App-provided frames were inspected for the desktop,
icons, fonts, hosted content, and failure notification. Test instances were
closed after verification.

Runtime inspection also found and corrected the inherited run-view area
selection: widget queries now follow the presented app surface after the
temporary startup backdrop stops drawing.

## Scope

Source development on macOS is verified. Linux/Windows code and optional linked
app modules are retained but untested in this milestone. Fonts and other
framework assets remain in Cargo's dependency checkout; executable-only or
installer distribution needs separate resource packaging.

## Daily sync automation — 2026-09-06

The new `sync` command was verified with 40 script tests, including local Git
revision transitions, review-branch handoff, cache reuse, conflicts, failed GUI
verification, concurrent edits, interruption cleanup, and failed report writes.
No-op behavior was also exercised against the real Makepad checkout, whose HEAD
still matched the recorded baseline.

The complete runtime verifier passed on an isolated source copy: metadata,
workspace check, 163 Rust tests, 40 script tests, both profile builds, release
hosting smoke, and `cargo run` with the default catalog. Its target directory
was seeded from existing compiled artifacts; both native test modes used that
copy's executables and retained PNG frames and host/client logs in the report.
This validates the orchestration at the current revision; it does not establish
compatibility with a newer upstream commit that has not yet been pulled.

## Fork WM feature import — 2026-09-08

Imported `guofoo/makepad` at `beb3857aea22a6a99fb4a7b6a3b60f92359f6a4d`, after first fast-forwarding MakeOS main to the completed sync at `8b2dc9c`. The published fork revision supplies all Makepad Git crates, including the Reference app's widgets. No framework files or unrelated apps were copied. The lockfile changes only Makepad source URLs and revisions.

Validation passed:

- Workspace Cargo check, 205 Rust tests, 44 Python tests, release/debug builds.
- Release native smoke with `--styles`: all eight styles, repeated MakeOS/Omarchy transitions, retained Reference state, input inside rounded glass windows, menus, calendar and notification captures.
- Pointer/keyboard forwarding, workspace movement, fullscreen restoration, independent instances, failed Cargo launch, and shutdown/reaping during an unfinished build.
- Exact `cargo run` with the shipped catalog and isolated state.
- All 88 pristine source hashes and coordinated Cargo pins; 73 imported files match the fork exactly and 15 retain documented local adaptations.
- Runtime host/client logs reject shader compilation failures, error logs and panics. Independent review found no remaining material issues.

The stronger smoke exposed inherited desk bookkeeping defects: its reported area came from the tiling-only border, and widget-tree refreshes could discard surviving dynamically hosted children after another closed. An explicit WidgetNode now uses the stable turtle area and enumerates hosted children. A failing child-enumeration regression passed after the fix, and native clicks in MakeOS glass incremented the surviving Reference counter. Script layout metadata, phone selection delegation and layer inspection were retained.

The smoke also waits for asynchronous child sizing before fullscreen comparisons and dismisses flyouts with their supported outside-click gesture. Earlier failed diagnostic runs are retained alongside the successful evidence.

Local evidence is archived under `target/fork-import-20260908/`, including `smoke-verified/` (glass/style/lifecycle), `smoke-default-final/` (exact cargo run), unit/build logs, and the original comparison. Captures are app-provided images. Test processes were closed and source checkouts were left unchanged.

Validation remains macOS process hosting. Optional linked app modules and other OS targets are retained without new runtime validation; the lean catalog intentionally has no assistant process. The imported assistant/OSD glass paths share the shell material implementation but were not separately exercised with real assistant or system-volume changes.


# Omarchy startup wallpaper — 2026-09-08

A clean MakeOS state directory reproduced the blank startup gradient: the default palette was bundled but its image lived only in the old Makepad state, while MakeOS wallpaper downloads are opt-in. The unmodified Tokyo Night winding-road image is now embedded (653,482 bytes), with its source revision, checksum and upstream license recorded in `resources/wallpapers/README.md`. Installed backgrounds still take precedence; embedded bytes do not prevent an explicit download of the full set.

Verified 205 Rust tests, 44 Python tests, and debug/release workspace builds. Two catalog tests were updated for the previously expanded default app list: Reference must retain its local manifest and independent-instance policy, and Cargo metadata validates the configured package/binary targets, including workspace-root manifests.

The native smoke first failed on the missing wallpaper before the fix. Updated startup checks wait for both the visible image widget and a detailed decoded frame, then retain the app-provided screenshot. Release smoke passed all eight desktop styles, repeated MakeOS/Omarchy transitions, hosted Reference input/state, failed launches and process cleanup. Plain `cargo run` with the default catalog also passed using isolated fresh state and no wallpaper download flag. No runtime rendering errors were reported.

Artifacts: `target/wallpaper-validation/` (ignored), with original reproduction in `before/`, final style checks in `styles-verified/`, and final default startup in `cargo-run-verified/`. The intermediate `styles/` failure was an invalid SVG visibility assertion, corrected to inspect the Omarchy raster path; its captured MakeOS SVG was visibly rendered.


# Shared local Qwen model — 2026-09-08

Configured this machine’s `~/.makeos/weights/Qwen3.5-9B-UD-Q4_K_XL.gguf` as a symlink to the existing `~/.makepad/weights/unsloth/Qwen3.5-9B-UD-Q4_K_XL.gguf`. The paths refer to the same 5,966,095,584-byte file. No model download, weight copy, source-checkout change, or app-code change was needed. The reusable setup is documented in the README; the machine-local link is outside Git.

An isolated MakeOS release instance launched the catalog’s AI app through the normal WM Cargo path. The provider showed `Local · Qwen3.5 9B · local only`; its log confirmed loading the linked GGUF after no fleet node answered. A short arithmetic prompt returned `4`. The prompt was injected through the hosted assistant’s own remote input endpoint. Two earlier attempts through host pane coordinates did not submit text, including after waiting for child sizing; pane input routing remains unverified by this check. The host and assistant exited after validation.

Artifacts: `target/aichat-validation/model-verified/`, including the assistant log, exact reply and app-provided frame. Earlier probe attempts are retained alongside it.

# Android library target — 2026-09-09

Reproduced `cargo makepad android build -p makeos --release` failing with a
duplicate `[workspace]` in its generated wrapper manifest. MakeOS's own manifest
was valid: the installed builder copied its existing workspace section and
appended another when adapting the binary-only package for Android.

An explicit library target now uses the existing `src/main.rs` entry point, so
the builder compiles it directly as an Android shared library. The desktop binary
continues to use the same source and owns its unit tests. Cargo reports that the
source is shared by two targets; this avoids moving or duplicating the imported
WM source. No builder or dependency-checkout changes were needed.

The exact Android release command passed and produced
`target/android/makepad-android-apk/makeos/apk/makeos.apk`. The desktop check
(`cargo check --locked --workspace`) and upstream provenance/pin check passed.
Missing launcher icons remain nonfatal packaging warnings. This check built the
APK; it did not install or exercise it on a device.

Logs: `target/android-validation/` (ignored), including the original failure,
successful Android build, desktop check, and provenance check.

# Platform startup defaults and native phone controls — 2026-09-09

MakeOS now chooses its startup shell using the compiled target OS: Android and
iOS enter their respective phone layouts before the first frame; desktop/web
retain Omarchy. Selection uses the existing style-switch path to configure phone
state, controls, icons and hosted-app styles together. Native mobile toolbars
use 48-point height and an 8-point left inset, reserve the reported safe area,
and do not act as desktop window-drag handles. Desktop caption geometry and
manual style switching remain available.

Two new regression checks failed before the fix: Android selected Omarchy and
native toolbar geometry was `(26, 84)` instead of `(48, 8)`. All 208 Rust tests
then passed. The Android release command produced an APK, installed on the
connected OnePlus 6T. Native ADB taps opened the style menu, toggled light/dark,
and opened the app drawer. The desktop `cargo run` smoke passed Omarchy wallpaper
startup, Reference pointer/keyboard input, workspace/fullscreen behavior,
independent instances, and shutdown cleanup.

Native testing also exposed vertically flipped phone home content: the pinned
GL backend normalizes render-target rows, while the compositor applies a legacy
Android flip. The Android-only shader override in
`src/makeos/android_rendering.rs` restores upright home/drawer content. This is a
local compatibility correction until the framework is updated; neither the
Makepad dependency pin nor its checkout was changed. Android hosted-app captures
and blur paths require separate coverage; the desktop Cargo app catalog is not
bundled into the APK.

The iOS policy is unit-tested, but the full cross-check
`cargo check --locked -p makeos --lib --target aarch64-apple-ios` stops in the
pinned Makepad `platform/src/os/apple/metal.rs` at lines 1092 and 1183, where
macOS-only module references are not guarded for iOS. No iOS runtime validation
is claimed. Both framework follow-ups are recorded in `BACKLOG.md`.

Artifacts: `target/mobile-startup-validation/` (ignored), with red/green test
logs, Android builds, iOS diagnostics, desktop smoke artifacts, and device frames
for startup, style selection, appearance and the app drawer.

# Launcher centering and bundled mobile apps — 2026-09-09

Opening Apps from the shorter root menu retained the parent's frozen top,
putting the larger list below the window. A regression test failed with that
position retained. Menu navigation now recenters, while searches keep their top
only while the card fits. Native screenshots confirm both the first app rows
and the final AI row remain inside the desktop window.

Android previously had no catalog on the device and linked no app modules.
Native mobile targets now link Reference, Sheets and Photos automatically and
derive their installed catalog from the module registry. Desktop still uses its
Cargo/process catalog by default. Partial home-tile catalogs now reflow instead
of reserving empty Clock/Weather slots and hiding Reference/Sheets icons.

Reference's counter and input view is shared between its standalone binary and
embedded module. Native testing also required correcting Android capture
orientation, registering bundled theme fonts inside module isolates, using the
instance's theme for its background, and forwarding touch presses to module
focus. Device taps incremented Reference and Android keyboard text appeared in
both its input and echo label. Sheets and Photos opened from the home screen.

Validation: 213 Rust tests with `--features mobile-apps`, 44 Python tests, the
Android release build, and the final `cargo run` desktop smoke passed. The smoke
covers launcher screenshots, Reference pointer/keyboard input, independent
instances, workspace/fullscreen operations and cleanup. Earlier attempts exposed
Reference script import/background issues and intermittent remote snapshot
404s; the final desktop run completed successfully. Provenance remains at the
same Makepad revision, and no framework checkout was edited.

Photos has no picture library bundled; its empty-library screen was exercised,
not image import or persistence. Sheets launches but its grid labels and narrow
toolbar need further work (`MOBILE-04`). The other 17 desktop apps still need mobile
ports (`MOBILE-03` in `BACKLOG.md`). iOS retains the previously documented
framework compilation blocker. Android blur/Recents coverage remains a separate
framework follow-up.

Artifacts: `target/launcher-mobile-validation/` (ignored). The successful desktop
run is `desktop-input-final/`; native screenshots record home, drawer, counter,
keyboard and app launches. `android-build-complete.log` records the final APK.

## Official work update, 2026-09-11

Baseline: official `makepad/makepad` `74b63be83e101ab3a28d3604df77e9662d50a833`.
All Git dependencies, including wm_api/wm_theme, use this revision. Both WM
libraries are unchanged from the previous fork pin and remain external.

- `cargo check --workspace --all-features` passed.
- `cargo test --workspace --all-features --locked --quiet`: 217 tests passed.
- Python maintenance tests: 48 passed, including the existing artifact tests
  and new bounded retry checks for explicitly unsubmitted remote requests.
- Release GPU hosting/style smoke passed all eight styles, repeated MakeOS
  transitions, pointer/keyboard input, independent instances, failure handling
  and process cleanup. Frames are in
  `target/upstream-20260911/smoke-styles-verified/`.
- Plain `cargo run` with the shipped catalog passed the native smoke, including
  centered launcher scrolling and Reference input. Frames are in
  `target/upstream-20260911/smoke-default-verified/`.
- Android release APK builds with the linked Reference, Sheets and Photos.
  ADB reported no connected device for this update, so this revision has not
  been installed or tested on hardware.
- iOS compilation remains blocked upstream: `ios.rs:520` calls the missing
  `Cx::recover_after_caught_panic`; `ios.rs:1598` calls the missing
  `IosApp::set_deferred_system_gesture_edges`. Desktop iOS-style smoke passes.

The native test exposed a new upstream retained-memory scan reading freed draw
list roots from retired pass slots. `src/makeos/retired_passes.rs` detaches only
those invalid passes, leaving live roots intact. The pool regression was
observed failing before the fix; all-style GPU verification passes afterward.
The diagnostic call stack is in `target/upstream-20260911/trace-tap/host.log`.

The starting dirty working tree is preserved in
`target/upstream-20260911/before/`; the integration diff and exact file list
are saved beside it. Source checkouts were read only.
