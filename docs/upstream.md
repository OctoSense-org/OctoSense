# Updating the Makepad import

MakeOS maintains the WM source and icons plus the original license notice.
`upstream/makepad.json` records their original paths and SHA-256 hashes at one
full Makepad commit. The hashes describe **pristine upstream content**, so local
MakeOS adaptations do not require changing them. Framework and hosted-app
dependencies use that same commit. Do not independently change their revisions.

The maintenance script needs Python 3.11+, Git, Cargo, and an existing Makepad
clone containing both commits. Obtain new commits in that clone separately.
The script reads Git objects; it never checks out files, fetches, or changes the
clone's working tree, index, or refs. Uncommitted changes in that clone are not
part of an import.

```sh
python3 scripts/upstream.py status --source ../makepad
python3 scripts/upstream.py status --source ../makepad --to <commit>
python3 scripts/upstream.py diff --source ../makepad --to <commit>
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
python3 scripts/upstream.py update --source ../makepad --to <commit>
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
   including the reference app. Run `cargo metadata --format-version 1` to
   resolve the staged lockfile, then `cargo check --locked --workspace`,
   `cargo test --locked --workspace --quiet`, and the Python maintenance tests.
   Check the resulting manifest and lock revisions
   again. Cargo may download dependencies; all compilation happens in the
   staging directory. The ordinary Cargo cache is shared.
4. After verification succeeds, generate the new provenance baseline and check
   that MakeOS has not changed during verification. Apply the staged source,
   Cargo manifests, and lockfile, writing the baseline last. Files are replaced
   atomically; an ordinary write failure rolls back previous writes. The Git
   index is unchanged so the result remains available for review.

Run the host/client GUI smoke tests before committing the update: desktop
startup, icons/fonts/theme, launch the reference app, keyboard/pointer input,
resize, and close. Use `cargo build --release --locked --workspace` followed by
`python3 scripts/smoke.py` and `python3 scripts/smoke.py --cargo-run --default-catalog`.
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
