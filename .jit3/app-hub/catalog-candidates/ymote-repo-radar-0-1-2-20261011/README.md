# Repo Radar 0.1.2 catalog candidate

English | [简体中文](README.zh-CN.md)

Prepared for [submission #202](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/202).
This candidate replaces the unadmitted 0.1.0 proposal; it changes no public
catalog or historical release. Production awaits consumer acceptance and a
compatible published OctoSense host with D3 and the WASI HTTPS fix.

## Verified release

[GitHub run 38094354008](https://github.com/ymote/octosense-repo-radar/actions/runs/38094354008) rebuilt the tagged Rust source,
passed 17 unit tests, one doctest, 14 harness tests, fmt and clippy, and attested
[version 0.1.2](https://github.com/ymote/octosense-repo-radar/releases/tag/v0.1.2) at `e18eb6a88b74398c5df2472d6fd5c0a3d6a1d50b`.

- Pack SHA-256: `62fb0262fa9139675abbd286f71d23efd385eb16697ec6341c12893d216067c3`.
- Bundle BLAKE3: `da4272cead19e613638d8381c47662ef4cff3716838c7e719916d779f3c9edf9`.
- Component SHA-256: `d0ee1835711ed4b1c952f564789d96c160d02035b1d2104db1388e6affdbc757`.

Independent download verification passed the intended GitHub identity, remote
tag, publisher proof, gate, exact file-set comparison and candidate preparation.
The component is 497,521 bytes, has nine typed exports and lists 18 crates.
The advisory scan found no affected listed crate; database revision and precise
checks are in `prior-review.json`. Only sealing metadata/default serialization and
the CI-rebuilt component differ from the tag. No sealed files were changed.

## Behavior and limits

Public GitHub search, private watchlists, recent PR samples, native D3 charts,
release notes and explicit Glance summaries require no account. Version 0.1.2
has the redesigned 0.1.1 runtime with an added listing disclosure: optional Chat
is a preview; live model replies and agent tool execution are unverified.
All nine tools are foreground-only and non-shareable; agent unpin requires host
review. The listing claims macOS only and requires `charts.d3@1`.

Source UX evidence is linked in `prior-review.json`. It does not prove execution of
this sealed version, a second-version update, or public-store installation.
Those claims require separate receipts. RC4 cannot install this app.

A separate [sealed-release acceptance](https://github.com/ymote/octosense-repo-radar/blob/ed221f103ef9e184451538bf6a7e9aeeac3d68fd/review/acceptance-release-0.1.2.json)
passed all 93 native steps with the previous signed rehearsal and a development
host. It verified real GitHub HTTPS, charts, watchlist persistence and exact
installed release bytes. Independent visual review passed within that scope;
this is not released-host or public-store acceptance. The RC5 package build is
[run 38095460684](https://github.com/OctoSense-org/OctoSense/actions/runs/38095460684).
`review.json` records this date refresh and links the separate acceptance.

## Reviewed transaction

The candidate retains all 15 existing entries unchanged and proposes sequence
16 on authenticated public sequence 15. Prepared payload SHA-256:

`b12e9e24022f6e7dbac177cc997a994b968c7ccfcb21a475c3217c6cedc63be3`

After review and merge, dispatch the protected catalog workflow for
`ymote-repo-radar-0-1-2-20261011`, its exact commit and digest, first with `dry_run: true`.
Verify the signed rehearsal and complete consumer acceptance before production.
Re-prepare and review if the public base or UTC date changes. Preserve other
candidates and all history. See [GitHub publication](../../docs/GITHUB-PUBLISHING.md).

UTC publication-date refresh for 2026-10-11; only the catalog date changed. Original release review is preserved as `prior-review.json`.
