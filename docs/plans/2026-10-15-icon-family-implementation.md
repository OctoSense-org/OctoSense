# Approved icon family implementation plan

**Goal:** Connect the 28 approved SVG icons and original Rinx PNG to both shell packagings.

**Architecture:** Embed native and optional app artwork in the shared shell catalog. Preserve the shared platform frame policy and bundle precedence. Mail and Calendar also carry their approved SVGs in their system bundles. Rinx uses its unchanged original PNG through the existing image mask shader.

**Technology:** Rust/Makepad, SVG, PNG, Python asset generation.

1. Extend `crates/shell/src/octosense/style.rs` regression coverage to require approved app IDs, preserve custom identities, validate frame variants and parse new SVGs with the actual renderer. Check bundle icon admission through existing system app tests. Run focused tests before implementation to establish failures.
2. Extend `tools/build_icon_family.py` to install/check approved sources under `crates/shell/resources/icons/apps/family/`, copy Mail/Calendar bundle SVGs and copy original Rinx PNG with attribution. Keep proposal sources and shipping copies synchronized.
3. Register the SVGs in the shared shell style catalog and support their explicit square background in `icon_frame::styled_svg`. Keep freeform backgrounds absent; preserve classic artwork where it already exists. Add original Rinx PNG rendering through `AppIconDraw` before the generic SVG fallback.
4. Run focused tests and asset checks, then the required shared shell tests and desktop/Home packaging checks. All Rinx-linked runs use separate empty data directories outside the checkout and hidden windows.
5. Capture the existing `icon_shapes` example and a family preview through the production renderer. Inspect Android/macOS/iOS, small icons and transparent PNGs. Devices remain unverified unless a device is explicitly assigned.
6. Update paired design guides and root README entry points to describe implemented source behavior and exact validation. Keep this change on the existing dedicated branch.

Commands listed here are planned until their executions are recorded in the design guide or local work log. No deployment, installed app replacement or external dependency pin changes are required.
