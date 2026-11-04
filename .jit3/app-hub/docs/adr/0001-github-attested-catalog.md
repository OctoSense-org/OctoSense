# ADR 0001: GitHub-admin catalog attestations

English | [简体中文](0001-github-attested-catalog.zh-CN.md)

Status: accepted, implemented and in production. App Hub #150 implemented
this decision. The protected workflow published catalog sequence 11
([run 37736098082](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/37736098082))
and every later catalog through sequence 15. OctoSense desktop 0.1.0-rc.1
is a compatible host release and selects the `catalog-v2.json` channel by
default.

## Context

Administrators want to approve catalog publication through their GitHub identity
without maintaining another private signing key. The existing catalog uses an
offline Ed25519 anchor, a certified working key, and publisher signatures. Those
are different responsibilities: catalog authorization decides what the Hub offers;
publisher continuity identifies the owner of an app across updates.

GitHub artifact attestations use short-lived Sigstore certificates. For this
public repository they include public transparency-log evidence. An attestation
proves origin and bytes; it does not establish that an app passed admission or has
good UX. Those checks remain part of the reviewed publication procedure.

## Decision

Add a separate `catalog-v2.json` channel with an explicit envelope:

```json
{
  "schema": 2,
  "kind": "github-attested-catalog",
  "catalog": "<base64 of the exact catalog-v2.payload.json bytes>",
  "attestation": { "<Sigstore v0.3 bundle>": "..." }
}
```

The inner catalog retains the entry schema, with no legacy catalog key or
signature. Base64 preserves its exact attested bytes. This illustrative JSON is
not a valid proof. The existing `catalog.json`, its anchor, old entries, and
publisher signatures are not rewritten or replaced.

[`github_catalog`](../../crates/app-hub/src/github_catalog.rs) uses the maintained
`sigstore-verify` 0.14.0 implementation from `sigstore/sigstore-rust`, with default
network features disabled. Verification uses the public-good trust snapshot
compiled into the host. It checks the certificate chain, certificate transparency,
signature, authenticated signing time, transparency-log inclusion and checkpoint,
and issuer/identity policy. No phone-side `gh` process or custom certificate
verifier is involved. Missing roots, unsupported proofs and failed checks refuse
the catalog; they do not trigger a weaker verification mode.

The host additionally requires the authenticated claims for:

- issuer `https://token.actions.githubusercontent.com`;
- `OctoSense-org/OctoSense-App-Hub`, immutable repository ID `1378439410` and
  owner ID `328148893`;
- `.github/workflows/publish-catalog.yml@refs/heads/main`, in both the certificate
  identity and workflow claims;
- `workflow_dispatch`, public repository visibility, a GitHub-hosted runner,
  and deployment environment `app-hub-catalog`.

The signed statement must be in-toto v1 / SLSA provenance v1 with exactly one
subject named `catalog-v2.payload.json`, its exact SHA-256, and the same workflow,
repository, branch, event and immutable IDs in GitHub's workflow/v1 predicate.
The limits are 8 MiB of catalog bytes, 2 MiB of proof, 14 MiB for the envelope,
and 4,096 entries. These bounds do not grant an app any host permission.

## Publication boundary

The admin workflow runs its trusted implementation from `main`. It verifies
the dispatch actor's and rerun actor's administrator permissions through GitHub,
and requires the protected environment before requesting an OIDC identity.
It reads the approved immutable candidate commit and digest as data; it never
runs that candidate's workflows, build scripts, or app code. Publication is
serialized and refuses a stale main/base catalog before a non-force update.

Before enabling publication, maintainers must restrict environment approval to
current administrators and disable environment protection bypass. The administrator
must review the exact workflow commit, candidate digest and publication receipt;
approval cannot rely only on the workflow's name. The dispatching administrator
may approve their own publication; a second person is not required. The
certificate binds the workflow path and environment, not a specific reviewed
implementation commit or the approver's role. The protected environment and this
review are therefore the authorization boundary. Branch protection and mandatory
workflow review are recommended governance controls; mandatory pull-request
review would also require adapting the direct non-force publication transaction.

The native `hub` commands are the workflow's validation boundary:

| Command | Responsibility |
| --- | --- |
| `catalog-prepare` | Authenticate the existing legacy/v2 base, require sequence +1 and today's date, preserve old history, admit each new signed bundle, and compare its pack/index. Emit exact prepared bytes and a JSON receipt binding base, candidate and output hashes. |
| `catalog-envelope` | Verify the production GitHub proof over those bytes and write the explicit v2 document. It cannot sign or bypass verification. |
| `catalog-verify` | Independently verify a v2 document with the same native policy as a device. |

Existing entries can only change from offered to withdrawn with a reason.
Re-offering, deletion, altered manifests, changed publisher keys, mismatching
pack/index bytes and mutable source refs fail. The commands never replace a
publisher signature with a catalog attestation.
[ADR 0002](0002-github-attested-publisher-identity.md) defines GitHub-attested
publisher identity; key-signed apps do not migrate to it.

## Client and cache boundary

[`Store::with_github_catalog`](../../crates/app-hub/src/client.rs) is a host-selected
channel requirement. It rejects legacy, unsigned and invalid v2 documents from
the first call, before any successful v2 fetch. Legacy stores retain their
existing anchor verification. A downloaded document cannot choose the trust
mode, and a failed v2 request must not retry the legacy channel.

`accept_catalog_and_cache` takes a nonblocking cache lock, verifies the persisted
catalog before the incoming one, and atomically replaces the complete document
before updating memory. Proof, payload and sequence stay together. A lower
sequence or different contents at the same sequence is refused. The caller must
use separate legacy/v2 cache paths and run verification outside its UI event
handler. The current 14-day new-install freshness rule is unchanged.
`for_install_root` clones this authenticated state for staged installs without
changing the selected channel, admitted catalog, host limits or API versions.

New hosts can ship the v2 verifier and trust policy before publication. A
synthetic isolated-channel attestation then tests this exact policy without
changing the public catalog. Only after native and phone acceptance should a
compatible release select the v2 channel. Old clients require that host update;
keyless proof cannot be converted into their Ed25519 trust chain without the old
private key. The old catalog's freshness limit still applies.

## Validation and limits

The local tests verify a real public upstream Sigstore fixture, then tamper with
bytes, signatures and transparency evidence. They also test wrong identities,
roots and authority validity windows, exact subject/workflow policy, actual signed
bundle admission, publisher history, sequence rollback, cache contention and
failed-write behavior. The upstream fixture is **not** a production App Hub
attestation. Cache policy tests isolate the post-verification state machine; they
do not substitute for an actual GitHub signing run.

Production publication runs with live protected-environment approval and
production-format App Hub attestations. Command-line checks of the
production proof passed on macOS and on an Android phone
([evidence](../GITHUB-PUBLISHING.md#validation)). A phone Store reading
`catalog-v2.json` and any iOS host remain **unverified**. Trust-root rotations
require a compatible host update; the verifier does not fetch trust material
from the catalog or silently enable online TUF. These limits are not reasons
to accept unsigned catalogs.

## References

- [GitHub artifact attestations](https://docs.github.com/en/actions/concepts/security/artifact-attestations)
- [Official actions/attest](https://github.com/actions/attest)
- [GitHub provenance generator](https://github.com/actions/toolkit/blob/main/packages/attest/src/provenance.ts)
- [sigstore-verify 0.14.0](https://docs.rs/crate/sigstore-verify/0.14.0)
- [Existing App Hub trust design](https://github.com/OctoSense-org/OctoSense/blob/main/docs/adr/home/0003-app-hub-and-store.md)
