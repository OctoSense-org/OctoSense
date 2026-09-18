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

- [ ] **MOBILE-01 — P2: Bring up iOS.**

  The 2026-09-09 cross-check failed in Makepad's `platform/src/os/apple/metal.rs`
  and `ios.rs`; on the framework pinned since MOBILE-06 (`03091405`)
  `cargo check --locked -p octosense --lib --target aarch64-apple-ios` passes,
  with the mobile features and the News crate too.

  First run on 2026-09-17, iPhone 16 Pro simulator (iOS 17.0.1), built with
  the fork's tool: `cargo-makepad makepad apple ios --org=dev.makepad
  --app=octosense run-sim -p octosense --features mobile-only` (iOS needs
  the feature: `build.rs` turns `mobile_only` on for Android alone). The
  shell starts, applies the safe-area insets (top 62, bottom 34), draws the
  home page, and `--test-action launch-news` opens News with headlines
  fetched over the network; storage lands in `$HOME/.octosense/storage`
  inside the app container, so there is no iOS twin of NEWS-09. Packaging
  needed a tool fix: this crate builds `src/main.rs` as a lib and a bin,
  so the binary carries two identical font-asset manifests and the Apple
  packager refused the duplicate (fork branch `fix/font-manifest-lib-and-bin`).

  Touch and the reader are checked through `--test-action
  taps:<x>,<y>@<s>[;…]`, added the same day: Xcode 27 here ships no
  Simulator.app and `simctl` injects no input, so the simulator runs
  headless, and the action puts a finger down and up at a window point
  after a delay, through the app's own `handle_event`. With
  `launch-news` and `taps:200,376@6;31,129@12` a headline opens the reader
  (the WKWebView attached at the page rect, the article rendered) and Back
  returns to Today with the overlay gone.

  On the iPhone 16 Pro itself (iOS 26.6.1) the same evening, signed with
  a Personal Team profile minted by `xcodebuild -allowProvisioningUpdates`
  on a throwaway project: the first build crashed at startup with
  `EXC_BAD_ACCESS` in `PhoneSurface::script_new` — a main-thread stack
  overflow, since iOS gives the main thread 1 MB and the widget tree is
  built through nested `script_apply`/`script_new` calls (the simulator
  inherits macOS's 8 MB). `.cargo/config.toml` now links aarch64-apple-ios
  with a 16 MB main-thread stack; the shell then starts, and the home
  swipe, News and Photos work by hand. Two device-only faults followed:
  every storage write failed with `Operation not permitted` (the container
  root is not writable on a device; the simulator allowed it) — fixed in
  the fork's `feat/ios-bringup` by recording Application Support as the
  platform data directory, the NEWS-09 shape — and `http://` feed
  pictures were refused by App Transport Security — fixed with
  `resources/apple/Info.plist` merged into the bundle
  (`package.metadata.makepad.ios.info_plist`). With both, the log shows no
  storage or ATS errors, and by hand on the phone: feed pictures show,
  Today's headlines are there at once after a relaunch, and a headline
  opens the reader and Back returns. The transport-security exception
  was then narrowed to web content only, with feed pictures asked for
  over https (`feed.rs`). Not yet checked: rotation. Also seen: the
  AppCard banner and icon are off the first home page on the iPhone's
  shorter safe area (layout budget, to confirm), and the shell keeps its
  own light/dark toggle rather than the system appearance. The deep stack
  at startup is MOBILE-07.

  Acceptance: rotation checked on the device; the fork's `feat/ios-bringup`
  (packager fix, iOS storage root) merged and re-pinned.

- [ ] **MOBILE-07 — P2: Startup builds the widget tree on a deep stack.**

  On an iPhone 16 Pro the first device build crashed with `EXC_BAD_ACCESS`
  in `PhoneSurface::script_new`, reached through nested
  `script_apply → on_after_apply → script_new` frames while the shell's
  widget tree was built: iOS gives the main thread 1 MB, and the build
  needed more. `.cargo/config.toml` links iOS with a 16 MB main thread,
  which is a workaround: the frames should not be that large or that
  deep. Measure the stack the build takes (a probe in `script_new`, or
  `pthread_get_stacksize_np` against the stack pointer at the deepest
  point), find the big frames (large structs built by value, likely
  `PhoneSurface`), and box or stage them so the default stack suffices.

  Acceptance: the shell starts on an iPhone with the linker's stack_size
  removed.

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

- [x] **MOBILE-05 — P1: An Android HOME intent stops the shell presenting frames.**

  On the OnePlus 6T a HOME intent delivered to the running activity
  (`adb shell input keyevent KEYCODE_HOME`, or the system's Home while an
  app is open) left the shell alive but blank: touches were still
  recognised but nothing was drawn until the process was force-stopped.
  Reproduced on 2026-09-17 with logcat and `dumpsys activity`: two
  `MakepadApp` records in one process. A launcher started by a plain
  component intent (`am start -n`, which the build tool and the reset
  recipe use) lives in a *standard* task, and Android never reuses a
  standard task for a home-type start, so the Home button created a second
  instance in the home task. The framework keeps one `Cx` and one surface:
  the new instance's surface was adopted, then the old instance was stopped
  and its `surfaceDestroyed` tore that surface down. This is why the
  `activityOnCreate` intent-extras pass ran after the press and
  `[phone] home intent` never followed (the HOME intent was the new
  instance's launch intent, not an `onNewIntent`). It did not reproduce
  while the running instance had itself been started by a HOME intent.

  Fixed on 2026-09-17 in the fork (`feat/news-reader-platform`,
  `5c5b6b443`): the newest `MakepadActivity` owns the native side and an
  instance it replaces is superseded — its surface and lifecycle callbacks
  no longer reach native, and it finishes — and `initChoreographer` no
  longer starts a second render loop. Checked on the device: force-stop,
  `am start -n`, HOME shows the home page and keeps presenting frames, one
  activity record remains, and a later HOME reaches `onNewIntent`. A
  launcher started as Android would start it (`am start -a
  android.intent.action.MAIN -c android.intent.category.HOME`, no `-n`)
  never hits the path at all.

  Acceptance: the fork revision adopted (MOBILE-06).

- [x] **MOBILE-06 — P1: Adopt the fork's `feat/news-reader-platform` revision.**

  The host's storage root (`src/octosense/paths.rs`, NEWS-09) and the News
  reader call framework APIs the pinned revision `d4502ef` does not have:
  `home::platform_data_dir`, `CxSystemBrowser::spawn_navigable` and the
  `NativeSystemBrowserPageError` action, with their Android activity and
  JNI side and the `news` app icon. They are published on the fork's
  `feat/news-reader-platform` branch
  (`5c5b6b443`, four commits on `d4502ef`),
  not on its `main`. Until the pin moves, this tree builds only against a
  `../makepad` checkout of that branch, and
  `tools/setup-native.py --check` rejects the checkout.

  The revision is pinned as a chain, so the manifests here cannot move
  alone: Octoscript's crates and Octoscript-Makepad (`runtime.json`, its
  `Cargo.toml`) name the same framework revision, and the runtime's verify
  step rejects an application manifest that names another.

  Done on 2026-09-17: the branch merged to the fork's `main` as
  `03091405` (OctoSense-org/makepad#12); Octoscript pinned to it at
  `68f65cd3` (Octoscript#31); Octoscript-Makepad released at `77c1e50b`
  (Octoscript-Makepad#24) naming both; `native-runtime.lock.json` and the
  makepad `rev` in the five manifests here moved to those revisions, and
  `python3 tools/setup-native.py --check --cargo-manifest Cargo.toml`
  passes with the siblings at the released commits.

## UPSTREAM-01: Remove retired-pass compatibility adapter

Makepad 74b63be8 `platform/src/draw_list.rs:485` indexes a freed draw list from
a retired pass slot in `prepare_retained_working_set`. Reproduced by desktop
style switching to iOS; the call stack is in
`target/upstream-20260911/trace-tap/host.log`. OctoSense detaches only passes with freed roots in
`src/octosense/retired_passes.rs` before GPU submission. Once upstream ignores
retired roots/slots, remove the adapter and rerun the all-style GPU smoke.

The iOS check is no longer blocked: on the revision pinned since MOBILE-06 the
iOS target compiles (MOBILE-01).

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

- [x] **NEWS-05 — P2: Check the web view plumbing on an Android device, and on iOS.**

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

  The fork revision is pinned since MOBILE-06 (2026-09-17). iOS checked on
  the iPhone 16 Pro simulator the same evening and on the phone itself
  (MOBILE-01): a headline opens the reader on the WKWebView inside
  OctoSense's window and Back returns to the list. The page-error report
  is Android-only at this revision: NEWS-10.

- [ ] **NEWS-10 — P3: The failure pane on the Apple backends.**

  A failed main-frame load is reported as `NativeSystemBrowserPageError`
  by the Android WebView alone, so on iOS and macOS a page that does not
  load leaves the reader's pane blank instead of showing the host, the
  reason and `Try again`.

  Acceptance: `webView:didFailProvisionalNavigation:` (and
  `didFailNavigation:`) on the Apple backends' WKWebView delegate reported
  as the same action; the failure pane seen on the phone with the network
  off.

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

- [x] **NEWS-09 — P1: Module storage was read-only on the phone.**

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

  The fork revision is pinned since MOBILE-06 (2026-09-17). Existing phones
  keep no state from before (it was never written).
