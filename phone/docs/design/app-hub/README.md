# App Hub design evidence

The native App Hub module these records describe (`apps/app-hub/...` in the
JSON records below) now lives in OctoSense-App-Hub as `crates/app-hub-app`
(crate `octosense-app-hub-app`), which Home links as a git dependency. The
records keep the paths they were validated at.

References: the user's five App Store screenshots are kept locally in the
Git-ignored `appstore/` directory; they are not included in a fresh checkout.
Their provenance hashes are retained in `source/references.json`.
The generated atlas uses the same large titles, editorial
artwork, grouped app rows, blue actions and bottom navigation. The native view
adapts them to the OctoSense palette, width and shell chrome.

The exact atlas prompt and original output are in `source/`. Provider: OpenAI
imagegen tool; exact model was not reported. Requested geometry was 1624×1552;
actual output is 1019×1544. The lab intake measured eight reviewed crops without
pretending the requested dimensions were delivered. `intake/intake.json` records
hashes, dimensions and uniform transforms.

Reproduce the image-to-appcard intake from `home/`, using a checkout of
OctoScript-App-Design-Flow (formerly Octoscript-AppCard) at `cbbda4da`:

```sh
python3 /path/to/OctoScript-App-Design-Flow/lab/image-to-appcard-flow/atlas.py \
  --manifest docs/design/app-hub/image-to-appcard-flow.json \
  --project docs/design/app-hub --output docs/design/app-hub/intake
```

The complete flow runner requires English and Chinese; this version is English.
We use its atlas intake directly. `semantic-map.json` records the native scene
contract, widget mappings, actions and deliberate adaptations. The production
module renders responsive Makepad widgets directly; it does not compile these
references to L0. No scene screenshot is shipped as runtime UI.

Only the two declared photographs in `artwork.json` are runtime rasters. They are
reused from the existing Photos sample library, whose original attribution is in
`apps/photos/resources/SOURCES.md`. The Photos source images are from Unsplash.
Icons are native SVGs; headings, descriptions, search, buttons, navigation and
confirmation are native widgets.

A phone-sized instrument host is available for visual inspection:

```sh
MAKEPAD_REMOTE=true cargo run --release -p octosense-app-hub-app --example preview
```

Its launch actions are logged; use the OctoSense shell to open apps. Set
`OCTOSENSE_APP_DATA` to an isolated directory when validating installation.
Native compilation, catalog tests and visual acceptance are separate checks;
intake success alone does not imply visual acceptance.

## Native visual review

The captures in `evidence/` come from the actual OctoSense shell at 412×892
logical pixels on macOS, using its phone layout. They are not atlas crops.
Each JSON sidecar preserves the instrument's capture receipt. Reviewed:

- Live catalog's empty state and explicit preview entry point.
- Preview Today editorial artwork, six grouped app rows and search with the
  native keyboard. Titles, subtitles and actions fit the phone width.
- Details with preview artwork labeled as such, About text and an Open action.
- Signed local fixture icons/screenshots, permission confirmation, successful
  installation and both apps in Library.
- Opening the existing Photos module from both its list row and detail page.
- Opening installed Trail Notes from App Hub and installed Focus Timer from the
  launcher. Their native screens and separate Recents entries were inspected;
  the runtime log records distinct clients and per-app containment settings.
- Switching the phone's Controls shade to dark mode with Trail Notes running.
  Its content remains mounted, and returning to App Hub retains the installed
  catalog. Card bundles keep their own chosen artwork/theme.

`live-fixture`, `install-consent`, `installed-detail` and `library` use the
optional local signed catalog described in the
[App Hub README](../../../apps/app-hub/README.md). Those are development
fixtures, not production store listings. The production store remained empty.
The fixtures deliberately use a fixed 360×640 artboard, visible inside the
larger phone content area in their running-app captures.

The shell supplies status and navigation chrome. App Hub's bottom tabs remain
above the shell's swipe indicator, and scrollable content continues underneath
the fixed tab area without covering the tab labels. The native composition
adapts the references instead of claiming pixel identity with the generated
atlas. Android packaging was checked separately; no device was connected for this
initial validation. The later physical-device checks are recorded below.

Final verification passed 33 App Hub library tests, 2 signed fixture tests,
15 native setup tests and 293 shell tests with all mobile features. The final
Android APK also built successfully. [Validation receipt](validation.json)
records the source and artifact hashes, checked flows and remaining device
limitation. `apphub-dark` and `detail-dark` show active navigation and wrapped
detail text after the live appearance change.

The initial validation explicitly enabled App Hub. A later local-launch check
found it missing from the default feature set; this is now corrected. The
documented `cargo run --release --features mobile-only` command shows App Hub
in All apps and opens it without additional feature or module flags. That
configuration passes 289 shell tests. Its screenshots and updated build hashes
are recorded in [local launch validation](local-launch-validation.json).

## Shared icons and physical-device follow-up

The adopted [icon convention](icon-standard-proposal.md) now drives preview
icons through the same shared widget as the launcher. App Hub owns its SVG at
`apps/app-hub/assets/icon.svg`, declared in its local `listing.json`. The initial
Octo Bloom kept its eight petals and added a central 2×2 app grid. Installed
Hub apps use their local listing icon, with bounded SVG/PNG loading and fallback.

The updated APK was installed on a Pixel 7 Pro with existing data retained.
The launcher and Hub header show the new bloom; preview icons match the launcher.
Native Photos search, details and opening Photos passed. The phone is left on
the [preview Apps page](evidence/adb-final-apphub.png). Camera retains the same
existing generic fallback in both surfaces. Desktop signed fixture icons also
match between the launcher and Hub.

Those checks passed 35 App Hub library tests and 293 shell tests, plus
release desktop and Android builds. The production catalog is empty, so the
phone walkthrough did not include store installation. The earlier desktop
signed-fixture install checks remain separate. See the
[icon and ADB validation receipt](icon-validation.json) for hashes and captures.

## Store symbol follow-up

The first store-symbol revision used a shopping bag with a prominent 2×2 app grid,
replacing the bloom at the user's request for a clearer hub/store symbol.
The teal tile, ivory bag and mint handle preserve the established palette.
This was hand-authored SVG geometry; the earlier generated concepts remain
historical references. The shared asset updates the launcher and Hub header.

[Small-size review](evidence/store-icon-sizes.png) covers light and dark
backgrounds down to 24 pixels. All 35 App Hub tests and the Android release build
passed. The APK was installed on the Pixel 7 Pro with data retained; the new
icon was checked in the [home dock](evidence/store-icon-home.png) and
[App Hub header](evidence/store-icon-hub.png). See the
[store icon validation receipt](store-icon-validation.json).

The current refinement places the **OctoSense logo inside the shopping bag**,
as requested. It reuses all eight original path definitions from
`apps/news/resources/icons/octosense.svg`, uniformly scaled and colored teal.
The bag outline, mint handle, ivory surface and shared asset mapping are retained.
[Preview](evidence/store-logo.png) and [size review](evidence/store-logo-sizes.png).
All 35 App Hub tests and the Android release build passed. The updated APK was
installed with data retained and checked in the Pixel 7 Pro's dock and Hub header.
See the [logo-in-bag validation receipt](store-logo-validation.json).
