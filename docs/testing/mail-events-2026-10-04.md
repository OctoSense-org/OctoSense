# Incoming Mail test on OnePlus 6 — 2026-10-04

English | [简体中文](mail-events-2026-10-04.zh-CN.md)

Real AgentMail emails reached a connected Gmail account and automatically
started Mail agent turns in the isolated Android package
`dev.makepad.octosense.studio`. The system agent provisioned the policy and later
review feedback. There was no per-email chat prompt, manual peer dispatch, or
handwritten replacement for a generated card. The tester sent fictional emails,
inspected receipts, and operated the phone UI.

The trial handles only subjects starting `[OctoSense simulation]`. Other new
messages are skipped without reading their bodies. The initial 25 messages
established a baseline and did not become events. Gmail login remained on the
host-owned sheet; no passwords, provider keys, real mail bodies or personal
account identifiers are included in this evidence.

## Observed outcomes

| Model and policy stage | Shipping | Appointment | Quiet newsletter |
| --- | --- | --- | --- |
| DeepSeek V4 Flash, initial | Published after repairing an overlong notification summary; card too verbose | Published; phone/native review found clipping | Explicit `no_action` skip; no publication |
| DeepSeek, compact feedback | Published after two syntax repairs; complete readable facts | Published on first attempt; complete readable facts | Previous control retained |
| MiniMax M3.1 Flash Preview, same compact policy | Published after two syntax repairs, but facts rendered as placeholders | Published, but facts rendered as placeholders | Explicit `no_action` skip; no publication |
| MiniMax, data-binding feedback and host gate | Published after two syntax repairs; complete facts, still verbose | Published on first attempt after sender verification; complete readable facts | Previous control retained |

Ten fixture emails were accepted and processed: eight publications and two
explicit skips. Publication alone was insufficient evidence of a useful card:
the first MiniMax pair had valid L0 syntax but flat data instead of `data.note`.
Those failed visual results are preserved. Host admission now rejects missing
dataset objects/fields, missing field lists and cyclic source dependencies so
the model receives an error it can repair.

[Filtered tool outcomes](mail-events-2026-10-04/tool-results.json) retain
validation failures as well as successes. [Provider attribution](mail-events-2026-10-04/provider-attribution.json)
comes from each completed turn's kernel ledger model metadata, not the subject
line or the model's self-description. [Generated card arguments](mail-events-2026-10-04/cards/)
preserve the model's source strings and data values without edits.

## Phone and native Makepad review

The phone showed the revised DeepSeek cards in Glance and the corrected MiniMax
shipping card in both Glance and the full-card sheet. Tapping its Mail notification
opened that exact card. The final MiniMax appointment also passed notification
→ full card → Android Back → Glance on version `2026100406`; the test activity
remained in the foreground. Earlier builds failed Android Back because the
pinned Makepad loader expects an extension class under the APK package name.
A small delegating extension for the isolated package fixed that test-packaging
issue without replacing Home or changing the Makepad pin. The sheet close
button additionally has unit coverage; its physical tap was not tested.

| Capture | What it establishes |
| --- | --- |
| [DeepSeek Glance](mail-events-2026-10-04/screenshots/deepseek-compact-glance.png) | Complete appointment facts, proposed/not-booked status and next action |
| [MiniMax notification](mail-events-2026-10-04/screenshots/minimax-shipping-notification.png) | Mail attribution and generated shipping notification |
| [MiniMax Glance](mail-events-2026-10-04/screenshots/minimax-shipping-glance.png) | Corrected dataset values rendered on the phone |
| [MiniMax full card](mail-events-2026-10-04/screenshots/minimax-shipping-full-card.png) | Notification tap opened the shipping card |
| [Final appointment](mail-events-2026-10-04/screenshots/minimax-appointment-full-card.png) | Fresh email on final build opened as a complete card |
| [Android Back](mail-events-2026-10-04/screenshots/minimax-appointment-back.png) | Returned to Glance, staying in the test activity |

The unchanged cards were also rendered through pinned native Makepad
`card-host`/`card-studio` with hidden windows and the instrument API. At the
phone's 344-point inner tile width, native text ends at 266 points for compact
DeepSeek shipping, 228 for its appointment, and 308 for corrected MiniMax
shipping. The final MiniMax appointment ends at 214 points and displays all
seven used fields. Production tiles fit their content up to 440 points; 240 was a design
target, not a runtime cap. Thus exceeding 240 does not itself prove phone clipping.
The first MiniMax pair had seven missing bindings each; the corrected shipping
card displayed all eight used fields.

The native renderer uses a different palette/font from the phone shell. Its
automatic `fits` result can overlook partially clipped labels, so review compared
same-width instrument rectangles against a taller render and inspected actual
phone captures. [Native measurements](mail-events-2026-10-04/native-review.json)
record this distinction. These few iterative cases do not establish a general
model speed or quality ranking. In this run, DeepSeek's compact cards followed
the requested hierarchy more closely; MiniMax's repaired shipping still repeats
a simulation disclaimer and spreads labels across extra lines.

## Build and verification boundaries

The test APK retained the Gmail account across upgrades. Production Home was
not replaced. DeepSeek ran on test versions `2026100402`; the first MiniMax pair
and notification route ran on `2026100403`; its corrected shipping ran on
`2026100404`. Version `2026100405` adds stricter field-list/dependency checks and
clearer admitted skill text. Final version `2026100406` additionally loads the
isolated package's Android extension; its fresh MiniMax appointment, full-card
opening and physical Back test passed. AgentMail's earlier unverified daily-limit
HTTP 429 was resolved by the human providing the verification code; it was not
a Gmail or agent failure.

The [build receipt](mail-events-2026-10-04/build-receipt.json) binds the final APK
to the base revision, changed code hashes and device observations.

Executed checks:

- Phone shared-shell suite: **862 passed**. After tightening dataset admission,
  its two focused tests passed again, including flat/missing/null/nested data,
  omitted/empty field lists and dependency cycles.
- Mail service: **28 passed, 2 ignored**. The ignored tests require optional live
  Gmail/platform keychain setup; they are not counted as passes.
- Broker suite: **62 passed**; AI host: **39 passed**. Guidance/account isolation
  was also exercised in focused tests. Scripted connector coverage is distinct
  from the real provider turns above.
- Desktop checks with and without `mobile-apps`, phone check with `mobile-apps`,
  both shell-graph checks, source setup/pin check, native-app catalog check,
  Android contracts export, APK build, and no-local-paths test passed.

The flow is process-bound: no Android background job/service or native Android
notification delivery is implemented. Pending events and decisions are durable;
visible Glance cards are in memory and disappear on process restart. A crash
between publication and saving its receipt can repeat a notification. See the
[implementation walkthrough](../mail-agent-events.md) for these boundaries and
the account-scoped tool path.
