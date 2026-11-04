# Publish a catalog through GitHub

English | [简体中文](GITHUB-PUBLISHING.zh-CN.md)

An App Hub repository admin authorizes the exact catalog digest. GitHub
Actions signs those bytes with a short-lived identity certificate and records
the signature in a public transparency log. No admin stores a separate Hub
private key. This is GitHub-managed artifact signing, not an SSH signature or
the green Verified badge on a commit.

**Delivery status:** [production run 37736098082](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/37736098082)
published catalog sequence 11 at commit
`27eeec5b3abc3f9c2aa06a3894bab2f15228f116`. Later runs published sequences
12 to 15;
[run 37755718288](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/37755718288)
published sequence 13 at commit `3842c5ec503a8e9124cbbe99655556ffe24c41e1`, and
[run 37852340163](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/37852340163)
published sequence 14 at commit `14135444bb2ded83cee3458553287aa60387f795`,
withdrawing the six key-signed `org.octosense.samples.*` entries, and
[run 37902582113](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/37902582113)
published sequence 15 at commit `18cd41d91b326db199fbed4129484a9ba1a8c63d`,
adding GitHub Notes 0.2.2.
This source selects the GitHub channel by default for new compatible hosts.
OctoSense desktop 0.1.0-rc.1 (2026-10-08) and
[0.1.0-rc.2](../README.md#download-a-compatible-host) (2026-10-09) are
compatible host releases and read `catalog-v2.json` by default. Old installed hosts and reference-app bundle
bytes are unchanged. [Validation](#validation) records the sequence 11 proof
and its native consumer checks.

## Who authorizes publication

Only a current repository **admin** may dispatch or rerun
[`publish-catalog.yml`](../.github/workflows/publish-catalog.yml) on `main`.
The workflow checks the original actor's numeric identity and both actors'
current admin permissions. A writer or ordinary app publisher cannot sign a
catalog through this workflow.

Before enabling publication, configure the `app-hub-catalog` GitHub environment
to allow deployment only from `main`, require an App Hub admin as reviewer,
and disable protection bypass. Keep its reviewers aligned with current admin
membership. Protect workflow and publication-tool changes through repository
review rules. The native verifier requires the signed certificate's environment
claim; removing the environment from a modified workflow cannot bypass it.

Approval covers a complete commit, candidate name and prepared SHA-256, not
an app name or mutable branch. Admin review still includes the
[normal admission checks](SUBMITTING.md#8-what-reviewers-check).

## Prepare the reviewed candidate

Keep a candidate in a reviewable commit on `main`:

```text
catalog-candidates/<name>/catalog.json
catalog-candidates/<name>/artifacts/<app>-<version>.bundle/...
catalog-candidates/<name>/artifacts/<app>-<version>.bundle.pack.json
catalog-candidates/<name>/index/<app>-<version>.json
catalog-candidates/<name>/artifacts/<component>-<version>.wasm
catalog-candidates/<name>/index/components/<component>-<version>.json
```

The candidate `catalog.json` is an unsigned payload: schema 1, no `key` or
`signature`, sequence exactly one above the authenticated base, and today's
UTC publication date. Retain all prior entries. Existing entries may only
change from offered to withdrawn with a reason. Never remove history,
replace existing bytes or re-offer a withdrawn version. For a freshness-only
update, no new artifact files are needed.

For new versions, include the exact reviewed bundle, matching pack and index.
`hub catalog-prepare` re-runs admission and publisher continuity checks and
compares every derived entry and packed file.

A shared component ([ADR 0003](adr/0003-shared-components.md)) is recorded
in the catalog's `components` list, under the same history rules. For a new
component version, include its reviewed `.wasm` file and the index entry
that `hub component-entry` writes. `hub catalog-prepare` re-runs the
component gate with GitHub provenance required, compares the rebuilt entry
with the candidate's entry and index, and lists `{"component", "index"}` for
each new component version in its receipt. A new app in the same candidate
may pin a new component of that candidate. The workflow extracts candidate
files as data; it never executes their scripts or build commands.

After [preparing the native workspace](FIRST-APP.md#1-prepare-the-tools-and-an-app-repository):

```sh
cargo build --locked --release -p octosense-app-hub
# Use the current authenticated v2 catalog as the base.
target/release/hub catalog-prepare --base catalog-v2.json \
  --candidate catalog-candidates/<name>/catalog.json \
  --artifact-root catalog-candidates/<name> \
  --out /tmp/catalog-v2.payload.json
```

Record `payload_sha256` from the JSON receipt and the full 40-character
candidate commit. Review the resulting payload and all added files before
authorizing that digest. Re-prepare if the base changes or the UTC date rolls
over. The command itself is covered by native integration tests; the production
dispatch above is separate evidence for its reviewed candidate.

## Sign, verify and publish

Open **Actions → Publish catalog → Run workflow**, select `main`, then enter
the candidate name, full commit and exact `payload_sha256`. Start with
`dry_run: true`. Approve the protected environment using an admin account.

The first job checks the admin identity, builds trusted tooling, verifies the
candidate and binds the packet to the reviewed digest. The environment-gated
job signs `catalog-v2.payload.json` using GitHub OIDC and Sigstore, then runs
the native verifier over the returned proof. A dry run retains the verified
`catalog-v2.json` envelope and receipt as workflow artifacts without changing
the public catalog.

After that proof passes consumer acceptance, run the same reviewed transaction
with `dry_run: false`. The workflow commits only the envelope and its newly
admitted artifacts/index files. Its non-force push must advance the exact main
commit it checked. Concurrent changes stop publication; they are never
silently rebased or retried. A new dispatch requires review against the new
base. Legacy `catalog.json` is untouched.

## What OctoSense verifies

The native verifier checks the exact payload SHA-256, certificate chain,
certificate transparency proof, signed transparency-log entry and inclusion
proof, GitHub issuer, App Hub repository and owner numeric IDs, workflow path,
`main` ref, manual dispatch, GitHub-hosted runner and protected environment.
The signed statement must describe the exact catalog subject and approved
workflow. Network mirrors cannot substitute another repository or workflow.

New compatible hosts select `github-v2` by default; setting
`OCTOSENSE_HUB_CATALOG=github-v2` is optional. A custom legacy test hub requires
`OCTOSENSE_HUB_CATALOG=legacy` and a fresh library. Any existing v2 cache,
including a damaged one, makes an explicit legacy selection fail closed.

An old `catalog.json` cache is not converted into trusted v2 data offline.
The first v2 load needs a network fetch or an explicitly supplied v2 mirror.
It cannot use the legacy cache as a fallback. Reference-app bundle bytes and
publisher signatures remain unchanged.

The v2 client is selected by host configuration before fetching. It
refuses legacy or malformed documents instead of falling back. Cache the whole
envelope, including its proof, atomically; preserve the largest accepted
sequence across restarts. Different contents at the same sequence are also
refused. The existing 14-day freshness limit still gates new installations.

## Migration and app publishers

Older releases understand the anchor-signed `catalog.json` only. They need a
host update to consume `catalog-v2.json`; the two files and cache names remain
separate. A new default does not update an already installed host or convert
its offline legacy cache.

Catalog signing and app publisher proofs are different boundaries. New apps
use the [GitHub publisher workflow](PUBLISHING.md#signing), without a developer
key; routine updates retain the verified repository/owner/workflow identity.
Existing reference entries and signatures are preserved as historical releases.
Opening the submission issue remains the publication request, and administrator
approval still controls catalog publication.

## Validation

The [production receipt](evidence/catalog-v2-production-37736098082.json) records
an independent fetch of the immutable public envelope: 59,745 bytes, SHA-256
`90576462177341de69eae4737f1e36bd16efce5e474223c1f74182b4b150d5e2`, equal to
the Actions artifact. Its sequence is 11 and payload SHA-256 is
`d7a43c63ca3219691f0879b3637aef7088963c579812228a9afa82967437de0c`.

| Native consumer | Exact App Hub source | Result |
| --- | --- | --- |
| macOS CLI | `af0cf7f3c8e4e8eb91829bd8ac44219508ba7f0c` | 6/6 passed |
| Android arm64 CLI on OnePlus 6 | `6e3b8ffefff018269efb45f7d2c9b08fe75c4d74` | 6/6 passed |

Each accepted the real production proof and refused altered payload, altered
signature, altered transparency log, missing log, and a legacy document.
The receipt includes each binary hash and completed cleanup. These are
native proof-consumer checks at the recorded sources, not app installation,
GUI/account acceptance or a test of this later default-channel build.

Run `python3 -m unittest discover -s tools -p test_catalog_publish.py` for
admin authorization, input/path refusal, exact-packet binding and real Git
concurrent-update rejection. The PR workflow runs these without signing or
write permissions. Native proof, tamper, admission, rollback and cache tests
live in `crates/app-hub`. Keep production proof and device acceptance claims
separate from those automated checks.
