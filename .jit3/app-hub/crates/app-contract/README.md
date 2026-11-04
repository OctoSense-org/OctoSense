# octosense-app-contract

The OctoSense app contract: the one small, versioned interface between App
Hub and every app ([OctoSense ADR 0005](https://github.com/OctoSense-org/OctoSense/blob/main/docs/adr/0005-app-contract.md)).
It holds only what an app or a host needs to read, check and run an app
package:

| Part | Items |
| --- | --- |
| Manifest | `AppManifest` and its parts, `SCHEMA`, `MANIFEST_FILE`, `parse` |
| Policy | `policy::resolve`, `HostLimits`, `AppPolicy` (capabilities, network hosts, the storage block as `StorageGrant`, budgets, research scope) |
| Integrity | `digest_dir`, `bundle_digest`, `admit`, `admit_digest`, `SignatureVerifier`, `RefuseAllSignatures` |
| Running a package | `SCRIPT_ENTRY`, `script_source`, `ASSETS_PLACEHOLDER`, `AssetServer`, `StaticAssets`, `rewrite_assets` |

The contract describes app metadata, required ABIs and resource limits;
the host enforces isolation and actual authorization. Capability names and
network hosts in `AppPolicy` are usage disclosures, not execution grants.
Keep app/account identity, bounded storage, device consent, provider scopes,
trusted write reviews and inter-app sharing checks.
App Hub's store, catalog, agents, listings and host services stay in
`octosense-app-policy` and `octosense-app-hub`, which depend on this crate.

```toml
[dependencies]
octosense-app-contract = "1"
```

```rust
use octosense_app_contract::{admit_digest, digest_dir, parse, policy, HostLimits, RefuseAllSignatures, MANIFEST_FILE};

fn open(package: &std::path::Path) -> Result<octosense_app_contract::AppPolicy, String> {
    let manifest = parse(&std::fs::read_to_string(package.join(MANIFEST_FILE)).map_err(|e| e.to_string())?)?;
    admit_digest(&manifest, &digest_dir(package)?, &RefuseAllSignatures)?;
    policy::resolve(&manifest, &HostLimits::default().with_require_signature(false))
}
```

## Host API declarations (1.6)

`host_api::HostApiRequirements` records required and optional exact ABI-major
versions. `AppManifest::check_host_apis` compares required methods and feature
ABIs with the host's implemented inventory. Hosts must perform this check at
install and launch; admission alone does not prove runtime support.

`backend::BackendRegistration` describes an app's public HTTPS login and named
business operations. Credentials remain in the host. Backend and device access
still require actual authorization, supported native adapters and the person's
consent. See [Host API compatibility](../../docs/HOST-API.md) for examples and
limits.

## Versions on crates.io

**[1.11.0 is published on crates.io](https://crates.io/crates/octosense-app-contract/1.11.0)**
through the [release workflow](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/38026012863)
from commit `b7e4ba8f2f1ccc7606fc8815256f0bf123c43c57`.
The registry checksum and independently downloaded archive SHA-256 both match
`57d8c87511e17c5a8f965cbdba07a1cae100d78316159827c3a2061c84003bf6`.
It adds component/shared-component ABI metadata and the declaration-only
capability policy ([CHANGELOG.md](CHANGELOG.md)). App Policy requires 1.11.0;
consumers can now resolve it from crates.io. Capabilities describe usage.
Actual user consent, account scopes, integrity, quotas and API availability
remain host checks. Publishing this crate does not update a host binary;
component apps still need a compatible OctoSense release.

[1.10.0](https://crates.io/crates/octosense-app-contract/1.10.0)
([publication receipt](../../reviews/public-os-api-v1/contract-1.10.0-publication.json))
included the unpublished 1.9.0 work's `files`
capability, the distinct `device_calendar` capability and separate `audio`
playback grant. Audio implies no recording, storage or background authority. `files` provides
host-owned import/export dialogs, separate from `storage`; it grants no
arbitrary filesystem access. `device_calendar` is separate from `calendar`
and `gcalendar`. These declarations do not install a host adapter or grant OS
access: declare required method versions and check the actual host. App Policy
at that release required contract 1.10, which consumers can resolve from
crates.io without a git patch. Released hosts do not gain these APIs by parsing
new capabilities.

crates.io has 1.0.0, 1.1.0, 1.2.0, 1.5.0, 1.6.0, 1.7.0, 1.7.1, 1.8.0,
1.10.0 and 1.11.0.
Versions 1.3.0, 1.4.0 and 1.9.0 exist only in this repository
([CHANGELOG.md](CHANGELOG.md)); 1.5.0 includes the first two versions' changes,
and 1.10.0 includes 1.9.0's changes. A lock file that still holds 1.2.0 refuses
every capability added since, such as `auth`:

```text
app org.example.connect requests unknown capability "auth"
```

[1.8.0](https://crates.io/crates/octosense-app-contract/1.8.0)
([publication receipt](../../reviews/github-publisher-v1/contract-1.8.0-publication.json)) added optional
`integrity.github`, the required `publisher-github-v1` marker and a
`SignatureVerifier::verify_github` extension that refuses by default. Existing
manifests and Ed25519 signing bytes remain unchanged. Hosts must supply a real
provenance verifier; accepting the new marker alone does not authenticate an
app. These additions remain available in 1.11.0.
The contract release does not deliver a compatible OctoSense shell binary.

Published 1.7.1 reserves exactly four full app IDs, ignoring ASCII case:
`catalog.json`, `catalog.lock`, `catalog-v2.json` and `catalog-v2.lock`.
`json` and `lock` are not reserved namespaces; `org.example.json` and
`org.example.lock` remain valid.

`cargo update -p octosense-app-contract` moves an unconstrained 1.x consumer
to 1.11.0. Hosts using API declarations must select 1.6 or later,
hosts serving `wasm` 1.7 or later, and hosts admitting `files`,
`device_calendar` or `audio` a published version of at least 1.10. Hosts using
component schemas and the corrected declaration-only policy must select 1.11
or later; older checked-in lock files remain on their existing version. OctoSense
desktop 0.1.0-beta.2 patched the crate to an App Hub revision. To build
against a contract newer than the latest release, patch crates.io's copy with
an App Hub revision, then update the lock file:

```toml
[patch.crates-io]
octosense-app-contract = { git = "https://github.com/OctoSense-org/OctoSense-App-Hub", rev = "<App Hub commit>" }
```

```sh
cargo update -p octosense-app-contract
```

Without `cargo update`, a lock file that already holds 1.2.0 keeps it, and
Cargo warns that the patch `was not used in the crate graph`.

## Stability

Within `1.x` the contract only grows (ADR 0005 section 2):

- **Additive only.** A minor release adds only new types, functions, optional
  manifest fields and enum variants. Nothing is removed or renamed, and no
  existing field, default or rule changes meaning. Anything else is `2.0`,
  decided in an ADR.
- **Every public struct and enum is `#[non_exhaustive]`** (except the unit
  marker `RefuseAllSignatures`), so adding a field or variant is a minor
  change. Build values with the provided constructors instead of struct
  literals, and match enums with a `_` arm:

  ```rust
  use octosense_app_contract::{HostLimits, Signature};
  let limits = HostLimits::default().with_require_signature(false);
  let signature = Signature::new("release", "aabb");
  ```

  Get manifests from `parse` and policies from `policy::resolve`.
- **Unknown manifest fields are classified, never silently dropped.** A field added in
  `1.x` is *optional* (a host may run the app without it: it only adds
  information or asks for less) or *required* (it restricts or changes
  what the app gets). A manifest that uses a required field lists its
  feature in `requires`. `parse` applies, in order:

  1. `schema` must be `1`, for the whole `1.x` line.
  2. Every `requires` entry must be in `KNOWN_FEATURES`, at every
     `schema_minor`. Otherwise: `app <id> needs a newer host: <feature>`.
     `1.0.0` knows no features.
  3. If `schema_minor` (default 0) is at most `SCHEMA_MINOR` (0 before
     1.12.0; 1 from 1.12.0, which adds the optional `window`), `parse` reads
     the manifest strictly and refuses it if any level has an unknown field.
  4. If `schema_minor` is above `SCHEMA_MINOR`, the manifest was written for
     a newer `1.x`. Rule 2 has passed, so every unknown field, at any level,
     is optional: `parse` ignores it, and `AppManifest::ignored_fields()`
     lists it (for example `network.retry`) for the host to log. Known
     fields are checked as always.

  ```json
  { "schema": 1, "schema_minor": 2, "requires": ["<feature>"], ... }
  ```

  An older host therefore never runs an app under weaker rules than its
  author wrote, and a newer optional field never breaks an older host.
  That holds only when the manifest declares the minor that added the
  field. `window`, the first field added at a new minor, needs
  `"schema_minor": 1`, and `parse` refuses it in a manifest that declares
  less.
  Ignored fields stay in the manifest's signing bytes, so a newer signed
  manifest still verifies. A field added in `1.x` must be omitted from
  serialization when it holds its default, and must serialize exactly as
  written.
- **Older signatures still verify.** `requires` and `schema_minor` are left
  out of the canonical signing bytes when empty, so a manifest signed before
  they existed produces the same signing bytes.
- **Security fixes may reject unsafe identities.** Version 1.7.1 closes the
  host-file collision for the four exact IDs listed above; it does not reserve
  additional tool namespaces or change valid apps' signature payloads.
- **Fixtures pin the behavior.** [`tests/fixtures/`](tests/fixtures/README.md)
  holds real manifests and packages with their digest, signing bytes,
  ignored fields and resolved `AppPolicy`. Every `1.x` must reproduce all
  of them; the corpus is append-only.

## Checks and releases

App Hub's CI (`.github/workflows/app-contract.yml`) tests this crate on its
own, as crates.io builds it: the unit tests, the fixture corpus, the doc
tests, clippy and a publish dry run. It also runs `cargo semver-checks`
against the latest version on crates.io, so a non-additive change fails the
pull request.

To release a version:

1. Bump `version` in this crate's `Cargo.toml`, add the release to
   [CHANGELOG.md](CHANGELOG.md), and update the crates.io status in this
   README ([Versions on crates.io](#versions-on-cratesio)) and in the
   changelog.
2. Have App Hub and the owner of one consuming app (Rinx) review the change.
3. Run the `publish-app-contract` workflow
   (`.github/workflows/publish-app-contract.yml`) by hand, or push the tag
   `app-contract-v<version>`. The workflow refuses a version that is already
   on crates.io, runs the tests, and publishes with the
   `CARGO_REGISTRY_TOKEN` secret.

## License

Apache-2.0.
