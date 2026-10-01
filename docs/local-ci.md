# Local CI

`tools/ci-local.sh` runs the checks of `.github/workflows/` (desktop, phone, apps, rom) on your machine, so a pull request can be merged on a local pass while the GitHub macOS runner queue is saturated. GitHub CI stays on: it runs on every push to `main`, including these merges, and a red `main` is fixed before anything else is merged.

## Run it

```sh
python3 tools/setup.py                     # .sources/, as in every session
tools/ci-local.sh --only all               # or desktop | phone | apps | rom, comma-separated
tools/ci-local.sh --only phone --keep-going --jobs 8
tools/ci-local.sh --list                   # the plan; runs nothing
tools/ci-local.sh --check-drift            # the local mapping still fits the workflows
tools/ci-local.sh --only apps --linux-host  # the ubuntu jobs on the Linux build host (below)
```

- **Same commands as GitHub.** The runner reads the workflow files and runs each job's `run:` steps verbatim: same working directory, same `env:`, same `bash --noprofile --norc -eo pipefail`. Nothing is copied, so nothing drifts. The few things it maps (the actions: checkout, setup-python, rust-toolchain, rust-cache, the kernel cache, setup-node; expressions like `${{ runner.temp }}`) are checked by `--check-drift`. Every run checks them too, and so does `tools/test_ci_local.py` (part of the desktop job). A workflow that starts using a new action, job-level `if:`, a matrix or an unknown expression fails that check until `tools/ci_local.py` is taught about it.
- **Output.** Each step is printed as PASS, FAIL, SKIPPED or NOT RUN (after a failure, without `--keep-going`), followed by a summary table with times. The full output goes to `target/ci-local/<timestamp>.log`. `target/ci-local/last.json` records the commit, whether the tree was dirty, the workflows run, and every step's result. The exit status is non-zero on any FAIL.
- **Skips are never passes.** A step that cannot run here is SKIPPED and the summary says why. Today that is the rom product tests, unless a JDK works (`javac -version`, from `PATH` or `JAVA_HOME`; macOS's `/usr/bin/javac` is only a stub), the rom `Check generated Agent Binder client` step, unless the Android SDK has `build-tools;35.0.0` and `platforms;android-35` (found through `ANDROID_HOME`), and the web installer job, unless `node`/`npm`/`npx` are on `PATH`. Without `--linux-host`, jobs GitHub runs on `ubuntu-latest` (apps `services` and `kernel-security`, rom) run on the Mac. Their `#[cfg(target_os = "linux")]` code is not exercised, and the summary notes this.
- **The octos kernel.** The real-kernel steps (phone's app-peers relay and two-lane scenario, apps' `kernel-security`) need `octos` at the revision `Cargo.lock` pins. The workflow's own `Build octos (unless cached)` step builds it once (`tools/kernel-artifact.py --host`). The binary is then kept in a per-user cache that every clone shares, keyed by OS, architecture and octos revision (`~/Library/Caches/octosense-ci-local/octos-kernel/`, `$XDG_CACHE_HOME` or `OCTOSENSE_CI_LOCAL_CACHE`). The copy is written atomically, under a per-revision lock, so two clones never build the same kernel at once.
- **Sharing the machine.** At most two runs execute at once (`--slots N` or `OCTOSENSE_CI_LOCAL_SLOTS`), coordinated by mkdir locks in `${TMPDIR}/octosense-ci-local/` (or `OCTOSENSE_CI_LOCAL_LOCKS`). A run waits for a free slot and says so. With `--no-wait` it exits with status 75 instead. A slot whose owner process has died is taken over. `--jobs N` sets `CARGO_BUILD_JOBS` (default: half the CPUs).
- **Tools.** `cargo` (with clippy and rustfmt) comes from `PATH`, or `~/.cargo/bin` if it is not on `PATH`. `python3` also comes from `PATH`: GitHub uses 3.12, and a different version is noted in the summary. `cmake` is not needed by any of these steps. If a build ever asks for it, put one on `PATH`, e.g. `python3 -m venv <dir> && <dir>/bin/pip install cmake`.

## Run the Linux jobs on the Linux build host

`--linux-host` runs every job whose `runs-on` is ubuntu on the Linux build host configured in `~/.config/octosense/build.env` (`OCTOSENSE_BUILD_HOST`, the ssh target, and `OCTOSENSE_BUILD_KEY`, its key; the environment overrides the file, and `--linux-host <user@host>` names another target). The macOS jobs still run on the Mac, at the same time. It also adds the Linux-only checks no workflow runs: today the process sandbox's Landlock and seccomp tests (`cargo test --locked -p octosense-shell --lib sandbox::`, the `#[cfg(target_os = "linux")]` tests in `crates/shell/src/sandbox/`), as the job `linux-host / sandbox`. They come with any run that includes a workflow a change to the shell triggers (desktop, phone, apps). On a kernel without Landlock the tests skip themselves; that step then fails instead of passing.

```sh
tools/ci-local.sh --only apps --linux-host       # services, kernel-security and the sandbox tests on Linux; apps on the Mac
tools/ci-local.sh --only all --linux-host --list # the plan, with where each job runs
```

- **What runs there.** The exact commit under test (`HEAD`; uncommitted changes stay on the Mac, and the run is marked dirty as usual) goes to the host as a git bundle, incrementally after the first run, into `~/octosense-ci/repo.git`. It is checked out as `~/octosense-ci/runs/<sha>-<timestamp>`, and the host runs `tools/setup.py` itself from the same lock files (its sources are worktrees of hub clones in `~/octosense-ci/cache/hub/`). The newest five run directories are kept. The host then runs this checkout's own `tools/ci_local.py`, so the same `run:` steps run there, extracted from the same workflow files.
- **Results.** Each remote step appears in the same PASS/FAIL/SKIPPED table, marked `(linux)`, and in `last.json` with `"host": "linux"` and `"sha"`, the commit the host checked out. The summary's `linux_host` records that host's OS, architecture, CPUs, tool versions and timings, never its address. A host that cannot be reached, a missing result or a result for another commit is a FAIL, never a pass. The output lines prefixed `linux|` are the host's; each job's full log stays in its run directory under `target/ci-remote/`.
- **Sharing the host.** At most four runs use the host at once (`--linux-slots N` or `OCTOSENSE_CI_LINUX_SLOTS`), through mkdir locks in `~/octosense-ci/locks/` on the host, separate from the Mac's two slots. A run's Linux jobs run in parallel, each with its own cargo target in `~/octosense-ci/cache/target/slot-<n>/<job>` (a warm run reuses the slot's built dependencies; the workspace's own crates and `.sources/` rebuild, since cargo keys them by the run directory's path) and `CARGO_BUILD_JOBS` of the host's CPUs divided by the slots (`--linux-jobs N`). The octos kernel cache is `~/octosense-ci/cache/octos-kernel/`.
- **Security.** The host runs code from pull-request branches of a public repository, as any CI does. Everything runs as the ssh user in `~/octosense-ci/`, never with sudo. The remote steps get an allow-listed environment (`HOME`, `USER`, `LOGNAME`, `LANG`, a fixed `PATH`, and the runner's own variables): no tokens and nothing from the Mac's environment. Never copy credentials there. The host's address and the key path are kept out of the log and `last.json`.
- **One-time setup on the host** (as the ssh user; no sudo): rustup with the stable toolchain and clippy and rustfmt, `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal -c clippy -c rustfmt`, and a Python venv at `~/octosense-ci/venv` (`python3 -m venv --without-pip ~/octosense-ci/venv` when the distribution's `python3-venv` is not installed; the steps need no Python packages). The hub clones are made on the first run. Building the shell (the sandbox tests) needs Makepad's Linux build dependencies, which the host administrator installs (`pkg-config`, `clang`, `libx11-dev`, `libxcursor-dev`, `libxkbcommon-dev`, `libwayland-dev`, `libvulkan-dev`, `libasound2-dev`, `libpulse-dev`, `libgl-dev`). Steps that need more than that (a JDK for rom's product tests, the Android SDK, node for the web installer) are SKIPPED there with the reason, as on the Mac.

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
- for steps run with `--linux-host`: ran on the Linux host on that same head commit. A remote result bound to any other commit is stale and refused. The Linux-only checks (`linux-host / sandbox`) count like the workflows they cover: they block a PR that triggers desktop, phone or apps.

It also refuses while the latest completed GitHub run of a workflow on `main` has failed. Pass `--fixes-main` only for the PR that fixes it. Once every check passes, it posts the summary table as a PR comment ("Local CI passed on `<sha>` …") and runs `gh pr merge <n> --admin --merge --match-head-commit <sha>`, with the subject `Merge pull request #<n> from <owner>/<branch>`.

The merge commit is an ordinary push to `main`, so GitHub CI runs on it. The runs don't pile up: pushes to `main` share one concurrency group per workflow (`desktop-main`, `phone-main`, `apps-main`; rom.yml runs only on rom changes and has no group) with `cancel-in-progress`. Only the newest `main` commit's run completes, and older queued or running `main` runs are cancelled. Pull-request runs keep their own per-PR groups, as before. A cancelled run doesn't count as red, but a failed one does: fix it before merging anything else.
