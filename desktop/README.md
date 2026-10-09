# OctoSense desktop

English | [简体中文](README.zh-CN.md)

**Download:** [desktop 0.1.0-rc.2](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.2) supports the current App Hub catalog, GitHub-attested app bundles and SDK 1.10.0 public host APIs. Platform support and acceptance limits are listed in [Host OS API status](../docs/host-os-api-status.md). Choose the package for your operating system and architecture; check its release notes and checksums. Embedded pages also need the [platform browser prerequisites](../docs/desktop-embedded-browser.md#runtime-requirements).

**New to the code?** Read the [desktop, Home, ROM and system-app walkthrough](docs/code-walkthrough.md), then the [agent and Tokio walkthrough](../docs/architecture-walkthrough.md). The first follows launch, native hosting, script bundles, app data and Android platform boundaries.

> **Where this fits.** The desktop is one shell process with the octos kernel as its child (the packaged `octos-kernel`, or `OCTOS_APP_CORE_BIN`). App Hub, the Card runner with the script apps, and Rinx run in process; the Terminal runs as its own process, in an OS sandbox on macOS and Linux (not yet on Windows), attached over the shell's hub. On macOS that sandbox keeps `~/.cargo`, `~/.rustup` and the OctoSense checkout read-only, so run `cargo install`, `rustup update` and builds of OctoSense itself in another terminal. Diagrams of the processes, an app agent's two lanes and a tool call with its approval: [How it fits together](../README.md#how-it-fits-together); the details: [docs/architecture.md](../docs/architecture.md) and [ADR 0004](../docs/adr/0004-native-apps-hosting-and-peers.md).

The desktop shell of [OctoSense](https://github.com/OctoSense-org), the agent shell on top of your operating system, and the desktop packaging of the OctoSense repository (formerly the OctoSense-Desktop repository). It is one Makepad window that is the desktop: a launcher, a dock and tiles, hosting system apps and App Hub store apps as contained script programs, trusted native modules in-process, and Makepad developer programs as child processes. It gets its apps the same way the phone shell, [Home](../phone/README.md), does. Setup, the repository layout and CI are in the [root README](../README.md).

**Building an OctoSense app?** You do not need this repository to build, check or publish one. Start at the [OctoSense-org profile](https://github.com/OctoSense-org)'s reading list: the `AGENTS.md` of OctoSense App Flow (formerly Design Flow), then its `docs/QUICKSTART.md`. Build this shell only if you want to see your app in the desktop shell before it is published ([Try your own app](#try-your-own-app-before-it-is-published)).

## Where it sits

| Where | Role for the desktop |
| --- | --- |
| [`../phone/`](../phone/README.md) | Home, the phone shell. Same app model, same runtime, same system apps. |
| [`../apps/`](../apps/README.md) | News, Photos, Maps, Mail, Calendar, AI providers and YouTube bundles (Camera is phone-only), the Mail, Calendar and `llm` host services, the AppCard assistant (`octos-app`, opt-in, not shipped by default), and Reference. |
| [`../crates/`](../crates/) | The shell itself (`crates/shell`, package `octosense-shell`, which this package wraps), its AI services (`crates/ai-host`), the octos kernel service (`crates/kernel`, package `octosense-kernel`) and the app-agent broker (`crates/app-peers`). |
| [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) | The signed catalog, the store and the Card runner. Linked as the Git crate `octosense-app-hub-app`. |
| [OctoSense-App-Flow](https://github.com/OctoSense-org/OctoSense-App-Flow) | Where apps are designed, built and published to the App Hub. |
| [OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad) | The runtime release that pins Makepad and OctoScript. Checked out in `.sources/`. |
| [makepad (OctoSense fork)](https://github.com/OctoSense-org/makepad) | The framework. Checked out in `.sources/makepad`. |
| [octos](https://github.com/octos-org/octos) | The agent kernel, a shell service (`octos-core`, on by default): AI providers configures it, AppCard, Rinx and other consumers connect to it. One revision, pinned in the root `Cargo.toml`; the kernel itself is a separate binary (desktop: the packaged `octos-kernel` beside the shell, see [Build and run](#build-and-run), or `OCTOS_APP_CORE_BIN`; Android: bundled `liboctos.so`). |
| [Rinx](https://github.com/hagency-org/Rinx) | Matrix chats and mini apps, linked as a module (`app-rinx`, on by default). |

## Layout of `desktop/`

| Path | What it is |
| --- | --- |
| `src/main.rs` | The entry point (package `octosense`): `octosense_main!()` over the shell's `App`. The shell (tiling, launcher, dock, bar, hosting, the app registry, `shell/`, `octosense/`) is [`../crates/shell/src`](../crates/shell/src). |
| `config/apps.json` | The default developer-program catalog: the Makepad apps OctoSense picked (`pick` in `apps.overlay.json`), each also a native app in [`../native-apps.json`](../native-apps.json). `apps.makepad.json`, for `--apps`, has every app upstream's registry curates; `apps.overlay.json` holds the adaptations applied when regenerating both. |
| `system-apps.json` | Which system apps this build packs, and from where (`../apps`). |
| `scripts/` | `package.py` (the release packages, [Release builds](#release-builds)), `upstream.py` (WM provenance and catalog regeneration), `smoke.py` (native smoke test), their Python tests, `system_apps_remote.sh`, `ai_providers_remote.sh` and `glance_remote.sh` (hidden `--remote` end-to-end runs of the system apps and of AI providers), and `provision-appcard-llm.sh` (Android). |
| `packaging/` | The release packages' cargo-packager config (`release.json`), app icon (`icons/`, `make_icons.py`) and macOS `Info.plist` additions and entitlements. |
| `upstream/makepad.json` | Provenance of every file imported from Makepad's `apps/wm`. |
| `resources/android/` | The Android manifest template. Themes, wallpapers, icons and the startup script are the shell's, in [`../crates/shell/resources`](../crates/shell/resources). |
| `docs/` | [Validation record](docs/validation.md), [upstream sync](docs/upstream.md), [local AI](docs/local-ai.md), [Android AppCard build](docs/android-appcard-build.md), dated plans. |
| `KEYBINDINGS.md`, `BACKLOG.md` | Keymap notes; open follow-ups. |

The modules the desktop links live elsewhere in the repository: Reference in `../apps/reference`, the AppCard module in `../apps/appcard/module` (with `octos-app` in `../apps/appcard/app/app`).

## Prerequisites

- Stable Rust (`cargo` in `~/.cargo/bin`) and the native toolchain for your OS. On macOS, Xcode Command Line Tools (`xcode-select --install`).
- Git and Python 3.9+ for `tools/setup.py`. `scripts/upstream.py` needs Python 3.11+ (it imports `tomllib`).
- Network access for the first setup and build.

## Set up

From the repository root, prepare the pinned framework sources once:

```sh
python3 tools/setup.py                  # makepad, octoscript, octoscript-makepad into .sources/
python3 tools/setup.py --check --cargo  # verify: one Makepad, App Hub, octos and Rinx in the graph
```

Details, `--update` and `--cache`: [root README](../README.md#set-up).

## Build and run

The shell discovers agent metadata and prepares Glance permissions on background workers. Cards run only after their existing admission checks finish. Agent choices take effect immediately and save asynchronously; normal quit waits for pending consent writes, and a write failure is shown in the shell. Theme selection still reloads custom artwork.

From the repository root, after setup: stage the octos kernel beside the shell, then build and run it.

```sh
python3 tools/kernel-artifact.py --host --stage target/release   # once per octos pin
cargo run --release -p octosense
```

`kernel-artifact.py --host` checks out the octos revision `Cargo.lock` pins into `target/octos-kernel/` (never a checkout of yours) and builds it for this machine; `--stage target/release` puts it there as `octos-kernel` with its receipt `octos-kernel.json` (revision, version, SHA-256). `--kernel <path>` stages a binary you already built instead; it is refused unless its `--version` names the pinned revision. For a debug build stage into `target/debug`. Plain `cargo run` never builds or installs the kernel.

At startup the kernel service looks for `octos-kernel` beside the `octosense` executable (in a macOS `.app` also in `Contents/Resources`), never in the working directory or on `PATH`, and runs it only when its receipt names the octos revision this build pins and the binary's SHA-256 matches. Otherwise the desktop runs without an assistant and says why: for example, after the octos pin moves, `refusing the packaged kernel …: it is octos <old> but this build pins <new>`; stage it again. `OCTOS_APP_CORE_BIN=<path>` still wins and is not checked. While developing against another octos, `OCTOSENSE_KERNEL_ANY_REVISION=1` runs a packaged kernel of another revision (the receipt and its SHA-256 are still required).

The desktop starts empty. Start App Hub, a system app or a developer program from the dock, the top-left **Apps** menu, or **⌘Space** (menu and search). **System → Quit OctoSense** closes the desktop and everything it hosts.

Developer programs from `config/apps.json` build on first launch (progress shows in the tile).

| Platform | Status |
| --- | --- |
| macOS | Source and packaged-app builds, process hosting, App Hub and system apps have native validation. RC1 public sample install/update/restart checks are recorded in its release evidence. |
| Windows, Linux | CI package builds and payload privacy scans pass. [Embedded-browser checks](../docs/desktop-embedded-browser.md) and [synthetic backend/browser/vault checks](../tools/connected-e2e/README.md#native-backend-browser-acceptance-2026-10-08) passed at their recorded source revisions. Windows/Linux installer GUI acceptance remains unverified. |
| Android | `cargo makepad android run -p octosense --release`; see [Phones](#phones). |
| iOS | Startup policy is tested, but a full build currently fails in the pinned Makepad Metal backend ([validation](docs/validation.md)). |

Installable packages (macOS `.app`/`.dmg`, Windows installer, Linux `.deb`/`.AppImage`) come from [Release builds](#release-builds). A Linux session compositor is not provided.

### Cargo features

The native apps' features (`app-hub`, `app-rinx`, `app-reference`, `app-sheets`, `app-terminal`, `app-appcard`), the default set and `mobile-apps` come from [`native-apps.json`](../native-apps.json) (ADR 0004 §1): edit the manifest and run `python3 tools/native_apps.py`, never the generated blocks in the `Cargo.toml`s.

| Feature | Default | Effect |
| --- | --- | --- |
| `app-hub` | on | Links `octosense-app-hub-app` (store `apphub`, Card runner `card`, system apps); the Mail, News, Calendar and AI providers host services; the connected-account services (`auth`, `github`, `gmail`, `gcalendar`); and the Markdown editor GitHub Notes uses (`octosense-markdown-editor`). Without it the build has no App Hub and no system apps. |
| `octos-core` | on | The octos kernel service (`octosense-kernel`, from `../crates/kernel`) and the app-agent broker (`octosense-app-peers`): the one kernel AppCard, Rinx and other consumers share, configured by AI providers. Always on for Android and iOS. Leave it out with `--no-default-features --features app-hub` (and whatever else you want). |
| `wasm-functions` | on | The `wasm` host service: apps' own WebAssembly functions ([ADR 0011](../docs/adr/0011-apps-own-functions-in-webassembly.md), [WebAssembly in OctoSense](../docs/wasm.md)), run by `../crates/wasm-host` (Wasmtime, compiled by Cranelift). It runs on macOS and Linux; a Windows build leaves the runtime out. `wasm-lab` is its former name. |
| `app-rinx` | on | Links [Rinx](https://github.com/hagency-org/Rinx), the Matrix client, as a module; implies `octos-core` (its assistant is the shell's). |
| `app-reference` | off | Links Reference (`../apps/reference`) as a module. |
| `app-sheets` | off | Links Makepad's Sheets as a module. |
| `app-terminal` | on | Links Makepad's Terminal as a system app: a login shell in a tile. On macOS and Windows it runs as its own process (`terminal`, which the shell builds from the OctoSense checkout with `cargo build` and starts itself, else the binary beside `octosense`), so a crash in it leaves the shell running; on Linux only with a Vulkan build in a Wayland session. Where it cannot start a process (no checkout and no binary: release packages ship only `octosense` and the kernel) it opens in-process; a `terminal: Module` or `terminal: Process` line in `wm/apps.splash` under the state directory overrides that. The AI pane can read it in either hosting (`read_screen`, `read_scrollback`), but it can type a command (`run`) only when the Terminal runs as its own process; in-process, the Terminal offers its read tools only (ADR 0004 §10). Outside developer mode, each command waits for the person's approval on a host sheet (`confirm: host`, `auto_approvable: false` in `native-apps.json`), and a command over 4 KiB is refused. In-process on macOS, the shell's PTY helper is `octosense` itself. |
| `app-appcard` | off | Links the AppCard assistant module (`../apps/appcard/module`); implies `octos-core`. Opt-in on every target, phones included; not shipped for now. |
| `app-aichat` | off | Links Makepad's AI chat as a module, without its model engine. |
| `mobile-apps` | off | `app-rinx` + `app-reference` + `app-sheets` + `app-hub` + `octos-core` + `wasm-functions`: the set phone builds link, for testing on desktop. Not AppCard. |

A linked native app is hosted as its `hosting` in `native-apps.json` says for the platform: App Hub, Rinx, AppCard, Reference and Sheets in-process everywhere, the Terminal as a process on macOS and Windows (and on Linux with a Vulkan build in a Wayland session), and Task, which has no module, only as a process, and not at all where there are no processes. `--module <id>` (or a `<id>: Module` line in `wm/apps.splash` under the state directory) opens one in-process instead:

```sh
cargo run --release -p octosense -- --module terminal
```

App Hub's modules have no process form and always open in-process.

### Flags and environment

| Name | Effect |
| --- | --- |
| `--apps <file>` | Use this developer-program catalog. |
| `--module <id>` | Host a linked module in-process. |
| `--assistant`, `--prewarm` | Start the assistant app / prewarm apps (need matching catalog entries). Off by default. |
| `--demo-home`, `--download-wallpapers` | Generate a demo filesystem; fetch the Omarchy theme's full wallpaper set. |
| `OCTOSENSE_HOME` | State directory (default `~/.octosense`; falls back to an existing `~/.makeos`, and `MAKEOS_HOME`). |
| `OCTOSENSE_APP_DATA` | Where App Hub keeps installed apps (default `apps/` in the platform data directory). |
| `OCTOSENSE_HUB`, `OCTOSENSE_HUB_ANCHOR` | App Hub catalog origin (path or URL) and trust anchor; default is the App Hub repository's `main`. |
| `OCTOSENSE_SYSTEM_APPS` | The system-app selection file; the root `.cargo/config.toml` sets it to `desktop/system-apps.json`. |
| `MAKEPAD_APP_CONFIG='{"mail_demo":true}'` | Serve Mail's demo mailbox (see [Demos](#demos)). |
| `OCTOSENSE_MAIL_VAULT=file` | Keep Mail passwords in a 0600 file instead of the macOS keychain. |
| `OCTOSENSE_LLM_VAULT=file` | Keep AI providers' keys in the owner-only octos profile instead of the macOS keychain. |
| `OCTOS_APP_CORE_BIN`, `OCTOS_APP_CORE_DIR` | The octos kernel binary the shell's kernel service runs, unchecked (unset: the packaged `octos-kernel`, see [Build and run](#build-and-run)) and its core dir (default `~/octos-home/.octos`; the AI providers profile is `<dir>/profiles/_main.json`). |
| `OCTOSENSE_GLANCE_DEMO=1` | Publish a sample L0 News digest card (as `os.news`) to the glance screen at startup: F9 on a desktop style, the glance page on a phone style. A test path for the `glance` service. |
| `OCTOSENSE_GLANCE_DEMO=mail` | Publish two fake Mail action cards (L0, as `os.mail`, with a toast each) at startup: clicking a toast opens that card in the card window, where Reply, Send (demo), Ask and Track work on fake data. They work the same in the glance panel, which opens with them, and on a phone style's glance page. No mail is read and no model is called. `scripts/mail_card_remote.sh` drives it hidden. |
| `OCTOSENSE_REDUCE_MOTION=1` | Keep the glance panel and its toasts still instead of sliding them in and out ([The glance panel](#the-glance-panel)). |
| `MAKEPAD_REMOTE`, `MAKEPAD_HIDE_WINDOWS` | Remote-control bridge; hidden windows (see [Demos](#demos)). |

Since desktop 0.1.0-rc.1, apps can show web pages on Linux under X11 or XWayland and on Windows. The shell embeds the system's WebKitGTK or WebView2 engine; when the engine is missing, or the session is native Wayland, `WebReader` reports an error. [Desktop embedded browser](../docs/desktop-embedded-browser.md) lists what each platform needs.

## Application icons

Both `octosense` and `octosense-home` use OctoSense's green eight-petal mark.
The workspace's Cargo environment embeds the desktop icons for normal
`cargo run`/`cargo build`. Each package also supplies the PNG sizes, ICO and
ICNS files that the pinned `cargo-makepad desktop` commands discover for
macOS bundles, Windows executable resources and Linux desktop entries.
Release packages continue to use `packaging/icons/`.

Android builds of either package include legacy density icons, adaptive
icons and Android 13 themed icons. Both packages supply opaque, correctly
sized iPhone/iPad asset catalogs; Home's OpenHarmony wrapper replaces the
framework's application, layered launcher and launch-window artwork too.
These assets do not change which app targets currently build successfully.

The source geometry and colours live in `packaging/make_icons.py` and
`packaging/icons/icon.svg`. Generated files are committed; building the app
needs no image renderer. From the repository root:

```sh
python3 desktop/packaging/make_icons.py
python3 desktop/packaging/make_icons.py --check
python3 -m unittest discover -s tools -p test_app_icons.py
```

Android resource compilation, iOS asset compilation and macOS ICNS decoding
were checked locally. A fresh Home installation on Pixel 7 Pro / Android 17
shows the branded icon in App info and launches successfully. Other installed
platforms, Android themed-icon mode and Windows/Linux desktop integration
remain **unverified** for this icon change.

## Release builds

`desktop/scripts/package.py` turns a checkout into installable packages that need neither `.sources/` nor the repository at run time. It needs no secret and always builds **unsigned** packages, so it is also how to test packaging locally. From the repository root, after setup, with [cargo-packager](https://github.com/crabnebula-dev/cargo-packager) installed (`cargo install cargo-packager --locked --version 0.11.8`):

```sh
python3 desktop/scripts/package.py                     # this OS's formats, version from desktop/Cargo.toml
python3 desktop/scripts/package.py --formats app       # macOS: just OctoSense.app
python3 desktop/scripts/package.py --kernel <octos>    # ship a kernel you built (its --version must be the pinned revision)
python3 tools/release-scan.py target/octosense-package/dist/*   # refuse private paths before sharing anything
```

| OS | Packages (in `target/octosense-package/dist/`) | Resources in the package |
| --- | --- | --- |
| macOS (arm64) | `OctoSense.app`, `OctoSense_<version>_aarch64.dmg` | `OctoSense.app/Contents/Resources/<crate>/resources/` |
| Windows (x64) | `octosense_<version>_x64-setup.exe` (NSIS, per-user) | beside `octosense.exe` in the install directory |
| Linux (x86_64) | `octosense_<version>_amd64.deb`, `octosense_<version>_x86_64.AppImage` | `/usr/lib/octosense/` (`usr/lib/octosense/` in the AppImage) |

What a package contains and how it is found at run time:

- **Resources.** The build sets `MAKEPAD_PACKAGE_DIR` (and `MAKEPAD=apple_bundle` on macOS), so Makepad reads every `crate_resource` from the package: `Contents/Resources` through `NSBundle` on macOS, the executable's directory on Windows, `../lib/octosense` from `usr/bin/octosense` on Linux. The script stages the `resources/` of every git or path crate the app links: Makepad's widgets (fonts, icons, textures), the shell's icons and themes, App Hub, Rinx and its article crates.
- **No checkout.** A packaged build never looks for an OctoSense checkout: not the one it was built in, not its working directory, not its executable's ancestors (`crates/shell/src/octosense/paths.rs`, `packaged()`). Starting the installed app inside someone else's checkout therefore cannot make it build and run that code. It has no developer-program catalog (those rows build from source); `--apps <file>` with `executable` rows still works.
- **System apps** (News, Photos, Maps, Camera, Mail, AI providers) are already in the binary: App Hub packs the bundles `system-apps.json` selects at build time. Nothing else is read from `apps/` or `desktop/config/`.
- **The octos kernel.** The script builds octos at the revision `Cargo.lock` pins with `tools/kernel-artifact.py --host`'s steps and stages it with its `stage` (which checks the binary's `--version`). It ships as `octos-kernel` beside the executable (`Contents/MacOS/` in the app), with its receipt `octos-kernel.json` among the resources; the kernel service runs it only when the receipt names the pinned revision and the binary's SHA-256 ([Build and run](#build-and-run)). `--no-kernel` ships none, and the app then runs without an assistant. `target/octosense-package/receipt.json` records the version, resource crates and the kernel's receipt.
- **Linux kernel receipts are per format.** `linuxdeploy` adds a RUNPATH to the AppImage kernel after staging. Packaging verifies that transformation, updates the AppImage's embedded receipt, rebuilds its SquashFS without running the image, and re-extracts it to verify the permitted changes. Apart from the host-library exclusions below, every other file, link and its metadata must survive. This requires `squashfs-tools`; unsupported signatures, ownership or xattrs stop packaging. The external `receipt.json` keeps `kernel` as the staged/raw receipt (`kernel_scope: staged_before_packaging`) and records final file/kernel hashes separately in `packages`. The DEB's original kernel hash stays unchanged. Publication still requires a full privacy scan of the finalized artifacts.
- **Use the host's graphics stack.** AppImage packaging excludes Wayland client, cursor, EGL and server libraries. An older bundled Wayland library can override the host Mesa driver's dependency through `LD_LIBRARY_PATH`, preventing even X11 startup. The finalizer also removes those exact library families from `usr/lib` in already packaged images and records their hashes or link targets; all other payload bytes stay unchanged. The host must supply the matching Wayland/Mesa libraries. The DEB is unchanged.
- **Private-path-free.** Paths in the binaries are remapped (`--remap-path-prefix` for the home directory, `CARGO_HOME` and the checkout) and debug info stripped (symbol names stay, for readable backtraces). Crates also embed their source directories as plain strings, which remapping does not reach, so build from a directory outside any user's home, with `CARGO_HOME` outside it too (the release workflow does). `tools/release-scan.py` fails on `/Users/…`, `C:\Users\…` and homes other than a CI runner's (`/home/runner`, `C:\Users\runneradmin`), `*.local` hosts, private IPv4 addresses, the scanning account's and host's names and any `RELEASE_SCAN_EXTRA` pattern, inside the `.app`, `.dmg`, `.deb`, `.AppImage`, `.zip` and NSIS installers.
- **Package inspection does not run the AppImage.** The scanner reads its type-2 ELF boundary and extracts its filesystem with the host's `unsquashfs` (`squashfs-tools`, installed by the Linux build and release jobs). Missing tools, malformed headers and extraction failures stop publication. The GitHub publication token is scoped to the tag-verification and upload steps; it is absent during artifact inspection. Packaging also removes the inline `APPLE_API_KEY_P8` signing credential from build subprocesses.
- **Identity.** Product name **OctoSense**, identifier `org.octosense.desktop` (`desktop/packaging/release.json`), icon from `desktop/packaging/icons/` (`make_icons.py` renders it). Android keeps `dev.makepad.octosense`.

### Cutting a desktop release

Connected-app sign-in needs a separate distributor setup: the current release workflow supplies no Google or GitHub client registration. A package built without one will show that provider as unavailable. Supply the [compile-time registrations](../crates/oauth-service/README.md#configure-a-release-maintainers) when building, or provision `<apps root>/.host/oauth/clients.json` privately on an operator-managed installation. Signing the executable does not configure OAuth. Do not ask ordinary app users to create provider developer projects; do not advertise public Google sign-in until its verification/audience requirements are met.

`.github/workflows/release-desktop.yml` (not part of `tools/ci-local.sh`):

1. Dry-run it on `main`: **Actions → Release desktop → Run workflow** on `main`, or `gh workflow run release-desktop.yml --ref main`. It builds, scans and (from `main`) signs all three platforms and keeps the packages as workflow artifacts for 14 days.
2. Tag the commit and push the tag: `git tag desktop-v0.1.0 <commit> && git push origin desktop-v0.1.0`. The version is the tag's (`desktop-v<major>.<minor>.<patch>[-<pre>]`). A manual run from `main` with `tag` set and `dry_run` off does the same for an existing tag; either way the workflow checks out **the tag**, not the branch, and verifies the commit before attaching anything.
3. The `package` jobs (macOS 14 arm64, Windows 2022, Ubuntu 22.04 x86_64, whose older glibc keeps the `.deb` and AppImage usable on older distributions) run `tools/setup.py`, the graph checks, `package.py` in a neutral directory and the scan, and upload **unsigned** packages. They have no secrets.
4. `sign-macos` and `sign-windows` run in the `release` environment and only download those packages: codesign (hardened runtime, `entitlements.plist`) of the kernel and the app, the kernel's receipt updated to the signed bytes, a new `.dmg`, notarytool and stapling; signtool for the installer. They never check out or build code.
5. `release` checks out the tag, verifies HEAD is the tag's commit, scans again and attaches every package, a receipt per platform and `SHA256SUMS` to a **draft** release for the tag, not marked latest (`home-v*` and `rom-v*` share this repository). Review the draft, try the packages, and publish it by hand.

Pull requests that change the packaging run the `package` jobs only: no secrets, no signing, nothing released. x86_64 macOS builds are not produced (the macOS runner is arm64).

### Signing

Without the secrets the signing jobs pass the packages through **unsigned**, with a warning: macOS Gatekeeper then asks to confirm the first open (right-click → Open), and Windows SmartScreen warns. To sign, create a GitHub environment named `release` (**Settings → Environments**), limit it to `main` and `desktop-v*` tags (and add required reviewers if wanted), and add these as its environment secrets, not repository secrets:

| Secret | For |
| --- | --- |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` | base64 of the **Developer ID Application** certificate and key (`.p12`) and its password; imported into a temporary keychain, deleted at the end |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: <name> (<team id>)` |
| `APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_P8` | App Store Connect API key id, issuer id and base64 of `AuthKey_<id>.p8`, for notarytool; the key file is deleted in an `always()` step |
| `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` | base64 of the code-signing `.pfx` and its password; signtool signs the installer from the file (never imported into a certificate store) |
| `RELEASE_SCAN_EXTRA` | optional: comma-separated regular expressions the final scan also refuses |

The signing paths have not run yet (no certificates): they are **unverified**. On Windows only the installer is signed; the `octosense.exe` and `octos-kernel.exe` it installs are not, because re-signing them would mean rebuilding the installer in the signing job.

## The app model

The launcher lists four kinds of app together:

| Kind | Comes from | Runs as | Launcher id |
| --- | --- | --- | --- |
| **System apps**: News, Photos, Maps, Mail, Calendar (desktop only), AI providers, YouTube (Camera ships on the phone only) | `../apps/<name>/bundle`, selected by `system-apps.json`, packed into the build | Contained Splash programs in App Hub's Card runner, each in its own isolate under the capabilities its manifest asks for | `<name>` (manifest id `os.<name>`) |
| **Store apps** | The signed App Hub catalog, installed from the store (`apphub`) | The same Card runner. Every open is checked against the catalog; an update closes old instances. | `hub:<manifest-id>` |
| **Native modules** | Rust crates linked into this binary | In-process `AppModule`s. Trusted code only: App Hub, AppCard, Rinx, Reference and the `app-*` features. | module id |
| **Developer programs** | `config/apps.json` | Separate processes in tiles, over Makepad's `--stdin-loop` hosting protocol, built on first launch | catalog `id` |

Precedence: a linked native module beats a system app of the same id, and a system app beats a catalog row of the same id. That is why Makepad's example Mail, Photos and Calendar are dropped from the shipped catalogs (`drop` in `config/apps.overlay.json`).

### Containment and permissions

A contained app is a bundle: `manifest.json` (id, version, capabilities) plus `main.splash`. The Card runner grants only the capabilities the manifest lists (Mail asks for `storage` and `mail`). The pinned Makepad ([makepad#30](https://github.com/OctoSense-org/makepad/pull/30)) enforces this at every exit of an isolate: network and web sockets answer to the app's host list, raw sockets and servers are refused, files stay in the app's storage jail, and password or one-time-code fields are inert inside a policed isolate.

### Host services and host-owned sheets

Secrets are the host's. An app that needs an account calls a **host service** through `host.request`; the service runs in the shell with the credentials, and the app never gets a socket or a password.

Mail is the worked example (`octosense-mail-service`, from [`../apps/mail/host-service`](../apps/mail/host-service)):

- `mail.add_account` raises the host's **sign-in sheet**, a separate isolate drawn over the app. Only that sheet's calls (`mail.sheet.submit`, `mail.sheet.cancel`) can carry a password.
- The service tests the account, stores the password in the platform secret store (macOS keychain), and grants the account only to the app that added it.
- Mail state lives under the host's own directory, outside every app's jail.

AI providers (`os.ai-providers`) edits the octos kernel's LLM providers through the `llm` service (`octosense-llm-service`, from [`../apps/ai-providers/host-service`](../apps/ai-providers/host-service)). Keys are typed only on host sheets and go to the macOS keychain entry octos reads; the providers are written to the kernel's profile under the shell's octos core dir (`<core dir>/profiles/_main.json`; core dir `OCTOS_APP_CORE_DIR`, else `~/octos-home/.octos`). A phone's provider QR is imported from a picture of it: **Choose image** opens the open panel, or drop a screenshot on the import sheet. **Start → Settings → AI providers** opens it. After a change the service restarts the kernel if one runs; its consumers (AppCard) reconnect to the new one.

**The octos kernel** is a shell service, not part of any app: `octosense-kernel` ([`../crates/kernel`](../crates/kernel), feature `octos-core`, default). The shell starts it through its AI services at startup ([`../crates/ai-host`](../crates/ai-host/README.md), `octosense_ai_host::start`); nothing runs until a consumer connects, then one kernel per process (the packaged `octos-kernel` or `OCTOS_APP_CORE_BIN`, with `serve --stdio --data-dir <core dir>`, on a desktop; the APK's `liboctos.so` on Android; none on iOS or on a desktop with neither). AppCard's agent connects to it; Rinx reaches it through the app-agent broker. It stops when the last consumer leaves and when the shell exits.

**Talk to Octos** (off by default): **AI providers → Talk to Octos** turns on a loopback server so a web client or a terminal UI can talk to this device's assistant. While it is on, the kernel runs as `octos serve --host-managed` instead of `--stdio` and native apps keep working over its WebSocket; external clients get a separate token that opens the UI Protocol socket and nothing else. A web client pairs with a one-time code or the QR of its link; a terminal client of this user reads the private connection file. The server stays up when native apps close, until it is turned off or the shell exits. See [ADR 0003](../docs/adr/0003-shared-octos-client-access.md) and the [kernel guide](../crates/kernel/README.md).

New app features that need a password, PIN or token belong in a host service and a host sheet, never in the app's own UI.

### Store apps (App Hub)

App Hub is on by default. Open **App Hub** from the launcher to browse the signed catalog and install apps; installed apps appear in the launcher without a restart. The catalog origin defaults to the App Hub repository and can be pointed elsewhere with `OCTOSENSE_HUB`. To build and publish an app, start from [App Flow](https://github.com/OctoSense-org/OctoSense-App-Flow).

#### Try your own app before it is published

App Hub accepts only GitHub-attested releases, so you need no publisher key: the app's GitHub repository is its publisher identity ([App Hub ADR 0002](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/adr/0002-github-attested-publisher-identity.md)). For public distribution, use the GitHub release workflow in App Hub's [submission guide](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/SUBMITTING.md#5-produce-the-final-bytes): install it with App Flow's `tools/octo publish-github <app-directory>`, commit it to the app's public repository together with the tested app, then push a new `v<manifest.version>` tag. Add the release evidence to an [App Hub submission issue](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/new?template=submit-app.yml), opening one if needed. A tag or Release alone is not Hub approval.

The following is an **optional local rehearsal**. It uses a legacy-format test catalog that you sign with throwaway catalog keys (`hub keygen`, `certify`, `publish`), and it publishes nothing to App Hub. It needs no publisher key: use the release pack that your release workflow built, and never restamp a GitHub-attested release. App Flow's [PUBLISHING §4](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/PUBLISHING.md#4-rehearse-the-store-path-locally) has the steps. Installing a GitHub-attested release from such a catalog is **unverified**.

After preparing the mirror, replace the two quoted values below. Always use a fresh profile: a library that already cached the v2 catalog refuses a legacy downgrade. **This revised launch recipe is source-reviewed, not yet run against the newly compatible shell binary.**

```sh
REHEARSAL_MIRROR='/absolute/path/to/your/test-mirror'
REHEARSAL_ANCHOR='replace-with-test-anchor-public-hex'
REHEARSAL_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/octosense-hub-rehearsal.XXXXXX")"
OCTOSENSE_HUB_CATALOG=legacy \
  OCTOSENSE_HUB="$REHEARSAL_MIRROR" OCTOSENSE_HUB_ANCHOR="$REHEARSAL_ANCHOR" \
  OCTOSENSE_HOME="$REHEARSAL_ROOT/shell" OCTOSENSE_APP_DATA="$REHEARSAL_ROOT/apps" \
  OCTOS_APP_CORE_DIR="$REHEARSAL_ROOT/core" \
  cargo run --release -p octosense
```

Open **App Hub**, choose the app, **Get**, scroll to **Install**, then **Open**: it runs in the Card runner under its manifest. The earlier legacy-catalog UI flow was verified on macOS on 2026-09-26 with a new script app, before the repository merge; that historical result does not verify the new GitHub publisher path. The separate shell/app directories keep this rehearsal out of your normal profile, and the explicit kernel directory prevents copying personal provider settings into it. This local catalog does not submit or publish an app to the official Hub.

### Choosing and overriding system apps

`system-apps.json` names the apps and where their bundles are (paths relative to this file):

```json
{
  "schema": 1,
  "source": "../apps",
  "apps": ["news", "photos", "maps", "mail", "ai-providers"],
  "assets": {}
}
```

- Remove an id from `apps` to leave it out; point `OCTOSENSE_SYSTEM_APPS` at another file for a different selection. Without that variable the build ships no system apps.
- To change a bundle, edit it in `../apps/<name>/bundle` and rebuild; it ships with the next desktop build, in the same pull request.
- The desktop mounts no photo library, so Photos shows the thumbnails its bundle ships. To give it full-size photos, add `"assets": {"photos": {"photos": "<dir>"}}`.
- A linked native module of the same id would override a system app; none does (the native News, Photos and Maps modules are deleted).

### Developer programs and the catalog

`config/apps.json` lists Reference and the Makepad apps OctoSense picked (Browser, Files, Task, Terminal, Sheets, Clock, Weather, Finance, Notes, Reminders, Calculator, Route, and the Image and PDF viewers). Calculator, Clock, Notes, Reminders and Weather are native apps ([`../native-apps.json`](../native-apps.json)): linked by default and opened in-process, on the desktop and on the phone, with their read tools offered to the system agent while they are open. Task is a native app that runs only as its own process (`"module": null`), sandboxed. Terminal is also linked (`app-terminal`, on by default); its `config/apps.json` row is the process form it opens in on macOS and Windows, and the linked module is the in-process form. The `aichat` row is the assistant pane's own process (F10), which the pane starts; no list shows it. A launcher row whose id is listed in `wm/launcher.hides` under the state directory is hidden.

Catalog lookup: `--apps <file>` if given, else `~/.octosense/apps.json` if it exists, else `config/apps.json`. A catalog is a JSON array; each entry picks one launch target:

```json
[
  { "id": "notes", "label": "Notes", "manifest": "../notes/Cargo.toml", "package": "my-notes", "bin": "notes", "policy": "new", "args": [] },
  { "id": "installed-notes", "label": "Installed Notes", "executable": "/opt/my-apps/notes" },
  { "id": "browser", "label": "Browser", "source": "makepad", "package": "makepad-browser", "bin": "browser", "policy": "focus" }
]
```

- `"source": "makepad"` resolves through Cargo to the same Makepad checkout as the host (`.sources/makepad`); such builds go to `~/.octosense/build/makepad`.
- Step-by-step guides (Chinese): [open a hosted app](docs/open-apps.md), including the fix for an empty catalog when `cargo metadata --offline` fails, and [open the full Makepad catalog](docs/add-all-makepad-apps.md).
- Relative paths resolve from the catalog's directory. Arguments are passed literally, without a shell.
- `policy`: `"new"` opens another instance; `"focus"` (default) focuses a running one.
- Do not add `--stdin-loop` or Studio variables; the shell adds them. Restart after editing.
- A hosted program must be a Makepad app built against the same Makepad revision; the hosting protocol is not stable across revisions. Start from `../apps/reference`.

The Makepad rows are generated from upstream's app registry at the pinned revision:

```sh
python3 scripts/upstream.py catalog          # report drift
python3 scripts/upstream.py catalog --apply  # rewrite config/apps.json and apps.makepad.json
```

Only the upstream apps named in the overlay's `pick` reach `config/apps.json`: an app upstream curates later stays out until it is picked, and a pick upstream no longer curates is reported. `--apps config/apps.makepad.json` starts the desktop with all of them.

### The AppCard assistant

AppCard is **not shipped for now**: it interfered with the other apps, so no build links it unless asked. Default, `mobile-apps`, Android and iOS builds leave its UI out (the octos kernel service stays), and it has no tile, group or launcher entry. `--features app-appcard` brings it back on any target (for a phone, pass the feature to `cargo makepad`).

`../apps/appcard/module` (package `octosense-appcard`, feature `app-appcard`, opt-in) hosts the whole AppCard assistant in one tile: `octos-app` from `../apps/appcard/app/app`, built without its `standalone` feature. Routing, cards, sessions, the composer and the kernel agent run inside the tile's isolate; `ask` is the module's AI-bus tool.

```sh
cargo run --release -p octosense --features app-appcard -- --module appcard
```

It starts no kernel of its own: it connects to the shell's. On desktop that is the packaged `octos-kernel` or `OCTOS_APP_CORE_BIN` (and optionally `OCTOS_APP_CORE_DIR`); without a kernel it shows its login / WebSocket screen. Every octos crate comes from octos-org/octos at the one revision the root `Cargo.toml` pins.

## The glance panel

Apps and their agents publish glance cards with `glance.publish` ([how cards work](../README.md#cards-and-questions)). On the desktop they appear here:

| Surface | What happens |
| --- | --- |
| Panel | A new card opens the glance panel unless a card window is already open. The bar’s bell or F9 also opens the panel. A press on a card outside its own controls opens it in the card window. The hovered card shows its open and dismiss actions, and a card that just came wears an accent mark for a few seconds. A card that does not fit whole peeks in at the end of the list. A card taller than its tile scrolls inside it with the wheel; a card with an in-card chat stays at its newest rows, so its field and the latest exchange stay in view. Opened with F9 (or a press in it), the panel has the keyboard: the arrows move a focus ring from card to card, Return opens the card, Delete dismisses it and Esc closes the panel. |
| Notifications | A card published with `notify` also posts a toast with the app’s icon and name, the card’s title and its `summary` (else the card’s own). Selecting the toast opens the card in its own window. At most three toasts show at once; a “+N more” chip under them shows the rest. While the panel is open, toasts stack to its left. |
| Dismissal | The hovered card shows a dismiss button: `glance::dismiss` removes the card as if the app had withdrawn it. Clear all dismisses every card. A dismissal can be undone from the toast that reports it, or with ⌘Z while the panel has the keyboard. The panel has a separate close button. |

On a phone, `notify` posts a shade notification instead, and selecting it opens the glance page.

A card that names no theme takes the shell’s light or dark palette. Toasts and the panel slide in; `OCTOSENSE_REDUCE_MOTION=1` keeps them still. The code: [`glance_panel.rs`](../crates/shell/src/glance_panel.rs), [`glance_sheet.rs`](../crates/shell/src/glance_sheet.rs) and [`notifications.rs`](../crates/shell/src/shell/notifications.rs).

## Demos

### Mail without an account

```sh
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release -p octosense
```

Open **Mail**, sign in on the host sheet with any address and password `demo`. The demo serves sample messages from a file vault: no network, no keychain.

With a real account on an unsigned development build, macOS asks for keychain access again after every rebuild. For development, keep passwords in a file instead:

```sh
OCTOSENSE_MAIL_VAULT=file cargo run --release -p octosense
```

### Remote-control bridge

Every desktop Makepad app, this shell included, carries a localhost HTTP control surface. Start it with `MAKEPAD_REMOTE=<port>`, `MAKEPAD_REMOTE=on` (ephemeral port; a number is always read as the port, so `1` means port 1 and fails), or `--remote[=PORT]`:

```sh
MAKEPAD_REMOTE=8399 cargo run --release -p octosense
# prints: [makepad-remote] listening on 127.0.0.1:8399 pid=... app=... grabs=...
```

| Route | Does |
| --- | --- |
| `/` | Cheat sheet of every route. |
| `/s` | Windows and their geometry. |
| `/snap?q=` | Visible widgets with rects and text, filtered by id/type/text. |
| `/click?x=&y=` | Click at window-local layout points. `/m`, `/k`, `/t` for mouse, keys, text. |
| `/g` | Grab a window to PNG. |
| `/log?n=` | Tail the log. |
| `/gq` | Grab every window, then quit. Use this (or `/quit`) to end any session you started. |

Add `&wait=1` to an input route to answer after the next frame. The bridge injects real input and serves screenshots: bind a non-loopback host (`MAKEPAD_REMOTE=0.0.0.0:8399`) only on a trusted network.

### Headless UI checks

On macOS, `MAKEPAD_HIDE_WINDOWS=1` keeps windows off screen while still rendering, so a remote-driven run does not take over the display:

```sh
MAKEPAD_HIDE_WINDOWS=1 MAKEPAD_REMOTE=on cargo run --release -p octosense
```

Makepad's [`makepad_test`](https://github.com/OctoSense-org/makepad/tree/main/libs/makepad_test) crate (in `.sources/makepad`) builds on the same two pieces: `#[makepad_test]` tests launch the app hidden, drive it over `--remote` with selectors and waits, and close it with `/gq`. The desktop does not have a `makepad_test` suite yet.

## Phones

With the Makepad Android toolchain installed and a device on ADB:

```sh
cargo makepad android run -p octosense --release
```

Phone builds of this package always link Reference and Sheets and the octos kernel service, and App Hub with the system apps through the default feature; AppCard only with `--features app-appcard`. The launcher label is **OctoSense**, application id `dev.makepad.octosense`. The APK must bundle the kernel as `liboctos.so`: `python3 ../tools/kernel-artifact.py --sdk <cargo-makepad Android SDK> -- cargo makepad android run -p octosense --release` (from `desktop/`) cross-builds `octos` at the revision the workspace `Cargo.lock` pins and runs the packager with `MAKEPAD_ANDROID_EXTRA_LIBS=liboctos.so=<octos>` (`--kernel <path>` for a prebuilt one). Without it the phone runs no kernel; the AI providers are still saved. AppCard's Java features (GPS, notifications, share, intents) need the fork's buildtool; see [docs/android-appcard-build.md](docs/android-appcard-build.md). The dedicated phone shell is [Home](../phone/README.md) (package `octosense-home`); this Android build is for development.

## Desktop styles and settings

- Eight desktop styles. Desktop builds start in **OctoSense**, with Liquid Glass frames and a **Light / Dark** switch in the top bar; the others are Omarchy, macOS, Windows, Windows 2000, NeXTSTEP, iOS and Android. Theme sources are in `../crates/shell/resources/themes/`, wallpaper provenance in [crates/shell/resources/wallpapers/README.md](../crates/shell/resources/wallpapers/README.md).
- Keys: **⌘Space** menu, **⌘W** close tile, **⌘F** tile fullscreen, **⌘1…0** workspaces, **⌘Shift1…0** move tile. On macOS, **⌘A** selects all in the focused app; **Ctrl+Alt+A** remains the window manager's alternate-layer prefix. **Learn → Keybindings** lists them; see [KEYBINDINGS.md](KEYBINDINGS.md).
- State lives in `~/.octosense` (`OCTOSENSE_HOME`); hosted apps get it as `MAKEPAD_HOME`.
- Local models for the AI pane (**F10**): [docs/local-ai.md](docs/local-ai.md). The desktop works without a model.

## Pins and upstream sync

Every external dependency is pinned once for the whole repository: Makepad and OctoScript through `native-runtime.lock.json` (and the reviewed patch in `runtime-patches.lock.json`), App Hub, octos and Rinx in the root `Cargo.toml` `[workspace.dependencies]`. The system apps, the host services, the kernel service and AppCard are in this repository and change in the same pull request as the shell; there is nothing to pin. After moving a pin: `python3 tools/setup.py --update`, `cargo update` as needed, then `python3 tools/setup.py --check --cargo` and the tests below.

`scripts/upstream.py sync|status|diff|update` tracks the WM files imported from official Makepad (`upstream/makepad.json`, baseline `74b63be8`); see [docs/upstream.md](docs/upstream.md). It needs `--source` pointing at a full clone of official Makepad: `.sources/makepad` is a checkout of the fork and does not contain the baseline commit.

## Testing

CI (`.github/workflows/desktop.yml`) runs on macOS 14 for changes under `desktop/`, `crates/`, `apps/`, `tools/` and the workspace files:

```sh
python3 tools/setup.py
cd desktop
cargo check --locked -p octosense
cargo check --locked -p octosense --features mobile-apps
cargo check --locked -p octosense -p octosense-reference -p octosense-appcard --features mobile-apps,app-appcard
bash ../tools/check-shell-graph.sh -p octosense   # AI services linked, no AppCard UI without app-appcard, Rinx only as a module, one Makepad/App Hub/octos (host and aarch64-linux-android)
cd ..
python3 -m unittest discover -s tools -p 'test_*.py'
python3 tools/setup.py --check --cargo
```

It also fails when a shell source file exists in two of `crates/shell/src`, `desktop/src` and `phone/src`.

The desktop job does **not** run `cargo test` (the shell's tests run in `phone.yml`), the `desktop/scripts` tests other than `test_package.py`, or the smoke tests; run those locally before opening a pull request:

```sh
(cd phone && cargo test --locked --features mobile-apps -p octosense-shell)   # the shell's tests, as phone.yml runs them
python3 -m unittest discover -s desktop/scripts -p 'test_*.py'
python3 desktop/scripts/upstream.py catalog
```

Native smoke tests open their own windows, isolate state in a temporary directory and drive the shell over the remote bridge. They need GUI access and a prior release build (**unverified** since the move into `desktop/`: `smoke.py` still looks for Reference at `desktop/apps/reference`):

```sh
cargo build --release --locked -p octosense
python3 desktop/scripts/smoke.py --styles
python3 desktop/scripts/smoke.py --cargo-run --default-catalog
```

Results for each change are recorded in [docs/validation.md](docs/validation.md).

## Known gaps

- Windows/Linux package builds and focused native tests do not establish installer GUI acceptance or a complete desktop UX soak. The iOS build still fails in the pinned Metal backend.
- Ordinary embedded pages need GTK 3 and WebKitGTK 4.1 (or compatible 4.0) on Linux X11/XWayland, or a supported WebView2 Runtime on Windows. Neither engine is bundled; see [runtime requirements](../docs/desktop-embedded-browser.md#runtime-requirements). Native Wayland embedding and Windows/Linux embedded backend authentication remain unsupported; backend login there uses the external browser.
- Protected connected-account writes still fail closed on Windows/Linux because trusted native approval is unavailable. The packages do not include Google/GitHub OAuth client registrations; an operator must configure the host before users can sign in ([account service](../crates/oauth-service/README.md#current-delivery-boundary)).
- Source builds (`cargo run`) read fonts and resources from the `.sources/makepad` checkout, so keep it in place; [release builds](#release-builds) carry their own. RC1 packages have no Apple Developer ID signature/notarization or Windows publisher signature; the release notes distinguish local macOS builds from tagged CI packages.
- Photos on desktop has thumbnails only unless you mount a photo directory.
- The hosted AppCard assistant does not yet wire notifications, share or the WebView overlay.
- Mobile Sheets needs grid-label and toolbar fixes ([BACKLOG.md](BACKLOG.md)).
- Native host-API and embedded-browser fixtures provide focused CI checks; they are not a complete `makepad_test` suite or UI soak of every app.

## Contributing

`main` is protected: every change goes through a pull request (admins included), and force pushes are blocked. Branch from `main`, run `python3 tools/setup.py --check --cargo`, the Rust and Python tests above and, for UI changes, a smoke or remote-driven run with hidden windows; record native checks in `docs/validation.md`. Keep the one-Makepad, one-octos, one-App-Hub rule: `python3 tools/setup.py --check --cargo` must pass. Rules for people and coding agents: [AGENTS.md](../AGENTS.md).

## License

Apache License 2.0 ([LICENSE](../LICENSE), [NOTICE](../NOTICE)). Source copied from Makepad keeps its [MIT notice](../LICENSES/Makepad-MIT.txt). Dependencies keep their own licenses.
