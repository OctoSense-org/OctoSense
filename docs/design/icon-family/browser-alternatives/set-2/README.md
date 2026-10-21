# Browser icon alternatives — set two

English | [简体中文](README.zh-CN.md)

Four further design studies explore navigation and discovery with the existing
Browser teal ground and the OctoSense ivory, mint and apricot materials.

![Second Browser study set](comparison.png)

| Option | Direction | Tradeoff |
| --- | --- | --- |
| [E — Compass](e-compass.svg) | A simple ivory dial and two-tone needle | Recommended as a cleaner evolution of the current icon; familiar navigation metaphor |
| [F — World + pointer](f-world-pointer.svg) | A world with broad land shapes and an apricot pointer | Clearest web-interaction cue; pointer adds a second object |
| [G — Sail](g-sail.svg) | An ivory sailboat with a mint sail and apricot hull | Distinctive exploration identity; browser meaning depends on learned association |
| [H — Search lens](h-search-lens.svg) | A globe inside a tactile magnifier | Clear web-search metaphor; can also suggest a dedicated search app |

The earlier A–D studies remain available in [set one](../README.md). These SVG
masters have square, full-bleed backgrounds, no baked platform corners or outer
shadows, and foreground artwork inside the central 66% safe area. The sheet
shows 64/40/24 px sizes, light and dark grounds, approximate platform frames,
and approved family neighbors. No shipping artwork is changed.

Rebuild from the repository root:

```sh
python3 docs/design/icon-family/browser-alternatives/set-2/build.py
```

This command was run. It uses the first set's preview builder, librsvg and
Pillow, checks XML parsing and raster foreground bounds, and generates the
masters and comparison sheet. The comparison was visually inspected.
Production rendering and device behavior are **unverified** for these studies.
