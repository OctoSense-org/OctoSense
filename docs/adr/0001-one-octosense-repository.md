# ADR 0001: One OctoSense repository for the shell, its services, the system apps and both packagings

- **Date:** 2026-09-27
- **Status:** Proposed
- **Scope:** Where OctoSense's own code lives: the shell (desktop and phone), the shell services (octos kernel service, app-agent broker, AI-providers glue), the first-party system apps and their host services, and the desktop and ROM packagings. It does not move makepad, octos, OctoScript, its runtimes, App Hub, the app-building harness or the websites.
- **Supersedes:** the split into OctoSense-Desktop, OctoSense-ROM and OctoSense-System-Apps, and the interim idea of a separate OctoSense-Core repository.
- **Relates to:** ROM `home/docs/adr/0003` (App Hub), `0004` (system apps are contained script apps), Rinx ADR 0007 (host-owned octos app peers).

## Context

OctoSense is one product built from three repositories that are always changed together:

| Repository | Holds |
| --- | --- |
| OctoSense-Desktop | the shell (window manager, desktop styles, phone layer, module hosting) and desktop packaging |
| OctoSense-ROM | a second copy of the same shell under `home/`, plus the phone-only UI, the privileged Settings app, the Android system bridge and the ROM image (vendor, patches, scripts, web installer, OpenHarmony) |
| OctoSense-System-Apps | the system apps and their host services, and, since 2026-09-26, shell infrastructure that every shell needs: the octos kernel service (`crates/octos-core`) and the app-agent broker (`crates/app-peers`) |

Measured on 2026-09-27 (ROM `main` 5498e5b, Desktop `main` 19514f1):

- **The shell exists twice.** ROM `home/src` and Desktop `src` share 47 Rust files (about 35,700 lines in ROM's copy). Only 18 are identical; the others differ by about 5,000 lines. Desktop's `src/` is a subset of ROM's: 32 of its 33 top-level files also exist in ROM. Changes routinely had to be made twice (`app_peers_host.rs`, kernel registration, the AI-providers registration), and the copies drift (Rinx is in ROM's `mobile-apps` but not Desktop's; the AI-providers QR importers differ per shell).
- **Every internal change is a chain of pin PRs.** The shells pin System-Apps by commit (`native-apps.lock.json`), and System-Apps' host services and the shells must agree on one App Hub revision. Since 2026-09-26, 17 of the 54 PRs merged into these three repositories only moved pins. A kernel change took four PRs and up to three 40-minute ROM CI runs to reach both shells.
- **One-App-Hub is kept by hand.** When the Mail service and the shells pinned different App Hub revisions, the build linked two copies and needed a `www.github.com` `[patch]` workaround.
- **Infrastructure lives in the wrong place.** System-Apps is presented to app builders (including hackathon contestants) as "the first-party apps", yet holds the kernel service and the broker.
- **Commit metadata leaked** a work address (`futurewei.com`) into public history, from repository-local git identities on the development machine (removed on 2026-09-27). A scan of every OctoSense-org repository, octos and Rinx on 2026-09-27 found it only in OctoSense-ROM (5 fields, 2026-09-20), the org profile `.github` (3 commits, rewritten and force-pushed on 2026-09-27), five stale octos branches (deleted on 2026-09-27) and the private, archived OctoSense-mobile. App Hub, OctoScript-Makepad, makepad and System-Apps are clean.

## Decision

### 1. One repository, `OctoSense`

OctoSense-Desktop is renamed to **OctoSense** (it was named that before) and becomes the single home of:

```
octosense/
  crates/
    shell/        the one shell crate: window manager, desktop styles, phone layer
                  (gestures, pages, shade, island, navigation, Back), module hosting, app registry, AI bus
    kernel/       the octos kernel service (was System-Apps crates/octos-core, crate renamed octosense-kernel)
    app-peers/    the app-agent broker (Rinx ADR 0007)
    ai-host/      kernel + llm service + broker glue, one entry point for both packagings
  apps/           system apps and their host services (was OctoSense-System-Apps apps/):
                  news, photos, maps, camera, mail, ai-providers, appcard (opt-in)
  desktop/        desktop packaging: main, catalogs (config/apps.json), themes, wallpapers,
                  upstream window-manager sync, release (macOS/Windows/Linux; dev Android APK)
  phone/          the Home app: Android and OpenHarmony (and iOS simulator) packaging,
                  APK build scripts and bundled kernel, the Settings app, the phone side
                  of the system bridge
  rom/            the OnePlus 6 ROM image only: vendor/ (product, privileged permissions,
                  overlays, Settings backends), the system side of the bridge (SystemUI,
                  Quickstep, PermissionController hooks), patches/, image/flash/OTA
                  scripts, web-installer/
  tools/          kernel-artifact (cross-built liboctos.so), shell-graph checks, setup
  docs/           ADRs (this series; ROM home ADRs 0001–0006 move here as history), guides
```

The two shells become packagings of one `crates/shell`. Desktop and phone differences are targets and features of that crate, not copies.

`phone/` and `rom/` are separate because they are different products with different build systems: the Home APK installs on any Android phone (cargo-makepad, built in CI), while the ROM image is a LineageOS build for the OnePlus 6 (AOSP chroot, not in CI) that consumes the Home APK and adds the privileged system side. "Home" remains the product name of the phone app.

### 2. What stays separate

These have their own users, upstreams or release cycles and are consumed by pin:

| Repository | Why it stays |
| --- | --- |
| OctoSense-org/makepad | fork tracking upstream Makepad |
| octos-org/octos | separate project and organisation |
| OctoScript, OctoScript-Makepad, OctoScript-Android, OctoScript-OH | the language and its runtimes, used beyond OctoSense |
| OctoSense-App-Hub | the public store: catalog, gate, `hub`, `card-host`, submission issues |
| OctoScript-App-Design-Flow | the contestant harness; must stay small to clone and link-stable during the contest |
| websites (OctoSense-website, OctoScript-website, octosense-org.github.io) | separate deploys |
| hagency-org/Rinx | separate project, hosted as a module |

The external pin chain becomes: makepad → OctoScript-Makepad (runtime) → App Hub → OctoSense, and octos → OctoSense.

### 3. Pinning rules inside the repository

- **One revision per external dependency**, set once in the workspace (`Cargo.toml` `[workspace.dependencies]` and the runtime lock). App Hub, octos, Rinx and the runtime are pinned exactly once; host services, the shell and AppCard inherit them. Two App Hub or octos sources become impossible, not merely avoided.
- **No internal pins.** The shell, services and apps change in the same commit.
- **CI graph checks** (one shared script) assert one makepad, one App Hub, one octos per build, the kernel service in default and `mobile-apps`, and no AppCard module in `mobile-apps`.

### 4. CI and releases

- **Path-filtered workflows:** the 40-minute phone job runs when `phone/`, `crates/` or `apps/` change, not for desktop-only or docs changes. Desktop checks run for `desktop/`, `crates/`, `apps/`.
- **Products keep their own releases,** tagged per product: `desktop-v*`, `home-v*` (APK), `rom-v*` (image). System apps ship inside the shells (section 6). Build receipts record the repository commit.
- **Contributors** keep one PR per change; required checks are the union of the affected paths.

### 5. History and identity

The new layout is built by importing the three repositories with history, rewritten once so every commit carries the project's public identity (`ymote <151983+ymote@users.noreply.github.com>` or the author's own public address), using a mailmap. OctoSense-Desktop's own history is rewritten the same way during the import (it becomes OctoSense). This replaces the deferred force-push rewrite for these three repositories: after the import, **OctoSense-ROM and OctoSense-System-Apps are archived with a pointer and made private** (decided 2026-09-27).

The rewrite exists only to keep the work address out of public history (see Context). It is needed only for OctoSense-ROM, which this import covers; the repositories that stay separate (makepad, App Hub, OctoScript-Makepad) are clean and are **not rewritten**. A rewrite does not remove commits reachable from GitHub's pull-request refs, existing clones or forks; a GitHub Support request purges the cached pull-request refs of OctoSense-ROM and `.github` after the import.

### 6. System apps ship only inside the shells

System apps (`os.*` ids) are packed into the shell build, admitted by digest and run under system limits; App Hub refuses `os.*` ids from any store by design. They are not released separately (decided 2026-09-27). Updating system apps through App Hub would need a signed, rollback-safe system-app channel and is future work.

### 7. Contestant and link stability

The hackathon is running (preliminary deadline 2026-10-04, finals 2026-10-12). Contestant-facing entry points do not move: App-Design-Flow and App Hub stay where they are. Every moved path keeps a pointer: archived repositories' READMEs and descriptions, redirects via GitHub's rename for OctoSense-Desktop → OctoSense, and updated links in the org profile, READMEs, AGENTS.md and the websites (English and Chinese). No contestant-visible move happens before 2026-10-12 unless the links are verified the same day.

## Migration

Each phase ends green and shippable.

| Phase | Work | Exit criteria |
| --- | --- | --- |
| 0. Freeze and land | Land or move open PRs on the three repositories (including outside contributors' PRs on forks); announce a short freeze | No open PR left without an owner and a target |
| 1. Import | Rename OctoSense-Desktop → OctoSense. Import ROM (its `home/` into `phone/`, with `home/src` kept at `phone/src` for now; the image parts into `rom/`) and System-Apps (`apps/`, `crates/kernel`, `crates/app-peers`) with rewritten history. Workspace dependencies pinned once. Both packagings build from the one checkout | Desktop and ROM CI green from the new repository; APKs contain `liboctos.so`; graph checks pass |
| 2. Services | Create `crates/ai-host`; replace both shells' kernel, llm and broker glue with it; one kernel-artifact tool; one CI graph script | One registration path; ROM and Desktop features forward to `ai-host`; tests from both shells moved and passing |
| 3. One shell | Reconcile the 29 differing shared files into `crates/shell`; move the generic phone layer into `crates/shell`; `desktop/`, `phone/` and `rom/` keep only packaging, the Settings app, the Android integration and ROM-only code | No `.rs` file exists twice; headless ROM and Desktop runs pass; Android builds for both packagings |
| 4. Archive | Archive OctoSense-ROM and OctoSense-System-Apps with pointers; update org profile, READMEs, AGENTS.md, websites (en + zh); retire sibling locks (`native-apps.lock.json`) | Link check clean across the org; contestant path re-verified |

Estimated effort: phases 0–2 about one week, phase 3 one to two weeks (the shared files must be reconciled, not copied), phase 4 one to two days.

## Consequences

- One PR per product change; no pin-only PRs between the shell, services and apps.
- The shell exists once; desktop and phone differences are explicit targets and features.
- App Hub, octos and the runtime are pinned once, so the one-source rules hold by construction.
- The repository is larger (ROM image pieces, patches, web installer); desktop-only contributors clone more. Contestants are unaffected (they use App-Design-Flow and App Hub).
- CI must be path-filtered to keep desktop changes fast.
- System apps and their host services are versioned with the shell. App builders still read them as examples, now under `octosense/apps/`.

## Alternatives considered

- **Status quo (three repositories).** Keeps today's duplication and pin chains.
- **A separate OctoSense-Core repository** for the kernel service, broker, glue and a shared shell crate, with Desktop and ROM as thin packaging repositories. Removes duplication but keeps pins between four repositories; Desktop would shrink to little more than a `main.rs`.
- **Also moving App Hub and App-Design-Flow in.** Rejected: they are the public store and the contestant harness, with their own users, issues and release cadence.

## Risks

- **Contest disruption.** Mitigated by keeping contestant repositories unchanged and delaying visible moves until after 2026-10-12 or verifying links the same day.
- **Reconciling drifted shell code** can regress either packaging. Mitigated by phase 3 file-by-file with headless runs and both Android builds, and device checks on the OnePlus 6 when available.
- **Other sessions and contributors in flight.** Phase 0 lands or retargets their work first.
- **Large CI cost.** Path filters and cached builds.

## Decisions (2026-09-27)

1. **Rename OctoSense-Desktop to `OctoSense`** (keeps its issues, stars and redirects).
2. **Make OctoSense-ROM and OctoSense-System-Apps private** after archiving them.
3. **System apps ship only inside the shells** (section 6).

## Open questions

None.
