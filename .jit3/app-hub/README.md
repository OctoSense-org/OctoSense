# OctoSense App Hub

English | [简体中文](README.zh-CN.md)

The App Hub publishes apps for OctoSense. This repository holds:

- the signed catalog that every OctoSense store reads;
- App Hub's copy of each admitted bundle;
- the code for the gate (the admission checks every bundle must pass), signing,
  the store and the reference host.

Each app's source stays in its publisher's own repository.

| Looking for | Repository |
| --- | --- |
| How to build an app: quickstart, script API, script-app template, design flows, examples | [OctoSense App Flow](https://github.com/OctoSense-org/OctoSense-App-Flow) (formerly Design Flow) |
| The AppCard assistant runtime (opt-in in the shells, `--features app-appcard`) | [OctoSense `apps/appcard`](https://github.com/OctoSense-org/OctoSense/tree/main/apps/appcard) |
| The first-party system apps (AI providers, Calendar, Camera, Mail, Maps, News, Photos, YouTube) and their host services (`llm`, `model`, `calendar`, `mail`, `news`) | [OctoSense `apps/`](https://github.com/OctoSense-org/OctoSense/tree/main/apps) |
| The L0 parser and checker, and the Makepad lowering and renderer | [OctoScript](https://github.com/OctoSense-org/OctoScript) and [OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad) |
| The bundle format, the gate, signing, submission and the store | this repository |

**Building an app?** Start with the "read these first" list on the
[OctoSense-org profile](https://github.com/OctoSense-org): the `AGENTS.md` of
App Flow, then its `docs/QUICKSTART.md`. Next,
follow [Build your first Hub app](docs/FIRST-APP.md) and
[App icons and bundled artwork](docs/ICONS.md). Clone this repository beside
your App Flow checkout to build `hub` and `card-host`. Do not edit
`catalog.json`, `index/` or `artifacts/`.

**Submitting an app?** To request publication, open an issue here with the
repository, version and requested capabilities, as
[Submit an app to the App Hub](docs/SUBMITTING.md) describes. You can open it
before the release is ready and add the tag, commit, screenshots and verified
release pack later; a tag or GitHub release alone submits nothing. That
guide's four stages cover review, approval and publication, and its
[`hub` command table](docs/SUBMITTING.md#the-hub-command) lists who runs each
command and when.

The catalog's [three reference apps](docs/SUBMITTING.md#the-three-reference-apps)
(GitHub Notes, Inbox Assistant and Google Calendar) passed admission end to
end; their repositories show a complete submission. Look up rules,
capabilities and fields in the [publishing reference](docs/PUBLISHING.md).

**Using host APIs?** [Host API compatibility](docs/HOST-API.md) shows how an
app declares the host APIs it needs and checks which ones a host implements.

## Download a compatible host

[**Desktop 0.1.0-rc.4**](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.4) is the compatible prerelease,
built from source `9266b008`. It includes contract 1.11, declaration-only
capability policy, Wasm core modules and components, and shared-component
catalog/installation support. Existing Host API and connected-account flows
retain their platform, consent and provider limits.

| Platform | Download |
| --- | --- |
| macOS Apple silicon | [DMG](https://github.com/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.4/OctoSense_0.1.0-rc.4_aarch64.dmg) or [app ZIP](https://github.com/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.4/OctoSense_0.1.0-rc.4_macos_aarch64.app.zip) |
| Windows x64 | [Installer](https://github.com/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.4/octosense_0.1.0-rc.4_x64-setup.exe) |
| Linux x86_64 | [Debian package](https://github.com/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.4/octosense_0.1.0-rc.4_amd64.deb) or [AppImage](https://github.com/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.4/octosense_0.1.0-rc.4_x86_64.AppImage) |

Check downloads against [SHA256SUMS](https://github.com/OctoSense-org/OctoSense/releases/download/desktop-v0.1.0-rc.4/SHA256SUMS) and read the
[release notes](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.4) for platform limits.
These packages have **no Apple Developer ID signature or notarization, and no Windows Authenticode signature**. The Mac binary has only a linker ad-hoc signature. Package privacy scans passed on all three platforms.
The [release assets](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.4) include platform packaging receipts and exact checksums.
The Mac app ZIP passed **11/11 App Hub/component checks** on its first candidate run; final release ZIP and DMG hashes match the tested bytes. This does not establish Windows/Linux installed GUI acceptance, live-model behavior or account writes.
To build yourself, follow the
[pinned setup guide](https://github.com/OctoSense-org/OctoSense/blob/9266b0083544d86bd7636543b5ff60c61b26460f/README.md#set-up).

For the four samples below, use **macOS on Apple silicon**. Open **App Hub →
Search**, enter the exact app ID, choose **Get**, review its usage disclosures
and choose **Install**, then **Open**. **Library** reopens an installed app
and offers **Update** when a compatible newer version is available. Keep the
default public catalog; no custom origin, anchor, developer key or OctoSense
cloud account is needed. An older desktop beta.2 host cannot read the new publisher
proof or public v2 channel: install a compatible host instead of changing the
app's proof or catalog settings.

RC4 supports shared components, but a component or app still needs separate
Hub review and catalog publication before users can find it. Installing this
desktop release does not upgrade Android Home; existing phone releases and
isolated OnePlus acceptance fixtures are separate.

RC4 does **not** ship public GitHub/Google OAuth client registrations.
Local drafts and no-account screens work; provider sign-in needs registration
supplied by the host distributor/operator ([configuration](https://github.com/OctoSense-org/OctoSense/blob/9266b0083544d86bd7636543b5ff60c61b26460f/crates/oauth-service/README.md#configure-a-release-maintainers)).
Ordinary users should not need to register a Google developer project.
Installing an app does not prove login, email delivery, a GitHub commit or a
Calendar write. The connected samples declare macOS only. Linux/Windows
implement external-browser backend login and declared backend reads. The
Windows synthetic-backend sign-in result recorded for RC2 is historical,
not acceptance of a newly installed RC4 package. Linux refuses embedded backend
login; the Windows runtime can intercept callbacks, but RC4's host-owned
embedded login flow is disabled, with no production opt-in. Both platforms
use the external browser instead; protected writes remain unsupported and
fail closed. Android Google
authorization remains unavailable. Ordinary WebReader is separate from login;
Linux needs GTK 3/WebKitGTK and X11/XWayland, and Windows needs WebView2. Those
engines are not bundled; see the [browser requirements](https://github.com/OctoSense-org/OctoSense/blob/9266b0083544d86bd7636543b5ff60c61b26460f/docs/desktop-embedded-browser.md).

## Repository layout

| Path | What it is |
| --- | --- |
| `catalog-v2.json` | The GitHub-attested catalog selected by default in new compatible hosts. Stores verify its complete proof before using it. |
| `catalog.json` | The unchanged legacy signed catalog for older hosts and explicit legacy mirrors. |
| `index/<app>-<version>.json` | One admitted entry per app version: its manifest, publisher, source and status. A maintainer exports it from the catalog after `hub publish`. |
| `artifacts/<app>-<version>.bundle/` | App Hub's copy of the bundle, exactly the bytes that were reviewed. `hub publish` creates it. |
| `artifacts/<app>-<version>.bundle.pack.json` | The same bundle as one file, which stores download. |
| `artifacts/<component>-<version>.wasm` and `index/components/<component>-<version>.json` | A shared component version that apps pin: App Hub's copy of the reviewed file and its admitted entry ([ADR 0003](docs/adr/0003-shared-components.md)). None is published yet. |
| `docs/FIRST-APP.md` | A first-app walkthrough for a card app or a script app: create, run, capture and check. |
| `docs/SUBMITTING.md` | The submission's four stages, who runs each `hub` command, and every step from repository layout to publication. |
| `docs/PUBLISHING.md` | The reference: gate rules; capabilities and who serves them; manifest, listing and tool fields; host services; `hub` commands; signing. |
| `docs/GITHUB-PUBLISHING.md` | Admin-authorized GitHub signing, exact candidate review and v2 migration. |
| `docs/HOST-API.md` | Declaring the host APIs an app needs, `runtime` discovery and what today's hosts implement. |
| `docs/ICONS.md` | Canonical icon ownership, export constraints and visual review. |
| `docs/DEVELOPMENT.md` | The guide map, delivery paths, `card-host` and its remote-control routes, and `card-studio`. |
| `docs/adr/` | Architecture decisions: GitHub-admin catalog attestations ([ADR 0001](docs/adr/0001-github-attested-catalog.md)), GitHub-attested publisher identity ([ADR 0002](docs/adr/0002-github-attested-publisher-identity.md)) and shared components ([ADR 0003](docs/adr/0003-shared-components.md), proposed). |
| `templates/app/` | A card app repository scaffold with metadata, an example icon and linked agent instructions. |
| `crates/app-contract` | The app contract, `octosense-app-contract` ([OctoSense ADR 0005](https://github.com/OctoSense-org/OctoSense/blob/main/docs/adr/0005-app-contract.md)): the manifest, the policy an app gets, bundle integrity and running a bundle. Within `1.x` it only grows ([README](crates/app-contract/README.md)). Contract 1.11.0 is published on crates.io ([Versions on crates.io](crates/app-contract/README.md#versions-on-cratesio)). |
| `crates/app-policy` | The signed manifest and listing, admission, and resolution into an isolate's settings and an agent session profile ([OctoSense Home ADR 0002](https://github.com/OctoSense-org/OctoSense/blob/main/docs/adr/home/0002-agentic-app-security-model.md)); an app's own agent (`tools.json`, `AGENT.md`, skills) loaded as an `AgentBundle`; the `tools.json` parser and checks that native modules' tool manifests share (`ToolManifest::load`). It also re-exports the app contract. |
| `crates/app-hub` | The index, the signed catalog, the gate, the agent scan, the device client and the `hub` command ([OctoSense Home ADR 0003](https://github.com/OctoSense-org/OctoSense/blob/main/docs/adr/home/0003-app-hub-and-store.md)). |
| `crates/appstore` | The store as an OctoSense module, the `card` module that runs an installed app as its own client, system apps (`os.` ids) and host services with their sheets. |
| `crates/appstore-app` | The store as a standalone app (`appstore`). |
| `crates/card-host` | The reference contained host: it runs one bundle, a card app or a script app (`card-host`). |
| `crates/card-studio` | A tool that renders a card in a hidden `card-host --remote` at Glance, phone and desktop sizes, runs the measured checks and prepares the vision critique (`card-studio`, [OctoSense ADR 0002 section 7](https://github.com/OctoSense-org/OctoSense/blob/main/docs/adr/0002-event-driven-app-agents.md#7-cards-l0-grounded-rendered-and-critiqued-before-publishing)). |
| `crates/app-host` | A one-window host that runs any OctoSense `AppModule` as a standalone app. |
| `crates/app-hub-app` | The shell integration every OctoSense shell links: the native store module, the `card` runner module, the system apps named by `OCTOSENSE_SYSTEM_APPS`, installed apps and icons ([README](crates/app-hub-app/README.md)). |
| `skills/card-studio` | The octos skill over `card-studio`: `card_render`, `card_critique_payload`. |

## Build and test

The crates build against pinned revisions of OctoSense's Makepad fork,
OctoScript-Makepad and OctoScript, resolved from sibling checkouts
(`../makepad`, `../octoscript-makepad`, `../octoscript`);
[Build your first Hub app](docs/FIRST-APP.md#1-prepare-the-tools-and-an-app-repository)
sets them up. Build the two authoring tools:

```sh
cargo build --release -p octosense-card-host -p octosense-app-hub
```

To run `hub` from source without a release build, use
`cargo run -p octosense-app-hub --bin hub`. [Run the right host](docs/CODE-WALKTHROUGH.md#2-run-the-right-host) lists the
package tests. The [native tools CI](.github/workflows/native-tools.yml) runs
the release build above and those tests on macOS, Windows and Linux, against
sibling checkouts without OctoSense's runtime patches. Native font rendering
is checked in a graphical macOS session; the other jobs cover builds and tests.

Build only the packages you need. A build that includes the store packages
with their default features, such as `cargo test --workspace`, also needs
OctoSense's runtime patches on `../makepad`. If a build fails with
`no variant … TextInputStateQuery`, see
[`card-host` fails to build](docs/DEVELOPMENT.md#card-host-fails-to-build).

The store's UI uses one set of controls for desktop and phone layouts. To
preview it, build its example and run it with an app-data directory of its own:

```sh
cargo build --locked --release -p octosense-app-hub-app --example preview
OCTOSENSE_PREVIEW_SIZE=1200x860 OCTOSENSE_APP_DATA="$(mktemp -d)" target/release/examples/preview
```

`OCTOSENSE_PREVIEW_SIZE` sets the window: `1200x860` for desktop, or
`406x820`, the default, for phone dimensions. A phone-size window is not a
test on a phone. For automated checks, also set `MAKEPAD_HIDE_WINDOWS=1` and
`MAKEPAD_REMOTE=<port>`. The preview builds the store with its default
features, so it needs the runtime patches as well. Installing an app from the
preview still goes through the store's consent and admission checks.

## Code walkthrough

Start with [Run the right host](docs/CODE-WALKTHROUGH.md#2-run-the-right-host).
The walkthrough then traces a bundle into its UI, a host-service request back
to its callback, and one app-agent request for saved notes. Contributor rules
are in [AGENTS.md](AGENTS.md).

## What hosts serve today

The gate validates usage declarations; the host must implement each call and
check actual authorization. Current source treats capability and destination
fields as disclosures ([policy](docs/PUBLISHING.md#current-source-policy-declarations-and-authorization));
RC4 includes that policy, Wasm components and shared-component installation
with contract 1.11 on desktop. App/ABI admission, quotas, consent, account
isolation and reviewed writes remain. Catalog compatibility does not publish
a component. The inventory below records the earlier RC2 baseline.
[Capabilities](docs/PUBLISHING.md#capabilities) is the reference. The
[historical RC2 release](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.2) includes everything RC1 had, plus
the items marked RC2:

- Public GitHub v2 catalog and `publisher-github-v1` verification, with contract 1.10.0 (RC1: 1.8.0).
- `runtime` discovery and `script-tools-v1`: an admitted `implemented_by: "app"`
  tool calls the open full app's Splash handler; a closed app returns `app_not_running`.
- Connected-account services (`auth`, `github`, `gmail`, `gcalendar`), subject
  to provider registration and platform limits. Tokens remain with the host.
- Declared backend reads on macOS, Android, Linux and Windows; protected
  writes require a supported native approval path. Per-app camera, microphone
  and location consent covers macOS/Android; `location.get` is Android-only,
  and RC2's `location.sample` reads a fresh fix on macOS and Android.
- Physical confirmation for protected GitHub/Calendar saves and Gmail sends
  on supported platforms; agent Glance publication refuses executable Splash
  `script`; Calendar synchronization uses a bounded date window.
- `model` media methods as well as text, only with a configured, entitled
  provider. A registered method does not prove live provider execution.
- RC2: document import, export and photo selection (`files.*`) on macOS,
  Windows and Android, and on Linux with a dialog helper; text sharing on
  Android; binary writes to app storage (`fs.write_bytes`). 1 MiB per file.
- RC2: device calendars (`device_calendar.*`) through EventKit on macOS and
  the Android Home adapter, with physically reviewed writes; the OS-calendar
  interaction itself is pending acceptance.
- RC2: Mail drafts (`mail.compose`, `mail.compose_status`) for any app granted
  `mail`, and the native send review (`mail.review_send`), which approves only
  on macOS and Android.
- RC2: foreground audio playback and recording (`audio.*`,
  `microphone.record_*`) on macOS and Android, over files in the app's storage;
  hardware acceptance pending.
- RC2: `Video` widget playback controls, and native link openers for
  `LinkLabel` on Windows and Linux (browser launch unverified).

`card-host` serves only `runtime` discovery, runs no agent, and refuses sealed
releases or the host-only requirement markers. `calendar`, `llm` and `news`
remain system-app services; `photos`/`youtube` notifications serve only their
system apps. Wasm is **on in RC2 on macOS and Linux**: standard builds run the app's own
`fns/*.wasm`, with limited support; Windows, iOS and OpenHarmony builds leave
it out, and RC1 and every earlier release left it off. Supported Android Home
source builds run it too. Declaring a capability cannot add native code or
enable a build feature. See [Host API compatibility](docs/HOST-API.md).

## Trust anchor

Legacy stores trust this anchor and follow its certificate to the working key
that signs `catalog.json`. Rotating the working key needs no store release.

```text
6000284a069ba7cada2925094074e8e0baae07e25d1b7fc31f396c993f363e11
```

## Point a store at another hub

New compatible hosts select `github-v2` by default and fetch `catalog-v2.json`
from this hub. `OCTOSENSE_HUB` can select another directory or base URL, but
changing the origin never changes the trusted GitHub identity or channel.
A mirror must serve the same verifiable v2 envelope and artifacts.

For a **legacy local test hub**, explicitly select `legacy`, provide its trust
anchor and use a fresh app-data directory:

```sh
OCTOSENSE_HUB_CATALOG=legacy OCTOSENSE_HUB=/path/to/legacy-mirror \
OCTOSENSE_HUB_ANCHOR="<test-anchor-hex>" OCTOSENSE_APP_DATA="<fresh-test-directory>" \
appstore
```

Replace the placeholders before running this source-reviewed example. A library
with any v2 cache refuses a legacy downgrade, even if that cache is malformed.
An old legacy cache is not converted offline: the first v2 fetch needs network
access (or an explicitly supplied mirror containing a verified v2 envelope).
Fetch/proof failure never falls back to legacy. Older beta hosts continue to use `catalog.json`; use the
[historical RC2 release](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.2) for the public v2 catalog.

## How the Hub publishes an app

After you open a submission issue
([Submit an app to the App Hub](docs/SUBMITTING.md)), a reviewer runs the gate
on the exact bytes of your release and posts findings in the issue. An App
Hub admin then approves the submission; the Hub publishes nothing without
that approval. The admin publishes the approved entry with
[GitHub admin publication](docs/GITHUB-PUBLISHING.md): GitHub Actions signs
the new `catalog-v2.json` with Sigstore, so no admin keeps a separate Hub
private key. The RC1 and RC2 releases read `catalog-v2.json`; desktop beta.2 reads only the
legacy anchor-signed `catalog.json`, which `hub publish` produces.

For v2, publish a reviewed withdrawal candidate through the same protected
workflow, retaining history and a reason. Stores honor it after accepting the
new catalog. `hub withdraw` / `hub remove` affect the legacy catalog only;
they do not publish a v2 update.

## Apps

The authenticated public **catalog sequence 14** offers the GitHub publisher
identities below. The three historical `org.octosense.samples.*` apps are
withdrawn, including their 0.1.0 and 0.1.1 releases. Their installed data is
retained; the new IDs do not migrate it. RC1 still displays withdrawn apps as
unavailable; RC2 includes the discovery fix in [App Hub #170](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/170)
and hides them from browsing and search.

| App | Exact app ID | Latest version | Runs on |
| --- | --- | --- | --- |
| [GitHub Notes](https://github.com/ymote/octosense-github-notes/releases/tag/v0.2.1) | `io.github.ymote.githubnotes` | 0.2.1 | macOS |
| [Inbox Assistant](https://github.com/ymote/octosense-inbox-assistant/releases/tag/v0.2.1) | `io.github.ymote.inboxassistant` | 0.2.1 | macOS |
| [Google Calendar](https://github.com/ymote/octosense-google-calendar/releases/tag/v0.2.1) | `io.github.ymote.googlecalendar` | 0.2.1 | macOS |
| [Camera Card Demo](https://github.com/ymote/camera-card/releases/tag/v1.1.1) | `io.github.ymote.cameracard` | 1.1.1 | macOS |

All four are developer previews. Camera Card Demo is a **static L0 screen**,
not a camera capture app. The three connected apps retain local drafts and
require configured host OAuth for provider access; Calendar's agent advises
and does not book events. No app needs an OctoSense cloud account.

The public [first-admission candidate](catalog-candidates/ymote-github-samples-first/admission-review.json)
and [update candidate](catalog-candidates/ymote-github-samples-updates/independent-review.json)
record the reviewed releases. The [retirement review](catalog-candidates/retire-legacy-connected-samples/review.json)
records the six withdrawals. Catalog 14 preserves the preceding entries,
including these IDs' 0.2.0/1.1.0 versions. Historical
[0.1.1 admission](docs/admissions/connected-apps-0.1.1/README.md) and
[0.1.0 evidence](reviews/connected-apps-0.1.0/README.md) remain unchanged;
they describe earlier Ed25519 identities, not the new publishing path.
Follow [Download a compatible host](#download-a-compatible-host) for installation
and the current account/platform limits.
