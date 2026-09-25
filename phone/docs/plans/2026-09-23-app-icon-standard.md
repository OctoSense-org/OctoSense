# Shared App Icon Implementation Plan

**Goal:** Apply the approved Hub icon convention, show actual app icons in preview,
refine the selected bloom icon with an app grid, and validate on the ADB phone.

**Architecture:** Reuse Makepad's shared app identity/icon catalog for built-ins.
App Hub owns its canonical SVG and Hub-compatible listing declaration. Existing
theme assets retain ownership during the incremental migration. Installed apps
resolve their icon from the verified local bundle, never from an app-ID guess.

**Tech stack:** Rust, native Makepad widgets, SVG, Hub listing metadata, ADB.

- [x] Add a native regression that preview rows/details use the shared app icon
  widget, including a recycled live-catalog row; retain existing all-theme checks.
- [x] Replace preview-only glyphs in `apps/app-hub/src/view.rs` with the shared
  `AppIcon`, preserving full-color rendering and matching launcher geometry.
- [x] Refine bloom artwork; create canonical `apps/app-hub/assets/icon.svg` and
  its listing declaration. Route shell and Hub branding to the same asset.
- [x] Resolve installed bundle artwork for shell icon surfaces with bounded
  loading and existing path rules; test format, size and fallback behavior.
- [x] Document the adopted convention, ownership transition and icon provenance.
- [x] Run focused Rust tests, build the APK, install on the connected Pixel,
  inspect launcher/Hub icon consistency, search/details/open and save evidence.

Keep work on the existing `feat/app-hub` branch; do not commit or publish.

Verified: 35 App Hub tests, 293 shell tests with all mobile features, release
desktop and Android builds. Pixel 7 Pro: actual launcher, preview icons, signed
live empty catalog, Photos search/details/Open. Local signed desktop fixture:
installed icons match between App Hub and the launcher. No store install was
possible on the phone because the production catalog remains empty.
