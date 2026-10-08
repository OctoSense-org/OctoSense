# Unified native runtime

Every AppCard uses the release of
[Octoscript-Makepad](https://github.com/OctoSense-org/Octoscript-Makepad)
selected by `native-runtime.lock.json`. That framework owns `runtime.json`,
which fixes the underlying Makepad and Octoscript commits. Applications do not
carry alternate Makepad branches or compatibility patches.

In the OctoSense repository the release's checkouts live in `.sources/` at
the repository root, prepared by `python3 tools/setup.py` from the root:

```text
OctoSense/
  .sources/
    octoscript-makepad/     # shared UI framework; runtime.json owns engine pins
    octoscript/             # VM packages at the framework's revision
    makepad/                # native platform at the framework's revision (plus the reviewed patch)
  native-runtime.lock.json  # the release (apps/appcard's copy names the same one)
  apps/appcard/             # the octos-app runtime (app/), personal-data/, tools/
```

AppCard's crates are members of the root workspace; the root `Cargo.toml`
patches Makepad, Octoscript and Octoscript-Makepad to `.sources/`, and
`python3 tools/setup.py --check --cargo` verifies that Cargo resolves a single
Makepad VM/platform/draw/widgets source. The root `.cargo/config.toml` sets
`OCTOSENSE_WORKSPACE=.sources`, so `app/app/build.rs` embeds framework
resources from there. Git consumers set the same variable in their Cargo
configuration.

`apps/appcard/tools/setup-native.py` still prepares or checks a workspace of
sibling checkouts (`--update` moves clean ones; dirty trees and custom Cargo
configuration are preserved). It resolves the workspace from
`OCTOSENSE_WORKSPACE`; outside cargo the default is the parent of the
repository root, so point it at `.sources` (`OCTOSENSE_WORKSPACE=<repo>/.sources`).
Its `--cargo-manifest app/Cargo.toml` check named the former AppCard
workspace manifest, which the move removed (**unverified**: use
`tools/setup.py --check --cargo` instead).

Before 2026-09-27 AppCard lived in the OctoSense-System-Apps repository, and
the workspace was the parent of that checkout, with `makepad/`, `octoscript/`
and `octoscript-makepad/` beside it.

Mail's `scripts/setup_native.py` (in OctoSense App Flow, formerly Design Flow) calls the same setup. Its default runtime root
is the organization workspace. `OCTOS_MAIL_NATIVE_ROOT` may select an isolated
copy, but it must use the same AppCards runtime release and engine commits.
The WASM builder also consumes this release in both `existing` and `isolated`
modes. Neither mode applies application-specific runtime patches.

Update the framework first, verify its native and browser behavior, then update
AppCards' framework commit. CI prepares that exact release before compiling the
Android/desktop client and checks the resolved Makepad source graph. The old
`aichat` Makepad submodule is retired.

Native UI checks use standalone release binaries, Makepad's built-in HTTP
instrument and hidden Metal windows. They do not use Studio. Close owned test
instances through `/gq` and verify exit. See
[the instrument runbook](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/core/NATIVE-INSTRUMENT.md).
Historical evidence retains the source paths and hashes from its original run.
