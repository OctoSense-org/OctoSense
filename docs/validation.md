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
