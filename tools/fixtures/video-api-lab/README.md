# Video API acceptance

English | [简体中文](README.zh-CN.md)

This fixture uses a five-second, silent H.264 clip of three color bars and a
moving white marker. It contains no personal media or external footage.
`tools/generate-video-fixture.swift` generated it with the installed macOS SDK.
The generator refuses to overwrite its output.

The unreleased runtime exposes the existing native `Video` player to Splash:

| Method | Result / argument |
| --- | --- |
| `prepare_playback()` / `begin_playback()` | Request preparation / playback |
| `pause_playback()` / `resume_playback()` | Control the same player |
| `stop_and_cleanup_resources()` | Request native resource cleanup |
| `mute_playback()` / `unmute_playback()` | Change the player's mute state |
| `seek_to(milliseconds)` | Nonnegative position, bounded by known duration |
| `set_volume(value)` | Finite value, clamped to 0–1 |
| `set_playback_rate(value)` | Finite value in 0.25–4; backend may restrict rates |
| `current_position_ms()` / `total_duration_ms()` | Position estimate / duration |
| `state()` / `error()` / `is_muted()` | State string / last error / boolean |

Commands acknowledge dispatch; preparation and decoding are asynchronous.
Numeric controls return `false` for invalid inputs or an unprepared player.
Read `state()` and `error()` to observe completion. Seeking is not frame-accurate:
a backend may select a nearby frame, and `current_position_ms()` can briefly
report the requested target before subsequent native frames update it. Use `CameraPreview` for a
camera: directly constructing `VideoDataSource.Camera` in a contained app is
refused because it has no correlated permission approval.

File sources remain inside the app's storage jail. Closing an isolate or
dropping its Video widget retires the native player through the UI event pump.
An OS pause/resume cycle preserves a manual pause. Merely hiding a view is not
the same as dropping it; explicitly stop playback when retaining a hidden view.

**macOS local-MP4 acceptance passed:** all 16 native instrument checks passed on
runtime `fc938badf`, including decoding/rendering, pause, native forward/backward
seek, stop/restart and teardown. The Metal captures were reviewed; the player
reported both native releases and no frames after the final release.
[Sanitized receipt](evidence/macos-local-mp4.json). This is the separate contained
fixture, not signed App Hub admission.

**OnePlus 6 / Android 15 local-MP4 acceptance passed:** all 20 checks passed on
runtime `7c859055`. A bounded app-private command lane invokes the contained
Splash buttons' original callbacks; native decoder events verify their effects.
Forward seek to 2,000 ms resumed at 2,050 ms, rewind at 66 ms, and stop/restart
and app teardown released both native players without late frames. The driver
removed its isolated package. [Sanitized receipt](evidence/oneplus6-local-mp4.json).
This does not establish Android pixel rendering or physical-touch behavior.

Android API 26+ now uses `MediaPlayer.SEEK_CLOSEST`; the old overload selected the
previous sync frame and failed the phone's forward-seek check. Earlier Android
versions retain the legacy fallback and remain unverified. The Android-only Java
change does not extend the scope or runtime identity of the macOS receipt.
See Android's [MediaPlayer seek contract](https://developer.android.com/reference/android/media/MediaPlayer#seekTo(long,%20int)).

The fixture covers local MP4 only. It does not validate streaming protocols,
DRM, every codec, or network redirect/playlist policy. The current network
source checks the initial URL; nested playlist and redirect enforcement needs
separate work before it can be claimed to honor a contained app's full network
allowlist.
