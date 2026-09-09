# Upstream sync validation — 2026-09-08

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
