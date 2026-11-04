# ADR 0002: GitHub-attested publisher identity

English | [简体中文](0002-github-attested-publisher-identity.zh-CN.md)

Status: accepted, implemented and in production. App Hub #153 implemented
this decision in app contract 1.8.0, which adds the `publisher-github-v1`
host requirement. Two tag-push releases of a test app passed native Store
install, update and launch checks
([evidence and limits](../PUBLISHING.md#github-publisher-provenance)). Public
catalog sequence 15 offers the three reference apps (GitHub Notes at 0.2.2, the
others at 0.2.1) and Camera Card Demo at 1.1.1, all published with GitHub provenance; each app kept the same
GitHub identity across its update. OctoSense desktop 0.1.0-rc.1 (RC1)
installs them on macOS and checks their attestations and publisher
continuity at install and update.
[Amended on 2026-10-08](#amendment-2026-10-08): App Hub accepts only
GitHub-attested releases; gate enforcement is pending.

## Context

Until contract 1.8.0, an Ed25519 publisher key was an app's only update
authority. Once a version signed by the key was in the catalog, every later
version needed that key's signature. No command replaces a lost key, and the
Hub has no key rotation: a lost key ends the app's updates. Hackathon
developers also stalled on signing: some submitted a commit from before the
final signature, and others could not run `hub check` or `hub scan` on a
signed bundle without its public key.

[ADR 0001](0001-github-attested-catalog.md) introduced GitHub-admin catalog
attestations and deferred publisher identity, including whether key-signed
apps migrate.

An app's code is OctoScript: Splash scripts and L0 cards, shipped in the
bundle as readable text. The only compiled code a bundle may carry is
WebAssembly in `fns/`. OctoSense runs it only in builds with the `wasm-lab`
feature, and no OctoSense release enables that feature. App Hub keeps an
exact copy of every admitted bundle in its public `artifacts/`. OctoScript
apps are therefore open by default: anyone can read an admitted app's source.

Note, 8 October 2026: since
[OctoSense #400](https://github.com/OctoSense-org/OctoSense/pull/400),
standard desktop and Home builds of OctoSense `main` run the WebAssembly in
`fns/` on macOS, Linux and Android. No OctoSense release includes it yet.

## Decision

Apps prove their publisher with GitHub provenance, an attestation from a
GitHub Actions run, instead of an Ed25519 publisher key. App Hub accepts only
GitHub-attested releases, for new apps and for updates.

- **Attestation.** Pushing a `v<manifest.version>` tag runs the app's
  workflow on a GitHub-hosted runner in its public repository.
  `hub publisher-prepare` records the repository, its owner, the workflow,
  the tag and the commit in `integrity.github` and adds
  `requires: ["publisher-github-v1"]`. It then writes the canonical manifest:
  the exact bytes that the attestation covers. `actions/attest` signs that
  manifest through Sigstore, and the workflow attaches the attestation and
  packs the release.
- **Verification.** The canonical manifest carries the bundle digest, so the
  attestation binds every file in the bundle. The Hub's gate and the host
  verify the attestation offline against the public-good Sigstore trust
  snapshot built into them. They pin the repository ID, owner ID, workflow
  path, tag and commit, and require a push trigger, a GitHub-hosted runner
  and a repository that was public at signing.
- **Updates.** The catalog records the publisher as `github:<repository_id>`.
  Each update must come from the same repository name, repository ID, owner
  ID and workflow path, with a higher semantic version. The gate and the host
  refuse replays and rollbacks.
- **GitHub provenance only.** A release carries GitHub provenance and no
  Ed25519 signature. Reviewers do not approve a key-signed release.
- **Approval.** The submission issue remains the publication request. A tag
  or GitHub Release alone submits and approves nothing: an App Hub admin
  approves each version and publishes it through the protected catalog
  workflow of ADR 0001.
- **Existing apps.** Catalog sequence 14 marked the six key-signed reference
  entries withdrawn in favor of their GitHub-attested successors
  ([amendment](#amendment-2026-10-08)). GitHub provenance cannot take over an
  app on record as unsigned or key-signed, and an app published with GitHub
  provenance cannot fall back to a key or to unsigned releases.
- **No migration.** An app moves to GitHub provenance only under a new id, as
  the reference apps did with their `io.github.ymote.*` releases.
  Installations and their data do not migrate.

## Consequences

- Developers manage no keys: no `hub keygen` and no repository signing secret.
  GitHub issues a short-lived identity for each workflow run.
- The repository becomes the update authority. Anyone who can push a tag
  there can produce a valid release, although an admin still approves its
  publication. Renaming or transferring the repository, or renaming the
  workflow, ends the app's updates.
- Repositories must be public. This suits OctoScript's open-by-default model:
  a public repository exposes little that App Hub does not already publish.
- The workflow template from OctoSense App Flow (formerly Design Flow), which
  `tools/octo publish-github` installs, handles only a `bundle/` at the
  repository root. A bundle elsewhere, as in a monorepo, has no template.
- Installing requires a host that supports `publisher-github-v1`, such as
  RC1; older hosts refuse the app.

## Open items

- The gate does not yet refuse a key-signed release
  ([amendment](#amendment-2026-10-08)).
- Apps in private repositories, and closed-source apps, need a future
  decision.
- Nothing restores updates after a repository is renamed, transferred or
  deleted; the app continues only under a new id.
- Rotating the Sigstore trust root requires a host update, as it does for
  catalog attestations.
- Store installs remain unverified on iOS, Windows and Linux. No released
  phone build supports `publisher-github-v1` yet; on Android, only a debug
  build has passed install and update checks, against a local test catalog.

## Alternatives considered

- **Keep Ed25519 publisher keys.** The problems in Context would remain: key
  custody, no recovery for a lost key, and signing steps that stall new
  developers. The original decision kept the route for apps already published
  with a key; the amendment closed it, because no third-party app used it.
- **Unsigned releases.** The GitHub-attested catalog does not admit an
  unsigned bundle, and no released Store installs an unsigned app.
- **GitHub commit signatures or the Verified badge.** A commit signature
  needs a GPG, SSH or S/MIME key that the developer manages. It covers only
  the commit, and the workflow generates the canonical manifest later. The
  badge only displays GitHub's own check of that signature; a host cannot
  verify it offline.
- **Hub-hosted builds.** The attestation would name the Hub's workflow
  instead of the developer's repository, so it could not carry update
  authority. Catalog authorization would also absorb publisher continuity,
  which ADR 0001 keeps apart.
- **Private repositories.** GitHub signs their attestations with its own
  Sigstore instance, which has no public transparency log and is absent from
  the host's trust snapshot. A private repository would also hide little: the
  bundle ships the app's source, and App Hub keeps a public copy.

## Amendment, 2026-10-08

App Hub closed the Ed25519 publisher-key route to new apps and to updates. No
third-party app had been published with a key.

- App Hub accepts only GitHub-attested releases (`publisher-github-v1`).
- Catalog sequence 14, published on 2026-10-08
  ([run 37852340163](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/37852340163)),
  marked the six key-signed reference entries withdrawn in `catalog-v2.json`;
  the entries stay in the catalog with `status.state` set to `withdrawn`:
  `org.octosense.samples.githubnotes`, `org.octosense.samples.inbox` and
  `org.octosense.samples.googlecalendar`, each at 0.1.0 and 0.1.1, from the
  publisher `ymote`. Their GitHub-attested successors are the
  `io.github.ymote.*` apps.
- Gate enforcement is pending. The change that makes the gate refuse new
  key-signed releases is tracked in
  [App Hub #168](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/168)
  and is not merged. Until it merges, the gate still admits a key-signed
  release, and review alone keeps the route closed. The same issue decides the
  future of `hub keygen`, `hub pubkey` and `hub sign-manifest`.

## References

- [App Hub #153](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/153)
- [App Hub #168](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/168)
- [OctoSense desktop 0.1.0-rc.1 release notes](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.1)
- [GitHub publisher provenance](../PUBLISHING.md#github-publisher-provenance)
- [App Flow's release workflow template](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/tools/publish-app.template.yml)
- [GitHub artifact attestations](https://docs.github.com/en/actions/concepts/security/artifact-attestations)
