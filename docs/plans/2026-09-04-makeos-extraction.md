# MakeOS Extraction Implementation Plan — Discussion Draft

> Historical record from before the OctoSense rename. Original names, commands and artifact paths are retained for traceability.


**Goal:** Create a minimal MakeOS project in this directory that opens with `cargo run`, hosts compatible Makepad applications, and records every copied upstream file for deliberate updates.

**Architecture:** Copy the WM application boundary while keeping Makepad framework and protocol crates external at one exact Git revision. Preserve the existing process and module hosting mechanisms; put MakeOS app registration, startup policy, and state paths behind small local modules. Validate one hosting path end to end before expanding the app catalog.

**Tech Stack:** Rust/Cargo, Makepad widgets/platform, WM child-process protocol, optional Makepad app modules, a small Python standard-library maintenance script.

**Status:** User approved the proposed process-first milestone; implementation and macOS verification are complete. See [validation](../validation.md) for results. The original investigation, alternatives, and acceptance criteria below remain the design record.

## 1. Proposed scope and defaults

- Project root is `/Users/guofoo/git/mp/makeos`, without a second nested `makeos` directory.
- Package and default executable are `makeos`; one root Cargo package can also own a small workspace for reference apps.
- First validation target is this Mac. Preserve upstream desktop platform branches, but promise Linux/Windows support only after testing those platforms. Mobile, web delivery, and a Linux login-session compositor are later milestones.
- Start with an empty, usable desktop: tiling, focus, workspaces, launcher, themes, resizing, and lifecycle handling.
- Recommended first hosting milestone: separate compatible Makepad app processes, with a tiny local reference app proving input, rendering, and teardown. Keep module infrastructure available; select optional linked apps explicitly.
- Optional app modules default off. AI model execution, assistant autostart, background app prewarming, demo filesystem generation, and wallpaper downloads default off.
- Keep the wire/service crates already used by WM; their presence does not require the model engine. Avoid deleting established host interfaces merely to remove small dependency entries.
- MakeOS gets its own state directory, provisionally `~/.makeos`, and a `MAKEOS_HOME` override for tests/custom placement. Preserve protocol environment names used by compatible children.
- The built-in theme works without a network connection. Optional theme importing can remain an explicit action.
- Show only apps and controls that work in the selected build. Unavailable application shortcuts must produce a clear response, not a failed hidden build.

The user can revise these defaults during discussion. Once agreed, they cover routine implementation choices without repeated clarification.

## 2. What “host apps” currently means

| Mode | Existing mechanism | Consequence |
|---|---|---|
| Separate Makepad processes | `--stdin-loop`, a localhost hub, GPU surfaces, forwarded input, WM messages | Apps can be developed and added independently; keep host/client protocol revisions compatible. |
| Linked app modules | `AppModule`, per-instance script isolates, shared host runtime | Apps must export the module contract and be compiled into MakeOS; this is not runtime loading of arbitrary Rust shared libraries. |
| Arbitrary native desktop apps | Not the existing nested WM contract | Hosting unrelated OS windows would require a substantially different platform/compositor project. |

The WM uses Studio protocol crates internally but does not require running the Studio application.

## 3. Dependency approaches

| Approach | Benefits | Costs | Recommendation |
|---|---|---|---|
| Copy WM; pin external Makepad Git crates | Small maintained footprint, exact version alignment, no required sibling checkout | Cargo fetches the upstream repository; resource and root-patch behavior need verification | Use for the initial project. |
| Copy WM; use local `../makepad` path dependencies | Fast local experimentation with unpublished changes | Depends on machine layout and a moving checkout | Optional developer override only. |
| Copy WM and its transitive libraries | Can be made self-contained for source distribution | Much larger import and update burden | Reserve for a demonstrated dependency that cannot be consumed externally. |

Published crates can replace Git dependencies later when their APIs match the chosen WM revision. A version number in an upstream manifest alone does not establish compatibility with a registry release.

Cargo supports finding named crates within a Git repository and pinning them with `rev`. This reduces files maintained here; it does not promise a small Git download. See [Cargo dependency documentation](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#specifying-dependencies-from-git-repositories).

## 4. Import boundary and layout

Starting source snapshot: `83a00d2801e4864c42c3a40e85186a8b1743fd84` on Makepad's `work` branch. The local `origin/work` reference contains it; actual remote fetchability remains to be tested.

At inspection, `apps/wm` has 69 tracked files totaling 881,317 bytes: 27 Rust files, 41 SVGs, and one Cargo manifest. Rust files total 23,057 lines, including existing tests.

```text
makeos/
  Cargo.toml                 # MakeOS package/workspace, pinned dependencies
  Cargo.lock                 # MakeOS's own resolved dependency graph
  src/                       # WM source, preserving its filenames/layout
    makeos/                  # Small local policy/catalog/path modules
  resources/icons/           # WM's existing SVG assets
  apps/reference/            # Tiny process-hosting example, if process-first
  config/apps.json           # Default app registrations, if process-first
  upstream/makepad.json      # Revision, file map, baseline hashes, decisions
  scripts/upstream.py        # Status/diff/update against an explicit revision
  scripts/test_upstream.py   # Three-way sync and conflict fixtures
  LICENSES/Makepad-MIT.txt   # Original notice from the copied source
  README.md
  docs/plans/
```

No framework sources, Makepad root workspace, source repository `local/` data, Studio app, downloaded wallpapers, model files, or unrelated applications are included in the default copy set.

External direct crates:

- `makepad-widgets`
- `makepad-studio-protocol`
- `makepad-network`
- `makepad-wm-api`
- `makepad-ai-services`
- `makepad-strict-json`
- `makepad-app-module`
- `makepad-wm-theme`

Use the same Git URL and full revision for these and any optional app crates. Keep Makepad-provided transitive dependencies external. Inspect the resolved graph before adding or removing root patches: upstream currently patches `bitflags`, `smallvec`, and `windows-link`, and dependency-root patches are not inherited by a new application root. See [Cargo patch documentation](https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html#the-patch-section).

Retain WM filenames and avoid a broad symbol rename or formatting pass. Record the upstream Cargo manifest as an adapted file, rather than copying the original monorepo paths into the new manifest.

## 5. Required adaptations

| Source area | Current assumption | Planned change |
|---|---|---|
| `Cargo.toml` | Relative paths into Makepad; three optional apps enabled | Pin shared Git crates, choose lean defaults, set default executable. |
| `src/shell/ui.rs` | Four font references escape through `../../widgets` | Use named widgets-crate resource paths, e.g. `makepad_widgets:resources/jetbrains_mono_variable.ttf`; verify fonts at runtime. |
| `src/clients.rs` | Makepad-root discovery, curated package paths, sibling executables | Resolve explicit app registrations in MakeOS, retaining launch and cleanup mechanics. |
| `src/main.rs` | Assistant autostart, prewarming, wallpaper fetch, demo defaults | Route startup through explicit MakeOS policy; ordinary startup launches only the desktop. |
| `src/theme.rs`, `src/host.rs` | Shared Makepad state paths | Use MakeOS state locations and retain theme compatibility for children. |
| `src/apps.rs`, launcher/menu actions | Linked presence can imply availability even when selected mode is Process | Check the selected mode and its real prerequisites together. |
| User-facing titles/menu | `wm` / `makepad-wm` | Brand the application MakeOS without renaming protocol identifiers. |

The mention of `config/omarchy/shell.json` in source comments does not establish a runtime dependency: no such directory exists in the inspected checkout. The default theme and shell tokens are already defined in Rust.

## 6. First hosted app and registration

If the user chooses process-first, provide a tiny reference application under `apps/reference`, depending on the same pinned widgets crate. Give it a text input and a visible counter so forwarded keyboard and pointer input have observable results. Its only purpose is to supply a runnable, understandable example of the hosting contract.

The app catalog should accept an explicit executable or a development Cargo manifest/package/binary tuple, argument list, label, and launch policy. Resolve relative paths against the catalog location, not an assumed Makepad checkout. Use argument arrays rather than shell command strings. Duplicate ids and malformed launch specifications should report actionable errors.

For the developer workflow, `cargo run` at the root opens MakeOS immediately. Selecting the reference app builds and starts its explicit manifest through the existing asynchronous child-launch path, showing progress in the tile. Cargo dependencies alone do not build application binaries. Document this first-launch build, and verify the launcher cannot accidentally select the MakeOS host again.

If the user prefers module-first, link `makepad-sheets` with defaults disabled and explicitly select Module hosting for Sheets. It already exports `SHEETS_MODULE`; no Sheets implementation copy is needed. This gives a functioning app from the initial host build. Test its teardown and two independent instances.

If both modes are required for the first milestone, retain the reference process app and Sheets module and test both. Do not silently expand the initial catalog to every upstream application.

A full Terminal app is a useful follow-up, but its library currently exports terminal components rather than its complete application entry point. Integrating the complete existing binary entails tracking its application wrapper/AI integration or introducing a deliberately smaller local wrapper.

## 7. Upstream tracking and updates

Keep one baseline manifest with the upstream URL, full commit, source-to-destination mappings, original content hashes, external dependency revision, deliberate omissions, and a brief list of local adaptations. Store original copyright notices. Use Git objects from the local Makepad clone or a disposable cache to obtain pristine files; do not check in a duplicate pristine WM tree.

Proposed maintenance interface:

```text
python3 scripts/upstream.py status --source ../makepad
python3 scripts/upstream.py diff --source ../makepad --to <commit>
python3 scripts/upstream.py update --source ../makepad --to <commit>
```

`status` and `diff` are read-only. `update` requires a clean MakeOS working tree and stages a three-way merge from old upstream, current MakeOS, and new upstream. It reports additions, removals, renames where identifiable, and modifications. It never overwrites unresolved local edits, never writes the source checkout, and leaves the baseline unchanged if a conflict or verification failure occurs.

Treat copied WM source and protocol/framework revision as a coordinated upgrade. A change to Makepad libraries can affect MakeOS even when no WM file changed. Report relevant dependency changes and run the same host/client smoke tests for each upgrade.

Resolve routine merge conflicts within the agreed design. Bring a conflict back for discussion only if it changes the selected product scope or hosting contract. Advance the baseline and lockfile with the successful merge so rollback is a single MakeOS change.

## 8. Implementation sequence and gates

### Task 1: Establish provenance and dependency fetch

Files: `upstream/makepad.json`, `LICENSES/Makepad-MIT.txt`, `Cargo.toml`, `Cargo.lock`.

1. Confirm the agreed source commit and tracked source content; import from that commit, excluding working-tree extras.
2. Verify the commit can actually be fetched from the canonical remote. If it is local-only, resolve that dependency-source decision before accepting a nonportable fallback.
3. Initialize the MakeOS repository if it is still empty and import the minimal file map.
4. Write the new package manifest with shared revision pins and selected features.
5. Run `cargo metadata --format-version 1` and `cargo tree -e features`; inspect framework duplication, unexpectedly enabled app/model features, and patches needed by this dependency graph.
6. Save the exact graph in MakeOS's own lockfile. Do not copy the monorepo lockfile.

Gate: dependency resolution succeeds from a separate project root; no committed sibling path dependency is needed.

### Task 2: Make the desktop independent of source layout

Files: imported `src/main.rs`, `src/shell/ui.rs`, `src/theme.rs`, `src/host.rs`; new `src/makeos/mod.rs`, `src/makeos/paths.rs`, `src/makeos/policy.rs`.

1. Introduce the MakeOS state and startup policy boundary.
2. Repair named resource paths and application titles.
3. Turn optional startup services and downloads off through policy; align visible controls with available features.
4. Add focused checks for state isolation and selected startup behavior; preserve existing upstream behavior tests that remain in scope.
5. Run `cargo check --locked` and `cargo build --locked`.

Gate: a build from this directory has no compile-time reference to the user's sibling checkout in MakeOS-owned configuration or paths. Dependency-cache paths are expected during source development.

### Task 3: Supply the selected hosting demonstration

Process files: `src/makeos/catalog.rs`, `src/clients.rs`, `src/apps.rs`, `src/shell/launcher.rs`, `config/apps.json`, `apps/reference/Cargo.toml`, `apps/reference/src/main.rs`.

Module alternative: root `Cargo.toml`, `src/apps.rs`, and launcher configuration for existing Sheets module support.

1. Implement the selected registration path with explicit launch modes.
2. Test relative/absolute manifest and executable paths, unavailable targets, duplicate ids, and the exact launch argument boundaries.
3. Ensure `cargo run` has an unambiguous root default target and hosted Cargo launches have an explicit package/binary.
4. Verify startup failure and child exit produce usable tile status; preserve group cleanup.
5. For modules, verify an unlinked id cannot be advertised as launchable and selected Module mode uses the linked instance.

Gate: one known app is available and launches through the intended host path without Studio or a sibling Makepad checkout.

### Task 4: Implement maintainable upstream comparison

Files: `scripts/upstream.py`, `scripts/test_upstream.py`, `upstream/makepad.json`, maintenance section in `README.md`.

1. Implement read-only status and diff first.
2. Test unchanged files, upstream-only changes, local-only changes, non-overlapping edits, conflicting edits, additions, and deleted locally modified files in disposable Git fixtures.
3. Add staged three-way update behavior and verify failures leave the live baseline and working files intact.
4. Test that mismatched source/dependency revisions and unexpected source files are reported.
5. Run `python3 -m unittest discover -s scripts -p 'test_*.py'`.

Gate: a synthetic upstream change can be reviewed and safely incorporated; a conflict is surfaced without lost edits.

### Task 5: Verify the actual user workflow

Files: targeted test fixes, `README.md`, finalized tracking manifest.

1. Run relevant retained Rust tests with `cargo test --locked`; adjust only tests tied to intentionally replaced checkout/catalog assumptions.
2. Run the exact root `cargo run` workflow with isolated MakeOS state and confirm the desktop opens without extra environment configuration.
3. Build `cargo build --release --locked`, then inspect that exact standalone binary with `--remote` for interactive runtime validation.
4. Launch the reference app/selected module; exercise typing, pointer input, resizing, focus changes, workspace moves, and closing/reopening it.
5. Exercise missing app/failed child startup and confirm the host remains responsive.
6. Check logs and an app-provided frame for missing icons, fonts, theme data, or child GPU-sharing errors.
7. Close the test host and verify its children are reaped. Use the app's own remote capture/quit endpoints for instances created by the agent.
8. Repeat the relevant workflow from a clean temporary copy outside the sibling checkout layout, with no Makepad-specific environment overrides. Verify offline runtime after the necessary dependencies and reference executable are built.

Gate: plain `cargo run` and the chosen hosting path are observed working, and no test instances remain. A successful `cargo check` alone does not meet this gate.

### Task 6: Finalize documentation and handoff

1. Document prerequisites actually observed on this Mac; do not assume nightly or a Makepad CLI is necessary.
2. Document normal run, hosted app registration, first-launch build behavior, optional features, and state locations.
3. Record the validated revision and any known platform-specific limitations.
4. Check the final tracked inventory for accidentally imported monorepo files or unnecessary application assets.

## 9. Acceptance criteria

- From this repository root, `cargo run` opens a correctly rendered MakeOS desktop.
- A fresh source checkout needs Rust and normal platform prerequisites plus dependency downloads, without a sibling Makepad checkout or manually copied runtime assets.
- Icons, international/default fonts, and a usable default theme resolve correctly.
- Ordinary application startup does not need wallpaper downloads, an AI engine, or unrelated child apps.
- The agreed hosted reference app works, receives keyboard/pointer input, resizes, and exits cleanly.
- Missing registrations or failed app launches produce clear feedback without breaking the desktop.
- MakeOS uses its own user state by default.
- Every copied upstream file and intentional adaptation is traceable to an exact commit.
- Relevant Rust tests, maintenance-script tests, and runtime smoke checks pass.

## 10. Known boundaries of this plan

- Investigation confirmed package structure and source behavior; it did not compile or run the extracted application.
- Fresh remote Cargo resolution and any inherited root-patch requirements remain early implementation gates.
- Source builds may read fonts/assets from Cargo's dependency checkouts. A relocatable `.app`/installer or executable-only distribution needs separate asset staging and packaging acceptance criteria.
- Other operating systems, arbitrary native-window embedding, dynamically loaded native modules, an app store/installer, and new isolation mechanisms are separate scopes.
- The consequential open choice is the initial hosting mode and sample app. The proposed defaults above cover the remaining routine decisions.
