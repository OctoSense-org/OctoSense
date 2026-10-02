# Working on the desktop product

Follow the [repository rules](../AGENTS.md). Use the
[product walkthrough](docs/code-walkthrough.md) to trace startup and hosting.

- Put shared window-manager, app-hosting and UI changes in `../crates/shell/`.
  Keep `src/main.rs` as the desktop entry point; keep desktop catalogs,
  resources and packaging scripts here.
- Change native app features, grants and per-platform hosting in
  `../native-apps.json`, then regenerate with `python3 tools/native_apps.py`
  from the root. Check the generated graph with
  `python3 tools/native_apps.py --check`.
- Run desktop Cargo commands from the root or `desktop/`. Run Home commands
  from `phone/` so Cargo loads its system-app selection.
- Use `MAKEPAD_WM_TEST_APP=<id>` to launch a test app at startup and `--module
  <id>` to select module hosting. Use hidden windows and the Makepad remote
  control surface for UI checks.
- When changing process-agent integration, follow `peer_link` and test context
  ownership, process death, cancellation and confirmation acknowledgements.
  Grant `agent.octos` explicitly in the native manifest before expecting a peer.
- Run the root's desktop/shared-shell checks for code changes. Update both
  README languages and the corresponding walkthrough when startup behavior,
  feature defaults or tool grants change.
