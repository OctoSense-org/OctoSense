# Working on Home

Follow the [repository rules](../AGENTS.md) and the
[product source walkthrough](../desktop/docs/code-walkthrough.md).

- Run Home Cargo commands from `phone/`: `.cargo/config.toml` selects its
  system bundles. `mobile-only` chooses phone presentation on desktop;
  `mobile-apps` links extra native modules. They are not synonyms.
- Keep shared shell changes in `../crates/shell/`. Home's `App` wraps the
  shared `App` and adds Settings; preserve the before/after event ordering in
  `src/main.rs` when changing platform integration.
- Settings is privileged through its compiled trusted module identity, not a
  script-provided name. Preserve request attribution, freshness, correlation
  and observed-state checks at Rust/Java boundaries.
- The octos system agent, System Bridge and ROM `AgentPlatformService` are
  distinct components. Binder permissions are not LLM tool grants. Standalone
  Home does not gain the ROM's platform signature or permissions.
- APK building, installation and ROM flashing are separate operations. Follow
  `../rom/docs/home-build.md` for the supported Home/Bridge build. Use only an
  assigned device and a separate test package; no device result may be inferred
  from desktop preview or compilation.
- Apply the root's phone checks to code changes. For doc-only changes, check
  paths, features and links, keep README languages paired, and label unrun
  build/device instructions unverified.
