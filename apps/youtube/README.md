# YouTube music cards

English | [简体中文](README.zh-CN.md)

YouTube search, its app agent and Glance music cards share a host-owned cache of
actual YouTube search results. `youtube.search`, `youtube.read`,
`youtube.recommend` and `youtube.publish` are shareable tools; other agents still
need an explicit caller grant. `youtube.preferences` belongs to YouTube itself.

In the app, choose Morning, Noon break, Relax, Dinner or Sleep. Search results
appear in the app and the first result becomes a quiet Glance card for that
occasion. Expand it for the title, channel and duration; **Play in YouTube**
opens that exact result's mobile YouTube watch page in the app's WebReader. Native **Chat** lets the person
refine music with YouTube's agent, subject to normal agent consent. Schedules
never start audio. YouTube may require another tap or sign-in to play.

**Daily music** is off initially. Enable it and allow YouTube’s agent in Settings to get quiet suggestions at 07:00,
12:00, 15:00, 18:00 and 21:00 in the device's local timezone. Ask the agent to
change the five distinct hours or each occasion's search query. The toggle shows
**paused** when agent consent is missing or revoked; it does not silently run. The host checks
while running; Android suspension can delay or stop checks. This is not a new
Android background job. Only the current occasion is considered, without a
catch-up burst, and each successful local-day/occasion is recorded durably.

The host stores queries, searched metadata, settings and scheduling receipts
under `.host/youtube/recommendations.json`; exact published video records live
in `.host/youtube/cards.json`. Cards restore quietly without extending expiry.
Dismissal and Undo are persisted; a dismissed time slot does not reappear from
the same day’s scheduler. It stores no YouTube credentials.
Queries go to YouTube's public search endpoint. Results older than a day must be
searched again before publishing; invented ids are refused. The previous card
stays intact if a new search fails. UI history remains in the app's device account.

Implementation: `crates/shell/src/youtube.rs`, the app bundle's `tools.json`,
`AGENT.md` and `recommendation.card`. The L0 template is host-authored presentation;
it is not attributed to DeepSeek or MiniMax. Phone validation exposed Error 153
when the iframe player was opened as a top-level page. Playback now uses the
same mobile watch URL as the pinned runtime's `sys.video`, including old history
entries. The corrected OnePlus 6 test opened the exact saved video and displayed
playback with YouTube’s “Tap to unmute” prompt. Audibility, provider-backed
YouTube chat and natural time-slot scheduling remain unverified. A hidden native run on
macOS at Android phone dimensions verified search, publication, restart restoration,
focused expansion, native Card/Chat, retained unsent chat, consent-paused settings
and navigation to the exact video. The native WebReader overlay is black in a
Makepad framebuffer capture, so that run does not establish audio playback.

Validation performed for this change: ten deterministic native shell tests
passed; the optional live public YouTube search also passed through Makepad's
platform networking backend. The live check verifies actual searchable video
identities and metadata, not audio playback. The commands run were:

```sh
cargo test --locked -p octosense-shell --features mobile-apps youtube::tests -- --test-threads=1
cargo test --locked -p octosense-shell --features mobile-apps youtube::tests::live_public_search_returns_actual_video_results -- --ignored --test-threads=1
```
