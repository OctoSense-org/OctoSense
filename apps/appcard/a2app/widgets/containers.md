# Containers & style widgets

Style ONLY with the allowed props below. Setting a property a widget does NOT have
crashes the whole card. Never write custom shaders inside `draw_bg` (`pixel: fn`,
`fn(`, `let`, `mut`, `Sdf2d`, `uniform(`, `instance(`, `.mix(`).

Allowed styling props (the ONLY ones):
- `draw_bg.color`
- `draw_bg.color_2` — gradient views only
- `draw_bg.border_radius` — rounded views only (a single float, e.g. `20.0`)
- `draw_bg.shadow_color` / `draw_bg.shadow_offset` / `draw_bg.shadow_radius` — shadow views only
- `draw_text.color`
- `draw_text.text_style.font_size`

## SolidView

Solid flat-color rectangle background.
```
SolidView{ width: Fill height: Fit draw_bg.color: #1c1c1e }
```

## RoundedView

Solid fill with rounded corners. Supports `draw_bg.border_radius`.
```
RoundedView{ width: Fill height: Fit draw_bg.color: #445 draw_bg.border_radius: 20.0 }
```

## RoundedShadowView

Rounded corners + a soft drop shadow (it DOES support border_radius). Keep a
`margin` so the shadow has room. Good default CARD container.
```
RoundedShadowView{ draw_bg.color: #hex draw_bg.border_radius: 24.0 draw_bg.shadow_color: #00000055 draw_bg.shadow_offset: vec2(0.0, 8.0) draw_bg.shadow_radius: 24.0 margin: 14 }
```

## GradientYView / GradientXView

A full-width RECTANGLE gradient. `GradientYView` = vertical, `GradientXView` =
horizontal. Set both `draw_bg.color` (start) and `draw_bg.color_2` (end). It has NO
border_radius — NEVER put `border_radius` on a Gradient*View. Do not mix gradient
and rounded in one container; pick one.
```
GradientYView{ width: Fill height: Fill draw_bg.color: #00000022 draw_bg.color_2: #000000EE }
```

## View

Transparent layout container (no background). Layout props:
- `flow: Down` | `Right` | `Overlay`
- `align: Align{x: y:}` (e.g. `Align{y: 0.5}`)
- `spacing:` — gap between children
- `padding: Inset{left: top: right: bottom:}` (or a bare number for uniform)
- `margin: Inset{...}`
```
View{ width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 10 }
```

## Filler

A spacer that pushes siblings apart (`Filler{}`). Use it between fixed-width
siblings in a `flow: Right` row.

## Image

- `src: http_resource(...)` for a remote/AI/map image.
- `fit:` — `ImageFit.CropToFill` (fills the box, cropping overflow — a true
  edge-to-edge photo), `ImageFit.Smallest`, etc. Use `CropToFill` for full-bleed
  backgrounds and maps; never Smallest/Biggest/Vertical/Horizontal on a full-screen
  photo (they leave letterbox bands).
```
Image{ src: http_resource("https://...") fit: ImageFit.CropToFill width: Fill height: 190 }
```

## Label

Text. Style with `draw_text.color` and `draw_text.text_style.font_size`. Set
`width: Fill` on any headline/sentence Label so it wraps to multiple lines instead
of clipping. Default text color is white, so set a dark `draw_text.color` on light
backgrounds.
```
Label{ width: Fill text: "..." draw_text.color: #ffffff draw_text.text_style.font_size: 20 }
```
