# Photos cards and agent tools

English | [简体中文](README.zh-CN.md)

Photos uses one bundled **sample library** for its app, tools and Glance cards.
It does not read the Android photo gallery. The app requires the shell’s Photos
host service; standalone `card-host` without that service shows an unavailable
state instead of a separate copy of the library. `catalog.json` contains the 75 sample
records; thumbnails ship in the bundle. Existing albums and favorites stay in
`os.photos/accounts/device/library.json`, and AI memory snapshots remain in
that same account folder. The agent does not receive image pixels by reading
metadata, and must not infer the person's relationships from sample names.

In Photos, **Show in Glance** publishes the current collection's first 12 photos;
the viewer's **Glance** action publishes the current photo. The card shows sample
metadata and thumbnails. **Open Photos** inside the card returns to that exact
saved selection in the existing viewer. Card/Chat is supplied by the shared
Glance workspace, with the selection bound to Photos' account-scoped agent.
Publication is quiet by default. Repeating the same ordered selection replaces
its card. Saved selections restore quietly until expiry; dismissal remains saved.
These UI/device paths require acceptance on the current build; no phone pass is
claimed by this document.

| Tool | Behavior |
| --- | --- |
| `photos.list` | Bounded search by metadata, with saved favorite flags. |
| `photos.read` | One exact sample photo, by ID. |
| `photos.collections` | Current saved albums and favorites. |
| `photos.publish_card` | Show 1–12 existing IDs, with optional explicit notification. |
| `photos.notify` | Existing generic notice. |

The first four are shareable through the agent relay. Another app still needs an
explicit grant and admission offer; declaring a shareable tool grants no caller
access by itself. The host service rejects direct foreign-app calls. `photos.view`
is a UI-only route consumer, not an agent tool. Photos has explicit News tool
grants for a public topic the person asks to research; it must not automatically
forward photo metadata to News or an external search. Agents can search and publish while
Photos is closed because the shell registers its service before the generic
notice service.

`crates/shell/src/photos.rs` owns the API, selection persistence and trusted local
asset server. Glance grants only this Photos server's exact loopback endpoint to
Photos cards. Models supply catalog IDs, never URLs or filesystem paths. Existing
album/favorite writes stay in the app; tools read their saved file and refuse
corrupt data rather than replacing it. Agent guidance is in `bundle/AGENT.md`.

The new regression tests cover shared saved state, corrupt-state refusal,
selection identity, valid L0, bounded catalog IDs and route consumption while
editing. Validation results belong to the implementation's test report; native
pixels, real agent inference and phone lifecycle are separate checks.

To reproduce the card flow in an already configured OctoSense Home build:

1. Open Photos → Library, search `Evening on the shore`, then open the photo.
2. Tap **Glance**, return Home and swipe right to **At a glance**.
3. Open the Photos summary. Check the thumbnail, sample label and **Open Photos**.
4. Switch **Card → Chat → Card**. An unsent draft should survive the switch.
   Chat requires Photos assistant consent and a configured provider; opening the
   tab alone does not prove model inference.
5. Open a different photo in Photos, then return to the original Glance card and
   tap **Open Photos**. The original saved selection should replace the viewer.

From the repository root, run the service and production-script regressions:

```sh
cargo test --locked -p octosense-shell --features mobile-apps photos
cargo test --locked -p octosense-llm-service --test photos_memories
```

Use an isolated `OCTOSENSE_HOME`, `OCTOSENSE_APP_DATA` and `RINX_DATA_DIR` for
native validation. Keep screenshots, model transcripts and account files out of
the repository. Shareable-tool validation must use the real agent relay and the
caller’s grants; directly calling the Rust function proves only service logic.
