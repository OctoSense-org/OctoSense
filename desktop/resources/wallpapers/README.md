# Wallpaper sources

## Abyssal Currents — OctoSense desktop

`octosense-abyssal-currents.png` is an original image generated for OctoSense
on 2026-09-11 with the built-in image generation tool. It replaces the glass-demo
vector scene. Dark sculpted oceanic folds, restrained teal light and copper
edges leave quiet space for windows and icons.

`octosense-abyssal-currents-light.png` is its light appearance companion, generated
on the same date by editing the dark original with the built-in image tool.
It preserves the curled crest, flowing contours and framing, with pearl ridges,
pale aqua recesses and soft champagne highlights.

- Light dimensions: 1672 × 941 pixels (native generated output; not upscaled).
- Light size: 2,363,947 bytes; PNG copied without conversion.
- Light SHA-256: `2e6493c6c5738e194b129f1569620af6d7ad0a7e3f40aa5966e7bae00d67451b`.
- Full edit prompt: [abyssal-currents-light-prompt.txt](abyssal-currents-light-prompt.txt).

The **Light / Dark** control automatically selects the matching image.
Both images are bundled; the following record describes the dark original:

- Saved dimensions: 1672 × 941 pixels (native generated output; not upscaled).
- Size: 2,244,284 bytes; PNG copied without conversion.
- SHA-256: `ac8b9eddf58abb44c001b581530adb16eeb00166a5e71560cc12d9191f6af8f3`.
- Full generation prompt: [abyssal-currents-prompt.txt](abyssal-currents-prompt.txt).
- Embedded in the executable and rendered by Makepad's existing Image widget
  with centered crop-to-fill sizing; no runtime image download is needed.
- Android's animated background is unchanged. This wallpaper belongs to the
  OctoSense desktop style; Omarchy keeps its selected theme image.

The removed `octosense.svg` came from the guofoo/makepad WM fork at
`beb3857aea22a6a99fb4a7b6a3b60f92359f6a4d`. Its original source path and hash
remain recorded under `retained_fork_assets.replaced_files` in
`upstream/makepad.json`; it is no longer bundled.

## Tokyo Night — Omarchy

`tokyo-night.webp` is the unmodified `0-winding-road.webp` from Omarchy’s Tokyo Night theme. It is embedded in the executable as the offline fallback when that theme has no installed images. The other Omarchy wallpapers remain optional downloads.

- Source repository: https://github.com/omacom/omarchy
- Source revision: `5b91db503c904bbfc5f34bdaaa9c708814958f3d` (quattro, checked 2026-09-08)
- Source file: https://github.com/omacom/omarchy/blob/5b91db503c904bbfc5f34bdaaa9c708814958f3d/themes/tokyo-night/backgrounds/0-winding-road.webp
- Git blob: `2f1f8d539f9b5fb1cb170e0f154386cdc730dc58`
- SHA-256: `b149c3e1c8ce383812e7bc3170c2cf8b160ca583d72d59173cd42efdb79ced2b`
- Size: 653,482 bytes; 6016 × 3384 pixels.
- Upstream repository license: [MIT, copyright David Heinemeier Hansson](LICENSE.omarchy).

This asset is maintained locally, independently of the Makepad WM sync. To update it, copy the desired upstream file without conversion, retain the applicable license, update this record, and run the native startup/style smoke checks.
