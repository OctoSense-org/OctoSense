# Shared-component installation rehearsal

English | [简体中文](README.zh-CN.md)

This candidate is **for `dry_run: true` only**. Merging it does not modify the
public catalog or add the synthetic apps to the public store.

The two components and two consumer apps come from
[ymote/octosense-component-demo v0.1.0](https://github.com/ymote/octosense-component-demo/releases/tag/v0.1.0),
commit `8faa66c3e4006d68dd4de16fba375688578814e2`.
[Publisher workflow 38024551136](https://github.com/ymote/octosense-component-demo/actions/runs/38024551136)
built the Rust components and verified all four genuine GitHub attestations.
It used App Hub `f0b9b22f521607d67c7e3fdc925a810c45d62a9b`.

The `shared_component_candidate` helper at `6264190` prepared this directory
from the unchanged public catalog (sequence 15). It verified the real base and
publisher proofs, both component pins, first-publisher identity continuity,
all artifacts and indexes, and the complete candidate payload. The first app
has an empty capability declaration; the second declares `wasm` and `storage`.
Both must receive app-private runtime state and files under the host's current
capability policy.

The exact admin-reviewed SHA-256 for `catalog-v2.payload.json` is:

```
87bde245807a5ff6a1b3297c409d4ef6684414e47b038519a196feab29f42a7e
```

`prepare-receipt.json` records all new paths and base/candidate hashes.
After review and merge, an App Hub admin may dispatch the protected
`publish-catalog.yml` workflow from main with this directory name, the full
reviewed commit, the digest above, and **`dry_run: true`**. Its verified envelope
can be combined with these exact artifacts for isolated Mac and OnePlus 6
installation/execution acceptance. Native execution and device acceptance are
**unverified until their separate receipts exist**. No personal data, account
credentials or developer signing key is included.
