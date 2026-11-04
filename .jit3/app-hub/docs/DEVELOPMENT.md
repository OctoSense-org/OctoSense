# App developer guide map

English | [简体中文](DEVELOPMENT.zh-CN.md)

To build your first downloadable Hub app, follow
[Build your first Hub app](FIRST-APP.md). This repository owns what a bundle
must satisfy to be published: the bundle format, the gate, signing and
submission. Authoring tools and the shells that run apps live in other
repositories:

| Repository | What it owns |
| --- | --- |
| [OctoSense App Flow](https://github.com/OctoSense-org/OctoSense-App-Flow) (formerly Design Flow) | The app-development harness: quickstart, script API, the script-app template, the `tools/octo` command, the design flows (`flows/`) and the example apps (`examples/`). |
| [OctoSense `apps/appcard`](https://github.com/OctoSense-org/OctoSense/tree/main/apps/appcard) | AppCard, an opt-in "Ask anything" agent the shells can link, and the L0 card language (`a2app-l0/framework/l0.md`). |
| [OctoSense `apps/`](https://github.com/OctoSense-org/OctoSense/tree/main/apps) | The system apps (News, Photos, Maps, Camera, Mail, Calendar, AI providers, YouTube), each a script app bundle in `apps/<name>/bundle/`, and the host services of Mail, Calendar, News and AI providers in `apps/<name>/host-service/`. |
| [OctoSense `crates/oauth-service`](https://github.com/OctoSense-org/OctoSense/tree/main/crates/oauth-service) | The connected-account host services: `auth`, `github`, `gmail` and `gcalendar`. |

Find the guide for your task:

| Task | Guide |
| --- | --- |
| Set up the workspace and build `hub` and `card-host` | [Quickstart](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/QUICKSTART.md) |
| Prepare shared Makepad/Octoscript dependencies | [Native workspace](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/NATIVE-WORKSPACE.md) |
| Start a script app from a runnable template | [Quickstart](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/QUICKSTART.md) and [`templates/script-app/`](https://github.com/OctoSense-org/OctoSense-App-Flow/tree/main/templates/script-app) |
| Start a card app repository with metadata and agent instructions | [App starter](../templates/app/README.md) |
| Write a script app: state, handlers, storage, requests, host services | [Script-app flow](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/script-app/FLOW.md) and [Script API](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/SCRIPT-API.md) |
| Find which shell serves a host service | [Host services](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-SERVICES.md) |
| Turn UI designs into native cards | [Image-to-card flow](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/image-to-card/FLOW.md) |
| Understand card data, state, events, copy, themes and views | [L0 language](https://github.com/OctoSense-org/OctoSense/blob/main/apps/appcard/a2app-l0/framework/l0.md) and the [L0 notes](https://github.com/OctoSense-org/OctoSense-App-Flow/tree/main/docs/l0) |
| Run a bundle and drive it over HTTP | [`card-host`](#run-a-bundle-locally-card-host), below |
| Test real native input, capture frames and clean up test instances | [Native instrument](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/core/NATIVE-INSTRUMENT.md) |
| Set up an app-owned icon and bundled artwork | [Icons](ICONS.md) |
| Package, validate and sign a bundle | [Publishing](PUBLISHING.md) |
| Submit a bundle for review | [Submitting](SUBMITTING.md) |
| Read worked examples | [Examples](https://github.com/OctoSense-org/OctoSense-App-Flow/tree/main/examples) and the [system apps](https://github.com/OctoSense-org/OctoSense/tree/main/apps) |

<a id="choose-the-appropriate-delivery-path"></a>

## Choose a delivery path

| Path | What you build | How it ships |
| --- | --- | --- |
| Hub card app | `page.card` with its data and `kit/`, local artwork, `manifest.json` and `listing.json`. The host converts (lowers) the card to native widgets: the card has no logic of its own and runs within the host's existing capabilities. | A store bundle, submitted to the Hub. |
| Hub script app | `main.splash`, local artwork, `manifest.json` and `listing.json`. A Splash program with its own state, handlers, requests and storage, run in its own isolate (a separate script environment) under the policy its manifest resolves to. Capability and destination declarations disclose usage. The private jail and quotas, device consent, account scopes and service availability still govern execution; omitting a family or host does not deny a public API. | A store bundle, submitted to the Hub. |
| System app | A script app bundle under a reserved `os.` id, packed into a shell at build time. OctoSense's are in `apps/`. | With the shell release. A store bundle may not take an `os.` id. |
| Built-in native app | Source compiled into a shell release, built with the native workspace and the owning app's instructions. The shared icon conventions apply. | With the shell release. An icon declaration does not make it installable from the Hub. |
| Agent-generated app type | Specifications and lint rules in OctoSense `apps/appcard` that teach AppCard's agents to compose a new kind of app. | Not a bundle. |

Installed L0 cards keep their measured canvas inside the host app pane. The
renderer positions descendants relative to that pane; a native two-axis scroll
view keeps controls reachable when the canvas exceeds the available space.
Validate at a nonzero pane position and in a viewport shorter or narrower than
the authored canvas, including scrolling to and clicking its final control.
Role-based L0 without placements uses the same flow lowering as `card-host`.
Script apps continue to manage their own responsive layout and scrolling.

A store bundle carries no native code. New native Rust or JNI code, Python
services and browser controllers do not install as a card app or a script app.
Rust compiled to WebAssembly can ship in a store bundle, with `wasm`
disclosing that usage. Core modules reach no files, network or clock;
components use supported WASI and public host APIs within the app's actual
authorization and resource limits. Desktop RC2 includes core-module support
on macOS and Linux. Current OctoSense source also includes Windows and
Android support and OpenHarmony builds using Pulley; build inclusion is not
device or release acceptance (App Flow's
[Run your own Rust code](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/RUST.md)).

Some App Flow examples include a native service or a website integration.
Before you base a Hub app on one, check that every behavior runs inside the
app's isolate. Copying a service project's source directory does not make it
installable.

<a id="running-a-bundle-locally-card-host"></a>

## Run a bundle locally: `card-host`

`card-host` (`crates/card-host`) runs one bundle, a card app or a script app,
under exactly the policy its manifest resolves to. It follows the order a
device uses: admit, resolve, apply, evaluate.

Build it from this repository, in the prepared
[native workspace](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/NATIVE-WORKSPACE.md):

```sh
cargo build --release -p octosense-card-host --bin card-host
```

If the build fails with `no variant … TextInputStateQuery`, see [`card-host` fails to build](#card-host-fails-to-build).

The binary is `target/release/card-host`. Run it with:

```sh
card-host [--bundle <dir>] [--app-data <dir>] [--allow-unsigned] [--stamp] [--system] [--static <prefix>=<dir>]... [--size <w>x<h>] [--remote [<port>]]
card-host --help
```

`--help` prints the usage and exits 0. An unknown or malformed option prints
the problem and the usage, and exits 2. Neither opens a window.

| Flag | Effect |
| --- | --- |
| `--bundle <dir>` | The bundle to run. Default: the current directory. |
| `--app-data <dir>` | Where the app's storage jail is made, at `<dir>/<app id>/`. Host services keep their state in `<dir>/.host/`. Default: `$TMPDIR/octosense-card-apps`. |
| `--allow-unsigned` | Admit a manifest with no signature. `card-host` has no publisher verifier, so it refuses a **sealed** (GitHub-attested) release even with this flag: `this host has no GitHub publisher verifier`. Run and capture the unsigned development bundle. |
| `--stamp` | Rewrite the manifest's `integrity.bundle_blake3` to match the directory before admitting. Without it, `card-host` refuses a bundle whose bytes changed since the last `hub stamp`. |
| `--system` | Admit the bundle as a system app: by digest only, under the system ceilings. An empty digest is filled in memory. Use it to develop a system app. |
| `--static <prefix>=<dir>` | Serve `<dir>`'s files at `<prefix>/...` from memory, as a shell serves a system app's compiled-in artwork. Photos uses `--static photos=<dir>`. Repeatable. |
| `--size <w>x<h>` | The window's inner size in layout points, with no caption bar, so the card gets exactly that viewport. Default: the manifest's [`window`](PUBLISHING.md#the-window-size) size when it has one, sized the same way and not clamped to the screen; otherwise `412x892`. `card-studio` starts one `card-host` per size with it. |
| `--remote [<port>]` | Start the localhost HTTP control surface ([below](#drive-it-over-http-makepad_remote)) on `<port>`. Without a port, `card-host` picks a free one and logs it. `MAKEPAD_REMOTE=<port>` is equivalent to `--remote <port>`. |

The log says what happened:

| Log line | Meaning |
| --- | --- |
| `card-host: <id> <version> admitted — capabilities {…}, hosts {…}, storage <n> bytes, agent <profile>` | The bundle was admitted; the log shows usage declarations and resolved resource limits, not user consent. |
| `card-host: window sized <w>x<h>` | The window was sized by `--size`, or by the manifest's `window`, which the next line then says. |
| `card-host: refused: <reason>` | The bundle was refused. The window shows "card-host refused this bundle" and the reason. None of the bundle's code runs. |
| `card-host: realize {json}` | An L0 card's lint and realize report, logged before the card is drawn. A script app logs none. |
| `card-host: the card did not lower: <error>` | The card was admitted but could not be lowered to widgets. |

The `realize` report has four parts. Read it with the `/log` route
([below](#drive-it-over-http-makepad_remote)):

| Key | Content |
| --- | --- |
| `lint` | `check_ui_l0`: `valid`, `level` and `diagnostics`. |
| `realize` | `nodes`, `truncated` (a bound was hit and the tree is partial) and `diagnostics`. |
| `sources` | Each declared source's `$state`: the data's `$status` entry, else `ready` with a value and `pending` without one. |
| `lowering` or `lower_error` | `design` for a native kit pack; `l0-kit` for a card composed from the role kit, whose nodes get inspectable ids `beauty_0_1_…`. |

`card-host` registers no host services, including `model` and the `octos.*`
services that reach octos, the agent kernel OctoSense runs. Only `runtime`
discovery answers there. A script app that
calls `host.request("mail.list", …)` gets
`no service answers "mail" on this device`. Test a service-backed app in an
OctoSense shell that registers the service.
[Host services](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-SERVICES.md)
lists which shell serves which. The connected-account services need
desktop-v0.1.0-beta.2 or later.

`card-host` also refuses an app whose manifest requires `host-api-v1`,
`backend-api-v1` or `script-tools-v1`, because it implements none of the APIs
they need. The window shows "card-host refused this bundle" and
`app <id> needs a host implementing <method>@1`. Test such an app in
[OctoSense desktop 0.1.0-rc.1](../README.md#download-a-compatible-host), which
implements those APIs within its [platform limits](HOST-API.md#limits).

### Drive it over HTTP: `MAKEPAD_REMOTE`

Launch with `MAKEPAD_REMOTE=<port>` or `--remote [<port>]` to get a localhost
HTTP control surface. Every route is a GET and, except `/g?raw=1`, answers one
line of JSON. Coordinates are window-local layout points.

| Route | What it does |
| --- | --- |
| `/snap[?q=text]` | Lists the visible widgets with their rects and text, ready to click. `q` filters by id, type or text. |
| `/click?x=&y=` | Clicks at a point. |
| `/t?t=TEXT` | Types text into the focused widget. |
| `/k?k=down\|up&c=KeyA` | Presses or releases a key (`ReturnKey`, `Backspace`, `Escape`, …). `/k?t=TEXT` types text. |
| `/g` | Grabs the window and answers `{"png": "<path>", …}`. `/g?raw=1` sends the PNG bytes. |
| `/log` | Returns the latest log lines, including the `realize` report. |
| `/quit` | Shuts the app down. End every session with it. |

Add `&wait=1` to an input route to get the answer after the next frame is
drawn, so a following `/g` sees the result. `GET /` lists every route.

```sh
MAKEPAD_REMOTE=8151 card-host --bundle my-app/bundle --allow-unsigned --app-data .local-state &
sleep 7
curl -s 127.0.0.1:8151/snap
curl -s '127.0.0.1:8151/click?x=200&y=280&wait=1'
curl -s 127.0.0.1:8151/g          # {"png":"/…/grab-w0-00001.png",…}
curl -s 127.0.0.1:8151/quit
```

`card-host` needs the platform's graphical session even when its window is
hidden. Unverified: frame capture with Linux software rendering (llvmpipe,
including WSL). In reported runs there, `/g` answered `grab timeout`.

<a id="inspecting-a-card-before-publishing-card-studio"></a>

## Inspect a card before you publish: `card-studio`

`card-studio` (`crates/card-studio`,
[OctoSense ADR 0002](https://github.com/OctoSense-org/OctoSense/blob/main/docs/adr/0002-event-driven-app-agents.md)
section 7) renders a card in a hidden `card-host --remote` at each target
size, runs the measured checks and prepares a vision-critique request. It
drives `card-host` over HTTP and never links Makepad, so it builds without the
prepared runtime. Its checks also run on saved captures, with no GPU.

1. Build [`card-host`](#run-a-bundle-locally-card-host) first, then
   `card-studio`:

   ```sh
   cargo build --release -p octosense-card-studio
   ```

   `card-studio` looks for `card-host` in this order: `--card-host`, the
   `card_host` key of `card-studio.json` next to its binary, `$CARD_HOST`, the
   binary next to its own, then `PATH`.

2. Render the card at the three named sizes: glance `350x160`, phone
   `390x844` and desktop `1200x800`.

   ```sh
   export CARD_STUDIO_KIT=../octoscript-makepad/components/l0   # the L0 kit a bare card is lowered with
   target/release/card-studio render --card news.card --data digest.json \
       --size glance --size phone --size desktop --out out/
   ```

   `render` writes `out/report.json`. Per size, the report holds the PNG, the
   widget snapshot, the widget tree, the log, `card-host`'s `realize` report,
   the findings and the metrics. It exits 0 when no finding is an error and 1 when
   one is. Fix every error, then render again.

3. Prepare the vision-critique request:

   ```sh
   target/release/card-studio critique --report out/report.json --rubric skills/card-studio/rubric.md --inline > request.json
   ```

   `request.json` holds the prompt, the answer's JSON schema and, per size,
   the PNG, the widget snapshot and the findings. Send it to a vision model
   yourself: `card-studio` makes no model call.

4. Re-run the measured checks on saved captures. This step needs no
   `card-host`:

   ```sh
   target/release/card-studio check --snap out/glance.snap.json --tree out/glance.tree.txt --log out/glance.log.json --size glance
   ```

   It prints the findings and metrics as JSON, and exits 0 on a pass and 1 on
   a failure. For a card with more rows than the glance tile holds, it prints
   (trimmed):

   ```text
   {
     "findings": [
       {
         "check": "text_truncated",
         "message": "\"Artificial intelligence regulation\" gets 12x14pt, about 9.9x too small for 34 characters: it is cut or squeezed",
         …
         "severity": "error",
         …
       },
       {
         "check": "text_hidden",
         "message": "\"Grid\" was not laid out: the card ran out of room before it",
         …
         "severity": "error",
         …
       },
       …
     ],
     …
     "summary": {
       "errors": 7,
       "infos": 0,
       "pass": false,
       "warnings": 0
     },
     …
   }
   ```

The crate documents the checks, the severity model, the report and the
critique payload in `src/checks.rs`, `src/report.rs` and `src/critique.rs`.
[`skills/card-studio`](../skills/card-studio/SKILL.md) wraps `card-studio` as
a skill for octos.

## Maintain your app repository

- Use the starter's `AGENTS.md` as the entry point to these guides. Merge it
  into an existing repository's instructions; do not overwrite them.
- Keep app-specific behavior, data sources and tests in the app's own
  repository.
- Record the Hub and runtime revisions each release was built and tested
  with.
- Offline, keep a copy of the guides pinned to a known Hub revision, not an
  untracked copy that drifts.
- A passing build or gate does not prove that the visuals are right, that
  input works or that the app runs on a given platform. Test those with the
  [native-instrument guide](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/core/NATIVE-INSTRUMENT.md).

## Troubleshooting

### `card-host` fails to build

`cargo build -p octosense-card-host` stops while compiling `octosense-appstore`:

```text
error[E0599]: no variant, associated function, or constant named `TextInputStateQuery` found for enum `makepad_widgets::Event` in the current scope
error: could not compile `octosense-appstore` (lib) due to 1 previous error
```

`crates/appstore/src/services.rs` matches `Event::TextInputStateQuery`, an
IME event that plain Makepad `d653bc44` does not define. OctoSense's runtime
patch (`tools/runtime-patches/makepad-settings.patch`) adds it. `octosense-appstore`
matches the event only under the feature `text-input-state-query`, which
`octosense-appstore` turns on by default. `octosense-app-hub-app` also turns
it on by default and passes it to `octosense-appstore`. `card-host` depends on
`octosense-appstore` with the feature off.

If `card-host` fails on its own, your App Hub checkout predates that change.
Update it and build again:

```sh
git pull
cargo build --release -p octosense-card-host -p octosense-app-hub
```

Cargo unifies features across the packages of one build, so the feature is on
whenever any package in the build keeps it on. Without OctoSense's runtime
patches:

| Packages | Build without the patches |
| --- | --- |
| `octosense-card-host`, `octosense-app-hub`, `octosense-card-studio` | Yes |
| `octosense-appstore`, `octosense-app-hub-app` | Only with `--no-default-features` |
| `octosense-appstore-app`, or the whole workspace (`cargo test --workspace`) | No |

To build the last row, apply OctoSense's runtime patches to `../makepad`, in
the order OctoSense's `runtime-patches.lock.json` lists them.
