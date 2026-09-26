# OctoSense System Apps

The first-party apps every OctoSense device ships: **News, Photos, Maps, Camera
and Mail**. Each is a *contained script app* (OctoSense ROM `home/docs/adr/0004`):
a Splash program run by App Hub's Card runner in its own isolate, under the
permissions its manifest asks for, exactly like an app a developer publishes
through the App Hub.

| App | Bundle | Asks for | Notes |
| --- | --- | --- | --- |
| News | [apps/news/bundle](apps/news/bundle) | `storage`, `net`, `images`, `web` | feeds, a reader for stories |
| Photos | [apps/photos/bundle](apps/photos/bundle) | `storage` | the shell mounts its sample library as `{{assets}}/photos` |
| Maps | [apps/maps/bundle](apps/maps/bundle) | `storage`, `net`, `location` | map, places, routes |
| Camera | [apps/camera/bundle](apps/camera/bundle) | `storage`, `camera`, `microphone`, `library` | over the runtime's `CameraPreview` |
| Mail | [apps/mail/bundle](apps/mail/bundle) | `storage`, `mail` | calls the [`mail` host service](apps/mail/host-service) |

## Layout

```
apps/<name>/bundle/         the app: manifest.json, main.splash, artwork
apps/<name>/host-service/   a Rust service the app calls through host.request (Mail only)
```

`bundle/` has the same shape as any App Hub app, so these double as worked
examples. How to write, run, check and publish such an app is in
[OctoScript App Design Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow).

## Who uses this repository

The OctoSense shells pin a revision of it and choose which apps to ship:

- **OctoSense ROM** (`home/`): `home/native-apps.lock.json` pins this repository,
  `home/system-apps.json` lists the apps, and `home/apps/app-hub/build.rs`
  packs each `bundle/` into the build. The same Home runs as the standalone
  launcher and inside the ROM image.
- **OctoSense desktop shell**: not wired yet; it would pin and select the same way.

The Mail host service (`octosense-mail-service`) is linked by the shell. Its
tests run from a shell workspace that links it: in the ROM,
`cd home && cargo test -p octosense-mail-service`.

## Rules for these apps

- **No app collects a secret.** Passwords and codes are typed only on a host
  service's sheet (Mail's sign-in). A password field in an app takes no input.
- **Ask only for what the app uses.** Every network host is declared; the
  runtime refuses anything else.
- **Ids under `os.` are reserved** for this repository; no store can install one.

History: these bundles and the Mail service were first written in
OctoSense-mobile (archived) and OctoScript-App-Design-Flow (formerly
Octoscript-AppCard), where their history remains.
