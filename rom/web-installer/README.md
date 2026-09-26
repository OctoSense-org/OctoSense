# OctoSense Installer — developer preview

English | [简体中文](README.zh-CN.md)

A static WebUSB installer for the OnePlus 6 development recipe. Public-origin
installation is disabled until signed release verification and hardware
qualification are implemented. The architecture and remaining release gates
are recorded in [ADR 0001](../docs/adr/0001-public-web-installer.md).

## Flash from your browser

This guide describes the local preview on a computer. There is no enabled
public flashing website yet. **The exact OnePlus 6 identification method is
still awaiting hardware qualification:** if the page cannot verify your phone,
installation stays disabled. The steps below do not bypass that check.

### 1. Prepare your computer, phone and ROM files

- Use desktop Chrome or Edge and a USB data cable. The setup commands below
  are for macOS or Linux, with Git and Python 3 installed. The full
  browser/host hardware support matrix is not yet qualified.
- Use a OnePlus 6 (`enchilada`). OnePlus 6T (`fajita`) is a different target.
- Back up the phone and charge it before starting. **Unlocking and a fresh
  installation erase phone data**, including apps, accounts and files.
- Obtain a complete, trusted image set from one OctoSense development build.
  Keep the files together in a directory on your computer:

  ```text
  system.img
  vendor.img
  dtbo.img
  vbmeta.img
  boot.img
  ```

  Use the build's partition-image output or an image bundle supplied by its
  maintainer. The installer does not accept an OTA ZIP, `payload.bin`, Home
  APK or `update.json` as a substitute for these five images. Do not mix
  files from different builds. Ask the build maintainer for its firmware
  prerequisites; this preview does not yet verify them automatically.
- Leave enough computer disk space for the image set **and** another copy in
  browser storage. The installer checks browser storage quota before downloading.

The computer's browser communicates directly with the bootloader over USB.
Root access and a terminal app on the phone are not required.

### 2. Start the installer on your computer

If you do not have the source checkout yet:

```sh
git clone https://github.com/OctoSense-org/OctoSense-ROM.git
cd OctoSense-ROM
```

From the repository root, replace `/absolute/path/to/rom-images` below with
the directory containing your five images. The following commands generate
`manifest.json`, prepare a separate temporary serving directory, and start a
server accessible only from this computer. Leave this terminal running:

```sh
(
  set -eu
  octosense_images="/absolute/path/to/rom-images"
  python3 scripts/make-manifest.py "$octosense_images" "OctoSense development" enchilada

  octosense_preview="$(mktemp -d "${TMPDIR:-/tmp}/octosense-web.XXXXXX")"
  cp -R web-installer/index.html web-installer/fastboot.mjs \
    web-installer/src web-installer/vendor "$octosense_preview/"
  for partition in system vendor dtbo vbmeta boot; do
    ln -s "$octosense_images/$partition.img" "$octosense_preview/$partition.img"
  done
  cp "$octosense_images/manifest.json" "$octosense_preview/manifest.json"
  python3 -m http.server 8321 --bind 127.0.0.1 --directory "$octosense_preview"
)
```

Open **[http://127.0.0.1:8321](http://127.0.0.1:8321)** in desktop Chrome or
Edge. Confirm that the page shows the expected release name and build date.
Open the HTTP address rather than double-clicking `index.html`. No npm install
or JavaScript build is needed to serve the installer.

Use this same browser profile, hostname and port throughout installation and
recovery review: the browser stores its operation journal for that origin.
Keep the source images unchanged while the server is running. Once all device
operations have finished, press **Ctrl+C** in the terminal to stop the server.

Maintainers with the existing private build-host configuration can instead
run `scripts/release.sh <build-tag> --serve`. Without `--serve`, it only
packages the build. Neither form reboots or flashes a phone or publishes a
release.

### 3. Connect in bootloader mode

1. Close other installer tabs and USB flashing tools that are using this phone.
2. Power off the OnePlus 6, then hold **Volume Up + Power** to enter the
   bootloader. This is the sequence in the
   [LineageOS OnePlus 6 device instructions](https://github.com/LineageOS/lineage_wiki/blob/main/_data/devices/enchilada.yml).
3. Connect the phone directly to a USB port on your computer.
4. Click **Connect phone**. If the browser shows a USB chooser, select this
   phone and allow the connection.
5. Review the phone and bootloader status. Continue only when the page confirms
   that the device identity and partition layout match the development recipe.

A report of `sdm845` alone does not identify a OnePlus 6. If the page says
**model unverified**, stop and use the support report described below; there
is no supported force-install option.

### 4. Unlock if the bootloader is locked

Skip this step if the page already reports **Unlocked**. On a locked phone,
OEM unlocking must be permitted in Android's Developer options before the
bootloader can accept an unlock request; return to Android to enable it if
needed, then reconnect in bootloader mode.

Once the installer verifies the locked phone, read and check the separate
unlock data-erasure confirmation, then click **Unlock bootloader**. Follow
the confirmation shown on the phone. After unlocking and any restart have
finished, return to bootloader mode and click **Connect phone** again. The
page must read the new **Unlocked** state before you can continue.

### 5. Verify and install

1. Click **Download and verify release**. Wait for **All images verified**.
   Every image is stored and fully checked before any partition write starts.
2. Review the release and the fresh-install data-erasure statement. Check
   **I have backed up what I need and agree to erase all phone data**.
3. Click **Erase data and install OctoSense**. Keep the phone connected and
   the computer awake; leave the tab and local server open while it works.
4. Wait for **Images written and data erased. Ready to restart**. If a write
   fails, follow the recovery guidance below instead of retrying the install.

### 6. Restart and check the phone

Click **Restart phone** when it becomes available. The first boot can take
several minutes. Complete any on-phone setup, and click **I can see OctoSense
Home** only when Home is actually visible on the physical screen. This records
your confirmation; automatic verification of the running build is not yet
implemented.

For later supported upgrades, use the [ROM update flow](../docs/updates.md).
The browser preview currently performs a fresh install with data erasure.

### Troubleshooting

| What you see | What to do |
| --- | --- |
| No page, or `Address already in use` | Check that the local server is running and that port 8321 belongs to this preview. Do not replace a server during an active installation. |
| No release, HTTP 404, or invalid manifest | Check that all five images come from the same build and that manifest generation succeeded. Serve the HTML, `src/`, `vendor/`, `fastboot.mjs`, manifest and images together. |
| Unsupported browser or disabled Connect | Use desktop Chrome/Edge at the local HTTP address. The page requires WebUSB, Web Locks, WebCrypto and browser file storage. |
| Phone missing from USB chooser, or connection denied | Confirm bootloader mode, a data-capable cable and browser permission. Close other tools using the phone. OS USB permissions or drivers may also need attention; see [Chrome's WebUSB guidance](https://developer.chrome.com/docs/capabilities/usb). |
| Model, mode, slot or partition state unverified | Stop and save a support report. Do not edit the manifest or code to force a match. Physical-device qualification is still pending. |
| Insufficient storage, checksum mismatch or download failure | Preparation has not written partitions. Free disk space or obtain a complete matching image set, then repeat verification. |
| Recovery review required after a write, USB loss or page reload | Keep the phone in bootloader mode and save the report. Do not clear browser site data, switch origins to bypass the journal, or replay the installation. |

To save a report, expand **Technical details and support report** and click
**Download support report**. Send it to the build maintainer for recovery
review. The preview does not provide automatic recovery or resume a partial
flash. [Manual flashing and recovery notes](../docs/flashing.md) describe the
separate recovery-based workflow; choose a recovery procedure for the actual
device state before issuing further commands.

## Current behavior

- Require a complete schema-1 development manifest: system, vendor, dtbo,
  vbmeta and boot images, each with full SHA-256, file/expanded sizes and a
  fixed slot rule. Legacy manifests must be regenerated.
- Verify exact phone identity, bootloader mode, unlock state, slot, partition
  layout and transfer capacity. `sdm845` alone is insufficient. The adapter
  also queries `getvar:device`; that exact-identification route has **not yet
  been qualified on the physical OnePlus 6**. Missing/ambiguous values block
  writing, even on localhost. OnePlus 6T is unsupported.
- Stage every image in origin-private disk storage, hash every byte in a
  worker, and validate raw/sparse framing before the first flash. Large
  images are not loaded into one JavaScript ArrayBuffer for hashing.
- Require explicit consent to a fresh installation that erases phone data.
  Unlocking has a separate data-erasure confirmation. This preview does not
  offer a data-preserving update, relock, root or temporary kernel boot.
- Reserve operations with Web Locks, bind the USB handle and reviewed slot,
  and recheck device state before writes. No automatic device substitution,
  partition retry or replay after interruption.
- Journal write intent before USB writes. A partial/uncertain installation
  blocks another ordinary install, including after reload or from another
  tab. Recovery review is manual; an automated recovery recipe is pending.
- Distinguish images written, restart requested, and user-confirmed Home.
  The last step is **not** automatic running-build verification.

The app needs desktop Chrome/Edge capabilities: secure context, WebUSB,
Web Locks, WebCrypto and origin-private file storage with enough free quota.
Browser-managed storage is not an archival backup; closing the tab interrupts
preparation. A killed tab can leave temporary staging files; automatic orphan
cleanup and resumable downloads are pending. Do not clear site storage to
bypass an unresolved write journal.

## Development and verification

Node 24 and Python 3 are used in CI. No JavaScript build step is needed to
serve the application. npm dependencies are test-only:

```sh
cd web-installer
npm ci --ignore-scripts
npm test
npx playwright install chromium
npm run test:browser
cd ..
python3 -m unittest discover -s tests -p test_installer_manifest.py -v
bash -n scripts/release.sh
```

Tests never operate a physical phone. The Chromium suite runs the actual
page, worker, OPFS storage and Web Locks with simulated USB and downloads.
Coverage includes a corrupted 64 MiB download, incomplete manifests,
late download/format failures, incompatible devices, conflicting tabs,
interrupted writes, reload recovery, public-origin gating, keyboard consent
and a narrow viewport. Separate tests exercise the real fastboot adapter
against simulated USB handles, the hash implementation against Node crypto,
and the packaging script against generated raw/sparse fixtures.

`src/contracts.mjs` owns manifest and device checks; `session.mjs` owns
operation ordering/journaling; `transport.mjs` wraps the vendored fastboot
library; the download worker and `image-format.mjs` verify staged files;
`app.mjs` renders the page. Runtime dependencies are local, pinned sources:
`android-fastboot` 1.1.3 and the `@noble/hashes` 2.4.0 SHA-256 closure. See
[NOTICE](../NOTICE) and the hash dependency's
[integrity receipt](vendor/noble-hashes/provenance.json).

## Next milestones

1. Qualify exact identity, mode, firmware prerequisites and partition rules
   on dedicated OnePlus 6 hardware; implement tested recovery and boot checks.
2. Sign release manifests, define key rotation/channel promotion, reject
   stale or failed builds and bench ADB authorizations, and publish immutable
   images with tested browser download behavior.
3. Complete Chinese UI localization, platform USB help and public website
   integration. Run the supported host/browser and ROM hardware test matrix
   before promoting a public beta. The display patch remains separate work.
