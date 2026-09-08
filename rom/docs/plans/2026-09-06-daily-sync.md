# One-command Makepad sync

User runs Git pull in the Makepad checkout, then `python3 scripts/upstream.py sync`
from MakeOS. Defaults are the sibling `makepad` checkout and its local HEAD.
Optional source/revision overrides preserve the lower-level workflow.

The command resolves HEAD once, compares the current import, and exits without
building when the revision is unchanged. A real update requires a clean MakeOS
repository. The existing staged three-way merge remains the transaction boundary.

Use `target/makepad-sync/project` as a serialized candidate directory, preserving
only its build cache between attempts. Every invocation that attempts an update
gets a separate report directory containing comparison diffs, verification logs,
runtime artifacts, and a candidate source snapshot on failure. A filesystem lock
prevents concurrent sync invocations from sharing the candidate.

Run existing metadata/check/Rust/Python checks, release and debug workspace
builds, then both smoke modes. Keep the staged provenance revision coordinated
before runtime verification. Override the candidate Cargo target directory to
keep its executable/resource discovery independent of the live project.

After successful verification, recheck the starting Git HEAD/branch and file
snapshot, create a unique sync branch, and apply the verified result. Leave all
changes unstaged and uncommitted, with diff and report instructions. A failure
must not be labeled ready for review as a passing upgrade. Clean up an empty
new branch after a rolled-back apply failure where no user changes intervened.

Test the user-visible CLI defaults/no-op, real Git fixture updates and conflicts,
failed GUI verification, branch collisions, concurrent changes, repeated runs,
cache reuse, lock contention, and exact verification command sequence. Run the
real staged build/smoke verifier on an isolated copy at the current baseline;
do not change the live dependency revision just to test orchestration.
