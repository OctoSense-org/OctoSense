# Per-app language: native audit

The built-in language picker and finite native adapter are implemented in source; privileged and ordinary emulator acceptance is complete for this bounded slice. This is not whole Settings parity. The pinned
Settings checkout is `0f0669fc699f70adb20fe6ed4b2ff1c600da6f86`; framework sources
are `ff7620a38e54c5f7ec14a5b8ccc5be1ba41e2b1b`. No phone is part of validation.

Native entry uses `AppLocalePreferenceController`, `AppLocaleUtil` and
`AppLocalePickerActivity`. Eligibility requires a launcher entry, an app without
the platform signature, and absence from the trusted native Settings resource
`config_disallowed_app_localeChange_packages`. `LocaleConfig` must declare a
nonempty supported list when `SETTINGS_APP_LOCALE_OPT_IN_ENABLED` is enabled.
Without that flag, native Settings can also expose an app with non-system
asset locales. An explicitly empty declared configuration does not fall back
to assets. Malformed configuration remains distinct from missing configuration.

`new LocaleConfig(packageContext)` first consults the app's runtime override;
reading only the manifest XML would miss this supported mechanism. The framework
`AppLocaleStore` also filters configured locales against actual assets for some
system apps. `AppLocaleCollector` combines supported app languages with native
system locales and region choices, current app/system languages and suggestions.
Its locale tree should be reused or matched explicitly; a free-form language-tag
setter is not equivalent to the supported native picker.

Native selection calls `LocaleManager.setApplicationLocales(packageName,
LocaleList.forLanguageTags(tag))`. An empty list follows the system. This overload
marks the change as coming from a delegate, which preserves framework behavior
for later app locale-configuration changes. Observe the complete current list;
do not silently truncate a preexisting multi-locale/custom choice merely by
opening the page. Language changes can recreate the target app's Activity.

The Octoscript view belongs under Apps → App details → App language.
Use current-owner/unlocked checks, a bounded native-derived catalog with local
search/paging, exact package incarnation and one-use observed choices, then
native service readback. Keep system language editing separate. Preserve app
filter, nested Back, theme and accessibility identities; retiring a language
row must not retarget a held press. Platform-signed essential components remain
outside this app picker, following native eligibility.

Acceptance should use a disposable APK declaring several translations and a
fixture that observes its own effective application locale and rendered strings.
Cover system default, language/region selection, actual target restart, native
external edits, override-config changes, stale reinstall denial and unavailable
ordinary installation. Never change a real app's language during automated
acceptance. Native Settings recommendation notifications and usage telemetry are
separate from the authoritative LocaleManager operation and must not be claimed
as reproduced without implementation.


## Implemented boundary and pending acceptance

`AppLanguagePlatform` runs in the existing system-UID Settings Broker and uses
`AppLocaleCollector`, `LocaleHelper.LocaleInfoComparator` and `LocaleInfo` from
the installed framework. Language, region and numbering-system branches follow
`LocalePickerWithRegion`, including its direct selection of a sole child. Native
labels use public helpers; the framework system-default title is resolved by
resource name rather than an inlined ID from another platform build. Stable
semantic tie-breaking removes HashSet iteration order from the state revision.

Agent transactions 75/76 expose only a bounded snapshot and selection of an
opaque observed choice. The Home and Agent processes gain no locale-setting
permission. The Broker requires the exact platform-signed Agent caller and the
unlocked foreground owner. The current UID, signing identity, installation
version/timestamps and source inode bind each package incarnation. The revision
also includes eligibility, complete current locale list, effective LocaleConfig,
asset locale list, native hierarchy and writable state. A selection re-resolves
these before the native setter and checks the resulting current list. No API
accepts a caller-provided language tag. As with native package-name setters,
package replacement and the framework's eventual application of the request
are not a single atomic transaction; post-call incarnation/readback failure is
reported as unconfirmed, never retried.

Catalogues are bounded to 1,024 nodes and three levels, with 20 rows per page,
128-character filters, a ten-minute catalogue lifetime and twenty-second
observed-choice authority. Fresh explicit root reads establish a catalogue;
retained branches fail stale rather than adopting a replacement. A choice
consumes its catalogue before the write, including failed or uncertain writes.
Current lists up to 128 entries are retained in order; excess data fails
unavailable rather than silently truncating it. A missing config may use the
native asset fallback only when the native opt-in flag allows it. An invalid
config is reported separately and never treated as missing.

The UI preserves the filter widget across polling/theme changes and restores
branch query/page/scroll on Back. Row presses bind the actual opaque choice;
replacing a row between press and release cannot redirect selection. Selection
keeps its observed read identity until host authorization, then requests a fresh
root/readback. Enqueueing never invents the new locale. Native recovery is the
finite, trusted Android per-app language Activity in a fresh task; it is not
reported as built-in completion. Public standard-entry replacement and native
recommendation notifications/telemetry remain outside this increment.

Local shared-backend Java regression passes hierarchy/paging, complete custom
current lists, stale incarnation/configuration/eligibility, lock, lifetime,
one-use authority and applied/requested/unconfirmed outcomes. The shared Java
sources compile against API33. Two strict Rust model tests passed in a standalone harness; the combined Home
suite now passes all 482 tests, including hierarchy Back, stable input/theme,
held-row replacement and catalogue-reset regressions. Parent verified every requested
native framework descriptor on the actual AOSP35 clone and preflighted an
independent translated fixture (French, both Chinese scripts, Arabic, actual
Activity recreation, runtime LocaleConfig override and restoration). Ordinary Home2530 on emulator5556 passed unavailable/native-picker recovery
27 checks, eight cold entries/33 and accessibility smoke/33. All 1,574 existing
permission records across 243 packages were preserved, with no crashes; Home
certificate, UID, data inode and first-install time were preserved on update.
Receipts are under `out/home/settings-app-language/validation`. Native Agent and
Broker compile and caller-boundary checks pass; Home2530 native selection QA on emulator5560 passed 141 checks: French and
simplified/traditional Chinese rendered resources, Activity recreation, System
default, complete custom current list, runtime-configuration and reinstall
stale denial, protected Home, and exact native fallback/Back. All 906 existing
permission records were preserved. Arabic translation was fixture preflight
only; a numbering-system branch is implemented but not device-proven. A cosmetic
System default secondary label was corrected afterward; the updated Broker
passed 14 installed inspection checks without the “Unknown language” subtitle.
No phone or real app locale was changed.
