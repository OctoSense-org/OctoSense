# OctoSense backlog

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
  Keep external app/framework sources out of tracked OctoSense files. Add coverage
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
  the OctoSense baseline, local fork HEAD, cached official tracking status, and fetch
  freshness when known. Clearly distinguish stale/unknown remote information,
  pending updates, and comparison errors. Summarize WM and non-WM changes without
  building. Keep fetch/pull user-controlled and make the check suitable for daily
  or more frequent invocation.

- [ ] **SYNC-04 — P3: Support verified conflict resolution and resume.**

  Current recovery requires resolving adaptations in OctoSense, committing with the
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

  Acceptance: `cargo check --locked -p octosense --lib --target aarch64-apple-ios`
  passes, followed by iOS startup, safe-area and touch verification.

- [ ] **MOBILE-02 — P2: Adopt the upstream Android compositor orientation fix.**

  The pinned GL backend already stores 2D render targets with top-left rows, but
  its compositor still requests an Android Y flip. OctoSense currently overrides
  the scene shader in `src/octosense/android_rendering.rs` to keep the phone home
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

- [ ] **MOBILE-05 — P2: An Android HOME intent stops the shell presenting frames (not reproduced since).**

  On the OnePlus 6T with the framework at `d4502ef`, a HOME intent delivered
  to the running activity (`adb shell input keyevent KEYCODE_HOME`, or the
  system's Home while an app is open) leaves the shell alive but blank: the
  JNI intent-extras pass runs (`android_jni.rs:685` logs the proxy state),
  the shell's own `[phone] home intent` line never follows, later gestures
  are still recognised (`gesture commit HomeUp` is logged) but nothing is
  drawn again until the process is force-stopped. Reproduced four times on
  2026-09-17, twice with only Photos or nothing open, so it is not the News
  module. The earlier note that the Home role worked was against the
  previous fork revision.

  Not reproduced later the same day on the fork's `feat/news-reader-platform`
  (`6973b68`) with the host at `0c1314d`, built with the fork's tool:
  seven HOME deliveries (from the home page, from the News reader with
  its web view attached, from Photos, three in a row, and after a forced
  in-process activity re-creation) each logged `[phone] home intent`,
  showed the home page and kept presenting frames. What the original
  report does establish: the intent-extras pass runs only from
  `activityOnCreate`, so in the failing runs the HOME press *re-created*
  the activity in the live process (the main loop quits on `Destroy`, and
  `android_entry` starts a second `Cx` in the same pid); that path was
  exercised here by a font-scale change (`settings put system font_scale
  1.15`) and drew correctly. No logcat of a failing run survives, and the
  fork tree carried uncommitted edits at the time, so the cause is not
  known. Diagnostics for the next sighting: capture `adb logcat -v time`
  across the press; this ROM logs `vendor.debug.egl.swapinterval` once per
  `eglSwapBuffers`, so a count of those lines is a frame counter, and an
  `android_jni.rs … proxy` line after the press means the activity was
  re-created rather than handed `onNewIntent`.

  Acceptance: a fresh reproduction with logcat, or the item closed after a
  week without one.

- [ ] **MOBILE-06 — P1: Adopt the fork's `feat/news-reader-platform` revision.**

  The host's storage root (`src/octosense/paths.rs`, NEWS-09) and the News
  reader call framework APIs the pinned revision `d4502ef` does not have:
  `home::platform_data_dir`, `CxSystemBrowser::spawn_navigable` and the
  `NativeSystemBrowserPageError` action, with their Android activity and
  JNI side and the `news` app icon. They are published on the fork's
  `feat/news-reader-platform` branch
  (`6973b6850dac2e70abc0e5a0b1a230c99158d728`, three commits on `d4502ef`),
  not on its `main`. Until the pin moves, this tree builds only against a
  `../makepad` checkout of that branch, and
  `tools/setup-native.py --check` rejects the checkout.

  The revision is pinned as a chain, so the manifests here cannot move
  alone: Octoscript's crates and Octoscript-Makepad (`runtime.json`, its
  `Cargo.toml`) name the same framework revision, and the runtime's verify
  step rejects an application manifest that names another.

  Acceptance: the branch merged to the fork's `main`; Octoscript and
  Octoscript-Makepad released on that revision; `native-runtime.lock.json`
  and the `rev` of every makepad, Octoscript and runtime dependency here
  moved to the released revisions; `python3 tools/setup-native.py --check
  --cargo-manifest Cargo.toml` passes on clean checkouts.

## UPSTREAM-01: Remove retired-pass compatibility adapter

Makepad 74b63be8 `platform/src/draw_list.rs:485` indexes a freed draw list from
a retired pass slot in `prepare_retained_working_set`. Reproduced by desktop
style switching to iOS; the call stack is in
`target/upstream-20260911/trace-tap/host.log`. OctoSense detaches only passes with freed roots in
`src/octosense/retired_passes.rs` before GPU submission. Once upstream ignores
retired roots/slots, remove the adapter and rerun the all-style GPU smoke.

The iOS check on this revision is still blocked in upstream `ios.rs`: missing
`Cx::recover_after_caught_panic` and `IosApp::set_deferred_system_gesture_edges`.
This supersedes the earlier Metal compile diagnostics in MOBILE-01.

## UPSTREAM-02: PortalList ignores set_visible

At ad8f3729 `PortalList` keeps the `Widget` trait's no-op `set_visible`, so a
list is hidden only by wrapping it in a view (News keeps its list in a
`list_box`); drop the wrapper once the widget honours visibility.

## News app follow-ups

- [ ] **NEWS-01 — P2: Open links in the system browser on Linux, Android and iOS.**

  Phase 2's reader covers the phones, and phase 3 puts it first: where the
  platform has a native web view (macOS, iOS, Android) a headline opens in
  the app's own reader, so the `Cx::open_url` stub matters only for the
  system-browser tier behind `Open in Browser`, and it is the only tier left
  on Linux (no web view there, NEWS-04). `open_url` is a stub on Linux, Android
  and iOS in the pinned framework (`platform/src/os/linux/windowing_backend.rs`,
  `platform/src/os/linux/direct/linux_direct.rs`,
  `platform/src/os/linux/android/android.rs`, `platform/src/os/apple/ios/ios.rs`);
  macOS shells out to `open` and the web build uses the browser.

  Acceptance: implement `open_url` with `xdg-open` on Linux, an `ACTION_VIEW`
  intent on Android and `UIApplication.openURL` on iOS in the framework fork,
  adopt the revision through the normal sync workflow, and verify a News
  headline reaches the system browser on all three.

- [x] **NEWS-02 — P3: Edit user feeds in the app.**

  Done in phase 3 (2026-09-16): the Following page lists every source with a
  follow toggle, removes the person's own feeds, and adds one from a form; it
  writes `feeds.json` in the same shape and fetches the new source at once.

- [ ] **NEWS-07 — P3: Rounded corners on a hero's picture.**

  A section's hero draws its picture inset in the card: the framework's
  `Image` has no corner radius and a rounded view clips rectangularly, so an
  edge-to-edge picture would poke out of the card's rounded top.

  Acceptance: a rounded image draw (a radius on `DrawImage`, or a rounded
  clip) in the framework fork, and the hero's picture bleeding to the card's
  edges under its rounded corners, as Apple News draws it.

- [ ] **NEWS-08 — P3: Pictures for Hacker News and Google News stories.**

  Phase 3 shows a picture only when the feed carries one; Hacker News and
  Google News RSS carry none, so most of Today is text. Fetching each
  article's `og:image` was set aside as one request per headline.

  Acceptance: a bounded, cached page-head fetch for stories without a feed
  picture (first N visible rows, one small ranged request each, a per-link
  cache in the jail), showing the picture when it decodes.

- [ ] **NEWS-03 — P3: Open links in the running Browser instead of a new tile.**

  Every link the host hands to the bundled Browser spawns a new Browser tile:
  the Browser reads URLs from its arguments and has no message that navigates
  a running instance.

  Acceptance: a fork-side message for the Browser (a `WmEvent`, or a custom
  message of its own) that tells a running instance to open a URL, Browser
  support for it, and the host reusing an existing Browser tile for
  `Open { app: "browser" }`; a second headline then opens in the same tile.

- [ ] **NEWS-04 — P3: A reader on Linux.**

  The pinned framework has no native web view on Linux, so the reader tier is
  skipped there and a standalone News window on Linux can only notify.

  Acceptance: a Linux web view in the framework fork, `has_webview` true for
  it in `OpenPolicy::for_platform`, and a headline opening in the reader on a
  Linux desktop.

- [ ] **NEWS-05 — P2: Check the web view plumbing on an Android device, and on iOS.**

  Android checked on the OnePlus 6T on 2026-09-17: a headline opens in the
  Android WebView inside OctoSense's activity, placed at the reader's page
  rect, and Back returns to the list. GitHub pages first painted at about a
  third of the view: the activity enabled `setLoadWithOverviewMode`, and a
  page whose DOM overflows its declared `width=device-width` (412 CSS px
  wide, 1094 px of content) was zoomed out to fit the overflow (DevTools:
  `visualViewport.scale` 0.377; Hacker News and BBC stayed at 1). The fork's
  `MakepadActivity.ensureSystemBrowser` now turns overview mode off and
  enables pinch zoom without the zoom buttons; GitHub paints at full width
  on the device. The same session gave the reader a navigable web view
  (`spawn_navigable`: the default spawn, made for web app cards, cancels
  every hop on Android, so a redirector link never reached its article) and
  a failure pane: a failed main-frame load takes the overlay off and shows
  the host, the platform's reason and `Try again`, seen on the device with
  the network off. These changes are on the fork's
  `feat/news-reader-platform` branch, not in this repository (MOBILE-06).
  iOS is still open (MOBILE-01).

  Acceptance: the fork revision adopted (MOBILE-06); the same check on iOS.

- [ ] **NEWS-06 — P2: Keyboard focus while the reader's web view is attached.**

  On macOS, once the reader's WKWebView is attached inside an OctoSense tile
  (the News module, reader open), keyboard chords no longer reach the host
  until the reader closes: the workspace keys (⌘2 / Ctrl+Alt+2, Super+Tab),
  the menu chord (Ctrl+Alt+Space) and Super+wheel over the tile were all
  inert, and a synthetic modifier press (System Events `keystroke … using
  {control down, option down}`) stayed down until released with `key up`,
  so even plain clicks failed in between. Mouse clicks on Makepad-drawn
  areas kept working throughout (the reader's Close, the bar's dropdown), so
  the reader itself stays usable. The web view most likely becomes the
  window's first responder when it is attached, so key events never reach
  the Makepad view; the standalone News window shows the same pattern. In
  one capture the OctoSense window's traffic lights were inactive with the
  reader open, so the window itself had lost key status: the fix concerns
  key-window handling as well as the responder chain.

  Acceptance: the host or the platform returns key focus to the Makepad
  window while a native overlay is shown (or the reader offers a
  keyboard-free Close that always works, which it does today); verify ⌘W
  and the workspace keys with the reader open in the module tile, and that
  the overlay then leaves the window with its tile (the reader's watchdog).

- [ ] **NEWS-09 — P1: Module storage was read-only on the phone.**

  Every storage write on the device failed with `storage create directory
  failed: Read-only file system (os error 30)`, so the headline cache,
  `saved.json`, `hidden.json` and `feeds.json` never persisted. Two causes:
  the framework's native storage root is `$MAKEPAD_HOME` or
  `$HOME/.makepad`, and `HOME` is not writable for an Android app; and the
  host sets `MAKEPAD_HOME` for its own process to `paths::home()`, which on
  Android resolved to `/.octosense`. Fixed on 2026-09-17: the fork's Android
  backend records the app's files directory before `Event::Startup`
  (`makepad_platform::home::set_platform_data_dir`, used by
  `makepad_home()` when `MAKEPAD_HOME` is unset), and the host's
  `octosense::paths::home()` prefers `platform_data_dir()`. Verified on the
  device: no write errors, and a saved story survives a force-stop and
  relaunch. The host's theme choice is stored the same way, so the
  earlier "dark mode not persisted" report likely has this cause too
  (not rechecked).

  Acceptance: the fork revision adopted (MOBILE-06); existing phones keep
  no state from before (it was never written).
