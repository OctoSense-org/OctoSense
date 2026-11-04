# ADR 0003: Shared components

English | [简体中文](0003-shared-components.zh-CN.md)

Status: proposed, 9 October 2026. This is phase 4 of
[OctoSense ADR 0014](https://github.com/OctoSense-org/OctoSense/pull/436)
(proposed) on App Hub's side, implemented in App Hub on top of
[#186](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/186) and
[#188](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/188), which
admit an app's own WebAssembly components. No catalog offers a component, and
a compatible host release is still required; source support is not release acceptance.

## Current source policy: declarations and authorization

`capabilities` and `network.hosts` describe an app's expected API and network
use for users and reviewers. Omitting a family or destination does not deny
an otherwise supported public API. This is the source policy being prepared
for the next compatible release; historical RC2 behavior is not evidence of
this change being deployed.

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


## Context

ADR 0014 lets an app ship an ordinary Rust crate as a WebAssembly component in
its bundle's `fns/`. Its phase 4 asks for **shared components**: reviewed,
versioned components in App Hub's catalog that several apps use, as npm
packages are shared. The installer verifies them, and every app still runs
its own instance under its own isolation, quotas and actual consent.

These constraints apply:

- **Code changes only with an app update.** A reviewer approves an app
  together with the code it runs. A shared component must never change under
  an installed app: no version ranges, and no file that differs from the one
  reviewed.
- **Review as for an app.** A component reaches the clock, files or the
  network through the app that uses it, so App Hub reviews it, validates it as
  it validates an app's own component (allowed imports only), and accepts only
  GitHub-attested releases ([ADR 0002](0002-github-attested-publisher-identity.md)).
- **One catalog.** Hosts accept one attested catalog document, with one
  sequence and one proof ([ADR 0001](0001-github-attested-catalog.md)). A
  withdrawal must reach a device as fast as an app's does.
- **Hosts read the catalog strictly.** `Catalog`, `Entry` and the app manifest
  refuse unknown fields, and every host checks each entry's `requires` against
  the features it knows (`CatalogPublishers::from_catalog`). A host refuses
  the whole catalog when one entry uses something it does not know. This
  already holds for an app that requires `wasm-components-v1`.

## Decision

### 1. A component entry next to the apps

The catalog gets a `components` list beside `entries`. Each item is one
component version:

```json
{
  "component": {
    "schema": 1, "id": "org.example.markdown", "version": "1.2.0", "name": "Markdown",
    "wasm_blake3": "<64 hex>", "bytes": 320538,
    "imports": ["wasi:clocks/wall-clock@0.2.12"],
    "exports": [{"name": "to-html", "params": [["markdown", "string"]], "result": "string"}],
    "publisher": {"name": "…", "support": "…", "privacy_policy_url": "https://…"},
    "license": "MIT OR Apache-2.0",
    "integrity": {"github": {"repository": "…", "repository_id": "…", "owner_id": "…",
      "workflow": "…", "tag": "v1.2.0", "commit": "…", "attestation": {}}}
  },
  "listing": {"subtitle": "…", "description": "…", "keywords": []},
  "artifact": "artifacts/org.example.markdown-1.2.0.wasm",
  "publisher": "github:<repository_id>",
  "source": {"repository": "https://github.com/…", "commit": "<40 hex>"},
  "status": {"state": "offered"},
  "admitted": "2026-10-09"
}
```

`component` and `listing` are what the publisher releases and the GitHub
proof covers. `artifact`, `publisher`, `source`, `status` and `admitted` are
the Hub's record, as in an app's entry. The list is left out when it is empty.

We chose this as the least invasive fit:

- A catalog without components serializes, signs and attests byte for byte
  as before. The tests re-serialize the published `catalog.json` and the
  attested `catalog-v2.json` payload and compare the bytes.
- `entries` stays a list of apps, so nothing that reads apps changes: the
  store's listings and search, publisher continuity, the v2 history rules,
  and the `Entry` struct that OctoSense builds in its tests.
- One document still carries everything, with one sequence and one proof. A
  component's withdrawal travels in the same verified catalog that installs
  resolve from.

The alternatives fit worse. Component entries shaped like apps inside
`entries` would appear as apps in older stores, and every app code path would
need a component branch. A separate attested document would need a second
subject, envelope and sequence in the publication workflow, and a device could
then hold two documents that disagree about a component. Neither alternative
helps older hosts: they refuse a catalog with an app that pins a component, as
they refuse any entry with an unknown feature.

### 2. A component release

A release is the `.wasm` file and `<id>-<version>.component.json`, which holds
`component` and `listing`. The publisher writes the id, version, name,
publisher, licence and listing. `hub component-prepare` derives
`wasm_blake3`, `bytes`, `imports` and `exports` from the file. With the
GitHub identity, it writes the attested subject,
`octosense-component.json`: the canonical release (sorted keys, no
whitespace) without the proof. The subject carries the file's digest, so the
proof binds the file, as an app's canonical manifest binds its bundle.
`hub component-pack` attaches the proof, verifies everything and writes the
two release files.

### 3. Review

`hub component-check`, and the catalog preparation that publishes a
component, run the same gate. It refuses the release when:

- the file is not a valid component, or imports anything outside `wasi:cli`,
  `wasi:clocks`, `wasi:filesystem`, `wasi:http`, `wasi:io`, `wasi:random`, `wasi:sockets` and
  `octosense:host`, as for an app's own component;
- the file's BLAKE3 digest, size, imports or exports differ from the release;
- the id breaks the app id rules, starts with `os.`, or is an app's id in the
  catalog;
- the version is not one exact semantic version, is already published, or is
  not higher than the newest on record;
- the name, publisher, licence or listing breaks its rule;
- the release has no GitHub provenance, or its proof fails. A development
  check may allow an unsigned release with `--allow-unsigned`.

An update must come from the same repository, owner and workflow as the
earlier versions. Reviewers also see what the component reaches, for
example: `org.example.markdown 1.0.0 is a component that reaches the clock,
random numbers and files in its app folder, but no network or other app`.

Socket imports now use the extensible host API requirement `wasm.sockets@1`.
An app pinning such a component must require it, and the store checks actual
host support before installation and at launch. This does not add a manifest
feature that would make RC5 reject the entire catalog; see the current
[publishing rules](../PUBLISHING.md#shared-components). Socket support is not
in RC5.

### 4. The app contract

The manifest gets the optional field `components`, which requires the
contract feature `wasm-shared-components-v1`:

```json
"components": [{"as": "markdown", "id": "org.example.markdown", "version": "1.2.0", "blake3": "<64 hex>"}]
```

`as` is the app's name for the component: `[a-z][a-z0-9_]{0,31}`, unique.
An app names at most 8 components, each at one exact version **and** digest.
The parser refuses the field without the required host feature. The field is left out when it is empty, so existing manifests and
their signing bytes do not change. A host that does not know the feature
refuses the app.

### 5. Admitting an app that pins components

With a catalog, the gate resolves each component: it must be there at that
exact version, be offered, and hash to the pinned digest. Supported imports
are independent of `storage`, `net` and `wasm` disclosures. Execution keeps
the app jail, quotas and actual host-service consent. Each component gets a reviewer
line, such as `component markdown (org.example.markdown 1.2.0) reaches the
clock, but no files, network or other app`.

A store app's bundle never carries `components/`: its components come from
the catalog. A shipped system app has no catalog. It carries each component it
pins at `components/<blake3>.wasm` in its bundle, and the system-app check
validates each file.

### 6. Publication and withdrawal

Components use the GitHub-admin publication flow of ADR 0001. A candidate adds
`artifacts/<id>-<version>.wasm` and `index/components/<id>-<version>.json`.
`hub catalog-prepare` keeps the component history append-only by the same
rules as for apps: entries keep their order and bytes, and may only change
from offered to withdrawn, with a reason. It runs the gate on each new
version, with GitHub provenance required, and compares the entry the gate
rebuilds with the candidate's entry and index. New apps in the same candidate
may pin new components in it. The receipt lists `{component, index}` for each
new component version.

### 7. On the device

- **Install and update.** Before the app, the store resolves each component
  from the same verified catalog and fetches its artifact, as it fetches the
  app's bundle. It checks the size, the digest and that the file is a valid
  component. It keeps each digest once, read-only, at
  `<apps root>/.components/<blake3>.wasm`. `Store::install_staged` refuses an
  app whose components are not there.
- **Launch.** `Store::may_run` and `prepare_launch` check the app's
  components with its bundle. A missing, changed or withdrawn component stops
  the app, as a changed bundle or a withdrawn app version does.
- **Load.** A host loads them with
  `octosense_appstore::components::resolved(app_id)`. It returns each
  component's alias, id, version, digest and verified path. A system app's
  come from its bundle; an installed app's from the store, resolved against
  the cached verified catalog. It hashes every file on every call, with no
  verified-on-write marker to trust, and never fetches.
- **Collection.** An uninstall (`Store::remove`), and each install or
  update through App Hub's store, remove every stored component that no
  installed app pins.

## Consequences

- **Older hosts.** A host that predates this decision refuses the whole
  catalog once it holds a component, or an app that pins one. That includes
  OctoSense desktop RC1 and RC2, and OctoSense `main` until it moves its App
  Hub pin. App Hub must not publish either until a compatible host release
  ships. The same already holds for an app that requires `wasm-components-v1`.
- **Withdrawal.** Withdrawing a component stops every installed app that pins
  it, until each app's publisher releases an update with another version.
- **Disk.** A component is stored once, however many apps pin it, and is
  removed when none does.
- **Building consumers.** A consumer that takes App Hub from git needs the
  unreleased contract until a contract release includes `components`
  ([Versions on crates.io](../../crates/app-contract/README.md#versions-on-cratesio)).
- **Cost.** A host hashes at most 8 components of at most 8 MiB on each
  launch check and each load.

## Open items and unverified parts

- **Unverified:** no GitHub workflow has attested a component release, and App
  Flow has no workflow template for one. The proof checks use the app proof's
  verifier with the component subject. Tests cover their refusals, but no
  production proof has passed them. Catalog preparation of a new component is
  tested only with the proof check turned off.
- **Unverified:** no OctoSense build calls
  `octosense_appstore::components::resolved` or loads a shared component.
  Running an instance per app is OctoSense's part.
- **Runtime support.** A host that knows the feature but whose `wasm` service
  cannot load shared components would still accept the app. OctoSense must
  ship the loader in the same change that takes this App Hub version, or a
  host API marker must be added.
- **Concurrency.** Collection is safe when installs and collection run in
  order, as in the shell's App Hub, which holds its catalog lock for both. Two
  processes that install into one apps root at the same time could collect a
  component that an unfinished install wrote. The next launch then refuses the
  app with an instruction to reinstall it.
- **Other uninstall paths.** A host that removes an app without
  `Store::remove` must call `octosense_app_hub::components::collect`
  afterwards, or the component stays on disk.
- **Store UI.** The store does not list components on their own. An app's
  permission lines name the components it pins.
- **Devices.** No phone or desktop host has installed an app that pins a
  component.

## Alternatives considered

- **Version ranges, as npm resolves them.** Refused: a range lets an
  installed app's code change without an app update.
- **Copying the component into each app's bundle.** An app can already do
  this with its own `fns/`. Nothing is shared: no single reviewed artifact,
  no single withdrawal, and one copy per app on disk.
- **A separate attested component catalog.** See section 1.
- **Pinning by digest only.** A person or reviewer could not tell what an app
  pins, and withdrawal is decided per id and version.
- **Grants of a component's own.** Refused: a component reaches only what the
  app that uses it may reach.

## References

- [OctoSense ADR 0014](https://github.com/OctoSense-org/OctoSense/pull/436)
- [App Hub #186](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/186) and [#188](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/188)
- [Shared components](../PUBLISHING.md#shared-components) in the publishing reference
- [ADR 0001](0001-github-attested-catalog.md) and [ADR 0002](0002-github-attested-publisher-identity.md)
