# Working on Home

Follow the [repository rules](../AGENTS.md) and use the
[product walkthrough](../desktop/docs/code-walkthrough.md) to trace events.

- Run Home Cargo commands from `phone/`; its `.cargo/config.toml` selects the
  phone bundles. Use `mobile-only` for phone presentation on desktop and
  `mobile-apps` to link Reference/Sheets for testing.
- Put shared shell changes in `../crates/shell/`. Keep Home's Settings,
  platform clients and packaging here. Preserve the before/after shell event
  order in `src/main.rs` when changing `App::handle_event`.
- Keep Settings authority tied to the trusted compiled module. Preserve
  request attribution, freshness and correlation checks across the Rust/Java
  boundary. Update the relevant `settings_*_host.rs` tests with behavior changes.
- Verify platform changes against both the Java client and its Android
  contract. Handle accepted commands separately from observed platform state;
  show unavailable capabilities as unavailable.
- Use `../rom/docs/home-build.md` for Home/Bridge packaging. Validate on an
  assigned device with a separate test package, following the root's device
  rules. Record device evidence separately from desktop preview results.
- Run the root's phone/shared-shell checks after code changes. Update both
  README languages when build flags, platform behavior or supported entry
  points change.
- Custom Android test packages must load the manifest-declared application
  extension. Rebuild the pinned packager after runtime patches, and check
  edge Back from a hosted app after installation; a visible Home screen
  alone does not establish that the Java integration loaded.
