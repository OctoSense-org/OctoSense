# OctoSense icon family

English | [简体中文](README.zh-CN.md)

**Approved family, 2026-10-15.** This package contains 28 original SVG replacement
icons, the original Rinx PNG, and an icon design system derived from the existing News, Photos, App Hub,
Maps and OctoSense artwork. The approved family is now connected to the shared
shell catalog and the Mail/Calendar system bundles. It takes effect in builds
from this source; existing installed releases are not updated by a source edit.
**Browser updated, 2026-10-21:** C5 Night rose is the approved replacement,
with an ink dial, mint rim, four-spoke rose and ivory northeast point.

![Primary icon family with existing references](family-primary.png)

## How many icons are needed?

**14 product identities need new artwork**, including feature-selectable native
apps; **Rinx reuses its original icon**. **14 additional identities** appear in the optional checkout catalogs.
Together, these are **28 new icons and one restored original**. This is a count of app identities across
both products, not a claim that every build exposes all 29.

The audit covers the union of `desktop/system-apps.json` (10 bundles),
`phone/system-apps.json` (8 bundles; 11 unique bundles across both), active or
feature-selectable entries in `native-apps.json`, Home's Settings, the shell's
Assistant entry, and both `config/apps.json` catalogs. An entry with a binary
in a checkout catalog need not be installed or visible on a given machine.

Before this change, six product identities had no matching catalog artwork and resolved to the
same default `app.svg`: **Settings, Updates, Rinx, OctosCode, Reference and
AppCard**. The other replacements use Makepad's existing illustrations; they
are app-specific, but have no local OctoSense identity.

Rinx does have original artwork in its own repository. The shell now restores
that icon instead of redesigning it. Its 1024 px PNG is copied byte-for-byte from
`resources/icon_1024.png` at the revision pinned in `native-apps.json`.
See [provenance and checksums](references/rinx/provenance.json); upstream license
and attribution notices are retained alongside it.

| Product app | Existing artwork | Proposed identity |
| --- | --- | --- |
| Mail | Makepad envelope | Folded ivory envelope, mint letter, coral seal |
| Calendar | Makepad calendar | Coral flip calendar with physical binding rings |
| Calculator | Makepad calculator | Ivory calculator, sage display, coral equals key |
| Clock | Makepad dial | Two-color ivory dial and dark hands; no rim, feet or second hand |
| Notes | Makepad notepad | Pocket notebook with a mint pencil |
| Reminders | Makepad checklist | Ivory clipboard with coral completion marker |
| Weather | Makepad sun/cloud | Butter sun, ivory cloud and two rain strokes |
| Sheets | Makepad workbook | Ivory workbook with a green formula tab; every cell uses the same color |
| Settings | Shared default app | Ivory plate with three tactile sliders |
| OctoSense Updates | Shared default app | Upward install arrow between mint cycle arcs |
| Terminal | Makepad prompt | Ink command window, mint prompt, apricot cursor |
| OctosCode | Shared default app | Ivory code window, violet brackets, amber tab |
| Rinx | Shared default app | Restore the original Rinx cube artwork |
| Reference | Shared default app | Ivory reference book with apricot bookmark |
| AppCard | Shared default app | Layered ivory cards with an inset chat panel |

Reference, Sheets and AppCard have feature-dependent availability; Terminal is
desktop-only. Settings belongs to Home. The artwork can be shared whenever the
same identity appears on either product.

The **14 optional catalog apps** are Browser, Files, Task Manager, Finance,
Route, Image Viewer, PDF Viewer, Mixer, Fabric, Score, Video Player, VJ, Fab and
Director (`studio`). Task's native module is disabled in both products, but its
process entry remains in the checkout catalogs.

![Optional catalog icon family](family-optional.png)

## Existing identities to retain

| Identity | Current source | Treatment |
| --- | --- | --- |
| News | `crates/shell/resources/icons/apps/<style>/news.svg` | Reference; retain |
| Photos | Pinned Makepad `widgets/themes/<style>/icons/photos.svg` | Explicit user-selected reference; retain despite upstream origin |
| App Hub | Linked App Hub `APP_ICON_SVG`; local fallback `crates/shell/resources/icons/apps/apphub.svg` | Reference; retain |
| Maps | `crates/shell/resources/icons/apps/<style>/maps.svg` | Reference; retain |
| OctoSense | `desktop/packaging/icons/icon.svg` and `mark.svg` | Master brand; retain |
| Assistant | `crates/shell/resources/icons/apps/assistant.svg` | Existing branded speech bubble; retain |
| Rinx | Pinned Rinx `resources/icon_1024.png` | Original cube artwork; restore without redrawing |
| Camera | `apps/camera/bundle/icon.png` | Existing custom bundle artwork; retain |
| AI providers | `apps/ai-providers/bundle/icon.png` | Existing custom bundle artwork; retain |
| YouTube | `apps/youtube/bundle/icon.svg` | External brand identity; retain |
| Writer | `apps/writer/bundle/icon.svg` | Existing custom document/pencil; retain |
| Quick Deck | `apps/quickdeck/bundle/icon.svg` | Existing custom slide stack; retain |
| PDF Tools | `apps/pdftools/bundle/icon.svg` | Existing custom page stack; retain |

Wasm Lab already has its own SVG and is not in either shipping bundle catalog.
The `aichat` process is pane-only. Headless craft services, file-type glyphs,
user-created Studio apps and installed third-party apps are outside this count.
PDF Viewer and PDF Tools, Route and Maps, and Image Viewer and Photos are
separate identities; their icons intentionally remain distinguishable.

## The design system

The recommended direction extends the references' **small illustrated objects
on colored grounds**. A monochrome symbol set would lose their material character;
rendered 3D would introduce a different level of detail. Original SVG drawings
keep the family editable and compatible with the current vector asset pipeline.

**Form.** Use one dominant object and, where useful, one supporting cue. Give
objects broad silhouettes, gently rounded edges and deliberate overlap. Offset
paper stacks and fold corners instead of filling the icon with small decoration.
Use the OctoSense mark on established branded surfaces such as App Hub and
Assistant; do not stamp it onto every app.

**Material.** Warm ivory `#fffdf7` is the common light material. Ink `#233d40`
anchors dark objects. Mint supports ivory paper, apricot identifies a control or
bookmark, and coral marks a completed action or small emphasis. Limit depth to
two or three planes with small local contact shadows. No outer drop shadows,
glassy highlights, photographic texture or lens effects.

**Color.** Give each app a recognizable ground, with a restrained vertical
gradient. Teal serves communication and navigation; coral serves dates and
documents; amber serves notebooks and music; green serves structured work and
maintenance; violet serves code and conversational cards. Color supports the
object, so icons must remain distinguishable by silhouette. Exact per-app pairs
are recorded in [inventory.json](inventory.json).

**Geometry.** Author square, full-bleed SVG masters on a 64-point canvas. Keep
the entire foreground within the central 66%, approximately 10.88–53.12 on
both axes. Design major strokes at roughly 2–3.5 points before optical fitting.
Use paths for any necessary numerals; no font dependency. Local accents must
remain secondary at 24 px. Review artwork at 64, 40 and 24 px on light and dark
grounds, with Android, macOS and iOS frames.

**Framing.** The source assets have square backgrounds with no baked corners,
circular masks or outer shadows. The shell's shared frame policy owns the
shape. OctoSense uses macOS geometry. The review sheets apply static
approximations of the current geometry; they are not shell screenshots.
The retained original Rinx PNG is a legacy exception: its existing treatment
is preserved, and the previews apply the selected frame without editing the PNG.
See the [existing launcher artwork rules](../../../apps/README.md#launcher-icon-artwork).

## Authoring a new app icon

Apply this format when creating artwork for a new OctoSense app. The shared
family is a visual language, not a set of generic icons for new apps to reuse.

1. Choose one dominant object that represents the app's actual purpose. Give it
   a distinctive silhouette, with at most one supporting cue. Keep established
   app branding when it is yours to use; do not copy another app's icon or put
   the OctoSense/App Hub mark on an unrelated app.
2. Author one editable SVG master with `viewBox="0 0 64 64"` and square numeric
   dimensions. Start with a full-bleed colored ground and a warm ivory object
   (`#fffdf7`), ink (`#233d40`) for contrast, and restrained mint, apricot or coral
   accents. Use broad shapes, rounded object edges and two or three overlapping
   planes. Keep every foreground detail inside 10.88–53.12 on both axes. Ground
   colors may vary by app; the objects must remain recognizable without color.
3. Use self-contained paths, rectangles and circles, with explicit colors and
   lettering converted to paths. Keep depth local to the object. Export square
   artwork without platform corners, masks or an outer drop shadow. If the
   original artwork is raster, use a square PNG and preserve intentional
   transparency; follow App Hub's size and admission limits.
4. Review 64, 40 and 24 px on light and dark grounds, alongside the family and
   with Android, macOS and iOS frames. Inspect a real target-shell render before
   claiming native acceptance. Record the master, provenance and review images
   outside the release bundle unless the app needs them at runtime.
5. For a store app, replace the starter artwork with this app's own identity,
   ship `bundle/assets/icon.svg` (or PNG), and declare that exact bundle-relative
   path in `listing.json`. The shell draws it automatically through `AppIconDraw`.
   Do not add store apps to the built-in overlay or modify Makepad's icons.
   Built-in family additions instead use the local generator, explicit registry
   and Omarchy outline, with the existing resource and rendering checks.

[App Hub's icon guide](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/ICONS.md)
owns packaging and gate limits; App Flow's starter instructions and publishing
workflow carry these authoring rules into each new app. A gate pass checks file
validity, not visual quality or conformity to this family.

## Integration and verification

The shared shell embeds 28 approved SVG masters and their Omarchy outline
variants under `crates/shell/resources/icons/apps/family/`. The style catalog
replaces the corresponding upstream entries and adds Settings, Updates,
OctosCode, Reference and AppCard. Rinx's original PNG is embedded separately
and drawn by the same platform image mask used for bundle artwork. Installed
`hub:` identities keep their own icons.

Mail and Calendar also carry their approved `icon.svg` in their system bundles;
App Hub packs those icons automatically. The explicit family catalog takes
precedence for owned built-in identities in every style. Other existing custom
bundle artwork and installed `hub:` apps retain their own icons.
Launcher rows, dock items, Home, group previews and app badges use the existing
shared `AppIconDraw` path. Omarchy's catalog uses outline versions of the
approved objects so its single-ink tint preserves internal detail. Windows
2000 keeps a freeform canvas without colored background tiles. Existing News
and Maps artwork in that style is preserved.

### Keeping Makepad updates easy to merge

The Makepad checkout, its theme icon files and its runtime patches are unchanged
by this feature. `icon_assets` loads the upstream catalog and then calls
`icon_family::apply` once. The local module owns the explicit 28-ID replacement
list, merges by ID and adds missing local identities. It does not copy or freeze
the full upstream catalog.

When Makepad changes an owned icon, OctoSense keeps its approved replacement.
When Makepad adds an unowned ID or changes an unowned icon, that artwork passes
through unchanged. Removing an override is an explicit local policy change.
Rinx remains the separate original PNG; Photos remains inherited. The existing
News, Maps, App Hub and Assistant custom artwork stays in its existing layer.

The effective policy is: native Android package art is preserved by its existing
path; installed `hub:` apps own their icons; explicit built-in replacements use
the local family; other system bundles can provide their own art; remaining
identities use Makepad's catalog and generic fallback. Platform shape still
belongs to `AppIconDraw` and the shared frame policy.

For future Makepad updates, update its normal pin and prepared sources, then run
the family synchronization check, shell tests and hidden preview described
below. No icon cherry-picks or edits inside `.sources/makepad` are needed.
Regression tests simulate upstream Browser changes and new IDs, verify repeat
application does not duplicate entries, and compare ID unions instead of a fixed
upstream icon count.

- [Latest revisions](revised-icons.png): simplified Clock, uniform Sheets cells and original Rinx.
- [Primary family](family-primary.png): 14 new icons and original Rinx alongside the five references.
- [Optional family](family-optional.png): 14 additional catalog tools.
- [Before and after](before-after.png): current and proposed primary icons.
- [Platform and size review](platform-review.png): all 29 at three sizes, three frames, two grounds.
- [Assets](assets/): 28 editable, unframed SVG masters and the original Rinx PNG.
- [Inventory](inventory.json): app IDs, scopes, concepts, palettes and source paths.
- [Validation receipt](validation.json): counts, measured foreground bounds and asset hashes.
- [Implementation receipt](implementation-validation.json): source hashes, build/test results and GPU checks.
- [Production GPU captures](evidence/): all family icons and preserved custom artwork in seven shell styles.

Rebuild from the repository root after setup:

```sh
python3 tools/build_icon_family.py
python3 tools/build_icon_family.py --install
python3 tools/build_icon_family.py --check
```

These commands were run. Review generation requires `rsvg-convert` and Pillow;
`--check` is read-only and checks all 63 shipping resources against their
approved sources. The builder checks coverage
against the current native and checkout catalogs, 28 distinct SVG foregrounds, XML
parsing, full-bleed square sources and raster-measured safe-area bounds for new
SVGs, Clock's two-color palette, uniform Sheets cell colors, and original Rinx
PNG checksums against its provenance. All
checks passed. Primary and optional sheets were visually inspected, together
with the platform/size review and before/after comparison.

Production validation passed **1,280 shared shell unit tests**, desktop checks
with default features and `mobile-apps`, Home's `mobile-apps` check, and both
shell dependency-graph checks. The existing branding resource tests passed
(5 tests), as did the personal-path regression check (1 test).

The hidden production-renderer preview passed **238 family pixel checks** and
**34 existing PNG/SVG shape, color and opacity checks**. Captures cover all 29
identities in seven styles at 64 and 24 logical pixels, on dark and light
grounds, at full and half opacity. C5's dark dial, ivory northeast point and mint
spoke are checked in each framed style. Freeform rows are explicitly checked so
Mail/Calendar bundle precedence cannot bypass the family variants. All four
family pages were visually inspected. The suite exercises unit and GPU fixtures;
no real model calls or business effects are claimed.

To repeat the GPU review, first build from `phone/`:

```sh
cargo build --locked --features mobile-apps -p octosense-shell --example icon_shapes
```

Then run from the repository root:

```sh
python3 tools/check_icon_family_preview.py --capture
```

Both commands were run. The capture harness creates separate temporary Home
and Rinx directories, hides every window, uses loopback control ports, and
closes each preview through `/quit`. It accounts for Retina backing pixels.

**Devices: unverified.** No device was assigned for this change. Platform-style
previews exercise geometry in a macOS process; they are separate from native
Android, iOS, Windows or Linux device acceptance and from a published release.
