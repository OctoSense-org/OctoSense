# News app, phase 2: look, tap-to-open, the bundled Browser

Phase 1 (`2026-09-16-news-app-design.md`) shipped a plain News app. Phase 2
changes three things, decided in the design conversation on 2026-09-16:

1. **Look**: liquid glass over an app-owned backdrop, matching the OctoSense
   desktop style and the Weather app.
2. **Interaction**: tapping a headline opens the article; a chevron button on
   the title line reveals the summary.
3. **Opening**: links go to the bundled Browser app when the host can launch
   it, otherwise to an in-app reader on the native web view, then the system
   browser, then a notification.

## Look

The app paints its own backdrop in both appearances, as Weather paints its
sky: a vertical gradient from deep ink to a warmer navy with a faint diagonal
highlight. The framework's glass widgets assume a dark ground with light
text, so the backdrop is what makes glass work in the light appearance too.

- **Header**: a `glass.NavBar` with the title, the status line in
  `glass.Caption`, and a `glass.IconButton` for Refresh (the host's refresh
  icon, copied into `apps/news/resources/icons/`).
- **Tabs**: one `glass.GlassSegmented` control. The view sets its labels
  from the model's tabs and reads the selected index back.
- **Rows**: each headline is a `glass.Card`. A badge in the source's colour
  carries the source label (as shipped, a local `RoundedView` role: the
  framework's `glass.Badge` draws nothing at this revision). Title in
  `glass.Body` at 14 points, two lines; meta line in `glass.Caption`.
- **Expanded row**: the summary appears inside the same card under a
  hairline; the card grows.
- **Tile face**: one glass card over the same gradient with three headline
  lines, each with a coloured source dot.

Source colours live once in `model.rs`, `source_color(id)`: Hacker News
orange, TechMeme teal, Google News blue, user feeds slate.

## Interaction

- Tapping anywhere on a card calls `open_link(cx, &link)`. The Open button
  goes away.
- A 28-point glass icon button with a chevron sits at the right end of the
  title line; it toggles the summary and rotates 180 degrees when open (the
  button's own `draw_icon.rotation`, one SVG). The framework delivers a
  finger-up only to the area that captured the finger-down, so the chevron
  never opens the row.
- The expanded area shows the summary and a caption with the link's host.
  Hacker News rows also get a "Discussion" chip that opens the discussion
  URL through the same opener chain.
- `toggle_expanded`/`is_expanded` are unchanged; only the trigger moves.

## Opening a link

`open_link` runs opener tiers in order and stops at the first that acts. The
view knows how it is hosted (`Hosting::{Standalone, Process, Module}`),
set by `main.rs` or the module's `create`.

1. **The bundled Browser** (hosted only). The view sends
   `WmRequest::Open { app: Some("browser"), path: url }`: from a process
   through `makepad_wm_api::send`; from a module by posting the `WmRequest`
   as a widget action from the view's root. A host hook in the module tile
   forwards `WmRequest` actions from instance roots to the request handler
   with the module's client id, which also gives modules the title and
   notification requests. The host spawns the Browser with the URL as its
   argument (the Browser reads positional arguments as URLs). Each link
   opens a new Browser tile; the Browser has no message to navigate a
   running instance (fork follow-up). The host now also forwards `Launch`
   arguments, which it dropped before this phase.
2. **The in-app reader**. When the host answers that the Browser is not
   launchable, or the app is standalone, the view opens the article in an
   `ArticleReader` pane inside its tile on the native web view (WKWebView on
   macOS and iOS, Android WebView). It works in the standalone window and
   in-process modules; a process-hosted child has no native window, so that
   case skips this tier. Linux has no native web view.
3. **The system browser**: `cx.open_url` (macOS and web).
4. **Tell the person**: a notification naming the link's host.

The host replies to an `Open` or `Launch` whose app is not launchable with
`{"wm_unavailable": {"app": "...", "path": "..."}}` as an `Event::Custom`
to the requester (a process over its socket; a module as an event into its
isolate). The view treats it as "tier 1 failed for this path" and continues
with the next tiers. Any app can use the envelope.

## Reader pane

`ArticleReader` is hidden until used. It drives `cx.system_browser(id)`:
spawn on first use, `set_url` for later links, `update(area, visible)` on
every draw while open, hide on close, `detach` then `close` on shutdown. No
platform emits a navigation event at this revision, so the glass bar above
the page shows the link's host, a Back button (`history_go(-1)`) and a Close
button back to the list; there is no page title, loading indicator or error
text. The overlay is glued to the reader's `page` child, so the bar stays
above it. The pane replaces the list; the header and tabs stay. Because
`PortalList` ignores `set_visible` at this revision, the list sits in a
`list_box` view that the view shows and hides. A `HostedViewMode::Tile`
message closes an open reader, since the native overlay would otherwise
outlive the full face; a tap on the tile face never opens a link.

The overlay also outlives a tile the desk stops drawing (a hidden
workspace, another tile gone fullscreen), since it only moves or hides
when the reader draws or is told to. So while the pane is open a watchdog
runs on a half-second timer: each tick asks for a redraw and checks that
the redraw it asked for last time drew the reader; a tick that finds no
draw takes the overlay off the window, the pane stays open, and the next
draw puts the overlay back. Two redraws of the tile a second while
reading, none when the pane is closed; the overlay may linger up to a
second after the tile goes.

Known limitation (NEWS-06): on macOS, once the WKWebView is attached
inside an OctoSense tile, keyboard chords no longer reach the host until
the reader closes: the workspace keys and the menu chord are inert, and a
synthetic modifier press stays down until released. Mouse clicks on
Makepad-drawn areas still work, so Close does. The web view most likely
becomes the window's first responder.

## Testing

Unit-tested, without a window:

- Model: `source_color`, the opener tiers as a pure function of hosting and
  platform (`OpenPolicy::tiers`), and the `wm_unavailable` envelope parser.
- Host: the envelope's round trip (`src/wm_reply.rs`); a module root's uid
  names its client and `send_custom` delivers into its isolate
  (`src/module_host.rs`); a launch's extra arguments come last, after the
  app's own (`launch_argv` in `src/clients.rs`).
- View, in an isolate: the reader starts hidden; a module posts the Browser
  request and the host's `wm_unavailable` for that link opens the reader
  where the platform has a web view; a reply for another link is ignored;
  a standalone window opens the reader at once and Close restores the
  list; the tile face opens nothing and closes an open reader; only http(s)
  links leave the list; an open reader that is not drawn takes its overlay
  off the window and asks for no frames once closed.

Verified on screen, on this Mac: the host's actions-loop hook (a module
root's `WmRequest` reaches `on_wm_request`), `reply_unavailable` (the
module hears the envelope and opens the reader inside its tile),
`launch_app_with_args` (a tap in the module tile launched the Browser with
the link once the sibling makepad checkout existed; the first launch
compiled it), the reader tier in the standalone window (Back, Close), and
the glass look at the window, phone and tile sizes.

## Delivery

Branch `feat/news-app`, a separate commit. Backlog: open in the running
Browser (fork), a Linux reader, an Android device check of the web view
plumbing in OctoSense's activity.
