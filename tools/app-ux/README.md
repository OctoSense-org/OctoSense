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
python3 tools/app-ux/theme_journey.py --output target/app-ux/theme-final
python3 tools/app-ux/providers_journey.py --output target/app-ux/acceptance-providers-cancel
python3 tools/app-ux/retention_journey.py --output target/app-ux/acceptance-retention-pixels
python3 tools/app-ux/mail_journey.py --output target/app-ux/mail-journey-round2
python3 tools/app-ux/news_journey.py --output target/app-ux/news-journey-round3
python3 tools/app-ux/glance_swipe_journey.py --output target/app-ux/glance-curved-native-after
python3 apps/photos/tests/ui.py --binary target/release/octosense --output target/app-ux/photos-journey-round2
```

Calendar creates a Unicode event with multiline notes, reviews it, publishes it
to Glance and checks the authoritative store after restart. The appearance
journey changes light/dark mode while a draft is unsaved, then saves that draft.
Mail uses a synthetic local demo account, reads a message and edits a reply;
it does **not** send mail. News reads live public feeds and saves a story locally.
Photos uses its existing local mock provider to exercise Memories; this does
not prove a real provider call or device photo-library access.

The desktop Calendar journey activates the hosted window, submits each input
once and waits for a separate frame capture before checking widget geometry.
This avoids repeating an already applied input when the hidden-window control
surface rejects an immediate present. Its timings include PNG capture overhead
and must not be compared with the editor-only benchmark samples.

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

Phone mode is a desktop host at phone dimensions. Device claims require separate evidence. The assigned Redmi Note 12
(Android 15) has verified Glance touch dismissal at eight starting paths and
Calendar multiline editing, focus reveal above the soft keyboard, Save with the
keyboard open and saved-detail review in an isolated test package. Physical
iOS/OHOS and assistive-input acceptance remain **unverified**. A preview also does not prove
real email delivery, model execution, map routing or video playback.

For shared shell changes, also run the packaging and feature checks listed in
the repository [AGENTS.md](../../AGENTS.md). The harnesses quit only their own
processes and do not replace installed applications or personal profiles.
