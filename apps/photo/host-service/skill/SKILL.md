---
name: photo-engine
description: Raster images (PNG, JPEG, WebP, PSD, TIFF): what the photo engine and its 800-plus commands can do, and how little of it you reach today. Read before taking on an image edit.
---

# Photo engine

photocraft is a headless raster editor in the Photoshop tradition: layered
documents with masks and adjustment layers, filters, selections, paths,
type, colour modes and ICC profiles. It opens and writes PNG, JPEG, WebP,
PSD, TIFF and more. Developing camera RAW files is the light engine's
ground, not this one's.

## Tools

You have no photo tool of your own yet: the engine's `photo.*` methods are
not on your list. The Photos app's agent has `photos.info` (width, height,
layers and colour mode of a file in that agent's own folder); ask it with
`agents.ask` when that is what the person needs. For an edit, say plainly
that you cannot change images yet, and what the engine would do.

## Files

The photo engine works in the folder of whoever calls it, with paths
relative to it: for the Photos agent, that agent's own folder. It never
replaces an existing file, and a document whose smart object links a file
outside that folder is refused. Nothing you have puts an image there: the
person's photos, the Photos gallery included, and your own workspace are
outside it.

## Examples

1. The person asks you to blur a photo's background. Grep `commands.md` for
   `blur` (`grep -i blur commands.md`) and tell them what the engine offers
   (`filter.blur.gaussianBlur` and its radius, the lens and surface blurs),
   and that you cannot run it for them yet.
2. The person asks how large an image in the Photos agent's folder is: ask
   the Photos agent with `agents.ask`, which answers with `photos.info`.

## The engine's commands

`commands.md` in this skill's folder lists every photocraft command, one
line each (id, label, parameters), with a tag on those that reach past the
open document: files, plug-ins, the app. Grep it rather than reading it
whole. No tool on your list runs these ids: they show the engine's reach,
not what you can call.
