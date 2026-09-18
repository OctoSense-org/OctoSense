# News app, phase 3: the Apple News look

Phase 2 (`2026-09-16-news-app-phase2-design.md`) gave the News app liquid
glass over a dark backdrop, a tab per source and a reader behind the
Browser. Phase 3 remakes the app after Apple News for the phone, decided in
the design conversation on 2026-09-16:

1. **Appearance**: Apple News' light look (off-white ground, white cards,
   heavy black headlines, a red accent) with a dark variant that follows
   the host's light or dark setting.
2. **Images**: a hero or thumbnail when the feed carries an image; text-only
   cards otherwise. No page scraping.
3. **Screens**: a floating bottom bar with Today, Following and Saved, and
   a round Search button; Following is the in-app source editor.
4. **Reader**: a tap opens the article in the app's own reader first; the
   OctoSense Browser is a menu item.

The backend stays: the three built-in sources, the person's feeds, the
fifteen-minute budget, the per-source cache, the host request channel and
the `wm_unavailable` reply, the `headlines` tool.

## Appearance

The app paints its own skin, two variants, one set of roles:

| Role | Light | Dark |
|---|---|---|
| ground | `#F2F2F7` | `#000000` |
| card | `#FFFFFF` | `#1C1C1E` |
| ink (titles) | `#000000` | `#FFFFFF` |
| secondary (meta) | `#6E6E73` | `#98989F` |
| hairline | `#3C3C43` at 18% | `#545458` at 40% |
| field (search, inputs) | `#E5E5EA` | `#2C2C2E` |
| accent | `#FA2D48` | `#FA2D48` |

The variant is picked when the crate's `script_mod` runs, from the
palette's `light_mode` (`makepad_wm_theme::current_for_vm`); with no
palette (a standalone window) it is light, and `--dark` forces the dark
skin for a look. The host re-runs every module's `register` inside a
reload and re-applies the root's DSL on a style change
(`module_host.rs` `apply_style`), so the skin follows the host's toggle
with no Rust-side colour state: the roles are spliced into the DSL as
values.

The floating controls (the bottom bar, the round buttons) are the
framework's `glass.*` lens surfaces, which refract whatever is behind them
and so work on both grounds; the text and icons inside them are the skin's
ink and accent, not glass's own light text. Nothing else is glass: Apple
News' cards are opaque.

Typography is Inter, as the host's: a `Heavy` role (weight 800, the way
Weather defines `Light`) for page titles (30 pt), section names (20 pt) and
hero headlines (20 pt); `font_bold` for compact headlines (15 pt, three
lines) and publisher names (12 pt); regular for a hero's deck (13 pt,
two lines) and captions (11 pt). Every bar centres its items vertically,
every round button centres its icon, every tab centres its icon over its
label, and empty states are centred in the list.

## Screens

One `NewsView` full face, one `PortalList`. The view keeps a `Nav`
(`model.rs`, pure): a root page picked by the bar (Today, Following,
Saved), an optional pushed page over it (a Source page, or Search), and the
reader over everything. `render` turns the current page into a flat list
of items, each drawn by one template:

- `PageTitle`: the page's name in Heavy 30 with a caption under it
  (Today: the local date and the status, `Wednesday, September 16 ·
  Updated 5 min ago`; Saved: the count).
- `SectionHeader`: a source's name in Heavy 20 with a caption (Hacker
  News: `Ranked by points`, TechMeme: `Editors' picks`, Google News: `Top
  stories`, a feed: its host) and a chevron button that opens the Source
  page.
- `Hero`: the first story of a section, a card: the image (16:9,
  `CropToFill`) when the row has one, the publisher line (a coloured dot
  and the name), the headline, the deck (the summary, when the feed gives
  one), the meta line (`1h ago · 245 points · 120 comments`) and a `•••`
  button.
- `Compact`: the next stories of a section: the publisher line, the
  headline, the meta line and `•••`, with a 72 pt thumbnail at the right
  when the row has an image; a hairline above each.
- `Footer`: `More from Hacker News ›` in the accent, closing the section's
  group; opens the Source page.
- `SourceRow` (Following): a coloured monogram, the label, the host, a
  toggle that follows or hides the source, and a remove button on the
  person's own feeds.
- `AddFeed` (Following, last): a URL field, an optional name field and an
  `Add feed` button; the error caption names the problem (not an http(s)
  URL, already a source, four feeds is the cap).
- `Empty`: a centred caption (`Stories you save appear here.`, `No
  results for “…”`, `No headlines yet`).
- `Spacer`: room under the last row for the floating bar.

Today lists one section per followed source that has rows: the header,
the hero, four compacts, the footer. A Source page lists the page title,
a status caption, the source's rows (a hero, then compacts). Following
lists every source (built-in and the person's own) and the add form.
Saved lists the saved stories, newest first. Search has a fixed field row
above the list (a search field and `Cancel`), and lists the rows of every
followed source whose title, publisher or summary contains the query,
case-insensitively, at most fifty.

The bottom bar is a `glass.TabBar` floating 12 pt above the bottom edge
with three tab buttons (icon over label, the selected one in the accent)
and, to its right, a round glass button with the search icon, as Apple
News lays it out. A tap on the current tab scrolls its page to the top.
A round glass refresh button floats at the top right of Today and of a
Source page; a round glass back button at the top left of a Source page
and of the reader.

The `•••` button opens an action sheet: a scrim over the page and a card
at the bottom with `Save` (or `Unsave`), `Open in Browser`, `Copy link`
and `Cancel`. The same sheet opens from the reader's `•••`.

Back — the round button, the phone's back gesture or key
(`Event::BackPressed`, marked handled) — closes the reader if open, else
pops the pushed page, else is left to the host. The host's `Tile` message
still closes the reader and the sheet.

## Opening a link

`OpenPolicy::tiers` puts the reader first wherever there is a native web
view (a standalone window, a module in the host's window): Reader, then
the Browser through the host, then the system browser, then a notice. A
process child has no window for a web view, so for it the order is
unchanged. `Open in Browser` from the sheet asks the host for the Browser
directly and, when the host answers `wm_unavailable`, falls through to the
system browser or a notice.

The reader is a full-screen layer over the page and the bar: a top row
with the back button, the link's host centred (no platform reports the
page's title at this revision) and a `•••` button; the web view fills the
rest. The watchdog, the overlay glue and the shutdown are phase 2's.

## Images

`Headline` gains `image: Option<String>`. `feed.rs` reads, in order,
`<media:content url>` whose `medium` is `image` or whose `type` starts
with `image/`, `<media:thumbnail url>`, `<enclosure url>` with an image
`type`, and the first `<img src>` in the description or content; only
http(s) URLs count. Hacker News rows have none. The cache keeps the field
(optional on read, so older caches still load). The row's `Image` loads
the URL through the framework's image cache
(`load_image_http_by_url_async`; JPEG and PNG decode at this revision, a
row whose image fails to decode keeps its text layout). The view forwards
`Event::NetworkResponses` to the image cache itself, so a reply that lands
while no row is on screen is not lost.

## Following and Saved

Two more storage values beside `feeds.json`:

- `hidden.json`: the ids of sources the person switched off. A hidden
  source is left out of Today, the tile and Search, and its refresh
  stops; it stays on the Following list and its Source page is still
  reachable from there.
- `saved.json`: the saved rows as `Headline` JSON, newest first, at most
  one hundred, one per link.

Following writes `feeds.json` in phase 1's shape when a feed is added or
removed (the built-in three cannot be removed, only hidden), and the model
refetches the new source at once. The cache of a removed feed is left
behind, as before.

## Tile face

The wide home tile keeps its three lines and its short form, restyled: the
skin's card at 92%, `NEWS` in the accent, coloured source dots, ink
headlines. Hidden sources do not reach it.

## Testing

Unit-tested, without a window:

- `feed.rs`: the image extraction order and the http(s) rule.
- `model.rs`: the skin per mode; the date line; `Nav` push, pop and the
  bar; Today's sections (followed sources only, five rows each, the
  hero first); the tile's rows skip hidden sources; Search matching and
  its cap; Saved toggle, order, dedupe, cap and JSON round trip;
  `hidden.json` round trip; add and remove feed rules; the opener tiers'
  new order.
- `view.rs` (isolate): the bar switches pages and the search page
  filters; a tap opens the reader first standalone and as a module; Back
  closes the reader, then pops, then is unhandled; the sheet's Save
  writes `saved.json`; `Open in Browser` posts the Browser request from a
  module and the `wm_unavailable` reply moves on; the tile face closes
  the reader; the `--dark` skin.

On screen: the standalone window at the phone size in both skins, the
mobile-only desktop shell's News tile and full app. The Android device
check (NEWS-05) needs the phone on ADB.

## As built

Where the code settled differently from the text above, on 2026-09-16:

- **TechMeme is a digest.** Its titles end in ` (Author/Outlet)` and its
  descriptions read `Outlet: headline …`; `SourceKind::Digest` takes the
  outlet off the title into the publisher line, and a description that
  opens with the headline again keeps only what follows it as the deck.
  The same `Outlet:` rule serves any feed whose description is shaped so,
  when the outlet reads as a name (capitalised words, no sentence
  punctuation).
- **Icons are not pictures.** An inline `<img>` whose `width` or `height`
  says under 50 px (TechMeme's permalink icon, tracking pixels) is skipped;
  tags and attributes match in any case, values quoted or bare.
- **Square corners are a hair's radius.** The per-corner rounded view's
  distance field misfills with a zero radius, so a group segment's square
  corners are 0.5 pt.
- **The glass bar composites above ordinary content**, so it hides while
  the sheet is up (and on Search, for the keyboard). The sheet's scrim is a
  `SolidView`: a plain view's ground draws nothing at this revision.
- **A toggle's animator resets on its first draw**, so the Following
  switches are synced after each row draws, with a cut. They are restyled
  in the accent; the stock pill's on and off states were indistinguishable.
- **Search takes focus after its first draw**, when the field has an area.
- **The AppCard kit's `KitTabBar` and `KitBottomNavigation` were not
  used**: they are the design translator's targets, configured by a
  generated JSON contract over named children, not hand-authored; the bar
  is the framework's `glass.TabBar` with flat icon-over-label buttons.
- **Standalone dev flags**: `--phone` and `--tile` as before, plus `--dark`
  and `--light` to force a skin in a window that has no host palette.

From the check on the Android device, on 2026-09-17:

- **The reader's web view navigates.** It is spawned with
  `spawn_navigable`: a story link is often a redirector (Google News RSS
  links are), and the default spawn, made for web app cards, cancels every
  hop on Android. Every open states the policy again.
- **A page that does not load says so.** On a
  `NativeSystemBrowserPageError` for its own browser the reader takes the
  overlay off the window and shows the host, the platform's reason and
  `Try again` in the page's place. Only Android reports it at this
  revision.
- **Both need the framework fork's `feat/news-reader-platform` branch**,
  as does the host's storage root on Android (NEWS-09); the pinned
  revision has neither (MOBILE-06).

## Delivery

Branch `feat/news-app`, after phase 2's commit: the host's storage root
first, then phase 3 in one commit. The README's News paragraph and the
backlog are updated (NEWS-02 done; NEWS-07 for the per-corner group card if
the framework's rounded view cannot do it; MOBILE-06 for the framework
revision the branch builds against).
