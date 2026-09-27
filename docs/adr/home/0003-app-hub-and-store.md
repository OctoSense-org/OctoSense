# ADR 0003: The app hub, its signatures, and the store app

- **Date:** 2026-09-19
- **Status:** Proposed
- **Implementation status:** Built and exercised end to end on 19–20 Sep 2026. `octosense-app-hub`: the index; the catalog signed by an anchor-certified working key (ed25519); the deterministic gate; a one-file pack format for HTTP delivery; the freshness window (installs pause after 14 days without a verified catalog, running apps unaffected); the agent-scan packet and a pluggable reviewer whose verdict can only route to pass, human review or reject, and which falls to human review whenever the reviewer fails; and the device client (verify, list, search, install, remove, refuse to run what is withdrawn). The `hub` command covers keygen, pubkey, certify, stamp, sign-manifest, check, scan, publish (with `--reviewer`), withdraw and verify. `octosense-appstore` is the store app: it fetches a catalog over HTTP or from a mirror, verifies it, lists and searches, shows resolved permissions before install, installs from a pack with hash and publisher-signature checks (publisher keys travel in the signed catalog), serves the running app its artwork from a per-app loopback origin that is the app's one allowed loopback entry, and runs the card under the enforced policy of ADR 0002. Demonstrated: a bundle reaching an outside server refused by the gate; a hand-edited catalog refused by the device; a stand-in reviewer rejecting a bundle whose data addressed an assistant; publish → fetch over HTTP → install → run; and a withdrawal that a relaunched store shows and refuses to open. Not built: a real model behind the reviewer (the interface is a command), publisher identity beyond holding a key, and a hosted hub; the first version is a directory served statically. **On the phone (20 Sep 2026):** the store module is linked into the OctoSense shell (`app-appstore` feature; unconditional on Android), the shell was built as a separate debug package beside the ROM's platform-signed home app, and on the OnePlus 6 the store verified the catalog, listed the camera card with its permissions, installed it (pack fetched, digest and publisher signature checked) and ran it in a contained isolate. The phone had no network in range, so the hub was served over the USB link from an identical mirror through a `hub.txt` override in the store's data directory; HTTPS to the public hub is compiled in (rustls with bundled roots) but not yet exercised from the device. Two phone findings fixed: the generated store screens must be fit-sized inside their container or the controls land outside it; the store logs its control rectangles because the remote instrument does not exist on Android. Still cosmetic: isolates lack a CJK face, and the camera scene renders at the half-size artboard it was compiled at. **Listing (20 Sep 2026):** `listing.json` is validated by the gate (closed category, platform and age-rating lists, https privacy policy, assets inside the bundle), carried in the catalog entry and the review packet, and shown by the store with the manifest-derived privacy summary; installed apps open as their own window-manager clients through a `card` host module. camera-card 1.0.1 is the first published entry with a listing.
- **Scope:** How an app that is not compiled into OctoSense is published, admitted, distributed and installed, and what the person sees when installing one. Builds directly on [ADR 0002](0002-agentic-app-security-model.md), which defines what contains such an app once it runs.
- **Depends on:** ADR 0002 phase 1 (the manifest and its resolution into an isolate's settings and an agent session profile).

## Context

ADR 0002 settled what an installed app may do. It did not say where an app comes from, who vouches for it, or how a device decides to run one. Without that, the contained tier has no supply chain: the only way to get an app onto a phone is still a shell release.

Three constraints shape the answer.

**Native code cannot arrive after the build.** Rust has no stable plugin ABI and Android refuses to execute code written after install, measured on device. So the hub distributes cards and script apps, never native modules. Native apps contribute by pull request into the apps repository and ship in a shell release.

**Cards are text.** A card bundle is readable, which makes community review and automated checking realistic in a way that reviewing a binary is not.

**The device must be able to refuse.** A catalog that cannot be verified, or an app version that cannot be withdrawn, is worse than no catalog at all.

## Decision

### 1. Two repositories, two jobs

- **`octosense-apps`** holds code we compile: modules and their entry crates. Pull requests here are source, reviewed as code, shipped in a shell release. The hub's own code — the policy and hub crates, the store module and the hosts — lives with the catalog in `OctoSense-App-Hub` under `crates/`, so an app repository never carries hub code and the catalog never depends on an app.
- **`OctoSense-App-Hub`** holds no app code. It holds the index, the checks, the artifact store and the signed catalog. Publishing is a pull request that adds one index entry.

A developer of a card app keeps their app in their own repository. What they send the hub is an index entry: the manifest, the bundle hash, their key identity, the source repository and commit, and a status field.

### 2. The hub keeps its own copy of what it admits

On merge, a job fetches the artifact, checks it against the hash in the entry, and stores an immutable copy keyed by that hash. Pointing devices at a third-party URL makes availability someone else's uptime problem and leaves nothing to serve when a link rots. Review binds to exact bytes; the hub must hold those bytes.

### 3. Two signatures, for two different questions

| Signature | Covers | Answers | Required |
|---|---|---|---|
| Publisher, over the canonical manifest (which carries the bundle hash) | What the app is and what it may do | Who is responsible for this app | Optional at first, mandatory once the hub takes outside submissions |
| Hub, over the catalog | Every admitted version, its hash, its resolved capabilities, and its status | Was this admitted, and is it still allowed | Always |

The catalog signature carries revocation, so it must be verified even when a bundle is already on the device. The hub does not sign bundles individually: the catalog names their hashes, so signing the list endorses the bytes.

**Update continuity.** Once an app has a publisher key on record, a later version must be signed by that key. An account takeover alone then cannot ship a different author's code as an update. Re-keying is a human-reviewed change to the index, never self-service.

### 4. Three trust domains stay separate

The ROM's update key signs system updates. The platform key signs the packages in the ROM. The **hub anchor** covers the catalog, and nothing else. One key for all three would make a store compromise a device compromise.

The anchor is held offline and used rarely. The hub's working key, used by automation on every publish, is certified by the anchor, so rotating it is a signed statement the device already understands. Only an anchor compromise needs a shell release.

### 5. Admission: facts by code, judgement by agent

**Stage one, deterministic.** No model involved, and nothing here is arguable:

- the manifest parses and resolves under the host's ceilings, requesting nothing unknown;
- the bundle digest matches the entry; no symlinks; size within budget;
- **every asset is inside the bundle.** ADR 0002's prototype found a card with no network grant fetching nine images over HTTP through the resource loader, so this check is what makes the host allowlist real;
- the card lowers and renders headless in the reference host, producing a screenshot;
- a scripted walk-through drives each declared surface;
- identity: unique id, strictly newer version, signature continuity where a publisher key is on record.

**Stage two, the agent scan.** Given the manifest, the card text, the screenshots and the walk-through, an agent answers what code cannot: does the app do what it claims, do the requested capabilities match visible behaviour, is the interface deceptive or impersonating, does any bundled content read as an attempt to steer an agent. It returns a structured verdict with citations, routing the submission to pass, human review, or rejection with reasons.

**The rule that keeps this honest:** the agent never decides a security fact. Containment comes from the manifest and the runtime, so a mistaken pass yields a contained app, not a compromised device.

### 6. What the device does

Fetch the signed catalog, verify it against the anchor chain, and for an install: the entry is not withdrawn, the bundle hash matches, the manifest resolves. Only then create the app's jail and run it contained per ADR 0002. An installed app whose version is later withdrawn stops running.

**Offline is a decided behaviour, not a default.** A device that cannot reach the hub keeps running what it has, serves its cached catalog for a bounded freshness window, and refuses new installs once that window lapses. Otherwise a phone kept offline is a place revocation never reaches.

### 7. The store app

The store is an ordinary OctoSense app in the trusted tier — a native module, shipped in the build, because it holds the anchor, writes app jails and is the one place that installs. It shows:

- the catalog: what is available, its publisher, its version;
- **what each app will be allowed to do, in plain words, before install** — derived from the resolved policy, not from the app's own description;
- what is installed, and what each installed app has actually used;
- a way to remove an app, which removes its jail;
- why an app stopped, when a version is withdrawn or a budget kills it.

The store never grants anything an app did not declare, and never asks the person to approve something the manifest did not request.

### 8. The listing

Beside the manifest, a bundle carries `listing.json`: subtitle, description, category, keywords, screenshots and icon as bundle paths, the platforms the publisher ran it on, the publisher's name, support contact and privacy policy URL, release notes, an age rating and a licence. It is reviewed with the bundle under the same digest, travels in the signed catalog, and the store shows it beside the permissions. Two things the store shows are never written by the publisher: what the app may do, and a privacy summary, both derived from the resolved manifest, so a listing cannot understate the app. An icon and at least one screenshot are required by the gate, as on every store people know: the icon is what the launcher shows once installed, and a screenshot is the one listing claim a reviewer can check against the rendered card. The store fetches each entry's icon and screenshots from the hub's copy of the bundle (the artifact path in the catalog), so a listing's pictures are the reviewed bytes, not a URL the publisher controls. The store's screens follow the App Store's shape — icon-and-pill rows, a product page with an information strip, a screenshot rail, description, what's new, information and privacy — because that is the shape people already read. Ratings and reviews are deliberately absent until publisher identity is settled, because a review needs an identity behind it just as a submission does.

## Consequences

- Publishing is a reviewable diff with history, and the developer sees the same report the hub does before a human looks.
- The hub becomes a place that must be operated: keys, an artifact store, and a job that runs the checks. The first version keeps this to a repository, its continuous integration, and a static signed file, with no service to run.
- Holding artifacts means storage cost and a takedown responsibility, which is the price of review binding to exact bytes.
- Key loss by an individual publisher is a certainty at some scale; the recovery path must exist before it is needed.
- The store app is in the trusted tier, so it is our code and ships in releases. A bug there is a shell bug, not a contained one.

## Alternatives considered

- **Apps contribute source into the apps repository.** Rejected for card apps: it puts a community's code in our release, couples their cadence to ours, and gains nothing, since their code never compiles into the shell anyway.
- **Index points at third-party URLs, hub stores nothing.** Rejected: link rot and availability, and a hash pin cannot serve bytes that have vanished.
- **One key for updates, packages and the catalog.** Rejected: a store key is used constantly by automation; an update key must be rare and offline.
- **Agent review as the gate.** Rejected: a model's verdict is judgement, and judgement must not be the thing standing between an app and the person's data. Deterministic checks plus containment do that work.

## Open questions

1. Where the catalog and artifacts are hosted, which decides offline and restricted-network behaviour.
2. The freshness window for a cached catalog, and what an app does when it lapses mid-use.
3. Whether the store surfaces per-app usage (network hosts contacted, storage used) from the runtime, which requires the enforcement points of ADR 0002 phase 2 to report as well as refuse.
4. How a publisher proves identity at first registration, beyond holding a key.
5. Whether script apps join cards in the first version of the hub, or whether the hub ships card-only until per-app vocabulary exists.

## Implementation note

See the implementation status line at the top for what has actually been built. The pieces are ordered: the checker first, since it is the shared gate and reuses ADR 0002's policy crate and reference host; then catalog signing and its device-side verification, which is what makes install and revocation trustworthy; then the index and the store app; the agent scan last, because it adds judgement on top of facts and is useless without them.
