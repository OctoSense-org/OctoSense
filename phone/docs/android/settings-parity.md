# OctoSense Settings replacement checklist

Updated: 2026-09-25. Target: the complete system Settings experience on the
OctoSense OnePlus 6 ROM (LineageOS 22.2 / Android 15), implemented using the shared
Octoscript–Makepad presentation and trusted typed service boundary in
[ADR 0006](../adr/0006-builtin-settings.md).

## Audit baseline and status rules

The actual ROM build checkout was inspected read-only at
`packages/apps/Settings`, clean commit
`0f0669fc699f70adb20fe6ed4b2ff1c600da6f86`. The inventory comes from
`res/xml/top_level_settings.xml`, `system_dashboard_fragment.xml`, the associated
feature XML files/controllers, and `packages/apps/LineageParts/res/xml`.
Controller visibility, overlays and hardware can hide individual features;
inventory presence is not proof that a particular phone supports them.

**Implemented** means an OctoSense screen reads real state and performs the
listed supported operations. **Native link** means the current screen delegates
that area to Android Settings; it does not count as replacement UI. **Missing**
means no dedicated OctoSense implementation or verified direct route. A broad
Android Settings recovery entry does not turn every missing row into a native
link. The second implementation increment has emulator validation; hardware and
full parity gates remain open.

## Navigation target

Use a searchable Settings overview with these stable sections. Keep related
advanced controls inside their section and show supported controls based on
capabilities, rather than duplicating Android's current top-level arrangement.

| Section | Contents |
| --- | --- |
| Connections | Internet/Wi-Fi, SIM/mobile, hotspot/tethering, VPN, private DNS, data usage |
| Connected devices | Bluetooth, NFC/payments, USB, casting, printing and accessories |
| Appearance | OctoSense themes, wallpaper, system colors and light/dark schedules |
| Display | Brightness, timeout, font/display size, rotation, lock screen and LiveDisplay |
| Sound and vibration | Stream volumes, ringtone/sounds, haptics and charging sounds |
| Notifications and Modes | Notification policy/history/channels, lock screen notifications, DND rules |
| Apps | Installed apps, defaults, permissions, special access and storage/data policy |
| Battery | Status, usage, saver, charging controls and app battery policy |
| Storage | Capacity/categories, per-app storage, removable media and cleanup |
| Security and privacy | Lock/biometrics, credentials, permission/privacy controls, Trust and work/private profiles |
| Location and safety | Location/scanning, emergency information and alerts |
| Accounts | Accounts, sync, passwords/passkeys/autofill and provider sign-in |
| Accessibility | Screen reader, text/vision, hearing, input/motor and shortcuts |
| System | Languages/input, gestures/buttons, date/time, users, updates, backup/reset, developer options |
| About phone | Model/build, Android/security patch, kernel, hardware/status, legal and diagnostics |

Octos/provider configuration can become a separate OctoSense section; it is not
a substitute for any Android/Lineage parity row. Preserve existing removal of
the unrelated App Hub store app from Home's app catalog; system app management is a
different Settings feature and remains a target here.

## Feature parity ledger

All rows require real service readback, permission/policy checks, loading/error
states and persistence after process restart. The acceptance column adds checks
specific to each feature. “Native link” is the current migration status, not the
desired final behavior.

| Feature | Current status | Backend / remaining work | Acceptance checks |
| --- | --- | --- | --- |
| Search and navigation within migrated Settings pages | Implemented navigation; cold and warm input validated2436 | Compiled typed destination index; English/Chinese aliases; pending pages are not indexed as implemented | Stable input/caret, no query transmission, results pagination/scroll, exact route and nested Back, live theme/150% fonts, touch row scrolling |
| Android per-app notification Settings entry | Implemented; ordinary and privileged emulator validated2513 | Separate standard intent filter; exact package selector followed by fresh current-user details lookup; entry/root correlation and normal notification authority | 2513:33 entry checks on5556/5558, ordinary recovery24; cold/latest-only delivery; forged UID/channel/value extras ignored; missing/removed apps; no mutations from navigation; Back/filter preservation and no native fallback loop |
| Theme preset, light/dark/system, solid/gradient wallpaper | Implemented | Existing catalog, stored Choice, ThemeApplier; Android palette may be partial | Cancel preserves saved choice; Apply survives restart; native picker and app agree; live update preserves controls/drafts |
| Custom wallpaper, lock wallpaper, color extraction, schedules | Missing | WallpaperManager, theme overlays, UiModeManager schedules | Preview/cancel, permissions and scheduled changes; confirm supported wallpaper targets |
| Brightness, automatic brightness, rotation lock | Implemented | Bridge finite controls; permission-dependent | Disabled until authorized; observed values change; policy denial; brightness remains usable |
| Timeout, font scale, touch haptics | Implemented; emulator validated | DeviceSettingsBackend typed allowlist; WRITE_SETTINGS or ROM helper | Accepted finite values persist; font change preserves active page; unsupported state is explicit |
| Display size | Implemented; owner-service emulator validated2508 | WMS initial/base density; bounded SettingsLib choices; Default clears override | Current/custom observations; Save/Cancel, stale/policy/owner denial, actual density readback; same editor/process/IME through changes verified on both emulators |
| Night Light | Implemented; supported and unsupported emulator validated2508 | ColorDisplayManager capability/ranges/custom times; helper-only permission | Unsupported/ordinary gating; Save/Cancel/readback; seconds preserved; sunset Location gate; actual service/compositor confirmed with temporary resource overlay; physical hardware tint remains pending |
| Lineage LiveDisplay, calibration, refresh/resolution, lock screen | Native link: Display | Lineage services, hardware overlays, lock-screen settings | Hardware capability gating; readable low-brightness recovery; rotation/configuration and display-driver stress |
| Media volume | Implemented; owner-service emulator validated2435 | Typed DeviceSetting/AudioManager path shared with the other streams; independent of the optional System Bridge | UI and external stream readback5/15→9/15; policy denial and restore; missing/revoked state disabled |
| Alarm/ring/notification volume and touch sounds | Implemented; emulator validated | AudioManager plus typed SettingsProvider access | Stream independence, quantized readback, ringer/DND policy denials |
| Default ringtone/notification/alarm sound selection | Implemented; local and owner-service emulator validated2432–2433 | RingtoneManager, opaque observed targets, WRITE_SETTINGS or owner ROM helper; Preview/Stop and explicit Save/Cancel | Real observed title/Silent/unavailable; cancellation/readback, URI change/policy/user denial, preview stops on page exit/background, no DND/volume changes |
| Sound feedback and haptics | Implemented; owner-service emulator validated2510 | Finite Controls API; actual vibrator defaults, native capability resources and ringer/policy gates; legacy ring/touch compatibility writes | Offered device levels only, preserve custom values; no preview; exact readback and partial-write reporting;191 actual checks incl Vibrator service, legacy couplings and restored preferences; keyboard unsupported on this image, physical haptics and higher-level hardware remain pending |
| Custom sound import and other audio policy | Native link: Sound | Platform audio picker/provider and device audio policy | Custom-file cancellation/access lifetime; audio route changes; supported-device behavior |
| Manual Do Not Disturb modes | Implemented; standalone and ROM-service emulator validated | Finite system-UID broker: Off, Priority only, Alarms only, Total silence; observed state, policy and foreground checks | Four exact modes/readback; ordinary installation read-only; denied authority; restore initial mode |
| DND default interruption policy | Implemented; emulator accepted 2524 | Existing system-UID Zen broker; copied policy, finite choices and one-use observed authority | 837 actual checks cover all nine fields and synthetic notification ranking; complete captured policy/rule and listener state restored. Native legacy visual-bit normalization is separately recorded; contact affinity, repeat-call history and conversations remain separate behavior checks |
| Android DND time schedules | Implemented; emulator accepted 2524 | Native scheduler, copied rule records and verified Android provider aliases | 210 checks cover create/edit/Cancel, enable/disable, empty days, reviewed deletion and stale drafts; actual start/end with Home stopped; exact captured policy/rule and listener restoration. Earlier native short/full provider mismatch is fixed; alarm-exit, clock/time-zone changes and hardware remain separate checks |
| DND direct entry and recovery | Implemented; ordinary, service-enabled and updated-system emulator checks pass | Finite custom dnd route, ZEN_MODE_SETTINGS, preferred-route capability metadata and separate native recovery task | 77 ordinary unavailable/recovery checks; 15 dedicated entry checks; 64 ROM-routing checks on both old/new system bases with active Home 2524 data update, covering all 22 standard actions and the SystemUI gear; native fallback restored after cleanup |
| DND provider/calendar rules and per-rule policy/effects | Native recovery / pending | Observed provider configuration and scoped policy | Provider identity/consent, calendar data, effects, alarm-exit/sender behavior and individual rule policy |
| Notification history viewer and enable/disable review | Implemented; owner-service emulator validated2431 | ACCESS_NOTIFICATIONS with AppOp; owner-only20-row paging and expiring generation-bound snapshots; explicit disable confirmation | Actual24records/20+4 pages/Chinese+emoji; new-arrival stable paging; Refresh, off/on invalidation, revoke/expiry clearing; Cancel preserves and Confirm clears history |
| Lock screen visibility and sensitive content | Implemented; owner-service emulator behavior validated | Finite observed SettingsControls backend and authorized choices | Secure lock screen: public/private visible with both On; sensitive Off hides private only; visibility Off hides both; dependency denial; original preferences and lock state restored |
| Notification bubbles | Capability-gated Controls; real bubble behavior pending | Finite observed SettingsControls backend; AOSP config_supportsBubble=false is unavailable | Actual supported-device bubble rendering, persistence and policy; no claim from unsupported emulator |
| Per-app notifications, channel/group enable and importance | Implemented; owner-service emulator validated (2512) | Finite owner-only SettingsBroker notification service; observed package/channel targets; copied native channels | 281 actual checks: app permission and fixed-state denial; 20-row paging; real app/channel/group delivery; reviewed importance/default sound with preserved fields/user locks; mixed ungrouped channels; legacy app/default-channel coupling; stale review rejection. Ordinary install unavailable/native recovery passed24; full ROM/hardware remain |
| Channel sound/vibration/badge, conversations, DND bypass and Lineage LED | Native link: per-app Android notification settings | NotificationManager, app permissions, Settings/Lineage LED support | Individual field persistence; protected channels; current user; real effects |
| Wi-Fi enabled/connection/scan/saved observations, authorized scan/toggle/saved connect/forget | Implemented; owner-service toggle/connect and native credential handoff emulator validated | Bounded WifiManager/Connectivity observations; finite ROM-helper mutations; fresh selected-network fingerprints | Actual helper Off/On and saved connection with DHCP; exact native network configuration returns to details. Bounded scans, permission/redaction/stale-target tests; remaining enterprise authentication, real access points, full policy matrix and physical hardware remain separate gates |
| Wi-Fi credentials, enterprise EAP/certificates, share and advanced network policy | Native link: Wi-Fi | Platform credential/certificate components and remaining native presentation migration | WPA2/3 and EAP success/failure; cert/domain validation; captive portal; no credential logging; no passwords or certificate aliases in scripts |
| Mobile/SIM/eSIM, APN, roaming, call/SMS preference, Wi-Fi calling | Native link: Mobile | Subscription/Telephony/carrier APIs and carrier-privileged flows | No-SIM, dual-SIM, airplane mode, carrier restrictions; real device connectivity |
| Hotspot, USB/Bluetooth tethering | Native link: Hotspot | TetheringManager/WifiManager and entitlement flow | Client obtains traffic; stop/restart; policy/entitlement and concurrent radio limitations |
| VPN | Native link: VPN | VpnManager and provider consent; always-on/lockdown policy | Consent, connect/disconnect, reboot and blocked-network behavior |
| Airplane mode, global Data Saver, Private DNS | Implemented; owner-service emulator validated2423 | Finite helper APIs; exact raw-state observation key; IDNA validation; current-user/admin policy; connection/DNS readback | Actual Airplane/Data Saver On/Off; DNS hostname/Off/Automatic readback, dns.google validated on default Mobile transport; invalid URL rejection and external-state stale Save denial. Original rows restored; physical radio/network and wider policy acceptance remain |
| Per-app background data and Data Saver exceptions | Implemented; emulator accepted 2525 | Finite system-UID Broker; observed UID membership, native NetworkPolicyManager bit operations and coupled native Settings semantics | 68 checks: real UID policy and metered restriction readback, independent changes, Home restart, protected host, nested Back; original UID policies and global Data Saver preserved. Traffic throughput and hardware remain separate |
| Per-app Lineage network/Wi-Fi/mobile/VPN restrictions | Implemented; unsupported AOSP emulator gating accepted 2525 | Exact pinned Lineage policy constants required at runtime; shared-UID membership disclosed and bound to reviewed authority | All four unavailable controls disabled on AOSP; actual Lineage service and packet behavior remain unverified |
| Proxy and data usage/limits | Missing | ConnectivityManager, NetworkPolicy/Stats services, per-network config | Billing cycle, traffic accounting and proxy configuration |
| Bluetooth adapter/name, paired/nearby devices, authorized pair/cancel/connect/disconnect/forget and sharing | Implemented; adapter/name/discovery owner-service emulator validated2423–2427; peripheral actions unverified | Bounded Bluetooth observations; finite helper profile/sharing APIs; platform-owned pairing confirmation | Actual adapter On/Off, name Save/Cancel/restoration, owned discovery start/stop; bounded stable pages and observed state. Pairing, profile connections, sharing and forget still require real peripherals and policy checks; no emulator claim of successful peripheral operation |
| Bluetooth profile-specific controls, discoverability, codecs and advanced peripheral features | Native link: Bluetooth | Profile services, device-specific capability and platform confirmation | Audio/HID reconnect, codec readback, unsupported LE features hidden; permission changes |
| NFC/payments, USB modes/default, casting, printing, hearing devices | Missing | NFC/payment roles, USB manager, MediaRouter, print service, capability-specific profiles | Actual peripheral/service scenarios and per-user defaults; secure accessory consent |
| Installed apps list/filter/system toggle and app details | Implemented; emulator validated | Bounded current-user PackageManager catalog, version/state/dates, observed target identity | Stable typing/caret on polls/themes; 20-row pagination, generation changes; detail Back restores scroll; no row retargeting during a press; button-start swipes transfer to scrolling without clicking |
| Requested permission state and per-app storage statistics | Implemented: observations; common runtime editing tracked separately | PackageManager grant/protection data and StorageStatsManager, optional usage access; shared UID disclosure | Granted/denied/unavailable remain distinct; 20-permission pages; denied stats never become zero; data includes cache; hosted modules share Home data |
| Launch and user-confirmed uninstall | Implemented: launch and Android confirmation | Fresh allowlisted action capabilities; Android launch and uninstall confirmation | Missing/disabled launch target; cancel confirmation; protect Home/Bridge/helper/system/admin packages; removal readback; no action from stale/forged target |
| Common runtime permission choices | Implemented; emulator accepted2521 | Narrow PermissionController adapter reuses native groups/button state/requestChange/warnings; finite observed one-use choices; no Home/Agent grant privilege |451 Rust/63 repository checks; ordinary2521 unavailable/recovery25, cold-entry29 and accessibility/IME33 without navigation retry; native compilation and exact-original controller packaging verified. Privileged521 checks cover all11 common groups, actual grants/AppOps and native warnings; three consecutive legacy104-check repetitions pass with actual rotation/window completion. All906 unrelated grant/flag records preserved; specialized media/Health Connect/virtual scopes, hardware and production-ROM acceptance remain separate |
| Specialized permission controls: selected photos/media, Health Connect, virtual devices, special access, unused-app revocation | Native recovery / missing built-in controls | PermissionController specialized models and platform-owned consent | No common-group grant shortcut; category-specific consent/revocation, policy and current-user checks |
| Default-app role picker | Implemented; native consent emulator validated2515 | Octoscript overview/candidates; narrow PermissionController adapter reuses native qualification, restrictions, confirmation and ViewModel; eight finite roles, owner-only | 83 actual checks: browser Cancel/Back/confirmed choices, stale reinstall denial, Assistant/None, native recovery and complete40-role restoration;22 caller-boundary probes per signer. Native qualification/restriction gates, stable paging/theme/accessibility; other role-specific behavior and full-ROM integration remain unverified. Standard Defaults entry2516 passes11 targeted entry and53 ordinary generic entry checks; the separate priority3 filter passes old/new-base caps and data-update survival (60 ROM checks,22 standard actions) |
| Per-app language | Implemented; emulator accepted2530 | Native AppLocaleCollector hierarchy and LocaleManager; opaque observed choices, current-user and incarnation gates |141 checks: real French and both Chinese translations/Activity recreation, default and custom list, runtime config and reinstall denial, nested Back and recovery. Ordinary27 checks, cold33 and smoke33 on both emulators;482 host tests. [Language audit](settings-app-language-audit.md); numbering/Arabic picker and hardware remain explicit separate checks |
| Specialized permissions and special app access | Native link: Apps/app info | Permission services; AppOps and per-app policy | Specialized access restrictions; removed/disabled apps |
| Battery status | Implemented; emulator validated | BatteryManager sticky observations | Real level/status/power source; missing data unavailable; charging transitions |
| Battery saver, automatic threshold, disable at 90%, adaptive battery | Implemented; saver/threshold/adaptive owner-user service emulator validated; 90% transition hardware acceptance pending | PowerManager and finite secure/global settings with ROM helper | Charging transition, readback, custom threshold observations and policy denial |
| Per-app battery optimization | Implemented; emulator accepted 2525 with corrected Broker | Finite system-UID Broker policy API; current package incarnation, stored AppOps and device-idle allowlist; protected/shared UIDs read-only | 111 checks: Restricted review/Cancel, all three policies, pre-O coupling, custom policy repair, stale review, restart persistence, protected/shared-UID denial. Original allowlist and both AppOps inventories preserved. See [battery audit](settings-app-battery-audit.md); real Doze/job delivery, hardware and Settings-local telemetry remain separate |
| Battery usage and Lineage charging control | Native link: Battery | BatteryStats/PowerManager, Lineage charging HAL/service | Real discharge/charge session; hardware charging limits |
| Storage capacity | Implemented; emulator validated | StatFs on user data volume | Byte totals match volume; low/free changes reflected; absent volumes handled |
| Per-app clear cache/data and declared manage-space | Implemented; emulator accepted2528 | Reviewed one-use native package operations, fresh incarnation/policy/stats eligibility, async completion |129 checks: cache/data and framework resets, Cancel, deliberate cache growth, stale reinstall denial, manage-space Back and protected Home. All906 existing grant/flags preserved; ordinary unavailable/recovery24. See [storage audit](settings-app-storage-audit.md); hardware/full-ROM and racing framework-queue identity limitations remain |
| Storage categories, removable media/format, cleanup | Native link: Storage | StorageManager/Stats, document access and volume operations | Scoped/user storage; media insert/remove; explicit destructive confirmation and cancellable cleanup |
| Date/time observations, 12/24h, automatic time/zone | Implemented; owner-user platform-service emulator validated | Typed System/Global access and TimeManager capabilities; automatic controls require ROM privilege | Actual readback, permission/policy restrictions and external changes; cold-start lifecycle and rapid selected replacement/Clear accepted in2430–2431 |
| Manual date/time and time zone | Implemented; owner-user platform-service emulator validated (2428) | TimeManager/TimeDetector capability checks; strict civil time and first/second repeated-hour choice; bounded zone catalog with reviewed targets | Valid clock changes actual epoch; invalid date disables Save; DST gap rejected; zone Apply/persistence and Cancel; policy retires an open draft. Selected-text editing passed after the causal IME fix; real repeated-hour clock changes and physical-device acceptance remain |
| Regional date/time formats | Native link: System | ICU/locale configuration | Locale-specific date/week conventions and system-wide readback; native destination alone does not satisfy parity |
| About model/build/Android/patch/kernel/memory | Implemented; emulator validated | Build, ActivityManager, kernel metadata | Real immutable fields; unavailable fields explicit; values match phone |
| SIM/network status, legal/licenses, hardware status and diagnostics | Native link: About/System | Telephony/system properties, legal resources and diagnostics services | Sensitive identifiers redacted/permission-gated; supported diagnostics and actual build legal resources |
| Screen lock, credential confirmation, biometrics | Native link: Security | LockSettings/Gatekeeper/Biometric services; platform-owned secure verification/enrollment | OctoSense overview/policy; trusted credential UI; no secrets/templates in scripts; lockout and cancellation |
| Certificates, credential storage, encryption, device admin, Trust | Native link: Security | KeyChain, DevicePolicy, keystore state, Lineage Trust | Alias-scoped choices; admin restrictions; install/removal confirmation; no private-key export |
| Camera and microphone global access | Implemented; owner-user SystemUI-role emulator validated, including actual camera blocking | SensorPrivacy through finite ROM helper hooks | Actual sensor blocked/unblocked in apps, hardware support and screen-lock policy |
| Privacy dashboard, permission manager and Safety Center | Missing | PermissionController, SensorPrivacy, SafetyCenter and AppOps | Camera/mic disabled in actual apps; recent usage and user scope; safety provider availability |
| Location enable and Wi-Fi/Bluetooth scanning flags | Implemented; owner-user service emulator validated, including policy denial | LocationManager and finite scanning settings through ROM helper | Actual readback, current-user/profile restrictions, no invented permission |
| Location accuracy/services and recent app requests | Missing | LocationManager and scanning/settings services | Permissions and global off reflected in actual location requests; work profile policy |
| Emergency information, emergency gesture and wireless alerts | Missing | Emergency app/telephony contracts and platform confirmation | Avoid live emergency calls in tests; validation mode for emergency routing; region/carrier availability |
| Account inventory, providers, master and per-authority sync | Implemented; owner-user broker/full and ordinary-app/limited emulator paths validated | Explicit full/limited visibility; bounded account/provider/authority identities; AccountManager/ContentResolver and narrow system-UID SettingsBroker owner-user route | Current-user inventory vs public visibility; automatic sync readback, pending/active sync, request/cancel, revoked capabilities, stable pagination/theme/Back |
| Account Add/visibility consent/Remove | Implemented; platform/provider consent and exact-incarnation removal emulator validated | Typed observed provider/account; Home account chooser; immutable SettingsBroker removal confirmation with incarnation/policy recheck | Native Cancel preserves account; confirmation only removes exact incarnation; provider auth cancellation; no credential payload enters scripts |
| Detailed sync history/errors, work-profile/account administration | Missing; native Accounts fallback | Sync status/history and user/profile policy services | Actual failure reasons/retries; managed/secondary users and restricted providers; no cross-user visibility leak |
| Passwords/passkeys/autofill/credential-provider defaults | Native link: Accounts (subfeatures unverified) | CredentialManager, Autofill/Role services, provider consent | Default selection; real sign-in flow; locked/profile behavior; no credential payload retention |
| Work profile, private space, multiple users | Native link: System/Security (subfeatures unverified) | UserManager/DevicePolicy/LockSettings | Isolation, lock/unlock, quiet mode, user switching, policy-controlled visibility |
| Settings UI accessibility tree/actions | Implemented; Android virtual-node emulator validated2437 | Trusted visible Settings tree, stable semantic node identities, clipped scroll ancestor, finite widget actions and lifecycle clearing | 403 Home tests plus33 actual UiAutomation checks on both API35 emulators: editing, stable focus, Back/reopen, native Activity retirement, scroll ancestry and stale/disabled node denial; full TalkBack/switch-access and hardware acceptance remain |
| Accessibility color inversion/correction | Implemented; emulator accepted2529 | Finite three-row secure-setting allowlist; independent correction enable and all four native modes |106 native checks demonstrate distinct compositor transforms, combined inversion, restart/external state, unknown-mode denial and exact restoration. Ordinary18 unavailable/native-recovery checks, cold33 and smoke33 on both emulators. [Color audit](settings-accessibility-controls-audit.md); saturation/shortcuts, standard-entry takeover and hardware remain separate |
| Mono audio, balance, caption enable/size/preset | Implemented; emulator accepted2532 | Finite existing Controls boundary, exact native observations,201 balance positions and coupled caption-enable semantics |306 native checks with AudioFlinger mixer gains and independent CaptioningManager consumer/rendering; all5 sizes/6 presets, custom/restart/stale denial and exact restoration. Ordinary30, cold33/smoke33 both;4 identities/40roles/906grants preserved. [Hearing audit](settings-accessibility-hearing-audit.md); physical audio and hearing aids remain separate |
| Caption custom appearance and language | Implemented; emulator accepted2533 | Nine finite appearance fields with scoped cache; full color palette;24-row native asset language chooser |316 custom/86 language native checks,23+38 ordinary unavailable,58 Binder denials per signer; cold41/10launches and smoke33 both emulators. Native style/locale callbacks, restart/stale/cache/Back and exact restoration passed. Color/opacity sample preview works; Android-specific typeface/edge preview remains incomplete. [Hearing audit](settings-accessibility-hearing-audit.md) |
| Text and basic interaction accessibility | Implemented;2535 native behavior and2537 renderer/navigation validated | Seven finite controls plus read-only shared renderer preferences; no new app permissions |555 native checks incl rendered TextView weight, real long presses, both recommended timeouts, WMS scales and kernel-mouse automatic click;2537 ordinary30 unavailable checks,60 caller denials per signer. Makepad bold/contrast screenshots restore exactly;2537 passes45 cold checks/11 launches and33 smoke on5560 after detecting broken emulator shader cache. [Text/interaction audit](settings-accessibility-text-motor-audit.md); a first-ever driver-cache launch, physical pointer appearance and hardware remain separate |
| TalkBack, remaining vision/motor controls and hearing devices | Native link: Accessibility | AccessibilityManager, enabled-service secure flow, Settings/permission state | Real TalkBack tree/focus/actions, large text and contrast, switch/keyboard navigation; native links alone fail parity |
| Ordered system languages | Implemented; native emulator accepted2539 | Native Android language/region/numbering catalog, reviewed add/reorder/remove drafts, one-use observed targets; full locale tags and regional Unicode preferences preserved |248 native checks prove French/Chinese/Arabic resources, Arabic RTL, seven changes preserving Home’s actual Activity, review/Cancel/restart/stale denial, and exact language/provider restoration. Numbering branches have unit coverage; none was selected in this emulator run. [Language audit](settings-system-language-audit.md); full Home translation/RTL and hardware remain separate |
| Regional formats, keyboard/input, mouse/trackpad, TTS | Native link: System; keyboard implementation in progress | Locale/InputMethod/TextToSpeech and input-device services; native keyboard eligibility, default picker and warning/save flows | Regional formats; IME secure picker and composition; hardware input; native handoff alone does not complete subtype-editing parity |
| Gestures, navigation, buttons, power menu | Native link: System | Window/input services, overlays and Lineage hardware-key/gesture backend | OnePlus keys/alert slider if supported; back/home navigation; safe recovery from remapping |
| Lineage status bar, traffic monitor, LEDs, profiles | Missing | LineageParts controllers and supporting services/hardware | Capability-gated behavior, reboot persistence, notification/battery LED tests |
| System/Home OTA status, explicit check, reviewed install and restart | In progress: built-in System updates page | Existing updater with captured expiring offer keys, serialized mutations, observed service phases/errors and signed payload/APK enforcement | Read polls never check/install; exact reviewed offer, changed/expired targets rejected; separate restart confirmation; emulator tests must not apply updates or reboot |
| OTA cancellation/resume, feed/channel editing and release hardware acceptance | Missing | Extend existing updater finite API and user-visible recovery; retain one downloader | Real signed update/digest rejection, interruption/resume, free-space/power failures, reboot and retained data on authorized hardware trial |
| Backup/reset/factory reset | Native link: System | BackupManager, RecoverySystem/DevicePolicy with platform confirmation | Backup provider/error state; destructive actions require clear secure confirmation; no automated reset in validation |
| Developer options, USB/wireless debugging, diagnostics | Native link: System | DevelopmentSettings/AdbManager and restricted engineering service APIs | Developer enablement/owner checks; pairing authorization; visible persistent debugging indication |
| Built-in search | Implemented; emulator validated2418–2437 | Local English/Chinese aliases to reviewed built-in routes | Stable editor/draft/Back, live theme, causal IME and Android accessibility checks passed; full product localization remains separate |
| Android Settings entry intents | Implemented; ordinary and service-enabled API35 emulator validation through 2524 | 30 finite custom routes, 22 standard Android actions, native-ready/receipt handshake and initial state reconciliation, latest-only cold queue and exact trusted singleton | 2539 passes12 cold starts/49 checks,60 entry checks and33 accessibility checks on both emulators; per-app notifications, Defaults and DND retain their focused entry coverage. No acknowledged-entry replay; main/alias Activity reuse; no arbitrary parameters/mutations, old confirmations retired, Home/Bridge native fallback pinned |
| ROM Settings default, Quickstep and SystemUI entry points | Implemented; updated-system routing emulator accepted2505/2510/2513/2516 | Priority-2 standard actions on the real renderer, separate Defaults filter at3, custom-only alias at0; Quickstep finite preferred routing requires enabled exported exact system Home alias and platform signature; native recovery/consent routes stay explicit | 58 repo tests; ordinary install keeps native default. Privileged fixtures passed the original19 implicit actions (2505,46 checks) and all20 including the separate Night Display action (2510,48 checks), preferred/native/unsupported fallbacks and actual SystemUI gear; 2513 adds the21st standard per-app notification action (53 checks);2516 adds Defaults as the22nd (60 checks), old-base fallback/new-base priority3 and data-update survival. Preferred Defaults also checks alias capability metadata. Cleanup retained native fallback5, cold29, a11y33, all47 preferences and app identity. Complete production ROM boot, customized Quickstep APK runtime and hardware remain pending |
| Help/support, communal/dock features, conditional external display | Missing; conditional target | Device/controller-gated providers and supported hardware | Only expose if ROM/product enables it; document an explicit not-applicable result |

## Security and delivery gates

The module/instance root check runs before every command. DeviceSetting is a
finite typed union; both Rust and Android validate values, permissions, policy
and version support. Read snapshots use optional fields, correlated request IDs,
bounded retry and active-page polling. Timeout or queue resynchronization never
replays a mutation. The ROM helper retains package/signature checks and a
[privileged permission allowlist](https://source.android.com/docs/core/permissions/perms-allowlist).
Native modules sharing Home's UID/process are trusted code, not mutually isolated
Android sandboxes.

Credentials, authentication tokens, biometric templates and private keys stay
inside platform/provider-owned flows. OctoSense may launch a specifically
authorized secure flow and consume its limited outcome; it must not simulate
the secure confirmation UI or bypass authentication. See
[Gatekeeper](https://source.android.com/docs/security/features/authentication/gatekeeper)
and [platform biometric authentication](https://developer.android.com/identity/sign-in/biometric-auth).

Each completed row needs: Rust capability/state tests, backend validation and
readback tests, emulator behavior where applicable, OnePlus 6 hardware checks,
external-change/restart checks, light/dark/live-theme and narrow-screen checks,
localization, accessibility, ordinary-script denial and restricted-user behavior.
The upstream Makepad Android accessibility path is incomplete. The dedicated
Settings virtual-node adapter now derives visible controls from the compiled
widget tree; unit coverage does not by itself establish Android/TalkBack parity.
Retain recovery navigation while device and assistive-service acceptance remains
open.

Suggested order: (1) core observed device controls and informative pages;
(2) connection management; (3) app/notification/Modes and battery/storage policy;
(4) security/privacy/accounts through secure platform APIs; (5) accessibility,
input, device-specific Lineage controls and advanced system flows; (6) intent,
search, localization and complete hardware parity. Parallel backend work must
not mark rows implemented before their actual OctoSense UI passes acceptance.


The stable editor transport gate passed on isolated Gboard/API35 build2431:
cold first-focus typing, eight immediate Ctrl+A replacements, eight Clear/input
runs, Back remaining hidden through refresh and after native Activity return,
retap and Done. A real InputConnection/native EditText fixture covers composition,
selected/surrogate deletion and retired editor query/action isolation. Later2432 cold tests exposed duplicate pre-focus InputConnections. Build2436 fixes
that session ownership defect: six cold Gboard and six immediate hardware trials,
eight Ctrl+A and eight Clear trials, Back/retap/Done and native-Activity return
passed on Gboard. The independent AOSP emulator passed three cold, four Ctrl+A,
four Clear trials and native-Activity return. Initial inactive replies preserve
pre-focus handles; pause/inactive retirement invalidates every handle in the
cohort. The Android regression fails before the fix and passes afterward. This evidence does not complete the language/input Settings feature
area, TalkBack or all-IME/hardware acceptance. No physical phone was modified.
