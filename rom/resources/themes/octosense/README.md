# OctoSense widget theme

`theme.splash` and `widgets.splash` preserve the dark palette and glass material from
`guofoo/makepad` at `beb3857aea22a6a99fb4a7b6a3b60f92359f6a4d`, originally
`widgets/themes/makeos/`. Exact original hashes are in
`upstream/makepad.json` under `retained_fork_assets`. They are maintained locally
and embedded by `src/octosense/style.rs`; no runtime resource path is required.

`theme-light.splash` and `widgets-light.splash` are the local light companion:
pearl surfaces, pale aqua glass, dark ink text and blue accents. Both appearances
share the same rounded geometry, spacing and lensing. Each loads a matching
Abyssal Currents wallpaper. Select OctoSense, then click **Light / Dark** in the
top bar; **OctoSense Dark** is also searchable in the desktop style menu.

Hosted applications receive the complete palette through the upstream `macos`
or `macos-dark` style family. Shell colors are derived from the same files;
switching away restores the selected Omarchy shell tokens. Source checkouts
reload these files when the style is selected; installed builds use embedded data.

The original Makepad MIT license is retained in `LICENSES/Makepad-MIT.txt`.
The local style adapter also derives from that fork's behavior. The original
Abyssal Currents wallpaper now uses the standard Image widget; its provenance
is documented in `resources/wallpapers/README.md`.
