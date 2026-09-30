#!/usr/bin/env python3
"""Run the GitHub workflows' checks on this machine (Python 3.9+).

`tools/ci-local.sh` runs this. It reads .github/workflows/*.yml and runs every
`run:` step of their jobs verbatim, with the step's working directory, `env:`
and the bash GitHub uses (`bash --noprofile --norc -eo pipefail`), so the
commands cannot drift from the workflows. What GitHub does with an action
(`uses:`) is mapped locally:

  actions/checkout, actions/setup-python   the working tree as it is
  dtolnay/rust-toolchain                   the cargo on PATH (~/.cargo/bin is
                                           prepended); its components must exist
  Swatinem/rust-cache                      the clone's own target/
  actions/cache (the octos kernel)         a per-user cache shared by every
                                           clone, keyed by the octos revision
  actions/setup-node                       the node/npm on PATH

A step that cannot run here is SKIPPED with the reason, never passed. Jobs
GitHub runs on ubuntu-latest run on this Mac; `#[cfg(target_os = "linux")]`
code in them is not exercised here (the summary says so).

  tools/ci-local.sh [--only desktop|phone|apps|rom|all[,...]] [--jobs N] [--keep-going] [--no-wait]
  tools/ci-local.sh --list              # the plan, nothing run
  tools/ci-local.sh --check-drift       # the local mapping still fits the workflows
  tools/ci-local-merge.sh <PR number>   # merge on a local pass (see docs/local-ci.md)

Heavy runs share the machine: at most OCTOSENSE_CI_LOCAL_SLOTS (default 2)
run at once, through mkdir locks in ${TMPDIR}/octosense-ci-local/; a run waits
for a slot (or, --no-wait, exits 75). Results: target/ci-local/<timestamp>.log
and target/ci-local/last.json.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / ".github/workflows"
GROUPS = {"desktop": ["desktop.yml"], "phone": ["phone.yml"], "apps": ["apps.yml"], "rom": ["rom.yml"]}
GROUPS["all"] = [w for g in ("desktop", "phone", "apps", "rom") for w in GROUPS[g]]
KERNEL_BINARY = "octos-kernel/target/release/octos"
EXIT_BUSY = 75

# ---------------------------------------------------------------------------
# A small YAML reader for GitHub workflow files (no PyYAML in CI's Python).
# Block mappings and sequences, `|`/`|-`/`>` block scalars, quoted and plain
# scalars, flow sequences of scalars and comments: what workflows use.
# tools/test_ci_local.py compares it with PyYAML when that is installed.
# ---------------------------------------------------------------------------


class YamlError(ValueError):
    pass


def _indent(line):
    return len(line) - len(line.lstrip(" "))


def _strip_comment(text):
    quote = None
    for i, ch in enumerate(text):
        if quote:
            if ch == quote:
                quote = None
        elif ch in "'\"" and (i == 0 or text[i - 1] in " [,:"):
            quote = ch
        elif ch == "#" and (i == 0 or text[i - 1] in " \t"):
            return text[:i].rstrip()
    return text.rstrip()


def _split_key(text):
    """('key', rest) for `key: rest` / `key:`, or None for a scalar."""
    if text[:1] in "'\"":
        end = text.find(text[0], 1)
        if end > 0 and text[end + 1:end + 2] == ":" and text[end + 2:end + 3] in ("", " "):
            return text[1:end], text[end + 2:].strip()
        return None
    match = re.match(r"([^\s:#][^:#]*?)\s*:(?:\s+|$)(.*)$", text)
    if not match or text.startswith(("[", "{")):
        return None
    return match.group(1), match.group(2)


def _scalar(text):
    text = _strip_comment(text)
    if text.startswith("'") and text.endswith("'") and len(text) >= 2:
        return text[1:-1].replace("''", "'")
    if text.startswith('"') and text.endswith('"') and len(text) >= 2:
        return json.loads(text)
    if text.startswith("[") and text.endswith("]"):
        inner = text[1:-1].strip()
        return [_scalar(part.strip()) for part in inner.split(",")] if inner else []
    if text.startswith("{") and text.endswith("}"):
        inner = text[1:-1].strip()
        out = {}
        for part in filter(None, (p.strip() for p in inner.split(","))):
            key, value = _split_key(part)
            out[key] = _scalar(value)
        return out
    if text in ("true", "True"):
        return True
    if text in ("false", "False"):
        return False
    if text in ("", "~", "null"):
        return None
    if re.fullmatch(r"-?\d+", text):
        return int(text)
    if re.fullmatch(r"-?\d+\.\d+", text):
        return float(text)
    return text


class _Reader:
    def __init__(self, text):
        self.lines = text.replace("\t", "    ").splitlines()
        self.i = 0

    def skip(self):
        while self.i < len(self.lines):
            stripped = self.lines[self.i].strip()
            if stripped and not stripped.startswith("#"):
                return True
            self.i += 1
        return False

    def node(self, indent):
        if not self.skip():
            return None
        line = self.lines[self.i]
        if _indent(line) < indent:
            return None
        stripped = line.strip()
        if stripped == "-" or stripped.startswith("- "):
            return self.sequence(_indent(line))
        return self.mapping(_indent(line))

    def mapping(self, indent):
        out = {}
        while self.skip():
            line = self.lines[self.i]
            current = _indent(line)
            if current < indent:
                break
            if current > indent:
                raise YamlError(f"line {self.i + 1}: unexpected indent")
            stripped = line.strip()
            if stripped == "-" or stripped.startswith("- "):
                break
            split = _split_key(stripped)
            if split is None:
                raise YamlError(f"line {self.i + 1}: expected `key: value`")
            key, rest = split
            self.i += 1
            out[key] = self.value(rest, indent)
        return out

    def value(self, rest, indent):
        head = _strip_comment(rest)
        if head[:1] in ("|", ">"):
            return self.block_scalar(indent, head)
        if head == "":
            if not self.skip():
                return None
            line = self.lines[self.i]
            nested = _indent(line)
            is_item = line.strip() == "-" or line.strip().startswith("- ")
            if nested > indent or (nested == indent and is_item):
                return self.node(nested)
            return None
        return _scalar(rest)

    def block_scalar(self, indent, header):
        body = []
        while self.i < len(self.lines):
            line = self.lines[self.i]
            if line.strip() and _indent(line) <= indent:
                break
            body.append(line)
            self.i += 1
        while body and not body[-1].strip():
            body.pop()
        widths = [_indent(line) for line in body if line.strip()]
        width = min(widths) if widths else 0
        lines = [line[width:] if line.strip() else "" for line in body]
        if header.startswith(">"):
            text = " ".join(lines)
        else:
            text = "\n".join(lines)
        return text if header.endswith("-") or not text else text + "\n"

    def sequence(self, indent):
        out = []
        while self.skip():
            line = self.lines[self.i]
            current = _indent(line)
            stripped = line.strip()
            if current != indent or not (stripped == "-" or stripped.startswith("- ")):
                if current > indent:
                    raise YamlError(f"line {self.i + 1}: unexpected indent")
                break
            content = stripped[1:].lstrip()
            if not content or content.startswith("#"):
                self.i += 1
                out.append(self.node(indent + 1))
            elif _split_key(content) is not None:
                column = current + (len(stripped) - len(content))
                self.lines[self.i] = " " * column + content
                out.append(self.mapping(column))
            else:
                self.i += 1
                out.append(_scalar(content))
        return out


def load_yaml(text):
    reader = _Reader(text)
    value = reader.node(0)
    if reader.skip():
        raise YamlError(f"line {reader.i + 1}: trailing content")
    return value


def load_workflow(name):
    return load_yaml((WORKFLOWS / name).read_text())


# ---------------------------------------------------------------------------
# The local mapping: what the workflows need that GitHub provides.
# ---------------------------------------------------------------------------

# Actions GitHub runs, and what stands for them here. A workflow that starts
# using another action fails --check-drift until it is mapped.
ACTIONS = {
    "actions/checkout": "the working tree at HEAD",
    "actions/setup-python": "the python3 on PATH",
    "dtolnay/rust-toolchain": "the cargo on PATH",
    "Swatinem/rust-cache": "this clone's target/",
    "actions/cache": "the per-user octos kernel cache",
    "actions/setup-node": "the node/npm on PATH",
}

# Every job the workflows define, and where it runs here. A new job fails
# --check-drift until it is listed (it would still run, as `run`).
JOBS = {
    "desktop.yml:desktop": {},
    "phone.yml:home": {},
    "apps.yml:services": {"linux_only_note": True},
    "apps.yml:kernel-security": {"linux_only_note": True},
    "apps.yml:apps": {},
    "rom.yml:product": {},
    "rom.yml:web-installer": {},
}

# Steps that need something only some machines have. Keyed by
# "<workflow>:<job>:<step name>"; a key that names no step fails --check-drift.
STEP_REQUIREMENTS = {
    # Many product tests compile Java contracts; ubuntu-latest has a JDK, and
    # macOS's /usr/bin/javac is only a stub without one.
    "rom.yml:product:python3 -m unittest discover -s tests -v": {
        "probe": ["javac", "-version"],
        "hint": "install a JDK (17 or newer) and put its bin/ on PATH or set JAVA_HOME",
    },
    "rom.yml:product:Check generated Agent Binder client": {
        "android_sdk": ["build-tools/35.0.0/aidl", "platforms/android-35/framework.aidl"],
        "hint": "install the SDK packages 'build-tools;35.0.0' and 'platforms;android-35' "
                "(sdkmanager) and set ANDROID_HOME",
    },
}

# GitHub expressions the runner can evaluate. Anything else fails --check-drift.
EXPRESSION = re.compile(r"\$\{\{\s*(.*?)\s*\}\}")
KNOWN_EXPRESSIONS = [re.compile(p) for p in (
    r"runner\.temp", r"runner\.os", r"steps\.[A-Za-z0-9_-]+\.outputs\.[A-Za-z0-9_-]+")]
# Where expressions may appear without being evaluated (not run locally).
IGNORED_EXPRESSION_KEYS = {"concurrency"}


def step_label(step, index):
    if step.get("name"):
        return step["name"]
    if step.get("uses"):
        return step["uses"].split("@")[0]
    first = (step.get("run") or "").strip().splitlines()
    return first[0] if first else f"step {index + 1}"


def action_name(uses):
    return uses.split("@")[0]


def is_kernel_cache(step):
    return action_name(step.get("uses", "")) == "actions/cache" and \
        str((step.get("with") or {}).get("path", "")).rstrip().endswith(KERNEL_BINARY)


def jobs_of(workflow_name, data=None):
    data = data if data is not None else load_workflow(workflow_name)
    for job_id, job in (data.get("jobs") or {}).items():
        yield job_id, job


def check_drift(workflows=None):
    """Problems where the workflows and the local mapping disagree."""
    problems = []
    names = workflows or sorted(p.name for p in WORKFLOWS.glob("*.yml"))
    seen_jobs, seen_steps = set(), set()
    for name in names:
        try:
            data = load_workflow(name)
        except (OSError, YamlError) as error:
            problems.append(f"{name}: cannot read: {error}")
            continue
        if name not in GROUPS["all"]:
            problems.append(f"{name}: a workflow ci-local does not run: add it to GROUPS in tools/ci_local.py")
        for job_id, job in jobs_of(name, data):
            key = f"{name}:{job_id}"
            seen_jobs.add(key)
            if key not in JOBS:
                problems.append(f"{key}: a job ci-local does not know: add it to JOBS in tools/ci_local.py")
            if "container" in job or "services" in job or "strategy" in job:
                problems.append(f"{key}: uses container/services/matrix, which ci-local does not model")
            for index, step in enumerate(job.get("steps") or []):
                label = step_label(step, index)
                seen_steps.add(f"{key}:{label}")
                if "uses" in step:
                    action = action_name(step["uses"])
                    if action not in ACTIONS:
                        problems.append(f"{key}: step '{label}' uses {action}, which has no local mapping (ACTIONS)")
                    elif action == "actions/cache" and not is_kernel_cache(step):
                        problems.append(f"{key}: step '{label}' caches something other than the octos kernel")
                    continue
                if "run" not in step:
                    problems.append(f"{key}: step '{label}' has neither run nor uses")
                for field in ("if", "shell", "continue-on-error"):
                    if field in step:
                        problems.append(f"{key}: step '{label}' sets `{field}:`, which ci-local does not model")
                texts = [step.get("run") or ""] + [str(v) for v in (step.get("env") or {}).values()]
                for text in texts:
                    for expression in EXPRESSION.findall(text):
                        if not any(p.fullmatch(expression) for p in KNOWN_EXPRESSIONS):
                            problems.append(f"{key}: step '{label}' uses ${{{{ {expression} }}}}, which ci-local cannot evaluate")
            for field in ("if", "needs", "env"):
                if field in job:
                    problems.append(f"{key}: sets job-level `{field}:`, which ci-local does not model")
    for key in JOBS:
        if key.split(":")[0] in names and key not in seen_jobs:
            problems.append(f"{key}: listed in JOBS but no workflow defines it")
    for key in STEP_REQUIREMENTS:
        if key.split(":")[0] in names and key not in seen_steps:
            problems.append(f"{key}: listed in STEP_REQUIREMENTS but no step has that name")
    return problems


def pull_request_paths(workflow_name, data=None):
    data = data if data is not None else load_workflow(workflow_name)
    triggers = data.get("on") or data.get(True) or {}
    if isinstance(triggers, dict) and isinstance(triggers.get("pull_request"), dict):
        return triggers["pull_request"].get("paths")
    return None


def glob_match(path, pattern):
    """GitHub's path filter: `**` crosses directories, `*` does not."""
    regex = ""
    i = 0
    while i < len(pattern):
        if pattern.startswith("**", i):
            regex += ".*"
            i += 2
        elif pattern[i] == "*":
            regex += "[^/]*"
            i += 1
        elif pattern[i] == "?":
            regex += "[^/]"
            i += 1
        else:
            regex += re.escape(pattern[i])
            i += 1
    return re.fullmatch(regex, path) is not None


def triggered_workflows(changed_files):
    """The workflows GitHub would run on a pull request touching these files."""
    out = []
    for name in GROUPS["all"]:
        paths = pull_request_paths(name)
        if paths is None or any(glob_match(f, p) for f in changed_files for p in paths):
            out.append(name)
    return out


# ---------------------------------------------------------------------------
# Machine sharing: a slot semaphore and the shared kernel cache.
# ---------------------------------------------------------------------------


def pid_alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


class DirLock:
    """A lock that is a directory (mkdir is atomic; macOS has no flock(1)).

    The owner writes its pid and host into it; a lock whose owner on this host
    has exited is stale and taken over."""

    def __init__(self, path):
        self.path = Path(path)
        self.held = False

    def owner(self):
        try:
            return json.loads((self.path / "owner.json").read_text())
        except (OSError, ValueError):
            return None

    def try_acquire(self, info=None):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        try:
            self.path.mkdir()
        except FileExistsError:
            owner = self.owner()
            if owner is None:
                # Being created right now, or left without an owner file:
                # stale once it is older than a minute.
                try:
                    if time.time() - self.path.stat().st_mtime < 60:
                        return False
                except FileNotFoundError:
                    return False
            elif owner.get("host") != socket.gethostname() or pid_alive(int(owner.get("pid", 0))):
                return False
            shutil.rmtree(self.path, ignore_errors=True)
            return self.try_acquire(info)
        record = {"pid": os.getpid(), "host": socket.gethostname(), "since": time.time(), **(info or {})}
        tmp = self.path / "owner.json.tmp"
        tmp.write_text(json.dumps(record))
        os.replace(tmp, self.path / "owner.json")
        self.held = True
        return True

    def release(self):
        if self.held:
            shutil.rmtree(self.path, ignore_errors=True)
            self.held = False


def lock_root():
    override = os.environ.get("OCTOSENSE_CI_LOCAL_LOCKS")
    if override:
        return Path(override).expanduser()
    return Path(os.environ.get("TMPDIR") or tempfile.gettempdir()) / "octosense-ci-local"


def acquire_slot(slots, wait, log, info):
    """Take one of `slots` run slots; None when busy and not waiting."""
    root = lock_root()
    announced = 0.0
    while True:
        for n in range(slots):
            lock = DirLock(root / f"slot-{n}")
            if lock.try_acquire(info):
                return lock
        if not wait:
            return None
        if time.time() - announced > 300:
            owners = [DirLock(root / f"slot-{n}").owner() or {} for n in range(slots)]
            busy = ", ".join(f"pid {o.get('pid')} in {o.get('root', '?')}" for o in owners)
            log(f"ci-local: all {slots} slots are busy ({busy}); waiting (--no-wait to exit instead) ...")
            announced = time.time()
        time.sleep(10)


def cache_root():
    override = os.environ.get("OCTOSENSE_CI_LOCAL_CACHE")
    if override:
        return Path(override).expanduser()
    if os.environ.get("XDG_CACHE_HOME"):
        return Path(os.environ["XDG_CACHE_HOME"]) / "octosense-ci-local"
    if sys.platform == "darwin":
        return Path.home() / "Library/Caches/octosense-ci-local"
    return Path.home() / ".cache/octosense-ci-local"


def kernel_revision():
    import importlib.util
    spec = importlib.util.spec_from_file_location("kernel_artifact", ROOT / "tools/kernel-artifact.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.octos_revision()


def kernel_cache_file(revision):
    return cache_root() / "octos-kernel" / f"{platform.system()}-{platform.machine()}-{revision}" / "octos"


def publish_to_cache(binary, cached):
    """Copy the built kernel into the shared cache atomically."""
    cached.parent.mkdir(parents=True, exist_ok=True)
    tmp = cached.parent / f".octos.{os.getpid()}.tmp"
    shutil.copy2(binary, tmp)
    with open(tmp, "rb") as handle:
        os.fsync(handle.fileno())
    os.chmod(tmp, 0o755)
    os.replace(tmp, cached)


def link_kernel(runner_temp, cached):
    link = runner_temp / KERNEL_BINARY
    link.parent.mkdir(parents=True, exist_ok=True)
    if link.is_symlink() or link.exists():
        link.unlink()
    link.symlink_to(cached)


# ---------------------------------------------------------------------------
# The runner.
# ---------------------------------------------------------------------------

PASS, FAIL, SKIPPED, NOT_RUN = "PASS", "FAIL", "SKIPPED", "NOT RUN"


class Run:
    def __init__(self, args):
        self.args = args
        stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
        self.out_dir = ROOT / "target/ci-local"
        self.out_dir.mkdir(parents=True, exist_ok=True)
        self.log_path = self.out_dir / f"{stamp}.log"
        self.log_file = open(self.log_path, "w", buffering=1)
        self.runner_temp = self.out_dir / "runner-temp"
        self.steps = []
        self.actions = []
        self.notes = []
        self.failed = False
        self.env = self.base_env()

    def log(self, message, echo=True):
        self.log_file.write(message + "\n")
        if echo:
            print(message, flush=True)

    def base_env(self):
        env = dict(os.environ)
        cargo_bin = Path.home() / ".cargo/bin"
        paths = env.get("PATH", "").split(os.pathsep)
        if cargo_bin.is_dir() and str(cargo_bin) not in paths:
            paths.insert(0, str(cargo_bin))
        env["PATH"] = os.pathsep.join(paths)
        env["CI"] = "true"
        env["GITHUB_WORKSPACE"] = str(ROOT)
        env["RUNNER_TEMP"] = str(self.runner_temp)
        env["RUNNER_OS"] = "macOS" if sys.platform == "darwin" else platform.system()
        env["CARGO_BUILD_JOBS"] = str(self.args.jobs)
        env.setdefault("CARGO_TERM_COLOR", "never")
        return env

    def which(self, tool):
        return shutil.which(tool, path=self.env["PATH"])

    def evaluate(self, text, outputs):
        def replace(match):
            expression = match.group(1)
            if expression == "runner.temp":
                return str(self.runner_temp)
            if expression == "runner.os":
                return self.env["RUNNER_OS"]
            m = re.fullmatch(r"steps\.([A-Za-z0-9_-]+)\.outputs\.([A-Za-z0-9_-]+)", expression)
            if m:
                return outputs.get(m.group(1), {}).get(m.group(2), "")
            raise RuntimeError(f"cannot evaluate ${{{{ {expression} }}}}")
        return EXPRESSION.sub(replace, str(text))

    def record(self, workflow, job, name, status, seconds=0.0, reason="", expected=True, command=None):
        self.steps.append({"workflow": workflow, "job": job, "name": name, "status": status,
                           "seconds": round(seconds, 1), "reason": reason,
                           "expected_skip": expected if status == SKIPPED else None,
                           "command": command})
        if status == FAIL:
            self.failed = True
        line = f"[{status}] {workflow} {job}: {name}" + (f" ({seconds:.0f}s)" if seconds else "")
        if reason:
            line += f" -- {reason}"
        self.log(line)

    def android_sdk(self):
        for candidate in (self.env.get("ANDROID_HOME"), self.env.get("ANDROID_SDK_ROOT"),
                          str(Path.home() / "Library/Android/sdk"), str(Path.home() / "Android/Sdk")):
            if candidate and Path(candidate).is_dir():
                return Path(candidate)
        return None

    def requirement_problem(self, key):
        need = STEP_REQUIREMENTS.get(key)
        if not need:
            return None, {}
        extra_env = {}
        if "probe" in need:
            env = dict(self.env)
            if env.get("JAVA_HOME") and need["probe"][0] in ("java", "javac"):
                env["PATH"] = os.pathsep.join([str(Path(env["JAVA_HOME"]) / "bin"), env["PATH"]])
                extra_env["PATH"] = env["PATH"]
            try:
                ok = subprocess.run(need["probe"], env=env, capture_output=True, timeout=60).returncode == 0
            except (OSError, subprocess.TimeoutExpired):
                ok = False
            if not ok:
                return f"`{' '.join(need['probe'])}` does not work here: {need['hint']}", {}
        if "android_sdk" in need:
            sdk = self.android_sdk()
            missing = [p for p in need["android_sdk"] if not sdk or not (sdk / p).exists()]
            if missing:
                where = str(sdk) if sdk else "no Android SDK found"
                return f"needs {', '.join(missing)} in the Android SDK ({where}): {need['hint']}", {}
            extra_env["ANDROID_HOME"] = str(sdk)
        return None, extra_env

    def job_problem(self, workflow, job_id, job):
        runs_on = str(job.get("runs-on", ""))
        if "windows" in runs_on:
            return f"runs on {runs_on}: Windows cannot run here"
        for step in job.get("steps") or []:
            action = action_name(step.get("uses", ""))
            if action == "dtolnay/rust-toolchain":
                if not self.which("cargo"):
                    return "no cargo on PATH (install rustup)"
                components = str((step.get("with") or {}).get("components", ""))
                for component in filter(None, (c.strip() for c in components.split(","))):
                    probe = {"clippy": ["cargo", "clippy", "--version"], "rustfmt": ["cargo", "fmt", "--version"]}.get(component)
                    if probe and subprocess.run(probe, env=self.env, capture_output=True).returncode != 0:
                        return f"the rust component {component} is missing (rustup component add {component})"
            if action == "actions/setup-node":
                missing = [t for t in ("node", "npm", "npx") if not self.which(t)]
                if missing:
                    want = (step.get("with") or {}).get("node-version", "")
                    return f"no {', '.join(missing)} on PATH (the workflow uses node {want}); put node on PATH"
        return None

    def run_step(self, workflow, job_id, job, index, step, outputs):
        label = step_label(step, index)
        key = f"{workflow}:{job_id}:{label}"
        problem, extra_env = self.requirement_problem(key)
        if problem:
            self.record(workflow, job_id, label, SKIPPED, reason=problem, expected=False, command=step["run"])
            return
        if self.failed and not self.args.keep_going:
            self.record(workflow, job_id, label, NOT_RUN, reason="after an earlier failure", command=step["run"])
            return
        defaults = ((job.get("defaults") or {}).get("run") or {}).get("working-directory")
        cwd = ROOT / (step.get("working-directory") or defaults or ".")
        env = dict(self.env)
        env.update(extra_env)
        for k, v in (step.get("env") or {}).items():
            env[k] = self.evaluate(v, outputs)
        script = self.evaluate(step["run"], outputs)
        output_file = self.out_dir / "github-output"
        output_file.write_text("")
        env["GITHUB_OUTPUT"] = str(output_file)
        building_kernel = "kernel-artifact.py --host" in script
        kernel_lock = None
        if building_kernel:
            kernel_lock = self.prepare_kernel_build()
        self.log(f"--> {workflow} {job_id}: {label}  (cwd {cwd.relative_to(ROOT) if cwd != ROOT else '.'})")
        for line in script.rstrip().splitlines():
            self.log(f"  $ {line}", echo=self.args.verbose)
        started = time.time()
        timeout = step.get("timeout-minutes")
        try:
            with tempfile.NamedTemporaryFile("w", suffix=".sh", delete=False) as handle:
                handle.write(script)
                script_path = handle.name
            process = subprocess.Popen(["bash", "--noprofile", "--norc", "-eo", "pipefail", script_path],
                                       cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                       text=True, errors="replace")
            tail = []
            for line in process.stdout:
                self.log_file.write(line)
                if self.args.verbose:
                    sys.stdout.write(line)
                tail = (tail + [line])[-40:]
            try:
                code = process.wait(timeout=None)
            finally:
                os.unlink(script_path)
            seconds = time.time() - started
            if timeout and seconds > int(timeout) * 60:
                self.log(f"note: took {seconds / 60:.0f} min, over the workflow's timeout-minutes: {timeout}")
            if code == 0 and building_kernel:
                self.finish_kernel_build()
        finally:
            if kernel_lock:
                kernel_lock.release()
        for line in output_file.read_text().splitlines():
            if "=" in line and step.get("id"):
                k, v = line.split("=", 1)
                outputs.setdefault(step["id"], {})[k] = v
        if code == 0:
            self.record(workflow, job_id, label, PASS, seconds, command=step["run"])
        else:
            if not self.args.verbose:
                sys.stdout.write("".join(tail))
            hint = ""
            text = "".join(tail)
            if "cmake" in text and ("not found" in text or "No such file" in text):
                hint = "; cmake is missing: put one on PATH (e.g. `python3 -m venv <dir> && <dir>/bin/pip install cmake`)"
            self.record(workflow, job_id, label, FAIL, seconds, reason=f"exit {code}; see {self.log_path}{hint}",
                        command=step["run"])

    # The kernel: restored from the shared cache as a symlink where the
    # workflow's steps look for it; a miss builds it with the workflow's own
    # step, under a per-revision lock, then publishes it to the cache.
    def restore_kernel(self):
        revision = kernel_revision()
        cached = kernel_cache_file(revision)
        if cached.is_file() and os.access(cached, os.X_OK):
            link_kernel(self.runner_temp, cached)
            return f"hit: {cached}"
        link = self.runner_temp / KERNEL_BINARY
        if link.is_symlink():
            link.unlink()
        return f"miss: octos {revision[:12]} is built by the next step and cached in {cached.parent}"

    def prepare_kernel_build(self):
        revision = kernel_revision()
        cached = kernel_cache_file(revision)
        lock = DirLock(cached.parent.parent / f"{cached.parent.name}.lock")
        waited = False
        while not lock.try_acquire({"root": str(ROOT)}):
            if not waited:
                self.log(f"ci-local: another run is building octos {revision[:12]}; waiting for it ...")
                waited = True
            time.sleep(10)
        if cached.is_file():
            link_kernel(self.runner_temp, cached)
        return lock

    def finish_kernel_build(self):
        link = self.runner_temp / KERNEL_BINARY
        if link.is_symlink() or not link.is_file():
            return
        cached = kernel_cache_file(kernel_revision())
        publish_to_cache(link, cached)
        self.log(f"ci-local: cached the octos kernel at {cached}")
        # The build tree (several GB) is not needed once the binary is cached.
        shutil.rmtree(self.runner_temp / "octos-kernel", ignore_errors=True)
        link_kernel(self.runner_temp, cached)

    def run_job(self, workflow, job_id, job):
        key = f"{workflow}:{job_id}"
        runs_on = str(job.get("runs-on", ""))
        self.log(f"\n=== {workflow} / {job_id} (GitHub: {runs_on}) ===")
        if JOBS.get(key, {}).get("linux_only_note") and "ubuntu" in runs_on and sys.platform == "darwin":
            note = f"{key} runs on {runs_on} in GitHub; here on macOS, so #[cfg(target_os = \"linux\")] code in it is not exercised"
            if note not in self.notes:
                self.notes.append(note)
        problem = self.job_problem(workflow, job_id, job)
        outputs = {}
        for index, step in enumerate(job.get("steps") or []):
            label = step_label(step, index)
            if "uses" in step:
                action = action_name(step["uses"])
                detail = ACTIONS.get(action, "no local mapping")
                if is_kernel_cache(step) and not problem:
                    detail = self.restore_kernel()
                self.actions.append({"workflow": workflow, "job": job_id, "action": step["uses"], "local": detail})
                self.log(f"[action] {workflow} {job_id}: {step['uses']} -> {detail}", echo=self.args.verbose)
                continue
            if problem:
                self.record(workflow, job_id, label, SKIPPED, reason=problem, expected=False, command=step.get("run"))
                continue
            self.run_step(workflow, job_id, job, index, step, outputs)

    def git(self, *args):
        result = subprocess.run(["git", "-C", str(ROOT), *args], capture_output=True, text=True)
        return result.stdout.strip() if result.returncode == 0 else ""

    def execute(self, workflows):
        started = time.time()
        sha = self.git("rev-parse", "HEAD")
        dirty = self.git("status", "--porcelain", "--untracked-files=no")
        self.log(f"ci-local: {ROOT} at {sha} ({self.git('rev-parse', '--abbrev-ref', 'HEAD')})"
                 + (" with uncommitted changes" if dirty else ""))
        self.log(f"ci-local: workflows {', '.join(workflows)}; CARGO_BUILD_JOBS={self.args.jobs}; log {self.log_path}")
        versions = {}
        for tool, argv in (("cargo", ["cargo", "--version"]), ("rustc", ["rustc", "--version"]),
                           ("python3", ["python3", "--version"]), ("node", ["node", "--version"])):
            if self.which(tool):
                versions[tool] = subprocess.run(argv, env=self.env, capture_output=True, text=True).stdout.strip()
        self.log("ci-local: " + "; ".join(f"{v}" for v in versions.values()))
        if not versions.get("python3", "").startswith("Python 3.12"):
            self.notes.append(f"GitHub runs Python 3.12; this run used {versions.get('python3', 'no python3')}")

        drift = check_drift()
        self.record("ci-local", "drift", "The local mapping fits the workflows",
                    FAIL if drift else PASS, reason="; ".join(drift))
        for workflow in workflows:
            data = load_workflow(workflow)
            for job_id, job in jobs_of(workflow, data):
                self.run_job(workflow, job_id, job)

        passed = not any(s["status"] in (FAIL, NOT_RUN) for s in self.steps)
        summary = {
            "sha": sha,
            "branch": self.git("rev-parse", "--abbrev-ref", "HEAD"),
            "dirty": bool(dirty),
            "only": self.args.only,
            "workflows": workflows,
            "workflow_digests": {w: hashlib.sha256((WORKFLOWS / w).read_bytes()).hexdigest() for w in workflows},
            "passed": passed,
            "unexpected_skips": [s["workflow"] + ":" + s["job"] + ":" + s["name"]
                                 for s in self.steps if s["status"] == SKIPPED and not s["expected_skip"]],
            "started": datetime.datetime.fromtimestamp(started).isoformat(timespec="seconds"),
            "seconds": round(time.time() - started, 1),
            "host": {"system": platform.system(), "machine": platform.machine(), "cpus": os.cpu_count()},
            "versions": versions,
            "jobs": self.args.jobs,
            "notes": self.notes,
            "actions": self.actions,
            "steps": self.steps,
            "log": str(self.log_path.relative_to(ROOT)),
        }
        self.log("\n" + format_table(summary))
        tmp = self.out_dir / "last.json.tmp"
        tmp.write_text(json.dumps(summary, indent=2) + "\n")
        os.replace(tmp, self.out_dir / "last.json")
        self.log(f"\nci-local: {'PASSED' if passed else 'FAILED'} in {fmt_seconds(summary['seconds'])}; "
                 f"log {self.log_path}; result target/ci-local/last.json")
        return summary


def fmt_seconds(seconds):
    seconds = int(round(seconds))
    return f"{seconds // 60}m{seconds % 60:02d}s" if seconds >= 60 else f"{seconds}s"


def format_table(summary, markdown=False):
    rows = [(f"{s['workflow']} / {s['job']}", s["name"], s["status"] + ("" if s["expected_skip"] in (None, True) else " (!)"),
             fmt_seconds(s["seconds"]) if s["seconds"] else "-") for s in summary["steps"]]
    lines = []
    if markdown:
        lines.append("| Job | Step | Result | Time |")
        lines.append("| --- | --- | --- | --- |")
        lines += [f"| {a} | {b.replace('|', '/')} | {c} | {d} |" for a, b, c, d in rows]
    else:
        widths = [max([len(r[i]) for r in rows] + [len(h)]) for i, h in enumerate(("Job", "Step", "Result", "Time"))]
        widths[1] = min(widths[1], 70)
        header = ("Job", "Step", "Result", "Time")
        lines.append("  ".join(h.ljust(w) for h, w in zip(header, widths)))
        lines.append("  ".join("-" * w for w in widths))
        for row in rows:
            name = row[1] if len(row[1]) <= widths[1] else row[1][:widths[1] - 3] + "..."
            lines.append("  ".join(v.ljust(w) for v, w in zip((row[0], name, row[2], row[3]), widths)))
    skipped = [s for s in summary["steps"] if s["status"] == SKIPPED]
    if skipped:
        lines.append("")
        lines.append("Skipped (these did NOT pass; (!) marks a skip that blocks tools/ci-local-merge.sh):")
        lines += [f"- {s['workflow']} / {s['job']}: {s['name']}: {s['reason']}" for s in skipped]
    if summary.get("notes"):
        lines.append("")
        lines.append("Notes:")
        lines += [f"- {n}" for n in summary["notes"]]
    return "\n".join(lines)


def print_plan(workflows):
    for workflow in workflows:
        for job_id, job in jobs_of(workflow):
            print(f"{workflow} / {job_id} (GitHub: {job.get('runs-on')})")
            for index, step in enumerate(job.get("steps") or []):
                label = step_label(step, index)
                if "uses" in step:
                    print(f"  [action] {step['uses']} -> {ACTIONS.get(action_name(step['uses']), 'NO MAPPING')}")
                else:
                    cwd = step.get("working-directory") or ((job.get("defaults") or {}).get("run") or {}).get("working-directory") or "."
                    extra = " (needs: " + STEP_REQUIREMENTS[f"{workflow}:{job_id}:{label}"]["hint"] + ")" \
                        if f"{workflow}:{job_id}:{label}" in STEP_REQUIREMENTS else ""
                    print(f"  [run]    {label}  (cwd {cwd}){extra}")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--only", default="all", metavar="|".join(sorted(GROUPS)),
                        help="Which workflows to run, comma-separated (default all)")
    parser.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 2) // 2),
                        help="CARGO_BUILD_JOBS for the run (default: half the CPUs)")
    parser.add_argument("--keep-going", action="store_true", help="Run every step even after a failure")
    parser.add_argument("--no-wait", action="store_true", help=f"Exit {EXIT_BUSY} instead of waiting for a free slot")
    parser.add_argument("--slots", type=int, default=int(os.environ.get("OCTOSENSE_CI_LOCAL_SLOTS", "2")),
                        help="How many runs may share this machine (default 2, or OCTOSENSE_CI_LOCAL_SLOTS)")
    parser.add_argument("--verbose", "-v", action="store_true", help="Echo every step's output (always in the log)")
    parser.add_argument("--list", action="store_true", help="Print the plan and exit")
    parser.add_argument("--check-drift", action="store_true", help="Check the local mapping against the workflows and exit")
    parser.add_argument("--linux-host", metavar="SSH", help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    unknown = [g for g in args.only.split(",") if g not in GROUPS]
    if unknown:
        parser.error(f"--only: unknown {', '.join(unknown)} (choose from {', '.join(sorted(GROUPS))})")
    workflows = [w for w in GROUPS["all"] if any(w in GROUPS[g] for g in args.only.split(","))]
    if args.linux_host:
        parser.error("--linux-host is not implemented: the ubuntu jobs run on this Mac (see docs/local-ci.md)")
    if args.check_drift:
        problems = check_drift()
        for problem in problems:
            print(f"drift: {problem}")
        print("ci-local: the local mapping fits the workflows" if not problems else f"ci-local: {len(problems)} drift problem(s)")
        return 1 if problems else 0
    if args.list:
        print_plan(workflows)
        return 0
    clone_lock = DirLock(ROOT / "target/ci-local/running.lock")
    if not clone_lock.try_acquire({"root": str(ROOT)}):
        print(f"ci-local: another run is using this clone (pid {(clone_lock.owner() or {}).get('pid')}); "
              f"one run per clone", file=sys.stderr)
        return EXIT_BUSY
    try:
        return run_locked(args, workflows)
    finally:
        clone_lock.release()


def run_locked(args, workflows):
    run = Run(args)
    slot = acquire_slot(args.slots, not args.no_wait, run.log,
                        {"root": str(ROOT), "only": args.only})
    if slot is None:
        run.log(f"ci-local: all {args.slots} slots in {lock_root()} are busy; not waiting (--no-wait)")
        return EXIT_BUSY
    try:
        summary = run.execute(workflows)
    except KeyboardInterrupt:
        run.log("ci-local: interrupted")
        return 130
    finally:
        slot.release()
    return 0 if summary["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
