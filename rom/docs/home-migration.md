# Home source migration

Date: 2026-09-21. Destination: `OctoSense-org/octosense-rom`.

## Imported source and maintained boundaries

Home is imported under `home/` using an unsquashed Git subtree merge. The mobile
main revision `653cd67b10c938f690933090c9f3f5d78ab91fae` and its original ancestors
remain reachable in the ROM history; the ROM history is also retained. Work now
builds from the ROM checkout. Android sources remain in `home/android/` for this
first migration so their existing resource and validation paths stay coherent.

The App Hub integration is ported from mobile commit
`d4efe16b1595ac684e55e596383161c8b4778df4` (PR #39, merged into the calendar branch,
not mobile main). The client dependency selects App Hub
`97c2a1fd9aa49a6b87586f228e070e0c16b1067b`. This preserves current main's Maps,
Photos, News, camera fixes, launcher gestures and native integration. Installed
cards refresh the launcher catalog and keep distinct client identities.

The pinned runtime remains `b0628d05a89369b0c3bae2750db6da06996a05c2`, including
Makepad's Android fixes at `825dbb422c6d7926e111e2ee7831d697870d8671`. The App Hub
prototype depended on additional isolate-policy APIs from Makepad commit
`0f88d2286698dc8d2a9c2b994e4ac2c7873af582`; its policy patch plus the
device fixes listed in `home/runtime-patches.lock.json` are carried in
`patches/runtime/` with a hash and resulting tree lock. Bootstrap validates both
the upstream base and this patch. It does not depend on uncommitted framework
worktrees or silently disable policy enforcement.

Signing, Android package IDs, data locations, signature permissions and existing
ROM platform imports are preserved. The ordinary and ROM builds produce distinct
Home/Bridge pairs. No cross-signer migration is part of this change. The ROM
release/update URL remains under `OctoSense-org/octosense-rom`.

## Work preserved outside this migration

The original worktrees were not reset, moved or deleted. The old repository must
not be archived until the following work is reconciled and the replacement
release is accepted:

| Work | Snapshot inspected | Disposition |
| --- | --- | --- |
| Calendar / mobile PR #11 | `c6e816bea3ab57557d63dd01872ec94f72eeef8e` | App Hub wiring ported; remaining Calendar changes still need review. |
| OpenHarmony / mobile PR #10 | `78e192c5f8f529428e15025c363472cb5a825717` | Native-mobile module selection and pinned nix ABI fix ported. AppCard advances to `025105c378f1ca44be00937b252e2b169d15b577` for the missing embedded transport. Mate 70 Home builds and runs from this repository. |
| Main local mobile worktree | `45dbbfbf257c05a7c2d5149b21ebe77f5c71013c` plus local changes | Active work preserved in place; not treated as a clean release baseline. |

The separate mobile repository is a transition/archive source, not a dependency
or second product in the combined build. After the migration lands and outstanding
work is transferred, place a migration notice there and archive it. Do not delete
the repository's history or silently discard pending PRs to achieve that state.

## Validation and release gates

Local validation on macOS used the pinned dependencies bootstrapped by this
repository, an existing Android SDK/NDK, Gradle 8.11.1 and full JDK 17:

- Home and all mobile modules compile with the locked Cargo graph.
- Home plus App Hub policy/catalog tests: 337 passed (295 Home, 42 Hub/policy).
- Makepad isolate policy and network gate tests: 11 passed.
- Product build/staging/source-preservation tests: 14 passed.
- Standalone Home and Bridge APKs build and verify with matching development
  certificates; ROM Home and Bridge build with the existing platform certificate.
- Runtime graph verification rejects duplicate/foreign Makepad crates.

The full-feature Home suite exposed an existing shade fixture that pretended to
be an AppCard turn. With AppCard linked, the live-turn poller finished that
fictional activity before the final assertion. The fixture now uses its own
producer identity; production island behavior is unchanged.

The Android builds use an already installed compatible `cargo-makepad` through
the explicit `--packager` option. Receipts record that override. Existing Rust
configuration/dead-code warnings and Java deprecation warnings remain; the
builds do not claim a warning-free baseline.

Home/Bridge updates have now been installed on the existing OnePlus OctoSense
ROM, and Home runs as a normal application on the Mate 70 Air. No full ROM was
flashed or built. These devices do not provide unrooted Android acceptance:
the OnePlus has privileged ROM integration, and the Mate runs OpenHarmony.
See [device validation](home-device-validation.md) for measured results and
remaining release gates. App Hub's non-atomic replacement and Android runtime
containment remain production acceptance items.
