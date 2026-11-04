# Submit an app to the App Hub

English | [简体中文](SUBMITTING.zh-CN.md)

Reviewers are App Hub maintainers, and an admin is a maintainer with admin
rights on this repository. Publishing an app on the App Hub has four stages:

| Stage | What happens | Step |
| --- | --- | --- |
| 1. Request | You open a submission issue with the repository, version and requested capabilities, then add the tag, commit, screenshots and release evidence. Opening the issue is your request to publish; you can open it before the release is ready. | [7](#7-open-the-submission-issue) |
| 2. Check | A reviewer runs the gate, the Hub's admission check, on the exact bytes at your tag and in your release pack, and posts any problems in the issue. | [8](#8-what-reviewers-check) |
| 3. Approval | An App Hub admin reviews the submission and approves it. | [9](#9-after-you-submit) |
| 4. Publication | The Hub publishes the approved catalog entry. People can then search for, install and run the app in a compatible OctoSense build. | [9](#9-after-you-submit) |

Pushing a tag or creating a GitHub release does not submit your app or
approve it.

App Hub accepts only GitHub-attested releases
([ADR 0002](adr/0002-github-attested-publisher-identity.md)). For an app's
first version and every update, your app's GitHub workflow prepares, attests
and packs the release, so you need no publisher key or repository signing
secret. Installing such an app takes a host that supports
`publisher-github-v1`; use the
[RC4 release and download guide](../README.md#download-a-compatible-host).

Public catalog 15 offers the GitHub-attested `io.github.ymote.*` reference apps
([current apps](../README.md#apps)). App Hub withdrew the older key-signed
`org.octosense.samples.*` entries in catalog sequence 14; later steps still cite
them as examples ([The three reference apps](#the-three-reference-apps)). The
gate, capability, manifest and signing rules are in
[PUBLISHING.md](PUBLISHING.md).

```text
issue request (may come first) → build hub and card-host → tools/octo doctor
→ repository, manifest, listing and screenshots
→ test editable source: tools/octo run, shot and check, hub scan
→ GitHub tag workflow → verify release pack: hub publisher-unpack, hub publisher-verify
→ add evidence to the issue → reviewer check → admin approval → catalog publication
```

## Before you start

A host runs your bundle: `card-host` while you develop, and an OctoSense shell
(the desktop or phone app) once people install it. A host service, such as
`github` or `model`, is shell code that an app calls through `host.request`.

### Get the tools

| Tool | Source | Use |
| --- | --- | --- |
| `hub` | App Hub `main` | Stamp editable source, check and scan it, then prepare, verify and pack GitHub-attested releases. Reviewers run the same code. |
| `card-host` | App Hub `main` | Run the unsigned bundle, drive it and capture screenshots. |
| `tools/octo` | [OctoSense App Flow](https://github.com/OctoSense-org/OctoSense-App-Flow) (formerly Design Flow) | Create, run and capture an app. It wraps `card-host` and `hub`. |
| OctoSense desktop | [RC4 prerelease, source `9266b008`](../README.md#download-a-compatible-host); platform downloads and prerequisites in that guide | Install GitHub-attested releases and run compatible host services, including components, declaration-only policy and document, device-calendar, Mail-draft and audio APIs on their supported platforms. Use macOS for the connected samples; OAuth registrations are not included. |

Set up the workspace as described in
[QUICKSTART §1](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/QUICKSTART.md#1-prerequisites),
then build `hub` and `card-host` from current `main`:

```sh
cd ~/octosense-ws/OctoSense-App-Hub
git pull
cargo build --release -p octosense-app-hub --bin hub
cargo build --release -p octosense-card-host --bin card-host
export PATH="$PWD/target/release:$PATH"
hub
```

`hub` with no arguments prints its usage, starting with
`hub — the OctoSense app hub command (ADR 0003)`. So does any command with
`--help` or `-h`, such as `hub check --help`. If `hub check --help` prints
`hub: No such file or directory (os error 2)` instead, your `hub` is older
than `main`: pull and build it again.

If the build fails with `no variant … TextInputStateQuery`, see
[`card-host` fails to build](DEVELOPMENT.md#card-host-fails-to-build).

Then run `tools/octo doctor` from your App Flow checkout, before you build
or check an app:

```sh
cd ~/octosense-ws/OctoSense-App-Flow
tools/octo doctor
```

`tools/octo doctor` finds `hub` and `card-host`, rejects GitHub's unrelated
`hub` CLI, and checks Python and the app template. Success ends with
`ready: tools/octo new <dir> --platform <target> && tools/octo run <dir>/bundle`.
Otherwise it prints a `[fail]` line, every place it looked and the commands
that fix it.

An older or patched `hub` can pass a bundle that the reviewers' build refuses.
Use an unpatched build of `main`, and leave that checkout as it is until you
submit. Record the exact tool revision and use its authenticated
`catalog-v2.json` for publisher continuity checks; `catalog.json` is the
explicit legacy channel ([step 6](#6-freeze-and-verify-the-release)).

### The `hub` command

`hub help` prints the exact usage of every command; use only the commands it
lists. The table shows who runs each command and in which step.
[Commands](PUBLISHING.md#commands) explains the flags and the gate's report.

| Command | Run by | Step | Purpose |
| --- | --- | --- | --- |
| `hub stamp` | You | 2, 5 | Write the editable bundle's digest into its manifest. |
| `hub check` | You, then a reviewer | 5, 8 | Run the gate. |
| `hub scan` | You, then a reviewer | 5, 8 | Run the gate, then write the review packet. |
| `hub publisher-prepare`, `hub publisher-attach`, `hub publisher-pack` | Your app's GitHub workflow | 6, on the tag push | Seal the release, attach GitHub's proof and pack the bundle. |
| `hub publisher-unpack` | You, then a reviewer | 6, 8 | Unpack a downloaded release pack into a new directory. |
| `hub publisher-verify` | The workflow, you and a reviewer | 6, 8 | Verify the release proof and run the gate. |
| `hub publisher-entry` | A reviewer | 8 | Build a candidate catalog entry; it publishes nothing. |
| `hub catalog-prepare`, `hub catalog-envelope`, `hub catalog-verify` | An admin, through the protected catalog workflow | 9 | Prepare, wrap and verify the signed `catalog-v2.json` ([GITHUB-PUBLISHING.md](GITHUB-PUBLISHING.md)). |
| `hub publish`, `hub withdraw`, `hub remove`, `hub certify`, `hub verify` | A maintainer, for the legacy catalog; you run `hub certify`, `hub publish` and `hub verify` only on a rehearsal catalog | 9 | Publish, withdraw or remove versions in `catalog.json`, certify its working key, or verify it. |
| `hub keygen`, `hub pubkey`, `hub sign-manifest` | A maintainer, for the legacy catalog; you run only `hub keygen`, for a rehearsal catalog's throwaway keys | — | Create an Ed25519 key, print its public half, or sign a legacy manifest. Never use them to sign or publish an app: App Hub accepts only GitHub-attested releases. [Issue #168](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/168) decides whether they stay. |

App Flow's `tools/octo` runs some of these for you; its
[`tools/octo` table](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/README.md#toolsocto)
lists the flags.

| `tools/octo` command | Runs |
| --- | --- |
| `new` | `hub stamp` |
| `check` | `hub stamp`, then `hub check --allow-unsigned`, passing `--catalog` and any other `hub check` flags on |
| `doctor` | `hub help`, to confirm that it found this `hub` |
| `publish-github` | Nothing itself: it installs the release workflow, which runs `hub publisher-prepare`, `hub publisher-attach`, `hub publisher-verify` and `hub publisher-pack` on the tag push |

In a local
[store rehearsal](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/PUBLISHING.md#4-rehearse-the-store-path-locally),
you also run `hub keygen`, `hub certify`, `hub publish` and `hub verify` on a
throwaway test catalog. The keys you create there sign only that catalog,
never your app, and the rehearsal publishes nothing to App Hub.

### Know where your app runs

| Host | Runs | Does not |
| --- | --- | --- |
| `card-host` | One unsigned bundle, with a remote bridge to drive and capture it | Serve any host service except `runtime` discovery: every other `host.request` fails with `no service answers "<family>" on this device`. It also refuses sealed releases and apps that require `host-api-v1`, `backend-api-v1` or `script-tools-v1`. |
| [RC4 prerelease](../README.md#download-a-compatible-host), source `9266b008` | Contract 1.11, declaration-only capability/network policy, Wasm core modules and components, and shared-component installation from verified catalogs; existing account, device-consent and trusted-review flows remain | Supply distributor OAuth registrations, automatically publish components to the public catalog, upgrade a phone, or bypass platform and authorization limits. See release provenance for build, archive and device acceptance boundaries. |
| Historical RC2 release `4ccf8e06` (RC1 `933abbcf` had the same catalog and discovery, without the RC2 APIs) | Public v2 catalog, `publisher-github-v1`, discovery and open-app Splash tools; the app's own Wasm functions on macOS and Linux; files, device-calendar, Mail-draft and audio services within their platform limits ([what hosts serve today](../README.md#what-hosts-serve-today)) | Ship OAuth registrations, run Wasm on Windows, approve a Mail send on Windows or Linux, serve device calendars, audio sessions or fresh location samples there, or run macOS-only listings on other OSes. Linux/Windows embedded backend login and protected writes remain unsupported; external-browser backend login and reads are implemented, and a Windows fixture built from RC2's source completed a sign-in against a synthetic backend. |
| Historical desktop-v0.1.0-beta.2 (macOS, Apple silicon) | Installed apps, including apps that use `auth`, `github`, `gmail` and `gcalendar` | Sign in to GitHub or Google until the host has an OAuth registration in `oauth/clients.json` ([setup](https://github.com/OctoSense-org/OctoSense/blob/desktop-v0.1.0-beta.2/crates/oauth-service/README.md)); the release ships none. Sign an app in to its own backend. Install an app that requests `wasm`: its store refuses it with `unknown capability "wasm"`. Unverified on this release: live provider sign-in. |
| desktop-v0.1.0-beta.1 and home-v0.1.0-beta.1 (the only released phone build) | Store apps that use only capabilities their older app contract knows | Install an app that requests `auth`, `github`, `gmail`, `gcalendar`, `calendar`, `photos`, `youtube`, `wasm` or `palpo.*`. Their stores refuse it, for example with `unknown capability "auth"`. |

The RC1 and RC2 releases differ from historical desktop beta.2 in these ways;
RC2 also adds the APIs listed in [what hosts serve today](../README.md#what-hosts-serve-today).
[Download packages and check prerequisites](../README.md#download-a-compatible-host).

- A distributor can compile GitHub and Google registrations into its build. A
  build you make from source has no registrations until you add them, for
  example in `oauth/clients.json`
  ([Connected accounts](PUBLISHING.md#connected-accounts)).
- An app can sign in to the backend its manifest declares, then call the
  operations it lists, within the [platform limits below](#check-your-platform)
  ([Sign in to your own backend](PUBLISHING.md#sign-in-to-your-own-backend)).
- An app that declares `host-api-v1` gets the device-permission methods on
  macOS and Android, and on RC2 fresh location samples, device calendars,
  Mail drafts and audio sessions on the platforms that have them
  ([Host API compatibility](HOST-API.md)).
- Approving a GitHub or Google Calendar save takes a physical press on the
  host's confirmation sheet, as approving a Gmail send does on both builds.
- An app agent's `glance.publish` refuses executable Splash (`script`) and L1
  card source.
- The host keeps Google Calendar events from 30 days back to 366 days ahead,
  not the calendar's whole history.

To try a release of your app before App Hub publishes it, install it from a
local test catalog in a shell built from source
([rehearsal](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/PUBLISHING.md#4-rehearse-the-store-path-locally)).
Unverified: the rehearsal with a GitHub-attested release; the recorded run
used a key-signed test app.

### Check your platform

- **macOS on Apple silicon:** use the compatible RC4 release for the current
  public samples. Installation, local drafts and updates are separate from
  authenticated provider effects; see [download and account limits](../README.md#download-a-compatible-host).
- **Windows x64 / Linux x86_64:** RC4 packages do not change an app's
  `listing.platforms`. The four current samples declare macOS only. Native
  browser and host tests are not acceptance of your app; validate every claimed
  platform, including its dependencies and missing-service states.
- RC2's Mail send review, device calendars, audio sessions and fresh location
  samples exist on macOS (and Android), not on Windows or Linux. Declare those
  methods under `host_api.optional` when your listing names Windows or Linux,
  or the app will not install there ([Host API compatibility](HOST-API.md)).
- On Windows, use `python tools/octo`; it finds `.exe` binaries. Preserve
  bundle bytes using [step 1's `.gitattributes`](#1-lay-out-the-repository),
  then verify a fresh checkout ([step 6](#6-freeze-and-verify-the-release)).
- Linux WebReader requires GTK 3/WebKitGTK and X11/XWayland; native Wayland
  is unsupported. Windows uses WebView2; these engines are not bundled.
  Linux/Windows implement external-browser backend login and declared reads;
  RC2 adds the native link openers that login needs, and a Windows fixture
  built from RC2's source completed a browser sign-in against a synthetic
  backend, while Linux sign-in is not validated. Embedded backend login and
  protected writes remain unsupported and fail closed. See the [browser requirements](https://github.com/OctoSense-org/OctoSense/blob/4ccf8e068399b1da139771a9ed94cef05fa6ae60/docs/desktop-embedded-browser.md).

## The three reference apps

For current downloads, use the GitHub-attested releases (GitHub Notes 0.2.2, the
others 0.2.1) and new ids in [the public catalog table](../README.md#apps). Their 0.2.0 → 0.2.1 update
retains the same GitHub publisher identity. None needs a developer signing key.

### Historical 0.1.x examples

Catalog sequence 10 holds three macOS developer previews from the publisher
`ymote`, each in two versions: 0.1.0, the first, and 0.1.1, which addresses the
three problems in [Lessons from 0.1.0](#lessons-from-010). These six entries
are signed with a publisher key; their GitHub-attested successors are the
0.2.x releases above. Each app lives in its own public repository, tagged
`v0.1.0` and `v0.1.1`. The table describes 0.1.1:

| | [GitHub Notes](https://github.com/ymote/octosense-github-notes) | [Inbox Assistant](https://github.com/ymote/octosense-inbox-assistant) | [Google Calendar](https://github.com/ymote/octosense-google-calendar) |
| --- | --- | --- | --- |
| App id | `org.octosense.samples.githubnotes` | `org.octosense.samples.inbox` | `org.octosense.samples.googlecalendar` |
| Demonstrates | Markdown drafts saved as GitHub commits the person approves | Gmail reading, one shared reply draft, background triage of new mail | An agenda with event drafts, saves the person approves and an advisory app agent |
| Capabilities | `storage`, `auth`, `github` | `storage`, `auth`, `gmail`, `model`, `glance`, `octos.session.open`, `octos.turn.start` | `storage`, `auth`, `gcalendar`, `glance`, `octos.session.open`, `octos.turn.start` |
| App agent | `read-only` profile, foreground only; 3 `read` tools | `read-only` profile, `background: true`, triggered by `inbox.new_message`; 1 skill; 9 tools (4 `read`, 5 `act`) | `read-only` profile, advisory; 4 `read` tools |
| Glance card | None | Template file `glance-workspace.splash`, published with `template` and `initial` | L0 card in `main.splash`, published with `source` and `data` |
| Protected write | Commit through `github.review_save`, approved on the host's confirmation sheet | Send through `gmail.draft.review`, approved by a physical press on the host's send confirmation | Save through `gcalendar.review_save`, approved on the host's confirmation sheet |

An app agent is the AI agent that OctoSense runs for one app, with that app's
tools. `read-only` is the permission profile of the agent's own session: the
agent asks before every write to its workspace (the app's storage). It does
not cover the tools in `tools.json`, whose `risk` levels decide when a call
waits for the person using the app. Inbox's 5 `act` tools edit reply drafts,
record triage decisions and publish Glance cards without asking.

App Flow's
[connected-apps README](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/examples/connected-apps/README.md)
explains how each app works. Its copies of GitHub Notes and Inbox have the
same fixes as 0.1.1; its copy of Google Calendar does not show the date range.

### Lessons from 0.1.0

Version 0.1.0 of each app passed review with a problem that version 0.1.1
addresses. Avoid each one from the start:

- **Never accept `script` in a card tool.** Inbox 0.1.0's background tool
  `inbox.notify`, mapped to `glance.publish`, accepted a `script` card of up
  to 16 KiB as well as a `template`. OctoSense desktop 0.1.0-beta.2 runs an
  agent's `script` card under the app's policy, so a prompt-injected
  background turn could publish arbitrary Splash code. Inbox 0.1.1
  accepts only its admitted template with `initial` data, and the RC1 and RC2
  releases refuse an agent's `script` card. Accept only
  `template` with `initial`, or an L0 `source` with `data`.
- **Declare `agent` if you ship tools.** GitHub Notes 0.1.0 shipped three tools
  in `tools.json` with `"agent": null`. The Hub admitted them as an app agent,
  and OctoSense offered that app agent, yet the gate's `grants:` line printed
  `agent none` and the store in OctoSense desktop 0.1.0-beta.2 said "Runs no
  assistant." GitHub Notes 0.1.1
  declares a `read-only` `agent` block with `AGENT.md`, so the gate prints
  `agent read-only` and every store shows the app agent
  ([An app with `tools.json` has an agent](PUBLISHING.md#an-app-with-toolsjson-has-an-agent)).
  Disclose the app agent in your privacy policy. If the app should have no
  agent, ship no `tools.json`.
- **Show a date range.** Google Calendar 0.1.0 listed the calendar's entire
  history, oldest events first: OctoSense desktop 0.1.0-beta.2 syncs every
  event, and the agent's `cached` tool returns them all. Google Calendar 0.1.1
  shows "past 30 days / next 366 days" when the host reports that range and
  "date range unavailable" when it does not, and no longer reports an event
  outside the range as deleted. The RC1 and RC2 releases sync only that range. Show a date range, and give the agent the selected
  event, not the whole cache.

One pattern remains in 0.1.1. Inbox's background tools can rewrite a reply
draft, including its `to` address, and only the host's send confirmation,
which takes a physical press, stops a redirected reply. Limit background tools
to reads and to writes the person approves.

## 1. Lay out the repository

The Hub admits only `bundle/`. The gate reads nothing else in the repository,
but reviewers load your privacy policy and support pages. Commit the release
workflow, `.github/workflows/publish-app.yml`, beside the editable `bundle/`.
An app repository holds these files:

```text
my-app/
  bundle/                             the submission: manifest, listing, code, tools, assets, screenshots
  .github/workflows/publish-app.yml   the release workflow
  .gitattributes                      keeps Git from converting the bundle's bytes
  PRIVACY.md                          the page privacy_policy_url points to
  SUPPORT.md                          how to report problems
  README.md                           what the app does and how to verify a release
  review/                             gate output (GATE.txt) and scan answers (ANSWERS.md)
  LICENSE, NOTICE
  .gitignore                          keeps build/ and .local-state/ out of Git
```

[GitHub Notes 0.2.1](https://github.com/ymote/octosense-github-notes/tree/v0.2.1)
follows this layout. Its `publisher.json` is left over from 0.1.x; you need
none.

Keep development READMEs, keys, review packets and `.local-state/` outside
`bundle/`. Keep required asset licenses and attribution inside as `.txt` or
`.md`; links in that documentation do not grant network access. The gate checks every file
in the bundle against its rules and the 8 MiB size limit. `tools/octo new`
creates a `.gitignore` that already excludes `build/`, `.local-state/` and
`*.key`.

Add a `.gitattributes` file so that Git stores and checks out the bundle's
bytes unchanged. The reference apps' 0.1.x tags lack one, so a checkout with
`core.autocrlf=true`, common on Windows, converts their text files to CRLF,
and their digests no longer match.

```sh
cd ~/apps/my-app
printf 'bundle/** -text\n' >> .gitattributes
git check-attr text -- bundle/manifest.json
```

Success prints `bundle/manifest.json: text: unset`. If the bundle is already
committed, run `git add --renormalize bundle` and repeat
[step 5](#5-produce-the-final-bytes).

Git matches the pattern from the repository root, so `bundle/**` covers only a
bundle there. If your bundle is deeper, name its real path, such as
`apps/my-app/bundle/** -text`, or write `**/bundle/** -text` to cover a
`bundle/` at any depth. Then run `git check-attr` on that bundle's
`manifest.json`, and `git add --renormalize` on its directory.
`text: unspecified` means that the pattern misses the bundle, and Git can
still convert its files.

## 2. Get the manifest right

Edit `bundle/manifest.json`. GitHub Notes' 0.1.0 manifest before stamping:

```json
{
  "schema": 1,
  "id": "org.octosense.samples.githubnotes",
  "version": "0.1.0",
  "name": "GitHub Notes",
  "integrity": { "bundle_blake3": "" },
  "capabilities": ["storage", "auth", "github"],
  "storage": { "accounts": true, "max_bytes": 4194304 }
}
```

- **`id`** is permanent. Use `[a-z0-9.-]`, at most 64 characters. The gate
  refuses ids under `os.`. The last segment becomes your tools' namespace
  (`githubnotes.*`) and must not be a reserved name; the
  [identity row](#common-refusals-and-how-to-fix-them) lists all 23.
  `tools/octo new` refuses them too.
- **`version`** is new for every submission. Your tag is `v` followed by it.
- **`capabilities`** lists only capabilities that your screens use and that a
  host serves
  ([What the Hub cannot do yet](#what-the-hub-cannot-do-yet)). The store shows
  each one to the person before install.
- **`integrity`** holds the bundle digest, which `hub stamp` writes. Never
  edit it by hand.

Then stamp the bundle:

```sh
hub stamp bundle
```

Success prints the bundle digest: 64 hex characters of BLAKE3 over every file
in `bundle/` except `manifest.json`. If `hub stamp` prints
`hub: manifest is not valid: unknown field …`, the manifest holds a field the
contract does not define; the message lists the valid ones. `hub stamp` does
not judge capabilities or the listing. `hub check` does, in step 5.

## 3. Write the listing

`bundle/listing.json` is what the store shows before install. The release's
GitHub attestation covers it through the bundle digest, so changing one word
later takes a new version. Write nothing that will go stale.

| Field | Rule | Reference apps |
| --- | --- | --- |
| `subtitle` | At most 80 characters | `"Markdown drafts with reviewed GitHub saves"` |
| `description` | At most 4000 characters | States what is fictional and what is unverified |
| `category` | One of the 15 in [PUBLISHING.md](PUBLISHING.md#the-listing) | `productivity` |
| `keywords` | At most 10 | 4 each |
| `screenshots` | 1 to 8 PNG or SVG files inside the bundle | 2 to 4 real PNG captures |
| `icon` | A square SVG or PNG inside the bundle ([ICONS.md](ICONS.md)) | `assets/icon.svg` |
| `platforms` | Only platforms you ran the app on | `["macos"]` |
| `publisher` | `name`, `support` (a URL or an email address) and an `https://` `privacy_policy_url` | The repository's Issues page and `PRIVACY.md` |
| `age_rating` | `all`, `12+`, `16+` or `18+` | `all` |

Replace every placeholder in the template's listing (the `example.com` URLs
and the "Replace with …" text), and set `platforms` to the platforms you ran
the app on. The gate accepts placeholders; `tools/octo check` flags them.

Write the privacy policy before you submit. Say what leaves the device and
where it goes: provider APIs reached through host services, text sent to the
person's model provider, Glance cards and whether the app has an app agent.
GitHub Notes corrected its policy after publishing 0.1.0, because its
`tools.json` gave it an app agent the policy did not mention.

Check the listing's links before you push the tag: after that, a wrong URL
takes a new version to fix. Publish the privacy page first, then request every
URL in the listing's `publisher` block:

```sh
python3 -c 'import json; p = json.load(open("bundle/listing.json"))["publisher"]; print(p["privacy_policy_url"]); print(p["support"])' \
  | grep -E '^https?://' \
  | while read -r url; do echo "$(curl -sL -o /dev/null -w '%{http_code}' "$url") $url"; done
```

Success prints `200` for each URL. GitHub Notes:

```text
200 https://github.com/ymote/octosense-github-notes/blob/main/PRIVACY.md
200 https://github.com/ymote/octosense-github-notes/issues
```

The command skips a `support` email address. `-L` follows redirects, so a
privacy page on your own domain reports its final status. `404` means a page
that is not public yet, a private repository or a typo.

## 4. Capture the screenshots

A screenshot is the one listing claim that a reviewer can check against the
running app. Run the unsigned bundle in `card-host`, drive the app to each
state over the remote bridge
([QUICKSTART](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/QUICKSTART.md)
lists the routes), and capture each state. From your App Flow checkout:

```sh
tools/octo run ~/apps/my-app/bundle --port 8141 --detach
tools/octo shot 8141 ~/apps/my-app/bundle/screenshots/01-main.png
curl -s 127.0.0.1:8141/quit
```

Success:

```text
wrote …/bundle/screenshots/01-main.png (824x1784, 32539 bytes). Look at it before you ship it.
{"ok":1}
```

Open every PNG before you use it. Never ship an error frame, an empty first
frame or a mock-up. If the app keeps animating, `shot` saves the last frame
anyway and adds
`(still changing after 2s, e.g. an animation; this is the last frame)` to its
output; look at that frame. For other capture problems, see
[QUICKSTART's troubleshooting](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/QUICKSTART.md#troubleshooting).

Unverified, Linux only: if `shot` times out under software rendering
(llvmpipe, WSL), set `MAKEPAD_WRITE_FRAMEBUFFER_PNG=<file>` before you start
`tools/octo run` or `card-host`. Makepad's OpenGL backend then saves the window
to that PNG each time it draws. On macOS, `shot` stays the verified path.

A screen that needs a host service, or a native widget such as GitHub Notes'
Markdown editor, does not render in `card-host`. Capture such a screen with
fictional data in a shell that provides the service or widget. Otherwise,
capture its unavailable state, as Google Calendar's `03-host-required.png`
does. Say in the description which screenshots use fictional data.

The `hub scan` packet carries no screenshots. Reviewers see them in the bundle
and in your issue.

## 5. Produce the final bytes

For an app's first release and every update, use GitHub publisher provenance
from a public repository; it is the only route App Hub accepts. OctoScript
apps are open by default:
each bundle already ships the app's source as readable text, so a public
repository reveals little more
([ADR 0002](adr/0002-github-attested-publisher-identity.md)). This route
needs app contract 1.8.0 and a host that supports `publisher-github-v1`. Two
tag-push releases of a test app passed native Store install, update and
launch checks ([evidence and
limits](PUBLISHING.md#github-publisher-provenance)). For the current public
catalog, use the [RC4 prerelease](../README.md#download-a-compatible-host).

1. Open the submission issue if you have not already done so. Missing release
   evidence can be added later; mark it pending rather than inventing a pass.
2. Test the editable source and capture its real UI. Run the gate
   and `hub scan bundle --packet build/review.json`, with
   `build/` outside the bundle. Answer every question in that packet: seven,
   or eight when the bundle ships `tools.json`, `AGENT.md` or skills. Include
   the file supporting each answer and name untested behavior.
3. Install and review `.github/workflows/publish-app.yml` using App Flow's
   `tools/octo publish-github <app-directory>` (also included by `tools/octo new`).
   The workflow needs no publisher key and no repository signing secret. The
   native steps are documented in
   [GitHub publisher provenance](PUBLISHING.md#github-publisher-provenance).
4. Commit the tested editable source, screenshots, listing, privacy/support
   pages, `.gitattributes` and workflow. Use a new semantic version for each
   release and an exact `v<manifest.version>` tag. The workflow generates the
   attested manifest and final release pack; do not commit those generated
   bytes over your editable source.

Routine updates keep the same repository name, repository ID, owner ID and
workflow, with a higher semantic version. Never restamp a sealed
release: change the editable source and release a new version.

## 6. Freeze and verify the release

The tag identifies the tested editable source and workflow. The Release
pack contains the final attested manifest; checking a source clone alone
does not verify that pack.

1. After reviewing the tested commit, push its new `v<version>` tag. Record
   `git rev-parse "v0.1.0^{commit}"` (substitute your version) and confirm it
   with `git ls-remote origin 'refs/tags/v0.1.0*'`. For annotated tags the
   `^{}` line identifies the commit. Never move, delete or recreate a pushed
   tag; a correction needs a higher version and a new tag.
2. Wait for the GitHub workflow to succeed. Keep the workflow run URL, exact
   commit, release URL, `app.bundle.pack.json` and `release-receipt.json`.
   The receipt's `pack_sha256` must match the downloaded pack, for example
   with `shasum -a 256 app.bundle.pack.json` on macOS/Linux.
3. Verify the downloaded final bytes without modifying them:

   ```sh
   hub publisher-unpack app.bundle.pack.json --out review-bundle
   hub publisher-verify review-bundle --catalog /path/to/catalog-v2.json
   git -C ~/octosense-ws/OctoSense-App-Hub rev-parse HEAD
   ```

   `review-bundle` must not exist beforehand. Use the current authenticated
   catalog, before this proposed version has been admitted; that additionally
   checks publisher continuity and version advancement. Record the complete
   verification output and exact tool revision. A rejection is a finding to
   fix, not a reason to strip the proof or use `--allow-unsigned`.
4. Add these artifacts and the test evidence to your existing submission
   issue. Reviewers verify both the source commit and the downloaded pack.
   A successful release workflow does not install, submit or approve the app.

## 7. Open the submission issue

Open an issue with the [Submit an app form](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/new?template=submit-app.yml)
and title it `Submit <app id> <version>`. Opening the issue is your request to
publish the app. **You can open it before steps 1–6 are complete.** Add the
release evidence to this issue once the release is ready; pending fields do
not mean approval.

Until App Hub first publishes your app, keep this one issue: post each new
release as a comment with its tag, full commit SHA and workflow-run link, and
update the issue title and the Version field in the issue body to the new
version. After the first publication, each new version needs a new issue
([step 9](#9-after-you-submit)).

| Form field | When and what to provide |
| --- | --- |
| Release status | Now: request with evidence pending, or GitHub release ready |
| App id / Version / Repository URL | Now: manifest id, planned version and public GitHub repository |
| Requested capabilities and app behavior | Now: purpose and reason for each capability |
| What is not verified | Now and after every update: incomplete checks, host/platform/provider limits |
| Tag / Full commit SHA / Bundle path | Before approval: immutable source tag and commit, editable bundle path |
| GitHub workflow run URL | Before approval: successful tag-push run for the exact release |
| Release and pack URL / Release pack SHA256 | Before approval: final pack, receipt and matching downloaded hash |
| Bundle BLAKE3 digest | Before approval: digest from the attested release manifest |
| Privacy policy URL / Support contact | Before approval: working listing links or support email |
| Platforms tested / App Hub revision the gate ran on | Before approval: exact host/tool revisions and exercised behavior |
| Gate and publisher verification output | Before approval: complete development gate and downloaded-pack verification output |
| Scan answers / Screenshots | Before approval: all packet answers with source evidence and real native captures |
| Confirmations | No secrets, immutable pushed tags, and honest pending evidence |

Do not open a PR editing `catalog.json`, `catalog-v2.json`, `index/` or
`artifacts/`. Catalog publication is an App Hub admin's separate
[protected GitHub workflow](GITHUB-PUBLISHING.md), after review.

## 8. What reviewers check

A reviewer, not a bot, checks your submission: no workflow runs when you open
the issue. The reviewer runs the gate on the exact bytes at your tag and in
your release pack, and verifies the GitHub identity, source commit, release
proof, pack digest, privacy and UI evidence. The reviewer posts any problems
as comments in your issue. The protected catalog workflow checks the same bytes again; it
never runs contributor code. The historical admission record for the
reference apps' 0.1.0 submissions,
[`reviews/connected-apps-0.1.0/admission.json`](../reviews/connected-apps-0.1.0/admission.json),
lists what was confirmed for each one:

| Check | Field in the 0.1.0 record | Check it yourself |
| --- | --- | --- |
| The tag resolves to the stated commit | `tag_verified` | `git ls-remote` (step 6) |
| The privacy and support URLs return HTTP 200 | `public_privacy_and_support_http` | The link check in step 3 |
| Release downloads match their hashes | `release_download_hashes_match` | `shasum -a 256 -c SHA256SUMS`, if you create a release |
| The publisher's proof verifies on the downloaded release | `downloaded_publisher_signature_verified` | `hub publisher-unpack`, then `hub publisher-verify`, on the downloaded pack (step 6) |
| The gate passes on the downloaded bundle | `downloaded_gate_output` | The same check |
| The bundle digest matches your issue | `bundle_digest` | `integrity.bundle_blake3` |
| Each requested capability's family serves store apps on the platforms your listing names | Not in the 0.1.0 record; checked since catalog 14 | App Flow's [HOST-API-FAMILIES](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-API-FAMILIES.md) |

The historical record for 0.1.1 is
[`docs/admissions/connected-apps-0.1.1`](../docs/admissions/connected-apps-0.1.1/README.md).
It also keeps each app's signed gate output and source review, and records a
test that installs each app with the store's code and upgrades it from 0.1.0.

Reviewers compare each scan answer with the bundle: the listing's claims,
platforms and category, accurate usage disclosures, deceptive UI, text written as
instructions to an AI agent, abusive wording and each tool's scope and risk.
Reviewers promise no review time.

Admission does not prove that an app works with live providers. The 0.1.0
record states `"live_provider_login_and_remote_effects_verified": false`, and
the 0.1.1 record proves no native UI, provider traffic or physical approval.
Say in your listing what you have not verified.

## 9. After you submit

When the checks pass, an App Hub admin reviews the submission and approves
it; the Hub publishes nothing without that approval. The admin then runs the
protected [catalog workflow](GITHUB-PUBLISHING.md). The workflow admits the
exact reviewed bytes, has GitHub Actions sign the new `catalog-v2.json` with
Sigstore, and commits the catalog to `main`. People can then search for,
install and run the app in an OctoSense build that reads `catalog-v2.json`
and supports `publisher-github-v1`, such as the
[RC4 prerelease](../README.md#download-a-compatible-host). Historical
desktop-v0.1.0-beta.2 reads only the legacy `catalog.json` that
`hub publish` produces.

- A reviewer posts any findings in the issue and, once your app is published,
  closes the issue with the catalog sequence it appears in. Answer questions
  in the issue, and change nothing at your tag.
- **Before the first publication, post each new release on the same issue.**
  To fix a finding, raise `version` and repeat steps 4 to 6 (step 4 only if
  the UI changed). Then comment on your submission issue with the new tag,
  full commit SHA and workflow-run link, and update the issue title and
  Version field ([step 7](#7-open-the-submission-issue)).
- **After publication, each new version needs a new issue.** Raise
  `version`, open a new issue that links the previous one (it can precede the
  release), repeat steps 4 to 6 and add that release's evidence to the new
  issue. The Hub never replaces a published version.
- An App Hub admin can publish a catalog withdrawal with a reason, through
  the same protected workflow, after a maintainer commits the reviewed
  withdrawal candidate. Stores stop running installed copies of that version
  at their next catalog fetch; other versions keep working. To ask for a
  withdrawal, open an issue with the app id, the version and the reason that
  people should see. A withdrawn version number stays taken, so publish the
  fix as a new version.

## Common refusals and how to fix them

`hub check` prints one line per finding:
`[refused|warning] <check> (<file or property>): <detail>`. A bundle the gate
cannot read gets no report, only one `hub: …` line. The full rules are in
[PUBLISHING.md](PUBLISHING.md#rules-the-gate-enforces).

| Check or message | Cause | Fix |
| --- | --- | --- |
| `digest: the bundle hashes to …, the manifest claims …` | The bytes changed after `hub stamp`: an edit, a stamp after the commit (`tools/octo check` restamps), CRLF line endings from a Git checkout or a Windows `hub` older than `main`, which hashed paths with `\`. | Build `hub` from `main`, add the `.gitattributes` from [step 1](#1-lay-out-the-repository), then restamp editable source and create a new GitHub-attested version; verify the downloaded pack. Never restamp sealed releases. |
| `continuity: existing legacy app ownership cannot be adopted by GitHub provenance` | The id is on record for an unsigned or key-signed app, such as a copied `org.octosense.samples.*` id. | Choose a new id. |
| `continuity: GitHub publisher repository, owner or workflow changed` | An update came from another repository, owner or workflow file, or the repository was renamed or transferred. | Release from the recorded repository and workflow. A renamed or transferred repository cannot update the app; publish it under a new id. |
| `version: version … is already published; publish a new version` | That version is in the catalog. | Raise `version`, use a new tag and verify the workflow's new release pack (steps 5 and 6). |
| `policy: app … requests unknown capability "<name>"` | The name is not in the contract (`contacts`, `model.image`, a prefix such as `octos.`), or `hub` is older than the capability. | Use exact names from [PUBLISHING.md](PUBLISHING.md#the-manifest), and rebuild `hub` from `main`. |
| `hub: manifest is not valid: unknown field …`, with no report | `hub stamp` and `hub check` cannot parse the manifest. | Remove or rename the field. The message lists the valid ones. |
| `contents: <file> has extension "…", which a bundle may not hold` | `.DS_Store`, `LICENSE` or another file without an allowed extension. | Delete it or move it out of `bundle/`. |
| `contents-invalid (<file>): cannot decode the image: …` | A corrupt image, another format renamed to `.png` or an image over 4096 pixels a side. | Capture or export it again. |
| `resource-invalid (…/font_src): not a portable bundle path: "makepad_widgets:resources/…"` | A built-in font outside the shipped allowlist. | Use the exact Inter, LXGW WenKai Regular or Bold resource name, or bundle a licensed font subset. See [Fonts](PUBLISHING.md#fonts) for names and older-host limits. |
| `assets: <file> contains https://…` | Card data names an external URL. Script/agent HTTPS links are not refused for an omitted destination. | Bundle the card asset. Disclose script destinations accurately; scripts and agent guidance still reject `http://`, `file://` and `../`. Plain `.txt`/`.md` documentation links do not trigger this check or grant network access. |
| `identity: … is under os.`, or `identity: app id "…" ends in "…", which is reserved` | The id is under `os.`, or the id or its last segment is one of `agents` `apphub` `appcard` `browser` `calculator` `card` `clock` `dev` `notes` `octos` `octoscode` `os` `reference` `reminders` `rinx` `sheets` `shell` `system` `task` `terminal` `toolbox` `weather` `workflow`. | Choose another id before your first release. |
| `listing: listing has more than 10 keywords`, `… more than 8 screenshots` or `listing platform "…" is not one of […]` | The listing breaks a limit or misspells a name. | Trim the list, or use a name from the message. |
| `listing: screenshots/01-main.png is named by the listing but is not in the bundle` | The file does not exist. | Capture it ([step 4](#4-capture-the-screenshots)) or fix the path. |
| `hub: the bundle exceeds the size limit`, with no report (`--json` shows `bundle-invalid`) | The files other than `manifest.json` exceed 8 MiB (8,388,608 bytes). | Shrink images and subset fonts. |
| `entry (main.splash): the bundle has no entry: …` | No `main.splash` or `page.card` at the bundle root. | Put the entry file at the root of `bundle/`. |
| `secrets: <file> declares is_password:true: …` | A password or one-time-code field. | Remove it. Sign-in belongs to a host service. |
| `agent: AGENT.md is in the bundle but agent.instructions does not name it` | Agent files without a matching `agent` block. | Declare `"instructions": "AGENT.md"` in `agent`, or remove the file. |
| `card-host: refused: this host has no GitHub publisher verifier` | `card-host` runs only unsigned bundles, never a sealed release. | Run and capture the editable source; your release workflow seals the release ([step 5](#5-produce-the-final-bytes)). |

## What the Hub cannot do yet

| You want | Status | Instead |
| --- | --- | --- |
| Sign in to your own backend | RC1 and RC2 implement host-run backend sign-in and declared reads, within the [platform limits](#check-your-platform); compatible Android source builds have a separate embedded route. Each write requires a supported native review path ([Sign in to your own backend](PUBLISHING.md#sign-in-to-your-own-backend)). | For provider identity only, identify the person with identity-only sign-in: `auth` with GitHub's `read:user`, or Google's `openid`, `email` and `profile`. For provider data, add `github`, `gmail` or `gcalendar`. |
| Keep an API key or token in the app | Not supported. The gate refuses only password and one-time-code fields, so it does not catch a key typed into a plain field or kept in storage. | Ship no keys. For text generation, use `model`, which calls the person's own AI provider. |
| Generate images, audio, video or embeddings | RC1 and RC2 have methods under the `model` capability; media method names are not separate capabilities. Provider configuration, entitlement and platform limits still apply. | Discover methods at runtime and handle unavailable providers; see the [media guide](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/AI-SERVICES.md#media-and-embeddings-model). |
| Use `llm`, `news`, `calendar`, `matrix.*`, `prompt`, `ledger.read`, `clipboard`, `palpo.*` or an engine service | The gate admits them, but no host serves them to a store app. `llm` and `news` answer only `os.*` apps, `calendar` only `os.calendar`, and `matrix.*` is served only by Rinx's own host inside Rinx; the engine services (`sheet`, `photo`, `word`, `deck`, `cad`, `light`, `sound`, `design`, `film`, `effect`, `vector`, `pdf`) are declarable on App Hub `main` but serve the system assistant only, and rc.2's admission does not know them. Nothing acts on the others. | Do not request them. For Google Calendar, use `gcalendar`. Which families serve store apps, where and since when: App Flow's [HOST-API-FAMILIES](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-API-FAMILIES.md). |
| Send mail from your app | On RC2, after the person signs in with `mail.add_account`: `mail.review_send` (or `mail.send`, which opens the same review) shows the host's native review, and the person approves with a physical press; `mail.compose` and `mail.compose_status` keep a draft and its status. The review exists on macOS and Android only; on Windows and Linux it fails with `Physical Mail send approval is unavailable on this platform`. RC1 had no send path, and SMTP acceptance on hardware is unverified ([OctoSense #409](https://github.com/OctoSense-org/OctoSense/issues/409) tracks the error text). | Declare `mail.review_send` under `host_api.optional` when your listing names Windows or Linux; there, read the granted accounts (`mail.accounts`, `mail.folders`, `mail.list`, `mail.message`) and notify the person, who sends from the Mail app. |
| Run your app's own logic in agent tools | The RC1 and RC2 releases run `implemented_by: "app"` tools while the full app is open; a closed app answers `app_not_running`. OctoSense desktop 0.1.0-beta.2 refuses these tools with `app_tool_unavailable`. A `host-service` tool without `host_method` targets the service named by your app's namespace. Naming it does not install a service: the call needs a real registered implementation. | To reach a shared service, map the tool with `host_method` to a method of `github`, `gcalendar`, `gmail` or `glance` ([Map a tool to a shared service](PUBLISHING.md#map-a-tool-to-a-shared-service-host_method)). For your own logic, declare `requires: ["script-tools-v1"]` and implement the `app_tool` hook ([Script tool execution](PUBLISHING.md#script-tool-execution-script-tools-v1)). Test it in the [compatible desktop release](../README.md#download-a-compatible-host). |
| Ship native Rust code | A store bundle cannot carry it. The gate refuses native libraries, and native code needs a shell release ([delivery paths](DEVELOPMENT.md#choose-a-delivery-path)). | For pure computation, compile your Rust code to a WebAssembly module in `fns/` and request `wasm` ([Capabilities](PUBLISHING.md#capabilities)). Desktop RC2 runs it in standard builds on macOS and Linux; Windows, iOS and OpenHarmony builds leave the `wasm` service out, and RC1 left it disabled everywhere. App Flow's [Run your own Rust code](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/RUST.md) shows how to build it, and which route to take for device APIs, the network and files. |
| Submit a system app (`os.*`) or a native app | No route here. System apps ship with the shells, and native code needs a shell release ([delivery paths](DEVELOPMENT.md#choose-a-delivery-path)). | Build a store app with an id of your own. |
| Install an `auth` app on a phone | No released phone build can. | Use RC2 on macOS for these macOS-only samples; Android Google authorization is unavailable. |
| Name Makepad's built-in CJK font in a card kit | Supported by the current Hub and pinned runtime for the exact Regular/Bold resources. | See [Fonts](PUBLISHING.md#fonts) for names, bundled fonts and older-host limits. Native `card-host` rendering is verified on Mac; every shell/platform is not yet verified. |

App Flow's
[HOST-API-FAMILIES.md](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-API-FAMILIES.md)
lists, for every host API family, who the shells serve, on which platforms and
since which release; [HOST-SERVICES.md](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-SERVICES.md)
explains each service.
