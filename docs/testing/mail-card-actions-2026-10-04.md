# Mail card buttons on OnePlus 6 — 2026-10-04

English | [简体中文](mail-card-actions-2026-10-04.zh-CN.md)

**The final DeepSeek and MiniMax cards passed the local interaction test.**
Fresh AgentMail emails reached Gmail, automatically triggered Mail's agent and
produced model-authored cards. On the assigned OnePlus 6, each notification
opened its exact card; Show code → Back → Details → Back worked in both the
full sheet and Glance. Both displayed the new pickup code **9064**, correct
pickup facts and item identity. This closes the missing-button gap in the
[earlier incoming-mail trial](mail-events-2026-10-04.md), not the full
[email action-card plan](../../apps/mail/docs/2026-10-01-email-action-card-plan.md).

## Final paired result

Both final emails used identical body bytes and the same provision revision,
with a provider label in each subject and the corresponding provider route. Their fictional desk-lamp notice supplies
a pickup code, location, hours, deadline/timezone and photo-ID requirement;
it contains no tracking URL. [Send receipts](mail-card-actions-2026-10-04/repair-send-receipts.json)
and the [policy receipt](mail-card-actions-2026-10-04/policy-receipt-repair.json)
record that pairing. No fixture code was embedded in the policy. The final
9064 differs from earlier codes 4821 and 7392; the tracking reference still
contains 4821, so the result also checks extraction of the current pickup code.

| Final phone views | DeepSeek V4 Flash | MiniMax M3.1 Flash Preview |
| --- | --- | --- |
| Summary and buttons | [Full card](mail-card-actions-2026-10-04/screenshots/deepseek-repair-full-brief.png) | [Full card](mail-card-actions-2026-10-04/screenshots/minimax-repair-full-brief.png) |
| Code 9064 and Back | [Code view](mail-card-actions-2026-10-04/screenshots/deepseek-repair-full-code.png) | [Code view](mail-card-actions-2026-10-04/screenshots/minimax-repair-full-code.png) |
| Address, hours, deadline and Back | [Details](mail-card-actions-2026-10-04/screenshots/deepseek-repair-full-details.png) | [Details](mail-card-actions-2026-10-04/screenshots/minimax-repair-full-details.png) |
| Same interaction on Glance | [Code view](mail-card-actions-2026-10-04/screenshots/deepseek-repair-glance-code.png) | [Code view](mail-card-actions-2026-10-04/screenshots/minimax-repair-glance-code.png) |

The filtered Android logs record **eight applied transitions per model**:
[DeepSeek](mail-card-actions-2026-10-04/deepseek-repair-taps.log) and
[MiniMax](mail-card-actions-2026-10-04/minimax-repair-taps.log), four in the sheet
and four on Glance. [Phone observations](mail-card-actions-2026-10-04/phone-review.json)
record the corresponding checks. The unchanged sources also pass all four
native clicks at **310 × 330** and **344 × 440** logical points:
[DeepSeek native record](mail-card-actions-2026-10-04/repair-deepseek-native.json),
[MiniMax native record](mail-card-actions-2026-10-04/repair-minimax-native.json).

These are functional and readability passes for the exercised paths, not a
claim of perfect accessibility or complete visual polish. Both native Details
targets remain **61 × 24 points**, below a 44-point height target. DeepSeek's
hours and deadline captions lack explicit labels. MiniMax labels them more
clearly; its full-sheet details show a scrollbar, but facts and Back remain
visible. No general model ranking follows from this small iterative trial.

## Earlier attempts remain part of the result

Six fresh emails produced six cards across three shared-policy stages. Every
card was authored by its model through `mail.publish_card`; no tester wrote
or patched the final source/data. Reviewers changed policy through the system
agent and operated the UI. There was no per-email chat prompt or manual peer
dispatch. Initial and intermediate receipts remain separate from final passes.
Each stage's APK also shipped revised bundled Mail guidance (`SKILL.md`; hashes in
the build receipts); Git keeps only the final 0409 version.

| Policy stage / APK / pickup code | DeepSeek | MiniMax |
| --- | --- | --- |
| Initial / `2026100407` / 4821 | Two validation repairs before publication. Full-card buttons work, but repeated summary and missing detail-view item identity fail the layout goal. Glance code was not tested. | Two validation repairs before publication. Local buttons work, but Pickup details is horizontally clipped. |
| Layout feedback / `2026100408` / 7392 | First-attempt publication. All four phone actions work on both surfaces, but the verbose card fails the tighter native 310 × 400 target. | First-attempt publication. Buttons are invisible; detail copy invents a deadline consequence and misbinds a tracking label. Fails review. |
| Compact repair / `2026100409` / 9064 | First-attempt publication; final phone and native actions pass, with the target-size/label caveats above. | One repair for unsupported TextEyebrow width, then publication; final phone and native actions pass, with the target-size caveat above. |

The initial MiniMax [phone summary](mail-card-actions-2026-10-04/screenshots/initial-minimax-full-brief.png)
and [native record](mail-card-actions-2026-10-04/initial-minimax-native.json)
show a width failure: the first chip takes 210 points, leaving only 86 after
spacing for the second. Extra height cannot fix it. The initial DeepSeek
[summary](mail-card-actions-2026-10-04/screenshots/initial-deepseek-full-brief.png),
[details](mail-card-actions-2026-10-04/screenshots/initial-deepseek-full-details.png)
and [native record](mail-card-actions-2026-10-04/initial-deepseek-native.json)
preserve its separate issues.

At the intermediate stage, DeepSeek's [native record](mail-card-actions-2026-10-04/final-deepseek-native.json)
shows only one visible point of the Details target at 310 × 400. That failed
compact-height target does not negate its actual dynamically sized phone pass.
MiniMax's [phone card](mail-card-actions-2026-10-04/screenshots/minimax-final-full-brief.png)
and [native reproduction](mail-card-actions-2026-10-04/final-minimax-native.json)
show zero-sized controls even at 344 × 800: accepted `Chip(width: .fill)`
collapses inside Fit wrappers. **This is a renderer limitation, not invalid
L0 syntax.** The final guidance avoids the width argument and shortens content;
the general renderer defect is not fixed here. Data containing 7392 did not
make that earlier unreachable code view a successful test.

## Supported behavior and evidence boundaries

A button follows `Chip.on_tap` → declared L0 event → local `mode` change →
matching `when` view. Back restores the summary. State is temporary; these
controls do not change mailbox or remote state. The original plan's external
**Track** needs a real URL and an implemented handler; this fixture has no URL
and the shell does not execute `sys.link` writes. Real **Send**, **Confirm**,
**Mark read/Mark done**, booking and persisted completion remain outside this
test. No reply was sent and no read status or appointment was changed.

The hidden-window native harness uses pinned Makepad and a shell-equivalent L0
navigation/state adapter; it is not the Android shell. Its four-click checks
report local transitions with empty host-write lists. Phone taps and captures
provide separate Android evidence. Native fonts differ; early incomplete glyph
captures were rerun with a full redraw and were not attributed to the model.

Build receipts bind isolated package `dev.makepad.octosense.studio` to APK and
payload hashes for [0407](mail-card-actions-2026-10-04/build-receipt.json),
[0408](mail-card-actions-2026-10-04/build-receipt-final.json) and
[final 0409](mail-card-actions-2026-10-04/build-receipt-repair.json). Production
Home was not replaced. [Tool outcomes](mail-card-actions-2026-10-04/tool-results.json)
retain the five failed publication attempts as well as six successes;
[provider attribution](mail-card-actions-2026-10-04/provider-attribution.json)
identifies each turn. The [six card arguments](mail-card-actions-2026-10-04/cards/)
are unchanged copies, checked by [artifact/source hashes](mail-card-actions-2026-10-04/artifact-hashes.json).
Hashes verify preservation; they are not independent authorship replay.
Public evidence omits Gmail addresses, credentials, private profile files and
transport message identifiers. The process-lifetime and remote-action limits
in the [implementation guide](../mail-agent-events.md) still apply.
