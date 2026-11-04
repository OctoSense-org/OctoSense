# Changelog

`octosense-app-contract` follows the rules in [README.md](README.md#stability):
within `1.x` it only grows.

crates.io has 1.0.0, 1.1.0, 1.2.0, 1.5.0, 1.6.0, 1.7.0, 1.7.1, 1.8.0,
1.10.0 and 1.11.0.
Versions 1.3.0, 1.4.0 and 1.9.0 are unpublished: 1.5.0 includes the first two
versions' changes, and 1.10.0 includes 1.9.0's changes.
[README.md](README.md#versions-on-cratesio) shows how to update a lock file
that holds 1.2.0.

## 1.12.0 — unreleased

- `RESERVED_NAMES` gains `updater`: OctoSense's native Updates window. No
  store app may take `updater` as its id or namespace.
- Add the optional `window` manifest field (`WindowHint { width, height }`,
  with `WindowHint::new`): the size, in points, at which the app prefers its
  desktop window to open. It is a hint, not a grant: hosts clamp it to the
  screen, phones and other full-screen hosts ignore it, and the person can
  still resize the window. `AppPolicy` is unchanged by it.
- `SCHEMA_MINOR` is now 1, the first bump. A manifest that sets `window`
  must declare `"schema_minor": 1` or later; `parse` and `resolve` refuse it
  otherwise. A host on an earlier contract then reads such a manifest
  leniently: it ignores `window`, lists it in `ignored_fields()`, and
  computes the same signing bytes, instead of refusing an unknown field.
- `parse` refuses a width outside 320 to 4096, a height outside 240 to
  4096, and a `window` that is not an object of those two integers. The
  field is skipped when absent, so existing manifests and their signing
  bytes do not change.
- A manifest with `"schema_minor": 1` is now read strictly, like any
  manifest at this build's minor: an unknown field in it is refused rather
  than ignored. Manifests for later minors are still read leniently.

## 1.11.0 — 2026-10-10

Published through the [release workflow](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/38026012863)
from `b7e4ba8f2f1ccc7606fc8815256f0bf123c43c57`. The registry and downloaded
archive SHA-256 both match
`57d8c87511e17c5a8f965cbdba07a1cae100d78316159827c3a2061c84003bf6`.
This contract release alone does not supply a compatible host binary.

- Include the component and shared-component ABI/schema additions below.
- Treat capability names and network destinations as usage declarations.
  Backend configuration keeps its required ABI and account storage layout,
  but does not require `auth` in the capability list. Component ABIs no longer
  require `wasm`, and network/research metadata does not need a family grant.
- Preserve manifest signing bytes, exact dependencies, format validation,
  resource limits and required host features. `AppPolicy::allows` and
  `allows_host` remain declaration queries, not execution authorization.

- Add the `wasm-components-v1` feature. An app whose `fns/` holds a
  WebAssembly component (OctoSense ADR 0014) must list it in `requires`;
  `wasm` is a usage disclosure. A host that does not know it refuses the app instead
  of failing when the app first calls its functions. Existing manifests and
  their signing bytes do not change.
- Add the optional `components` manifest field and the
  `wasm-shared-components-v1` feature (App Hub ADR 0003). An app pins each
  shared WebAssembly component from App Hub's catalog by its name for it
  (`as`), the component's `id`, one exact `version` and the file's `blake3`
  digest. `parse` refuses the field without the feature,
  more than 8 entries (`MAX_COMPONENTS`), a name that
  is not `[a-z][a-z0-9_]{0,31}` or is used twice, a version that is not one
  exact semantic version, a digest that is not 64 lowercase hex characters,
  and an id that breaks the app id rules or starts with `os.`. New public
  items: `ComponentDependency`, `MAX_COMPONENTS`, `check_component_id` and
  `is_exact_version`. The field is skipped when empty, so existing manifests
  and their signing bytes do not change. A host that does not know the
  feature refuses the app.

## 1.10.0 — 2026-10-09

Published through the [release workflow](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/37901486707);
the [publication receipt](../../reviews/public-os-api-v1/contract-1.10.0-publication.json)
records the source commit, registry checksum and downloaded archive checksum.
This release includes the selected-file capability from the unpublished 1.9.0 work.

- Add separate `audio` playback capability. It implies no microphone, storage,
  network or background authority. Recording uses the existing `microphone`
  grant. Foreground-only recording, playback, photo selection and sharing
  methods have no agent aliases.

- Add the distinct `device_calendar` capability for host-mediated, selected
  native calendars. This is not the system Calendar app's `calendar` service
  or the Google Calendar `gcalendar` connector.
- The companion app policy admits read-only native-calendar aliases and
  app/account-bound `mail.compose` / `mail.compose_status` aliases. Draft
  preparation is not sending; external writes remain foreground host reviews.
- Existing capability grants, schema versions and signed manifest bytes do
  not change. Required host API declarations still fail on a host without the
  declared method versions. Adapter implementation and OS consent belong to
  the host; a contract release alone supplies neither.


## 1.9.0 — unpublished

- Adds the `files` capability for host-owned file selection and export dialogs.
  It grants neither arbitrary filesystem access nor the separate `storage`
  capability. Applications must discover or require the specific file methods
  their host implements; the contract alone supplies no OS adapter.

## 1.8.0 — 2026-10-08

Published through the [release workflow](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/37739082884);
the [publication receipt](../../reviews/github-publisher-v1/contract-1.8.0-publication.json)
records the registry and downloaded archive checksum.

- Adds optional `Integrity::github` / `GithubPublisher`, the required
  `publisher-github-v1` marker and the default-refusing
  `SignatureVerifier::verify_github` extension. Full Sigstore verification
  lives in App Hub, not this small contract crate.
- GitHub identity is included in canonical manifest signing bytes; only the
  attached proof is excluded, avoiding a manifest/bundle hash cycle.
- Existing manifests retain their serialized bytes and Ed25519 behavior.
  Older hosts refuse the new marker. New contract support alone grants no
  runtime authority, capability or publisher continuity.

## 1.7.1 — 2026-10-08

- Reserve the exact full app IDs `catalog.json`, `catalog.lock`,
  `catalog-v2.json` and `catalog-v2.lock`, ignoring ASCII case, so an app's
  install directory cannot collide with a host catalog or cache-lock file.
- Export `RESERVED_HOST_FILE_IDS`; `check_reserved_id`, policy resolution and
  the Hub gate share the refusal. `json` and `lock` are not reserved namespaces:
  `org.example.json` and `org.example.lock` remain valid.
- This is a security correction to identity validation. Schema, capability
  grants, manifest serialization and signature bytes are unchanged.

## 1.7.0 — 2026-10-07

- `KNOWN_CAPABILITIES` gains `wasm`: the app's own functions, WebAssembly
  modules in its bundle's `fns/`, run by the host's `wasm` service in a
  sandbox with a deadline and a memory cap. A function gets its input and
  returns its output; it reaches no file, network, clock or other app. The
  grant implies no other capability. When 1.7.0 was cut, OctoSense served
  `wasm` only in builds with its `wasm-lab` feature, and no release had it.

## 1.6.0

- Adds signed `host_api` requirements with exact ABI-major versions, implemented
  method descriptions, and the introspection-only `runtime` capability.
- Adds public, app-owned `backend` declarations: one HTTPS origin, fixed public
  client metadata, and a bounded named operation map. They contain no
  credentials.
- Adds the `host-api-v1`, `backend-api-v1` and `script-tools-v1` requirement
  markers. Older hosts reject the markers; newer hosts must also check that
  required methods and runtime policy/dispatch ABIs exist before installation.
- Old manifests preserve their serialized defaults and signature payloads.
  A declaration never grants OS access, authorizes a write or implements a service.

## 1.5.0

- `KNOWN_CAPABILITIES` gains the `auth`, `github`, `gcalendar` and `gmail`
  host-service capabilities. Each remains an independent grant. OAuth returns
  app-bound connection handles; provider tokens stay in the host. These
  declarations require corresponding OctoSense implementations and do not make
  a provider available by themselves.
- `KNOWN_CAPABILITIES` gains `photos` and `youtube`, so the contained Photos and
  YouTube UIs can call their owning host services. Neither grants Android
  Gallery access, a YouTube account or another app's agent tools; cross-app
  tools remain separate grants. When 1.5.0 was cut, OctoSense served neither
  family beyond its notice service, which answers only `photos.notify` and
  `youtube.notify` for its own system apps;
  [OctoSense#346](https://github.com/OctoSense-org/OctoSense/pull/346) tracks
  the services.
- The crate gains `portable_path`: a bundle-relative path written with `/` on
  every platform. `digest_dir` hashes these names, so a bundle with folders
  gets the same digest on Windows, Linux and macOS. Linux and macOS digests do
  not change, because the files keep their path-component order. Before 1.5.0,
  a Windows host hashed `\`, so its digests matched no other host's. A path
  that is not a plain UTF-8 name is refused instead of hashed lossily; the
  gate already refused such bundles.

## 1.4.0

- `KNOWN_CAPABILITIES` gains `calendar`, so a contained Calendar UI can call
  its owning host service. Agent tool sharing remains a separate grant. The
  capability grants no access to Android or Google Calendar accounts; the
  registered host service checks its owning app identity on every call.

## 1.3.0

- `RESERVED_NAMES` gains `octoscode`, reserved for OctosCode
  (octos-org/octoscode-app), a native app OctoSense planned for desktops and
  phones ([OctoSense#341](https://github.com/OctoSense-org/OctoSense/pull/341)).
  A store app with that id, or with it as its namespace
  (`com.example.octoscode`), is now refused as reserved. Nothing else changes:
  every other manifest 1.2.0 admitted is admitted unchanged.

## 1.2.0

Adds the operations of the Hagency/Rinx mini-app that administers Palpo. The
release is additive only: the schema stays at minor 0, and every manifest
1.1.0 admitted is admitted unchanged.

- `KNOWN_CAPABILITIES` gains 29 exact `palpo.*` operations:
  `palpo.intent.new`; `palpo.session.open` and `.disconnect`;
  `palpo.catalog.list`; `palpo.projects.list` and `.create`;
  `palpo.requests.list` and `.create`; `palpo.fleets.list`, `.register`,
  `.install`, `.set_state`, `.migrate`, `.queue`, `.export` and
  `.connect`; `palpo.agents.list`, `.register`, `.rename` and `.retire`;
  `palpo.activity.list`; `palpo.accounts.list`; `palpo.inbox.list`,
  `.submit`, `.get`, `.decide`, `.activate`, `.seen` and `.snooze`.
  Each is a grant of its own and implies no other. None changes Matrix
  admin, owner or membership authority, and credentials stay the host's.
- `palpo::SERVICES` lists them, and `palpo::words` gives each one's
  plain-language line for the consent sheet.
- `KNOWN_FEATURES` gains `palpo-admin-v1`; an app that uses these operations
  lists it in `requires`.

## 1.1.0

`RESERVED_NAMES` gains the native apps OctoSense shipped at the time (its
`native-apps.json`). The rule is unchanged: a store app whose id or namespace
is on the list is refused.

- `RESERVED_NAMES` gains `calculator`, `clock`, `notes`, `reminders` and
  `weather`: OctoSense shipped those Makepad apps as native apps and granted
  their read tools (`notes.search`, …) to its system agent by name. A store
  app with one of those ids, or one as its namespace (`com.example.notes`),
  is now refused as reserved. The tests' example app is `forecast` (it was
  `weather`).
- `RESERVED_NAMES` gains `browser` and `task`: OctoSense's desktop-only
  native Browser and Task Manager.

## 1.0.0

The contract as OctoSense ADR 0005 section 1 defines it, moved out of App
Hub's `octosense-app-policy` with its behavior unchanged:

- Manifest: `AppManifest` and its parts, `SCHEMA`, `MANIFEST_FILE`, `parse`.
- Policy: `policy::resolve`, `HostLimits`, `AppPolicy` (what the app may
  do: capabilities, hosts, storage, budgets, research scope). The app's
  agent is resolved by hosts that run agents, not here.
- Integrity: `digest_dir`, `bundle_digest`, `admit`, `admit_digest`,
  `SignatureVerifier`, `RefuseAllSignatures`.
- Running a package: `SCRIPT_ENTRY`, `script_source`, `ASSETS_PLACEHOLDER`,
  `AssetServer`, `StaticAssets`, `rewrite_assets`.

New in the contract:

- `requires` and `schema_minor` in the manifest, `KNOWN_FEATURES` (empty)
  and `SCHEMA_MINOR` (0): a manifest requiring an unknown feature is refused
  ("needs a newer host"); a manifest for a newer `1.x` is read with its
  unknown (optional) fields ignored and listed by
  `AppManifest::ignored_fields`.
- Every public struct and enum is `#[non_exhaustive]` (except the unit
  marker `RefuseAllSignatures`); `HostLimits` gains `with_*` builders and
  `Signature` gains `new`.
- `AppPolicy` carries the whole storage block (`StorageGrant`) and
  serializes.
- The fixture corpus (`tests/fixtures/`).
