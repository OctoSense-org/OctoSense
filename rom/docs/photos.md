# Photos

`apps/photos` is the native Photos AppModule in the OctoSense phone shell. It
replaces the upstream picture wall while retaining the `photos` launcher ID,
icon, and focus-existing-instance policy. The layout follows the Library,
Collections, People, and Memories screenshots in the
[Apple Photos listing](https://apps.apple.com/us/app/photos/id1584215428).

## Using the app

- **Library** shows all 75 bundled photos in a chronological grid, three to a
  row by default.
- **Zooming the Library** changes that density between one and nine photos per
  row. Pinch the grid on a phone; on desktop, roll the wheel over the photos or
  drag the **Zoom** slider above them. The photo under the fingers, the pointer,
  or the top of the grid stays where it is while the rows reflow, so zooming
  does not lose your place. Collections, albums, People, and Favorites keep
  their own layouts.
- **Collections** contains Memories, editable albums, People, and Favorites.
- The home-screen Photos card shows up to three library photos, preferring
  favorites and filling any remaining spaces with recent photos. Tap the card
  to return to the same screen, photo, or unfinished album draft.
- Tap a photo to view it; swipe horizontally or use the arrows to move through
  the current collection. The heart toggles its favorite status.
- Tap **+** to name an album and select photos, then **Save**. Open an album and
  choose **Edit** to rename it or change membership. **Cancel** discards the
  draft. Deleting an album requires a second tap and preserves its photos.
- The search icon filters titles, people, places, ISO dates, events, and tags.
- Tap a Memory to play its photos at three-second intervals. **Pause/Play** and
  previous/next controls remain available. Back exits the viewer.

The sample library uses the user's six individual portraits, the original group
image, twelve landscape samples, 32 royalty-free Pexels photos, and 24 generated
everyday scenes based on those portraits. Each person has three new full-body
individual photos; six more photos show pairs, trios, and groups of four at a
cafe, garden, market, stadium, beach,
and woodland trail. The new scenes are included in People, search, Memories,
and album selection. The Pexels set adds animals, architecture, city streets,
transport, food, flowers, waterfalls, hiking, sports, books and music, with eight
sample dates in each month from June through September 2026. Photographer credits
and licenses are recorded in [SOURCES.md](../apps/photos/resources/SOURCES.md).
Saved user albums are preserved. Camera/MediaStore import, cloud sync,
image editing, music, and video export are outside this version. People are
explicit catalog tags, not facial recognition. Memory groups come from catalog
events, with a place/month fallback, and require at least two photos. No image
generation service or network connection is required for browsing.

## Adding more photos

1. Add a PNG or JPEG to `apps/photos/resources/photos/`.
2. Add an entry to `apps/photos/resources/catalog.json`, keeping `id` unique and
   stable. Use a zero-padded ISO date (`YYYY-MM-DD`). For example:

   ```json
   {
     "id": "coast-2026-09-20",
     "file": "coast-2026-09-20.jpg",
     "title": "An afternoon by the water",
     "date": "2026-09-20",
     "location": "By the coast",
     "people": ["Alex", "Sofia"],
     "tags": ["travel", "ocean"],
     "moment": "Days by the water"
   }
   ```

3. Record image provenance in
   [SOURCES.md](../apps/photos/resources/SOURCES.md), then rebuild. `build.rs`
   embeds the directory automatically; no Rust asset list needs editing.

The library, search, People, and Memories include new catalog entries on the
next launch. Saved user album membership remains unchanged. Removing a catalog
entry drops its stale membership/favorite references when saved state loads.
Sample names, dates, and places are demonstration metadata.

## Implementation and storage

The UI is Octoscript `script_mod!` composition using shared native widgets:
`KitBottomNavigation`, `PortalList`, `Image`, `TextInput`, `Button`, and labels.
Navigation reuses the AppCard semantic kit; image decoding uses the shared async
image cache. Search and arrow icons reuse OctoSense resources. The existing
runtime/framework pins are unchanged.

`zoom.rs` owns the Library's zoom: a bounded scale that maps to a whole number
of columns, with hysteresis so a gesture resting on a boundary does not flicker
between densities, and a two-contact tracker that tells a pinch apart from a
one-finger scroll. The wheel response is exponential, matching the reference
photo wall in `makepad/libs/image_tiles`. A pinch keeps the touch stream until
the last finger lifts, so its release neither opens a photo nor flings the list.
A `Grid` row carries nine reusable square cells and shows as many as the current
density asks for; the slider is created only on pointer platforms.

Rust owns the catalog, routes, album drafts, favorites, deterministic Memory
groups, and slideshow timer. The shell's `HostedViewMode` selects a compact
photo strip or the full app without resetting navigation or drafts.
The shell refreshes the compact capture when images finish decoding or the
app changes presentation, so the card reflects the current favorites.
Timer cleanup runs when returning to the home card, on app pause/background,
navigation away, and module shutdown. Album saves replace a JSON file through
a temporary file. Failed writes roll back the in-memory mutation; unreadable
or unsupported saved state is reported rather than overwritten.

State paths:

- Android: `<app files directory>/photos/library.json`.
- Desktop: `$OCTOSENSE_HOME/photos/library.json`, or
  `~/.octosense/photos/library.json`.
- Development override: `$OCTOSENSE_PHOTOS_HOME/library.json`.

## Build and validation

```sh
cargo test -p octosense-photos
cargo check --features mobile-only,app-photos
cargo test --features mobile-apps --bin octosense \
  bundled_apps_open_without_catalog_files_or_child_processes

# Standalone desktop development window
cargo run -p octosense-photos

# Android shell, with an existing Makepad SDK/NDK
cargo makepad android --sdk-path=/path/to/makepad/android_sdk \
  build -p octosense --release
adb install -r target/android/makepad-android-apk/octosense/apk/octo_sense.apk
adb shell am force-stop dev.makepad.octosense
adb shell am start -n dev.makepad.octosense/.MakepadApp
```

Validated on September 16, 2026, on the connected OnePlus 6T (`ONEPLUS_A6010`,
ADB serial `19f8cedf`). Used the installed `cargo-makepad` and SDK at
`/Users/guofoo/git/octos/octos-one/makepad/tools/cargo_makepad/android_33_macos_aarch64`.
The APK was installed with `-r`; application data was not cleared. The prior APK
was saved locally and inspected: it did not contain a bundled `liboctos.so`.

| Check | Result |
| --- | --- |
| Album validation, creation/edit/delete, state serialization/reconciliation, file replacement, search, Memory grouping, home preview selection | 9 model tests passed |
| Home card/full app transitions | Regression passed: album draft, error message, viewer position preserved; Memory playback paused |
| Compact capture refresh and shell tile bookkeeping | 11 tests passed |
| UI registration/instantiation and embedded catalog integrity | 2 tests passed |
| Bundled module registry initialization | Passed |
| Shared runtime/dependency graph verifier | Passed; one Makepad lineage |
| Android release build and shell compile check | Passed |
| Library/Collections navigation, scroll, photo opening, swipe, favorite, search, People, Favorites | Passed on device |
| Album create, rename, add membership, cancel draft, confirm delete | Passed on device; temporary test album removed |
| Saved albums/favorites after app restart and APK replacement | Passed on device |
| Automatic Memory advance and pause | Passed; advancing frames differed, paused frames were identical across a 3.5-second interval |
| Visual review | Square grids, visible captions, selected-tab surface, centered icon buttons/People names/Memory captions |
| Home photo card | Three photos rendered on device; favorite changes refreshed the selection |
| Final process logs | No app errors, panics, or shader failures in the captured log |

Local evidence is in `target/photos-validation/`: build/test logs and native
screenshots including `collections-final.png`, `library.png`, `people-final.png`,
`viewer-swipe.png`, `search.png`, `album-renamed.png`, `album-cancel.png`,
`album-delete-confirmation.png`, `favorites.png`, and `memory-frame-2.png`.
Home-card evidence includes `card-after.png`, `card-favorite-update.png`, and
`card-viewer-reopened.png`.
The phone validation exercised the workflows above; no formal frame-rate
benchmark was run for Photos.

### Library zoom, September 17, 2026

| Check | Result |
| --- | --- |
| Scale bounds, density mapping, boundary hysteresis, slider position | 5 zoom tests passed |
| Two-contact pinch tracking, hold until the last finger lifts, touches starting outside the grid | Covered by the same zoom tests |
| Row reflow at a new density, focal-photo anchoring, other collections unchanged | View controller regression passed |
| Wheel confined to the Library grid, pinch reflow around its midpoint, desktop-only slider | 3 view regressions passed |
| Existing Photos behavior | 9 model and 2 UI/catalog tests still passed (21 in total) |
| Shell compile check and formatting | `cargo check --features mobile-only,app-photos --locked` and `cargo fmt --check` passed |

Zoom behavior on the phone was confirmed interactively by the maintainer on a
build of these changes. No new screenshots or device logs were captured for this
change, and no desktop GUI session was recorded.

One known interaction detail: a pinch takes the touch stream over from the
`PortalList` mid-drag, and the list never sees the matching release, so it keeps
a stale drag state until the next press. That state does not move the viewport
(it only gates tail auto-scroll, which Photos does not use), but the first tap
after a pinch can be spent stopping that stale gesture. Clearing it needs a
`PortalList` API that is private in the pinned Makepad revision, so it is left
to a framework change rather than worked around here.

### Additional everyday scenes, September 18, 2026

Added 24 generated photos: three full-body solo scenes for each of the six
people, plus two pairs, two trios, and two groups of four. Settings include
school, office, stadium, market, garden, woodland, cafe, library and coast.
The built-in image generator used the original individual portraits as identity
references. All selected images were visually reviewed; three group compositions
were widened to keep complete heads and shoes in frame.

Prompts, reference mappings, refinement instructions and final hashes are in
[generated-scenes.json](../apps/photos/resources/generated-scenes.json). The
24 new PNGs total 64.92 MiB; solo images are 1024×1536 and groups 1536×1024.
All 43 catalog images decode, all 19 original image files and catalog entries are
unchanged, and the six portraits still match the supplied originals exactly.
All 21 Photos tests and formatting/whitespace checks pass. The Android release
build was subsequently installed on OnePlus 6T `19f8cedf` with `adb install -r`.
The Library visibly shows 43 photos and the new images, with existing favorites
preserved. All 24 new PNGs were also verified byte-for-byte inside the APK.
Device evidence and APK hashes are under `target/photos-scenes-review/device/`.

### Royalty-free photo expansion — September 18, 2026

Added 32 Pexels photographs with credits, license URLs, download URLs and hashes
in `apps/photos/resources/stock-photos.json`. The library now contains 75 photos.
Eight additions are dated in each month from June through September 2026.
The 1200-pixel JPEGs total 5,459,488 bytes; all decode, have distinct hashes, and
retain their source aspect ratio. Existing 43 assets and their metadata are unchanged.

All 21 Photos tests, formatting and whitespace checks pass. The Android release
APK was rebuilt, checked for all 32 embedded JPEGs, installed with data preserved
on OnePlus 6T `19f8cedf`, and launched. Device screenshots verify the 75-photo
Library, the June section, and the new September Memory. Evidence is in
`target/photos-stock-review/`.
