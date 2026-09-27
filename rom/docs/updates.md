# Updates

The OctoSense ROM and the OctoSense Home app update over the air from GitHub
Releases of the [OctoSense repository](https://github.com/OctoSense-org/OctoSense/releases). The [web installer](../web-installer/README.md#flash-from-your-browser)
is currently a local developer preview for a fresh installation with data
erasure. Automated browser recovery is not implemented yet; use this OTA flow
for supported updates.

## What a release carries

Each build is published as a versioned release tagged `rom-v<build-tag>` (for
example `rom-v20260919-j`):

| Asset | Used by |
|---|---|
| `octosense-<tag>-enchilada.zip` | The A/B OTA package (`payload.bin` inside), signed with the release key |
| `OctoSenseHome-<versionCode>.apk` | The Home app, signed with the platform key |
| `update.json` | What the phone reads: versions, URLs, sizes, hashes, the payload's offset and properties |

The phone always looks at
`https://github.com/OctoSense-org/OctoSense/releases/download/rom-latest/update.json`
(no API or token for a public repository). `rom-latest` is a moving release
that carries only `update.json`; its URLs name the assets of the versioned
`rom-v*` release it currently offers. GitHub's `releases/latest/download/`
redirect is not used because the repository also publishes `desktop-v*` and
`home-v*` releases, and "latest" could be any of them. The secure setting
`octosense_update_source` points the phone elsewhere, for a test channel for
instance.

Phones flashed with `20260919-j` or an earlier image check
`OctoSense-org/octosense-rom/releases/latest/download/update.json`, a URL in
the OctoSense-ROM repository (retired; merged into this repository), which no
longer exists. They never see an update: reflash them once with an image that
reads `rom-latest`, and OTA works from then on. (No flashed phone relied on OTA
when the repository was retired.)

## How an update installs

- **ROM.** The OnePlus 6 has two slots of every system partition. `update_engine`
  streams the payload from the release URL (HTTP byte ranges, no local copy) into
  the slot that is not running, while Android keeps running. It verifies the
  payload against the release key built into the ROM, so only our builds apply,
  and refuses a build older than the running one. One restart switches slots; if
  the new slot fails to boot, the bootloader returns to the old one.
- **Home app.** The agent downloads the APK, checks its sha256, and installs it
  with `INSTALL_PACKAGES` (no prompt). PackageManager accepts it only when it is
  signed with the platform key, like the ROM's copy, and has a higher version code
  (`YYYYMMDDHH`, from `cargo-makepad --version-code=auto`).

## Three ways to trigger it

1. **On the phone (option 3).** The agent checks every six hours on any network
   and posts "OctoSense update available — Install"; then "Restart to finish".
   `settings put secure octosense_update_auto 1` installs without asking and only
   asks for the restart. The agent's Binder API (`checkUpdate`, `applyUpdate`,
   `getUpdateStatus`, `rebootToUpdate`) lets the launcher or a card do the same.
2. **From the Mac over the phone's network (option 2).** `scripts/ota-push.sh
   [tag]` runs `update_engine_client` through `adb root` (Developer options >
   Rooted debugging) against the release URL; nothing big crosses the USB cable.
3. **From the Mac through the agent.** `adb shell dumpsys activity service
   dev.makepad.octosense.agent/.AgentPlatformService update-apply` (also
   `update-check`, `update-status`, `update-reboot`), no root needed.

## Publishing

    scripts/build-home.sh                 # Home app, platform-signed, rom-builds/home/latest.apk
    scripts/release.sh <tag>              # after a host build: images, zip, manifest (web installer)
    scripts/publish-release.sh <tag>      # GitHub release rom-v<tag>: zip, Home APK, update.json;
                                          # then update.json onto rom-latest

`publish-release.sh` creates the versioned release `rom-v<tag>` (not marked as
the repository's latest release) and then replaces `update.json` on the
`rom-latest` release, creating that release the first time. Only that file
changes; the `rom-latest` tag itself never moves. `update.json`'s format is the
same in both releases. To roll back, upload an older build's `update.json` to
`rom-latest` (`gh release upload rom-latest <file> --clobber -R OctoSense-org/OctoSense`);
phones still refuse to downgrade the ROM, so a rollback only stops offering the
newer build. While the file is being replaced, the URL briefly returns 404 and
a phone checking at that moment tries again at its next check.

## Security notes

- The release key and the platform key are the trust boundary: whoever holds
  them can ship code the phone installs. They live in a private key directory
  outside the repository.
- update.json is not signed; it cannot make the phone install anything the keys
  did not sign, but it can offer an older signed build (refused by update_engine
  for the ROM, by version code for the app) or withhold updates.
- The repository is public, so releases are world-downloadable. The ROM contains
  OnePlus's proprietary firmware and HAL binaries (from TheMuppets), which are
  community-tolerated but not licensed for redistribution.
