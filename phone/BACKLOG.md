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

  The fork's `feat/ios-bringup` (packager fix, iOS storage root) is merged
  (makepad#13) and pinned since 2026-09-18: framework `6e5898fe`,
  Octoscript `68f6a9df`, Octoscript-Makepad `2c9fe791`.

  Acceptance: rotation checked on the device.

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
  `9f0621b4b`): the newest `MakepadActivity` owns the native side and an
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
  reader call framework APIs the pinned revision `3a5ff12` does not have:
  `home::platform_data_dir`, `CxSystemBrowser::spawn_navigable` and the
  `NativeSystemBrowserPageError` action, with their Android activity and
  JNI side and the `news` app icon. They are published on the fork's
  `feat/news-reader-platform` branch
  (`9f0621b4b`, four commits on `3a5ff12`),
  not on its `main`. Until the pin moves, this tree builds only against a
  `.sources/makepad` checkout of that branch, and
  `tools/setup-native.py --check` rejects the checkout.

  The revision is pinned as a chain, so the manifests here cannot move
  alone: Octoscript's crates and Octoscript-Makepad (`runtime.json`, its
  `Cargo.toml`) name the same framework revision, and the runtime's verify
  step rejects an application manifest that names another.

  Done on 2026-09-17: the branch merged to the fork's `main` as
  `03091405` (OctoSense-org/makepad#12); Octoscript pinned to it at
  `117232cd` (Octoscript#31); Octoscript-Makepad released at `36e6ea19`
  (Octoscript-Makepad#24) naming both; `native-runtime.lock.json` and the
  makepad `rev` in the five manifests here moved to those revisions, and
  `python3 tools/setup-native.py --check --cargo-manifest Cargo.toml`
  passes with the siblings at the released commits.

- [x] **MOBILE-08 — P1: The phone shell flashes continuously (framework; fixed in the fork and pinned).**

  Seen on a Pixel 7 Pro (Android 17) on 2026-09-19: the screen alternates
  between a fully black frame, a half-drawn one (the wallpaper and one
  stray icon) and the complete home screen, for as long as the process
  lives, with nothing in the log. It is not a crash loop and nothing is
  drawn over the app.

  Cause: the fork's GL backend reserves each draw item's instance buffer
  against a GPU memory ledger (a quarter of the 1,536 MiB allowance) with
  a call that can be refused, and on a refusal leaves the item out of the
  frame and asks for a repaint. Over the limit every repaint is refused
  the same way. The Metal backend, and upstream's GL backend since
  2026-09-18, never refuse a draw item that is being drawn.

  How it was reached: a map app's tiles loaded (OctosMap, panned), then
  the activity's surface destroyed and recreated (leave to the system
  launcher and return; a sleep and unlock does the same). About seventeen
  seconds later, with no input, the re-upload pushed the ledger past its
  limit. Telemetry added for this read `reservation_refused` about 2,600
  times a second, and the process held about 1 GB of graphics memory. A
  first attempt to provoke it with a forced 256 KiB limit produced no
  refusals and proved nothing either way.

  Fixed in `OctoSense-org/makepad#16` (branch
  `fix/gl-skipped-draw-telemetry`): the one reservation call, plus a log
  line, at most once a second and only while it happens, that says why the
  GL loop skipped draw items. With it the same sequence three times over
  gives steady frames, no skipped items, and graphics memory flat at about
  360 MB. Pinned on 2026-09-19: `makepad#16` merged as `3c82c18f4`,
  `Octoscript-Makepad#31` named it (`8a7c6b50f`), and this repo's lock and
  six manifests moved with it; `Cargo.lock` did not change. The fix was
  verified on the phone with a build against the fix branch, whose tree is
  identical to the pinned revision's.
  Still worth a look afterwards: why the ledger goes over its limit after a
  surface is recreated at all.

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

At dd8562e2 `PortalList` keeps the `Widget` trait's no-op `set_visible`, so a
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

## OctosMap follow-ups

Left out of v1 by decision (`docs/plans/2026-09-18-octosmap-design.md`), or
found on the way. `docs/maps.md` describes what is there.

- [ ] **MAPS-01 — P2: Saved places and recent searches.**

  Home, Work and starred places, and the last searches, offered under the
  search field before anything is typed. They belong in the app's storage
  jail beside `state`.

- [ ] **MAPS-02 — P2: Nearby category chips.**

  Restaurants, Coffee, Gas, Groceries under the search bar, backed by an
  Overpass query around the map's centre, with a pin per result. The
  framework already has the query and its mirrors (`widgets/src/splash.rs`,
  `sys.places`).

- [ ] **MAPS-03 — P2: A wide home tile.**

  News and Photos draw one; OctosMap opens from the home grid and the App
  Library. A tile needs a `HostedView` with a `tile:` face in `MapsView`, a
  `TILE_APPS` entry and an `idle_text` arm in `src/mobile_tiles.rs`, and the
  home layout checked with a fourth wide tile. A commute line (`Home · 22
  min`) needs MAPS-01 first; a small live map is heavier than the other
  tiles.

- [ ] **MAPS-04 — P2: The assistant's tools.**

  `search_places`, `directions` and `start_navigation` on the module's
  `ServiceExecutor`, which declines every call today, so the assistant and
  the AppCard brain can drive the app instead of the L0 `nav` card.

- [ ] **MAPS-05 — P2: Spoken guidance.**

  The banner's text is the sentence to speak; the framework's route app
  speaks its own through `makepad-converse`, which is desktop-only there.

- [ ] **MAPS-06 — P3: Tap a point of interest on the base map.**

  `MapViewAction::PinTapped` fires for an overlay layer's pins only (the
  EV-charger layer); the base map's shops and stations have no tap target at
  this revision. A long press and the reverse lookup pick a spot today. The
  fix is in the framework's `MapView`.

- [ ] **MAPS-07 — P3: What `MapView` lacks.**

  No fit-to-bounds (the app computes the camera in `geo::fit_camera`),
  markers with no icon, label or selected state, and no satellite imagery.
  Each would be a framework change adopted through the normal sync workflow.

- [ ] **MAPS-08 — P1 before any release: services of OctoSense's own.**

  The tiles (`makepad.nl`), Photon and the FOSSGIS OSRM servers are public
  fair-use servers with no contract. The app behaves (a `User-Agent`, a
  debounce, one route at a time, cancelled requests, capped replies), but a
  released product needs hosted tiles, a geocoder and a router it may rely
  on. The URLs are constants in `apps/maps/src/{lib,places,routing}.rs`.

- [x] **MAPS-16 — P1: Adopt the fork's `fix/android-map-archive` branch.**

  Adopted on 2026-09-19. The four framework fixes OctosMap needs on a
  phone (MAPS-12 to MAPS-15) merged into the fork's `main` as
  `OctoSense-org/makepad#15` (`e7c1cdf6c`); `Octoscript-Makepad#28` named
  that revision in `runtime.json` (`e2d68f1d7`); and this repo's
  `native-runtime.lock.json` and six manifests moved with it. `Cargo.lock`
  did not change. The history rewrite later that day gave all of these new
  hashes, the ones written here, and #29 moved the pins to them: the lock
  now names `14fe992bf`, whose `runtime.json` names `e7c1cdf6c`. What the
  fixes mean for the other apps, which of them are candidates for upstream
  Makepad, the rewrite, and the steps of a pin move are in
  `docs/makepad-fork.md`.

  | Commit | Fixes |
  |---|---|
  | `5a9c20b0d` opengl: draw nothing for a pass that has no draw list | MAPS-13 |
  | `343053f8a` android: report a cancelled HTTP request and mark requests dispatched | MAPS-12 |
  | `136dea82a` opengl: bind compact vertex formats | MAPS-14 |
  | `d3d740808` map: the navigation layer clears only its own puck | MAPS-15 |

  Verified on the OnePlus 6T on 2026-09-18 with a release APK built against
  the branch, whose tree is identical to the pinned revision's: see
  `docs/maps.md`. The APK of the pinned build was run on a Pixel 7 Pro
  (Android 17) on 2026-09-19: the map, Locate and the puck, search, places,
  directions and a preview drive all work, with no panic and no skipped
  draws. Mail, Sheets and AppCard open. The AppCard nav card, the other
  user of the map, needs an APK with the assistant kernel bundled and has
  not been seen with these fixes.

- [x] **MAPS-12 — P1: The map draws no tiles on Android (framework; fixed in the fork, pinned by MAPS-16).**

  `MapView`'s HTTP archive reader (`widgets/src/map/archive.rs`) cancels
  its undispatched range requests when tile priorities change and queues
  the read again when the cancellation comes back as an `HttpError`; it
  marks a request dispatched on its first `HttpProgress`.
  `AndroidNetworkShimBackend::http_cancel` forgot the request and emitted
  nothing, and Android never sent a progress event, so the first camera
  move lost every read and no tile loaded. The framework's route app
  failed the same way. The fix emits the error from `http_cancel` and one
  progress event when a request is handed to Java, which cannot withdraw
  it.

- [x] **MAPS-13 — P1: Opening OctosMap froze the shell on Android (framework; fixed in the fork, pinned by MAPS-16).**

  About 0.3 s after the app opened, as its opening animation ended, the
  render thread panicked at `platform/src/os/linux/opengl.rs:1166`
  (`main_draw_list_id.unwrap()` on `None`) and the launcher kept its last
  frame: the app as a translucent card at about 97% of its size, which
  reads as "the app opens smaller than the others". The Metal backend takes
  a pass with no draw list with an `if let`; the GL backend now does the
  same, clearing the pass's dirty flag. With the fix the app opens in the
  same frame as News and Photos, and the log shows neither the panic nor
  the "no draw list" error, so the pass was a parentless one (the shell's
  never-drawn blur chain), as suspected. Recovery on a build without the
  fix: `adb shell am force-stop dev.makepad.octosense`.

- [x] **MAPS-14 — P1: Roads and area fills do not draw on the native GL backend (framework; fixed in the fork, pinned by MAPS-16).**

  Found once MAPS-12 let tiles load: the phone drew building outlines,
  labels and icons over a bare background, and logged `opengl: compact
  vertex formats are not implemented; skipping draw`. The map's roads,
  fills, faces and roofs use compact vertex records (half floats, shorts,
  normalized bytes); the GLSL generator already declares typed attributes
  for them and the WebGL backend binds them, but the native GL backend
  (Linux and Android) still assumed packed `f32` lanes and skipped those
  draws. The fix builds the typed attribute table and pointer calls there
  too.

- [x] **MAPS-15 — P1: A puck set with `MapView::set_puck` never draws (framework; fixed in the fork, pinned by MAPS-16).**

  The fork's navigation layer (`widgets/src/map/nav.rs`, the L0 `nav`
  card's) runs at the top of every draw and, with `nav_mode` off, cleared
  whatever puck was on the overlay. OctosMap's location dot and its
  guidance puck were therefore missing on every platform; the desktop
  verification on 2026-09-18 missed it. The layer now clears only the
  vehicle it placed itself.

- [x] **MAPS-17 — P2: No directions on Android 9: the public router speaks TLS 1.3 only.**

  `routing.openstreetmap.de` (and `router.project-osrm.org`, the same
  machine) refuses a TLS 1.2 handshake; Android's platform TLS reaches 1.3
  from Android 10. On the Android 9 test phone every route request ends in
  `Couldn't get directions · Secure connection failed` with **Retry**,
  which is the right thing to say. Photon and the tile host accept TLS
  1.2, so search, places and the map work there. The server answers plain
  HTTP too, and the manifest would allow it, but a route request carries
  both ends of a trip and stays on HTTPS. Confirmed on 2026-09-19:
  on a Pixel 7 Pro (Android 17) the same build gets its routes, and
  directions and the preview drive work there. A real drive is still
  unverified on any phone.

  Fixed 2026-09-23 with an Android service-only reqwest/rustls transport:
  TLS 1.3, normal WebPKI certificate validation, HTTPS-only requests,
  streaming body limits, timeouts and cancellation. The OnePlus 6T now
  fetches and draws driving, walking and cycling routes between SJC and
  SFO. All 106 Maps tests pass, including nine transport regressions.

- [ ] **MAPS-09 — P3: Route alternatives, more than one stop, transit.**

  OSRM answers `alternatives=true` and more than two coordinates; the model
  holds one route per mode between two ends. Transit needs another service.

- [ ] **MAPS-10 — P3: Offline regions.**

  The framework bakes a region with `map_build` and routes and searches it
  offline with `map_nav`; a bake needs a desktop (about 3 GiB free and
  minutes of CPU for one city), so a region would be baked there and copied
  to the phone.

- [ ] **MAPS-11 — P3: Keep the screen awake while navigating.**

  Nothing in the pinned platform holds a wake lock; the phone dims on its
  usual timer mid-drive.

## Second mobile sync follow-ups

Found in the review of the second sync from mobile on 2026-09-25
(`docs/home-migration.md`).

- [x] **HUB-01 — P1: Android placements reject `hub:` ids.**

  An installed App Hub app's launcher id is `hub:<manifest-id>`, but both
  placement validators accept only `[a-z][a-z0-9_-]{0,127}` for a hosted
  app: `LauncherPlacements.isHosted` (via `requireIdentity`) in
  `home/resources/android/java/dev/makepad/octosense/LauncherPlacements.java`
  and `hosted_identity` in `home/src/android_integration.rs`. Once a Hub app
  is on the home page, persisting a reorder, a dock drop, a hide or a folder
  (`pairs`) fails: `reorder` validates the whole list, so the extension answers
  `INVALID_ARGUMENT` (`home_placement_limit_or_identity`,
  `MakepadAppExtension.java`) and nothing is saved. Inherited from mobile's
  App Hub.

  Acceptance: both validators accept `hub:` followed by a valid manifest id
  and still reject other colons; change them in step; add `placement_tests`
  cases for a `hub:` id in `order`, `dock`, `hidden_hosted`, `pairs` and
  `hidden_tiles`.

  Fixed on 2026-09-25: `hosted_identity` and `LauncherPlacements.isHosted`
  accept a bundled module id or `hub:` and a manifest id as App Hub's policy
  admits it (1 to 64 of `a-z 0-9 . -`, not starting with `.`, no `..`). One
  table, `home/tests/fixtures/hosted_identities.json`, is checked by
  `placement_tests` and by `tests/test_hosted_identities.py` (the Java
  pattern), so the two stay in step; `pairs` and `hidden_tiles` use the same
  check. Not yet exercised on a device with an installed Hub app, since the
  public catalog is empty.

- [x] **HUB-02 — P2: Installed apps are read from disk on every lookup.**

  `installed_card_apps()` (`home/src/apps.rs`) lists and parses the install
  directory on every call. It is reached per frame through
  `clients::registry()`/`find_app()` (app labels in
  `home/src/mobile_surface.rs`, `home/src/desk/phone.rs`) and once per app
  in `AppRegistry::hosting()`.

  Acceptance: cache the list by data root and the
  `octosense_app_hub_app::icons` generation, so an install, update or
  removal (which bumps the generation) still shows at once.

  Fixed on 2026-09-26: `installed_card_apps()` lists the install directory
  once per data root and App Hub generation (`cached_installed_apps` in
  `home/src/apps.rs`). Every install or update reaches
  `App::installed_app_changed`, which bumps the generation. App Hub has no
  removal path yet; when one lands, it must bump the generation too.

- [ ] **HUB-03 — P3: `find_app("card")` answers the first installed Hub app.**

  `clients::find_app` falls back to matching the binary name, and every
  installed Hub row has `bin == "card"`, so `"card"` resolves to whichever
  Hub app is listed first.

  Acceptance: skip `bin == "card"` rows in the binary-name fallback, with a
  test.

- [ ] **HUB-04 — P3: Migrate persisted `appstore` ids to `apphub`.**

  Optional. The module id changed from `appstore` to `apphub` in the second
  sync; an old id stays in Android placements and in `wm/launcher.hides`.
  Unneeded while the public catalog is empty.

- [ ] **HUB-05 — P3: Small cleanups after the App Hub merge.**

  - `launch_module_as` (`home/src/main.rs`) returns silently for a `card`
    app without a `hub:` id; log why.
  - `apps::is_linked` (`home/src/apps.rs`) has no callers.
  - `bundled_catalog()` (`home/src/apps.rs`) repeats
    `retain(|a| a.id != "card")`, which `bundled_modules_catalog()` already
    does.
  - In the non-floating navigation branch of `home/src/mobile_surface.rs`,
    `android` is always false (Android uses floating navigation), so its
    band and pill conditions are dead.

- [ ] **CAL-01 — P2: Host the Calendar module from mobile PR #11.**

  Everything else in PR #11 (`feat/calendar-module`) is present; Calendar
  module hosting is not. Its source is in OctoScript-App-Design-Flow
  (formerly Octoscript-AppCard) at [`apps/calendar/native`](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/tree/cbbda4da0a9d0fbf13497335dd3342b71f35e71f/apps/calendar/native). Planned separately (Task 14 of
  `docs/plans/2026-09-25-sync-mobile-into-home.md`).

- [ ] **RUNTIME-01 — P2: `init_cx_os()` traps off the main thread on macOS 14.**

  Makepad's macOS `init_cx_os()` calls `AppleGameInput::init`, whose
  `+[GCController setShouldMonitorBackgroundEvents:]` starts GameController's
  legacy HID monitor; on macOS 14 that asserts the main queue
  (`dispatch_assert_queue`) and the process stops with SIGTRAP. The app calls
  it on the main thread, but libtest runs every test on a worker thread, so
  on the `macos-14` CI runner any test that calls it kills the test binary.
  Home, App Hub and News tests no longer call it. 29 of Maps' isolate tests
  need the start time only `init_cx_os()` sets (`seconds_since_app_start`),
  so CI skips Maps' `view::tests` and `module::tests` (33 tests,
  `.github/workflows/home.yml`); they all pass locally on newer macOS.

  Acceptance: `AppleGameInput::init` in OctoSense-org/makepad skips or
  dispatches its GameController setup to the main queue when called off the
  main thread; the runtime lock picks up that Makepad revision; CI runs all
  of `octosense-maps` without `--skip`.
