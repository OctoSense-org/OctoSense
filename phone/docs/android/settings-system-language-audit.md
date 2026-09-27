# Ordered system languages

The built-in implementation provides the current ordered system language list,
native language/region/numbering choices, local add/reorder/remove drafts, and an
explicit review before applying an order. System → System languages, local
Settings search, and the finite custom `system_languages` entry open it. The
public Android locale intent remains native until its routing is separately
validated. Keyboard enablement, keyboard defaults, regional formats, grammatical
gender, pointer speed, and text-to-speech remain separate work.

## Native authority and behavior

The audit uses Settings revision `0f0669fc699f70adb20fe6ed4b2ff1c600da6f86`
and framework `ff7620a38e54c5f7ec14a5b8ccc5be1ba41e2b1b`. Relevant sources are
`LocaleListEditor`, `LocaleDragAndDropAdapter`, `LocalePickerWithRegion`,
`SystemLocaleCollector`, `LocaleStore`, `LocalePicker`, and ActivityManager's
persistent configuration operation. The earlier language/input proposal and
source receipt paths remain in
`home/android/pending-controls/system-language-input/README.md`.

`SystemLanguagePlatform` runs in the existing UID 1000 Settings Broker. It requires
the current unlocked owner, denies `DISALLOW_CONFIG_LOCALE` and demo mode, and
checks both CHANGE_CONFIGURATION and the effective WRITE_SETTINGS capability.
The exported service retains the signature permission and exact Agent
package/signer/owner gates. Home and Agent receive no locale-setting privilege.

Authoritative observations use ActivityManager's actual Configuration LocaleList.
`LocalePicker.getLocales()` is not sufficient because its RemoteException path
returns the process default. The native collector supplies the complete bounded
catalog; its exclusions, suggestions, region hierarchy, numbering leaves, and
single-child shortcuts follow the pinned native picker. Sorting adds a stable
semantic tie-break for otherwise equal native comparator results.

Every unchanged language retains its complete locale tag and Unicode extensions.
Adding a native leaf merges all Unicode keywords from the actual
`Settings.System.LOCALE_PREFERENCES`, matching the native editor. A language
parent is never inferred to be selectable from its label. The native suggested
row and parent/numbering branch conditions determine whether it is a leaf.

The write uses `LocalePicker.updateLocales`, preserving native filtering,
persistent-configuration attribution, `userSetLocale=true`, and SettingsProvider
backup. This native method catches RemoteException internally. A returned call
is therefore not success: the adapter rechecks authority and compares the full
actual ordered LocaleList with the reviewed order. A mismatch is unconfirmed and
cannot be retried with the consumed target.

The AOSP 35 validation framework has a package-private collector class and
constructor while the pinned ROM makes them public. The adapter resolves that
exact native class by name and invokes only its exact constructor and supported
list method through finite reflection, validating the native return type and
LocaleInfo rows. Direct type/class-literal/method references are avoided. It does not link the incompatible ignored-list method
whose return descriptor differs between these revisions. The public collector
method descriptors were checked against actual framework DEX
`f3a7bca4fadf18b9454d004f7aec9dea9a44cb197e61df80ed545a97e87b107c`.
Unsupported linkage fails closed.

## Finite interface and presentation

Agent transactions 82/83 are `getSystemLanguagesSnapshot` and
`applySystemLanguages`; Broker has the matching finite read/apply interface.
The read carries request ID, optional catalog and parent keys, a filter, and page
offset. The write carries a catalog key and an ordered list of observed opaque
targets. Neither accepts caller-supplied locale tags.

Catalogs are bounded at 4096 nodes/depth 3, current orders at 128 entries, and pages
at 20. Filters accept 80 Unicode code points. A ten-minute fixed catalog lifetime
does not renew on polling. Current entries and leaves actually published on
visited pages remain available to the local draft for that catalog lifetime.
Branches cannot be applied as languages. Duplicate targets, duplicate resolved
locales, unknown leaves, and an empty order are rejected.

The key binds the actual current order, full native catalog semantics, regional
preferences, labels, and writable state. Claim consumes it before fresh native
revalidation and writing. Focus loss, owner loss, expiry, policy changes, or
configuration changes remove write authority. No automatic mutation retry exists.
The finite operation necessarily has a final-check/write race with a concurrent
system configuration change; it is not described as an atomic system transaction.

Home presents stable, themed widgets and bounded current-order/catalog pages.
Add, move, and remove change only a local draft. Removing the last language is
disabled. Review names a changed preferred display language and explains that
Android/apps can change layout direction or restart screens. Apply is explicit.
Stale or failed requests retain the draft while disabling its old authority;
successful transport alone does not replace the observed current order. Only an
actual matching native observation clears the submitted draft. Back preserves
hierarchy state, and Cancel makes no language write.

Accessibility actions bind each exact target and move/remove operation. A held
pointer or stale node cannot retarget a replacement list row. Native recovery is
the trusted Android language screen in a fresh task, preserving the built-in
page when Back returns.

## Validation status

Integrated Home tests passed **518**, including eight new model/UI tests for
reviewed order, complete custom tags, last-language protection, stale/pending
authority, stable theme/editor identity, branch Back, and held-target retirement.
Pure Java backend/client and actual native-adapter tests both passed, covering
hierarchy and numbering, deterministic catalogs, Unicode preferences, current
custom tags, one-use leases, fixed expiry, cross-page drafts, policy/owner/access
loss, swallowed native failures, and exact observed readback. Eighty astral code
points are accepted and 81 rejected consistently by Java and Rust.

The pinned Agent+Broker modules compiled successfully. The parent installed them
only on the disposable privileged emulator 5560 and reported **64 denied calls
per signer** for an ordinary and an unrelated platform-signed caller. Home2539
with Broker r2 passed **248 native checks**: real French, Chinese, and Arabic
resource text, Arabic RTL, order/add/reorder/remove, review/Cancel, restart, stale
regional preferences, and exact restored en-US/provider rows/userSetLocale=false.
All seven native Apply operations preserved Home's PID and Java Activity identity.
The receipt is
`out/home/settings-system-languages-r2/validation/octosense-system-languages2539-native5560-r1.json`.
The tested selections did not traverse a numbering-system leaf; native device
coverage for that branch remains open, with unit coverage only.

Ordinary Home2538 on emulator 5556 passed 15 checks for disabled mutation
controls, trusted native recovery/Back, unchanged authoritative en-US/order,
exact raw provider rows, and userSetLocale=false. Cold 49/12 launches,
entries 60/30 custom routes, and smoke 33 also passed. Fixture removal, all 11
existing text/motor rows, Home identity, and an empty crash buffer were verified.
The same ordinary gates passed on final Home2539: 15 unavailable, 49 cold,
60 entries, and 33 smoke checks, with identity, all 11 rows, fixture cleanup, and
empty crash buffer verified in `ordinary2539-summary5556.json`.

The separate `system-languages` public-SDK fixture and
`run-system-languages-probe.py` compile successfully. They are independent of
the shared Settings accessibility probe. The ordinary scenario is restricted
to emulator 5556; native mutations are restricted to the disposable 5560 AVD.
The native scenario changes languages through built-in UI only and checks public
LocaleManager order, real translated French/Chinese/Arabic TextView resources,
RTL, Activity recreation, review/Cancel, process restart, and external regional
preference invalidation. Its shell helper only reads actual Configuration.

Cleanup restores the original ordered language list through the built-in native
path before restoring exact absent/empty provider rows. Receipts separately
record `userSetLocale`: native LocalePicker always sets it true, and the fixture
does not invent a hidden setter to reset that metadata. Provider-row restoration
is never described as full Configuration restoration. The numbering hierarchy
has implementation/unit coverage; device coverage depends on actual offered
numbering choices and is reported explicitly.

The retained Home2538 failures distinguish product defects from fixture defects:

- Native r1 stopped before writing because the adapter directly referenced the
  nonpublic collector class. Broker-process instrumentation and the regression
  that compiles against a public class but runs against a nonpublic class both
  reproduced IllegalAccessError. Broker r2 uses the finite reflection fix.
- Native r2's fixture rejected a valid `fr-CA-u-hc-h23` tag because its regex did
  not allow singleton subtags. The fixture was corrected; no product change.
- Native r3 passed real French resource rendering, then its scrolling helper
  used a retired backward-scroll node after a list edit had reset scroll. The
  helper now reobserves navigation state within its existing deadline.
- Native r4 exposed a product lifecycle defect: locale/layoutDirection changes
  recreated Home's renderer Activity and left the JNI channel shut down. Its
  failed run and recovery receipt remain under `native-r4-failure5560`. The
  parent restored the original configuration through native Android Languages.
  Home2539 adds locale/layoutDirection to the configuration-change mask
  (`0xd0003fa4`) and uses the existing configuration callback. Native248 then
  proved Activity preservation across all seven real Apply operations.

Settings-private recommendation bookkeeping and pinned shortcut updates are not
reimplemented. Full Home translation/RTL layout, a production ROM boot, secondary
users, organization deployment, and physical-device behavior are not claimed by
this increment. No physical phone is used.
