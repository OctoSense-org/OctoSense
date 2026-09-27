# Phone themes

Open **Wallpaper & style** by long-pressing an empty area of Home. On the ROM,
it is also available in **Settings → Display**. Pick OctoSense (blue), Minimal
(neutral), Paper (warm), or Vivid (purple), then choose System, Light or Dark
appearance and a gradient or solid background.

The preview changes immediately; Home and other apps change only after **Apply**.
**Cancel** or Back discards edits. **Restore default** selects OctoSense with
System appearance and the theme gradient; tap Apply to save it. English and
Chinese picker text is included. Choices survive restarting Home and installing
an update that preserves app data.

OctoSense Home and theme-aware hosted widgets share the full preset. On an
OctoSense ROM the picker also updates Android's dynamic color seed, explicit
light/dark mode, and system wallpaper. Other Android apps choose whether to use
those dynamic colors. A standalone APK cannot change all privileged system
styling; it reports that limitation after applying the OctoSense theme.

Implementation: [ADR 0002](../../../docs/adr/0002-rom-themes.md).
App integration and migration status: [shared theme contract](app-theme-contract.md).
Some custom app interfaces still use fixed colors; receiving the shared widget
theme does not automatically recolor those surfaces.

## Validation, 24 September 2026

- All 313 app tests passed with `cargo test --locked --bin octosense --features mobile-apps`.
- All 27 repository tests passed, including the existing kernel transaction checks.
- OnePlus 6: Settings → Display and Home long-press entries work. All four presets
  were applied; Android's Monet seed and explicit night mode updated successfully.
- An open `reference` search retained its query across a palette change. Android's
  activity-create count stayed at 25 during the switch and subsequent external
  light/dark toggles. Follow-system updated Home in both directions without
  recreating its activity.
- Android emulator: preview cancellation, Paper/Dark/Solid persistence across a
  process restart and in-place APK update, Chinese text, and Restore default passed.
- Home APK version `2026092405` installed on both devices. Phone updates used its
  existing build checkout to retain local Mail, Octos and camera work; only Home
  was installed, preserving the OTA Bridge. Emulator used the main-based build.
  No ROM flash or phone reboot was needed. Both ended on OctoSense/System/Gradient.

## OnePlus 6 frame pacing

The existing `measure_android_frames.py` harness used Makepad's `phone.frames`
and input markers with SurfaceFlinger presentation timestamps. Six baseline
swipes used Home `2026092403`; ten candidate swipes used `2026092405`. Swipes
alternated `(800,1050) → (220,1050)` and reverse over 800 ms. Checks required a
received touch, the Home scene and a page-position change greater than 0.8.
The performance overlay was off, and screenshots were captured separately.

| Theme | Runs | FPS range | Highest run p95 | Missed refreshes / intervals |
| --- | ---: | ---: | ---: | ---: |
| Previous Home baseline | 6 | 57.99–59.80 | 16.75 ms | 7 / 391 |
| Paper, dark, gradient | 4 | 57.94–59.74 | 16.75 ms | 4 / 258 |
| OctoSense, dark, gradient | 6 | 58.79–59.72 | 16.78 ms | 5 / 390 |

These short sequential runs show comparable Home swipe pacing near the display's
60 Hz limit. Occasional 33.5 ms intervals remain. This does not establish an FPS
improvement, uninterrupted 60 FPS, or performance in every hosted application.
Per-run results are in [theme-frame-validation.json](theme-frame-validation.json).

Phone APK SHA-256: `9da7531c0bb4d7ef5ae3880543c78235adfecae3f5ecb29f2b90b84b46c024c5`.
Main-based APK SHA-256 before emulator development signing:
`fadaddc21640019d5452b61a17a86fdc21fa0d1538d12a6c80496d72c01f67c0`.
