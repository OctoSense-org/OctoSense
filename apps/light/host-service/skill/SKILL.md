---
name: light-engine
description: RAW photo develop (DNG, CR3, NEF, ARW, RAF; also JPEG, PNG, TIFF): read EXIF and XMP, develop with exposure and colour controls, batch export. Read before using light.* tools.
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

- `light.info {path}`: the photo's size and summary, and every EXIF/TIFF/GPS tag and XMP property.
- `light.controls {}`: the develop controls (id, label, range, default): the valid keys of `params`.
- `light.develop {path, out, params?, auto?, long_edge?, quality?}`: develop one photo and export it; the extension of `out` picks the format (.jpg, .png, .tif, .webp, .avif, .dng). `params` maps control ids to numbers, `auto` runs auto-tone first, `long_edge` caps the size, `quality` is 1 to 100 (default 92).
- `light.batch {paths, out_dir, format?, params?, auto?, long_edge?, quality?}`: the same develop on 1 to 16 photos, each written as `<out_dir>/<name>.<format>` (jpg by default); two inputs with the same file name are refused.

`light.info` and `light.controls` only read; the others write.

## Files

Every path is relative to the light engine's folder, a private workspace
that only `light.*` tools read and write. Absolute paths, `..` and links out
of it are refused. Your own workspace (`read_file`, `write_file`,
`view_image`), the person's files and the other engines' folders are outside
it, and what these tools write stays in it. Nothing you have puts a photo
into this folder: these tools open only files an earlier `light.*` call
wrote there, so a photo the person has elsewhere cannot be opened yet. Say
so rather than guessing names, and pick a new name for each `out`.

## Examples

1. Where and with what a photo was taken: `light.info {"path": "IMG_0042.dng"}`
   (camera, lens, exposure, GPS when the file has it).
2. Half a stop brighter, white balance at 7200 K, as a JPEG at most 2048 pixels long:
   `light.develop {"path": "IMG_0042.dng", "out": "IMG_0042-edit.jpg", "params": {"light.exposure": 0.5, "wb.temp": 7200}, "long_edge": 2048}`.
3. Auto-tone a set into WebP: `light.batch {"paths": ["a.dng", "b.dng"], "out_dir": "web", "format": "webp", "auto": true}`.

## References

- `controls.md` in this skill's folder lists every develop control, one line
  each (id, label, range, default), by section. Grep it for a key before
  you put it in `params` (`grep -i vibrance controls.md`); `light.controls`
  gives the same list live.
- `commands.md` lists every lightcraft command, one line each, with a tag on
  those that reach past the open photo (its library, files, the app). No
  tool on your list runs these ids: they show the engine's reach, not what
  you can call.
