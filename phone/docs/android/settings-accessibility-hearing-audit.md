# Hearing and caption controls: native audit and acceptance

The built-in hearing page and backend passed ordinary and privileged emulator acceptance on Home2532. Validation temporarily changed synthetic audio/caption preferences and restored every exact original row; stream volumes and DND were preserved. No physical phone was touched. The pinned Settings
source is `0f0669fc699f70adb20fe6ed4b2ff1c600da6f86`; framework source is
`ff7620a38e54c5f7ec14a5b8ccc5be1ba41e2b1b`. Per-app language validation remains independent.

## Native behavior

`PrimaryMonoPreferenceController` reads/writes `Settings.System.MASTER_MONO`
for the current user, with absent value 0. `BalanceSeekBarPreference` and
`BalanceSeekBar` read `MASTER_BALANCE`, absent 0, and expose 201 positions from
-1.00 through +1.00 in 0.01 increments, with a center snap. The two native
controllers are generally available; they do not impose an additional
`DISALLOW_ADJUST_VOLUME` UI restriction. Do not invent that restriction for
these settings merely because it applies to stream-volume changes.
`AudioService` observes both keys and applies `AudioSystem.setMasterMono` and
`setMasterBalance`. The corresponding native getters can independently verify
service state. A stored preference alone does not establish an audible effect.

`CaptioningTogglePreferenceController` delegates to `CaptionHelper`, which writes
`ACCESSIBILITY_CAPTIONING_ENABLED` and reads `CaptioningManager.isEnabled()`.
Missing caption enabled/font/preset values resolve to 0/1.0/0 respectively,
from CaptioningManager (the first displayed preset is 4, but it is not the absent
preference default). The font-size controller writes `ACCESSIBILITY_CAPTIONING_FONT_SCALE`; native
choices are 0.25, 0.5, 1.0, 1.5 and 2.0. Preset choices from the trusted native
Settings arrays are Set by app (4), White on black (0), Black on white (1), Yellow
on black (2), Yellow on blue (3), and Custom (-1). Selecting a font size or preset also enables captions through
`CaptionHelper.setEnabled(true)`. The selected field and enabled state are two
provider writes, not an atomic transaction. Existing custom
foreground/background/window/edge/typeface preferences must remain intact. A malformed or unsupported raw preset must be
validated before calling `getUserStyle()`, which indexes native preset data.

Caption language is a separate preference, not device or per-app language.
`CaptioningLocalePreferenceController` stores `CaptioningManager.getRawLocale()`
and writes a choice from native `LocalePreference`; empty follows the default.
It should use a bounded native catalogue with opaque targets rather than reusing
the per-app configuration filter or accepting an arbitrary locale string.

Native custom caption appearance includes foreground/background/window colors
and opacities, edge type/color, and typeface. The native custom-preference
availability also depends on `fixA11ySettingsSearch`; it is not equivalent to a
single preset dropdown. These are explicit remaining subfeatures until built-in
editing and preview are implemented.

Live Caption is separately capability-gated: `LiveCaptionPreferenceController`
resolves `com.android.settings.action.live_caption` only inside
`PackageManager.getSystemCaptionsServicePackageName()`. Its inference/service
implementation is not the ordinary CaptioningManager preference. Hearing aids
also depend on real Bluetooth profile/device capabilities, pairing and provider
UI. Neither can be declared replaced based on a native link or a setting bit.

## Concrete first implementation

Use a new compiled Hearing controls page under System → Accessibility, retaining
a precise native recovery action. Extend the existing finite Controls boundary
for mono audio, caption enabled, caption font size and caption preset. Existing
Agent write authority and active unlocked-owner guards suffice; no new Binder
transaction, Home write privilege or arbitrary SettingsProvider key is needed.
Use fresh observed values, preserve absent/unknown distinctions, and recheck
write capability at dispatch. Report applied only after native readback; expose
pending service propagation honestly. The generic stable choice renderer needs
six slots for all native presets and five for font size, with existing semantic
row identity and touch-scroll protections retained.

Balance must preserve the full native 201-step range, not claim parity with
only Left/Center/Right presets. A bounded typed balance observation can retain
the actual float and expose a finite integer target in -100..100, mapped by the
backend to hundredths. Existing custom floats remain visible and unchanged
until an explicit user choice. A built-in slider or step buttons plus Center
must use the same typed boundary; no free-form decimal setter is required.
This needs a small dedicated renderer/model addition rather than silently
rounding the observation to the generic unsigned battery-threshold type.

Caption-language and custom-appearance editors can follow as distinct bounded
pieces, preserving the same native readback and current-user rules. The first
page must label them as remaining Android controls rather than implying the
entire captions surface is complete. A custom preset may be selected while
retaining its stored custom values; it does not authorize modifying them.

## Validation plan

Before implementation freeze, verify all hidden AudioSystem descriptors against
the actual emulator framework, as done for language and DND. Pure backend tests
cover absent defaults, malformed raw data, custom values, read-only and policy
revocation, owner/lock changes, write failure and delayed native readback. Rust
checks cover all choices, exact balance bounds, six-choice narrow layouts at
150% font scale, stable focus/drafts, stale held presses and recovery Back.

Privileged emulator acceptance should capture and restore exact raw rows,
including absence versus empty. Observe AudioSystem/AudioFlinger mono and
balance state independently from SettingsProvider; preserve all stream volumes
and DND state. A synthetic caption consumer should register the public
CaptioningManager listener, show actual font/style changes, and report observed
values. Test all preset/font choices, Off editing, restart persistence,
external changes and exact preservation of custom style rows. Reading/writing
raw keys alone is insufficient evidence of consumer behavior. No actual call,
recording, physical hearing aid or user media is needed. An ordinary emulator
must show real observations where available, disabled privileged choices,
trusted native recovery and preserved Back state. Physical audio channel
routing, hearing aid behavior and third-party caption compliance remain explicit
hardware/application acceptance gates.

Local source bundles are `/tmp/octosense-accessibility-hearing-native.txt`,
`/tmp/octosense-hearing-native-{controllers,audio,capabilities,defaults}.txt`.
These are evidence paths, not required product runtime assets.


## Isolated implementation status

`HearingSettings` now validates the five finite controls and native mutation
choices. Its pure Java regression covers all 201 balance values, five caption
sizes, six presets, custom balance 0.123/font 1.25, missing defaults, malformed
values, access revocation, failed writes and delayed service readback. The Agent
`HearingPlatformSettings` adapter compares the raw preference against
AudioSystem or CaptioningManager and checks current unlocked-owner authority
before and after observations. A write is applied only when both agree;
accepted propagation delays remain requested. Shared integration and the corrected native Agent build are accepted on emulator5560. A final native audit found
that font/preset edits also enable captions. The corrected backend now performs
that linked write after a fresh authority and enabled-state check. It skips an
already-enabled write, requires both observed results before reporting applied,
and returns a partial result if the first write succeeds but the second cannot
complete. No action is replayed. Regression coverage includes loss of authority
between writes and during final readback, malformed enabled observations,
failed enable writes and delayed service propagation. Runtime AudioSystem getter
descriptors were independently verified by the parent on emulator5560; its
AudioFlinger dump exposes actual master mono and balance for later acceptance.

Wire observations preserve native float32 precision (`balance:<float>` and
`caption_scale:<float>`). Balance commands are separate finite integer targets
`balance_percent:-100..100`; custom observation strings cannot be used as a
write path. Caption size writes are the exact five canonical native values.
These values stay inside the compiled Controls contract; scripts cannot name
provider tables or keys.

The combined Rust suite passed 486 tests, including four isolated hearing UI
regressions for the complete balance range/custom observations, all font/style
choices, delayed observed state, revoked or replaced held choices, theme changes
and narrow 150% layouts. Home2532 on ordinary emulator5556 passed 30 hearing
read-only/recovery checks, eight cold launches (33 checks), and accessibility
smoke (33). The installed certificate, UID, data inode and first-install time
were preserved, the fixture was removed and the crash buffer was empty.
Privileged emulator5560 passed 306 checks with the native Agent compiled against
the pinned Lineage platform. The independent caption app received real enabled,
size and style callbacks and rendered the observed TextView size, visibility and
colors for all five sizes and six presets. Tests also covered linked enablement,
custom observations and unchanged custom fields, restart persistence, unknown
preset denial and a retired accessible choice. AudioFlinger reported actual
mono/balance on both output threads; an inaudible looping PCM track activated
the speaker mixer to verify center and endpoint channel gains. Idle mixer
processor gains retain their last mixed values and are not treated as evidence
of an active effect. The fixture never changes volume or requests recording.

Both emulators passed eight cold launches (33 checks) and accessibility smoke
(33). On5560, all four package identities,40 role records and906 existing
permission grant/flag records matched the original baseline. The caption and
instrumentation fixtures were removed and the crash buffer was empty.
Receipts are in `out/home/settings-hearing-coupled/validation`, alongside the
accepted Agent APK, frozen native inputs and exact probe hashes. Earlier probe
failures are retained: ordinary instrumentation could not restore private
System rows through adopted permission, idle gain expectations were invalid,
and stale scroll/observation timing was not awaited. Final probes restore via
shell stdin with literal quoting, preserve original failures, wait for native
and visible observations, and retry only read/scroll navigation. Mutations are
never retried. These checks do not establish physical stereo output, hearing-aid
behavior or all third-party caption compliance.

## Custom-appearance contract (integrated source; native validation pending)

The compiled Controls descriptors now include a `caption_custom` page, with a separate scoped transport for custom writes. Nine fields
follow the native appearance screen: typeface, foreground color and opacity,
edge type and color, background color and opacity, window color and opacity.
The native custom category is visible only when the observed preset is Custom
(-1); `fixA11ySettingsSearch` changes search availability, not that category's
visibility. A deep route must explain that prerequisite rather than implicitly
selecting Custom. Read actual custom preferences while preserving them; offer
mutations only for the active Custom preset and known enabled/native state.

The finite choice vocabulary is:

| Field | Choices | Boundary |
| --- | --- | --- |
| Foreground/edge color | Default plus 64 colors | `caption_swatch:default` or index 0–63 |
| Background/window color | None plus the same 65 choices | also `caption_swatch:none` |
| Opacity | 25%, 50%, 75%, 100% | `caption_alpha:64`, `128`, `192`, `255` |
| Edge type | Default, None, Outline, Drop shadow, Raised, Depressed | finite `caption_edge:*` enum |
| Typeface | Default, Sans-serif, Sans-serif condensed, Sans-serif monospace, Serif, Serif monospace, Casual, Cursive, Small capitals | nine finite `caption_typeface:*` choices |

Each color index maps to one of the exact 4×4×4 RGB combinations with channel
values 0, 85, 170 and 255. The native palette begins with named colors and then
additional RGB colors; preserve that display order while keeping semantic
identities tied to the color, not its slot. Native foreground/edge palettes have
65 entries, background/window have 66. Only nine native colors have resource
names; remaining labels show their RGB components. A bounded chooser must make
all entries reachable, retain semantic identity on page changes and provide
text labels as well as swatches. The six current generic buttons cannot silently
truncate these palettes or the nine typefaces.

Custom observations are separate from mutation values. A stored color outside
the native palette, custom alpha or unrecognized typeface remains visible and
unchanged until an explicit finite choice. No arbitrary packed-color, provider
key or typeface-string setter is exposed. Keep malformed raw data distinct from
an absent native default; do not normalize it as a side effect of reading.

Native `CaptionUtils` encoding is essential. Default color is represented by
0x00FFFFaa, with cached opacity in the low byte (the older 0x00000100 default is
also recognized). None is 0x000000aa. Ordinary colors use normal ARGB. Parsing
and merging must use the native `CaptionStyle.hasColor` rules, not a test for
alpha alone. Changing opacity preserves the selected RGB/inherit identity;
changing color preserves the native current opacity. Default/None color
disables its opacity control. Native edge color is disabled for edge None;
Default edge is still configurable. Preserve every unrelated custom field and
preset during these changes.

The native foreground/background/window controllers temporarily cache the last
non-default opacity while Default is selected. The isolated backend binds
that cache to its active page and unchanged raw observation, clearing it on
external change, leaving the page or losing authority. It must not reuse a cache
for another user or overwrite a new externally selected color. The integration has explicit enter/leave lifecycle: appended Agent transactions 77/78/79 accept a live Home Binder token, random process session and monotonically increasing page visit. The Agent links Binder death and retires the exact session; an old leave or death callback cannot retire its successor. Home retires immediately on focus loss, pause or destroy without blocking its UI thread. Rust closes the old scope when leaving the appearance subtree. Re-entering or refreshing creates a new visit; periodic reads and confirmed edits retain the current visit. The native ten-minute lifetime does not renew on polls. A process-global unbounded cache would not match native controller lifetime. A native UI comparison will cover the
semi-transparent color → Default → another color transition within one visit
and after reopening.

All custom edits also enable captions, exactly like native font/preset edits.
Recheck owner/unlock and the Custom-preset gate before the first write and again
before a linked enable. Read back the changed preference, enabled state and
CaptioningManager style. Report partial or pending outcomes honestly; these
provider writes are not atomic and must never be retried automatically. A
sample preview must distinguish app-inherited fields from concrete colors.
The current independent consumer renders ordinary style fields and shadow; it
does not yet prove Outline/Raised/Depressed or exact native typeface rendering.
Extend that fixture and compare the native preview before claiming those
visual behaviors.

Custom source validation currently includes pure backend and ordinary-client
regressions for all palette values/alpha composition, observed choices, linked
enable failure, native propagation, stale visits, duplicate enter, old leave,
process-session replacement, expiry, owner loss and external raw changes.
The client test also stalls a native read, retires on the UI path without
waiting for Binder, and verifies that the late reply cannot restore actions.
Rust tests cover all finite values, read-only custom observations, scope
identity, 20-row palette navigation through all 66 background choices, nested
Back, held-button target changes, live theme, and a narrow 150% layout.
The sample converts Android ARGB explicitly to renderer RGBA, with a real
widget assertion for semitransparent RGB(51,102,153). It renders foreground,
background and window colors/opacity and labels its inherited sample defaults.
It explicitly uses the OctoSense font without Android edge effects; exact
Android typeface/outline/raised/depressed preview parity remains pending.
These are source tests, not an accepted native custom-caption APK. Combined
native compilation, caller boundaries and actual caption application still
must pass before this slice is marked emulator accepted.

## Caption-language follow-up (isolated implementation; UI pending)

The source of choices is `LocalePicker.getAllAssetLocales(context, false)`, as
used by native `LocalePreference`, plus the native Default choice. Use its
labels and `Locale.toString()` underscore encoding; this is not a BCP47 free
text field or the per-app LocaleConfig catalogue. The Default value is an empty
string. `CaptioningManager.getRawLocale()` retains exact stored data and
`getLocale()` independently shows the effective parsed locale. Choosing a
caption locale does not enable captions or alter device/per-app locales.

The isolated backend provides a bounded 24-row native catalogue with stable opaque choices, query
filtering and explicit selection. A revision binds owner, raw locale and native
asset catalogue; a stale retained page must not silently redirect a choice.
Preserve a custom or malformed current raw value as such rather than replacing
it with Default. Recheck current-user authority and selected native membership
before the single finite write. The SDK consumer's actual locale callback and
observation establish readback; they do not claim that a video supplies a track
in that language. Live Caption and hearing-aid provider flows remain separate
capability-gated gaps.

Additional pinned-source receipts are `/tmp/octosense-caption-custom-native.txt`,
`/tmp/octosense-caption-custom-native-merge.txt`,
`/tmp/octosense-caption-native-custom-complete.txt`,
`/tmp/octosense-caption-native-preview.txt`, and
`/tmp/octosense-caption-native-dependency.txt`. No device preferences were changed
while preparing this follow-up design.


### Integrated caption editors (2533 emulator accepted)

`CaptionCustomSettings.java` implements the nine fields and finite choices;
`CaptionCustomPlatformSettings.java` supplies native custom-style observations.
The shared Controls model, scoped Binder transport, Home appearance route and
paged Makepad editor are now integrated. The shared backend compiles against SDK33. Pure
regressions pass every palette choice, all typefaces/edges/opacities, all 64×256
RGB/alpha compositions, native encoded Default/None cases, unedited field
preservation, custom observations and paired write outcomes. They also cover
replaced sessions, stale leave/death calls, duplicate entry, hard expiration,
owner/permission loss and external raw-state changes.

Readonly values are `caption_color_argb:<8 lowercase hex>`,
`caption_opacity:<0..255>`, `caption_edge_value:<signed integer>`, and known finite
`caption_typeface:*` or `caption_typeface_custom`. These cannot be passed back as
mutation choices. The raw color retains the exact packed encoding; the opacity
observation follows native CaptionHelper normalization (Default displays 100%
while its previous opacity can be cached for the active editor).

The backend requires an authenticated 64-hex process session plus a positive
page-visit number. `replaceSession(session)` registers a newly authenticated
Home session; `enterScope(session, visit)` is idempotent only for the still-active
visit. `leaveScope`, session invalidation and all apply/choice calls compare both
identities. Retired visits cannot reopen, old session death/leave cannot retire
a newer session, and polls cannot extend the ten-minute hard lifetime. New Home
processes must use distinct sessions even when visit counters restart.

The transport establishes the session from an authenticated client Binder
lifetime and rejects an old registration after a newer host. Append-only Binder
transactions77–79 carry process Binder, session and visit explicitly.
Do not substitute an implicit process-global current visit inside the existing
unscoped setter. Snapshot observations may remain in the Controls renderer, but
choices and writes must use the bound active scope. Leaving the page,
backgrounding, focus loss, destruction and Binder death need explicit retirement;
absence of polling is not a lifecycle event.

The adapter reads all raw caption keys before and after CaptionStyle and
CaptioningManager observations, rejecting a provider change during that read.
Raw/native has-flags and normalized colors must agree. Immediately before a
write, raw state and authority are checked again; an external change at that
point rejects the operation. Each field write preserves other fields, then
conditionally enables captions and verifies both outcomes. These separate
provider transactions still have ordinary external-write races; no atomic
transaction guarantee or automatic retry is claimed. Native runtime descriptors and the transport/UI are integrated. Actual
custom-style acceptance passed on the privileged emulator. All nine fields have finite choices,
with20 palette entries per page. The color/opacity preview explicitly describes
its inherited sample defaults and does not claim Android typeface or edge-effect
rendering; full preview parity remains open.

The isolated `CaptionLocaleSettings` backend and `CaptionLocalePlatformSettings`
adapter now implement the native asset catalog with a1024-locale bound,
24-row pages,80 UTF-16-unit search limit and opaque64-hex review/choice IDs.
Ten-minute reviews do not renew on polling; exact raw locale, catalog entries,
labels and authority must still match at selection. Only actually offered
choices can be selected, and the review is consumed before the single provider
write. Failed readback after a successful write remains requested. No enabled,
style, application-locale or device-locale preference is written. Pure Java
regressions cover paging/search, custom current values, missing/empty defaults,
revocation, catalog/external changes, expiry/clock rollback, duplicate entries,
replays, failed writes and delayed/failed native readback. Runtime native
`getRawLocale`, `getAllAssetLocales` and LocaleInfo descriptors were verified
against emulator5560's actual framework. The isolated typed Rust model retains
custom current values and rejects incomplete or contradictory wire pages.
Binder transactions80–81, Java client, correlated Rust host and the caption
language page are integrated. The page has24-row paging, a persistent filter
editor, exact observed selection, stale-choice retirement and fresh native
caption Settings recovery. Locale selection never enables captions. Real
locale-callback acceptance passed on the privileged emulator.

The combined source passes501 Rust/mobile-app tests and22 Java contract tests.
Focused language UI tests cover observed authority, retirement after dispatch,
held-row replacement and Chinese draft/widget preservation across live themes.
Build2533 is installed and accepted on the ordinary5556 and privileged5560
emulators. Native language acceptance passed86 checks and custom appearance316.
The ordinary installation passed23 language-unavailable and38 custom-unavailable
checks. Both emulators passed41 checks over10 cold launches, including direct
caption routes, plus33 accessibility/typing smoke checks. Both fresh ordinary and
public-test-platform-signed impostors were denied all58 tested Binder operations.

The native consumer verified actual locale callbacks, all nine typeface choices,
all four opacities and six edge types, native packed custom-style colors, caption
enablement, complete65-choice text palette, same-visit opacity cache and its
retirement on leave/restart. Filtering, paging, external changes, retired nodes,
unknown current locales and native Settings Back were exercised. Original
caption/audio rows, volumes and DND were restored exactly. Four package
identities,40 roles and906 permission-grant/flag records stayed unchanged; test
fixtures were removed and crash buffers were empty. No physical phone was used.

Initial fixture failures are retained with the evidence: reads of the native
provider can precede Home’s updated selected state; a no-op selected Default
choice cannot establish a fresh UI barrier; palette counting must wait for the
new page’s loaded summary. The final tests wait for the observed enabled
selection/page and exercise each typeface through an actual change. They do not
retry mutations. These corrections required no production changes.

Frozen build inputs, installed APKs/hashes, source snapshot, all accepted logs,
failed fixture logs and restoration receipts are under
`out/home/settings-caption-editors/validation/`. Physical audio/hearing devices,
Android-specific typeface/edge preview rendering and full accessibility parity
remain open.
