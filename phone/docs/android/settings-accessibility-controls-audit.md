# Accessibility controls: native audit and next implementation proposal

This started as a read-only preparation record. The bounded color-controls implementation is now in progress; the bounded controls passed native emulator acceptance in Home2026092529. It uses pinned Settings `0f0669fc699f70adb20fe6ed4b2ff1c600da6f86` and framework `ff7620a38e54c5f7ec14a5b8ccc5be1ba41e2b1b`. The bounded slice implements color inversion and color correction in the built-in Octoscript UI, using the existing finite Controls transport. These operate in Android's display compositor and therefore affect both native apps and Makepad content. No device setting was changed for the audit or child unit/API checks.

## Verified native behavior

| Control | Pinned native implementation | Authoritative setting and behavior |
| --- | --- | --- |
| Color inversion | `ColorInversionPreferenceController`, `ToggleColorInversionPreferenceFragment` | Secure `accessibility_display_inversion_enabled`, normal values 0/1, native missing default Off. The native controller reports available. |
| Use color correction | `ToggleDaltonizerPreferenceFragment`, `DaltonizerPreferenceUtil` | Secure `accessibility_display_daltonizer_enabled`, normal values 0/1, missing default Off. Changing the enable bit does not replace the selected correction mode. |
| Correction mode | `DaltonizerRadioButtonPreferenceController`, `res/values/arrays.xml` (`daltonizer_mode_keys`, `daltonizer_type_values`) | Secure `accessibility_display_daltonizer`; native choices are 12 Deuteranomaly (red-green, green weak), 11 Protanomaly (red-green, red weak), 13 Tritanomaly (blue-yellow), and 0 Grayscale. Native missing/malformed mode presentation defaults to 12. A parsed unknown integer remains an unknown/custom mode rather than one of these radio selections. Mode selection is permitted while correction is off and does not turn it on. |

`ColorDisplayService` observes these secure rows for its current Android user. Inversion calls `DisplayTransformManager.setColorMatrix` at the inversion level. Grayscale installs a grayscale matrix and disables native daltonization; the three color correction modes use the native daltonizer. The service reapplies these transforms during setup and adjusts the color-mode composition path. It does not depend on Night Light availability. Changes can be asynchronous after the provider write, so the UI must distinguish observed saved configuration from separately verified display effect.

There is an important nonstandard-value distinction: native Settings renders enable bits using `== 1`, but ColorDisplayService uses `!= 0`. The adapter must not advertise a fabricated On/Off observation for a custom integer such as 2. Unknown/malformed raw values should be unavailable/read-only with native recovery in this first slice; merely reading must never normalize them. Verified absent-row defaults may be displayed without writing rows. Preserve each unrelated enable/mode row on every operation.

`DaltonizerSaturationSeekbarPreferenceController` is a separate, flag-gated feature. It offers levels 1–10 with UI default 7 only when `com.android.server.accessibility.Flags.enableColorCorrectionSaturation()` is true and correction is enabled in a non-disabled, non-grayscale mode. ColorDisplayService also gates the strength observer on that flag and uses its own `NOT_SET` fallback for an absent row. Strength must remain explicitly pending until the installed framework flag/API and effective default path are verified; it must not be advertised solely because a similarly named secure key exists.

## Concrete finite interface and UI

Add `ControlsPage::AccessibilityVision` / wire `accessibility_vision`, with exactly these controls:

| Wire ID | Allowed values | Visible label |
| --- | --- | --- |
| `color_inversion` | `off`, `on` | Color inversion |
| `color_correction` | `off`, `on` | Use color correction |
| `color_correction_mode` | `deuteranomaly`, `protanomaly`, `tritanomaly`, `grayscale` | Color correction mode |

Reuse `controls_snapshot`, `control_set`, `launcher.controls_state`, finite page/control/value validation, foreground polling, typed Settings capability checks, and current root/press/accessibility-target correlation. Four explicit radio choices fit the existing stable four-choice row slots used by vibration intensity; generalize their finite presentation type rather than abusing intensity semantics. Mark the selected observed mode, retain scroll and widget identities across reads/theme changes, and keep 150% text and scrollbar gutters.

Use the existing Agent's granted `WRITE_SECURE_SETTINGS` through the compiled Controls allowlist. No new Binder methods, raw setting-name interface, Home privilege, Broker system UID, accessibility service permission, or external credential flow is required for these three rows. Existing exact caller/signature/current-owner/keyguard/unlock gates remain in force. Recheck authority and the finite current state immediately before a write, verify the specific written row afterwards, and return requested/unconfirmed rather than claiming success if readback differs. Do not invent a blanket administrator restriction absent from pinned native policy; framework write enforcement remains authoritative.

The Settings overview should expose the built-in vision page with a clear partial scope and a separate native accessibility recovery action for unmigrated features. Search aliases can include color inversion, color correction, grayscale and Chinese equivalents. Proposed custom route: `accessibility_vision`. Do not redirect the generic `android.settings.ACCESSIBILITY_SETTINGS` to a page that currently implements only vision colors.

Pinned dedicated native actions are `android.settings.COLOR_INVERSION_SETTINGS` and, unusually, `com.android.settings.ACCESSIBILITY_COLOR_SPACE_SETTINGS` (the value of hidden `ACTION_COLOR_CORRECTION_SETTINGS`). If advertising these entries in this increment, use independent priority-2 filters on the real renderer Activity, finite navigation-only mapping, and trusted pinned-native recovery. Never append new actions into an older multi-action filter: that loses existing priority on APK updates over an older ROM base. The dedicated shortcuts/settings-service enable flows themselves remain pending.

## Ownership and integration plan

The child can own a new tested finite color-control helper/contract, changes to `settings_controls.rs` and the Controls row rendering, and its dedicated audit/tests. Coordinate the overview/search/entry mappings, splash, shared contract/backend and Agent platform hook with the parent before editing; the parent is concurrently implementing per-app language and owns shared Home/Agent integration. The existing service/staging structure needs no new privileged component. Parent owns native build capture and privileged emulator acceptance; child owns ordinary emulator-5556 checks only.

## Acceptance before any completion claim

1. Unit/backend tests: finite page/control/value combinations; verified missing defaults without writes; malformed and unknown raw values; independent enable/mode preservation; read-only ordinary capabilities; current-user/locked/caller denial; changed capability between observation and mutation; failed or delayed readback. Include provider-read failure so no default is invented for an exception.
2. UI/model tests: all four choices and selection; correction Off permits selecting the next saved mode without enabling it; disabled choice cannot dispatch; polling does not remount controls; press/a11y node retirement cannot activate a replaced choice; large text/narrow layout and nested Back.
3. On the parent's owned privileged emulator, snapshot exact raw values including absent versus empty, baseline ColorDisplay/SurfaceFlinger state, existing Night Light/color settings, and all existing package policy/permission records. Exercise On/Off and all four modes from OctoSense; independently read the provider and display service. Verify a real inversion/grayscale compositor matrix change and, where observable, the native daltonizer mode. Do not equate a saved integer alone with demonstrated visual transformation.
4. Verify mode changes while Off leave the enable bit Off, external changes appear on foreground polling, process restart preserves configuration, and denied authority causes no mutation. Restore exact original rows and display state in `finally`; remove any disposable probes. No phone or real user data.
5. Ordinary emulator-5556 must show genuine observed read-only/unavailable state, reject writes, and return from pinned native recovery to the same page. Re-run accessibility/IME smoke and cold navigation after entry changes. If dedicated implicit routes are added, parent separately verifies ordinary/native resolution and privileged ROM/data-update priority survival.

## Remaining accessibility scope

This slice does not complete screen-reader service discovery/consent, accessibility shortcuts, magnification, captions/hearing devices, audio balance/mono audio, switch access, accessibility timeout, touch-and-hold delay, autoclick, system navigation, high-contrast text, reduce-bright-colors, or correction strength. Service enablement, restricted-setting consent, and third-party service configuration must remain platform/provider mediated until a reviewed integration exists.

Remove animations requires separate work. Pinned `DisableAnimationsPreferenceController` reads all three global scales and writes 0 or 1 to each, with possible partial failure; it does not preserve custom scales when turning animation back on. Home currently samples only animator duration into `reduce_motion`, and only some shell motion consumes that flag. Makepad widget animations have not been proven to honor Android's global scales. Similarly, high-contrast text and motor-input preferences cannot be claimed to affect custom canvas controls merely because a provider row changed. These are explicit integration and behavioral acceptance gaps, not completed native-link rows.

## Implementation checkpoint

The finite model/contract and shared ColorAccessibility backend are saved. The backend never normalizes reads, maps only the four native correction choices, changes only the selected row, and rechecks authority after reading immediately before writing. Accepted writes with delayed or unavailable readback return `control_requested`; matching saved values return `control_applied`, without claiming compositor verification. The Agent hook rechecks current owner, unlock/keyguard and its existing secure-settings permission. No Binder transaction or permission was added.

The actual pure backend regression and existing Java contract suite passed 10 tests; all shared Controls sources compiled against API33 and API35. Tests cover exact mode mappings, mode selection while correction is Off, preservation of unrelated rows, unknown/malformed values, read-only/restricted authority, permission/user changes between read and write, provider failures, and accepted writes without confirmation. Shared UI integration passed the full 476-test Rust suite, including three vision UI regressions. Actual native compositor acceptance is recorded below.

## Emulator acceptance

Parent-owned emulator-5560 accepted Home2026092529 in 106 checks: all four correction modes produced distinct actual SurfaceFlinger transforms; inversion and combined inversion/correction worked; changing the saved mode while correction was Off left the compositor unchanged; Home restart preserved state; unknown raw mode stayed unavailable; external edits refreshed correctly. Exact three secure rows and the original compositor matrix were restored. This demonstrates the emulator compositor path, not physical-display color accuracy or a production ROM boot.

Ordinary emulator-5556 passed 18 read-only/recovery checks, eight cold launches (33 checks), and accessibility/IME smoke (33 checks) on2529, with no crashes. Receipts are under `out/home/settings-vision-recovery/validation/ordinary2529-5556.json`. The initial2528 test correctly exposed native Accessibility returning to an older Sound task;2529 scopes that recovery to a fresh pinned-native task, and Back now returns to the preserved built-in colors page. No color setting changed on the ordinary installation. Native saturation/strength, generic accessibility entry routing, and the other accessibility gaps above remain pending.
