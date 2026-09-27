# Floating navigation device validation

Validated on 2026-09-22 from the local candidate based on `61cce5d`;
the package receipts record the working tree as dirty. Android Home version
**2026092203** and the signed OpenHarmony HAP
are installed as updates. The existing OnePlus default Home role, gesture
navigation mode, signer and first-install record were retained.

The floating ball provides explicit navigation inside Home. Tap for **返回首页**
and **最近应用**, drag to dock on either side, and tap outside to collapse.
The 48-point target stays inside native gesture boundaries. It reserves no
content height, moves above the native keyboard, and belongs to Home's window.
External Android activities have no OctoSense floating overlay.

## OnePlus 6 / Android 15

Passed with native ADB touch input and inspected screenshots:

- Open/collapse the panel and return from News to Home.
- Open Recents, launch Reference, and switch from Reference back to News.
- Drag to the left dock; the panel opens toward the screen interior.
- Tap the underlying Following tab while the panel is open: only the panel
  closes. A second tap activates Following.
- Focus a News input: only the native keyboard appears and the ball stays
  above it. Returning Home clears focus and hides the keyboard.
- Launch Android Settings: no floating overlay appears there. Native side
  Back from Display Settings returns to Home; native bottom Home from Settings
  also returns to Home.
- Pull the native notification shade while the floating panel is expanded;
  SystemUI receives focus. Dismissing it returns to Home.

Screenshots, window dumps and installation comparisons are in the local ignored
`out/device-validation/oneplus6-floating/`. The final package and build receipt
are in `out/home/android-floating/`. Bridge and Quickstep were not replaced.

## Mate 70 Air / OpenHarmony

The same ball, panel, docking, Recents and outside-dismiss checks pass.
Native side Back and bottom Home return to the Huawei launcher; the top pull
opens Huawei Control Center. Native system bars remain visible.

Input testing exposed two existing keyboard issues: a second Home keyboard was
drawn over the native one, and asynchronous per-frame show requests could
reopen the native keyboard after returning Home. Home now uses the native IME
and the generated ArkTS bridge serializes/coalesces show/hide requests. The
final screenshot confirms that returning Home hides the native keyboard.

Local evidence is in `out/device-validation/mate70-floating/`, including
`serialized-native-keyboard.jpeg` and `serialized-home-keyboard-dismissed.jpeg`.
The final HAP and build receipt are in `out/home/ohos/`.

## Checks and scope

- `cargo test --locked --offline --bin octosense --features mobile-apps mobile_`:
  **77 passed**, including drag/tap separation, complete dismissal-stream
  capture, keyboard avoidance, safe-area geometry and system-edge exclusions.
- Product/build-script unit tests: **14 passed**. Android and OpenHarmony release
  packaging and `git diff --check` pass.

This validates navigation on these two devices. It does not measure animation
frame timing, validate a new ROM image, or resolve the existing Quickstep signer
mismatch. News had no internet on the OnePlus and a TLS error on the Mate;
network delivery was not part of these interaction checks.
