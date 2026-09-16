# News app

A News app for OctoSense that aggregates headlines from several sources and
shows them in one view. It follows the Weather app's shape: one crate that is
a standalone Makepad window, a process-hosted tile in the desktop catalog, and
an in-process `AppModule` that phones link automatically.

Decisions taken in the design conversation on 2026-09-15:

1. Tabs per source plus an All tab. Each source keeps its own ranking
   (Hacker News points, TechMeme editorial order). All interleaves the sources
   by rank (the first row of each source, then the second, and so on) rather
   than sorting by time, so no single source floods it and every source's own
   order survives.
2. Tapping a headline expands the row in place to show the feed's summary and
   an Open button that hands the link to the system browser.
3. Three built-in sources (Hacker News, TechMeme, Google News) plus user RSS or
   Atom feeds from a file in the app's storage jail.
4. A compact wide home-tile face for the phone home screen, like Weather and
   Clock have.

## Crate layout

`apps/news`, package `octosense-news`, a workspace member.

| File | Owns |
|---|---|
| `src/lib.rs` | Re-exports; `pub mod` list; `NEWS_MODULE` |
| `src/model.rs` | `Source`, `Headline`, `NewsModel`: fetch bookkeeping, refresh rule, cache encoding, tile selection |
| `src/feed.rs` | RSS and Atom extraction into `Headline` rows; entity decoding; tag stripping |
| `src/hn.rs` | Algolia front-page JSON into `Headline` rows |
| `src/view.rs` | `NewsView` widget: both faces, tabs, list, expanded row, timer, storage |
| `src/module.rs` | `impl AppModule for NewsModule`, `NewsExecutor` |
| `src/ai.rs` | The `headlines` read tool: manifest and answer |
| `src/main.rs` | Standalone window; the crate has no features (like Reference), so the binary always builds |

Dependencies are crates already in the tree at the pinned makepad revision:
`makepad-widgets`, `makepad-app-module`, `makepad-wm-theme`,
`makepad-ai-services`. The standalone binary also takes `makepad-wm-api` for
the title and the polite close. JSON is parsed with the framework's
`makepad_micro_serde`, as Weather does. No XML crate is added; `feed.rs` is a
small purpose-built extractor tested against saved fixtures.

## Host wiring

- Root `Cargo.toml`: `octosense-news = { path = "apps/news", optional = true }`,
  feature `app-news = ["dep:octosense-news"]`, `mobile-apps` gains `app-news`,
  the Android/iOS target table links it unconditionally, and `apps/news` joins
  `workspace.members`.
- `src/apps.rs::linked_modules()` pushes `octosense_news::NEWS_MODULE` under
  `cfg(any(feature = "app-news", target_os = "android", target_os = "ios"))`.
  `bundled_catalog` keeps focus policy for it.
- `config/apps.json` and `config/apps.makepad.json` gain a `news` entry:
  manifest `../apps/news/Cargo.toml`, package and bin `octosense-news`,
  policy `focus`.
- `src/mobile_tiles.rs`: `("news", TileKind::Wide)` in `TILE_APPS`, `news` in
  the `Utilities` library group. The home layout must be checked with a third
  wide tile; the existing comment says a second wide tile makes both banners.

Capabilities declared by the module: `storage`, `net`.

## Sources and data flow

Every source is one HTTP GET through `cx.http_request` with a `LiveId` the
model owns. Responses arrive on `Event::NetworkResponses`; a response whose id
the model no longer owns is dropped. The same code runs standalone, in a
desktop tile and in a phone isolate. No threads, no blocking client.

Tab order: All, then the built-in sources, then user feeds. All is not a
source; it is a view over every source's rows, interleaved by rank. It never
fetches on its own: showing it fetches every source that is stale.

Built-in sources, in tab order:

| Tab | URL | Parser |
|---|---|---|
| Hacker News | `https://hn.algolia.com/api/v1/search?tags=front_page&hitsPerPage=30` | `hn.rs` |
| TechMeme | `https://www.techmeme.com/feed.xml` | `feed.rs` |
| Google News | `https://news.google.com/rss?hl=en-US&gl=US&ceid=US:en` | `feed.rs` |

Google News titles end in ` - <Source>`; the parser splits the suffix into the
row's source field. Algolia hits carry `title`, `url`, `points`,
`num_comments`, `author`, `created_at` and `objectID`; a hit without `url` links
to `https://news.ycombinator.com/item?id=<objectID>`.

User feeds: the storage key `feeds.json` holds an array of
`{"label": "...", "url": "..."}`, read once when the view starts. Entries are
trimmed, ones without an http(s) URL are dropped, a repeated URL keeps its
first entry, and the list is capped at four after that dedupe, so a duplicate
never costs a distinct feed its slot. Each becomes a tab after the built-in
three; an entry without a label is named by its host. A user feed's source id
is `user_<hash of url>`, not its position, so its cache key follows the URL
and a feed replaced by another never inherits its cache. The first release
has no in-app editor; the person edits the file. `feed.rs` autodetects RSS
(`<item>`) versus Atom (`<entry>`) and reads Atom links from
`<link href="...">`.

One row type across sources:

```rust
pub struct Headline {
    pub title: String,
    pub link: String,
    pub source: String,          // "Hacker News", a feed's <source> element (else empty), or the split Google source
    pub published: Option<i64>,  // unix seconds, when the feed gives one
    pub points: Option<u32>,
    pub comments: Option<u32>,
    pub summary: String,         // description with tags stripped, entities decoded, capped
}
```

Refresh and cache: a tab fetches when first shown and every fifteen minutes
while visible; a refresh button fetches the current tab now, budget or not.
A source is `due` when no request is in flight and no attempt, successful or
failed, was made within the refresh budget, so a failing source is retried
every fifteen minutes rather than every tick. The last good rows per source
are written to the storage jail under `cache.<source-id>`, so startup shows
cached headlines (status `Saved headlines`) until the fetch lands. A failed
source keeps its cached rows and shows an inline error line naming the HTTP
status or connection error. A feed that parses to zero rows is an error.

## The two faces

`NewsView` embeds a `HostedView` and reads its mode; the host switches faces
with the `HostedViewMode` message over `Event::Custom`. The standalone binary
accepts `--tile` and `--phone` to check both faces in a plain window.

Full face: a top bar with the title, a refresh button and a tab strip of
radio buttons, one per source. A `PortalList` draws the selected source's
rows with `set_item_range` and `next_visible_item`, as Mail does. A row shows
the title and a second line with source, relative time, and points and
comments when present. Tapping toggles the row open in place; one open row
per tab, tracked by its link, so a refresh that moves rows keeps the same
story open. The open row adds the summary and an Open button that calls
`cx.open_url`. On macOS that shells out to `open` and on the web it is the
browser; on Linux, Android and iOS the call is a stub at the pinned revision,
so the open row also shows the link's host as text; the platform follow-up is
recorded in the backlog.

Tile face: a wide card with the app name and the top three rows of the All
tab (so one from each built-in source), each on one line with its source
after a dot; below about 96 pt the card drops its heading to keep three lines.
It draws from the same model, so cached headlines appear on the home screen
before any network activity. The host handles the tap and opens the full app.

All colors and text roles come from the palette the host applies into the
isolate; the app defines no color constants of its own.

## Assistant tool

One read tool, `headlines`, with optional `source` (a tab label or id) and
`limit` (default 10, max 30). It returns the rows on screen as a numbered text
list with links. The module executor and the standalone `AiServicePort` answer
it identically through `ai::answer`. An unknown tool name is refused naming the
one that exists; so are an unknown source (naming the known ones) and
arguments that do not parse.

## Testing

- Parser fixtures under `apps/news/tests/fixtures/`: an Algolia JSON page, a
  TechMeme RSS page, a Google News RSS page, an Atom feed. Tests assert row
  counts, the Google source split, entity decoding, tag stripping, the
  zero-item error, and that malformed input never panics.
- Model tests: request-id ownership drops a superseded response; the refresh
  rule; cache round-trip through the storage encoding; All interleaves by
  rank; tile rows are the top three of All.
- Module tests: id, label, capabilities, empty open, a `NewsView` root minted
  in a fresh isolate without script errors, the face switch on the host's
  message, teardown in the host's order.
- Host tests: the linked-modules list gains `news` under `mobile-apps`; the
  home layout test gains the third wide tile.
- Manual: the standalone window with real network; the desktop tile with
  `cargo run --features app-news -- --module news`; the Android home tile and
  full app on a device.

## Delivery

Branch `feat/news-app`. The README's default-app list and add-an-app section
gain a News paragraph. The backlog gets the Linux, Android and iOS open-URL
follow-up and the in-app feed editor.
