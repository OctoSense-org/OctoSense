# Contextual card validation — 6 October 2026

English | [简体中文](contextual-app-cards-2026-10-06.zh-CN.md)

This change implements [ADR 0009](../adr/0009-contextual-app-cards-and-private-preferences.md).
Codex authored the host services/templates and drove the tests. DeepSeek v4 flash
authored live research and chat responses and selected explicit preference text.
Operator input is not an autonomous incoming-event demonstration. No MiniMax
comparison or numeric UX grade is claimed for this change.

## Results

| Journey | Evidence and outcome |
| --- | --- |
| News research | Hidden native Makepad: real Research action completed in about 41 s, with 12 points and two cited sources. OnePlus 6: the app's Research action completed in 37.45 s, with 12 points and three sources; the stored workflow receipt records one model call, four reads and six search fetches. One fetch failed without preventing a ready result. |
| Research card and chat | The result replaced its running card. Native and phone expansion/scroll showed the complete validated summary, points and citations with Card/Chat controls visible. Native DeepSeek follow-up used the stored report. Open News routed to the matching stored story. The existing article WebReader remained blank during the brief native navigation check; article rendering is not accepted by this test. |
| Photos | Real sample thumbnails, selection publication, expanded card, retained native chat draft and in-card Open Photos passed. The native test first opened a different photo, then verified that the saved card reopened its exact selection. On the phone, a synthetic coastal-photo preference reached DeepSeek, which called Photos tools and published a four-photo selection; the saved records confirmed the IDs. |
| YouTube | Real public search, time-slot publication, cold restoration, Card/Chat and retained unsent input passed in a hidden native window. Phone Play opened the exact saved video. The first iframe URL failed with Error 153; restoring the pinned runtime's mobile watch URL fixed it. Final phone capture showed video playback and YouTube's Tap to unmute prompt. Audibility was not measured. |
| Cold restoration | The final phone build restored saved Photos and YouTube cards in 1.93 s, measured from launch to their host restoration logs; a subsequent capture showed the cards together in the real Glance page. |
| Private preferences | Native human Card Chat produced an exact synthetic news preference; System Assistant called the real list, forget and configure tools. Forget left zero entries and a tombstone; disabling was persisted. Phone Photos Chat produced the exact synthetic coastal preference in the owner-only host store. That phone test preference was removed during cleanup. No personal memory was copied into the test app. |
| Cross-app tools | Three fresh-process relay scenarios called Photos→News, News→YouTube and YouTube→Photos owner stores without opening target apps or preparing their peers. Unrequested transitive grants, owner-only APIs, account injection and revoked consent were refused; cancellation discarded a queued owner reply. These were service fixtures, not live model delegation. |

The five scheduled music slots are opt-in and depend on the host running. Slot
boundaries, time changes, deduplication, failures, dismissal and manual-choice
precedence have deterministic tests. A whole day of natural scheduling, Android
background media execution and a live YouTube model-chat turn were **not tested**.
Photos remains a bundled sample library, without Android Gallery import or image
analysis. Preference capture is best effort for explicit statements, not a full
conversation archive.

## Checks and build identity

- Final broad run: **971 shell tests and 198 service/VM tests passed**, five
  ignored. Optional live-kernel checks that self-skip are not counted as live
  acceptance; the explicit model runs above provide that evidence.
- The subsequent mobile-watch repair passed all ten YouTube service tests.
- Desktop check with mobile apps, native Home build and Android release build
  passed. Both shell dependency graphs, setup/native-app pin checks, diff check
  and the repository's local-path test passed.
- App Hub: 189 contract/policy/hub/doc checks and four system-pack tests passed,
  including concurrent extraction and tamper refusal. The pinned revision is
  `2607fb36f81dffd3f9e1a66de219674e4de16858`.
- octos kernel revision: `056173e85b150e387805fc307fe231064ac1ed35`.
  The final APK's bundled kernel hash matched the build receipt:
  `6ce795bc0923e53f77333b7147a7452abb89317e8eed25678881ca4652a71079`.
- Final Android test APK SHA-256:
  `109a2f97b2fd6eab3163392b19863cd222e6b81570cb121187c96b2168da07dc`.
  It was installed as **ContextualCards**, package
  `dev.makepad.octosense.contextual`, alongside the normal Home.

Hidden native tests used Makepad's actual widget/input/capture APIs. Phone
checks used ADB input, Android captures and owner-service receipts. Raw captures,
provider configuration, account identifiers and logs stay outside Git. Host test
processes were stopped, phone playback was stopped, and the test app was returned
to Glance. No private credentials or real mailbox content are required to build.

## Reproduce the journeys

Build this source with its pinned dependencies using the [Home instructions](../../phone/README.md).
Use a separate phone package and configure your own provider through host Settings.
Allow the relevant app assistant; the first-use sheet discloses its cross-app tools.

1. News: open a collected story, choose Research topic, wait for completion, then
   expand its Glance summary. Scroll the report and use Chat for a follow-up.
2. Photos: choose sample photos and Show in Glance. Expand, switch between Card
   and Chat, then Open Photos. A chat request can publish a new selection.
3. YouTube: choose a music-time chip, open the resulting Glance card and Play.
   Enable Daily music if you want the foreground schedule; leave playback to an
   explicit tap. Refine queries or hours through its assistant.
4. In card Chat, state a short explicit test preference. Ask System Assistant
   what preferences it saved, then ask it to forget the test preference. Failures
   or exhausted model budgets can produce no preference entry.

The [contextual-card guide](../contextual-app-cards.md) traces API ownership,
cross-app admission and private-memory boundaries.
