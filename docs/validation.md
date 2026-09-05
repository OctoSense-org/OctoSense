# Initial extraction validation

Validated on macOS with Rust/Cargo 1.98.1 against Makepad commit
`83a00d2801e4864c42c3a40e85186a8b1743fd84`.

## Automated checks

- `cargo build --release --locked --workspace`: host and reference app built.
- `cargo test --locked --workspace --quiet`: 163 passed.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 24 passed.
- `python3 scripts/upstream.py status --source ../makepad`: all 70 imported
  file hashes and coordinated Cargo pins verified; 11 adapted files and 59
  unchanged files at the initial baseline.

The maintenance tests use disposable local repositories to check additions,
deletions, independent edits, conflicts, dependency pinning, staged verification,
write rollback, and concurrent edits. No upgrade to a newer real Makepad
revision has been performed.

## Native runtime checks

`python3 scripts/smoke.py` passed against the release build:

- Desktop starts with no child apps, using isolated state.
- Reference launches through Cargo into a MakeOS tile.
- Pointer input sent to the host increments the child's counter.
- Text input sent to the host updates the child's echo label.
- Moving to another workspace leaves the original workspace empty.
- Fullscreen expands the child's reported size and restores it afterward.
- A second reference process starts with independent counter state.
- Closing that tile reaps its process while the first instance survives.
- An invalid Cargo package produces a desktop notification and diagnostic log;
  the host remains responsive.
- Host quit reaps both a running app and the process group of an unfinished
  Cargo build, including its build-script process.

`python3 scripts/smoke.py --cargo-run --default-catalog` also passed from this
repository and from a temporary source copy outside the sibling Makepad layout.
The latter used its own target directory, seeded with compiled artifacts to
reuse the build cache. Cargo dependency caches were shared. The host and child
ran from the temporary project; no sibling Makepad checkout was needed.

Smoke tests run Cargo offline after initial dependency fetching, and set only
test isolation/remote-control settings. App registration and normal launching
require no such settings. App-provided frames were inspected for the desktop,
icons, fonts, hosted content, and failure notification. Test instances were
closed after verification.

Runtime inspection also found and corrected the inherited run-view area
selection: widget queries now follow the presented app surface after the
temporary startup backdrop stops drawing.

## Scope

Source development on macOS is verified. Linux/Windows code and optional linked
app modules are retained but untested in this milestone. Fonts and other
framework assets remain in Cargo's dependency checkout; executable-only or
installer distribution needs separate resource packaging.
