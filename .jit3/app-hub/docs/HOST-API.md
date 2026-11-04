# Host API compatibility

English | [简体中文](HOST-API.zh-CN.md)

Contract 1.6 lets an app declare the host APIs it needs and discover the ones
a host implements. It adds declarations and discovery, not access: an app can
call only the Rust services compiled into its host, and every call still
checks the app's actual authorization. The same contract lets a bundle declare its own backend
([Sign in to your own backend](PUBLISHING.md#sign-in-to-your-own-backend)) and
run its own agent tools
([Script tool execution](PUBLISHING.md#script-tool-execution-script-tools-v1)).

These discovery declarations shipped in contract 1.8.0. The historical
[RC2 release](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.2), source `4ccf8e06`,
implemented them within the platform limits below, as RC1 did. Use
[desktop RC4](../README.md#download-a-compatible-host) for the current contract 1.11 policy.
An older host, such as OctoSense desktop 0.1.0-beta.2, serves none of these
APIs and refuses an app that requires them.

Published contract **1.10.0** also admits `files`, `device_calendar` and
`audio` ([version and publication receipt](../crates/app-contract/README.md#versions-on-cratesio)).
Its file declarations include the unpublished 1.9.0 work. Desktop RC2 pins this
contract and implements the native adapters described below on the platforms
each section names; RC1 has none of them, and the contract package alone adds
nothing to an installed host.

## Current source policy: declarations and authorization

`capabilities` and `network.hosts` describe an app's expected API and network
use for users and reviewers. Omitting a family or destination does not deny
an otherwise supported public API. This policy is included in [desktop RC4](../README.md#download-a-compatible-host).
Historical RC2 behavior is documented separately and does not describe these
current authorization rules.

Every admitted app gets a private storage jail with the resolved quota and a
network module. Device access still needs per-app consent and OS permission;
connected accounts keep app/account ownership and provider scopes. External
writes retain native review, agents retain opt-in and reviewed tools, and
other apps' data/tools still require sharing authorization. Internal host
profile data and unowned `agent.notify` are not public APIs.

`requires` and `host_api.required` remain compatibility checks, not capability
permissions. Component ABI/import validation, exact digests, publisher proof,
catalog withdrawal, quotas and supported platforms remain enforced. Missing
usage declarations may produce review warnings. `AppPolicy::allows` and
`allows_host` are legacy declaration queries; hosts must not use them as
execution authorization. Use `runtime.list` / `runtime.describe` to discover
actual methods and their platform/consent requirements.


The compatible runtime must include explicit camera intent: `capture({library:true})`
and `record_start({audio:true,library:true})`; omitted options are false.
Enabling public runtime flags must never implicitly record audio or export
captures. This overlay and the updated Hub adapter ship together.
The adapter exposes the microphone/library runtime flags only when the host
provides both its per-app consent broker and registered
`camera.capture_intent@1` support. A manifest declaration cannot supply either.

## Declare what the app needs

This manifest fragment requires `runtime.list` and uses `location.get` when
the host has it:

```json
{
  "requires": ["host-api-v1"],
  "capabilities": ["runtime", "location"],
  "host_api": {
    "required": {"runtime.list": 1},
    "optional": {"location.get": 1}
  }
}
```

| Field or marker | Meaning |
| --- | --- |
| `host_api.required` | Methods the app cannot run without, each with its exact ABI major version. Version 2 does not satisfy a request for version 1, and a version is not a host release number. |
| `host_api.optional` | Methods the app uses when the host has them. Give each one a fallback. |
| `host-api-v1` | Needed for `host_api`. It also puts camera, microphone and location behind a per-app consent, so the host must implement the `app_policy.device_consent@1` runtime ABI. A runner that only parses these fields does not satisfy it. |
| `backend-api-v1` | Needed for `backend`. The host must implement `auth.backend.request@1`. |
| `script-tools-v1` | Needed for `implemented_by: "app"` tools. The host must implement the `app_tools.dispatch@1` runtime ABI. |

The store checks these requirements when it installs the app and again each
time the app opens. It refuses an app that the host cannot serve, with
`app <id> needs a host implementing <method>@1` or
`this host does not implement required APIs: <method>@<version>`. A catalog
update that needs newer APIs does not take away an installed version that the
host still serves.

Before evaluating app source, the shared runner's `apply_device_consent` helper
binds the verified bundle's app identity to its live Splash heap. Its caller
must already have admitted the bundle; this helper does not replace admission.
Native storage access requires that exact identity as well as the app's jail.
The identity binding grants no device consent or OS permission. A failed bind
clears the old identity, and the card runner clears it again on refusal or close.
Plain Makepad still refuses apps that require the unsupported device broker.

## Discover what the host implements

### WASI sockets (unreleased)

Components importing `wasi:sockets` require `wasm.sockets@1` in
`host_api.required`, with the existing `host-api-v1` feature in `requires`.
This is a runtime ABI marker, not a `host.request` method; TCP, UDP and DNS
operations use standard WASI interfaces. RC5 can read this requirement but
cannot satisfy it, so installation is refused without invalidating the catalog.
The same requirement applies when an app pins a shared socket component.
See [WebAssembly components](PUBLISHING.md#webassembly-components) for admission
and declaration rules. Confirm runtime/platform support in the actual host;
this source change does not update a released binary.

### Native device calendars (contract 1.10)

The new `device_calendar` capability is separate from `calendar` (the system
Calendar app's own store) and `gcalendar` (Google's connector). It does not
grant either of them, or an OS permission. An app that needs native events
declares `device_calendar` and `host-api-v1`, with
`host_api.required: {"device_calendar.events.list": 1}` and any other methods
its workflow requires. A host without those methods refuses installation.

The published contract admits the capability; App Hub's policy admits the
read tool aliases. Desktop RC2 ships the macOS adapter (EventKit) and the
Android Home adapter; RC1, Home beta.1, Windows and Linux have none. The
adapter enforces app/account scope, OS permission, selected calendars and
trusted review of mutations with a physical press. Actual OS-calendar
interaction is still pending device acceptance, and recurring events and
attendees are read-only. The service's method descriptions remain the
authority for supported platforms and agent access.

### Reviewed Mail drafts (desktop RC2)

A compatible host exposes `mail.compose`
for app/account-bound local draft changes and `mail.compose_status` for their
status. Agent aliases require minimum risk `act` and `read`, respectively,
with `private_data: true`. They do not send mail. The foreground app requests
`mail.review_send` (or the reviewed `mail.send` compatibility entry point);
the host shows the exact draft and sends only after physical user approval.
Declare the method versions, and put `mail.review_send` under
`host_api.optional` when the listing names Windows or Linux. Desktop RC2 serves
this flow to any app granted `mail`: compose and status on every desktop
platform, the send review on macOS and Android only (on Windows and Linux it
fails with `Physical Mail send approval is unavailable on this platform`).
RC1 has none of it, the older system Mail draft methods remain
system-app-only, and SMTP acceptance on hardware is unverified.

### Runtime inventory

For discovery, call `runtime.list` with `{}` for every
described method, or `runtime.describe` with `{"method":"location.get"}` for
one:

- A method description gives its schemas, ABI version, usage family, platforms
  and whether agents may call it.
- `runtime_features` lists runtime ABIs, such as `app_tools.dispatch@1`.
  `host.request` cannot call a runtime ABI.
- Only methods with a registered description appear. Some older services have
  none, so the list is not a complete inventory.
- The result holds no account data or credentials.

Availability is not configuration or permission. `configured: null` means the
host does not know, and `authorization: "checked-on-call"` means the host
checks actual authorization on every call. To learn whether a service is
configured and authorized, call its status or account methods. Declaring a
requirement neither turns on an OS permission nor adds a provider
registration.

## Selected-file transfer (published in contract 1.10)

The `files` declaration describes host-owned file dialogs, not arbitrary
host paths. Current hosts give every admitted app a bounded private jail;
import/export does not require `files` or `storage` family declarations. The
person still selects the file in a foreground host-owned dialog.
Require `files.import@1` or `files.export@1` through `host_api.required`, or declare
them optional and inspect `runtime.list`. Desktop RC2 implements these APIs;
RC1 does not, and contract support alone never does. `files.status` reports
adapter availability.

The matching OctoSense implementation imports one selected file into a new
app-relative destination, and exports a snapshot of an existing app file. It
returns a relative path and byte count, never an OS path or Android provider URI.
Transfers are foreground-only and cannot be initiated by an agent/background
turn. Native `fs.write_bytes` is identified by the `storage.binary_write@1`
runtime ABI, not a callable `host.request` method.

Imports keep the current 1 MiB file limit and app storage quota; they refuse
existing destinations. A cancelled dialog returns `{"cancelled":true}`. Current
adapters cover macOS, Windows, Android and Linux with a native dialog helper;
iOS, OpenHarmony, web and direct-framebuffer Linux are unsupported. Check the
host's status and release notes for build and device validation.

### Photo selection and text sharing (desktop RC2)

A matching host adds `files.pick_photo({"path":"photos/chosen.jpg"})`, which
imports into the app's private jail. It opens an image chooser and validates
PNG, JPEG or WebP signatures before importing into a new app-relative path. It returns
`path`, `bytes` and `mime`, or `{"cancelled":true}`. The 1 MiB limit still
applies: larger originals fail explicitly; no image is silently resized.

`files.share({"text":"A short note"})` opens Android's share chooser and is
Android-only in this batch. Text must contain 1–8192 UTF-8 bytes. The
foreground call opens the native chooser and returns
`{"handoff":"chooser_opened","delivery":"unknown"}` only when that handoff
succeeds. It does not prove delivery or support attachments. Neither method
has an agent alias. Declare each method version and check the host's status;
desktop RC2 includes these APIs and RC1 does not. Device validation of the
new adapters remains pending.

## Foreground audio sessions (contract 1.10)

Desktop RC2 on macOS, and compatible Android hosts, expose these methods;
RC1, Windows and Linux do not. Declare the method versions rather than
inferring support from the capability name, and put them under
`host_api.optional` when the listing names a platform without them:

| Methods | Usage disclosures | Behavior |
| --- | --- | --- |
| `microphone.record_start({path,max_duration_ms})` | `microphone`, `storage` | After app and OS consent, record at most 30 seconds of mono 16 kHz PCM WAV into a new app file. |
| `microphone.record_status/record_stop/record_cancel({session})` | `microphone` | Inspect, stop and save, or discard this live app's recording. |
| `audio.play({path})` | `audio`, `storage` | Play a bounded local WAV, MP3, FLAC or Ogg file. |
| `audio.status/audio.stop({session})` | `audio` | Inspect or stop this live app's playback. |

The table names usage disclosures, not execution grants. Current hosts do
not reject these methods merely because a family declaration is absent;
recording still requires the consent described below.

Each response contains an opaque `session`, `status`, `path`, `error`, `frames`
and `format`. A start response means `starting`, not successful recording or
playback. Poll status until the first native callback reports `recording` or
`playing`, and observe terminal failures. Stopping a recording initially
returns `stopping`; use its path only after `saved`. Device frame counts do not
prove audibility. File input/output remains within the current 1 MiB limit;
decoded playback also has separate duration/memory limits.

These APIs are foreground-only with no agent aliases. Playback does not grant
microphone access; recording still needs `microphone.permission.request` and
OS approval. Sessions bind to the live app isolate, stop when it loses its
foreground surface or permission, and never resume automatically. Hosts must
arbitrate microphone ownership with dictation. Embedded store previews without
a dedicated active app identity refuse recording; open the installed app.
Source and synthetic tests do not establish real microphone/speaker acceptance.

## Limits

- **Agent tools.** A `host-service` tool can map with `host_method` to
  `auth.backend.me`, `auth.backend.request`, `runtime.list`,
  `runtime.describe`, the three `*.permission.status` methods or
  `location.get`, at minimum risk `read`, with
  `private_data: true`
  ([Map a tool to a shared service](PUBLISHING.md#map-a-tool-to-a-shared-service-host_method)).
  An agent's call never prompts, so `auth.backend.request` runs only declared
  `GET` operations and refuses a write before any HTTP request; a write still
  needs the foreground app and the host's native review. Permission requests
  and revocation, account management and sheet controls have no `host_method`.
  Admission does not replace the host's account, consent and platform checks.
- **Backends.** The host reads the `backend` block only from the admitted,
  signed bundle. It returns opaque connection handles, refuses redirects and
  any backend answer that carries a token, and asks the person to approve
  every write on its native review screen. A background or agent call
  cannot approve a write. Changing the endpoints, updating the app or
  withdrawing it ends the app's backend sessions.
- **Script tools.** The signed `app_tool(name, call_id)` handler runs in the
  open full app's own VM and storage folder. No native library or Wasm loads,
  and a closed app answers `app_not_running`.
- **Platforms.** The device-permission methods cover Android and macOS;
  `location.get` covers Android only and returns the last-known fix, of
  unknown age, while RC2's `location.sample` returns a fresh fix on macOS and
  Android. The RCs also embed ordinary `WebReader` pages on Windows (WebView2)
  and Linux X11/XWayland (WebKitGTK); native Wayland is unsupported.
  Linux/Windows advertise `auth.backend.request@1` for declared reads and
  implement external-browser backend login; RC2 adds the native link openers
  that login needs, and a Windows fixture built from RC2's source completed a
  browser sign-in against a synthetic backend, while Linux sign-in is not
  validated. Embedded backend login and protected writes remain unsupported
  and fail closed.
- **Permission requests.** Only an app in the foreground can request a
  permission. A request from an agent or a background card fails with
  `<method> is unavailable to agents/background surfaces`. A request still
  waiting when the host goes to the background fails with
  `authorization_required`, as do `location.get` and `location.sample` until
  the person grants the app location access.
- **Device widgets.** With `host-api-v1`, `CameraPreview`,
  `sys.request_location`, `sys.gps` and map GPS reads need the app's device
  consent too. After the host starts, each stays closed until the app calls
  its capability's permission method, which loads that saved consent:
  `camera.permission.status` before `CameraPreview`, and
  `location.permission.status` before a GPS read. Call these when the app
  opens.
- **`card-host`.** It implements none of the APIs that the three markers need,
  so it refuses apps that require them
  ([Run a bundle locally](DEVELOPMENT.md#run-a-bundle-locally-card-host)).
- **Unverified:** a physical permission approval, camera capture, live
  providers, real models, device-calendar reads and writes, SMTP delivery,
  audio capture and playback, the native file and photo choosers, and host API
  acceptance on Linux, Windows and phones.

For the calls, see
[Discover and use host APIs](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/HOST-API-V1.md)
in OctoSense App Flow (formerly Design Flow).
