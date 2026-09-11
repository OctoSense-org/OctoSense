# Updating the Makepad import

MakeOS maintains the WM source, icons and bundled wallpaper plus the original license notice.
`upstream/makepad.json` records their original paths and SHA-256 hashes at one
full Makepad commit. The hashes describe **pristine upstream content**, so local
MakeOS adaptations do not require changing them. Framework and hosted-app
dependencies use that same commit. Do not independently change their revisions.

The active source is `https://github.com/makepad/makepad.git`, at
`74b63be83e101ab3a28d3604df77e9662d50a833` on the `work` branch. The local
source checkout is `../makepad`. `makepad-wm-api` (`libs/wm_api`) and
`makepad-wm-theme` (`libs/wm_theme`) are pinned Git dependencies alongside
widgets, platform, app-module and the linked app crates. They are not copied
into MakeOS. Advancing the shared pin includes their changes and their required
transitive dependencies. Unrelated monorepo sources are not imported here.

MakeOS's extra style is implemented in `src/makeos/style.rs` with two small
local theme files. It sends its complete palette/material using the recognized
`macos-dark` wire family, allowing unmodified upstream apps to select the right
icons and appearance. The local wallpaper widget preserves SVG cover behavior;
upstream already supplies the cached-view APIs. `src/makeos/retired_passes.rs`
detaches passes with freed draw-list roots before the new retained GPU working-set scan;
remove it when upstream guards retired pass slots. No framework fork is required.

The previous `guofoo/makepad` additions are retained as local WM adaptations.
The one-time migration compared official changes from the common ancestor,
rather than interpreting absent fork features as upstream deletions. Historical
asset origins are recorded in `upstream/makepad.json` under
`retained_fork_assets`; these are MakeOS-owned and are not ongoing upstream
file mappings. Future syncs use the official baseline and normal three-way
merge. See `docs/plans/2026-09-11-official-work-sync.md` for the migration scope.

## Daily command

After updating the Makepad checkout, run from MakeOS:

```sh
git -C ../makepad pull --ff-only origin work
python3 scripts/upstream.py sync
```

Run this at least daily during active development, or more often after upstream
changes. The first command is your source-repository Git step; `sync` performs
the remaining comparison, preparation, and verification without prompts. No
scheduled job is installed.

Defaults are the provenance file's `default_source` (`../makepad`) and its
current local `HEAD`. Relative recorded paths resolve from the MakeOS project
root, independent of the invoking shell's working directory. Older provenance
without this field retains `../makepad`. An explicit `--source` overrides it;
use that option from a nested worktree whose sibling location differs.
The command resolves the target once, so another pull during verification does
not change the candidate. `--source /path/to/makepad` and `--to <commit-or-ref>`
override those defaults. The chosen commit must be fetchable from the pinned
Makepad Git dependency URL; local unpublished commits cannot form a portable
MakeOS upgrade. Cargo will report a resolution failure for an unavailable pin.

An unchanged revision is a fast successful no-op, including when MakeOS has
uncommitted work; it reports that work separately. If upstream has advanced,
MakeOS must have a clean working tree first. Complete review/commit of the
previous update before applying another one. A nonzero result means the new
candidate has not passed the workflow.

For an actual update, the command:

1. Saves the source comparison and resolves the three-way merge in
   `target/makepad-sync/project`. Only that candidate's `target/` build cache
   survives between attempts; its source is rebuilt from tracked MakeOS files.
2. Advances the candidate's provenance, all Makepad dependency pins, and lockfile
   together. Runs Cargo metadata, locked workspace check/tests, and the Python
   maintenance tests.
3. Builds release and debug workspace binaries, then runs the release hosting
   smoke test with `--styles` and the exact `cargo run` test with the shipped catalog. The release test also switches through all eight styles, captures MakeOS glass and menus, and checks that the hosted app retains its state without background launches. Tests
   open and close their own windows and isolate user state. The command needs
   native GUI access and is currently validated on macOS.
4. Rechecks the starting MakeOS HEAD, branch, and files. Only after verification
   passes, creates `sync/makepad-<12-character-revision>` (with a numeric suffix
   if necessary) and applies the candidate. Existing branches are never reused
   or overwritten.
5. Prints `READY FOR REVIEW` and the report directory. Changes remain unstaged
   and uncommitted; review, commit, merge, and push belong to you.

Use the printed review branch and report to finish:

```sh
git status --short
git diff --stat
git diff
```

`git status` also shows new upstream files that an unstaged `git diff` does not
include. Review `comparison.txt`, `verification.log`, and the PNGs/logs in
`smoke-release/` and `smoke-default/`, then commit the complete upgrade. The
runtime checks verify interaction and lifecycle behavior; the captured frames
remain available for your visual review.

Each attempt retains a unique report under `target/makepad-sync/reports/`.
A failed merge/check preserves the live import and original branch, with a
candidate source snapshot under the report's `project/`. Review that snapshot
before rerunning; it is separate from the reusable staging area. If storage
errors prevent archiving, the error identifies the retained cache location.
An ordinary apply failure rolls back files and restores the original branch
when no concurrent user changes intervene. Recovery failures are reported.

A process lock serializes syncs using the same cache. Ctrl-C forwards one
interrupt to the active verification command and waits for its cleanup before
releasing that lock. Reports and the build cache are Git-ignored; keep useful
reports for review and remove old ones when no sync is running. The first sync
build may take longer; subsequent syncs reuse Cargo's incremental artifacts.

## Manual comparison and update

The maintenance script needs Python 3.11+, Git, Cargo, and an existing Makepad
clone containing both commits. Obtain new commits in that clone separately.
The script reads Git objects; it never checks out files, fetches, or changes the
clone's working tree, index, or refs. Uncommitted changes in that clone are not
part of an import.

```sh
python3 scripts/upstream.py status
python3 scripts/upstream.py status --to <commit>
python3 scripts/upstream.py diff --to <commit>
```

`status` classifies local adaptations and upstream changes, checks provenance
hashes, and checks every Makepad Git dependency pin in Cargo manifests and the
lockfile. `diff` also prints separate old-upstream-to-local and
old-upstream-to-new-upstream diffs. Both are read-only with respect to MakeOS and
the source clone. Exit status is 0 for a valid comparison, 1 for conflicts or
provenance problems, and 2 for operational errors. File changes alone are not an
error. Changes outside the WM subtree are listed for framework/API review; they
are not copied wholesale.

Commit MakeOS changes before running an update, including its current baseline
and lockfile:

```sh
python3 scripts/upstream.py update --to <commit>
git diff --stat
git diff
cargo run --locked
```

`update` performs these steps:

1. Require a clean MakeOS Git working tree, including no untracked files, and
   validate the existing hashes, import inventory, and dependency revisions.
2. Copy tracked project files into a disposable staging directory. Compare old
   Makepad, current MakeOS, and new Makepad. Merge independent text edits;
   preserve local-only changes. Treat conflicting edits, changed binary files,
   deletion of locally modified files, and new-file destination collisions as
   conflicts. Collisions include ignored files and directories. New files under
   `apps/wm/` map to the same relative path in MakeOS; deletions remove unchanged
   imported files. Renames appear as additions and deletions.
3. Change matching Makepad Git revisions in all staged Cargo manifests,
   including the reference app, and generate the candidate provenance baseline.
   Run `cargo metadata --format-version 1` to
   resolve the staged lockfile, then `cargo check --locked --workspace`,
   `cargo test --locked --workspace --quiet`, and the Python maintenance tests.
   Check the resulting manifest and lock revisions
   again. Cargo may download dependencies; all compilation happens in the
   staging directory. The ordinary Cargo cache is shared.
4. After verification succeeds, check that MakeOS has not changed during
   verification. Apply the staged source,
   Cargo manifests, and lockfile, writing the baseline last. Files are replaced
   atomically; an ordinary write failure rolls back previous writes. The Git
   index is unchanged so the result remains available for review.

Run the host/client GUI smoke tests before committing the update: desktop
startup, icons/fonts/theme, launch the reference app, keyboard/pointer input,
resize, and close. Use `cargo build --release --locked --workspace` followed by
`python3 scripts/smoke.py --styles` and `python3 scripts/smoke.py --cargo-run --default-catalog`.
The automatic compile check and Rust tests do not establish GUI or
protocol behavior. Commit the reviewed source, manifests, lockfile, and baseline
together. Use a MakeOS Git revert to roll back a committed upgrade.

If a merge or verification fails, the live project and baseline remain intact.
The script prints the retained temporary directory containing `project/`,
`comparison.txt`, and, if verification ran, `verification.log`. Inspect those
files to understand the failure. There is intentionally no command that
blindly applies a retained stage. Resolve local adaptations in MakeOS while
keeping the old baseline, commit the resolution, and rerun `update` against the
same target. For an overlapping edit, adopting the intended upstream lines in
the affected local region before rerunning allows the next three-way merge to
recognize that change. A larger adaptation may need a deliberate manual merge
and review. Preserve the old hashes until the script completes successfully.

An upstream file at the recorded baseline that is neither imported nor omitted
is an error. Deliberate omissions are exact source paths in `omissions`, either
strings or objects such as `{"source": "apps/wm/example", "reason": "..."}`.
Local files already occupying a newly added upstream path need a deliberate
rename or an omission before retrying. Unsupported Git file types, symlinks in
import paths, unsafe paths, and dependency syntax the script cannot safely
rewrite are rejected for manual review.

The transaction protects against merge/check failures and ordinary write
errors; it is not a filesystem-wide atomic transaction against power loss or
forced process termination during the final apply. A clean starting commit
provides the recovery point. If storage errors also prevent rollback, the tool
explicitly reports that manual recovery is required and that live files may
differ. Keep unrelated edits out of the tree while an update runs. Temporary
stages can be deleted after investigation.

Run the offline maintenance fixtures with:

```sh
python3 -m unittest discover -s scripts -p 'test_*.py'
```

The fixtures create local Git repositories and use an injected verifier or a
fake Cargo executable. They never access the network or build Makepad.
