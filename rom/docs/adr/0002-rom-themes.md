# ADR 0002: Shared phone themes

Date: 2026-09-24. Status: Accepted; picker and host propagation implemented.

## Decision

Offer four bundled presets, OctoSense, Minimal, Paper and Vivid, through a native
**Wallpaper & style** activity in the Home APK. Home's long-press menu opens it.
The ROM also exposes it under **Settings → Display** using SettingsLib's system
activity discovery. Standalone Home installations use the long-press entry.

The first version offers light, dark or system appearance and a theme gradient
or solid background. The picker previews synthetic Home and Mail screens. Draft
edits, including Restore default, have no effect until Apply. Cancel and Back
discard the draft. No private application data is used for previews.

One versioned JSON catalog supplies both the Android preview and Rust renderer.
Presets contain semantic colors and corner radii, not executable scripts or new
navigation layouts. The catalog lives in `home/resources/themes/mobile-presets.json`.
Android stores the validated selection as one atomic SharedPreferences record
in `octosense-appearance` (`selection-v1`). It is per Android user and survives
an in-place APK update or data-preserving OTA. Clearing Home's data or a fresh
ROM install resets it. Unsupported records fall back to the bundled default.

Home receives the choice with the existing `launcher.ui_mode` message on resume
and configuration changes. It resolves System against Android's night mode and
reapplies the Android widget stylesheet to existing hosted app instances. It
invalidates captured tiles and the cached backdrop when the palette changes,
including changes that keep the same light/dark mode. Font settings, navigation,
app instances and layout remain under their existing owners.

Both activities handle `uiMode` and `CONFIG_ASSETS_PATHS`: applying Monet changes
the resource-overlay paths as well as colors, which otherwise makes Android
recreate Home and discard its in-memory app state. Integer resources referenced
by the manifest express these flags because Makepad's API 33 compiler does not recognize the
newer public `assetsPaths` spelling.

On the ROM, the platform-signed Home uses the existing AgentPlatform settings
capability to merge its fixed Monet seed into
`theme_customization_overlay_packages`, preserving unrelated overlay choices.
It uses UiModeManager for explicit light/dark selections and WallpaperManager
for the matching generated background. System appearance follows the current
Android setting; it does not change Android's existing schedule. Native Android
apps receive the system palette only if they support Android dynamic colors.
Apps with fixed custom colors are not forcibly recolored.

Android side effects run on a worker. A failed system operation does not discard
the saved OctoSense choice; the picker reports partial application. Ordinary APK
installs retain OctoSense theming even without ROM privileges. The background
shader is a fixed gradient with no image download, blur pass or animation.

## Boundaries

All OctoSense apps adopt the [shared app theme contract](../../home/docs/android/app-theme-contract.md).
Existing custom app surfaces require migration where they override the shared
base theme; the contract records the audit and distinguishes these gaps from
the host's completed propagation support.

This version does not include downloadable packs, arbitrary fonts, personal
photo wallpapers, icon replacements or cross-family desktop layouts. Android
system wallpaper is regenerated on Apply; changing Android's night mode later
updates Home and hosted apps, but does not regenerate that bitmap in the
background. A future wallpaper service can handle that independently.

## Verification

The theme tests validate records, normal-text contrast, stylesheet roles and
light/dark switching for every preset. A hosted Sheets test checks that all eight
palettes evaluate without errors and retain the same widget and isolate.
Device validation is recorded in [the theme guide](../../home/docs/android/themes.md).

## References

- [Android dynamic colors](https://source.android.com/docs/core/display/dynamic-color)
- [SettingsLib system activity discovery](https://android.googlesource.com/platform/frameworks/base/+/3e62009709d9e329b7a309b78cce43943f7c2da6/packages/SettingsLib/src/com/android/settingslib/drawer/TileUtils.java)
- [UiModeManager](https://developer.android.com/reference/android/app/UiModeManager)
