# Fork WM features implementation plan

> Historical record from before the OctoSense rename. Original names, commands and artifact paths are retained for traceability.

**Goal:** Adopt the WM features from guofoo/makepad at beb3857aea22a6a99fb4a7b6a3b60f92359f6a4d and continue standalone WM development in MakeOS.

**Architecture:** Merge completed work into main first, then three-way merge apps/wm against the existing ae20efc5 baseline. Use the published fork revision for all Makepad Git crates; copy only WM assets/source and the license. Record the fork as the new pristine baseline, preserving standalone adaptations and the existing daily review workflow.

**Tech stack:** Rust, Makepad widgets/Splash/Metal, Python maintenance and native smoke tests.

## Accepted scope and decisions

- Main fast-forwards from f157660 to 8b2dc9c before feature work. No other local feature branches require merging.
- Preserve the catalog, lean default features, Reference app, state isolation, opt-in startup, diagnostics, process shutdown, fonts and run-view area fix.
- Import StyleSpec, the eighth MakeOS style, Liquid Glass shell surfaces, rounded child surfaces, wallpaper, material parsing, and backdrop ordering fixes with their tests.
- Keep Omarchy as the startup style, matching both source applications. MakeOS is selectable in the appearance menu.
- Remove the temporary WM rendering workarounds now that pinned widgets provide cached redraw and safe snapshot detachment.
- Pin framework crates to https://github.com/guofoo/makepad.git at beb3857a (publication checked). Do not vendor widgets or unrelated apps.
- Record default_source as ../guofoo-makepad, relative to the project root, with --source still taking precedence and legacy manifests defaulting to ../makepad. The fork must incorporate official updates before daily sync; switching to an official-only commit would drop required framework APIs.
- No source checkout changes and no pushes. Validate and integrate the resulting work into local main for continued development.

## Execution

1. Merge the completed sync, create ignored feature worktree, and preserve the previous verified baseline (191 Rust and 40 Python tests plus native smoke).
2. Add failing maintenance fixtures for recorded default source, CLI parity and explicit override. Implement common source selection and ensure all existing fixtures pass.
3. Use scripts/upstream.py comparison at the frozen fork commit to import only mapped WM files. Resolve overlaps explicitly, keep standalone adaptations, and adapt monorepo-only theme test paths to the public StyleSheet API.
4. Change both Cargo manifests and provenance together, regenerate Cargo.lock through Cargo, audit every source hash and dependency pin. Document the new source and imported features.
5. Run cargo check --locked --workspace, cargo test --locked --workspace --quiet, Python fixtures, release/debug builds and both native smoke modes. Exercise MakeOS glass frames, dock, menus, panels, notifications and all style transitions while preserving a hosted app.
6. Review the diff and validation evidence, correct material defects, commit the complete import and fast-forward local main. Confirm source checkout unchanged and working tree clean.

## Validation commands

```sh
python3 -m unittest discover -s scripts -p 'test_*.py'
cargo check --locked --workspace
cargo test --locked --workspace --quiet
cargo build --release --locked --workspace
cargo build --locked --workspace
python3 scripts/smoke.py --artifacts-dir target/fork-validation/smoke-release
python3 scripts/smoke.py --cargo-run --default-catalog --artifacts-dir target/fork-validation/smoke-default
python3 scripts/upstream.py status --source /Users/guofoo/git/mp/guofoo-makepad
git diff --check
```

Native tests require GUI access and isolate their own state and processes. Rendering must be verified at runtime; unit tests alone do not compile every shader or validate GPU capture ordering.
