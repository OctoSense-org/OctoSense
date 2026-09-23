# App Hub

App Hub is the native OctoSense store: Today, Apps, Search, app details and
Library. The launcher includes it in Android and standard desktop builds through
the default `app-hub` feature. Installed apps run in the Hub's contained Card host and use
distinct `hub:<manifest-id>` shell identities.

The local Card presentation adapter delegates verification, policy, asset
serving and execution to the shared Hub module. It mounts the card below the
shell's status area, omits an empty error-notice row and draws custom widgets in
their owning isolate. Refusal messages remain visible.

The live catalog comes from the pinned
[OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) backend.
App Hub verifies its signatures, preserves the catalog sequence across restarts,
and requires fresh catalog data and permission consent before installation.
Downloads are staged before replacing an installed bundle. App data survives an
update. A successful update closes that app's running instances so the next
Open loads its new code, assets and permissions. Other apps stay open, and a
failed update leaves the current instance running. This also applies when
App Hub is closed while its download finishes. Withdrawn apps remain
visible in Library with an unavailable status.

The **Live catalog / Preview catalog** button switches sources explicitly.
Preview contains existing OctoSense built-ins and opens them directly. It never
adds listings to the live store or installs sample bundles. The production
catalog was empty when this feature was developed.

Preview icons resolve the same app identities and theme artwork as the launcher.
App Hub's canonical icon is declared in `listing.json` and stored at
`assets/icon.svg`; its native build embeds that declaration. This built-in file
contains only icon metadata, while published Card apps require a complete Hub
listing. Installed app icons use their local listing assets in the launcher and
Recents. Shared author-facing requirements live in the Hub's
[icon guide](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/ICONS.md).
The [local integration record](../../docs/design/app-hub/icon-standard-proposal.md)
describes this shell's resolver and migration.

## Develop an app for the Hub

Start with [Build your first Hub app](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/FIRST-APP.md)
and the [app starter](https://github.com/OctoSense-org/OctoSense-App-Hub/tree/main/templates/app).
The shared [development guide map](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/DEVELOPMENT.md)
links UI authoring, data/state, runtime setup and native testing. Follow
[Publishing](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/PUBLISHING.md)
for the complete listing and bundle contract.

Downloadable apps use the contained Card bundle format. Native Rust modules
are integrated into a shell release. The icon-only listing in this native
App Hub directory is not a publishable Card listing template.

## Run locally

Prepare the pinned native dependencies with `tools/setup-native.py` as described
in the repository setup documentation, then run from the repository root:

```sh
cargo run --release --features mobile-only
```

Open App Hub in the launcher's All apps page. To also embed the other preview
apps in the local phone shell, use `--features mobile-only,mobile-apps` and
`-- --module news --module maps --module photos --module sheets --module camera --module mail`.
A standalone native UI preview is also available:

```sh
cargo run --release -p octosense-app-hub-app --example preview
```

The standalone preview logs built-in launch requests; the shell performs real
app launches. Design provenance and native captures are in
[`docs/design/app-hub`](../../docs/design/app-hub/README.md).

## Exercise installation without publishing apps

Generate a fresh local signed catalog in an empty directory:

```sh
cargo run -p octosense-app-hub-app --example fixture -- target/app-hub/local-fixture
```

The generated `environment.json` contains `OCTOSENSE_HUB`,
`OCTOSENSE_HUB_ANCHOR` and `OCTOSENSE_APP_DATA`. Set those three variables in the
environment when launching the shell. They select the local catalog, its fresh
public trust anchor and an isolated installation directory. The signing keys
are generated in memory and are never written to disk or published.

The fixture includes Trail Notes and Focus Timer, with actual native Card
bundles and clearly identified validation artwork. Use **Get → Install → Open**
and verify both apps appear in Library. Remove these environment overrides to
return to the production store. Never distribute a release configured with a
fixture trust anchor.

## Checks

```sh
cargo test -p octosense-app-hub-app --lib --offline
cargo test -p octosense-app-hub-app --example fixture --offline
cargo test --release --bin octosense --features mobile-only,app-hub --offline
python3 -m unittest discover -s tools -p test_setup_native.py
```
