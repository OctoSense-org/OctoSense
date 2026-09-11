# Official Makepad work integration

Update the external framework and WM baseline to official Makepad `74b63be83e101ab3a28d3604df77e9662d50a833`. Preserve all MakeOS features and uncommitted Android fixes.

1. Save tracked and nonignored untracked contents and starting Git state under `target/upstream-20260911/before`; compare both repositories in a disposable union Git database.
2. Use common ancestor `7535ce8d4c5a81a68c0b28993b8bbdca5b612ee0` to identify official WM changes independently from fork additions. Follow the Studio migration through to the public Director app; retain the studio catalog ID and custom catalog loader.
3. Pin all Makepad Git crates, including `makepad-wm-api`, `makepad-wm-theme`, optional app modules and Reference dependencies, to the official commit. Neither wm library changed since the existing pin; both remain external.
4. Preserve the MakeOS style with a local enum/adapter and its two small theme files. Send a recognized macOS dark wire family plus the complete MakeOS palette/material to stock upstream apps. Keep SVG cover behavior in a local wallpaper widget; safe cached-view APIs already exist upstream. Clear retired pass roots through the public API to accommodate the new GPU working-set scan.
5. Move catalog app manifests to `../makepad`, update provenance to official pristine hashes, and record fork-only assets as locally owned with their original source hashes. Update daily sync instructions.
6. Validate candidate lock/pins, Rust and Python tests, all-style real GPU hosting and default catalog launch. Build/install Android and inspect Reference touch behavior. Check iOS compilation if the updated framework allows it.
7. Apply only candidate changes after comparing each affected live file to the starting snapshot. Leave all existing and new changes uncommitted on main for review.

The source checkouts are read-only. No fork publication, unrelated app source imports, or vendored framework crates are needed. Artifacts and exact comparison evidence remain under the task report directory.
