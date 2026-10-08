# Device host APIs

English | [简体中文](README.zh-CN.md)

On OctoSense `main`, not yet in any release, an installed script app can ask the host for camera, microphone and location access. Three separate checks decide every call:

- **Capability.** The app's manifest declares `camera`, `microphone` or `location`. This is the most the app can ever get.
- **App consent.** The person allows this app on a native host sheet. Consent covers this app on all of its accounts, and no other app.
- **OS permission.** The operating system's permission for OctoSense itself. It does not authorize every installed app.

This module's device service (`DeviceService`) implements permission status, request and revoke on Android and macOS, and reading the last-known location on Android. On other platforms, API discovery (`runtime.list` and `runtime.describe`) does not list these methods: `status` answers `os_permission: "unsupported"`, and `request` and `location.get` fail with `unsupported_platform`. A permission adds no calendar, file picker, camera capture or background location service. To show the camera, use the existing `CameraPreview` widget.

## Declare what the app uses

Require `host-api-v1` and declare each capability the app uses. This manifest fragment asks for the camera and location:

```json
{
  "requires": ["host-api-v1"],
  "capabilities": ["camera", "location"]
}
```

The host applies device consent before it runs any of the app's source. To refuse hosts that lack a method, list the method and its exact version in `host_api.required`; App Hub then checks it at install and launch. For a method the app can do without, check at run time with `runtime.describe`, which needs the `runtime` capability.

## Methods

| Method | Arguments | What it does |
| --- | --- | --- |
| `camera.permission.status` | `{}` | Reads the app's consent and the OS camera permission. Never prompts. |
| `camera.permission.request` | `{}` | From the app in the foreground: shows the host's consent sheet, then the OS dialog if needed. |
| `camera.permission.revoke` | `{}` | Withdraws this app's consent and stops its running `CameraPreview`. The OS permission for OctoSense is unchanged. |
| `microphone.permission.*` | `{}` | The same three methods for the microphone. Revoking also stops a camera recording that captures sound. |
| `location.permission.*` | `{}` | The same three methods for location. On Android, a granted request also starts the host's existing foreground location updates. |
| `location.get` | `{}` | Android only: checks the OS permission again, then returns the last-known location. |

## Request access

Ask when the person chooses a feature that needs the device, not when the app starts:

```text
host.request("camera.permission.request", {}, fn(r) {
    if r.is_ok && r.data.os_permission == "granted" {
        ui.preview.start()
    }
})
```

This is an API example, not a published sample app. The answer reports the three checks separately: `app_policy_granted` (the capability), `app_consent` and `os_permission`. `os_permission` is `granted`, `not_determined`, `denied` or `settings_required` (denied for good; only the system settings can change it). If the app already has consent, the host skips its sheet; if OctoSense already has the OS permission, no OS dialog appears.

The consent sheet offers **Not now** and **Continue**, and it accepts only a physical press. Synthetic input from Makepad automation or ADB does not count, and an app cannot mount its own copy of the sheet's widget to approve itself. **Not now** ends the request with `cancelled`. A sheet left open for 5 minutes expires, and the request fails with `timeout`.

## Agents and background code

An agent, a background card (such as the app's card on the Glance screen), and the callbacks and timers they start cannot request permission. App Hub refuses the call first, with `<method> is unavailable to agents/background surfaces`, so neither the consent sheet nor the OS dialog opens. They can still call `status` and `revoke`, and `location.get` once the app has consent and the OS permission.

## Read the location

`location.get` returns `latitude`, `longitude`, `accuracy_m`, `source: "last_known"`, `timestamp: null` and `freshness: "unknown"`. The host's GPS cache has no timestamp, so the fix may be of any age: do not present it as current, and do not use it for safety-critical navigation. It grants no background location, and it cannot promise a fix when the device's location providers are off. Without a last-known fix, it fails with `location_unavailable`.

## Older device paths

For an app that requires `host-api-v1`, the same consent also gates the older paths: `CameraPreview`, `sys.request_location`, `sys.gps` and the map's GPS reads. The host supplies the app's identity and keeps consent in a cache. After the host starts, that cache denies everything until the app calls one of its permission methods, which loads the saved consent, so call `status` before using these paths.

An agent or a background card cannot use these paths to raise an OS dialog either. `CameraPreview` checks the OS permission before it previews or records, and only a request that starts in the foreground and is still there when the check returns may prompt. An existing OS permission keeps working, and a camera in an opted-in app never treats a timeout as approval. `sys.request_location` can prompt, so it needs the foreground; background code can read `sys.gps`, or call `location.get`, once the app has consent.

Apps that do not require `host-api-v1` keep the earlier manifest-only rules, and their calls to these methods fail with `host_requirement_missing`.

## Errors

| Error | When |
| --- | --- |
| `host_requirement_missing` | The manifest does not require `host-api-v1`. |
| `permission_denied` | The manifest lacks the capability, or the app was removed, lost the capability or had its consent changed while the request waited. |
| `invalid_arguments` | The arguments are not `{}`. |
| `authorization_required` | `location.get` ran without app consent or without the OS permission, or a `request` arrived when the host could not prompt, for example while it was in the background. |
| `unsupported_platform` | The platform has no adapter for `request`, or `location.get` ran outside Android. |
| `cancelled` | The person chose **Not now**, a newer request for the same capability replaced the sheet, or the host left the foreground. |
| `timeout` | The consent sheet stayed open for 5 minutes. |
| `busy` | 64 device requests, or 64 consent sheets, are already waiting. |
| `location_unavailable` | Android has no last-known location. |

## How the host runs a request

The device service queues each native operation on the shell's UI event loop and matches the OS result to its request ID. Before it answers, it checks again that the app is still installed with the capability and that its consent revision has not changed; a request cancelled in the meantime gets no answer. Revoking consent invalidates every older request still waiting for approval, and one app's denial never clears another app's consent.

On Android, the OS permission dialog can pause the activity. A request already waiting for that dialog survives the pause; every other queued request, and every consent sheet not yet approved, is cancelled.

Consent lives in `<apps root>/.host/device-api-consent.json`, outside every app's storage. The file holds app IDs, capability names, the consent flags and their revisions, and no device readings or provider credentials. On Unix it has mode 0600, and each update replaces it atomically.

## Verification

**Verified** on macOS (results recorded with the implementation change): the native Makepad policy tests and the camera regression tests. The [Host API Lab](../../../../tools/fixtures/host-api-lab/README.md) also reads the real camera permission status through this service.

**Unverified:** live permission dialogs, camera capture and the location flow on a OnePlus 6. They need the integrated host installed on the phone and a person pressing the controls.
