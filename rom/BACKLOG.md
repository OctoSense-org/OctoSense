# MakeOS backlog

Items from the upstream-sync review on 2026-09-09. All items below are pending.
The existing [sync workflow](docs/upstream.md) remains the starting point.

## Upstream sync

- [ ] **SYNC-01 — P1: Validate the external default apps in the staged candidate.**

  The live catalog resolves all 20 apps, but its relative manifest paths resolve
  only Reference in the candidate directory. Registry tests skip the missing
  manifests, and the default-catalog smoke only launches Reference.

  Acceptance: verification resolves external app sources at the frozen target
  revision without depending on a moving sibling checkout; asserts the expected
  app set is available; checks catalog packages/binaries and builds; and exercises
  representative external apps alongside Reference, including Terminal and AI
  Chat. Basic AI Chat hosting/input checks should not require model weights.
  Keep external app/framework sources out of tracked MakeOS files. Add coverage
  for candidate path resolution and missing expected apps.

- [ ] **SYNC-02 — P2: Merge upstream executable-bit changes.**

  Existing imported files always retain their local permissions. A reproduced
  upstream change from `100644` to `100755` was reported as unchanged, and sync
  advanced provenance while leaving the local file non-executable.

  Acceptance: compare and merge the Git executable bit alongside contents;
  preserve local-only permission changes; apply upstream-only permission changes;
  and report permission changes accurately. Add regression coverage for adding
  and removing execute permission, including simultaneous content edits.

- [ ] **SYNC-03 — P2: Add a lightweight, explicit update check.**

  Sync compares against local fork HEAD, while bare `status` compares against the
  recorded baseline. Neither establishes whether official Makepad has newer
  changes that have not reached the fork.

  Acceptance: provide a read-only check with human-readable and JSON output for
  the MakeOS baseline, local fork HEAD, cached official tracking status, and fetch
  freshness when known. Clearly distinguish stale/unknown remote information,
  pending updates, and comparison errors. Summarize WM and non-WM changes without
  building. Keep fetch/pull user-controlled and make the check suitable for daily
  or more frequent invocation.

- [ ] **SYNC-04 — P3: Support verified conflict resolution and resume.**

  Current recovery requires resolving adaptations in MakeOS, committing with the
  old baseline, and rerunning. Editing a retained candidate does not provide a
  supported path to resume validation and apply it.

  Acceptance: resume from an identified report and resolved candidate; verify
  the original live commit, branch, file state, and frozen target; reject
  unresolved conflicts; regenerate pins, lockfile, and provenance; and run the
  full verification sequence before applying to a review branch. Preserve
  failure reports and rollback behavior. Add coverage for stale candidates,
  concurrent edits, verification failures, and successful resolution.

## Mobile platform follow-ups

- [ ] **MOBILE-01 — P2: Restore iOS compilation in the pinned framework.**

  The 2026-09-09 cross-check fails in Makepad's `platform/src/os/apple/metal.rs`
  because two paths reference the macOS-only module on iOS. Correct this in the
  source fork and adopt a published revision through the normal sync workflow.

  Acceptance: `cargo check --locked -p makeos --lib --target aarch64-apple-ios`
  passes, followed by iOS startup, safe-area and touch verification.

- [ ] **MOBILE-02 — P2: Adopt the upstream Android compositor orientation fix.**

  The pinned GL backend already stores 2D render targets with top-left rows, but
  its compositor still requests an Android Y flip. MakeOS currently overrides
  the scene shader in `src/makeos/android_rendering.rs` to keep the phone home
  screen upright and its drawn controls aligned with hit regions.

  Acceptance: correct the framework's scene/blur texture orientation and verify
  hosted-app captures on Android; sync a published revision; remove the local
  shader override after native home, app-drawer, blur and hosted-app checks pass.

- [ ] **MOBILE-03 — P1: Extend the embedded mobile app catalog.**

  Native mobile builds now bundle Reference, Sheets, and Photos. The remaining
  17 desktop catalog entries require mobile-compatible embedded entry points;
  changing the desktop style alone does not port their Cargo/process hosts.

  Acceptance: add real `AppModule` implementations through external crates where
  possible, retain the shared framework revision, and verify launch, touch,
  navigation and storage on a device before adding each app to the default
  mobile catalog. Include Clock/Weather home tiles and account for platform
  services required by Browser, Files and Terminal. AI Chat additionally needs
  a mobile inference/provider setup; desktop Qwen model paths cannot be reused.

- [ ] **MOBILE-04 — P2: Finish Sheets and Photos mobile usability.**

  Both embedded modules launch on Android. Sheets still has missing grid labels
  and a toolbar sized for a wider viewport. Photos opens an empty-library screen
  and has no bundled picture library or verified mobile import flow.

  Acceptance: verify Sheets headers, cell text, editing and save/reopen on a
  phone; provide a usable Photos library/import setup; test portrait, landscape,
  appearance changes and persistence without a desktop checkout.

## UPSTREAM-01: Remove retired-pass compatibility adapter

Makepad 74b63be8 `platform/src/draw_list.rs:485` indexes a freed draw list from
a retired pass slot in `prepare_retained_working_set`. Reproduced by desktop
style switching to iOS; the call stack is in
`target/upstream-20260911/trace-tap/host.log`. MakeOS detaches only passes with freed roots in
`src/makeos/retired_passes.rs` before GPU submission. Once upstream ignores
retired roots/slots, remove the adapter and rerun the all-style GPU smoke.

The iOS check on this revision is still blocked in upstream `ios.rs`: missing
`Cx::recover_after_caught_panic` and `IosApp::set_deferred_system_gesture_edges`.
This supersedes the earlier Metal compile diagnostics in MOBILE-01.
