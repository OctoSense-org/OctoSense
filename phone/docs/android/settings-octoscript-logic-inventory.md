# Settings application logic migration to Octoscript

Status: source port complete; PR #21 rebased onto main `49f57b9`; emulator acceptance pending. `settings_app.rs` now compiles the mounted script adapter rather than the previous Rust application handlers. The controller owns the application decisions listed below; native rendering and service authority remain bindings. The original port passed 495 Home tests with `mobile-apps` and 31 focused Java/repository Settings checks; the rebase validation is recorded separately below. Existing native/emulator feature receipts establish the behavior to preserve, not acceptance of this implementation-language change.

## Ownership boundary

The bundled controller owns navigation, drafts, reviews, input handling, local filtering/ranking/paging, selected targets, displayed labels, enabled/visible state, accessibility semantic targets, and decisions to request reads or finite effects. Its only interface is `settings_step(state, event, observed) -> {state, patch, requests, handled}`. Observations are data, never evaluated source. Requests are data decoded by the native adapter, never executable Android commands.

The native layer retains mounted widgets, drawing, canonical IME editing, geometry and clipping, accessibility node allocation and after-draw acknowledgment, Android lifecycle/focus observations, finite request decoding, transport correlation, and authoritative service checks. Android framework policy, Binder authorization, package incarnation checks, and platform-owned consent remain native. These are bindings and security boundaries rather than application navigation or presentation decisions.

## Inventory

| Former application source | Logic now in Octoscript | Native responsibility retained |
| --- | --- | --- |
| `settings_app.rs` | Page, back parent, entry navigation, draft state, error scope, local theme choice, simple display/sound choices, summary labels, press targets | Generic widget/event adapter and Settings module identity |
| `settings_search.rs` | Reviewed local catalog, Unicode normalization, stable ranking, paging, search-return parent | Data bounds at event import |
| `settings_controls.rs`, `settings_control_picker_ui.rs`, text/hearing models | Page descriptors, value labels, selected state, nearest offered choice, history-disable review, bounded picker | Strict native observation decoding and final finite request validation |
| Wi-Fi, Bluetooth and Accounts sections in `settings_app.rs` | Inventory tabs, filtering, selection, sharing choices, name drafts, forget/remove review, sync UI | Typed observations, opaque targets, permission/owner and live-device checks |
| `settings_datetime_ui.rs`, `settings_display_ui.rs`, network UI | Date/DNS/time drafts, validation feedback, explicit review, density and schedule options | Native civil-time/zone/capability validation and execution |
| `settings_sounds_ui.rs`, notification/history UI | Catalog pages, selection, preview decisions, Save/Cancel, history navigation | Opaque catalog authority, preview lifecycle stop, protected notification data |
| DND UI/model | Policy option labels, native-rule drafts, day/time edits, review, retained rejected drafts | Native rule copies, protected unknown fields, one-use revision checks |
| Apps and per-app UI modules | Search/filter/detail navigation, raw permission paging, app policy/cache/data/language reviews and picker state | App identity, fresh capabilities, finite operation and async-completion checks |
| Roles and runtime-permission UI | Group/candidate paging, selected labels, native-consent launch decision, return refresh | Platform consent, authority, permission model and no-replay execution |
| System language, keyboard and caption UI modules | Hierarchical browsing, ordered-language drafts, provider selection, palette/locale paging and custom preview | Native catalog/IME authority, scope leases and protected provider entry |
| `settings_accessibility_ui.rs` | Page titles, contextual labels and stable semantic row target strings | Physical bounds, virtual-node IDs, stale-action rejection and queue acknowledgment |
| `settings_entry.rs` | Validated entry to page, transient-state retirement and Back parent | Intent validation, finite route decoding, latest-entry correlation |
| `settings_host.rs` and domain hosts | Page-specific read intent/subscription, user-facing outcome copy and controller result events | Bounded polling mechanism, request/response correlation, foreground gate and service lifecycle |

The script implementations include model-adjacent behavior such as nearest offered preset, formatting, draft comparison and list ordering. Native model types retain strict wire decoders and backend authority checks. The former 23 UI/controller-test files were retired; their application decisions no longer form an alternate native event path. The native search catalog is retained only under `cfg(test)` as a parity oracle.

## Current controller files

`core.octoscript`, `navigation.octoscript` and `basics.octoscript` contain executable application logic: persistent core state, local theme drafts, page metadata, local search, simple display/sound candidate selection and display formatting. The catalog contains 73 reviewed entries; search pages contain at most 20 rows. Domain controllers provide review cancellation and selected detail parents; `subscriptions.octoscript` exports finite desired reads and selected host context without executing an effect on every refresh.

`controls.octoscript` ports ordinary Controls and ControlChoices, including all ten automatic-click choices, custom observed values, native choice bounds, nearest threshold/balance steps, and notification-history disable review. Six bundled VM traces pass against serialized native snapshots; they cover stale pressed targets and capability retirement as well as normal choices. The request decoder independently checks each offered native option.

`consent.octoscript`, `languages.octoscript`, `system_languages.octoscript`, and `keyboards.octoscript` now contain the script implementations for roles, runtime permissions, app/caption language catalogs, ordered system-language reviews and keyboard inventory/native-consent returns. Ten dedicated behavior traces pass through the finite native request decoder and host-subscription validator (roles, permissions, app/caption/system languages, keyboards, and malformed/stale binding rejection). The ordered-language traces verify complete Unicode locale tags, explicit review/cancel, the last-language rule and observation-confirmed completion. The mounted adapter now calls these sources. They are not a device acceptance claim. CaptionCustom’s scoped palette/preview controller and the other domain controllers are included in the same bundle.

The original standalone ARM64 development APK is `out/home/settings-octoscript-port/OctoSenseHome.apk`, version code `2026092540`. Its adjacent build receipt records the APK/signing/source hashes and validation logs. It predates the rebase and does not validate the rebased source. It has not been installed: the current session sandbox denies ADB's local listener on port 5037. No emulator parity claim is made and the OnePlus 6 was not touched.

## Runtime requirements and observed limitations

The old layout path prepares a design tree and discards its evaluation VM. It cannot preserve application state across input and observation events. The new controller VM therefore persists compiled functions while importing state and observations afresh for every transition.

The script VM supports records, arrays, function calls, bounded evaluation, string search and codepoint conversion. String length counts UTF-8 bytes, so user-query limits must use codepoints where the existing contract does. Unicode lowercase and character classification are provided as pure functions. Wide native IDs are decimal strings; converting them through floating-point numbers would corrupt correlation. Native floats preserve their exact wire strings.

A VM compiler edge was reproduced in a conditional branch ending with a `for` or `while` statement: an enclosing expression expected a result that the loop did not leave on the stack. The controller now ends those branches with explicit `nil`; a small persistent-call regression covers both loops, and the full Controls traces pass. This is a source-level compatibility rule, not a relaxed runtime error policy.

State and every patch/request must validate together before commit. Script failure or native rejection discards the transition and VM mutations without replay. No Android services, arbitrary intent calls, file/network access, or general widget modules are installed in the controller VM. Theme changes and observation refreshes must patch the existing widgets rather than remount editors or reset application drafts.

## Acceptance sequence

1. Execute script traces against real serialized snapshots for each ported domain, including stale press/review, policy revocation, result rejection, paging and lifecycle return.
2. Keep the native binding decoder exhaustive and reject unknown fields, targets and operation types; run existing authority tests unchanged.
3. Replace Rust application state and event decisions only after equivalent script traces exist. Remove the retired Rust path rather than leaving it as hidden behavior behind script forwarding.
4. Run the whole Home suite and the relevant ordinary/privileged emulator flows after the complete adapter cutover. Preserve existing cold-entry, IME, focus, accessibility, locale and density regression gates.

All validation remains emulator-only. Existing native feature coverage and remaining native-only Settings areas are tracked separately from this implementation-language migration.


## Mechanical host facade for cutover

The current hosts query `SettingsView` for subscriptions and active targets. These are application decisions and must come from script state. The script-owned `state.subscriptions` map holds the same finite read records already accepted by the binding decoder. `settings_script_host_facade.rs` retains the existing host method names as mechanical typed accessors so correlation, foreground gating and polling code remain stable:

| Existing facade | Script-owned value / native responsibility |
| --- | --- |
| `apps_read`, each domain `*_read`, `controls_page` | Decode the corresponding subscription record. Preserve root/detail targets, filter, page, parent and catalog key exactly. DND may subscribe to both DND detail and finite mode controls. |
| `wifi_visible`, `bluetooth_visible`, `display_visible`, `network_visible`, `updates_visible`, `date_time_visible`, history visibility | Subscription presence or explicit script active-domain data; no duplicate Rust navigation decision. |
| Wi-Fi/Bluetooth selected target, account context permits | Script-selected semantic identity; native rechecks against the current observed target and allowed operation. |
| `caption_visit`, `renew_caption_visit` | Current script scope observation and a lifecycle event. Native owns session/death identity and generation correlation; script owns page/visit transitions. |
| `observe`, `outcome`, entry/entry-resolved | Deliver typed data events to the controller, validate the whole transition, apply patches to the existing widget tree. |
| `*_after_selection`, `keyboards_after_flow`, DND operation result | Deliver a correlated native-accepted result event. The script retires one-use choices, preserves pending drafts and determines observed completion. |
| accessibility layout/actions | Use script semantic/label patches and mounted-widget geometry/enabled state. Native keeps node IDs, clipping, focus/IME mechanics, bounded asynchronous acknowledgment and stale-node checks. |

Input `return` is a distinct data event, not text concatenated into code. Back-stack scroll positions come from native geometry observations, while the script chooses when to capture/restore them. Snapshot/request IDs remain decimal strings. Existing native gates continue checking fresh app details, owner/focus, catalog generations and capabilities after the script's local guards.

## Mounted adapter regression status

`accessibility.octoscript` supplies the stable pane/detail key, current pane title, static contextual labels, and complete reviewed-request semantic identities. Catalog polling/filter changes retain the pane token; changed row or one-use request identities allocate new native node IDs. `settings_script_accessibility.rs` only traverses actual rendered bounds, clips children to their real scroll parent, applies script labels/enabled state, and dispatches ordinary widget events. It contains no Settings page or domain matching.

All ten mounted-view behavior regressions pass after cutover: editor/widget identity across live theme changes, native IME dismissal and accessibility reopen, physical Clear touch/cancel focus continuity and next character, narrow 150% geometry/clipping, recycled app-node denial, held caption-button invalidation across lease replacement and capability retirement, actual Search scroll restoration, physical swipe handoff without activating a button, Apps filter/scroll/read preservation on Back, and all 31 public entry routes with absent optional services and bounded parent chains. The original module-identity authorization regression also passes unchanged: claimed module IDs/capability metadata, child widgets, and retired roots cannot dispatch Settings requests. The route test first exposed caption ARGB arithmetic rounding; the corrected script passed the same test. These are host tests, including synthetic native touch events, not Android device acceptance.

Original port validation commands (before the main rebase):

```sh
cd home
cargo test --offline --locked --bin octosense --features mobile-apps -- --test-threads=1
# 495 passed, 0 failed
python3 -m unittest discover -s ../tests -p 'test_*settings*.py'
# 31 passed; requires JAVA_HOME and PATH pointing to the installed JDK
python3 ../scripts/setup-home.py --check
# Runtime source/patch checks passed
```

The artifact's `build.json` retains the original 494-test pre-package receipt. `validation/latest.json` records the subsequent 495-test run after restoring the authorization regression in a `cfg(test)` module, together with its test-file hash. No production source changed between that original packaging and the 495-test run. The original frozen build-source hashes remain in `source-sha256.json`.

PR preparation also ran the full repository suite, `python3 -m unittest discover -s tests -v`, with the installed JDK: all 82 checks passed. Staging-shell syntax and pinned runtime-source verification passed. That revision serialized the VM tests to avoid production wall-clock cutoffs; the review follow-up removes those cutoffs and restores parallel tests.

## PR #21 rebase

The two proposed commits now apply above main `49f57b9`. Conflict resolution retains main's App Hub and `hub:` launcher identities, compact Recents layout, and dependency pins. Settings entry intents use the current module-launch API with compiled Settings catalog metadata and the trusted singleton. The product runtime patch was regenerated against Makepad `1d3d383`; the framework remains at main's `c4c9682` and both AppCard checkouts remain at `9e8e489`.

The generated AIDL compiler-command header was removed from the rewritten Settings commit. Both proposed commit trees pass a machine-local-path scan, and main's existing tracked-file path guard passes. Rebase validation has passed the locked workspace check, the single-runtime Cargo graph check, all 84 repository checks, and the 672-test Home/App Hub/app-policy/News/AppCard command (including 501 Home tests). The runtime policy suites also passed (8 widget-policy tests and 3 script-policy tests). Environment limitations are recorded below and with the PR update; earlier APK and test receipts remain historical evidence for their recorded source.

A generic read-only `View::scroll_pos`/`ViewRef::scroll_pos` exposes the scrollbar position already used when drawing. This replaces the former AppLanguage geometry approximation; script state owns when that observation is bookmarked and restored. Gesture cancellation is a pure `cancel_gesture` event that clears held targets without pausing services or stopping sound previews.

The Maps workflow command passed 64 tests; nine existing TLS/network tests failed when their local server attempted `TcpListener::bind` (`PermissionDenied` / `Operation not permitted`). The workflow's existing 33 filtered view/module tests are unchanged. No network test was disabled or altered. An unrestricted run remains required; this session result is not a green CI claim.

## PR #21 review follow-up

Settings transitions now use deterministic instruction limits, plus the existing heap, stack, frame and data limits. No blocking or external operation is installed in this controller VM. Scheduler delays no longer abort a valid transition; malformed output and exhausted instruction budgets still discard the whole transition without issuing effects. Commands are not automatically retried.

The ROM permission allowlist explicitly covers Home's night mode and the broker's secure settings/configuration permissions on `system_ext`. Agent-to-broker, PermissionController and SystemUI connections bind on first use, release after 30 seconds without a request, and retry only in response to a subsequent request. Shutdown, null binding and binding death also release the connection. This bounds the broker's idle lifetime without replaying commands.

Home hides non-system overlay windows on Android 12+, and its Activity rejects fully or partially obscured gestures before either native views or the renderer receives input. If an overlay appears during a gesture, the Activity sends cancellation and rejects the rest of the gesture. The Android renderer translates cancellation into a non-activating release and clears capture; internal drag cancellation does not issue a drop.

CI regenerates the Home Binder client with Android build-tools 35.0.0/platform 35 and compares the complete Java output with the checked-in file. The compiler invocation header is stripped before comparison to keep local paths out of source. Run `python3 scripts/generate-agent-aidl.py --check --sdk "$ANDROID_HOME"` locally.

The earlier architecture record is now ADR 0006, leaving ADR 0004 available for PR #18. The obsolete empty `appstore` entry remains hidden at the user's request; the actual App Hub stays visible.

PermissionController accepts identical repeat staging, but an older generated adapter can differ from both upstream and the next version's expected bytes. The build-host `stage-forks.sh` wrapper now resets the integration's manifest, build file and OctoSense adapter/contract directories before staging the new version, as it already does for Quickstep and SystemUI. Local edits under those integration-owned paths are replaced. Unrelated Permission files are preserved and still block staging when dirty. An integration regression invokes the real wrapper and Permission stager in temporary Git repositories, covering identical re-runs, changed adapter/manifest sources, obsolete adapters, and preservation of unrelated tracked and untracked edits.

Review validation passes the locked workspace compilation and single-runtime graph check, 673 application tests (including 502 Home tests), 73 Maps tests and 11 runtime-policy tests. The nine previously blocked Maps network fixtures now pass with unrestricted local socket access. Parallel controller tests include a 120 ms simulated descheduling pause and instruction-exhaustion rollback.

The repository run exercised 86 checks: 85 passed initially, and the Android batch harness needed the new cancellation message shape. Its correction preserves the existing IME ordering checks and adds cancellation as an ordering barrier; all six tests in that suite passed on rerun. The new lifecycle tests cover lazy startup, idle renewal/expiry, failed binds, binding death, closure and obscured gestures. All 12 Agent adapter clients also compile against regenerated AIDL and Android SDK 35. The generated Home AIDL comparison passes.

Standalone ARM64 release validation APK `2026092602` passes `settings_input_safety` on the dedicated Android API 35 emulator. The instrumentation drives the actual Activity dispatch and a native child button: a clean tap clicks, fully obscured input does not, a partial overlay appearing after press cancels the gesture, and a subsequent clean tap works. This is input-protection acceptance, not full Settings or privileged ROM acceptance. The emulator's previous Home APK is restored after the test. No physical-phone operation was performed. A platform-signed ROM build/boot and full ordinary/privileged Settings acceptance remain pending.
