# Proposed System languages and input increment

Read-only design against pinned Lineage Android15. These files are outside all
production/native source filegroups and are not part of Home2534/2535. No device
locale, keyboard or speech setting has been changed for this audit.

The next concrete implementation should be the complete ordered **System
languages** editor: observe current order, add via native hierarchical catalog,
move up/down, remove with explicit review, and retain at least one language.
Implement keyboard inventory/default selection in the following bounded slice;
new keyboard enablement must preserve Android's native security and Direct Boot
warnings. Pointer speed and TTS are separate service/preview workflows, not extra
unchecked provider rows in a generic list.

## Native evidence

Settings revision `0f0669fc699f70adb20fe6ed4b2ff1c600da6f86`; framework
`ff7620a38e54c5f7ec14a5b8ccc5be1ba41e2b1b`. Source receipts:

- `/tmp/octosense-system-language-input-native.txt`: Settings LocaleListEditor,
  LocaleDragAndDropAdapter and AvailableVirtualKeyboardFragment.
- `/tmp/octosense-system-language-input-framework.txt`: LocalePicker,
  SettingsLib InputMethodPreference/InputMethodAndSubtypeUtil,
  PointerSpeedPreference and TextToSpeechSettings.
- `/tmp/octosense-system-language-input-apis.txt`: InputSettings,
  InputMethodManager, native language picker/dialog, IME eligibility wrapper and
  TtsEnginePreferenceFragment.
- `/tmp/octosense-system-language-input-authority.txt` and `-authority2.txt`:
  SystemLocaleCollector, locale/IME native authority and picker branching.

The actual AOSP validation framework SHA `f3a7bca4fadf18b9454d004f7aec9dea9a44cb197e61df80ed545a97e87b107c`
was audited from its DEX class definitions, without device mutation. Receipt:
`/tmp/octosense-system-language-input-runtime-framework.json`. This exposed two
real ABI differences: SystemLocaleCollector(Context,LocaleList) is package-private
on AOSP35 (public on pinned Lineage), and getIgnoredLocaleList(boolean) returns
HashSet on AOSP35 versus Set in the pinned declaration. Do not directly link
those incompatible methods. A narrowly named native reflection constructor
(`getDeclaredConstructor(Context.class,LocaleList.class)`) with explicit access
checks/caught linkage failure may reuse the exact runtime implementation, but
must be validated in the intended privileged context before claiming support.
The catalog's public getSupportedLocaleList(LocaleInfo,boolean,boolean) matches
both. Avoid calling getIgnoredLocaleList directly. LocalePicker update/read,
ActivityManager persistent-config/read, InputSettings speed and InputMethodManager
list/picker exact descriptors were found. This is ABI presence, not permission or
mutation acceptance.

## Ordered system languages

`LocaleListEditor` is restricted by `DISALLOW_CONFIG_LOCALE`. Its inventory comes
from `LocalePicker.getLocales()`; the adapter retains complete `Locale` objects.
Adding a language appends it. Removing all entries is forbidden. Changing the
first entry receives a native confirmation; a built-in explicit review must
likewise name the future display language before a write. Secondary reorder does
not need a destructive warning. A removed first language changes the system's
primary locale and must not be presented as merely removing a list row.

Native picker calls `LocalePickerWithRegion.createLanguagePicker` with
`translatedOnly=false`, no app package and no explicit locale restriction outside
device demo mode. It uses `SystemLocaleCollector`, whose ignored set is the full
current language-tag list. Region and numbering branches must follow the native
picker's actual `hasNumberingSystems`/parent logic, including the single-child
case already audited for per-app languages. Keep untranslated support visible;
do not substitute a hardcoded short language list.

Before appending, native Settings applies every Unicode locale keyword from
`Settings.System.LOCALE_PREFERENCES` via `Locale.Builder`. Preserve those exact
extensions as well as extensions already present in unchanged current entries.
Native application is `LocalePicker.updateLocales`, which filters excluded
locales, sends persistent Configuration with `userSetLocale=true`, and marks the
SettingsProvider backup dirty. A direct write of `system_locales` is not an
acceptable replacement. Its helper intentionally catches RemoteException, so a
returned call alone is never success: re-read authoritative Configuration and
require exact observed order/tags. LocalePicker.getLocales also falls back to the
process default on RemoteException; the adapter must additionally read
ActivityManager.getConfiguration with explicit failure handling before/after
catalog assembly, rather than treating that process fallback as authority. A configuration change must not retire an
in-flight operation merely because it is that operation's expected consequence.

Use the existing UID1000 Broker behind exact Agent signer/package/current
unlocked owner gates, with a new finite language-specific interface. No grant
privilege or arbitrary locale-string setter in Home or Agent. ActivityManager
enforces CHANGE_CONFIGURATION and checkAndNoteWriteSettingsOperation for the
actual calling package before the persistent update; verify both Broker
manifest/UID permissions and the WRITE_SETTINGS AppOp, not just system signing. Recheck
DISALLOW_CONFIG_LOCALE and fresh native catalog/config at claim time. Catalog
keys bind current complete ordered list, regional keyword preferences, native
catalog semantics, policy and owner. Cap published pages at20; bound the catalog
and current-list input without silently dropping an existing language. Branches
use opaque targets and never accept free-form tags. Reads do not update locales.

Suggested built-in workflow:

1. **System languages** shows native names and ordered full locale tags; each
   observed row exposes Move up, Move down, Remove where valid.
2. **Add a language** is the native hierarchy in stable bounded pages with local
   filtering, nested Back and full region/numbering leaf selection.
3. Any add/reorder/remove creates a local reviewed draft with current and proposed
   order. **Apply language order / Cancel** remain reachable above the list.
4. The only write is an expiring one-use reviewed operation derived from observed
   opaque targets. Replaced catalog/current list/policy retires Apply while
   keeping the draft visible for review; never replay after a configuration or
   process restart. Pending is separate from observed success.

Settings-specific side effects require an explicit decision: the native adapter
also updates Settings' pinned shortcuts and maintains its private language
recommendation bookkeeping. These are not general LocaleManager operations.
Implementing the locale service does not silently claim those Settings-owned
features, regional-format/gender preferences, or complete Home localization.

Emulator acceptance: capture full Configuration LocaleList, raw system locale and
regional preference rows; use synthetic consumer resource locales to prove real
French/Chinese/RTL configuration and Activity recreation, not just provider
strings. Add nonprimary languages first, reorder with Cancel/Apply, remove a
nonprimary then a primary, reject last removal, custom Unicode preservation,
stale external list/policy change and process restart. Restore the exact original
ordered list through the same native service, then verify userSetLocale/raw rows
and unrelated settings. Only disposable5560 changes system language. Ordinary
5556 must show explicit unavailable mutation capability plus trusted native
recovery. No hardware claim.

## Keyboards and default input

Native `AvailableVirtualKeyboardFragment` reads input methods for its actual
user, including enabled methods, and refreshes `InputMethodSettingValuesWrapper`.
The wrapper excludes virtual-device-only IMEs. Organization permission is based
on `DevicePolicyManager.getPermittedInputMethods`: an already enabled disallowed
IME can remain disable-able, but must not be re-enabled afterward. It protects
the last enabled IME and the last valid nonauxiliary ASCII-capable system IME;
hardware-keyboard state also enters save semantics. Do not replace that policy
with only `isSystem` or a package allowlist.

`InputMethodPreference` first shows the native third-party security warning,
then a second Direct Boot warning if required. System IMEs bypass only the first
warning; non-Direct-Boot-aware system IMEs still warn (except TV). Cancel/Back
must make no enabling write. Native saves update enabled IMEs and subtype sets,
disabled-system-IME set, selected subtype and default consistently. Preserve all
untouched subtype state. Provider IDs and subtype hashes are observations, not
open setters across Binder.

For an eventual complete built-in IME list, use opaque observed service targets
bound to package incarnation, enabled/subtype/default state, current user,
organization policy and hardware-keyboard state. Default choices come only from
currently enabled eligible methods. Native InputMethodManager explicitly warns
that `setInputMethod(null,id)` is deprecated, and UID1000 calls return without
changing anything. Its documented migration writes selected subtype=-1 then the
observed default IME ID; if reused behind a finite Broker operation, recheck
between writes and report partial/readback outcomes. Prefer native system picker
for the first secure default-selection flow until exact service semantics are
validated; name that handoff honestly rather than claiming built-in selection.

Enabling a new IME should use a narrow Settings-owned adapter reusing native
`InputMethodPreference` and save logic, with an unexported native consent Activity
and exact current target/state checks at each approval. This requires a Settings
APK staging/validation plan; the existing PermissionController adapter does not
own these native classes. Do not copy only the warning strings into Home or enable
silently via a new generic secure-setting operation. Built-in inventory, native
consent and actual readback can coexist. IME-owned configuration/subtype editors
remain provider-owned unless separately implemented and verified.

Acceptance needs two independently signed synthetic IMEs (Direct Boot aware and
unaware), no network or real text handling, synthetic editor only. Verify native
Cancel/Back and both warnings, exact native enabled/default/subtype observations,
actual selected IME connection, last-keyboard protection, organization policy,
stale uninstall/reinstall approval and exact restoration. No production keyboard
package/data is removed. Ordinary inventory visibility is identified as limited
if platform caller visibility is not complete.

## Pointer and speech follow-ups

Pointer speed is a15-value domain [-7,7], default0, from `InputSettings`.
`setPointerSpeed` requires WRITE_SETTINGS. Native dialog previews with
`InputManager.tryPointerSpeed`, restores its old effective speed on Cancel, and
commits only positive confirmation. Preview requires SET_POINTER_SPEED and an
explicit lifecycle stop/restore, including foreground loss and external writes;
read-only hardware inventory does not establish cursor movement. Do not combine
trackpad-specific controls or new vector-cursor flags without separate capability
and runtime descriptor checks. A finite draft without preview is incomplete
relative to this native dialog and must be recorded as such.

TTS is an asynchronous engine workflow. Native engine choice warns for non-system
engines, binds a new `TextToSpeech`, falls back on init failure, and persists the
actual bound engine only on native SUCCESS. The broader TextToSpeechSettings
page separately checks voice data through package-pinned engine Activities and
updates language/sample capabilities from their results.
Native sample/data-install/engine configuration and network-required voice consent
are provider/platform-owned. Rate/pitch use native ranges and preview; supported
language comes from the selected engine's actual returned voices. Inventory alone
or blindly writing `tts_default_synth` would not implement TTS parity. No speech or
network test is authorized by this audit; future emulator fixture must synthesize
local test text without real user data and explicitly bound/stop playback.
