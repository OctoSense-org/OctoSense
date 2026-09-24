# OctoSense Installer — developer preview

A static WebUSB installer for the OnePlus 6 development recipe. Public-origin
installation is disabled until signed release verification and hardware
qualification are implemented. The architecture and remaining release gates
are recorded in [ADR 0001](../docs/adr/0001-public-web-installer.md).

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

## Local preview

From the repository root, package an existing complete build and serve the
installer together with its modules and images:

```sh
python3 scripts/make-manifest.py /absolute/path/to/build "OctoSense development" enchilada
ln -s /absolute/path/to/build/*.img /absolute/path/to/build/manifest.json web-installer/
python3 -m http.server 8321 --bind 127.0.0.1 --directory web-installer
```

Open `http://127.0.0.1:8321` in desktop Chrome or Edge. Packaging and starting
the server do not reboot or flash a phone. The images and manifest symlinks
are ignored by Git. Serve only a dedicated development directory; public
hosting and artifact distribution require the later signed-release pipeline.

The optional configured-build-host helper is now:

```sh
scripts/release.sh <build-tag>          # package locally only
scripts/release.sh <build-tag> --serve  # also serve the local preview
```

It requires the existing private build-host configuration. It no longer
accepts a phone serial, reboots phones, kills USB tools or opens a browser.
It does not publish a GitHub release or make an artifact stable.

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
3. Complete Chinese localization, platform USB help and public website
   integration. Run the supported host/browser and ROM hardware test matrix
   before promoting a public beta. The display patch remains separate work.
