# Camera

English | [简体中文](README.zh-CN.md)

The bundled `os.camera` app uses Makepad's native `CameraPreview` widget.
Preview frames stay native; `on_capture` returns the app-relative path of a
completed capture. The shell supplies the app's storage and device identity.
Camera and microphone access still require the app's host consent and the
platform's OS permission. Background callbacks cannot gain permission-prompt
authority by completing after the app returns to the foreground.

The manifest requires `host-api-v1` and `camera.capture_intent@1`; older
hosts refuse this bundle instead of ignoring capture options. The read-only
`camera.capture_intent` method reports the defaults and supported options.
The app requests host/OS camera permission before opening the preview and
microphone permission before recording with audio. The shell's unified device
permission adapter currently supports Android and macOS; other targets report
unsupported rather than bypassing consent.

## Capture intent

Capability declarations describe expected API usage; making an API available
does not select recording or export behavior. Each call chooses its options:

```text
ui.cam.capture()                         // local JPEG; no library export
ui.cam.capture({library: false})         // same behavior, explicit
ui.cam.record_start()                    // local silent video, if supported
ui.cam.record_start({audio: true library: false})
ui.cam.record_stop()
```

Both options default to `false`. Only boolean values are accepted; strings,
numbers, unknown fields and extra arguments return `false` and report an
explanation through `error()` / `on_error`. `audio` is a recording option,
not a still-photo option. Requested audio is refused when microphone consent
is missing; it does not silently become a silent recording. The options are
retained while asynchronous OS permission checks run, alongside the request's
original foreground/background authority.

`library: true` explicitly requests an additional gallery export. It requires
foreground authority and a backend that implements that export; unsupported
platforms reject it **before capture**. OpenHarmony has an existing ArkTS gallery
handoff. Android, Windows and Apple capture paths do not gain gallery export
from this change. Gallery handoff is not a delivery receipt.

The bundled app explicitly saves locally and requests audio for video mode.
Its recent-thumbnail list holds at most 30 paths. Trimming that list does not
delete capture files or assume a gallery copy exists. Existing storage quotas
still apply; a full or missing app storage directory causes a visible error.

## Platform limits and validation

This change does not add capture backends. Android supports its existing still
capture but reports video recording as unsupported. Windows has a bounded
still-capture worker and rejects recording. macOS lacks a camera recording
adapter. OpenHarmony uses its existing capture/recording and gallery bridge.
Exact platform support must be checked on the target device.

The runtime patch is
[`makepad-camera-capture-intent.patch`](../../tools/runtime-patches/makepad-camera-capture-intent.patch).
It extends existing CameraPreview tests with strict option parsing, defaults,
missing microphone consent, background export refusal, pending intent retention,
and unsupported export before file creation. From the prepared runtime:

```sh
MAKEPAD_HIDE_WINDOWS=1 cargo test --locked -p makepad-widgets camera_preview::tests -- --test-threads=1
```

These are logic tests: they open no camera hardware and do not prove GPU
rendering, OS dialogs, audio recording or gallery delivery. Live device
acceptance remains separate. See the [OS API status](../../docs/host-os-api-status.md).
