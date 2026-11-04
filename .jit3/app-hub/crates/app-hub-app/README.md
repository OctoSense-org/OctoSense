# octosense-app-hub-app — the shell integration

Every OctoSense shell (the phone's Home and the desktop, both in the OctoSense
repository) links this one crate to get App Hub: the native store, the
runner for card and script apps, the system apps the shell ships, the apps the
person installed, and their icons. It lives here, with the crates it builds
on, so that every shell links the same code at the same pin.

## What a shell gets

| Item | What it is |
| --- | --- |
| `APP_HUB_MODULE` | The native App Hub store as an `AppModule` (id `apphub`): Today, Apps, Search, app details and Library, backed by the signed catalog. Its executor answers the store's read-only agent tools (`search`, `installed` and `updates`, defined in `src/ai.rs`) from the catalog the view shows; installing stays on the store's own screens. |
| `CARD_MODULE` | The Card runner as an `AppModule` (id `card`). It runs a system app (`os.<name>`) or an installed app (`hub:<manifest-id>`) as its own client, mounted below the shell's status area, and draws a host sheet, such as a sign-in, over the app. Registering it first registers the system apps. |
| `system_apps()` | The system apps this build ships (OctoSense ADR 0004), each `{ id, name, pack, assets }`. The first call runs the generated `register_system_apps()`, which registers every selected pack and its compiled-in asset mounts with the runner. |
| `system_icon(short_id)` | A system app's own launcher art (`icon.svg` or `icon.png` in its bundle), by short id (`camera` for `os.camera`). |
| `installed_apps(root)`, `data_root_if_set()`, `data_root()`, `set_data_root()` | The apps App Hub installed, read fresh from the app-data root, so an install shows up without a restart; the `data_root` functions read and set that root. |
| `icons` | Installed-app icon lookup (`read_installed_icon`, `installed_icon_path`) and an icon `generation()` that bumps when icons change. |
| `catalog` | The verified catalog behind the store view, and `try_may_open_from_environment`, the check a shell runs before it opens an installed app. |
| `take_completed_installs()`, `AppHubAction` | What the store finished installing, for the shell to refresh its launcher. |
| `APP_ICON_SVG` | App Hub's own store icon, declared in `listing.json` (`assets/icon.svg`). |

<a id="adopting-it-in-a-shell"></a>

## Adopt it in a shell

1. Depend on it at an App Hub pin. It needs contract 1.6, which Cargo
   resolves from crates.io, as OctoSense `main` does:

   ```toml
   [dependencies]
   octosense-app-hub-app = { git = "https://github.com/OctoSense-org/OctoSense-App-Hub", rev = "<pin>" }
   ```

   If the pin's contract is newer than the latest release
   ([Versions on crates.io](../app-contract/README.md#versions-on-cratesio)),
   also patch the contract to the same revision:

   ```toml
   [patch.crates-io]
   octosense-app-contract = { git = "https://github.com/OctoSense-org/OctoSense-App-Hub", rev = "<pin>" }
   ```

2. Copy the Makepad and OctoScript `[patch]` sections, including
   `makepad-wm-theme`, from this workspace's root `Cargo.toml`, and point them
   at the shell's checkouts.

3. Name the shell's system-app selection in `.cargo/config.toml`:

   ```toml
   [env]
   OCTOSENSE_SYSTEM_APPS = { value = "system-apps.json", relative = true }
   ```

   The selection file names the apps and where their bundles are. OctoSense's
   phone selection
   ([`phone/system-apps.json`](https://github.com/OctoSense-org/OctoSense/blob/main/phone/system-apps.json))
   is:

   ```json
   {
     "schema": 1,
     "source": "../apps",
     "apps": ["news", "photos", "maps", "camera", "mail", "calendar", "ai-providers", "youtube"],
     "assets": {
       "photos": { "photos": "../apps/photos/resources/photos" }
     }
   }
   ```

   `source` is the directory that holds the bundles, here OctoSense's
   [`apps/`](https://github.com/OctoSense-org/OctoSense/tree/main/apps).
   `source` and the asset paths are relative to the selection file's
   directory. Each `<source>/<name>/bundle` is packed with its digest stamped,
   and each asset directory is compiled in and served at `<prefix>/<file>`,
   where `<prefix>` is the key inside the app's `assets` entry (`photos`
   above).

   **If `OCTOSENSE_SYSTEM_APPS` is unset, the build ships no system apps** and
   prints a `cargo:warning`. Set it explicitly: this crate usually builds from
   Cargo's git checkout, where its own location says nothing about the shell.

4. Link the modules: add `APP_HUB_MODULE` and `CARD_MODULE` to the shell's
   module list, and add a launcher row for each app that `system_apps()` and
   `installed_apps(..)` return; each row opens `card` with that app.

5. Register the host services the system apps call through `host.request`
   (`octosense_appstore::services::register_host_service`: Mail's accounts,
   Calendar's events, News's feeds) before the first system app opens.

6. Draw the host sheet. `CARD_MODULE` already wraps the runner in a host
   widget that draws a visible sheet over the app. A shell with its own card
   presentation must draw the sheet itself. Take the runner's card, sheet and
   notice references before any app widget exists, as `HostedHubCard::attach`
   does, and never look them up by widget id later: an app may name its own
   widget `sheet`.

With `relative = true` (step 3), Cargo passes `OCTOSENSE_SYSTEM_APPS` as an
absolute path. If you set a relative path another way, such as in the
environment, `build.rs` resolves it against the nearest directory above the
target directory that holds a `Cargo.lock`.

OctoSense is the reference: `crates/shell/src/apps.rs` (`system_card_apps`,
`register_host_services`, `card_apps`) and, for the sheet and mounting, the
`HostedHubCard` widget in this crate's `src/card_host.rs`.

## Run it alone

```sh
cargo run --release -p octosense-app-hub-app --example preview
```

The preview logs launch requests instead of launching; only a shell opens
apps.

This crate's default feature, `text-input-state-query`, turns on the same
feature in `octosense-appstore`, and that feature needs OctoSense's runtime
patches on `../makepad`. Against plain sibling checkouts, add
`--no-default-features` to the `cargo run` and `cargo test` commands on this
page.
[`card-host` fails to build](../../docs/DEVELOPMENT.md#card-host-fails-to-build)
explains the feature.

## Exercise installation without publishing apps

Generate a fresh, local, signed catalog in an empty directory:

```sh
cargo run -p octosense-app-hub-app --example fixture -- target/app-hub/local-fixture
```

The generated `environment.json` holds `OCTOSENSE_HUB`,
`OCTOSENSE_HUB_ANCHOR` and `OCTOSENSE_APP_DATA`. Set them when you launch the
shell to select the local catalog, its fresh trust anchor and an isolated
installation directory. The signing keys live in memory only. The fixture
includes Trail Notes and Focus Timer as real card bundles with clearly marked
validation artwork. Never distribute a build configured with a fixture anchor.

## Checks

```sh
cargo test -p octosense-app-hub-app
cargo test -p octosense-app-hub-app --example fixture
```

The [native tools CI](../../.github/workflows/native-tools.yml) runs the
library tests against plain sibling checkouts:
`cargo test -p octosense-app-hub-app --no-default-features --lib`.
