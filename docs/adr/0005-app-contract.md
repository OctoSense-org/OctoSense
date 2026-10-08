# ADR 0005: The app contract: one small, versioned interface between App Hub and every app

- **Date:** 2026-09-30
- **Status:** Accepted (2026-09-30); **Implemented** (2026-10-01): `octosense-app-contract` 1.0.0 on crates.io (App Hub [#46](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/46), [#47](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/47)); Rinx 1.1.0 depends on it and on no App Hub commit (hagency-org/Rinx [#49](https://github.com/hagency-org/Rinx/pull/49), tag `v1.1.0`); OctoSense takes the contract from crates.io, pins Rinx by tag and has no App Hub host alias ([#243](https://github.com/OctoSense-org/OctoSense/pull/243)). Not yet exercised on a device: Rinx's own mini-app catalog and sandbox (they sit behind a Matrix sign-in). *(2026-10-06: OctoSense now takes the contract from an App Hub git revision until 1.5 reaches crates.io; see section 3.)* *(2026-10-07: 1.5.0 is on crates.io, and since [#356](https://github.com/OctoSense-org/OctoSense/pull/356) OctoSense takes the contract from crates.io again; see section 3.)*
- **Scope:** What an app (a mini app, a script app, or a native app that runs other apps, such as Rinx) may depend on from App Hub; how that interface is versioned and kept stable; how OctoSense and apps depend on it so that App Hub changes never force an app release.
- **Relates to:** [ADR 0002](0002-event-driven-app-agents.md) (app manifests and agents); [ADR 0004](0004-native-apps-hosting-and-peers.md) (native apps, storage contract); App Hub `crates/app-policy`; hagency-org/Rinx#37 (Rinx release tags, point 6: break the App Hub lockstep).

## Context

### What apps take from App Hub today

Rinx runs mini apps, and to do that it uses App Hub's own code: about 30 call sites in `src/miniapps/`, `crates/system-apps` and `crates/miniapp-catalog`. OctoSense's shell uses the same crate.

| What it does | App Hub code (`octosense-app-policy` unless noted) |
| --- | --- |
| Read a manifest | `AppManifest`, `SCHEMA` |
| Decide what an app may do | `policy::resolve`, `HostLimits`, `AppPolicy` |
| Check a package wasn't tampered with | `digest_dir`, `bundle_digest`, `admit_digest`, `RefuseAllSignatures`, `SignatureVerifier` |
| Run a mini app: entry script, assets, Splash settings | `SCRIPT_ENTRY`, `script_source`, `AssetServer`, `rewrite_assets`, `splash_adapter::apply` |
| The mini-app catalog | all of `octosense-app-hub`, re-exported by Rinx's `miniapp-catalog` as `hub` |

`octosense-app-policy` also carries what only App Hub and the shell need: agent policy, listings, research scopes, the host service registry, containers and session profiles. It is versioned `0.1.0` and changes whenever App Hub does.

### Why App Hub changes force Rinx releases

- Every consumer names App Hub by **git commit**. Cargo treats two commits of one git repository as two different crates, so a build that contains OctoSense (App Hub at commit A) and Rinx (App Hub at commit B) links two copies, and the phone build fails ("specification is ambiguous").
- A `[patch]` cannot point a git source at another commit of the same URL. OctoSense worked around it with a second host spelling (`www.github.com`), removed in #221 once Rinx v1.0.2 matched; the next App Hub change (App Hub #44, OctoSense #210) brought the conflict straight back.
- The manifest parser is **strict** (`#[serde(deny_unknown_fields)]` on every manifest struct). A manifest with a field an older reader does not know is rejected. That is deliberate and right for security (a silently ignored field could be a restriction), but it means a manifest written for a newer App Hub cannot run in an older Rinx, and vice versa.

So the coupling is real (the mini-app rules are shared on purpose: a mini app in Rinx must get the same manifest format, permissions and integrity checks as an app in App Hub), but it is enforced at the wrong granularity: every commit, instead of every change to the rules.

## Decision

### 1. One contract crate, small on purpose

App Hub publishes **`octosense-app-contract`**, versioned `1.x`, holding only what an app or host needs to read, check and run an app package:

- **Manifest:** `AppManifest` and its parts, `SCHEMA`, `MANIFEST_FILE`, `parse`.
- **Policy:** `policy::resolve`, `HostLimits`, `AppPolicy` (the app-facing part: capabilities, network hosts, storage block, limits).
- **Integrity:** `digest_dir`, `bundle_digest`, `admit`, `admit_digest`, `SignatureVerifier`, `RefuseAllSignatures`.
- **Running a package:** `SCRIPT_ENTRY`, `script_source`, `ASSETS_PLACEHOLDER`, `AssetServer`, `StaticAssets`, `rewrite_assets`.

**What an app may do is in the contract; how a host sandboxes it is not (decided 2026-09-30).** The contract's `AppPolicy` fixes the app's permissions and limits: capabilities, network hosts, storage, budgets. Each host turns that into its own sandbox settings (App Hub's `IsolateSettings`, `splash_adapter::apply`, Rinx's own Splash setup), under one rule: **a host may restrict more than `AppPolicy` says, never less.** Hosts can evolve their sandboxes freely. App Hub may publish its Splash setup as a separate helper crate for other hosts to reuse, but that helper is not part of the contract and carries no stability promise. The fixture corpus checks `AppPolicy`, not any host's settings.

Everything else stays inside App Hub and may change freely: the catalog and store, listings, research scopes, the host service registry, agent session profiles, the Card runner. **No app depends on `octosense-app-hub` or `octosense-app-policy` directly any more**; Rinx's `miniapp-catalog` keeps its own catalog on top of the contract. `octosense-app-policy` itself depends on the contract and re-exports it, so App Hub and the shell keep one implementation.

### 2. The stability rules

Within `1.x`:

- **Additive only.** New types, new functions, new optional manifest fields, new enum variants behind `#[non_exhaustive]`. Nothing is removed or renamed, and no existing field, default or rule changes meaning. A change that would break this is `2.0` (section 4).
- **Unknown manifest fields are classified, not ignored.** The strict parser stays, with one rule for growth, after PNG's critical chunks:
  - every field added in `1.x` declares whether it is **optional** (a host that does not know it may run the app without it: it only adds information or asks for less) or **required** (it restricts or changes what the app gets);
  - a manifest that uses required fields lists them in `requires: ["<feature>", …]`; a host that does not know a listed feature **refuses the app** with a clear message ("needs a newer host");
  - unknown fields that are not covered by a `requires` entry and are marked optional in the manifest's own `schema_minor` are ignored; anything else is still rejected.

  So an older host never runs an app with weaker rules than its author wrote, and a newer manifest field never breaks an older host unless it has to.
- **`schema` stays `1`** for the whole `1.x` line; `schema_minor` records which additions a manifest uses.
- **Behaviour is pinned by fixtures.** The contract crate keeps a corpus of real manifests and packages from every released `1.x`. Every release must accept all of them with the same resolved policy, digest and run settings; CI fails otherwise.

### 3. Depend on the version, not a commit

- `octosense-app-contract` is published to a registry, so consumers write `octosense-app-contract = "1"` and Cargo resolves one `1.x` for the whole build. No lockstep, no duplicate copies, no host-alias tricks.
- **Registry: crates.io (decided 2026-09-30).** App Hub's source is already public, and Rinx, OctoSense and third-party apps can depend on it with no credentials or setup. A published version can never be deleted, only yanked, so releases go through review. A private registry was the alternative, rejected because every consumer's CI and machine would need a token and outside app developers could not use it. Plain git dependencies cannot work: Cargo never unifies two git commits.
- OctoSense's root `Cargo.toml` states `octosense-app-contract = "1"`, and its `Cargo.lock` records the exact version it ships with (1.0.0 today; *2026-10-04: 1.1.0 since [#312](https://github.com/OctoSense-org/OctoSense/pull/312)*); apps state the lowest `1.x` they need. Inside OctoSense, OctoSense's lock is what links. *(2026-10-06: OctoSense needs contract 1.5.0, which adds the connected-account capabilities (`auth`, `github`, `gmail`, `gcalendar`), but crates.io has only up to 1.2.0. The root `Cargo.toml` therefore overrides the contract with `[patch.crates-io]`, which points every consumer at one App Hub revision, now `d6a14133` (since [#347](https://github.com/OctoSense-org/OctoSense/pull/347)), so Cargo still links one copy. `Cargo.lock` records that revision. Remove the override once 1.5 is published.)* *(2026-10-07: crates.io now has 1.5.0; 1.3.0 and 1.4.0 were never published. [#356](https://github.com/OctoSense-org/OctoSense/pull/356) removed the override, so `Cargo.lock` resolves 1.5.0 from crates.io. `desktop-v0.1.0-beta.2` was built with the override, at `d6a14133`.)* *(2026-10-07, later: [ADR 0012](0012-app-host-api-discovery.md)'s host APIs need contract 1.6.0, which crates.io now publishes. Since [#360](https://github.com/OctoSense-org/OctoSense/pull/360), the root `Cargo.toml` states `octosense-app-contract = "1.6"`, and `Cargo.lock` resolves 1.6.0.)* *(2026-10-08: since [#364](https://github.com/OctoSense-org/OctoSense/pull/364), the root `Cargo.toml` states `"1.7"` and `Cargo.lock` resolves 1.7.0, which adds the `wasm` capability.)*
- Rinx's CI should build against the lowest and the highest `1.x` it claims. *Not yet in place (2026-10-01): with only 1.0.0 published both are the same version; the check joins Rinx's CI (`main.yml`) with the first 1.1.0.* *(2026-10-04: 1.1.0 is published; the check is still not in Rinx's CI.)*

### 4. Breaking changes

A change that cannot be additive becomes `octosense-app-contract 2.0`. Hosts support `1.x` and `2.x` side by side for at least one OctoSense release (a manifest's `schema` picks the parser), apps move at their own pace, and `1.x` keeps getting security fixes until the transition ends. This should be rare and is decided in an ADR.

### 5. Who changes the contract

The contract crate lives in the App Hub repository under `crates/app-contract`, with its own changelog. A pull request that touches it needs review from App Hub and from one app owner (Rinx). *Not yet enforced (2026-10-01): App Hub has no CODEOWNERS; the rule is followed by hand until a CODEOWNERS entry for `crates/app-contract` is added.* CI runs: the fixture corpus, a public-API diff (`cargo public-api` or `cargo semver-checks`) that fails on any non-additive change within `1.x`, and Rinx's build against the new version. *(2026-10-04: not in place for Rinx: App Hub's CI runs the fixtures and `cargo semver-checks`, but builds no Rinx.)*

## Consequences

- App Hub can change its store, catalog, Card runner and internal policy as often as needed without touching any app; only contract changes are coordinated, and those are additive by rule.
- Rinx releases when Rinx changes, not when App Hub does; OctoSense no longer needs host aliases or same-commit pins for App Hub.
- Manifests gain `requires` and `schema_minor`; older manifests (no `requires`) keep working unchanged.
- The contract surface is now public API: a deliberate cost, paid for with the fixture corpus and the API-diff check.
- Publishing adds a release step (a crates.io token in App Hub's CI) and a version to bump for each contract change.

## Plan

1. **Done (#210):** stopgap: restore OctoSense's `www.github.com` App Hub alias (dropped in #221), so #210 (App Hub 2a3d84b3) lands without a Rinx release. Rinx 1.0.3 is not made.
2. **Done (App Hub #46, #47; 1.0.0 on crates.io):** create `crates/app-contract` with the section 1 surface, moved out of `app-policy` (which re-exports it); add `requires`/`schema_minor`; add the fixture corpus and the API-diff check; set up a crates.io publishing token in App Hub's release workflow; publish `1.0.0`.
3. **Done (#243):** OctoSense: depend on `octosense-app-contract = "1"` where the shell uses the contract; remove the App Hub alias.
4. **Done (Rinx #49, tag `v1.1.0`):** Rinx: replace its `octosense-app-policy` and `octosense-app-hub` uses with the contract (about 30 call sites, plus its own catalog types), and build its mini-app sandbox from `AppPolicy` (or App Hub's optional helper crate); release as Rinx `1.1.0`, the last release coupled to an App Hub commit.
5. **Open:** other apps (AppCard, OctoScript tooling) follow the same rule when they next change.

## Decided on review (2026-09-30)

- **Registry:** crates.io (section 3).
- **Sandbox settings:** not in the contract. The contract fixes what an app may do (`AppPolicy`); each host builds its own sandbox from that and may only restrict further (section 1).
