# light engine develop controls

The lightcraft engine's develop controls at revision 2472021091a2: 114 controls, the keys of `params` for light.develop and light.batch, one per line as `id` label: min to max, default. Generated from the engine by `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-light-service --test skill`; do not edit.

## bwMix

- `bw.aqua` Aqua: -100 to 100, default 0
- `bw.blue` Blue: -100 to 100, default 0
- `bw.green` Green: -100 to 100, default 0
- `bw.magenta` Magenta: -100 to 100, default 0
- `bw.orange` Orange: -100 to 100, default 0
- `bw.purple` Purple: -100 to 100, default 0
- `bw.red` Red: -100 to 100, default 0
- `bw.yellow` Yellow: -100 to 100, default 0

## calibration

- `calibration.blueHue` Blue Hue: -100 to 100, default 0
- `calibration.blueSat` Blue Saturation: -100 to 100, default 0
- `calibration.greenHue` Green Hue: -100 to 100, default 0
- `calibration.greenSat` Green Saturation: -100 to 100, default 0
- `calibration.redHue` Red Hue: -100 to 100, default 0
- `calibration.redSat` Red Saturation: -100 to 100, default 0
- `calibration.shadowsTint` Shadows Tint: -100 to 100, default 0

## color

- `color.saturation` Saturation: -100 to 100, default 0
- `color.vibrance` Vibrance: -100 to 100, default 0
- `wb.temp` Temp: 2000 to 50000, default 6500
- `wb.tint` Tint: -150 to 150, default 0

## curve

- `curve.darks` Darks: -100 to 100, default 0
- `curve.highlights` Highlights: -100 to 100, default 0
- `curve.lights` Lights: -100 to 100, default 0
- `curve.refineSaturation` Refine Saturation: 0 to 100, default 100
- `curve.shadows` Shadows: -100 to 100, default 0
- `curve.splitHighlights` Highlights split: 30 to 90, default 75
- `curve.splitMid` Midtones split: 20 to 80, default 50
- `curve.splitShadows` Shadows split: 10 to 70, default 25

## detail

- `detail.nrColor` Color Noise Reduction: 0 to 100, default 0
- `detail.nrColorDetail` Detail: 0 to 100, default 50
- `detail.nrColorSmoothness` Smoothness: 0 to 100, default 50
- `detail.nrContrast` Contrast: 0 to 100, default 0
- `detail.nrDetail` Detail: 0 to 100, default 50
- `detail.nrLuminance` Noise Reduction: 0 to 100, default 0
- `detail.sharpenAmount` Sharpening: 0 to 150, default 0
- `detail.sharpenDetail` Detail: 0 to 100, default 25
- `detail.sharpenMasking` Masking: 0 to 100, default 0
- `detail.sharpenRadius` Radius: 0.5 to 3, default 1

## effects

- `effects.clarity` Clarity: -100 to 100, default 0
- `effects.dehaze` Dehaze: -100 to 100, default 0
- `effects.texture` Texture: -100 to 100, default 0

## geometry

- `crop.angle` Straighten: -45 to 45, default 0
- `geometry.aspect` Aspect: -100 to 100, default 0
- `geometry.horizontal` Horizontal: -100 to 100, default 0
- `geometry.offsetX` Offset X: -100 to 100, default 0
- `geometry.offsetY` Offset Y: -100 to 100, default 0
- `geometry.rotate` Rotate: -10 to 10, default 0
- `geometry.scale` Scale: 50 to 150, default 100
- `geometry.vertical` Vertical: -100 to 100, default 0

## grading

- `grading.balance` Balance: -100 to 100, default 0
- `grading.blending` Blending: 0 to 100, default 50
- `grading.global.hue` Global Hue: 0 to 360, default 0
- `grading.global.lum` Global Luminance: -100 to 100, default 0
- `grading.global.sat` Global Saturation: 0 to 100, default 0
- `grading.highlights.hue` Highlights Hue: 0 to 360, default 0
- `grading.highlights.lum` Highlights Luminance: -100 to 100, default 0
- `grading.highlights.sat` Highlights Saturation: 0 to 100, default 0
- `grading.midtones.hue` Midtones Hue: 0 to 360, default 0
- `grading.midtones.lum` Midtones Luminance: -100 to 100, default 0
- `grading.midtones.sat` Midtones Saturation: 0 to 100, default 0
- `grading.shadows.hue` Shadows Hue: 0 to 360, default 0
- `grading.shadows.lum` Shadows Luminance: -100 to 100, default 0
- `grading.shadows.sat` Shadows Saturation: 0 to 100, default 0

## grain

- `grain.amount` Grain: 0 to 100, default 0
- `grain.roughness` Roughness: 0 to 100, default 50
- `grain.size` Size: 0 to 100, default 25

## light

- `light.blacks` Blacks: -100 to 100, default 0
- `light.contrast` Contrast: -100 to 100, default 0
- `light.exposure` Exposure: -5 to 5, default 0
- `light.highlights` Highlights: -100 to 100, default 0
- `light.shadows` Shadows: -100 to 100, default 0
- `light.whites` Whites: -100 to 100, default 0

## mixer

- `mixer.aqua.hue` Aqua Hue: -100 to 100, default 0
- `mixer.aqua.lum` Aqua Luminance: -100 to 100, default 0
- `mixer.aqua.sat` Aqua Saturation: -100 to 100, default 0
- `mixer.blue.hue` Blue Hue: -100 to 100, default 0
- `mixer.blue.lum` Blue Luminance: -100 to 100, default 0
- `mixer.blue.sat` Blue Saturation: -100 to 100, default 0
- `mixer.green.hue` Green Hue: -100 to 100, default 0
- `mixer.green.lum` Green Luminance: -100 to 100, default 0
- `mixer.green.sat` Green Saturation: -100 to 100, default 0
- `mixer.magenta.hue` Magenta Hue: -100 to 100, default 0
- `mixer.magenta.lum` Magenta Luminance: -100 to 100, default 0
- `mixer.magenta.sat` Magenta Saturation: -100 to 100, default 0
- `mixer.orange.hue` Orange Hue: -100 to 100, default 0
- `mixer.orange.lum` Orange Luminance: -100 to 100, default 0
- `mixer.orange.sat` Orange Saturation: -100 to 100, default 0
- `mixer.purple.hue` Purple Hue: -100 to 100, default 0
- `mixer.purple.lum` Purple Luminance: -100 to 100, default 0
- `mixer.purple.sat` Purple Saturation: -100 to 100, default 0
- `mixer.red.hue` Red Hue: -100 to 100, default 0
- `mixer.red.lum` Red Luminance: -100 to 100, default 0
- `mixer.red.sat` Red Saturation: -100 to 100, default 0
- `mixer.yellow.hue` Yellow Hue: -100 to 100, default 0
- `mixer.yellow.lum` Yellow Luminance: -100 to 100, default 0
- `mixer.yellow.sat` Yellow Saturation: -100 to 100, default 0

## optics

- `optics.caBlue` Blue/Yellow Fringe: -100 to 100, default 0
- `optics.caRed` Red/Cyan Fringe: -100 to 100, default 0
- `optics.defringeGreen` Green Amount: 0 to 20, default 0
- `optics.defringeGreenHueHi` Green Hue High: 10 to 100, default 60
- `optics.defringeGreenHueLo` Green Hue Low: 0 to 90, default 40
- `optics.defringePurple` Purple Amount: 0 to 20, default 0
- `optics.defringePurpleHueHi` Purple Hue High: 10 to 100, default 70
- `optics.defringePurpleHueLo` Purple Hue Low: 0 to 90, default 30
- `optics.distortion` Distortion: -100 to 100, default 0
- `optics.profileDistortion` Profile Distortion: 0 to 200, default 100
- `optics.profileVignetting` Profile Vignetting: 0 to 200, default 100
- `optics.vignetting` Vignetting: -100 to 100, default 0
- `optics.vignettingMidpoint` Midpoint: 0 to 100, default 50

## profile

- `profile.amount` Amount: 0 to 200, default 100

## vignette

- `vignette.amount` Vignette: -100 to 100, default 0
- `vignette.feather` Feather: 0 to 100, default 50
- `vignette.highlights` Highlights: 0 to 100, default 0
- `vignette.midpoint` Midpoint: 0 to 100, default 50
- `vignette.roundness` Roundness: -100 to 100, default 0
