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

The pinned runtime is Octoscript-Makepad main `c4c9682219d5bb549856e35086adf1b354844dc3`,
selecting Makepad main `1d3d383e84a66dbb18a4a860f505430c9d5b20f4`. It
contains the isolate policy App Hub needs (Makepad #22) and the device fixes this
product used to carry as `patches/runtime/makepad-isolate-policy.patch` (Makepad
#26), so no runtime patch is applied. `home/runtime-patches.lock.json` stays as
the empty, reviewed place for a future patch. Bootstrap does not depend on
uncommitted framework worktrees or silently disable policy enforcement.

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
| Calendar / mobile PR #11 | `c6e816bea3ab57557d63dd01872ec94f72eeef8e` | Calendar module hosting outstanding (`home/BACKLOG.md`); everything else is present. |
| OpenHarmony / mobile PR #10 | `78e192c5f8f529428e15025c363472cb5a825717` | Fully ported. Native-mobile module selection and pinned nix ABI fix ported. AppCard advances to `025105c378f1ca44be00937b252e2b169d15b577` for the missing embedded transport. Mate 70 Home builds and runs from this repository. The last commit, `78e192c` itself, was cherry-picked in the second sync. |
| Main local mobile worktree | `45dbbfbf257c05a7c2d5149b21ebe77f5c71013c` plus local changes | Superseded: its `allowBackup="false"`, `phone_client_texture` and Mail hosting are already here. |

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
containment remain production acceptance items. The second sync below settles
the replacement gate.

## Second sync from mobile (2026-09-25)

Mobile main `65488bb9e84e17749d62ef12009e43fd8a6ae115` (PRs #44–#50) is merged
into `home/` by a second unsquashed subtree merge, "Merge OctoSense-mobile main
(65488bb) into home". It brings the swipe-start cue, the native App Hub,
generated portraits and Maps directions on Android 9 over rustls. The runtime
and AppCard pins from PRs #48–#50 were already mirrored here.

Decisions:

- Mobile's native App Hub (`home/apps/app-hub`, crate `octosense-app-hub-app`,
  module `apphub`) replaces the direct `octosense-appstore` wiring and its
  `app-appstore` feature. Installed card apps get `hub:<manifest-id>` launcher
  IDs. The data root is the same `<data_dir>/apps`, so installed bundles need
  no migration. App Hub's cfgs use `native_mobile`, so OpenHarmony still links it.
- Floating navigation stays on Android and OpenHarmony. Mobile's swipe cue is
  drawn only in the non-floating shell (the desktop phone shell and iOS); the
  Recents hint and the home dock follow whether the cue is drawn.
- The `.sources` dependency paths, `home/tools/setup-native.py` and the root CI
  workflow are kept. Mobile's `home/.github/workflows/runtime.yml` and
  `home/tools/test_setup_native.py` are not imported.

Review of the merge found three follow-up fixes, made on the same branch.
Installing or updating a Hub app refreshes the launcher's app list at once.
`clients::available_apps()` no longer reads the install directory twice. On
Android and OpenHarmony, where no cue is drawn, the Recents hint and home dock
keep their earlier wording and offsets.

Release gates: App Hub installs into a staging root and publishes the verified
bundle by renaming, keeping the previous bundle until the new one is in place
(`home/apps/app-hub/src/catalog.rs` then; now OctoSense-App-Hub
`crates/app-hub-app/src/catalog.rs`). This settles the non-atomic replacement
gate. Android runtime containment remains open. Installed Hub apps cannot yet be
placed on the Android home page (`HUB-01` in `home/BACKLOG.md`).

Work outside mobile main:

- PR #10's remaining commit `78e192c` ("mobile_app: the tick says why it asked
  for a frame") is cherry-picked. Its other commits were already here or are
  superseded by the AppCard `9e8e4898` pin.
- PR #11's Calendar module hosting is left as a follow-up (`home/BACKLOG.md`);
  its source is in OctoScript-App-Design-Flow (formerly Octoscript-AppCard) at
  [`apps/calendar/native`](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/tree/cbbda4da0a9d0fbf13497335dd3342b71f35e71f/apps/calendar/native).
- Unreferenced mobile commit `45dbbfb` is superseded: its `allowBackup="false"`,
  `phone_client_texture` and Mail hosting are already here.

Validation: the Home, App Hub, App Hub policy, Maps, News and AppCard tests
pass 589 locally, and the product tests pass 27. On the `macos-14` CI runner,
Maps' view and module tests (33) are skipped: 29 of them need `init_cx_os()`,
which traps off the main thread there (`RUNTIME-01` in `home/BACKLOG.md`).
A standalone development build was installed over the existing Home on a
OnePlus 6T (Android 9), keeping its data. Checked on the phone:

- Home starts and links `apphub` and `card`, with no crash or panic.
- App Hub opens. Offline, it shows the last verified cached catalog with the
  timeout reason. The preview catalog opens News.
- Floating navigation works, and no cue is drawn on Android.
- The Photos library and its albums are intact.
- Maps launches.

The phone had no network during that run. On 2026-09-26 it was online through
gnirehtet reverse tethering over USB, running the second sync's build with the
`HUB-01` fix:

- App Hub fetched and verified the live catalog (published 2026-09-20; it lists
  no apps yet).
- The native Maps module loaded tiles and found places. Directions from the
  phone's location to Santana Row, San Jose, returned all three modes (drive
  13 min / 5.7 mi, walk 2 hr 3 min, bike 40 min) over TLS 1.3 on Android 9.

The ROM variant was not built because the platform key is kept on the build
host.
