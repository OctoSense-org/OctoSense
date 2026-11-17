# Native app UX checks

English | [简体中文](README.zh-CN.md)

These checks run owned, hidden Makepad windows with isolated app data. They
exercise the actual hosted apps through pointer and keyboard input and save
original PNGs, widget snapshots, process logs and result receipts under
`target/app-ux/`. No screenshot is repainted or cropped for acceptance.
Use a fresh output directory for a new run; keep failed attempts separately.

## Build and run

Run from the repository root. These commands were exercised on macOS:

```sh
python3 tools/setup.py
python3 tools/sync-app-interface.py --check
cargo build --locked --release -p octosense --no-default-features --features app-hub,dev-mode
python3 tools/app-ux/calendar_journey.py --output target/app-ux/calendar-final
python3 tools/app-ux/calendar_journey.py --desktop --output target/app-ux/calendar-desktop-separate-frame
python3 tools/app-ux/calendar_storage_journey.py --appearance light --action new --output target/app-ux/calendar-storage
python3 tools/app-ux/calendar_edit_storage_journey.py --appearance dark --narrow --output target/app-ux/calendar-edit-storage
python3 tools/app-ux/calendar_history_storage_journey.py --appearance light --scenario edit --output target/app-ux/calendar-history-storage
python3 tools/app-ux/pdftools_readonly_journey.py --appearance light --output target/app-ux/pdftools-readonly
python3 tools/app-ux/pdftools_save_storage_journey.py --appearance dark --narrow --output target/app-ux/pdftools-save-storage
python3 tools/app-ux/pdftools_comment_journey.py --appearance light --output target/app-ux/pdftools-comment
python3 tools/app-ux/pdftools_edit_journey.py --appearance dark --output target/app-ux/pdftools-edit
python3 tools/app-ux/pdftools_history_journey.py --appearance light --output target/app-ux/pdftools-history
python3 tools/app-ux/pdftools_search_focus_journey.py --appearance dark --output target/app-ux/pdftools-search-focus
python3 tools/app-ux/maps_search_focus_journey.py --appearance light --output target/app-ux/maps-search-focus
python3 tools/app-ux/maps_saved_storage_journey.py --appearance dark --output target/app-ux/maps-saved-storage
python3 tools/app-ux/maps_read_storage_journey.py --appearance light --list saved --output target/app-ux/maps-read-storage
python3 tools/app-ux/maps_prefs_storage_journey.py --appearance dark --output target/app-ux/maps-prefs-storage
python3 tools/app-ux/maps_prefs_read_journey.py --appearance light --edits --output target/app-ux/maps-prefs-read
python3 tools/app-ux/maps_many_saved_journey.py --appearance light --output target/app-ux/maps-many-saved
python3 tools/app-ux/maps_saved_paging_journey.py --appearance dark --output target/app-ux/maps-saved-paging
python3 tools/app-ux/maps_phone_paging_journey.py --appearance light --output target/app-ux/maps-phone-paging
python3 tools/app-ux/maps_narrow_toolbar_journey.py --appearance light --output target/app-ux/maps-narrow-toolbar
python3 tools/app-ux/macos_spotlight_journey.py --appearance light --output target/app-ux/macos-spotlight
python3 tools/app-ux/theme_journey.py --output target/app-ux/theme-final
python3 tools/app-ux/providers_journey.py --output target/app-ux/acceptance-providers-cancel
python3 tools/app-ux/retention_journey.py --output target/app-ux/acceptance-retention-pixels
python3 tools/app-ux/mail_journey.py --output target/app-ux/mail-journey-round2
python3 tools/app-ux/mail_journey.py --desktop --output target/app-ux/mail-journey-desktop
python3 tools/app-ux/writer_journey.py --appearance light --output target/app-ux/writer-find-light
python3 tools/app-ux/writer_journey.py --appearance dark --output target/app-ux/writer-find-dark
python3 tools/app-ux/writer_draft_storage_journey.py --appearance light --scenario draft --output target/app-ux/writer-draft-storage
python3 tools/app-ux/writer_conversion_storage_journey.py --appearance dark --action save --output target/app-ux/writer-conversion-storage
python3 tools/app-ux/photos_rename_journey.py --appearance light --output target/app-ux/photos-rename-light
python3 tools/app-ux/photos_rename_journey.py --appearance dark --output target/app-ux/photos-rename-dark
python3 tools/app-ux/news_journey.py --output target/app-ux/news-journey-round3
python3 tools/app-ux/news_journey.py --desktop --output target/app-ux/news-journey-desktop
python3 tools/app-ux/glance_swipe_journey.py --output target/app-ux/glance-curved-native-after
python3 apps/photos/tests/ui.py --binary target/release/octosense --output target/app-ux/photos-journey-round2
```

Calendar creates a Unicode event with multiline notes, reviews it, publishes it
to Glance and checks the authoritative store after restart. The appearance
journey changes light/dark mode while a draft is unsaved, then saves that draft.
The macOS Spotlight journey needs Pillow and a shell built with `mobile-apps`;
it checks that the selected result's preview follows keyboard and pointer,
and checks scrolling and a narrow window.
`macos_glance_focus_journey.py` checks that Return and Delete act on the
visible top notification after a scrolled panel is closed and reopened, and
that Undo restores a dismissed notification. It uses synthetic notices and
disposable profiles; run it separately for light and dark appearances.
The macOS Glance groups journey checks grouped notices, per-app clear and
Undo, keyboard navigation and the transition back to a single card, using
synthetic notices and disposable profiles in each appearance.
The macOS Glance widgets journey checks local date and time, keyboard focus,
pointer cancellation and app launch, including a narrow grouped feed.
The Calendar storage journey blocks a temporary file only inside a fresh
profile and checks failure, cancellation and retry for `new` or `duplicate`.
It does not simulate a crash or a failure after the store has been replaced.
The edit-storage journey checks that failed edits stay open until retried or
explicitly discarded; `--narrow` exercises a 700-point window.
The history-storage journey checks that a blocked Undo or Redo can be cancelled
and retried for an `add` or `edit` without losing the history step.
The PDF Tools journey generates a local two-page form and checks read-only field
navigation, saving an active edit, saving on close and reopening the PDF, appearance
changes and a 700-point window. It uses the real local PDF service; it does not use
an external document or simulate a service failure.
The PDF save-storage journey temporarily makes only its disposable library directory
read-only, then checks the persistent error, retry and saved value after reopening;
`--narrow` also checks a 700-point window. It does not simulate a crash.
The PDF comment journey uses a synthetic author and local form to check note and
reply posting, appearance changes, reopening, and deletion with an unsent reply.
The PDF edit journey checks a paragraph draft across document tabs and an
appearance change, then applies it and reopens the saved PDF.
The PDF history journey checks field Undo and Redo, an appearance change, and
the saved value after reopening a generated local PDF.
The PDF search-focus journey checks search input and caret across appearance
changes, including an empty search and an active form field, then saves and
reopens the edited PDF.
The Maps search-focus journey uses a disposable saved place to check the search
caret, clearing a search, map focus and choosing a route start across appearance
changes. It does not test live search or route services.
The Maps saved-storage journey makes only its disposable Saved Places file
read-only and checks the notice, repeated failure, appearance change,
dismissal, retry and reopening. It does not simulate a partial write or crash.
The Maps read-storage journey makes one disposable Saved or Recents file
unreadable at startup, checks that it is not replaced, and retries after access
returns. Use `--list saved` or `--list recents`; `--narrow` also checks a
480-point window, and `--corrupt` tests a truncated list repaired before retry.
The list check detects a missing outer bracket; it does not detect every
malformed list that still has both brackets.
The Maps preferences journey refuses writes to a disposable preferences file,
then checks the warning, retry and selected style after reopening. It does not
simulate a partial write or crash.
The Maps preferences read journey starts with an unreadable disposable settings
file and checks recovery and reopening. With `--edits`, it also checks that
choices made before recovery are merged with the untouched stored settings;
`--corrupt` tests a truncated file repaired before retry.
Settings larger than 512 bytes are treated as unavailable to keep validation
within the UI script budget.
The Maps many-saved journey uses 100 disposable places to check loading,
keyboard navigation, scrolling, context menus, appearance changes and
saved places after reopening.
The Maps saved-paging journey uses 500 disposable places to check page
navigation, retained page selection, keyboard and context actions, and
persistence after reopening; `--count 1000` uses a larger list and `--narrow`
also checks the 480-point window. The phone-paging journey checks saved-place
actions and verifies that map pins remain behind the search sheet after an
appearance change. It is a phone-sized Mac preview, not a physical-device test.
The Maps narrow-toolbar journey checks the overflow menu, style selection and
resizing, and uses Pillow to check its appearance after a theme change. With
`--offline`, it also checks the map-error notice at 480 and 700 points; run the
binary with external network blocked for that option.
Mail uses a synthetic local demo account, reads a message and edits a reply;
it does **not** send mail. News reads live public feeds and saves a story locally.
Photos uses its existing local mock provider to exercise Memories; this does
not prove a real provider call or device photo-library access.
Writer checks Find and Replace navigation, focus after an appearance change,
and undo on a disposable document.
The Writer storage journeys make a file read-only only inside their fresh output
profile, then check the warning, retry and recovery. Draft scenarios are
`draft`, `index`, `index-revert` and `new-index`; `--narrow` also checks a
700-point window. Conversion actions are `save`, `preview` and `export` and
use the real local Word service. These tests do not simulate a partial write,
a crash, or an external export destination.
The Photos rename journey checks selection, editing a Unicode name, Escape and
restart using a disposable library.

The desktop Calendar journey activates the hosted window, submits each input
once and waits for a separate frame capture before checking widget geometry.
This avoids repeating an already applied input when the hidden-window control
surface rejects an immediate present. Its timings include PNG capture overhead
and must not be compared with the editor-only benchmark samples.

A hidden window is never the key window, so `files.import` and `files.export`
answer `foreground_required` in it. Send `/w?k=focus` first to tell the app its
window is in front. With `MAKEPAD_TEST_SAVE_DIR=<folder>` set, a hidden window's
macOS save panel then answers `<folder>/<suggested name>`; without it the panel
answers as cancelled. The open panel answers the same way: with
`MAKEPAD_TEST_OPEN_FILE=<file>` it picks that file, and without it the panel
answers as cancelled.

The provider journey opens the real host-owned Add model sheet and cancels; it
does not configure or call a provider. The retention journey captures six distinct
Unicode multiline drafts after switching through five other Glance workspaces.
These custom fixture fields are absent from the native widget snapshot, so the
receipt requires a separate visual review of all six original returned images.

The Glance swipe journey starts with a feed that fits on screen and checks left
swipes at four heights, each beginning with an upward or downward thumb arc.
The regression failed in the old native build and passes in the fixed build.
This preview drives mouse events; the corresponding Android check is
`android_glance_swipe.py`, which requires an explicitly assigned `--serial` and
the separately installed `dev.makepad.octosense.glanceswipe` test package. It
dispatches Android touch events, captures original device PNGs and stops if
another app takes the foreground. It never clears app data or changes Home.

Capture both widths and appearances:

```sh
python3 tools/app-ux/capture.py --output target/app-ux/final-phone-light --apps calendar photos mail ai-providers youtube maps --mode phone --settle-seconds 12
python3 tools/app-ux/capture.py --output target/app-ux/final-phone-dark --apps calendar photos mail ai-providers youtube maps --mode phone --dark --settle-seconds 12
python3 tools/app-ux/capture.py --output target/app-ux/final-desktop --apps calendar photos mail ai-providers youtube maps apphub --mode desktop --settle-seconds 12
```

Capture success means pixels were obtained, not that every network resource
loaded or the screen passed review. Inspect each original image. Additional
settling time accommodates resource loading; it is not a performance sample.

`hub_journey.py` drives App Hub's native `preview` example, with `--binary`
pointing to that executable. It browses the labelled preview catalog, searches,
recovers from empty results and opens app details without installing anything.
It accepts `--size 1200x860` and `--dark`. Build the example in the companion
App Hub checkout at the revision pinned in this repository.

## Performance and acceptance

`benchmark.py --before <binary> --after <binary> --output <directory>` compares
24 Calendar editor-open observations per binary. It records p50, p95, maximum,
all samples and binary hashes. This is pointer-dispatch-to-widget-observation
wall time including HTTP overhead, **not GPU frame latency or FPS**. A copied
cold binary can take longer to load its host resources; startup is outside the
samples. Control machine load before interpreting small differences.

Review task clarity, readable content, keyboard reachability, retained state,
responsiveness and visual consistency separately. Pixel diagnostics cannot
establish a 95/100 UX result or override a broken task. Record the actual author,
driver and reviewer; a self-review is not independent acceptance.

## The Mac look

`macos-rubric.json` is the 95/100 for a Mac desktop: eight weighted dimensions
(structure, controls, typography, color, clarity, fit, continuity, consistency),
hard gates, and the surfaces it covers. `macos_score.py <card.json>` turns a
reviewer's grades into the result and refuses a card with a gap. It only does the
arithmetic: a grade is a reviewer's judgement over original captures, and the
author's own grade is not independent acceptance. `--test-action style:macos` and
`style:macos-dark` choose the style for a capture.

Phone mode is a desktop host at phone dimensions. Device claims require separate evidence. The assigned Redmi Note 12
(Android 15) has verified Glance touch dismissal at eight starting paths and
Calendar multiline editing, focus reveal above the soft keyboard, Save with the
keyboard open and saved-detail review in an isolated test package. Physical
iOS/OHOS and assistive-input acceptance remain **unverified**. A preview also does not prove
real email delivery, model execution, map routing or video playback.

For shared shell changes, also run the packaging and feature checks listed in
the repository [AGENTS.md](../../AGENTS.md). The harnesses quit only their own
processes and do not replace installed applications or personal profiles.
