# OctoSense Android launcher plan

Date: **15 September 2026**. Status: **proposed implementation plan; launcher integration is not yet implemented or validated**.

## Decision

Build OctoSense as a **default Android Home app that works without root**. Make the existing home interface useful for everyday access to installed Android apps and OctoSense cards. Keep root and custom-ROM integration optional, driven by specific features that public Android APIs cannot support.

The first release should install as an ordinary APK and let the user select OctoSense as their default home app. Becoming Home must not depend on Magisk, a modified system image, or special permissions granted through ADB.

Rendering performance remains a separate requirement. Follow the [Android performance plan](performance-plan.md); launcher privileges do not automatically reduce shader cost, improve frame pacing, or eliminate startup stalls.

## 1. What Home, root, and a custom ROM mean

| Mechanism | Purpose | Implication for OctoSense |
|---|---|---|
| Home role | Selects the app that handles Android's Home intent | Provides the supported entry point for replacing the home screen |
| Root, for example through Magisk | Gives approved processes elevated access and supports system modifications | Can help prototype deeper integration; does not implement launcher or system UI behavior |
| Custom ROM | Packages an Android OS build with selected apps, services, and system configuration | Can ship OctoSense and its required system integration by default |

Android defines a Home intent filter using `MAIN`, `HOME`, and `DEFAULT`, and supports requesting the Home role through `RoleManager`. Home selection does not grant arbitrary system permissions. [Home role requirements](https://developer.android.com/reference/androidx/core/role/RoleManagerCompat#ROLE_HOME), [RoleManager](https://developer.android.com/reference/android/app/role/RoleManager).

Magisk is a toolkit providing root access, modules, and other Android customization facilities; it is not itself a ROM. An ordinary launcher on a rooted phone still has ordinary app privileges unless it explicitly uses additional integration. [Magisk](https://github.com/topjohnwu/Magisk).

### What Magisk would contribute

- **MagiskSU:** lets approved applications execute operations with root privileges.
- **Modules:** package system modifications, configuration, and startup scripts. Magisk can mount replacement files over system paths without rewriting the underlying system partition; this is commonly called "systemless" modification.
- **Zygisk:** an optional facility for running module code inside Android app processes.

These are tools for implementing and deploying modifications. They do not supply an Android app compositor, replace SystemUI automatically, or improve Makepad frame pacing by themselves. For OctoSense, a module could deliver an experimental privileged helper and its configuration after those components are implemented. [Magisk features](https://github.com/topjohnwu/Magisk), [module developer guide](https://topjohnwu.github.io/Magisk/guides.html).

A future OctoSense ROM could supply the necessary privileged components without giving user apps general root access. Required permissions must be configured appropriately; putting an APK in a system directory does not grant every capability. [AOSP privileged permissions](https://source.android.com/docs/core/permissions/perms-allowlist).

## 2. Product scope

| Capability | Initial launcher | Possible later extension |
|---|---|---|
| Default Home and reliable return from Android apps | Required | — |
| Installed app list, icons, launch, folders, and search | Required | More advanced profile and shortcut support |
| OctoSense cards alongside Android app shortcuts | Required | Richer card integrations |
| Android widgets | Separate root-free milestone | Native widget hosting and configuration |
| Notifications displayed inside OctoSense | Optional root-free milestone | Requires separately granted notification access |
| System-wide Recents, task previews, and gesture transitions | Android continues to provide these | Evaluate privileged integration if essential |
| System notification shade, quick settings, and lock screen | Android continues to provide these | Separate SystemUI project |

Root-free does not mean every capability comes from the Home role. Android exposes separate APIs for [app discovery and launching](https://developer.android.com/reference/android/content/pm/LauncherApps), [widget hosting](https://developer.android.com/reference/android/appwidget/AppWidgetHost), and [notification listeners](https://developer.android.com/reference/android/service/notification/NotificationListenerService). Each integration needs its own implementation and any applicable user consent.

OctoSense's hosted cards and internal app switcher are distinct from Android application tasks. Launching an installed Android app should open its normal Android activity. The initial launcher does not embed arbitrary Android apps into Makepad tiles or replace Android's system task switcher.

## 3. Current starting point

The inspected [Android manifest template](../../OctoSense-native-perf/resources/android/AndroidManifest.xml.template) declares an ordinary app-drawer entry and explicitly leaves Home handling to the device launcher. It also handles AppCard share and deep-link intents. The phone interface already supplies home layouts, cards, and internal navigation, but these do not establish Android Home or system-task integration.

Treat those existing interfaces as reusable UI. Add an Android integration layer that supplies installed-app data, launch actions, package changes, and lifecycle events to the Rust/Makepad interface. Keep system-specific calls behind that boundary so existing hosted cards remain usable.

### Native Android app composition: inspected capability

**Conclusion:** OctoSense currently composes bundled Makepad modules. The inspected implementation does not host arbitrary installed Android apps as live, interactive panels.

Here, composition means displaying and interacting with an Android app inside an OctoSense-controlled panel, potentially alongside another app. Opening an app's normal Android activity and returning Home is a separate, smaller launcher feature.

This is a **source inspection dated 15 September 2026**, using the local `OctoSense-native-perf` worktree and its Makepad dependency at `73dfc62a8358c7a6cce33e50a612ced28e5c7411`. Native Android app embedding has not been demonstrated on the phone. These findings describe the inspected revision, not every Makepad version or a fundamental rendering limitation.

| Source evidence | What it establishes |
|---|---|
| [App registry](../../OctoSense-native-perf/src/apps.rs) defines `Hosting::Module` and `Hosting::Process` | Existing hosting supports linked Makepad modules and the cooperating desktop process protocol |
| [Platform hosting gate](../../OctoSense-native-perf/src/host.rs) disables `processes_available()` on Android and iOS | The desktop child-app hosting path is not available on the phone |
| [Module frame capture](../../OctoSense-native-perf/src/dock_warp.rs) records drawing into a `WindowFrame` texture | Current app previews and effects operate on content rendered through Makepad |
| Makepad's `VideoPlayer.java` uses `SurfaceTexture`; its Android camera code supports hardware-buffer textures | Android media texture interoperation exists, but it does not establish access to other apps' windows |
| Searches of OctoSense and the inspected Makepad platform, widgets, and Android Java sources found no task-hosting implementation using `TaskView`, `TaskOrganizer`, `ActivityView`, `VirtualDisplay`, or `SurfaceControlViewHost` | A native Android app-hosting layer would be new integration work |

The phone's existing Reference, Sheets, Photos, and AppCard modules are linked into the OctoSense APK. Their successful embedding does not demonstrate embedding their separately installed APKs or unrelated Android apps. [Current Android packaging](../../OctoSense-native-perf/README.md).

### Feasible integration paths

| Desired behavior | Path and boundary |
|---|---|
| Open an installed Android app normally | Public launcher APIs; part of the initial root-free plan |
| Embed an app that cooperates | Cross-app activity embedding on supported devices; the target activity must opt in and platform layout restrictions apply |
| Display other Android tasks in OctoSense-controlled panels | Investigate an Android system component using task-hosting APIs; Home alone does not grant the required control |
| Apply Makepad blur or warp shaders to live Android app content | Requires a separately validated graphics-sharing or capture path; displaying a task surface does not automatically expose it as a Makepad texture |

Android's public cross-app activity embedding requires target-app consent through manifest configuration, such as trusted host certificates or permission for untrusted embedding. It is not a general mechanism to place every installed app into a launcher tile. [Android activity embedding](https://developer.android.com/develop/ui/views/layout/activity-embedding#cross-app_embedding).

AOSP's `TaskView` is a `SurfaceView` that displays a task; its controller attaches the task's surface to the host surface and manages visibility and bounds. The underlying task-organizer APIs require protected capabilities such as `MANAGE_ACTIVITY_TASKS`. These sources establish a possible system integration route, not compatibility with the OnePlus ROM or every target app. [AOSP TaskView](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/libs/WindowManager/Shell/src/com/android/wm/shell/taskview/TaskView.java), [task surface controller](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/libs/WindowManager/Shell/src/com/android/wm/shell/taskview/TaskViewTaskController.java), [TaskOrganizer](https://android.googlesource.com/platform/frameworks/base/+/master/core/java/android/window/TaskOrganizer.java).

### Architecture to evaluate for live panels

This is a proposal, not an implemented or benchmarked design:

1. Makepad renders the OctoSense interface and determines panel geometry.
2. An Android integration component hosts native app tasks and coordinates their bounds, visibility, focus, keyboard, and lifecycle.
3. Android's display compositor, SurfaceFlinger, combines the Makepad surface with the hosted app surfaces.
4. Effects that need another app's actual pixels receive a separate feasibility and performance evaluation. Do not assume task hosting gives unrestricted texture access or that every app/content type supports capture.

The first experiment should validate one representative app in one interactive panel, including input, keyboard, resize, pause/resume, and task exit. Only then evaluate multiple apps, synchronized transitions, additional effects, and sustained resource use. Broad task hosting would need a ROM-specific privileged component or equivalent authorized system integration; merely installing Magisk would not implement it.

**Scope of earlier effort estimates:** the estimated 1–2 weeks for an everyday launcher on the OnePlus covers opening Android apps normally and returning Home. It excludes native app composition, system Recents replacement, and the outstanding rendering work. Live composition is a separate project whose schedule should follow the first working integration experiment.

## 4. Implementation sequence

### Phase 1 — A functional default Home

1. Add a Home-capable activity or intent route with the required intent filter. Request the Home role through Android's supported selection flow, checking availability and current selection.
2. Distinguish Home requests from share and deep-link requests. Returning Home should reach the intended home state without breaking existing AppCard routing.
3. Discover launchable Android activities through `LauncherApps` and related public APIs. Track app identity by component and user/profile, rather than display name alone.
4. Populate the app drawer and search with real names and icons; launch selected apps through Android. Load and cache icons without blocking interaction.
5. Observe installation, removal, and package changes. Handle unavailable apps without leaving broken navigation or crashing the launcher.
6. Persist home layout, folders, and user choices. Handle Home requests, Back, activity recreation, and process restart consistently.
7. Let users combine Android app shortcuts and OctoSense cards while keeping their launch behavior clear.

**Acceptance:** the user can select OctoSense as Home, open representative installed apps, return Home, reboot and unlock, and retain their layout. Removing or updating an app refreshes the launcher. The same functions work on an unrooted device without ADB-granted privileges.

**Phase 1 status, 16 September 2026:** implemented in `OctoSense-mobile` PR #3 (HOME/DEFAULT filter; `Event::HomeIntent` from the makepad fork, `a359e165c`, with the Java forward on the fork's `feat/android-home-intent-java`). On the OnePlus 6 OctoSense is now the default Home (`cmd package set-home-activity`), Home presses from Settings and with the sheet open reach the shell in gesture and 3-button navigation, and the phone comes up in OctoSense after a reboot. One gap: in gesture navigation the first Home press after selection, with an OctoSense-hosted app open, delivered no intent (later presses did; 3-button always did). Installed-app discovery, launching and persistence (steps 3–7) remain to do. The phone was switched to 3-button navigation, which removes the bottom gesture-zone race; revert with `cmd overlay enable-exclusive --category com.android.internal.systemui.navbar.gestural`.

**Home-page pulls, 16 September 2026 (`OctoSense-mobile` PR #4, stacked on #3):** a pull down in the middle of the home page opens the App Library with its search field focused, in the right quarter the shade's controls, in the left quarter its notifications; a pull down on the library closes it. This replaces reaching for the top edge or the bottom band, which races the system's gesture zone in gesture navigation. Verified on the OnePlus 6 with screenshots (`op6-pulls-*.png`).

### Phase 2 — Everyday reliability and performance

- Cover cold start, repeated app-to-Home navigation, keyboard dismissal, rotation, display insets, and light/dark appearance.
- Verify screen-reader navigation, focus order, accessible labels, and font scaling for essential launcher actions.
- Handle supported user/profile states correctly and document any unsupported profile features.
- Measure idle CPU activity, memory stability, icon-loading behavior, and recovery after Android kills the launcher process.
- Apply the performance plan's repeated measurements to launcher-owned animations. Measure external app startup and system-controlled transitions separately; do not attribute all of their time to OctoSense rendering.
- Verify users can change their default Home through Android Settings.

**Acceptance:** core launcher flows remain usable through lifecycle changes, with stable idle resource use and documented performance on the exact tested build. A successful run on a rooted development phone alone does not prove root-free compatibility.

### Phase 3 — Optional root-free integrations

- **Widgets:** evaluate native Android widget-view hosting alongside the Makepad surface. Validate sizing, input, configuration, updates, and resource use before promising broad widget compatibility.
- **Notifications:** offer notification access only when the user enables notification cards. Respect platform filtering and permission limits; disabling access should leave the launcher functional.
- **Shortcuts and profiles:** expand supported Android launcher features based on actual usage and the relevant public APIs.

**Acceptance:** each feature works without root, handles permission denial or revocation where applicable, and does not regress Home responsiveness. Notification cards inside OctoSense do not imply replacement of Android's system shade.

### Phase 4 — Optional system integration experiments

Start only when a concrete product requirement is blocked by public APIs. Candidate requirements include system-wide Recents with Android task previews, full control of app-to-Home animations, or an OctoSense system shade.

Android's Quickstep implementation uses protected task, input, and transition capabilities. SystemUI owns interfaces such as the status bar, notification shade, and keyguard. These require integration with the target Android version; selecting Home does not transfer ownership. [AOSP Quickstep](https://android.googlesource.com/platform/packages/apps/Launcher3/+/master/quickstep/AndroidManifest.xml), [AOSP SystemUI](https://android.googlesource.com/platform/frameworks/base/+/master/packages/SystemUI/README.md).

For each proposed experiment:

1. Identify the missing user-visible behavior and the public-API limitation.
2. Identify the exact Android/ROM version, required services, permissions, and integration points.
3. Evaluate a Magisk module or a ROM build as the delivery mechanism. Keep experimental privileges outside the root-free launcher's essential path.
4. Compare functionality, performance, and maintenance cost against the root-free experience.
5. Retain the extension only if it delivers enough value to justify ongoing Android-version and device compatibility work.

The rooted OnePlus can serve as an optional development device. Experiments should be scheduled separately from rendering benchmarks so system changes do not invalidate performance comparisons.

#### Magisk's role as OctoSense gains deeper system integration

**Use Magisk as an optional tool for experiments and advanced features on supported phones.** It provides a way to deploy and run modifications on an existing Android installation while the core launcher continues to work without root.

| Deeper feature | Potential Magisk contribution | Work still required |
|---|---|---|
| Live Android app panels | Deploy a privileged task-management helper and its configuration | Task hosting, surfaces, input, keyboard, lifecycle, and app compatibility |
| System Recents and gesture transitions | Install components or modifications matched to the target ROM | Coordination with Android's task, gesture, and transition services |
| OctoSense system shade and other system UI | Deploy ROM-compatible SystemUI modifications | Implement the interface and connect it to the existing system behavior |
| Performance investigation | Run diagnostic helpers that need elevated access | Identify and fix measured costs; root does not itself improve frame pacing |

These are proposed uses, not features supplied by Magisk. Its root and module facilities do not automatically grant the regular OctoSense app every protected Android capability or make a modification work across ROM versions. [Magisk](https://github.com/topjohnwu/Magisk), [module and startup-service guide](https://topjohnwu.github.io/Magisk/guides.html).

**Proposed component boundary:**

1. **OctoSense app:** Makepad UI, cards, layouts, and public-API launcher functions, running with ordinary app permissions.
2. **Optional privileged helper:** a separate component exposing specific system operations through a defined Android interface. Keep task management and other privileged work out of the UI process.
3. **Magisk module:** packages, configures, and starts the helper or installs the required system modifications for explicitly supported Android/ROM versions.

The app should check which extension capabilities are available. If the helper is absent, disabled, or incompatible, normal Home and app launching must remain functional. Validate helper startup/restart and failure behavior alongside the feature's task and graphics behavior before treating it as usable.

**Later ROM path:** a complete OctoSense ROM is a separate product decision requiring a reproducible device build and ongoing system integration and update work. It could package the privileged components directly with the necessary signing, permissions, and system configuration, so Magisk would no longer be a required delivery mechanism. General root access for user apps is not a prerequisite for that design. [AOSP privileged permissions](https://source.android.com/docs/core/permissions/perms-allowlist).

Prefer reusing Android's notification, authentication, and device-control logic when adding OctoSense presentation. Magisk is an enabler for developing deeper integration, not a required permanent dependency of OctoSense.

### Phase 4 probe on the OnePlus 6 — 16 September 2026

Read-only inspection of the development phone (LineageOS 22.2, `ro.build.tags=release-keys`, `userdebug`), to size the privileged route before any system change:

- The gesture-navigation launcher is `com.android.launcher3/.uioverrides.QuickstepLauncher` from `/system_ext/priv-app/TrebuchetQuickStep`, holding `STATUS_BAR_SERVICE`, `MANAGE_ACTIVITY_TASKS`, `MONITOR_INPUT` and `START_TASKS_FROM_RECENTS` (all `signature|privileged`, granted). SystemUI binds to its `com.android.quickstep.TouchInteractionService` (protected by `STATUS_BAR_SERVICE`) for the bottom gesture zone, Recents and the home/app transitions.
- The build is signed with Lineage's private keys, so OctoSense cannot obtain `signature` permissions; the `privileged` alternative is open: a Magisk module could mount OctoSense under `/system_ext/priv-app` with a `privapp-permissions` allowlist, and a runtime resource overlay could point `config_recentsComponentName` at an OctoSense service.
- What that buys is only the binding: the service then has to implement Quickstep's `IOverviewProxy` contract (gesture ownership, Recents, transition animations, task and input coordination). That is a re-implementation of Launcher3's Quickstep for this ROM, not an integration hook, and it breaks with each Android/ROM version.

Decision: the Home role and 3-button navigation (Phase 1 + a device setting) resolve the launcher conflict for everyday use; the privileged route is only worth starting for a product requirement that needs the system's gesture zone or Recents to be OctoSense's, and its first experiment should be the smallest binding that can be measured (the Recents component alone), on a spare device, with the Magisk module kept reversible. No system modification was made.

## 5. Deliverables and decision gates

1. **Root-free launcher APK:** Home registration, installed-app discovery and launching, persistent layouts, and reliable lifecycle behavior.
2. **Validation record:** exact build and Android/device details; functional, accessibility, resource, and rendering results; remaining limitations.
3. **Optional integration decisions:** separate scope and acceptance evidence for widgets, notifications, and other public-API features.
4. **System-extension proposal, only if needed:** a specific blocked feature, target ROM, required changes, measured benefit, and maintenance implications.

Success for the first product is a useful everyday OctoSense launcher on ordinary Android. Root and a custom ROM remain optional ways to pursue additional capabilities after that foundation works.
