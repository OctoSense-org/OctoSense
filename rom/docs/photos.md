# Photos

`apps/photos` is the native Photos AppModule in the OctoSense phone shell. It
replaces the upstream picture wall while retaining the `photos` launcher ID,
icon, and focus-existing-instance policy. The layout follows the Library,
Collections, People, and Memories screenshots in the
[Apple Photos listing](https://apps.apple.com/us/app/photos/id1584215428).

## Using the app

- **Library** shows all 19 bundled photos in a chronological, three-column grid.
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

The first version uses the user's six individual portraits, the original group
image, and twelve landscape samples. Camera/MediaStore import, cloud sync,
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
