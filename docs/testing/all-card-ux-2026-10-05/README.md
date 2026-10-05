# All-card phone UX acceptance — 2026-10-05

English | [简体中文](README.zh-CN.md)

This acceptance covers every Glance publisher in the phone's current system-app
catalog: **Mail, Calendar, News, Photos, Maps, Camera and YouTube**. It also
exercises the Android-authored DeepSeek and MiniMax six-family collections,
including the unprivileged Finance prototype. AI Providers has no card and is
not applicable. This is not an audit of every native app, third-party bundle or
external business API.

The assigned OnePlus 6 ran isolated Lab builds `2026100501`–`2026100504`, based on
`a2a0524d` plus this change. [Build receipts](builds.json) identify the APKs and
source hashes. All card data was fictional; the installed Home and the separate
user Gmail test package were not replaced. ADB drove interactions and captured
the real screen. Makepad headless tests checked actual layout geometry; a new
Studio Instrument session was **not** used for this run.

## Coverage and verdicts

The matrix contains **33 input configurations**: nine shipping notice/event/
agenda cases, 12 model L0 sources and 12 model Splash programs. Light-mode
interaction and dark-mode initial rendering are separate checks, not 66 full
end-to-end business tests. See [case results](results.json) and the
[screenshot gallery](index.html).

| Publisher / family | What was exercised | Boundary |
| --- | --- | --- |
| Mail | Shipping notice, both model reply views, generated editors with keyboard, local draft retention, host Card/Chat | Model Send/Queue is a demo; authoritative Email/Chat uses a separately bound host draft |
| Calendar | Long event date/location/notes, three-event agenda, empty agenda, both model RSVP choices and list views | Local RSVP does not send an invitation response |
| News | Long notice, both model Save controls and article/list views, denied-assistant state | No live feed fetch or backend bookmark asserted |
| Photos | Long notice, both model Favorite controls and metadata/list views | No photo-library integration asserted |
| Maps | Long bilingual notice, native Chat input, keyboard and collapse/reopen | No route or navigation effect asserted |
| Camera | Long bilingual notice, first-use assistant consent, native Chat input and retention | No camera capture asserted |
| YouTube | Long notice, both model Watch later controls and list views | No playback or remote playlist update asserted |
| Finance prototype | Both model Watch controls and list views; no invented Chat tab | `test.finance` has no app agent or business privileges |

Both L0 collections completed local action, collapse/reopen and native composer
checks where consent allowed. Both Splash collections opened their L0/L1/L2
views. DeepSeek's explicit Loading/Empty/Error/Ready selectors were exercised;
MiniMax's collection does not expose equivalent lifecycle selectors, so those
states were not credited to it. Full Splash programs were loaded unchanged as
card publications; bundle installation, assets and App Hub admission were not
part of this path.

## Defects found and corrected

| Finding | Correction and evidence |
| --- | --- |
| Full `Fill` roots rendered blank inside a `Fit` wrapper | The host gives those roots the workspace height. Natural-height L0 still uses the outer scroller. All 12 Splash programs rendered on the phone. |
| Several full programs exceeded the old shared 16 KiB cap | L0 remains 16 KiB; Splash gets a separate bounded 64 KiB cap. Exact-limit acceptance and over-limit rejection are tested. |
| Long Calendar agenda metadata squeezed event titles out of the row; empty agendas retained blank rows | Stack title/metadata with bounded width, omit absent event rows and empty trailing summaries. |
| Long Calendar event dates were truncated in half-width tiles | Date and time use full-width labelled rows. Long notes remain scrollable. |
| DeepSeek Photo/YouTube/News explanations clipped horizontally | Android DeepSeek viewed the screenshots and added wrapping width in all six families' explanation captions. |
| A nested Splash reply editor disappeared behind the keyboard | The host follows the focused editor's actual navigation scroll ancestors, from inner to outer. The editor and following controls now scroll into view; manual scrolling is not continually reset. |
| MiniMax Mail's L1 Reply did not reveal the editor | Android MiniMax corrected its own `do_reply` to select the L2 parent before displaying the reply route. Keyboard input and collapse/reopen were then exercised. |
| DeepSeek's inherited button typography clipped States in all six full-app navigation rows | DeepSeek's first patch still clipped. After a second screenshot review, its explicit 11-point font and tighter padding passed the six phone captures, retaining 44-point height. |

Model source corrections, including the DeepSeek navigation typography review,
are preserved with successful tool mutations, original/output hashes and replay
verification in [App Design Flow PR #146](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/pull/146).
The operator supplied screenshots and feedback; it did not hand-edit generated
app source. DeepSeek and MiniMax used their configured providers with fallbacks
disabled; terminal ledger attribution identifies the actual model.

Some initial scripts failed because they attempted an action before opening
MiniMax's detail view, captured an intermediate frame, or matched two Ready/Open
app labels. The failed runs remain in the local evidence. Corrected selectors,
waiting and explicit follow-ups are recorded separately; these operator errors
are not relabelled as application defects or counted as successful first runs.
The last caption request also failed while the locked phone restricted the Lab
app's background network. After waking and returning it to the foreground, the
same configured DeepSeek provider completed the four remaining source edits.

## Checks and reproduction

All six cards were used consecutively, then reopened: the five local selections
and Mail's unsent host-chat text survived beyond the three-clean-workspace cache
limit. News was tested both with its existing refusal and, in the isolated Lab,
through the native first-use Allow sheet; the allowed composer accepted text and
retained it after collapse. The original Lab consent file was restored afterward.

During 12 alternating 600 ms Glance drags, 416 consecutive CPU frame gaps
within the same recorded Down/Up pair had median **16.74 ms**, p95 **17.72 ms**,
maximum **36.57 ms**, with nine above 25 ms. [Raw and segmented metrics](performance.json)
retain the unsegmented sample too: it includes pauses between ADB gestures and
must not be described as continuous scrolling latency. These numbers exclude
post-release fling and do not measure touch-to-display latency.

Three additional SurfaceFlinger samples measured **56.9–59.7 fps** during
marked active spans, with maximum presentation intervals of **16.77, 33.50 and
33.48 ms**. The report preserves idle-inclusive statistics separately. The
first presentation after input is not proof of visible response latency;
the scroll did not produce a separate measured state-response marker.

Executed checks: **924 shared-shell tests**, three Calendar service tests,
desktop default and mobile-apps checks, phone mobile-apps check, both shell
dependency graphs and runtime pin validation. The new Makepad drawing
regressions cover a full-height root, a keyboard-sized viewport, nested editor
and action geometry, retained text, and manual scroll without snapping back.

[`tools/card-workspace-fixtures.py`](../../../tools/card-workspace-fixtures.py)
exports the fictional inputs and source hashes used by the developer
`glance-fixtures:<path>` action. Shipping agenda export and the exact repaired
DeepSeek Photo export were executed. For a model case, provide the archived
continuations directory, model, syntax and optionally `--source` pointing to an
exact model-authored replacement. The tool neither operates a phone nor declares
acceptance; normal publisher grants still apply.

## What this does not pass

This is a scoped workspace acceptance, **not a 9/10 production UX sign-off**.
The prototype review chrome remains visually different from the native
Email/Chat workspace. Model sources paint their own light surfaces even in a
dark host; readable contrast does not establish adaptive dark-theme design.
MiniMax's fixed header/footer leave a small reply viewport with the keyboard;
the editor is usable, but review actions can require scrolling or hiding the
keyboard. These are visible polish limits, not evidence for inflating a score.

No fresh blind-email trigger, SMTP send, real RSVP, route planning, capture,
photo access, playback, live quote, full Chinese-localization audit, landscape,
screen-reader or large-font acceptance is claimed. Prior real Mail and model
tests remain separately described in [Composed Mail cards](../../mail-composable-cards.md).
Only physical human input may approve real Mail sending; this run did not do so.
