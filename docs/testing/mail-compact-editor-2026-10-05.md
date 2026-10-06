# Compact Mail editor — 2026-10-05

English | [简体中文](mail-compact-editor-2026-10-05.zh-CN.md)

The person reported that reply controls crowded out the message and selecting
text darkened the editor. The native Mail pane repeated the Glance summary,
reserved a full row for the original-message action, always expanded address
fields, and placed a full-width Review button beneath the save status.

## Change

Original, Details and Review share one 44-point action row. The body remains
directly editable; Details expands recipient and subject editing. The compact
header identifies the saved reply. The Glance summary stays in the feed instead
of being repeated above the editor. Original-message headers scroll with its
body. On panes shorter than 320 points, the header preview collapses; Details
still exposes the fields. On very short keyboard viewports, focused body editing
temporarily hides the title, tabs, action row and routine saved label. Android
Back or the keyboard-dismiss control restores them. Error/conflict state remains
visible. Exact-message review still uses the separate host-owned approval path.

The input palette now covers pressed, focused, disabled and gradient states.
Light appearance uses a translucent `#829ab5` selection band at 25% opacity;
dark appearance uses `#90a6c3` at 28% opacity with light text. Selection does not change the
whole input background. These are host widget changes: the model-authored card
source, Mail draft authority and physical send approval are unchanged.

## Validation

The first geometry run failed its short-viewport content-space requirement.
The measured body was 128 points high in a 220-point pane. Inherited label padding
was then removed; the failed run is retained separately from follow-up results.

Build 2026100519 passed 935 Shell and 55 Mail tests (two optional Mail tests
ignored), the desktop/phone checks, both dependency graphs and pins/catalog/
private-path checks. Native geometry measured editor heights of 620, 180 and
140 points within panes of 700, 260 and 220 points, respectively, with compact
header fields hidden as for body editing above the keyboard. Every action kept
a 44-point target. This is native widget geometry, not display-frame timing.

The first phone color check failed: its opaque gray selection obscured glyphs,
even though the whole editor stayed light. That build is not a selection pass.
The follow-up makes the band translucent, including focused/pressed states.

OnePlus 6 build 2026100520 verified readable partial selection and Select all,
the compact portrait editor above the actual Android keyboard, and Review/Back.
The exact displayed reply matched the saved message. No text was typed, cut or
pasted by the test driver, and no approval/send was activated. All thirteen
draft recipients, subjects, bodies and revisions matched before/after this
review check; one lifecycle record changed because review was cancelled.
Both upgrades to 0519 and 0520 separately preserved all thirteen draft files
byte-for-byte at their immediate upgrade checkpoints. The person also used the
phone between checkpoints, so those observations do not claim the entire live
session was idle or immutable.

The same-device portrait captures show substantially more message content than
the earlier stacked controls. The unchanged layout on 0519 also verified Details
expansion and Original/Reply with scrollable source headers. However, the 0520
landscape check still gave too much room to metadata; that failure prompted the
short-viewport header rule above.

Build 2026100521 verified the landscape header collapse and readable Select all
on the phone. Its immediate upgrade preserved all thirteen draft files exactly.
However, opening the landscape keyboard exposed another failure: the fixed
title and tab rows consumed the remaining editor space. That observation prompted
the focused-body layout above; 0521 is not a landscape-keyboard pass.

The follow-up source at `f2fe188f4e2ddce0ee7d38c9d6552ae0990cac60` passed
935 Shell and 55 Mail tests (two optional Mail tests ignored), desktop checks
with and without mobile apps, phone checks, both dependency graphs, runtime
pins, catalog and private-path checks, and the release Android build. The native
geometry test now includes a 96-point keyboard viewport: the focused editor
gets 88 points, and returning to 700 points restores the 44-point action row
without changing the text.

Normal OnePlus 6 Home **2026100522** is installed. Its APK SHA-256 is
`2eca35530ae17c208ce6c50d45a120084a10fef8d15c82bc01e1cdb604ef6493`;
the two compiled host source hashes match the revision above. On that build,
ADB/platform captures verified the compact portrait editor, readable Select all,
the body above the landscape keyboard, cursor navigation, restoration of the
normal controls after Android Back, and exact review followed by Back. The first
landscape keyboard opening left the cursor line partially at the lower clip edge;
cursor navigation scrolled it into view. This is not a complete IME/caret-scroll
acceptance result. Dark appearance was not exercised on the phone.

The 0522 upgrade preserved all thirteen draft files byte-for-byte. After the
read/selection/review checks, all thirteen recipients, subjects, bodies and
revisions remained unchanged; one draft's attempts list changed when review was
cancelled. Codex authored the native host changes and drove ADB; no draft text
was entered, cut or pasted, no new model turn was requested, and no send approval
was activated. Model-authored live L0 card source was not rewritten. Temporary
orientation/stay-awake settings were restored and ADB returned to its non-root
shell UID.

The phone observations are ADB/platform captures; the layout measurements are
native Makepad widget tests. No new model quality comparison, full accessibility
acceptance, display-frame benchmark or SMTP delivery is claimed. Private mail,
drafts and device screenshots remain outside this public report.
