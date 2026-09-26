# Shared theme contract for OctoSense apps

Every OctoSense app should follow the system by default. The ROM resolves the
selected preset and Android's appearance once. Apps consume that result rather
than maintaining independent light/dark preferences, copies of the preset
catalog, or separate theme engines. A preset change must also reach apps that
are already open, without recreating their instances or clearing user input.

## Existing implementation to build on

`ModuleHost` installs the same Makepad stylesheet in every hosted app isolate
before creation and reapplies it on theme changes. Use the existing `mod.theme`
roles in Splash widgets and `makepad-wm-theme` for custom Rust surfaces. Keep
theme resolution at registration/reapply time and retain resolved values with
the app instance. Do not read preferences or files from a draw loop, and do not
cache just a light/dark boolean: two presets can have the same appearance but
different colors.

| Purpose | Existing Makepad role |
| --- | --- |
| App background | `theme.color_bg_app` |
| Card or panel | `theme.color_bg_container` |
| Field or secondary surface | `theme.color_inset` |
| Main text | `theme.color_text` |
| Secondary text | `theme.color_text_disabled` |
| Accent and focus | `theme.color_focus` |
| Text on accent | `theme.color_text_on_accent` |
| Border | `theme.color_bevel_outset_2` |
| Font families | `theme.font_regular`, `theme.font_bold` |
| Corners | `theme.corner_radius`, `theme.container_corner_radius` |

Shared controls own their hover, pressed, focus and disabled states. Apps own
content and layout. Error/success states, category labels, charts and media can
use semantic or content colors, but ordinary page backgrounds, labels and action
buttons must use the shared roles. A camera viewfinder or photograph can retain
its own contrast treatment without introducing an independent app theme.

Generated AppCards and custom design packs should reference semantic roles too.
The renderer should resolve those roles from the same active theme snapshot;
generation should not bake the current palette into every card. HTML readers
need a CSS adapter for their outer controls and default document colors, while
retaining meaningful colors supplied by the document author.

For future separate APKs, Android's dynamic palette handles standard Material
controls. Exact OctoSense colors and metrics need a read/subscribe SDK adapter
to the same versioned theme snapshot through the ROM service. That adapter is
not implemented by the current picker. It must not create another source of
theme preferences. A future snapshot should include a schema version, revision,
resolved appearance, semantic colors, typography/metrics and reduced-motion
preferences; consumers should receive an initial snapshot and later revisions.

## Audit of the current bundled apps, 24 September 2026

The shared base theme reaches all nine visible hosted app modules. This does not by
itself recolor application code that draws literal colors or supplies its own
HTML/design pack.

| App | Current state and remaining migration |
| --- | --- |
| Reference | Uses the shared background and stock widgets. |
| Settings | Uses Octoscript layout and semantic stock widgets; live theme changes retain the page, draft and control identities. |
| Sheets | Uses the WM theme bridge; live restyling is covered by tests. |
| Photos | Custom UI in `apps/photos/src/ui.rs` contains fixed light backgrounds, text and blue actions; map them to shared roles, retaining media overlays. |
| News | Follows the host's light/dark mode, then uses its own `Skin::for_mode` colors. Replace interface colors with resolved shared roles. |
| Maps | Similar to News; keep map content, routing and warning colors semantic while unifying surrounding controls. |
| Mail | Its generated design pack explicitly selects `theme light`, and its HTML wrapper has fixed colors. Both need a shared-theme adapter. |
| Camera | Receives the base theme, but uses custom design/kit scenes. Audit scene controls separately from the viewfinder. |
| AppCard | Receives the base theme; generated cards need semantic bindings in the shared renderer rather than per-card palette copies. |
| App Store (hidden) | Retained in code but removed from the launcher while the catalog is empty; custom app surfaces need a visual compliance check before returning. |

The regression test `bundled_apps_receive_same_base_theme_without_recreation`
checks all modules against contrasting presets and verifies background, text,
accent, root identity and script errors. It tests the host contract, not every
pixel of each app's custom content. New apps should pass this test and a visual
check of light/dark, accent-only changes, text scaling and focused input.

Migration order: shared custom-surface adapter; Photos/News/Maps; Mail's design
and HTML adapters; generated AppCards and Camera/App Store scene controls. Keep
these migrations in the owning app/runtime repositories and update ROM pins;
avoid ROM-only color-replacement patches that drift from standalone apps.
