#!/usr/bin/env python3
"""The real-engine journeys' decks with clips (playback, SERVICE.md "Playback").

The deck service holds the engine's `insert.video`, `insert.audio` and
`media.options` (#448), so these decks are made with the deck engine's own
command line at the pinned deckcraft (d0e57d7e), built from a deckcraft
checkout at that revision:

  cargo build --locked --release -p deckcraft-cli
  python3 apps/quickdeck/tests/make_media_decks.py --cli <deckcraft-cli> --out apps/quickdeck/tests/media

It writes launch-film.deckcraft: slide 1, "Launch film", holds the
repository's synthetic five-second H.264 clip with no sound
(crates/video-api-smoke/resources/playback.mp4) set to play by itself;
slide 2, "Soundtrack", a one-second 440 Hz tone made here, played by a tap.
Then launch-film.pptx, the same deck saved as PowerPoint: deckcraft's
.pptx reader doesn't read PowerPoint's timing, so its clips come back set to
play on a tap only.
"""
import argparse
import json
import math
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import wave

ROOT = Path(__file__).resolve().parents[3]
CLIP = ROOT / "crates/video-api-smoke/resources/playback.mp4"


def tone(path, seconds=1.0, rate=16000, hz=440):
    """A sine tone with a short fade in and out, 16-bit mono."""
    n = int(seconds * rate)
    fade = rate // 20
    frames = b"".join(struct.pack("<h", int(6000 * math.sin(2 * math.pi * hz * i / rate) * min(1, i / fade, (n - i) / fade)))
                      for i in range(n))
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(frames)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--cli", type=Path, required=True, help="deckcraft-cli built at deckcraft d0e57d7e")
    parser.add_argument("--out", type=Path, required=True, help="where launch-film.deckcraft and launch-film.pptx go")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as scratch:
        scratch = Path(scratch)
        shutil.copyfile(CLIP, scratch / "clip.mp4")
        tone(scratch / "tone.wav")
        commands = [
            ("slide.new", {"layout": "titleOnly", "title": "Launch film"}),
            ("insert.video", {"path": "clip.mp4", "rect": [200, 120, 560, 315]}),
            ("media.options", {"autoplay": True}),
            ("slide.new", {"layout": "titleOnly", "title": "Soundtrack"}),
            ("insert.audio", {"path": "tone.wav", "rect": [440, 200, 80, 80]}),
            ("slide.delete", {"index": 0}),
        ]
        argv = [str(args.cli.resolve()), "run", "--print"]
        for name, params in commands:
            argv += ["--cmd", name + "=" + json.dumps(params)]
        argv += ["--save", "launch-film.deckcraft", "--export", "launch-film.pptx"]
        subprocess.run(argv, cwd=scratch, check=True)
        for name in ("launch-film.deckcraft", "launch-film.pptx"):
            shutil.copyfile(scratch / name, args.out / name)
            print("wrote", args.out / name)


if __name__ == "__main__":
    main()
