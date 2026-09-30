# Local CI

`tools/ci-local.sh` runs the checks of `.github/workflows/` (desktop, phone, apps, rom) on your machine, so a pull request can be merged on a local pass while the GitHub macOS runner queue is saturated. GitHub CI stays on: it runs on every push to `main`, including these merges, and a red `main` is fixed before anything else is merged.

## Run it

```sh
python3 tools/setup.py                     # .sources/, as in every session
tools/ci-local.sh --only all               # or desktop | phone | apps | rom, comma-separated
tools/ci-local.sh --only phone --keep-going --jobs 8
tools/ci-local.sh --list                   # the plan; runs nothing
tools/ci-local.sh --check-drift            # the local mapping still fits the workflows
```

- **Same commands as GitHub.** The runner reads the workflow files and runs each job's `run:` steps verbatim: same working directory, same `env:`, same `bash --noprofile --norc -eo pipefail`. Nothing is copied, so nothing drifts. The few things it maps (the actions: checkout, setup-python, rust-toolchain, rust-cache, the kernel cache, setup-node; expressions like `${{ runner.temp }}`) are checked by `--check-drift`. Every run checks them too, and so does `tools/test_ci_local.py` (part of the desktop job). A workflow that starts using a new action, job-level `if:`, a matrix or an unknown expression fails that check until `tools/ci_local.py` is taught about it.
- **Output.** Each step is printed as PASS, FAIL, SKIPPED or NOT RUN (after a failure, without `--keep-going`), followed by a summary table with times. The full output goes to `target/ci-local/<timestamp>.log`. `target/ci-local/last.json` records the commit, whether the tree was dirty, the workflows run, and every step's result. The exit status is non-zero on any FAIL.
- **Skips are never passes.** A step that cannot run here is SKIPPED and the summary says why. Today that is the rom product tests, unless a JDK works (`javac -version`, from `PATH` or `JAVA_HOME`; macOS's `/usr/bin/javac` is only a stub), the rom `Check generated Agent Binder client` step, unless the Android SDK has `build-tools;35.0.0` and `platforms;android-35` (found through `ANDROID_HOME`), and the web installer job, unless `node`/`npm`/`npx` are on `PATH`. Jobs GitHub runs on `ubuntu-latest` (apps `services` and `kernel-security`, rom) run on the Mac. Their `#[cfg(target_os = "linux")]` code is not exercised, and the summary notes this. There is no Linux-host mode yet.
- **The octos kernel.** The real-kernel steps (phone's app-peers relay and two-lane scenario, apps' `kernel-security`) need `octos` at the revision `Cargo.lock` pins. The workflow's own `Build octos (unless cached)` step builds it once (`tools/kernel-artifact.py --host`). The binary is then kept in a per-user cache that every clone shares, keyed by OS, architecture and octos revision (`~/Library/Caches/octosense-ci-local/octos-kernel/`, `$XDG_CACHE_HOME` or `OCTOSENSE_CI_LOCAL_CACHE`). The copy is written atomically, under a per-revision lock, so two clones never build the same kernel at once.
- **Sharing the machine.** At most two runs execute at once (`--slots N` or `OCTOSENSE_CI_LOCAL_SLOTS`), coordinated by mkdir locks in `${TMPDIR}/octosense-ci-local/` (or `OCTOSENSE_CI_LOCAL_LOCKS`). A run waits for a free slot and says so. With `--no-wait` it exits with status 75 instead. A slot whose owner process has died is taken over. `--jobs N` sets `CARGO_BUILD_JOBS` (default: half the CPUs).
- **Tools.** `cargo` (with clippy and rustfmt) comes from `PATH`, or `~/.cargo/bin` if it is not on `PATH`. `python3` also comes from `PATH`: GitHub uses 3.12, and a different version is noted in the summary. `cmake` is not needed by any of these steps. If a build ever asks for it, put one on `PATH`, e.g. `python3 -m venv <dir> && <dir>/bin/pip install cmake`.

## Merge on a local pass

```sh
git fetch origin && git rebase origin/main   # the head must contain current main
git push --force-with-lease                  # your branch, never main
tools/ci-local.sh --only all                 # on that exact head, clean tree
tools/ci-local-merge.sh <PR number>          # --dry-run to preview
```

`tools/ci-local-merge.sh` refuses unless `target/ci-local/last.json`:

- passed on the PR's exact head commit, with a clean tree;
- comes from a head that contains the current `origin/main`;
- covers every workflow GitHub would run for the PR's files (their `pull_request` `paths`), with no FAIL, no NOT RUN and no unexpected SKIP in them. A workflow GitHub would not run for the PR does not block it, so `--only desktop,phone` is enough evidence for a PR that only triggers those two.

It also refuses while the latest completed GitHub run of a workflow on `main` has failed. Pass `--fixes-main` only for the PR that fixes it. Once every check passes, it posts the summary table as a PR comment ("Local CI passed on `<sha>` …") and runs `gh pr merge <n> --admin --merge --match-head-commit <sha>`, with the subject `Merge pull request #<n> from <owner>/<branch>`.

The merge commit is an ordinary push to `main`, so GitHub CI runs on it. The runs don't pile up: pushes to `main` share one concurrency group per workflow (`desktop-main`, `phone-main`, `apps-main`; rom.yml runs only on rom changes and has no group) with `cancel-in-progress`. Only the newest `main` commit's run completes, and older queued or running `main` runs are cancelled. Pull-request runs keep their own per-PR groups, as before. A cancelled run doesn't count as red, but a failed one does: fix it before merging anything else.
