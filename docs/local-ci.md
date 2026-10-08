# Local CI

`tools/ci-local.sh` runs the checks of the desktop, phone, apps and rom workflows locally. When the GitHub macOS runner queue is saturated, a pull request may be [merged on a local pass](#merge-on-a-local-pass). GitHub CI stays on and runs on these merges too; a red `main` is fixed before anything else is merged.

## Run it

```sh
python3 tools/setup.py                     # .sources/, as in every session
tools/ci-local.sh --only all               # or desktop | phone | apps | rom, comma-separated
tools/ci-local.sh --only phone --keep-going --jobs 8
tools/ci-local.sh --list                   # the plan; runs nothing
tools/ci-local.sh --check-drift            # the local mapping still fits the workflows
```

**The same commands as GitHub.** `tools/ci_local.py` reads `.github/workflows/` and runs each job's `run:` steps verbatim, with the step's working directory and `env:`, in GitHub's `bash --noprofile --norc -eo pipefail`. Nothing is copied, so nothing drifts. GitHub's actions map to local stand-ins: the working tree, this clone's `target/`, a shared kernel cache and the tools on `PATH`. `--check-drift`, every run and `tools/test_ci_local.py` check that mapping: a workflow that starts using something the runner does not model, such as a new action or a matrix, fails until `tools/ci_local.py` learns it.

**One workflow is left out:** `release-desktop.yml`, a release workflow with a matrix, environments and secrets (`NOT_LOCAL` gives the reason). The desktop job runs its packaging scripts' tests. GitHub still runs its `plan` and `package` jobs on pull requests that change the packaging; the merge script ignores them.

**Output.** Each step prints PASS, FAIL, SKIPPED or NOT RUN (after a failure, without `--keep-going`), then a summary table with times. The full log goes to `target/ci-local/<timestamp>.log`, and `target/ci-local/last.json` records the commit, whether the tree was dirty, the workflows run and each step's result. Any FAIL makes the exit status non-zero.

**Skips are never passes.** A step that cannot run here is SKIPPED with the reason. A run with skips still exits 0, but in a workflow the pull request triggers, the merge refuses a skip like a failure. On a Mac that usually means rom:

- its product tests need a JDK (`javac -version` must work, from `PATH` or `JAVA_HOME`; macOS's `/usr/bin/javac` is only a stub);
- `Check generated Agent Binder client` needs the Android SDK's `build-tools;35.0.0` and `platforms;android-35` (`ANDROID_HOME`);
- the web installer needs `node`, `npm` and `npx`.

Without `--linux-host`, the ubuntu jobs (apps' `services` and `kernel-security`, both rom jobs) run on the Mac, and the summary notes that the apps jobs' `#[cfg(target_os = "linux")]` code went untested.

**The octos kernel.** The real-kernel steps in phone and apps need `octos` at the revision `Cargo.lock` pins. The workflow's `Build octos (unless cached)` step builds it once (`tools/kernel-artifact.py --host`), and the runner keeps it in a per-user cache that every clone shares (`~/Library/Caches/octosense-ci-local/octos-kernel/` on macOS). A lock per revision stops two clones from building it at once.

**Sharing the machine.** At most two runs execute at once (`--slots N`). Another run waits and says so, or with `--no-wait` exits with status 75. The slots are mkdir locks in `$TMPDIR/octosense-ci-local/`, taken over when their owner has died. A second run in the same clone exits with 75 at once. `--jobs N` sets `CARGO_BUILD_JOBS` (default: half the CPUs).

## Run the Linux jobs on the Linux build host

`--linux-host` sends every job whose `runs-on` is ubuntu to the Linux build host set in `~/.config/octosense/build.env`: `OCTOSENSE_BUILD_HOST` is the ssh target and `OCTOSENSE_BUILD_KEY` its key. The environment overrides the file, and `--linux-host <user@host>` names another target. The macOS jobs stay on the Mac, in parallel.

It also adds a Linux-only check, which no workflow runs: the process sandbox's Landlock and seccomp tests (`cargo test --locked -p octosense-shell --lib sandbox::`), as the job `linux-host / sandbox`, in any run that includes desktop, phone or apps. On a kernel without Landlock the tests skip themselves, so the step fails instead of passing.

```sh
tools/ci-local.sh --only apps --linux-host       # services, kernel-security and the sandbox tests on Linux; apps on the Mac
tools/ci-local.sh --only all --linux-host --list # the plan, with where each job runs
```

**What runs there.** The runner ships the commit under test (`HEAD`, without uncommitted changes) to `~/octosense-ci/repo.git` as an incremental git bundle and checks it out in `~/octosense-ci/runs/<sha>-<timestamp>`, keeping the newest five. The host runs `tools/setup.py` from the same lock files, then that checkout's own `tools/ci_local.py`, so the same `run:` steps run.

**Results.** Remote steps appear in the same table, marked `(linux)`, and in `last.json` with the `"sha"` the host checked out. An unreachable host, a missing result or a result for another commit is a FAIL. Each job's full log stays in its run directory on the host (`target/ci-remote/`).

**Sharing the host.** At most four runs use the host at once (`--linux-slots N`), through locks in `~/octosense-ci/locks/`. A run's Linux jobs run in parallel, each with its own cargo target and `CARGO_BUILD_JOBS` of the host's CPUs divided by the slots (or `--linux-jobs N`).

**Security.** The host runs code from pull-request branches. Everything runs as the ssh user in `~/octosense-ci/`, never with sudo, and remote steps get an allow-listed environment (`HOME`, `USER`, `LOGNAME`, `LANG`, a fixed `PATH` and the runner's own variables): no tokens and nothing from the Mac. Never copy credentials there. The log and `last.json` show `<linux-host>` in place of the host's address and key path.

**One-time setup on the host**, as the ssh user and without sudo: rustup with the stable toolchain, clippy and rustfmt, and a Python venv at `~/octosense-ci/venv`, first on the remote `PATH`. The steps need no Python packages, so `--without-pip` is enough, even without the distribution's `python3-venv`.

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal -c clippy -c rustfmt
python3 -m venv --without-pip ~/octosense-ci/venv
```

The first run clones the source hubs into `~/octosense-ci/cache/hub/`. Building the shell for the sandbox tests needs Makepad's [Linux build dependencies](https://github.com/OctoSense-org/makepad#linux-dependencies), which the host's administrator installs. Steps that need a JDK, the Android SDK or node are SKIPPED there too.

## Offload the macOS jobs to the Linux host

`--offload`, with `--linux-host`, also sends the macOS jobs' portable steps to the Linux host. The Mac keeps only the steps that check macOS itself, so a run spends a few minutes here instead of most of an hour, on a host with many more cores and its own slots:

```sh
tools/ci-local.sh --only all --linux-host --offload
tools/ci-local.sh --only all --linux-host --offload --list   # what runs where
```

`JOBS` in `tools/ci_local.py` says which steps stay here (`mac_steps`):

| Job (GitHub: macos-14) | Stays on the Mac | Goes to the Linux host |
| --- | --- | --- |
| desktop / `desktop` | Compile the desktop; desktop tools and scripts | The shell-source, single-graph and native-apps checks |
| desktop / `native-host-api` | All of it (a real macOS host) | Nothing |
| phone / `home` | Apple icons and asset catalogs; compile Home | The graph checks, the octos kernel, Home's and App Hub's tests, the two-lane scenario |
| apps / `apps` | Nothing | All of it |

Each part prepares the sources itself (the job's `python3 tools/setup.py`). The host's part runs as `<job>@linux`; its steps join the table under the job, marked `(linux)`. `--check-drift` fails when a name in `mac_steps` stops matching a step.

**What an offloaded pass means.** GitHub runs these jobs on macOS, so an offloaded run tests their Linux build: code under `#[cfg(target_os = "macos")]` is checked by the compile steps that stay here, but not tested, and code under `#[cfg(target_os = "linux")]` is tested in its place. `last.json` records `"offload": true`, and the merge comment says which steps ran where. GitHub CI still runs the macOS jobs on the merge commit on `main`.

## Merge on a local pass

```sh
git fetch origin && git rebase origin/main   # the head must contain current main
git push --force-with-lease                  # your branch, never main
tools/ci-local.sh --only all                 # on that exact head, clean tree
tools/ci-local-merge.sh <PR number>          # --dry-run to preview
```

`tools/ci-local-merge.sh` merges only an open, non-draft pull request against `main`, and only if `target/ci-local/last.json`:

- ran on the PR's exact head commit, with a clean tree;
- comes from a head that contains the current `origin/main`;
- covers each workflow GitHub would run for the PR's files (their `pull_request` `paths`), with no FAIL, NOT RUN or SKIPPED step in them or in the drift check. Other workflows don't count, so `--only desktop,phone` is enough for a PR that triggers only those two;
- binds every `--linux-host` step to that same commit; a remote result for another commit is stale. The `linux-host / sandbox` checks count like the workflows they cover when present, but a run without them is not refused.

It also refuses while the newest finished push run of a workflow on `main` has failed (cancelled runs don't count). Pass `--fixes-main` only for the PR that fixes it. Then it posts the summary table as a PR comment ("Local CI passed on `<sha>` …") and runs `gh pr merge <n> --admin --merge --match-head-commit <sha>`, with the subject `Merge pull request #<n> from <owner>/<branch>`. `--dry-run` prints the comment instead of posting it, and merges nothing.

On `main`, pushes share one concurrency group per workflow (`desktop-main`, `phone-main`, `apps-main`) that cancels older runs, so the merges don't pile up and only the newest commit's run completes. `rom.yml`, which few changes trigger, has no group.

## Reading the code

- [`tools/ci_local.py`](../tools/ci_local.py): the runner. `ACTIONS` maps GitHub actions to local stand-ins, `JOBS` lists every job the runner knows, `STEP_REQUIREMENTS` what a step needs (a JDK, the Android SDK), `NOT_LOCAL` the workflows left out, and `LINUX_HOST_JOBS` the extra Linux checks.
- [`tools/ci_local_merge.py`](../tools/ci_local_merge.py): the merge's checks (`evidence_problems`, `red_main`).
- [`tools/test_ci_local.py`](../tools/test_ci_local.py): the tests of both, run in the desktop job.
