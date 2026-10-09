---
name: light-engine
description: RAW photo develop (DNG, CR3, NEF, ARW, RAF; also JPEG, PNG, TIFF): read EXIF and XMP, develop with the engine's commands (exposure, colour, auto-tone), export. Read before using light.* tools.
---

# Light engine

lightcraft is a headless RAW developer in the Lightroom tradition: it
decodes camera RAW files (DNG, CR2/CR3, NEF, ARW, RAF, ORF, RW2, PEF and
more) and ordinary JPEG, PNG and TIFF, reads every EXIF, TIFF, GPS and XMP
tag, applies parametric develop settings (tone, colour, white balance,
detail, lens, effects) and exports JPEG, PNG, TIFF, WebP, AVIF or DNG. It
never overwrites an original. Pixel editing of layered documents is the
photo engine's ground, not this one's.

## Tools

- `light.info {path}`: the photo's size and summary, and every EXIF/TIFF/GPS tag and XMP property. Reads only.
- `light.run {path, cmds, out?, quality?, long_edge?}`: up to 64 of the engine's commands, run in order on the photo at `path`, then the developed photo exported to `out` when given; its extension picks the format (`.jpg`, `.png`, `.tif`, `.webp`, `.avif` or `.dng`), `long_edge` caps the size and `quality` is 1 to 100 (default 92). Each command is `{"id": ..., "params": {...}}`. The answer has each command's result in `results`, so query commands read without writing anything; `"cmds": []` with an `out` exports the photo as it is.

## How `light.run` works

The door checks every command of a call before it runs any. A command that
`commands.md` lists untagged works on the open photo only, and runs. Every
tagged id ([file], [network], [device], [host]) and any id that is not in
`commands.md` refuse the whole call, and nothing is written.

The develop commands: `develop.set {values: {control: number}}` sets
controls (or `{control, value}` for one), `develop.adjust {control, delta}`
nudges one, `develop.auto {}` runs auto-tone and `develop.reset {}` clears
the edit. `develop.controls {section?}` lists the controls with their
ranges, defaults and current values; `develop.get {}` gives the photo's
whole develop state. To develop several photos alike, make one call per
photo with the same commands. The original is never changed: write the
result to a new `out`.

## Files

Every path is relative to your own workspace, the folder your file tools
(`read_file`, `write_file`, `list_dir`, `view_image`) see: the light engine
works in it for you. Absolute paths, `..` and links out of it are refused.
Every engine works in the same folder, so what one writes the next can open,
and a file the person puts there is yours to use. No call ever replaces an
existing file: pick a new name for each `out`, or the call is refused. If
the person names a file outside your workspace, say that the light engine
cannot reach it. An original is never overwritten, and its XMP sidecar must
be inside your workspace too.

## Examples

1. Where and with what a photo was taken: `light.info {"path": "IMG_0042.dng"}`
   (camera, lens, exposure, GPS when the file has it).
2. Half a stop brighter, white balance at 7200 K, as a JPEG at most 2048 pixels long:
   `light.run {"path": "IMG_0042.dng", "cmds": [{"id": "develop.set", "params": {"values": {"light.exposure": 0.5, "wb.temp": 7200}}}], "out": "IMG_0042-edit.jpg", "long_edge": 2048}`.
3. Auto-tone, then a little more vibrance, as WebP:
   `light.run {"path": "IMG_0042.dng", "cmds": [{"id": "develop.auto"}, {"id": "develop.adjust", "params": {"control": "color.vibrance", "delta": 15}}], "out": "IMG_0042-auto.webp", "quality": 85}`.
4. The colour controls and their ranges, without writing anything:
   `light.run {"path": "IMG_0042.dng", "cmds": [{"id": "develop.controls", "params": {"section": "color"}}]}`.

## References

- `controls.md` in this skill's folder lists every develop control, one line
  each (id, label, range, default), by section. Grep it for a key before
  you set it (`grep -i vibrance controls.md`); `develop.controls` gives
  the same list live, with the photo's current values.
- `commands.md` lists every lightcraft command, one line each, with a tag on
  those that reach past the open photo (its library, files, the app).
  `light.run` runs the untagged ones, and refuses the rest.
