# Build your first Hub app

English | [简体中文](FIRST-APP.zh-CN.md)

You start from a template and finish with an unsigned bundle that runs in
`card-host` and passes the gate, the admission checks that `hub check` runs.
[Submit an app to the App Hub](SUBMITTING.md) then takes it through the four
stages of a submission, from request to publication.

A Hub app is one of two kinds, and both are submitted the same way:

| Kind | Entry | What it is |
| --- | --- | --- |
| Script app | `main.splash` | A Splash program with its own state, handlers, storage and requests. |
| Card app | `page.card` | An L0 card with its data and its kit, the widget kit that renders it. The host lowers the card to those widgets. It has no logic of its own. |

A native app ships inside a shell release instead
([Choose a delivery path](DEVELOPMENT.md#choose-a-delivery-path)).

## 1. Prepare the tools and an app repository

1. Set up the workspace `~/octosense-ws` as
   [Quickstart §1](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/QUICKSTART.md#1-prerequisites)
   describes: clone App Hub and OctoSense App Flow (formerly Design Flow) side
   by side, then run App Flow's `tools/setup-native.py`. It adds the `makepad`,
   `octoscript-makepad` and `octoscript` checkouts that App Hub's `Cargo.toml`
   patches in.

   ```sh
   cd ~/octosense-ws/OctoSense-App-Flow
   python3 tools/setup-native.py
   python3 tools/setup-native.py --check
   ```

   Success: `--check` exits 0 and prints the pinned revisions. If a checkout is
   at another revision or has local changes, `--check` stops with
   `RuntimeError: …/makepad differs from the unified runtime source lock`. If a
   checkout is only at another revision, run
   `python3 tools/setup-native.py --update`; it moves clean checkouts to the
   locked revisions. If a checkout has local changes, keep them, and set up a
   second workspace, such as `~/octosense-ws2`, the same way.

2. Build `hub` and `card-host`:

   ```sh
   cd ~/octosense-ws/OctoSense-App-Hub
   cargo build --release -p octosense-card-host -p octosense-app-hub
   ```

   Success ends with ``Finished `release` profile [optimized] target(s)``. The
   binaries are in `target/release/`, or in `$CARGO_TARGET_DIR/release/` if
   you set it. If the build fails with `no variant … TextInputStateQuery`, see
   [`card-host` fails to build](DEVELOPMENT.md#card-host-fails-to-build).

3. Put both tools on your `PATH`, and check `hub`:

   ```sh
   export PATH="${CARGO_TARGET_DIR:-$PWD/target}/release:$PATH"
   hub
   ```

   Success prints the usage, starting with
   `hub — the OctoSense app hub command (ADR 0003)`. The export lasts for this
   shell only; add the line, with the absolute path, to your shell profile to
   keep it.

4. Choose the app id now; it is permanent. It is 1 to 64 characters of
   `[a-z0-9.-]` and not under `os.`, and its last segment is not a
   [reserved name](PUBLISHING.md#ids-and-reserved-names)
   ([section 3](#3-choose-the-id-capabilities-and-artwork) has the details).
   Then create the app repository from a template, in a new directory:

   - **Script app.** Run App Flow's `tools/octo new`. Name each platform
     you will test the app on with `--platform`; repeat the flag for more
     than one.

     ```sh
     cd ~/octosense-ws/OctoSense-App-Flow
     tools/octo new ~/apps/my-app --platform macos --id com.example.mynotes --name "My Notes"
     ```

     ```text
     created …/apps/my-app
       id com.example.mynotes, name 'My Notes', version 0.1.0, bundle stamped
       target platforms: macos; verify each before publishing
     ```

     It copies the `bundle/` of App Flow's
     [`templates/script-app/`](https://github.com/OctoSense-org/OctoSense-App-Flow/tree/main/templates/script-app),
     a small notes app with a manifest, a listing and an icon, and adds
     `AGENTS.md`, `CLAUDE.md`, `GEMINI.md` and a `.gitignore`. It writes your
     `--platform` values into the listing's `platforms`. Then it stamps the
     bundle: it writes the bundle's digest into `manifest.json`. Before it
     creates any file, it refuses an id under `os.` unless you pass
     `--system`, and an id whose last segment is a reserved name:
     `octo: id 'com.example.notes' uses reserved native/host namespace 'notes'; choose an app-specific name`.

   - **Card app.** Copy App Hub's [app starter](../templates/app/README.md):

     ```sh
     mkdir -p ~/apps && test ! -e ~/apps/my-app && cp -R ~/octosense-ws/OctoSense-App-Hub/templates/app ~/apps/my-app
     ```

     It holds metadata and an example icon, and deliberately no `page.card`,
     kit or screenshot. The gate refuses it until you add them
     ([section 5](#5-check-the-bundle-with-the-gate)).

   - **Existing repository.** Merge the template files by hand, and keep the
     repository's own instructions and metadata.

   Only `bundle/` is submitted. Keep `AGENTS.md`, design prompts, source tools,
   logs, keys and test state outside `bundle/`
   ([Lay out the repository](SUBMITTING.md#1-lay-out-the-repository)).

5. Make the directory a Git repository:

   ```sh
   cd ~/apps/my-app && git init
   ```

## 2. Design and build the app

First write a short brief: the app's purpose, screens, actions, data sources,
stored state, and error and empty states.

### A script app

Follow App Flow's
[script-app flow](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/script-app/FLOW.md).
The calls a program may make (`fs`, `net`, `host.request`, widget handles) are
in the [script API](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/SCRIPT-API.md).
The [system apps](https://github.com/OctoSense-org/OctoSense/tree/main/apps)
are complete examples of the same format.

```text
my-app/
  AGENTS.md
  README.md
  bundle/
    manifest.json
    listing.json
    main.splash          # the program; its presence makes this a script app
    assets/              # local artwork and your icon
    screenshots/
      01-main.png        # a real capture, added in section 4
```

- Name the bundle's own artwork through `{{assets}}`, for example
  `Image{src: http_resource("{{assets}}/assets/logo.png")}`. The host replaces
  `{{assets}}` with the loopback origin that serves this bundle and nothing
  else. Never write that origin, a `file://` path or a `../` path yourself.
- Use HTTPS in `main.splash` and accurately disclose expected destinations
  in `network.hosts`. Current policy does not use that list as an allowlist;
  the gate still refuses `http://`.
- Never ask for a password, PIN or one-time code. The gate refuses such a
  field, and at runtime such a field accepts no input. Sign-in belongs to a
  host service, which collects the secret on its own sheet, a panel the host
  draws over the app ([Sheets](PUBLISHING.md#sheets-apps-never-collect-secrets)).
- `card-host` serves no host service except `runtime` discovery: every other
  `host.request` answers `no service answers "<family>" on this device`. Test
  those screens in the [compatible desktop release](../README.md#download-a-compatible-host)
  ([Before you start](SUBMITTING.md#before-you-start)).

### A card app

Use App Flow's
[image-to-card flow](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/image-to-card/FLOW.md)
to create and review the card. You still implement the app's data binding: a
screenshot, or the flow's atlas of screens, is not a running app. The
[L0 reference](https://github.com/OctoSense-org/OctoSense/blob/main/apps/appcard/a2app-l0/framework/l0.md)
defines data, state and events.

```text
my-app/
  AGENTS.md
  README.md
  bundle/
    manifest.json
    listing.json
    page.card
    page.data.json       # optional; omit it if the card needs no data
    kit/                 # every file this card's kit needs
    assets/              # local runtime artwork and your icon
    screenshots/
      01-main.png        # a real capture, added in section 4
```

The gate accepts a card's `font_src` as a font file in the bundle or one of
the three shipped resources: Inter, LXGW WenKai Regular and LXGW WenKai Bold. OctoSense desktop
0.1.0-beta.2 does not load a bundled font. Read [Fonts](PUBLISHING.md#fonts)
before you set a font or show Chinese text.

### Both kinds

- Keep every asset reference inside the bundle. Check exported kit and data
  files for author-machine paths and development-server URLs.
- Keep development notes outside `bundle/`; retain required asset licenses
  and attribution as `.txt` or `.md`. Documentation links do not grant network
  access. URLs in card data still face the asset checks. Agent files and
  `main.splash` reject unsafe references (`http://`, `file://`, `../`),
  without requiring HTTPS hosts to appear in `network.hosts`.
- Adapt any flow that relies on an external Python or browser controller to
  the contained runtime before you submit.

## 3. Choose the id, capabilities and artwork

Edit `bundle/manifest.json`:

- **`id`** is permanent: 1 to 64 characters of `[a-z0-9.-]`. The gate refuses
  ids under `os.`, and ids whose last segment is one of the 23
  [reserved names](PUBLISHING.md#ids-and-reserved-names), such as `notes`,
  `weather` or `terminal`. If the app may ship tools later, make the last
  segment a valid tool namespace, `[a-z0-9_]{1,24}`: `com.example.mynotes`,
  not `com.example.my-notes`.
- **`version`** starts at `0.1.0`. Every release needs a new one.
- **`capabilities`** lists only what a screen uses. Check the **Served today**
  column of [Capabilities](PUBLISHING.md#capabilities): some, such as `llm`
  and `calendar`, pass the gate but are served only to system apps, and
  `prompt` is served nowhere. Disclose expected destinations in
  `network.hosts`; neither field grants access. Compatible public APIs still
  check app/account scope, consent and resource limits.

Edit every placeholder in `bundle/listing.json`: `subtitle`, `description`,
`category`, `publisher` (with `support` and `privacy_policy_url`),
`platforms`, `release_notes` and `license`. The templates' `example.com` URLs
are placeholders. App Hub's app starter declares `"platforms": ["macos"]`, and
`tools/octo new` writes the platforms you named; neither is a test result.
Declare only the platforms you ran the app on.

Replace the example icon, following [Icons](ICONS.md). The icon path in the
listing must match the exported file.

## 4. Run the unsigned development bundle and capture it

Your editable bundle stays unsigned; the release workflow seals a copy when
you push a tag. `card-host` has no publisher verifier, so it refuses a sealed
release even with `--allow-unsigned`.

1. Stamp the bundle: `hub stamp` writes the bundle's digest into
   `manifest.json`. Do this after every change to the bundle.

   ```sh
   cd ~/apps/my-app
   hub stamp bundle
   ```

   Success prints the bundle's digest, 64 hex characters. If it prints
   `hub: manifest is not valid: unknown field …`, remove the field the message
   names.

2. Start the app in `card-host` from the App Hub checkout, as
   `tools/octo run` does. `MAKEPAD_REMOTE=8151` opens the remote-control routes
   on port 8151 for steps 3 to 5.

   ```sh
   cd ~/octosense-ws/OctoSense-App-Hub
   MAKEPAD_REMOTE=8151 card-host --bundle ~/apps/my-app/bundle \
     --app-data ~/apps/my-app/.local-state --allow-unsigned &
   ```

   Success: the log shows the app admitted, and the window shows the app.

   ```text
   card-host: com.example.mynotes 0.1.0 admitted — capabilities {"storage"}, hosts {}, storage 16777216 bytes, agent none
   ```

   On a refusal, the log shows `card-host: refused: <reason>`, and the window
   shows "card-host refused this bundle" with the reason. Fix what it names,
   stamp again and restart `card-host`.

3. Drive every screen through the remote routes. `/snap` lists the visible
   widgets with their rects and text; `/click`, `/t` and `/k` send input (see
   the [`card-host` reference](DEVELOPMENT.md#run-a-bundle-locally-card-host)).

   ```sh
   curl --silent 127.0.0.1:8151/snap
   ```

   Success is one line of JSON, starting
   `{"s":[{"i":"main_window","ty":"Window","r":[0,0,412,892],…`. Read the log
   for errors after the `admitted` line. For scripted input, see the
   [native instrument guide](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/core/NATIVE-INSTRUMENT.md).

4. Capture a frame:

   ```sh
   mkdir -p ~/apps/my-app/bundle/screenshots
   curl --fail --silent --show-error -o ~/apps/my-app/bundle/screenshots/01-main.png '127.0.0.1:8151/g?raw=1'
   ```

   Success: `01-main.png` is a PNG at the window's pixel size, 824 × 1784 for
   the default 412 × 892 window on a 2× display. If the capture fails with
   `curl: (22) The requested URL returned error: 404`, the first frame is not
   drawn yet; wait a few seconds and capture again.

5. Open the PNG and check it; never submit a screenshot that shows an error.
   Then quit `card-host`:

   ```sh
   curl --silent 127.0.0.1:8151/quit
   ```

   Success: `quit` answers `{"ok":1}`.

For screens that need a host service, see
[Capture the screenshots](SUBMITTING.md#4-capture-the-screenshots).

## 5. Check the bundle with the gate

The screenshot changed the bundle, so stamp it again, then run the gate on the
unsigned bundle:

```sh
cd ~/apps/my-app
hub stamp bundle
hub check bundle --allow-unsigned
```

Historical output from an earlier CLI (the current CLI labels the summary
`declarations:`, not `grants:`):

```text
com.example.mynotes 0.1.0 — PASSED
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
  grants: capabilities {"storage"}, hosts {}, storage 16777216 bytes, agent none
```

The unsigned warning is expected at this stage. Compare the current
`declarations:` line with actual usage and correct missing or unused
disclosures. Every admitted app has a bounded private jail, even without a
`storage` declaration; the older `storage none` output below no longer
describes current policy.

On `REFUSED`, `hub check` exits 1. Fix each `[refused]` line
([Common refusals and how to fix them](SUBMITTING.md#common-refusals-and-how-to-fix-them)),
then stamp and check again. The untouched app starter fails twice, on its
entry and on its screenshot:

```text
my-app 0.1.0 — REFUSED
  [refused] entry (main.splash): the bundle has no entry: main.splash (a script app) or page.card (a card)
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
  [refused] listing: screenshots/01-main.png is named by the listing but is not in the bundle
  grants: capabilities {}, hosts {}, storage none, agent none
hub: the bundle was refused
```

`PASSED` means the bundle meets the admission rules. `hub check` does not run
the app, catch placeholder text or check your privacy policy.

## 6. Release and submit

Continue with steps 5 to 9 of [Submit an app to the App Hub](SUBMITTING.md):

| Step | What you do |
| --- | --- |
| [5. Produce the final bytes](SUBMITTING.md#5-produce-the-final-bytes) | Scan the bundle, answer the review questions and add the GitHub release workflow. |
| [6. Freeze and verify the release](SUBMITTING.md#6-freeze-and-verify-the-release) | Commit, push a new tag and verify the release pack that the workflow builds. |
| [7. Open the submission issue](SUBMITTING.md#7-open-the-submission-issue) | Request publication, then add the tag, the full commit SHA, the release pack and the verification output. |
| [8. What reviewers check](SUBMITTING.md#8-what-reviewers-check) | See what a reviewer checks; the findings appear in your issue. |
| [9. After you submit](SUBMITTING.md#9-after-you-submit) | See how an App Hub admin approves and the Hub publishes, and ship updates as new versions. |

Two rules hold throughout:

- **Test the editable source, and never edit a sealed release.** The workflow
  seals the bytes it releases; any change needs a new version and tag
  ([GitHub publisher provenance](PUBLISHING.md#github-publisher-provenance)).
- **You need no publisher key.** App Hub accepts only GitHub-attested
  releases, and your app's GitHub workflow attests each one
  ([Signing](PUBLISHING.md#signing)).

To try a release before App Hub publishes it, install it from a local test
catalog in an OctoSense desktop shell built from source
([Rehearse the store path locally](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/PUBLISHING.md#4-rehearse-the-store-path-locally)).
Unverified: the rehearsal with a GitHub-attested release; the recorded run
used a key-signed test app.

## Troubleshooting

| Symptom | Cause and fix |
| --- | --- |
| `cargo build` stops with `no variant … TextInputStateQuery` | See [`card-host` fails to build](DEVELOPMENT.md#card-host-fails-to-build). |
| `card-host: refused: this host has no GitHub publisher verifier` | The bundle is a sealed release. Run the editable source. |
| `no service answers "<family>" on this device` | `card-host` serves no host service except `runtime` discovery. Test the screen in the [compatible desktop release](../README.md#download-a-compatible-host). |
| `curl: (22) The requested URL returned error: 404` from `/g?raw=1` | No frame is drawn yet. Wait a few seconds and capture again. |
| `[refused] identity: app id "…" ends in "…", which is reserved` | The id's last segment is a [reserved name](PUBLISHING.md#ids-and-reserved-names). Choose another before your first release. |
| `[refused] digest: the bundle hashes to …, the manifest claims …` | The bundle changed after `hub stamp`. Stamp again. |
