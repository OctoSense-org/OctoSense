# Photos Library Zoom Implementation Plan

> **For Codex:** Execute this plan in the current session with the executing-plans
> and test-driven-development skills. User explicitly requested Library zoom.

**Goal:** Zoom the Library's photo grid with a mobile pinch and desktop scrolling,
with a visible desktop zoom slider.

**Architecture:** Keep the existing catalog, dated rows, image cache and PortalList.
Vary Library density from one to nine columns (three by default), preserving the
photo at the pointer/pinch midpoint when rows reflow. Track multi-touch before
child input so a pinch neither opens a photo nor becomes list scrolling.

**Tech stack:** Rust, native Makepad TouchUpdate/Scroll events, Slider and PortalList,
existing Octoscript UI, native desktop remote control and Android/ADB.

## Reference and design

Read `/Users/guofoo/git/mp/makepad/apps/photos/src/view.rs` and its implementation
in `libs/image_tiles/src/grid.rs`. It uses exponential wheel zoom
`exp(-delta_y * 0.0025)` anchored at the pointer. Its picture-wall data source and
camera renderer differ from this app's dated Library; reuse the interaction rule
within this app's grid. Zoom changes density while preserving square cells.

Visual thesis: retain the white photographic canvas and blue controls. Content:
desktop-only compact zoom slider above Library, with existing date headings/grid.
Interaction: pinch-to-resize, wheel-to-resize, and anchored row reflow. One-finger
dragging and vertical scrollbar navigation remain available. Preserve density
across viewer, tab and home-card transitions; other collections keep their layout.

## Tasks

1. Add failing zoom-state and gesture tests in `apps/photos/src/zoom.rs` and a
   controller regression in `apps/photos/src/view.rs` for variable row density
   and anchor preservation. Verify the intended failures with Photos tests.
2. Implement bounded scale, pinch tracking/cancellation and pointer anchoring.
   Extend Grid to nine reusable slots, varying visible slots and square cell sizes.
3. Add the desktop Slider in `apps/photos/src/ui.rs`, bind it to the same zoom
   state, and intercept wheel/pinch only over the Library content. Stop list
   motion when pinch takes over and suppress photo activation until fingers lift.
4. Run Photos tests and `cargo check --features mobile-only,app-photos --locked`.
5. Validate desktop wheel/slider/scroll/tap and Android two-finger pinch, lift,
   ordinary drag/tap, bounds and return navigation. Inspect screenshots/logs.
6. Update `docs/photos.md` and the ongoing task notes with verified behavior.

## Acceptance

- [ ] Pinch out enlarges Library photos; pinch in shows more photos per row.
- [ ] Desktop wheel and zoom slider change thumbnail density.
- [ ] Reflow preserves the focal photo vertically as far as content bounds allow.
- [ ] Pinch release does not open a photo or fling the library.
- [ ] Ordinary drag, scrollbar, photo opening, tabs and home previews still work.
- [ ] Relevant tests, shell compile, desktop and Android validation pass.
