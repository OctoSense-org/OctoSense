# Hub app starter

English | [简体中文](README.zh-CN.md)

Copy this directory into a new app repository, following
[Build your first Hub app](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/FIRST-APP.md).
It is a **metadata scaffold** for a card app, not a runnable or publishable
demo. For a script app (`main.splash`), use `tools/octo new` from
[OctoSense App Flow](https://github.com/OctoSense-org/OctoSense-App-Flow)
(formerly Design Flow) instead of this directory.

The starter holds two groups of files:

| Where | Files |
| --- | --- |
| Inside `bundle/`, the release artifact | `manifest.json` (schema 1), `listing.json` with every field, and the example icon `assets/icon.svg` |
| Outside `bundle/` | `README.md` and `README.zh-CN.md`; `AGENTS.md` with `CLAUDE.md` and `GEMINI.md`; `.gitignore`; `.gitattributes`, which keeps `bundle/` byte-exact in Git |

Before you submit:

1. Replace the app id and name in `manifest.json`, and every example value
   in `listing.json`, including the publisher block.
2. Set `platforms` in `listing.json` to the platforms you tested. The starter
   declares
   `["macos"]`; keep it only if you tested the app on macOS.
3. Set `license` in `listing.json` to your app's license. The starter's
   `Apache-2.0` is only an example.
4. Replace `bundle/assets/icon.svg` with your app's artwork.
5. Generate and review `bundle/page.card`, the optional `page.data.json`, the
   card's `kit/` directory and local assets with the
   [image-to-card flow](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/flows/image-to-card/FLOW.md).
6. Run the unsigned bundle in
   [`card-host`](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/DEVELOPMENT.md#run-a-bundle-locally-card-host)
   and capture `bundle/screenshots/01-main.png`.
7. Stamp, check and review the completed `bundle/` directory, release it
   through the GitHub release workflow that App Flow's `tools/octo publish-github`
   installs, and submit it, as
   [Submit an app](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/SUBMITTING.md)
   describes.

The manifest's initial digest is a placeholder; `hub stamp` writes the real
value. The starter has no `page.card` and no screenshot, so a stamped but
otherwise untouched copy fails the gate with two refusals:

```sh
hub stamp bundle
hub check bundle --allow-unsigned
```

```text
my-app 0.1.0 — REFUSED
  [refused] entry (main.splash): the bundle has no entry: main.splash (a script app) or page.card (a card)
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
  [refused] listing: screenshots/01-main.png is named by the listing but is not in the bundle
  grants: capabilities {}, hosts {}, storage none, agent none
hub: the bundle was refused
```

Do not add a placeholder card or screenshot just to pass. The gate does not
replace native rendering and input tests.

Keep everything but the app itself outside `bundle/`: this README,
`AGENTS.md`, keys, tools, build output, application data and the review packet
that `hub scan` writes.
