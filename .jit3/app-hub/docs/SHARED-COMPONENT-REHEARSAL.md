# Prepare components and consuming apps together

English | [简体中文](SHARED-COMPONENT-REHEARSAL.zh-CN.md)

`shared_component_candidate` prepares a reviewable catalog transaction that
adds shared components and their consuming apps together. The ordinary
`publisher-entry` command resolves pins against an authenticated existing
catalog; this helper also resolves components newly added in the candidate.

Download real GitHub-attested releases into one directory. Include each
`<id>-<version>.component.json`, its matching `<id>-<version>.wasm`, and each
consumer's `*.bundle.pack.json`. Start from the current authentic
`catalog-v2.json`. No credentials or signing keys are used locally.

```sh
cargo run --locked -p octosense-app-hub --example shared_component_candidate -- \
  catalog-v2.json downloaded-releases new-candidate
```

The helper verifies the real admin base proof, verifies every component and
app publisher proof, resolves the exact pins, checks ownership continuity,
and runs the same final validation as `hub catalog-prepare`. It writes
`catalog.json`, all required artifact and index files, the canonical
`catalog-v2.payload.json`, and a receipt containing its SHA-256. It never
executes app or component code, authenticates its own output, or publishes.
Existing output directories are refused. Failures remove only a directory
newly created by that invocation.

Review the complete candidate and follow
[GitHub-admin publication](GITHUB-PUBLISHING.md). The protected workflow's
`dry_run: true` produces a real verified catalog envelope without changing
the public catalog. Use that envelope and the exact candidate artifacts for
isolated local installation; never substitute a fake proof or legacy test
catalog when claiming this end-to-end flow.

The example's three refusal/cleanup tests use the repository's genuine public
base proof. A successful real-proof rehearsal used
[the synthetic v0.1.0 release](https://github.com/ymote/octosense-component-demo/releases/tag/v0.1.0):
[publisher workflow 38024551136](https://github.com/ymote/octosense-component-demo/actions/runs/38024551136)
built and verified two components and two consumers. The helper verified the
sequence-15 public base and prepared the combined sequence-16 candidate in
[#195](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/195), with payload
SHA-256 `87bde245807a5ff6a1b3297c409d4ef6684414e47b038519a196feab29f42a7e`.
Admin attestation and native installation/execution need their own receipts;
this preparation result does not establish them.
