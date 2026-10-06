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
body. Save/conflict state remains visible, and exact-message review still uses
the separate host-owned approval path.

The input palette now covers pressed, focused, disabled and gradient states.
Light appearance uses a `#d6dee9` selection band on the existing light editor;
dark appearance uses `#4d596d` with light text. Selection does not change the
whole input background. These are host widget changes: the model-authored card
source, Mail draft authority and physical send approval are unchanged.

## Validation

The first geometry run failed its short-viewport content-space requirement.
The measured body was 128 points high in a 220-point pane. Inherited label padding
was then removed; the failed run is retained separately from follow-up results.

Final local checks and OnePlus 6 observations are pending. No new model quality
comparison, complete accessibility acceptance or SMTP delivery is claimed.
Private mail, drafts and device screenshots remain outside this public report.
