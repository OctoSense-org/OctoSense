# Playback evidence (13 Oct 2026)

English | [简体中文](README.zh-CN.md)

Quick Deck plays a deck's video and audio with the runtime's `Video` widget
(SERVICE.md "Playback"; the deck service's `deck.media`, #515). These are
the frames and checks of the runs that verified it on this branch, rebased
on main ffd5e488. [receipt.json](receipt.json) has the commands, the
binaries' digests, the checks and every round.

## card-host: the dev fixture, light and dark

App Hub's `card-host` (its pinned 38635915, built against this checkout's
runtime), hidden and driven over its remote bridge on macOS 26.6.2 (Apple
silicon), ran `tests/ui.py`'s `playback` run on `make_fixture.py --media`'s
Launch Film. The clip is the repository's synthetic five-second H.264 clip
with no sound (`dev-fixture/media/clip.mp4`): red, green and blue thirds
crossed by a white bar. The suite's `missing`, `fixture` and `states` runs
passed in the same session. Frames are halved; a `d` in front marks dark.

| Frame | What it shows |
| --- | --- |
| [p1-clip](card-host/p1-clip.png) | The chosen video: Play under it; the inspector's Video section says it starts by itself in a show and plays once |
| [p2-playing-pair](card-host/p2-playing-pair.png) | The clip's box in two grabs a second apart while it plays, at full size: 13,850 pixels changed, and the control's time read 0:01 then 0:03. The whole frames are [p2-playing-a](card-host/p2-playing-a.png) and [p2-playing-b](card-host/p2-playing-b.png) |
| [p3-paused](card-host/p3-paused.png) | Pause: the picture and the time held for 1.2 seconds |
| [p4-sound](card-host/p4-sound.png) | A sound: the small speaker control |
| [p5-unsupported](card-host/p5-unsupported.png) | `deck.media`'s `unsupported:` (the fixture's sound is linked to a file outside the deck), in plain words |
| [p6-failed](card-host/p6-failed.png) | A clip whose file isn't there, once a clip has played: "This clip couldn't be played." |
| [p7-storage-full](card-host/p7-storage-full.png) | `deck.media`'s `storage_full:` in the contract's words, and the storage bar |
| [p8-present-pair](card-host/p8-present-pair.png) | Present: the clip that plays by itself, two grabs a second apart: 33,418 pixels changed |
| [p9-present-paused](card-host/p9-present-paused.png) | A tap paused it ("Paused · 0:04 / 0:05"); a second tap played it on |
| [p10-present-next](card-host/p10-present-next.png) | The next slide: nothing plays |
| [p11-unavailable](card-host/p11-unavailable.png) | A new session where nothing has played, the same clip: "Playback isn't available on this system", and the player's words under it. On a Mac this is AVFoundation refusing the missing file; the runtime then tries its software decoder, which has no media plugin: the same error the widget gives where no player takes a file |

## macOS: the real engine

A desktop shell built from this branch, hidden on macOS 26.6.2, with the
real deck service: the `playback` run on `tests/media`'s decks (the sample
clip again), light then dark. After every step the shell's log had to show
no `wm: removing client` and no contained module fault. The sound was
never played: these runs open no audio device.

| Frame | What it shows |
| --- | --- |
| [p0-home](macos/p0-home.png) | The native deck and its `.pptx` copy in the library |
| [p1-editor-playing-pair](macos/p1-editor-playing-pair.png) | The native deck's video playing in the editor: 6,825 pixels changed in a second, the time 0:01 then 0:03 ([a](macos/p1-editor-playing-a.png), [b](macos/p1-editor-playing-b.png)). Pause, Play, a tap on the video and Stop followed |
| [p2-present-playing-pair](macos/p2-present-playing-pair.png) | Present: the video started by itself and played (12,814 pixels changed); four taps paused it and played it on, on the same slide |
| [p4-pptx-present-playing-pair](macos/p4-pptx-present-playing-pair.png) | The `.pptx` copy in Present: nothing started by itself; a tap played the clip (12,814 pixels changed) |

[playback.json](macos/playback.json) records `deck.media`'s file,
`.cache/media/66ed412a….mp4`, 26,739 bytes: the clip's own SHA-256.

## Linux: the real engine

The same run in a desktop shell built from this branch, hidden on headless
weston. The host has GStreamer's base plugins only, with no MP4 demuxer and
no decoder, so the widget fails every clip with GStreamer's "Your GStreamer
installation is missing a plug-in." The `shell` and `restart` runs passed
in the same session, unchanged.

| Frame | What it shows |
| --- | --- |
| [p0-home](linux/p0-home.png) | The native deck and its `.pptx` copy in the library |
| [p1-editor-unavailable](linux/p1-editor-unavailable.png) | The native deck's video, Play: the poster stays, "Playback isn't available on this system", the player's words in the inspector |
| [p2-present-unavailable](linux/p2-present-unavailable.png) | Present: the video set to play by itself tried, and the stage says it can't |
| [p3-sound-unavailable](linux/p3-sound-unavailable.png) | The sound, Play: the same words |
| [p4-pptx-present-unavailable](linux/p4-pptx-present-unavailable.png) | The `.pptx` copy in Present: nothing started by itself (deckcraft reads no autoplay from a `.pptx`); a tap tried the clip |

[playback.json](linux/playback.json) records what `deck.media` wrote: the
sample clip and the tone (`.wav`); closing the deck took both out of the
cache.

Both shells ran main ffd5e488, which has #516 and not yet #519: Quick
Deck's toolbar is no longer called `bar`, so the shell's lookup of its own
`bar` no longer finds Quick Deck's and doesn't panic.

## Not verified here

Sound with audio out on any system (these runs open no audio device);
Windows; a looping clip playing; a light/dark switch while a clip plays;
clips inside a group.
