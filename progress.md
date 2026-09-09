# MakeOS progress

## 2026-09-04 — Scoping
- Ran the requested Superpowers bootstrap.
- Loaded planning-with-files and brainstorming instructions.
- Confirmed destination directory is empty.
- Created planning notes; application implementation has not started.
- Identified exact source revision, binary-only package, direct dependencies, optional bundled apps, and escaping font paths.
- Asked which app-hosting mode should define the initial milestone; investigation continues independently.
- Finished tracked file inventory and inspected launch policy, startup side effects, theme/resource loading, and reusable app APIs.
- Ran read-only Cargo metadata successfully. No compilation or GUI validation has been performed.
- Checked official Cargo dependency and patch documentation; live remote revision verification remains an implementation preflight gate.
- Wrote `docs/plans/2026-09-04-makeos-extraction.md` as a discussion draft covering dependency alternatives, import inventory, exact adaptation areas, reference app delivery, three-way upstream sync, and runtime acceptance checks.
- No implementation, GUI launch, source-checkout mutation, or Git initialization was performed.

## Implementation
- User approved the proposed first milestone.
- Re-read plan and loaded executing-plans, TDD, and worktree guidance. This new root has no existing Git history or code to isolate; implementing directly in the requested directory.
- Verified the pinned upstream WM manifest can be downloaded from GitHub successfully.
- Imported 69 WM files and the original license notice; wrote source mappings/hashes and pinned external dependencies. Cargo fetch/resolution succeeded.
- Added MakeOS app catalog parser, state paths, explicit optional startup flags, and reference app. Repaired named-crate font paths and trimmed visible menu to supported operations.
- TDD: observed four catalog/state tests and launch-binary selection test fail, then pass after implementation.
- Full Rust test run: 159 passed after adapting source-catalog fixtures and menu expectations.
- Release host and reference app built successfully. Launched host with isolated state and its remote control; frame shows desktop/bar/icons correctly, no missing-resource log messages. Closed that instance through /gq.
- Maintenance agent implemented safe staged upstream comparison/update, with 24 offline fixture tests passing.
- Independent review identified lifecycle/diagnostic issues; reviewer is implementing bounded fixes while main session prepares runtime verification and documentation.
- Review fixes completed: synchronous final client-group shutdown, group escalation after wrapper exit, and startup-failure notification/logging. Final Rust suite: 163 passed.
- Live GUI regression found the run-view widget reporting its retired startup backdrop's area. Marking the live surface area fixed inspection after startup; expanded native smoke checks passed.
- Release smoke verified pointer/text forwarding, workspace relocation, fullscreen geometry, two independent instances, individual close, failed Cargo launch, and host quit during a deliberately unfinished build. All observed process groups were reaped.
- Exact `cargo run` with the shipped catalog passed in the repository and in an independent temporary copy with its own target directory. Initial remote dependency resolution was online; subsequent smoke checks used offline Cargo with cached dependencies.
- Maintenance suite: 24 passed. Baseline status verified all 70 imports and Cargo pins: 11 adapted, 59 unchanged. Source checkout retains only its four pre-existing untracked example directories.
- Added README, runtime smoke script, validation record, and maintenance/conflict-recovery documentation.
- Final inventory: 90 project files, including the 70 mapped upstream imports. Git whitespace check passed. Both final smoke modes passed; implementation and validation are complete.

## 2026-09-06 — Daily sync automation
- User requested one command after their source Git update, automating steps 2–4 and leaving step 5 to them.
- Source HEAD still equals the imported baseline; the live checkout can exercise the fast no-op path.
- Existing updater stages safely but discards build artifacts and requires separate smoke commands. Extending it with a cached sync workflow and retained review reports.
- Added `python3 scripts/upstream.py sync`, using sibling HEAD, fast no-op, persistent candidate build cache, per-attempt reports, and unique review branches created only after checks pass. No automatic Git commits or source fetches.
- New regressions cover cache reuse, branch collisions, conflicts, failed checks, concurrent branch changes, lock contention, and CLI defaults. Full suite: 40 script tests passed.
- Review caught interruption cleanup and failure-archive errors bypassing branch restoration. Both regressions failed before fixes and passed afterward; bounded follow-up review found no additional material defects.
- Full real verifier passed on an isolated source copy with seeded compilation caches: metadata, workspace check, 163 Rust tests, 40 script tests, release/debug builds, and both native GUI smoke modes. Captured frames and host/client logs were copied into the run's report.
- Actual source HEAD remains at the pinned baseline. Live CLI sync correctly returned a no-op and reported these uncommitted automation edits separately. No upstream revision or Git history was changed.
- Updated daily usage and recovery docs. Automation changes remain uncommitted for user review.

## 2026-09-08 — Resolve upstream sync conflicts
- Synced from the local Makepad commit ae20efc5 in a disposable candidate; the live main branch stayed clean during resolution and verification.
- Resolved catalog, Cargo progress/diagnostics, startup style, and menu overlaps while retaining MakeOS policies.
- Independent review caught unavailable mobile app shortcuts and background tile launches; fixed and covered catalog menu/layout regressions.
- Adapted the WM-owned rendering cache to missing View APIs in the pinned external widgets crate, preserving the minimal source footprint.
- Locked workspace check, 191 Rust tests, 40 Python tests, release/debug builds, and both native smoke modes passed. Artifacts and compatibility details are recorded in docs/validation.md.
- Additional native style checks passed across desktop and phone layouts; the reference process/state survived with no background launches. Reviewed rendered frames for desktop, macOS, iOS, and Android.

## 2026-09-08 — Adopt fork WM features
- Fast-forwarded local main from f157660 to 8b2dc9c before feature work; only one completed sync branch needed integration.
- Imported WM changes at published fork beb3857a through a conflict-free three-way merge, retaining standalone policies and moving all external Makepad pins together.
- Added StyleSpec, MakeOS Liquid Glass, theme/material parsing, rounded process surfaces and bundled wallpaper. Replaced local rendering workarounds with the now-available widgets APIs.
- Recorded the fork baseline/default checkout; daily sync retains the same review handoff and now exercises all styles. Added regressions for source selection, nested worktree exclusion and runtime error detection.
- Native glass input validation exposed inherited desk geometry/dynamic-child discovery issues; fixed with explicit WidgetNode enumeration and a regression test.
- Verification passed: 205 Rust tests, 44 Python tests, both profile builds, all-style release smoke and exact cargo run smoke. Frames reviewed for glass windows, dock, bar, menus, calendar and notifications. See docs/validation.md.
- Preparing the verified import commit and local main integration; no pushes or source-checkout changes.

## 2026-09-08 — Omarchy startup wallpaper
- Preserving uncommitted full app catalog and README changes. Traced the missing image to the extracted app’s offline startup policy and absent bundled Omarchy asset.

- Native reproduction failed as expected: bg_image was hidden with a zero rectangle; saved blank frame in target/wallpaper-validation/before. Verified cached image Git blob matches Omarchy upstream, then embedded the unmodified file with source/license record. Installed discovery remains unchanged so explicit downloads are not suppressed.
- Inspection hiccups: unquoted URL/glob caused zsh errors; corrected quoting. Sandbox DNS required curl escalation. API response included image bytes; subsequent downloads saved directly to artifact files.

- 205 Rust and 44 Python tests passed. Updated two obsolete Reference-only catalog tests to preserve Reference and validate package/binary targets through Cargo workspace metadata; first edit used incorrect JSON helper names, corrected to the local parse/as_arr API.
- Exact cargo run and hosted Reference interactions passed. Startup capture raced asynchronous decoding, so the smoke now waits for a detailed rendered frame. A MakeOS SVG visibility assertion was invalid because its widget snapshot has no raster area despite the SVG drawing correctly; limited that new assertion to the Omarchy raster path. Native frames confirmed the SVG and later Omarchy raster render.

- Final verification passed: release all-style smoke (including repeated MakeOS/Omarchy, Reference state/input and shutdown cleanup) and exact cargo run with the full default catalog. Reviewed decoded startup and return-to-Omarchy frames. All test instances were closed. Source checkouts unchanged; catalog plus wallpaper changes remain uncommitted on main.

## 2026-09-08 — Reuse the fork’s local Qwen model
- Committed the app catalog and wallpaper changes as 03224eb on main; working tree was clean immediately afterward.
- Found the existing 5.6 GiB Qwen3.5-9B GGUF in Makepad state and linked it into MakeOS weights after filesystem approval. The fork already defaults to the Local provider with local-only enabled; no settings file was present.
- Verified that the hosted assistant selects Qwen and produces a local reply from the linked file. Model test used the child remote endpoint after two host-pane input probes did not submit text; no claim of verified pane input routing. All owned test processes exited. README/setup and validation documentation remain uncommitted.

## 2026-09-09 — Document local AI setup for contributors
- Added docs/local-ai.md and a README entry covering per-user weights outside Git, assistant source setup at the recorded revision, a pinned model download with checksum validation, existing-file reuse, custom paths and verification. Added ignore rules for GGUFs and partial downloads.
- Verified the installed GGUF SHA-256 matches the publisher’s pinned file. Checked all four shell command blocks with bash -n, checked ignore behavior and ran git diff --check. No model download, source build, GUI launch or personal-state change was needed for this documentation update.
