# Native audio sessions

English | [简体中文](README.zh-CN.md)

Installed apps can record a short voice clip and play an audio file through
OctoSense's native audio devices. These APIs currently support macOS and Android.
They do not transcribe speech, synthesize speech, capture system audio, download
media, or continue after the app leaves the foreground. Hardware acceptance is
**unverified**; synthetic tests never activate a microphone or speaker.

| Method | Arguments | Result |
| --- | --- | --- |
| `microphone.record_start` | `path`, optional integer `max_duration_ms` (100–30000; default 30000) | Host session and `starting` after permission checking and device setup |
| `microphone.record_status` | `session` | Current state and eventual output path/error |
| `microphone.record_stop` | `session` | `stopping`; poll until `saved` or `failed` |
| `microphone.record_cancel` | `session` | Stop and discard, without saving |
| `audio.play` | `path` | Host session and `starting` after bounded decoding and device setup |
| `audio.status` | `session` | Current state, including `playing`, `completed`, `stopped`, or `failed` |
| `audio.stop` | `session` | `stopping`; poll for completion |

Every result has `session`, `status`, `path`, `error`, `frames`, and `format`.
`path` is non-null only after a recording is saved. `error` is null until an
error/cancellation. `format` is `wav` for recording and null for playback.
`frames` counts native device frames; it is not a transcription or proof that
the user heard the audio. `starting` becomes `recording`/`playing` only when
device callbacks deliver frames. Missing frames fail within five seconds.

Declare `microphone` and `storage` for recording, and `audio` plus `storage`
for playback disclosure. These declarations do not gate calls; app admission,
the storage jail and microphone consent are checked independently.
Declare `requires: ["host-api-v1"]` and the exact methods in
`host_api.required` at major version 1. `runtime.describe` exposes the supported
methods. Unsupported platforms omit these methods. All seven are foreground-only,
including calls through an app agent; none is an agent-tool alias.

Before recording, use `microphone.permission.status` and, when needed,
`microphone.permission.request`. The existing host-owned consent sheet requires
physical approval, followed by the OS permission flow. `record_start` only checks
the current OS grant; it cannot open a permission prompt or borrow another app's
consent. The OS microphone indicator remains active while native input is open.
Provide a visible recording state, Stop and Cancel in the app using these APIs.

Sessions belong to the host-assigned app identity and live Splash heap. Another
app or a reopened instance cannot operate them. The shell supplies the focused
installed app or expanded Glance owner, and the runtime verifies its foreground
baseline. Switching apps, Home, closing the heap, changing its storage/account,
backgrounding the host or switching to another OS window, revoking microphone
consent, or losing a device stops
the session without automatic resume. Admission is checked before setup and
again by a worker at most one second apart; a changed manifest or withdrawal
terminates access. App Hub's embedded preview has no dedicated foreground app
identity: open the installed app in its own window first.

Only one recorder and one player run at once. Microphone ownership is shared
with shell dictation, including OS-owned speech recognizers; contention returns
`busy`, and prewarming dictation cannot replace a recorder's callback. Commands
and capture buffers are bounded. Audio callbacks use atomics and a fixed queue;
resampling, encoding, decoding and admission checks run on workers. UI results
arrive through a bounded channel polled without waiting. Overflow discards the
recording with an error. Sixteen session records are retained for up to a minute
while new sessions are admitted; the count remains bounded.

Recordings are mono 16 kHz PCM16 WAV, at most 30 seconds (960,044 bytes). They are
written into a **new** file in the app's existing Splash jail. Stop or the duration
limit saves; cancellation and foreground loss discard. A queued Cancel is
processed before a ready worker result can publish its file. Existing files are never
overwritten. Per-file, whole-jail and entry quotas still apply, including changes
made by the app while recording. No host path or provider URI is exposed.

Playback reads at most 1 MiB from that same jail and supports PCM16/float32 WAV,
MP3, FLAC and Ogg Vorbis. Decoding is limited to mono/stereo, 60 seconds and
2,880,000 frames. File import/export uses the existing `files` APIs separately;
network retrieval requires the app's normal HTTP permissions. This is a short
clip API, not a music streaming or background media service.

MorningBrief's custom `llm.speech`, `llm.speak`, and `llm.listen_*` patch still
needs migration to separately admitted speech providers. Native recording and
playback do not implement those ASR/TTS contracts. `model.audio` returns provider
TTS audio bytes; it remains a separate provider/billing choice.

Validation uses synthetic PCM, malformed codecs, queue saturation, app/heap
ownership, foreground transitions without drawing, consent refusal, storage
changes and microphone contention. Full shell/platform validation and real
device acceptance must be recorded separately; no physical result is implied
by the codec tests.

The existing OpenHarmony camera-video recorder also holds the shared microphone
lease until its native recorder stops and releases. Competing audio capture is
refused; muted video needs no lease. At this pin, Android camera video reports
unsupported and macOS has no camera-video recorder. Those routes do not gain
video recording from this service.
