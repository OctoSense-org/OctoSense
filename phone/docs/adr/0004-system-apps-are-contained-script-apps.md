# ADR 0004: First-party system apps ship as contained script apps

- **Date:** 2026-09-25
- **Status:** Proposed
- **Implementation status:** News, Photos, Maps, Camera and Mail are built as script apps. News, Photos, Maps and Mail run on the desktop card host and in the phone shell (Android emulator: Mail's sign-in reached Gmail's POP3 server, and its demo account's password was sealed by the Android Keystore; the IMAP client was checked against Gmail's sign-in and a scripted server). Camera's capture path exists only in the Android and HarmonyOS backends and the emulator refuses the camera texture, so Camera awaits a real device. The native News, Photos and Maps crates stay behind `app-news`, `app-photos` and `app-maps` for comparison builds; the native Mail and Camera modules were removed on 2026-09-25, when their apps moved to Octoscript-AppCard as script bundles (since moved to OctoSense-System-Apps, `apps/<name>/bundle/`) (they remain in Git history). Ported from OctoSense-mobile into this Home on 2026-09-25 (both the standalone and the ROM variant link them; the makepad side rides a reviewed runtime patch until makepad#30 lands) and run on the OnePlus 6 as a separate test package: the home tiles, dock and app list show all five, Mail signs in to its demo account and reads, Photos opens, and Camera takes a photo. Performance measurement against the native apps is outstanding.
- **Scope:** First-party apps that ship with the shell and the ROM: which tier they run in and how they are packaged. It amends [ADR 0002](0002-agentic-app-security-model.md), where every first-party app is a trusted native module.
- **Relates to:** [ADR 0003](0003-app-hub-and-store.md) (bundles, manifests, the card runner).

## Context

ADR 0002 puts every first-party app in the trusted tier: Rust linked into the shell, running with the shell's full reach. A panic, a runaway loop or a leak in News, Photos or Maps runs in the one process that also draws the home screen. Only card and script apps from the catalog are contained.

Most of what these three apps do is presentation plus network requests and a little storage: a feed list, a photo grid, a place search and a route. The heavy parts are native already and reusable from script: the map engine (`MapView`), the system web view, image decoding, and route and place lookups (`sys.*`).

## Decision

**A first-party app whose logic is presentation, requests and storage ships as a system app: a contained script bundle that runs exactly like an installed app.**

- **The unit is an ordinary bundle** that lives with its app in OctoSense-System-Apps, `apps/<name>/bundle/`: `manifest.json` and a `main.splash` program, plus small data and artwork, beside the app's `host-service/` where one exists. Every OctoSense shell (this Home, the desktop shell) pins that repository and names the bundles it includes in its `system-apps.json`. The build packs each (`octosense_app_hub::pack::pack_system_app`, from `apps/app-hub/build.rs`) and stamps the digest of exactly the packed files into the packed manifest. Large artwork the shell owns (Photos' sample library) is mounted by `system-apps.json`, compiled into the binary, and served from memory at bundle paths, so it is not packed, unpacked or hashed on every build.
- **It is contained like an installed app.** The Card runner opens it in its own isolate, under the policy its manifest resolves to: its own storage jail, its host list on every network path (requests, artwork, `sys.*` data, map tiles, the web view), a location grant for GPS, and a heap cap. Only admission differs. The pack arrived with the signed build, so no catalog vouches for it and no signature is asked for; its bytes must still hash to its manifest, and it runs under `HostLimits::system()`, sized for an app kept open for an hour rather than a card.
- **Ids under `os.` are reserved.** No store may install one, so no download can take a system app's jail. The launcher keeps each app's short id (`news` for `os.news`), so icons, tiles, the dock and recents do not change.
- **The bundle is unpacked outside every jail** (`<app data>/.system/<id>/<pack hash>/`), so an app cannot rewrite its own program.
- **Two new capabilities** make the contained tier usable for real apps without widening requests: `images` (pictures from any public https host, such as a feed's thumbnails) and `web` (any public https page in a web view that has no way back into the app).

**Devices and credentials are host services, not script APIs.** An app that needs something it must not hold gets a service instead (`services.rs` in the App Hub `appstore` crate):
- The app calls `host.request("family.method", args, callback)`. The isolate refuses it unless the policy grants the family.
- The Card runner hands granted requests to the service registered for the family. The service works in Rust and answers, from a worker thread if the work is slow.
- When the person must act (typing a password), the service raises a **sheet**: a host-owned isolate drawn over the app, modal, and rebuilt on every opening. Calls from the sheet arrive marked as such, and only those may carry a secret.
- **Camera** is native vocabulary rather than a service: `CameraPreview` draws the device preview itself and hands script one call per finished capture. It is gated by `camera`, with `microphone` and `library` as separate grants. It saves only into the app's jail while under quota, and releases the device when it stops drawing, when the app closes and when its isolate is collected.
- **Mail** is the `mail` service (`apps/mail/host-service` in the AppCard repository). It reads over IMAP (every folder, and the read flag goes back to the server) or POP3 (the inbox), sends over SMTP, and grants each account only to the apps that added it. Accounts and mail sit under the host's own directory, outside every jail; passwords go to the platform's secret store (the keychain on Apple systems, a file sealed with an Android Keystore key on Android). A message's HTML reaches the app rebuilt from the few tags its `Html` view draws, with nothing remote in it, so opening mail fetches nothing. The app gets accounts, folders, headers, messages and a send; never a socket or a password.

**Secrets are the host's, for every contained app.** No script app (first-party or from the store) collects a password, a PIN or a one-time code. The person types a secret only on a host service's sheet, and it goes to that service, never through the app. This is held in four places, so that no one layer has to be perfect:
- *Runtime:* a password or one-time-code field (`is_password`, or content type Password, NewPassword or OneTimeCode, which also invites autofill) inside a policed isolate takes no input and says so. A sheet's isolate is the host's and is not policed.
- *Services:* a method under `<family>.sheet.` is accepted only from that family's sheet. `dispatch` refuses anyone else before the service runs, so no service has to remember the check. Only a service can open a sheet; there is no script call for it.
- *Admission:* the App Hub gate refuses a bundle whose scripts declare such a field.
- *Presentation:* the sheet is drawn by the host above the app, is modal, and names OctoSense as the one asking.

What this does not stop: an app can still ask for a secret in an ordinary text field. That is phishing rather than a capability, and it is left to the agent scan and review at admission, to the listing's accountability, and to the host list, which bounds where anything typed can be sent.

**App Hub stays native.** It verifies signatures and digests, enforces the anti-rollback floor, writes jails and mounts apps under their policy. It is the trust anchor, and its own logic is the check.

## What the runtime needed

Found by building these three apps; each fix is general, not app-specific.

- Every path out of an isolate answers to its policy: map tiles and archives, WebCard fetches and navigation, the WebView document (a CSP from the host list), and `sys.gps`/geocode fallbacks (the `location` grant). The user's saved lists need `profile`, and `agent.notify` needs `agent`. Grant-gated vocabulary: `register_splash_isolate_mod_for`.
- Script requests left `max_response_body_bytes` at 0 and failed every response. Native string builders bailed under a heap cap. An `on_render` re-render diffed onto old children, so rows kept stale structure and labels lost their wrap width.
- New vocabulary: `WebReader` (a navigable, bridge-less web view the isolate GC closes), `GestureView` and `SheetView` (native gesture recognition, one script call per gesture), `View.set_visible`, and `"xml".parse_feed()`.

## Consequences

- A fault in one of these apps is contained to its isolate: it can exhaust its budget or its heap, not the shell.
- These apps can be updated by rebuilding the bundle, and could later be delivered through the catalog like any other app.
- Script is interpreted. Rows are built in script and each isolate entry is capped at 64 ms. The apps keep per-render work small (thumbnails rather than full images in grids, one render per refresh), and a virtualized script list is the next runtime step for long lists.
- The native News, Photos and Maps crates remain until on-device measurement on the OnePlus 6 shows the script apps are at least as fast and as light, then they are removed. Mail and Camera were removed ahead of that measurement, at the owner's decision, so the platform carries one Mail and one Camera.

## Known gaps

- Mail: no attachments to open or send, no remote images (by design, until a per-message choice), no search, and over IMAP a folder's first sync fetches only its newest 25 messages. A development build on macOS asks for keychain access again after every rebuild; `OCTOSENSE_MAIL_VAULT=file` keeps its passwords in an owner-only file instead.
- Camera: capture, zoom, focus and recording are unverified on a device.
- News: the person's own feeds (a runtime host grant with consent), and tools exposed to the assistant.
- Photos: pinch-to-zoom is wired but unverified on a device; there is no device photo library, which would need its own capability and service.
- Maps: no offline archives; places, routes and guidance come from the same keyless services as before.
- Runtime: a view that starts hidden does not draw its `show_bg` background (use `SolidView`); string-keyed object maps with long keys are pathologically slow.
