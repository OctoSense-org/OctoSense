# Repo Radar 0.1.0 catalog candidate

English | [简体中文](README.zh-CN.md)

**Draft candidate; production publication is blocked.** This directory does
not change the public catalog, install the app or approve publication.
[Submission #202](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/202)
requests `org.ymote.reporadar` 0.1.0 from
[ymote/octosense-repo-radar v0.1.0](https://github.com/ymote/octosense-repo-radar/releases/tag/v0.1.0),
commit `ec202f7d6c8a192a80353a1640c19786db3f77e1`.

## Exact reviewed bytes

[Publisher run 38088072936](https://github.com/ymote/octosense-repo-radar/actions/runs/38088072936)
built and attested the release. The downloaded pack is copied unchanged:

- Pack SHA-256: `54ed1fe4fc2d885defe5fe9f8a507f3ac29104629d9c10e4b0f597de35b63f83`.
- Bundle BLAKE3: `90463f61990f62f1610bbcd9e3be76b7ca4bd046c05c9f2c222cbd92267bf136`.
- Sealed Rust component SHA-256: `d0ee1835711ed4b1c952f564789d96c160d02035b1d2104db1388e6affdbc757`.

The native verifier accepted the authenticated sequence-15 base, publisher
proof, bundle gate, first-publisher identity, candidate entry and full prepared
catalog. `review.json` and `prepare-receipt.json` record those bindings.
No publisher key was created, and the sealed release was neither restamped nor
otherwise modified.

## Behavior and acceptance boundary

Repo Radar searches public GitHub, keeps a private repository watchlist,
shows bounded recent PR/release data with native D3 charts, and publishes a
project summary to Glance. Its optional assistant uses nine Rust-backed tools;
agent-triggered unpinning requires host approval. It needs no GitHub account.
The listing currently supports macOS only and requires `charts.d3@1`.

The development full-shell scenario passed 94 steps, including real public
HTTPS, chart rendering, pin/select/reorder, Glance publication and restart
persistence. **That run used the locally built component.** GitHub rebuilt
`fns/radar.wasm`; the sealed component above still needs execution acceptance.
The source and release differ only in that rebuilt component and the attested
manifest. RC4 correctly refuses this app because it lacks `charts.d3@1`.
A compatible public host, sealed-release execution and released-host install
acceptance are required before production. Live model replies, updates and
other platforms remain unverified.

## Candidate transaction

This is a separate sequence-16 proposal on authenticated public sequence 15,
not an edit to the existing shared-components sequence-16 rehearsal. The new
payload retains every prior entry unchanged. If another candidate publishes
first, or the UTC date changes, re-prepare this candidate against the new base;
never overwrite or silently reuse the other candidate's transaction.

Prepared `catalog-v2.payload.json` SHA-256:

```
83867e6f8197dff3d7fb630ba50a408a44322c53fa03c619a6fd01fcba0c91bf
```

After review and a separate merge decision, an App Hub admin can use the
protected workflow with candidate `ymote-repo-radar-0-1-0`, its exact reviewed
commit, this digest and **`dry_run: true`**. Candidate preparation is not a
signed catalog proof. Verify the resulting envelope and complete the consumer
checks before any production dispatch. See
[GitHub publication](../../docs/GITHUB-PUBLISHING.md).
