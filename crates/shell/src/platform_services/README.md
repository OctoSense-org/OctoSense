# Device host APIs

English | [简体中文](README.zh-CN.md)

Installed apps can request camera, microphone and location authorization through the host. Manifest capabilities are the ceiling; per-app user consent and the operating system's package permission are separate. Consent applies to one app across its accounts, not to other apps. An OS grant to OctoSense does not authorize every installed app.

This first adapter implements permission status/request/revoke on **Android and macOS**, plus **Android last-known location**. Other platforms return an explicit unsupported result. A permission grant does not create a calendar, picker, camera-capture or background-location service. Camera capture remains the existing `CameraPreview` widget.

An app must declare `"requires": ["host-api-v1"]` and each capability it uses, such as `"capabilities": ["camera", "location"]`. The host applies the device consent gate **before evaluating app source**. App Hub can additionally check exact method versions in `host_api.required`; use API discovery for optional methods.

| Method | Arguments | Behavior |
| --- | --- | --- |
| `camera.permission.status` | `{}` | Read app consent and OS camera status; never prompt |
| `camera.permission.request` | `{}` | Foreground app opens a native consent sheet, followed by the OS prompt if needed |
| `camera.permission.revoke` | `{}` | Revoke this app's device consent and stop its running `CameraPreview`; the OS package grant remains unchanged |
| `microphone.permission.*` | `{}` | Same authorization operations for microphone access; revocation stops an active camera recording that uses audio |
| `location.permission.*` | `{}` | Same operations for location; Android requests start its existing foreground location feed after grant |
| `location.get` | `{}` | Android only: read a last-known fix after a fresh OS permission check |

A foreground interaction can call:

```text
host.request("camera.permission.request", {}, fn(r) {
    if r.is_ok && r.data.os_permission == "granted" {
        ui.preview.start()
    }
})
```

This is an API example, not an independently published sample bundle. The response distinguishes `app_policy_granted`, `app_consent`, and `os_permission`. OS states include `granted`, `not_determined`, `denied`, and `settings_required`. Existing app consent avoids another host consent sheet; an existing OS grant avoids another OS permission dialog. Apps should request permissions when a person chooses a feature, not at startup.

`location.get` returns `latitude`, `longitude`, `accuracy_m`, `source: "last_known"`, `timestamp: null`, and `freshness: "unknown"`. The existing GPS cache has no timestamp. Do not represent this result as a fresh fix or use it for safety-critical navigation. It does not grant background location or promise a location fix when providers are disabled.

Background/agent permission requests return `authorization_required`. Agents may read status, revoke the app's consent, and read an already-authorized last-known location. They cannot approve the native consent sheet. Approval requires trusted physical input; synthetic Makepad/ADB actions do not substitute for it. A contained app cannot mount a copy of the native widget to borrow that authority.

The service queues native operations onto the shell's UI event loop, matches permission callbacks by request ID, rechecks admitted app identity and the consent revision before answering, and drops cancelled requests. Revocation invalidates older pending approvals. Permission dialogs may pause Android; their already-started bounded OS request survives that pause, while other pending operations and unapproved consent sheets are cancelled. Denial does not erase another app's grant.

The runtime's `host-api-v1` gate also covers legacy `CameraPreview`, `sys.request_location`, `sys.gps` and map GPS reads for opted-in bundles. It uses host-assigned app identity and a cached consent lookup. On startup the cache is closed until the app calls a permission method to load its retained consent. Older bundles retain their previous manifest-only policy; they cannot call the new device service until they declare `host-api-v1`.

An agent or background card cannot use those legacy paths to raise an OS dialog. Camera preview and recording check the OS grant first, and only a request that began in the foreground and remains there may prompt. Existing OS grants remain usable; opted-in cameras never assume approval after a timeout. `sys.request_location` can prompt and therefore requires the foreground; background code can use passive `sys.gps` or `location.get` after authorization.

The private consent file is `.host/device-api-consent.json`, outside app storage. It contains app IDs, capability names and consent revisions, with no device readings or provider credentials. Unix files use mode 0600 and updates are atomic.

Validation is recorded with the implementation commit. Native Makepad policy and camera regression tests were run on macOS. Live permission dialogs, camera capture and this new service's location flow on OnePlus 6 remain **unverified** until the integrated host is installed and physically exercised. Windows/Linux/iOS support is not advertised by this adapter.
