# The Makepad fork

OctoSense builds on a fork of Makepad, `OctoSense-org/makepad`, not on
`makepad/makepad` itself. This note records how the two relate, how the
fork reaches this repo, why the sibling checkouts are shallow, and the state
of the four map fixes made for OctosMap. The comparison with upstream was
measured on 2026-09-18, at the revision pinned then (`471d680a5`); the
commands are given so the numbers can be taken again. The pin moved on
2026-09-19 to a revision that adds the four fixes and nothing else. Later
that day the history of every OctoSense-org repository was rewritten; the
hashes here are the ones after it (see "The history rewrite" below).

`docs/upstream.md` describes the older arrangement, in which this repo
pinned official Makepad directly. Its sync tooling for the imported WM
sources still applies; its statements that the framework is official
Makepad and that no fork is required do not.

## What is pinned, and through what

The revision is pinned as a chain, not in one place:

1. `native-runtime.lock.json` here names one revision of
   `OctoSense-org/Octoscript-Makepad` (`36ad1f220`, its main).
2. That repo's `runtime.json` names the Makepad fork revision
   (`57b31c997`, Makepad main: the enforced isolate policy of PR #22, module
   windows, and the ROM's Splash fixes) and the Octoscript revision
   (`68f6a9d`). OctoSense, AppCard, Rinx and the ROM lock the same release.
3. The manifests repeat the Makepad revision as `rev = "…"`: here in
   `Cargo.toml` and `apps/*/Cargo.toml`. Octoscript-Makepad's unmodified
   manifests retain their release pin. `[patch]` sections redirect every one
   of them to the sibling checkout `../makepad`, so a build has a single
   Makepad.

`tools/setup-native.py` prepares the siblings (`../makepad`,
`../octoscript`, `../octoscript-makepad`) at those revisions, and
`tools/setup-native.py --check` fails unless each sibling's `HEAD` is the
locked revision and its tree is clean. It looks at nothing else: not the
branch name, not the clone depth.

The override records the canonical repository URL, an exact commit and
its reason. It is necessary because the latest published wrapper release
predates the containment APIs. Setup validates the original wrapper's
manifest, then prepares and verifies the explicit Makepad override without
editing that wrapper. Consumer manifests must match the effective pins;
`--cargo-manifest Cargo.toml` also rejects duplicate or foreign framework
crates. Existing local changes are preserved. Octoscript stays at its
release revision. Remove the override when a coordinated wrapper release
includes these APIs, updating the consumer pins together.

Octoscript at the chain's revision names an older Makepad revision in its
own manifests: `bb45d4115`, two pin moves back, and a hash from before the
history rewrite, which the fork's remote no longer has (its twin is
`6e5898fe`). The patch sections make that harmless here, since no build of
this workspace ever fetches it, and it shows that Octoscript need not move
every time the Makepad pin does; it did not move on 2026-09-19. A build of
Octoscript on its own, without the sibling patches, would have to fetch
that revision.

The check reads every `Cargo.toml` under this directory, skipping only
`.git`, `target`, `vendor` and a few other names. A second worktree kept
inside the repo (`.worktrees/<name>`) is read too, so after a pin move the
check fails locally until that worktree's branch has the new revision,
although nothing is wrong with this branch. CI has no such directory. To
check one branch alone, give the script a clean copy beside this one:

```sh
git worktree add --detach ../OctoSense-mobile-pincheck HEAD
python3 ../OctoSense-mobile-pincheck/tools/setup-native.py --check --root .. \
  --cargo-manifest ../OctoSense-mobile-pincheck/Cargo.toml
git worktree remove ../OctoSense-mobile-pincheck
```

## How the fork relates to upstream

The fork is a GitHub fork of `makepad/makepad`. Its default branch `dev`
mirrors upstream; `main` carries OctoSense's work and is what gets pinned.

Ancestry, from GitHub's compare API:

```sh
gh api "repos/makepad/makepad/compare/work...OctoSense-org:main" \
  --jq '{ahead_by, behind_by, base: .merge_base_commit.sha}'
```

The fork's `main` and upstream `work` last shared a commit on 2026-09-01
(`915ce7c4e`). Since then the fork has 693 commits upstream lacks, and
upstream has 66 the fork lacks.

Content, by asking a full upstream clone whether each fork file's exact
blob ever existed upstream (`git ls-tree -r <pin>` in the fork, then
`git cat-file --batch-check` in the upstream clone):

| Of the 8,695 files in the fork at `471d680a5` | Files |
|---|---|
| Identical to upstream `work` at `6f1e44649` | 8,171 |
| An older upstream version: the fork is only behind | 321 |
| Modified by the fork: content that was never upstream | 121 |
| Only in the fork | 82 |

Upstream has 432 files the fork lacks. A plain directory diff reports
about 330 differing files and overstates the divergence, because it counts
the 321 files where the fork is merely behind. The real divergence is about
200 files. The modified ones are mostly the scripting engine
(`platform/script`), widgets, platform code, `draw`, `libs/svg` and the
Android build tool; the fork-only ones are mostly the AppCard widget kit
with its fonts and themes, the map's navigation layer
(`widgets/src/map/nav.rs`, added on 2026-09-11 for the AppCard nav card),
`platform/src/gps.rs` and `libs/makepad_ai`.

## The history rewrite

On 2026-09-19 the history of every OctoSense-org repository (this one, the
Makepad fork, Octoscript, Octoscript-Makepad) was rewritten to take local
home paths (`/Users/<name>/…`) out of old commits, and the pins were moved
to the new hashes (#29). What that means for anything written before it:

- Every commit after the first scrubbed one has a new hash. Author, time
  and subject are unchanged, which is how a commit's twin is found. The
  tree is identical too, except in the span where a scrubbed file differed.
- Hashes quoted inside commit subjects were rewritten along with the
  commits; branch names and directory names that embed an old hash
  (`chore/runtime-makepad-c31667a9`, `bt-1b11c4a`) were not.
- Official `makepad/makepad` was not rewritten. Its hashes in these docs
  (`74b63be83e`, `915ce7c4e`, …) and the upstream revisions in
  `upstream/makepad.json` stand as they were, although the fork holds
  rewritten copies of the same commits.
- The docs here were remapped on 2026-09-19: 72 references to 32 commits,
  each matched by author time and subject to one twin on the rewritten
  remote. Hashes in commit messages and pull request text were not, and
  could not be.
- The WM work that predates this organisation's fork is recorded against
  `OctoSense-org/makepad` too: the revision the early plans and
  `upstream/makepad.json`'s `retained_fork_assets` name is `ff134865d`
  there, the tip of `pin/octoscript-makepad-beb3857a` (the branch keeps its
  old name), with the recorded files at their recorded digests. Nothing
  here depends on, or points at, any other copy of the fork.

A clone made before the rewrite has local branches on the old history. A
push from one is rejected as non-fast-forward; do not force it. Fetch,
check that the old tip and the remote tip have the same tree
(`git diff --stat <old> origin/<branch>` prints nothing), move the branch
with `git branch -f <branch> origin/<branch>` from another branch, and
cherry-pick onto a fresh branch whatever was not pushed. The siblings
follow with `git -C ../<sibling> fetch origin` and
`python3 tools/setup-native.py --update`.

## The sibling checkouts are shallow

The bootstrap (`tools/setup-native.py`, following the shared runtime
contract and any explicit consumer override) makes each sibling with `git init`,
`git fetch --no-tags --depth=1 origin <locked revision>` and a detached
checkout. `--depth=1` has been there since the script's first version
(Octoscript-Makepad #22, 2026-09-16), and nothing written down says why:
not the commit, the pull request, its comments, the code or the docs. It
reads as the usual way to fetch one exact commit, which is what the pull
request set out to do, and it is the right thing for CI. Nothing depends on
the clone being shallow.

What a shallow checkout costs on a development machine:

- no `git log`, `blame` or `bisect` behind the pinned commit;
- ancestry questions fail (`git merge-base` between two pins finds nothing);
- rebasing or merging onto a newer fork `main`, or preparing a patch for
  upstream, needs real history. Cherry-picks and pushes still work.

To get full history, which the check accepts unchanged:

```sh
git -C ../makepad fetch --unshallow origin
```

On the bench Mac this was done on 2026-09-18. History went from 5 commits
to 2,927 and `.git` did not grow (239 MB before and after), because an
earlier `git fetch origin` had already brought in the other branches with
their history; only the pinned commit's ancestry boundary was lifted. On a
fresh bootstrap expect a download of the order of the whole fork, which
GitHub reports as about 274 MB.

To go back, either in place (tried on a scratch clone of Octoscript: 464
commits to 1, working tree untouched, space reclaimed only by the `gc`):

```sh
git -C ../makepad fetch --depth=1 origin
git -C ../makepad reflog expire --expire=now --all
git -C ../makepad gc --prune=now
```

or exactly as the bootstrap leaves it: rename `../makepad` out of the way
and run `python3 tools/setup-native.py`, which recreates a missing sibling.
A local branch keeps its files but loses its local history in the first
way, so push it first.

`tools/setup-native.py --update` fetches a new pin with `--depth=1` when
that commit is not already local, which can make a full clone shallow
again. `git fetch origin` beforehand avoids it.

## The four map fixes

Found while bringing up OctosMap on the OnePlus 6T. Fork branch
`fix/android-map-archive`, four commits on `471d680a5`, merged into the
fork's `main` on 2026-09-19 as `OctoSense-org/makepad#15` (`e7c1cdf6c`)
and pinned here the same day. The problems are `BACKLOG.md` MAPS-12 to
MAPS-15; adopting the revision was MAPS-16.

| Commit | What it changes | Where it runs |
|---|---|---|
| `5a9c20b0d` | The GL backend draws nothing for a pass with no draw list, where it used to panic | Linux and Android GL |
| `343053f8a` | Android's `http_cancel` ends the request with an `HttpError`; a request handed to Java reports one `HttpProgress` | Android |
| `136dea82a` | The native GL backend binds compact vertex formats, where it used to skip those draws | Linux and Android GL |
| `d3d740808` | The map's navigation layer clears only the puck it placed | Every platform |

macOS and iOS (Metal), Windows and the web build are untouched by the
first three.

### What they mean for the other apps

- **The GL guard** turns a panic into a no-op, in the case of a frozen
  snapshot pass. A pass that has a parent but no draw list stays dirty and
  logs an error on each repaint, as on Metal; the phone never showed that.
- **Android HTTP** reaches every app that makes requests. News and Mail
  cancel requests, and both drop the request id first and ignore errors for
  ids they no longer own. The image cache and both script network handlers
  (script resources, `sys.*` data fetches, `net.http`) ignore progress
  events and do not treat them as the end of a request. Desktop and iOS
  already delivered both events, so shared code has met them. Not read:
  the fork's `weather`, `asset-ui`, the `route` provisioner and the AI
  backends; none is in the phone build.
- **Compact vertex formats** are used by the map alone
  (`widgets/src/map`). Every other shader takes the packed path with the
  same GL calls as before. Map users on Android and Linux, which are
  OctosMap, the fork's `apps/route` and the AppCard nav card, should now
  get roads and fills, and pay the GPU cost of drawing them. Only OctosMap
  was confirmed.
- **The puck**: `set_puck` is called by OctosMap and by `apps/route`, whose
  puck also never drew. The AppCard nav card places its vehicle through the
  nav modes, not `set_puck`, and behaves as before.

### What was verified

- In the fork: the map's navigation and overlay tests, 14 passing. The full
  suite was not run.
- On the OnePlus 6T with the first two fixes: News and Photos open and load
  as before.
- On the phone with all four: OctosMap only (`docs/maps.md`).
- Not opened on the phone since: the AppCard nav card, Mail, Sheets. The
  nav card shares the map and is the one worth a look before the pin moves.

### Candidates for upstream

Three of the four problems exist in `makepad/makepad` `work` as of
2026-09-18; upstream's own route app should show no tiles and no roads on
an Android phone, which was not tried.

| Fix | Upstream today | As a patch for upstream |
|---|---|---|
| GL no-draw-list panic | The same `unwrap()`, at `opengl.rs:1230`; upstream's Metal backend already has the guard | Needs redoing: `opengl.rs` is modified in the fork, and upstream reworked it on 2026-09-18 (`a67096d20`) |
| Android HTTP cancel and progress | Identical code | Applies as it is: `android_network.rs` and `widgets/src/map/archive.rs` are byte-identical to upstream's |
| GL compact vertex formats | The same gate, with a comment that the compact layout "stays gated" | Needs redoing, for the same reason; ask first, the comment suggests upstream means to do it |
| Navigation layer and the puck | `nav.rs` does not exist upstream | Fork only |

Nothing has been offered upstream. Carried only in the fork, the two GL
patches will conflict on the fork's next sync from upstream.

## The flashing fix

A second fork change, pinned on 2026-09-19 (`BACKLOG.md` MOBILE-08):
`OctoSense-org/makepad#16`, two commits on `e7c1cdf6c`, merged as
`3c82c18f4`; `Octoscript-Makepad#31` named it (`8a7c6b50f`), and one commit
here moved the lock and the six manifests. The GL backend left a draw item out of the frame
when the GPU memory ledger refused its instance buffer, and asked for a
repaint that was refused the same way, so the phone shell flashed between
black, half-drawn and complete frames once a map's tiles were resident and
the activity's surface had been recreated. The backend now charges a draw
item that is being drawn whatever the limit says, as Metal does and as
upstream's GL backend does since `a67096d20`, and it logs, at most once a
second and only while it happens, why it skipped draw items
(`gl: draw items skipped since the last report: …`). That line is silent in
normal use; on a phone that misdraws it is the first thing to read.

Upstream already has the reservation change. The log line is a candidate
for upstream; WebGL and D3D11 in the fork still use the refusable call.

## Adopting a fork revision

The steps MAPS-16 took, for any later pin move. On 2026-09-19 they were
`OctoSense-org/makepad#15`, then `Octoscript-Makepad#28` (two files), then
one commit here (the lock and six manifests, 28 lines):

1. The revision must be fetchable from the fork's URL. A pushed branch
   commit is enough (MOBILE-06 pinned one), but a revision on the fork's
   `main` is the usual choice, so merge the fork pull request first and pin
   its merge commit.
2. In Octoscript-Makepad: the `makepad` revision in `runtime.json` and the
   same `rev` in its crates' manifests, then its lock files. Its
   `tools/runtime.py verify` rejects a manifest that disagrees with
   `runtime.json`.
3. Here: the new Octoscript-Makepad revision in `native-runtime.lock.json`;
   the Makepad `rev` in `Cargo.toml` and in `apps/appcard`, `apps/maps`,
   `apps/news`, `apps/photos` and `apps/reference`. `Cargo.lock` does not
   name the revision, because the patched crates resolve to paths; it
   changes only when the new revision changes a crate's own dependencies,
   which the locked check in the next step reports.
4. `python3 tools/setup-native.py --update`, then
   `python3 tools/setup-native.py --check --cargo-manifest Cargo.toml` and
   `cargo check --locked --workspace --features mobile-apps`.

A worktree inside the repo makes step 4's `--update` and `--check` fail
here for the reason given under "What is pinned"; the siblings are moved
before that failure, and the clean-copy check above stands in for it.
