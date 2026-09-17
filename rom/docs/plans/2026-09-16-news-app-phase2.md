# News App Phase 2 Implementation Plan

> Execute task by task; each task lists its files, tests and commands. The
> "Accepted changes" sections, added as reviews land, record where the code
> deliberately differs.

**Goal:** Restyle the News app in liquid glass over an app-owned backdrop, make a tap open the article with a chevron revealing the summary, and open links in the bundled Browser when the host can launch it, else an in-app native web view reader, else the system browser, else a notification.

**Architecture:** The model gains a hosting-aware opener policy, source colours and the `wm_unavailable` envelope parser (pure, tested). The view is restyled with the framework's `glass.*` widgets and gains an `ArticleReader` widget that drives the platform's native web view overlay. The host gains a generic channel: a module root posts a `WmRequest` as a widget action and the module tile's host forwards it; an `Open`/`Launch` naming an unlaunchable app is answered with a `wm_unavailable` event to the requester; `Launch` forwards its arguments.

**Tech Stack:** as phase 1 (Makepad fork `ad8f3729d2c24ba5a3bda5c8865a2b8366480147`). No new crates.

**Design:** `docs/plans/2026-09-16-news-app-phase2-design.md`. Phase 1 context: `docs/plans/2026-09-16-news-app.md`.

**Verified API facts (ground truth at this revision; paths under `~/.cargo/git/checkouts/makepad-d00a25647c09e43e/ad8f372/`):**
- `WmRequest` (`libs/wm_api/src/lib.rs`) is `Clone + Debug + PartialEq + SerJson + DeJson`; every type that is `'static + Clone + Debug + Send + Sync` implements `WidgetActionTrait` (`widgets/src/widget.rs:1471`), so a view can `cx.widget_action(self.widget_uid(), WmRequest::Open{..})`. A `WidgetAction` has public `action: Box<dyn WidgetActionTrait>` and `widget_uid`; `wa.action.downcast_ref::<WmRequest>()` recovers it (`widget.rs:1496`).
- `makepad_wm_api::send(cx, &req)` sends from a hosted PROCESS only (`in_makepad_studio`); it returns false standalone.
- The host handles requests in `src/main.rs` `on_wm_request(cx, client, req)`; `WmRequest::Launch { app, .. }` currently drops `args`; `open_request` spawns via `preview::spawn_for_request` which appends the path as the app's last argument; `clients::find_app(id)` returns a catalog entry even when its manifest is missing; `crate::apps::is_launchable(&app)` is the real check.
- The host injects an event into a module root as in `src/mobile_app.rs` `send_face`: `enter_isolate(cx, vm_id); root.handle_event(cx, &Event::Custom(json), &mut Scope::empty()); leave_isolate(cx, entry)`. To a process: `crate::hub::send_to_app(sender, vec![StudioToApp::Custom(json)])` as in `send_wm_event`.
- The Browser app (`apps/browser/src/main.rs` `initial_urls`) takes every non-flag argument as a URL.
- Native web view: `cx.system_browser(id: impl Into<SystemBrowserId>)` gives `spawn(url)`, `set_url(url, replace)`, `update(area, visible)`, `history_go(delta)`, `close()`, `detach()` (`platform/src/cx_api.rs:99–200`). `SystemBrowserId(pub LiveId)`. Implemented on macOS (`apple_webview.rs`, attaches to the window owning the widget's draw pass; a `--stdin-loop` child has no window so nothing shows), iOS, Android (JNI); not Linux. NO platform emits `NativeSystemBrowserNavigation` at this revision, so there is no title/loading/error feedback. Pattern to copy: `widgets/src/web_card.rs` `draw_walk` (spawn once, then `update(area, visible)` every draw) and `detach` in `WebCardRef::detach`.
- Glass: `mod.widgets.glass.{NavBar, Card, Badge, Caption, Body, IconButton, Chip, GlassSegmented}` (`widgets/src/glass_panel.rs`). `GlassSegmented`: DSL `labels: [...]`; Rust `set_selected(cx, index)`, `selected()`, `changed(actions)`; there is NO `set_labels` (the field is private; Reminders' `set_labels` at `view.rs:1160` is a `DropDown`), so labels are applied with `script_apply_eval!(cx, tabs, { labels: #(labels) })`; accessor `self.view.glass_segmented(cx, ids!(x))` (Mail `view.rs:897`). Mail and Calendar use `glass.*` without a `glass.Layer` wrapper; Weather wraps only its floating buttons in `glass.Layer`. `glass.Badge` is a `View` with `draw_bg.color` (plain value) so `script_apply_eval!(cx, badge, { draw_bg.color: #(c) })` with a `Vec4f` recolours it. `glass.Caption`/`Body` hard-code light text, hence the app-owned dark backdrop.
- Icons: `Icon`/`IconRotated` with `draw_icon +: {svg: crate_resource("self:resources/icons/x.svg")}` and `icon_walk` (Weather `view.rs`); the crate ships `resources/icons/` at its root like Weather. The host's `resources/icons/*.svg` are MIT with the Makepad source; copy `chevron-down.svg`, `chevron-left.svg`, `close.svg`, `refresh.svg`.
- A backdrop gradient is a `View{show_bg: true draw_bg +: {pixel: fn(){...}}}` (Weather's header scrim).

**Rules for every task:** Do not commit. No author names, signatures or AI references. TDD for every behaviour change. If an API name here does not compile, find it in the referenced file; never invent one. Run cargo with generous timeouts. Report exact `test result:` lines.

## Tasks

Each task was implemented test-first and passed a spec review and a code
quality review; the sections that follow record where the code deliberately
differs from the original task text. The original step-by-step text with
its code blocks was removed once the code existed; the source is the
reference now.

### Task 1: Host channel

- Create: `src/wm_reply.rs`
- Modify: `src/main.rs`
- Modify: `src/module_host.rs`

### Task 2: Model additions

- Modify: `apps/news/src/model.rs`
- Modify: `apps/news/src/hn.rs`

### Accepted changes from the review after Tasks 1–2

- `source_color` and `NewsModel::source_color_for_label` (with a
  `USER_SOURCE_COLOR` const) are `pub` until Task 3's view calls them; Task 3
  narrows them to `pub(crate)`.
- `launch_app_with_args` with non-empty arguments skips warm-pool adoption
  as well as the focus-existing block, so the arguments reach a fresh
  process; a module-hosted app launched with arguments logs that they are
  not forwarded.
- `on_wm_request` uses match guards ahead of the existing arms; the
  `Open { app: None }` path is untouched.
- The crate's `WmUnavailable` also has `to_json`, for the tests, and parses
  leniently.
- `Headline` carries `source_id`, stamped by `NewsModel::complete` from the
  source's id and forced by `load_cache`, so every row on All and the tile
  knows its source; the badge colour is `source_color(&row.source_id)`.
  `source_color_for_label` was removed. Because micro_serde defaults only
  `Option` fields, `Headline` derives `SerJson` and implements `DeJson` by
  hand through a `HeadlineJson` shim whose `source_id` is optional, so
  phase 1 caches still load.
- `load_cache` clears a `discussion` that is not an http(s) URL; `hn.rs`
  builds the discussion URL only from a numeric id.
- `open_request` takes the requesting client and replies `wm_unavailable`
  when the spawn fails, not only when the pre-check fails.
- Catalog repair, unrelated to News but needed once the sibling makepad
  checkout appeared on this machine: the `route` entry's binary is `route`
  in both catalogs (both the pinned and the sibling checkout name it so).
  Commit it separately.

### Task 3: The restyle

- Modify: `apps/news/src/view.rs`
- Create: `apps/news/resources/icons/chevron-down.svg`
- Create: `apps/news/resources/icons/chevron-left.svg`
- Create: `apps/news/resources/icons/close.svg`
- Create: `apps/news/resources/icons/refresh.svg`

### Accepted changes from the review after Task 3

- `GlassSegmented` labels are applied with `script_apply_eval!` (no
  `set_labels`); the chevron rotates through `Button.draw_icon.rotation`
  (a `DrawSvg` field), not `IconRotated`; `glass.Badge` paints nothing at
  this revision (a plain `View` with no pixel function), so the badge, the
  tile dot and the hairline are local `RoundedView`/`SolidView` roles;
  `glass.Body`/`Caption` are wrapped locally with `padding: 0`.
- Card taps are read from the card's own widget uid (`ViewAction::FingerUp`
  with `was_tap`); the plan's `as_view().finger_up` one-liner would also
  work since `GaussRoundedView` shares its inner view's uid. Task 4 verifies
  a real tap.
- The Hacker News summary is `Posted by <author>` (the meta line already
  carries points and comments), superseding Task 2's wording.
- `source_short_label` gives the badge text: `HN`, `TechMeme`, the outlet
  or `Google`, a user feed's label cut to 12 characters, `Feed` when empty.

### Task 4: The opener and the reader

- Create: `apps/news/src/reader.rs`
- Create: `apps/news/src/test_support.rs` (the isolate scaffolding the view and module tests share)
- Modify: `apps/news/src/view.rs`
- Modify: `apps/news/src/lib.rs`
- Modify: `apps/news/src/module.rs`
- Modify: `apps/news/src/main.rs`

### Accepted changes from the review after Task 4

- `ReaderAction` derives `Default` with `#[default] None` (this fork has no
  `DefaultNone`), like `ButtonAction`.
- The reader's `page` is a `SolidView`; the reader tracks the page child's
  rect so its bar stays above the web view.
- `PortalList` ignores `set_visible` at this revision (phase 1's hide of the
  list was a no-op), so the list sits in a `list_box` view that `render`
  shows and hides.
- `ArticleReader::shutdown` detaches and then closes the native browser, so
  a torn-down module instance releases its web view.
- A `HostedViewMode::Tile` message closes an open reader (the native overlay
  would otherwise outlive the face); `open_link` is a no-op on the tile face.
- The meta line no longer names the source (the badge carries it);
  `error_line` is a fixed amber; `shorten` saturates.
- After a successful Browser launch the host sends no reply, so
  `pending_open` stays set until the next tap; only tests read it.

### Task 5: Docs, backlog, verification

- Modify: `README.md`
- Modify: `BACKLOG.md`
- Modify: `docs/plans/2026-09-16-news-app-phase2-design.md`

Run in order; every suite must pass, the release binary must build, and
`git status` must list only the phase 2 files (nothing is committed):

```sh
cargo test --locked --workspace
cargo test --locked --workspace --features mobile-apps
cargo test --locked -p octosense-news
cargo build --locked --release -p octosense-news
cargo clippy --workspace --all-targets
python3 -m unittest discover -s scripts -p 'test_*.py'
git status --short
```

Expected: modified `README.md`, `BACKLOG.md`, `src/main.rs`,
`src/module_host.rs`, `src/clients.rs`, `apps/news/**`; new `src/wm_reply.rs`,
`apps/news/src/reader.rs`, `apps/news/src/test_support.rs`,
`apps/news/resources/icons/*.svg`, the two phase 2 docs.

### Accepted changes from the final review

- F1: the native overlay only moved or hid when the reader drew or was told
  to, so a tile the desk stopped drawing (a hidden workspace, another tile
  fullscreen) left the web view painted over the new content. While the
  pane is open `ArticleReader` runs a watchdog on a half-second timer
  (`cx.start_interval`): each tick asks for a redraw and checks that the
  redraw it asked for last time drew the widget; one that did not takes
  the overlay off the window (`overlay_visible` false, the pane still open,
  the timer still running); the next draw puts the overlay back. The
  redraw per tick is what makes the check meaningful: the framework draws
  on demand. Close and shutdown stop the timer; a closed pane runs none.
  A first version watched every frame and redrew the tile at frame rate,
  which was too much GPU work for a static page; the timer costs two
  redraws a second while reading and may leave the overlay up to a second
  after the tile goes.
- Keyboard focus with the web view attached (NEWS-06): on this Mac, once
  the reader's WKWebView is on the OctoSense window, keyboard chords no
  longer reach the host until the reader closes (the workspace keys and
  the menu chord are inert; a synthetic modifier press stays down until
  released), while mouse clicks on Makepad-drawn areas still work. The
  workspace-switch reproduction of F1 could therefore not be driven from
  the keyboard; the fix is covered by the isolate test and the reader was
  checked on screen to stay visible while idle and to close.
- F2: an unlaunchable `Launch` gets the "App unavailable" notification as
  well as the `wm_unavailable` reply (a requester from before the envelope
  hears nothing else); `Open` stays reply-only. `WmRequest::Notify` shows a
  desktop notification instead of only logging.
- F3: `launch_argv` has a test that a launch's extra arguments come last,
  after the app's own; the design document's Testing section says what is
  unit-tested and what was verified on screen.
- M1: `open_link` refuses anything but an http(s) link.
- M2: `OpenTier`, `OpenPolicy` and `WmUnavailable::{parse, to_json}` are
  `pub(crate)`; `WmUnavailable` itself stays `pub` because the micro_serde
  derives parse only a plain `pub`; `ArticleReader::url` is test-only.
- M3: the README no longer quotes a Browser build time.
