# Working on the desktop product

The [repository rules](../AGENTS.md) apply. Read the
[product source walkthrough](docs/code-walkthrough.md) before changing startup
or documenting native/script app hosting.

- `src/main.rs` is the entry point; shared UI and hosting belong in
  `../crates/shell/`, not copies under this directory.
- Run desktop Cargo commands from the root or this directory. Home commands
  must run from `../phone/` to pick up its Cargo environment.
- `--module` selects native hosting; startup launch can be requested with
  `MAKEPAD_WM_TEST_APP`. Match Cargo features to `../native-apps.json`.
- App Hub's Card runner is not the optional AppCard assistant. A native module,
  a child process, an app-agent peer and a Tokio task are different concepts.
- For code changes, use the desktop and shared-shell checks named in the root
  instructions. For documentation changes, inspect referenced symbols and
  relative links; mark commands not executed as unverified. Change both README
  languages together. Use hidden windows for UI validation.
