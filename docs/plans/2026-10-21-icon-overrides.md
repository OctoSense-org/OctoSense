# OctoSense icon overrides implementation plan

> Execute sequentially on the existing dedicated feature branch.

**Goal:** Ship the whole approved family, including C5 Night rose for Browser,
through an OctoSense-owned catalog overlay that survives Makepad updates.

**Architecture:** Load Makepad's catalog unchanged. Apply the explicit local
replacement list by app ID, preserving unknown upstream entries. Local family
identities take precedence over system bundle art in every style; installed
`hub:` identities retain their own artwork. Keep the shared platform frame
policy and original Rinx PNG. No Makepad source, theme, runtime patch or pin is
changed.

**Technology:** Rust/Makepad, SVG and Python asset generation.

1. Change the existing bundle-precedence regression to require explicit local
   identities in all styles; run it and observe failure. Add synthetic-catalog
   coverage for upstream replacement/addition, stable identity counts and
   idempotent overlay application.
2. Put replacement merging and identity ownership in `icon_family.rs`; keep one
   overlay call in `style.rs`. Replace the fixed upstream icon-count assertion
   with a union of upstream and local IDs.
3. Promote C5's approved geometry into `tools/build_icon_family.py`, draw its
   Omarchy outline, refresh shipping assets and review sheets, and add Browser
   GPU checks for the dark dial and four-spoke rose.
4. Run focused shell tests, resource checks, both packaging checks and dependency
   graph checks; build/capture the hidden production preview and inspect it.
   Rinx-linked checks use separate empty temporary data directories.
5. Update bilingual design docs with ownership, precedence, Makepad update steps,
   C5 approval and current validation evidence. Preserve unrelated `local/` data.

Commands in this plan are intentions until their execution is recorded. Device
rendering remains unverified. No push, PR or merge is part of this request.
