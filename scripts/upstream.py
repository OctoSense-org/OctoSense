#!/usr/bin/env python3
"""Compare and safely merge the recorded Makepad import (Python 3.11+).

The source checkout is read exclusively through Git objects. Update requires a
clean destination repository, verifies a disposable copy, then applies changes
without touching its index. See docs/upstream.md for recovery and limitations.
"""
from __future__ import annotations

import argparse
import copy
from contextlib import contextmanager
from dataclasses import dataclass, field
from datetime import datetime, timezone
import difflib
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import tomllib
from urllib.parse import parse_qs, urlsplit


BASELINE = "upstream/makepad.json"


class SyncError(RuntimeError):
    pass


class RecoveryError(SyncError):
    """An apply error was followed by a rollback error; do not claim safety."""


def run(command, cwd):
    result = subprocess.run(command, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise SyncError(f"{' '.join(command)}: {result.stderr.decode(errors='replace').strip()}")
    return result.stdout


def git(root, *args):
    # Avoid optional index refresh writes, including during read-only status.
    return run(["git", "--no-optional-locks", "-C", str(root), *args], root)


def canonical_repo(value):
    return value.removesuffix("/").removesuffix(".git")


def checked_path(root, name):
    path = PurePosixPath(name)
    if (not name or path.is_absolute() or str(path) != name or
            any(part in ("..", ".git") for part in path.parts) or "\\" in name):
        raise SyncError(f"unsafe import path: {name!r}")
    current = root
    for part in path.parts:
        current = current / part
        if current.is_symlink():
            raise SyncError(f"symlink is not supported in import paths: {name}")
    return current


def read_optional(path):
    if path.exists() and not path.is_file():
        raise SyncError(f"expected a regular file: {path}")
    return path.read_bytes() if path.exists() else None


def resolve(source, revision):
    return git(source, "rev-parse", "--verify", "--end-of-options", f"{revision}^{{commit}}").decode().strip()


def tree(source, revision):
    entries = {}
    for row in git(source, "ls-tree", "-rz", revision).split(b"\0"):
        if row:
            meta, path = row.split(b"\t", 1)
            mode, kind, oid = meta.decode().split()
            entries[path.decode()] = (mode, kind, oid)
    return entries


def blob(source, entries, name):
    if name not in entries:
        return None
    mode, kind, oid = entries[name]
    if kind != "blob" or mode not in ("100644", "100755"):
        raise SyncError(f"unsupported upstream file mode {mode}: {name}")
    return git(source, "cat-file", "blob", oid)


def manifest_paths(root):
    paths = []
    for directory, children, files in os.walk(root):
        children[:] = [name for name in children if name not in ("target", ".git", ".worktrees")]
        if "Cargo.toml" in files:
            paths.append(Path(directory) / "Cargo.toml")
    return sorted(paths)


def dependency_tables(value, repository):
    if isinstance(value, dict):
        if isinstance(value.get("git"), str) and canonical_repo(value["git"]) == canonical_repo(repository):
            yield value
        for child in value.values():
            yield from dependency_tables(child, repository)
    elif isinstance(value, list):
        for child in value:
            yield from dependency_tables(child, repository)


def pin_problems(root, repository, revision, check_lock=True):
    problems = []
    count = 0
    for path in manifest_paths(root):
        checked_path(root, str(path.relative_to(root)))
        try:
            data = tomllib.loads(path.read_text())
            for dep in dependency_tables(data, repository):
                count += 1
                if dep.get("rev") != revision or any(key in dep for key in ("branch", "tag", "path")):
                    problems.append(f"{path.relative_to(root)}: Makepad dependency must pin revision {revision}")
        except (ValueError, OSError) as error:
            problems.append(f"{path.relative_to(root)}: {error}")
    if count == 0:
        problems.append("no pinned Makepad Git dependencies found in Cargo manifests")
    if check_lock:
        lock = checked_path(root, "Cargo.lock")
        if not lock.is_file():
            problems.append("Cargo.lock is missing; resolve dependencies before updating")
        else:
            try:
                data = tomllib.loads(lock.read_text())
                lock_count = 0
                for package in data.get("package", []):
                    source = package.get("source", "")
                    if source.startswith("git+"):
                        url = urlsplit(source[4:])
                        repo = source[4:].split("?", 1)[0].split("#", 1)[0]
                        if canonical_repo(repo) == canonical_repo(repository):
                            lock_count += 1
                            if parse_qs(url.query).get("rev") != [revision] or url.fragment != revision:
                                problems.append(f"Cargo.lock: {package.get('name')} uses a different Makepad revision")
                if not lock_count:
                    problems.append("Cargo.lock contains no Makepad Git dependencies")
            except (ValueError, OSError) as error:
                problems.append(f"Cargo.lock: {error}")
    return problems


@dataclass
class Change:
    source: str
    destination: str
    status: str
    base: bytes | None
    local: bytes | None
    upstream: bytes | None
    merged: bytes | None
    mode: int = 0o644
    detail: str = ""


@dataclass
class Comparison:
    manifest: dict
    revision: str
    changes: list[Change] = field(default_factory=list)
    problems: list[str] = field(default_factory=list)
    dependency_changes: list[str] = field(default_factory=list)

    @property
    def conflicts(self):
        return [change for change in self.changes if change.status == "conflict"]


def merge(base, local, new):
    if new == base:
        return ("unchanged" if local == base else "local-only"), local
    if local == base:
        return ("deleted" if new is None else "upstream-only"), new
    if local == new:
        return ("deleted" if new is None else "converged"), new
    if local is None or new is None or any(b"\0" in data for data in (base, local, new)):
        return "conflict", local
    with tempfile.TemporaryDirectory(prefix="makeos-merge-") as directory:
        paths = [Path(directory) / name for name in ("local", "base", "upstream")]
        for path, data in zip(paths, (local, base, new)):
            path.write_bytes(data)
        result = subprocess.run(["git", "merge-file", "-p", "--diff3", "-L", "MakeOS", "-L", "old Makepad", "-L", "new Makepad", *map(str, paths)], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if result.returncode < 0 or result.returncode > 127:
            raise SyncError(f"merge failed: {result.stderr.decode(errors='replace')}")
        return ("merged" if result.returncode == 0 else "conflict"), result.stdout


def compare(root, source, to=None):
    root, source = Path(root).resolve(), Path(source).resolve()
    baseline = json.loads(checked_path(root, BASELINE).read_text())
    if baseline.get("schema_version") != 1:
        raise SyncError("unsupported provenance schema_version")
    revision = resolve(source, baseline["revision"])
    target = resolve(source, to or revision)
    result = Comparison(baseline, target)
    if baseline["revision"] != revision:
        result.problems.append("baseline revision must be a full commit hash")
    if baseline.get("dependency_revision") != revision:
        result.problems.append("source revision and dependency_revision do not match")
    result.problems.extend(pin_problems(root, baseline["repository"], revision))
    old_tree, new_tree = tree(source, revision), tree(source, target)
    prefix = baseline["source_prefix"]
    checked_path(root, prefix.rstrip("/"))
    if not prefix.endswith("/"):
        raise SyncError("source_prefix must end with /")
    omitted = set()
    for item in baseline.get("omissions", []):
        name = item if isinstance(item, str) else item["source"]
        checked_path(root, name)
        omitted.add(name)
    files = baseline["files"]
    sources, destinations = set(), set()
    for entry in files:
        name, destination = entry["source"], entry["destination"]
        checked_path(root, name)
        local_path = checked_path(root, destination)
        if destination == BASELINE or name in sources or destination in destinations or name in omitted:
            raise SyncError(f"duplicate/reserved/omitted import mapping: {name} -> {destination}")
        sources.add(name)
        destinations.add(destination)
        base, new = blob(source, old_tree, name), blob(source, new_tree, name)
        if base is None or hashlib.sha256(base).hexdigest() != entry.get("sha256"):
            result.problems.append(f"baseline hash mismatch or missing source: {name}")
            continue
        local = read_optional(local_path)
        status, merged = merge(base, local, new)
        mode = local_path.stat().st_mode & 0o777 if local is not None else int(new_tree.get(name, old_tree[name])[0], 8) & 0o777
        result.changes.append(Change(name, destination, status, base, local, new, merged, mode))
    for name in sorted(old_tree):
        if name.startswith(prefix) and name not in sources and name not in omitted:
            result.problems.append(f"unrecorded baseline source file: {name}; record an import or omission")
    for name in sorted(new_tree):
        if not name.startswith(prefix) or name in old_tree or name in omitted:
            continue
        destination = name[len(prefix):]
        local_path = checked_path(root, destination)
        new = blob(source, new_tree, name)
        collision = destination in destinations or local_path.exists() or any(
            (parent.exists() and not parent.is_dir()) for parent in local_path.parents if parent != root
        )
        status = "conflict" if collision else "added"
        detail = "new upstream file collides with an existing MakeOS path" if collision else "new upstream file"
        local = local_path.read_bytes() if local_path.is_file() else None
        result.changes.append(Change(name, destination, status, None, local, new, local if collision else new, int(new_tree[name][0], 8) & 0o777, detail))
        destinations.add(destination)
    if revision != target:
        # Include the entire non-WM diff: framework dependency changes can matter
        # even when the imported application has not changed.
        for row in git(source, "diff", "--name-status", "--no-renames", revision, target).decode().splitlines():
            if not row.split("\t", 1)[-1].startswith(prefix):
                result.dependency_changes.append(row)
    return result


REV = re.compile(r"(\brev\s*=\s*)([\"'])([^\"']+)([\"'])")
GIT = re.compile(r"\bgit\s*=\s*[\"']([^\"']+)[\"']")


def rewrite_pins(root, repository, old, new):
    for path in manifest_paths(root):
        original = path.read_text()
        expected = len(list(dependency_tables(tomllib.loads(original), repository)))
        if not expected:
            continue
        changed = 0
        def rewrite(block):
            nonlocal changed
            match = GIT.search(block)
            if match and canonical_repo(match[1]) == canonical_repo(repository):
                if len(REV.findall(block)) != 1:
                    raise SyncError(f"unsupported Makepad revision syntax in {path.relative_to(root)}")
                def replace(revision):
                    nonlocal changed
                    if revision[3] != old:
                        raise SyncError(f"unexpected Makepad revision in {path.relative_to(root)}")
                    changed += 1
                    return revision[1] + revision[2] + new + revision[4]
                return REV.sub(replace, block)
            return block
        # Handle inline tables and ordinary [dependencies.name] tables while
        # preserving formatting. Count against tomllib so unfamiliar syntax
        # fails safely instead of silently leaving a dependency behind.
        parts = re.split(r"(?m)(?=^\s*\[[^\n]+\]\s*(?:#.*)?$)", original)
        rewritten = []
        for part in parts:
            inline = re.findall(r"\{[^{}]*\}", part, re.DOTALL)
            if inline:
                part = re.sub(r"\{[^{}]*\}", lambda match: rewrite(match[0]), part, flags=re.DOTALL)
            else:
                part = rewrite(part)
            rewritten.append(part)
        text = "".join(rewritten)
        if changed != expected or any(dep.get("rev") != new for dep in dependency_tables(tomllib.loads(text), repository)):
            raise SyncError(f"unsupported dependency formatting in {path.relative_to(root)}; update this manifest deliberately")
        path.write_text(text)


def clean_snapshot(root):
    if git(root, "rev-parse", "--show-toplevel").decode().strip() != str(root):
        raise SyncError("run update at the MakeOS Git repository root")
    if git(root, "status", "--porcelain", "--untracked-files=all"):
        raise SyncError("update requires a clean MakeOS working tree; commit local changes first")
    snapshot = {}
    for name in git(root, "ls-files", "-z").decode().split("\0"):
        if name:
            path = checked_path(root, name)
            snapshot[name] = (path.read_bytes(), path.stat().st_mode & 0o777)
    for required in (BASELINE, "Cargo.toml", "Cargo.lock"):
        if required not in snapshot:
            raise SyncError(f"required file must be tracked: {required}")
    return snapshot


def put(root, name, data, mode=0o644):
    path = checked_path(root, name)
    if data is None:
        if path.exists():
            path.unlink()
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    # Atomic replacement of each file; apply() rolls back the transaction if
    # a later write fails. The live provenance file is written last.
    descriptor, temporary = tempfile.mkstemp(prefix=".makeos-sync-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(data)
        os.chmod(temporary, mode)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def run_verification(command, stage, env, output):
    # Own the command's group so Ctrl-C can be forwarded once. In particular,
    # smoke.py needs time to close its separately hosted app/build processes.
    process = subprocess.Popen(command, cwd=stage, env=env, stdout=output,
                               stderr=subprocess.STDOUT, start_new_session=os.name == "posix")
    try:
        return process.wait()
    except KeyboardInterrupt:
        print("Interrupted; waiting for verification process cleanup...", flush=True)
        previous = signal.signal(signal.SIGINT, signal.SIG_IGN)
        def send(sig):
            try:
                if os.name == "posix":
                    os.killpg(process.pid, sig)
                else:
                    process.send_signal(sig)
            except ProcessLookupError:
                pass
        try:
            send(signal.SIGINT)
            try:
                process.wait(timeout=45)
            except subprocess.TimeoutExpired:
                send(signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    send(signal.SIGKILL)
                    process.wait(timeout=5)
        finally:
            signal.signal(signal.SIGINT, previous)
        raise


def verify_stage(stage, *, runtime=False, report=None):
    report = report or stage.parent
    log = report / "verification.log"
    commands = [["cargo", "metadata", "--format-version", "1"],
                ["cargo", "check", "--locked", "--workspace"],
                ["cargo", "test", "--locked", "--workspace", "--quiet"]]
    if (stage / "scripts/test_upstream.py").exists():
        commands.append([sys.executable, "-m", "unittest", "discover", "-s", "scripts", "-p", "test_*.py"])
    if runtime:
        commands.extend([
            ["cargo", "build", "--release", "--locked", "--workspace"],
            ["cargo", "build", "--locked", "--workspace"],
            [sys.executable, "scripts/smoke.py", "--styles", "--artifacts-dir", str(report / "smoke-release")],
            [sys.executable, "scripts/smoke.py", "--cargo-run", "--default-catalog",
             "--artifacts-dir", str(report / "smoke-default")],
        ])
    # The candidate owns its binaries, so resource/catalog discovery cannot
    # accidentally climb from a shared build directory into the live project.
    env = dict(os.environ, CARGO_TARGET_DIR=str(stage / "target"))
    with log.open("wb") as output:
        for command in commands:
            print(f"Verifying staged project: {' '.join(command)}", flush=True)
            output.write(("\n$ " + " ".join(command) + "\n").encode())
            output.flush()
            if run_verification(command, stage, env, output):
                raise SyncError(f"{' '.join(command)} failed; see {log}")


def apply(root, stage, original, names):
    # Recheck after lengthy Cargo verification; a user's concurrent changes
    # must never be overwritten by the staged result.
    if clean_snapshot(root) != original:
        raise SyncError("MakeOS changed during verification; refusing to apply")
    for name in names:
        if name not in original and checked_path(root, name).exists():
            raise SyncError(f"new destination collision after verification: {name}")
    done = []
    try:
        for name in sorted(names - {BASELINE}) + [BASELINE]:
            path = checked_path(stage, name)
            data = read_optional(path)
            mode = path.stat().st_mode & 0o777 if data is not None else 0o644
            done.append(name)
            put(root, name, data, mode)
    except BaseException as error:
        failures = []
        for name in reversed(done):
            data, mode = original.get(name, (None, 0o644))
            try:
                put(root, name, data, mode)
            except Exception as rollback_error:
                failures.append(f"{name}: {rollback_error}")
        if failures:
            raise RecoveryError("apply failed and rollback needs manual recovery from the starting commit:\n" + "\n".join(failures)) from error
        raise


def update(root, source, to, verify=None, *, work_dir=None, report=None, before_apply=None):
    root, source = Path(root).resolve(), Path(source).resolve()
    original = clean_snapshot(root)
    comparison = compare(root, source, to)
    if comparison.problems:
        raise SyncError("cannot update:\n" + "\n".join(comparison.problems))
    for change in comparison.changes:
        if change.base is not None and change.local is not None and change.destination not in original:
            raise SyncError(f"import destination must be tracked before updating: {change.destination}")
    temporary = work_dir or Path(tempfile.mkdtemp(prefix="makeos-upstream-"))
    report = report or temporary
    stage = temporary / "project"
    stage.mkdir(parents=True, exist_ok=True)
    try:
        # Only compiled artifacts survive between daily sync attempts. Removed
        # or generated source files from the previous candidate must not leak in.
        for child in stage.iterdir():
            if child.name == "target" and child.is_dir() and not child.is_symlink():
                continue
            if child.is_dir() and not child.is_symlink():
                shutil.rmtree(child)
            else:
                child.unlink()
        for name, (data, mode) in original.items():
            put(stage, name, data, mode)
        names = {BASELINE, "Cargo.lock"}
        for change in comparison.changes:
            if change.status != "conflict":
                put(stage, change.destination, change.merged, change.mode)
                names.add(change.destination)
            elif change.merged is not None and change.local is not None:
                # Preserve conflict markers only in the disposable copy.
                put(stage, change.destination, change.merged, change.mode)
        (report / "comparison.txt").write_text(format_comparison(comparison, show_diff=True))
        if comparison.conflicts:
            raise SyncError("merge conflict(s): " + ", ".join(change.destination for change in comparison.conflicts))
        rewrite_pins(stage, comparison.manifest["repository"], comparison.manifest["dependency_revision"], comparison.revision)
        names.update(str(path.relative_to(stage)) for path in manifest_paths(stage))
        manifest = copy.deepcopy(comparison.manifest)
        manifest["revision"] = manifest["dependency_revision"] = comparison.revision
        manifest["files"] = [
            {"source": change.source, "destination": change.destination,
             "sha256": hashlib.sha256(change.upstream).hexdigest()}
            for change in sorted(comparison.changes, key=lambda change: change.source)
            if change.upstream is not None
        ]
        put(stage, BASELINE, (json.dumps(manifest, indent=2) + "\n").encode())
        try:
            (verify or verify_stage)(stage)
            problems = pin_problems(stage, comparison.manifest["repository"], comparison.revision)
            if problems:
                raise SyncError("\n".join(problems))
        except Exception as error:
            raise SyncError(f"staged verification failed: {error}") from error
        if before_apply:
            before_apply()
        apply(root, stage, original, names)
    except Exception as error:
        state = "Live files may differ from the starting commit." if isinstance(error, RecoveryError) else "Live import and baseline were not advanced."
        raise SyncError(f"{error}\n{state} Review retained stage: {temporary}") from error
    if work_dir is None:
        shutil.rmtree(temporary)
    return comparison


@contextmanager
def sync_lock(cache):
    # Native smoke automation currently targets macOS/POSIX. flock also releases
    # on an interrupted process, unlike a marker directory that can go stale.
    import fcntl
    cache.mkdir(parents=True, exist_ok=True)
    with (cache / "sync.lock").open("a") as handle:
        try:
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise SyncError("another MakeOS sync is running; wait for it to finish") from error
        try:
            yield
        finally:
            fcntl.flock(handle, fcntl.LOCK_UN)


def source_checkout(root, source=None):
    if source is not None:
        return Path(source).resolve()
    root = Path(root).resolve()
    baseline = json.loads(checked_path(root, BASELINE).read_text())
    default = baseline.get("default_source", "../makepad")
    if not isinstance(default, str) or not default.strip():
        raise SyncError("default_source must be a nonempty checkout path")
    # This is a read-only source location, not an import destination: sibling
    # and absolute paths are allowed. Relative defaults belong to the project.
    return (root / default).resolve()


def sync(root, source=None, to=None, verify=None):
    root = Path(root).resolve()
    source = source_checkout(root, source)
    # Freeze a moving ref once. Later fetches/pulls cannot change this attempt.
    target = resolve(source, to or "HEAD")
    comparison = compare(root, source, target)
    if comparison.problems:
        raise SyncError("cannot sync:\n" + "\n".join(comparison.problems))
    if comparison.manifest["revision"] == target:
        print(f"Already at Makepad {target}; no update or checks needed.")
        if git(root, "status", "--porcelain", "--untracked-files=all"):
            print("MakeOS has uncommitted changes; review/commit them separately.")
        return None

    original = clean_snapshot(root)
    cache = checked_path(root, "target/makepad-sync")
    ignored = subprocess.run(["git", "-C", str(root), "check-ignore", "-q", "--", "target/makepad-sync"])
    if ignored.returncode:
        raise SyncError("target/makepad-sync must be Git-ignored before running sync")
    with sync_lock(cache):
        if clean_snapshot(root) != original:
            raise SyncError("MakeOS changed while starting sync; retry from a clean tree")
        head = git(root, "rev-parse", "HEAD").decode().strip()
        starting_branch = git(root, "branch", "--show-current").decode().strip()
        reports = cache / "reports"
        reports.mkdir(exist_ok=True)
        stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ-")
        report = Path(tempfile.mkdtemp(prefix=stamp, dir=reports))
        (report / "comparison.txt").write_text(format_comparison(comparison, show_diff=True))
        print(f"Syncing Makepad {comparison.manifest['revision']} -> {target}\nReport: {report}", flush=True)
        branch = None

        def review_branch():
            nonlocal branch
            if (git(root, "rev-parse", "HEAD").decode().strip() != head or
                    git(root, "branch", "--show-current").decode().strip() != starting_branch or
                    clean_snapshot(root) != original):
                raise SyncError("MakeOS HEAD, branch, or files changed during verification; refusing to apply")
            existing = set(git(root, "for-each-ref", "--format=%(refname:short)", "refs/heads/").decode().splitlines())
            base = candidate = f"sync/makepad-{target[:12]}"
            suffix = 2
            while candidate in existing:
                candidate = f"{base}-{suffix}"
                suffix += 1
            git(root, "switch", "-c", candidate)
            branch = candidate

        try:
            update(root, source, target, verify=verify or (lambda stage: verify_stage(stage, runtime=True, report=report)),
                   work_dir=cache, report=report, before_apply=review_branch)
        except BaseException as error:
            secondary = []
            # An apply failure normally rolls files back. Remove only our empty
            # branch, and only when no subsequent user edit or Git change exists.
            try:
                if branch and git(root, "branch", "--show-current").decode().strip() == branch:
                    if git(root, "rev-parse", "HEAD").decode().strip() == head and clean_snapshot(root) == original:
                        git(root, "switch", starting_branch) if starting_branch else git(root, "switch", "--detach", head)
                        git(root, "branch", "-d", branch)
            except (SyncError, OSError) as cleanup_error:
                secondary.append(f"Review branch retained: {cleanup_error}")
            # Reporting failures must not mask the original error or prevent
            # Git cleanup, especially when the original failure was disk I/O.
            candidate = cache / "project"
            try:
                if candidate.exists():
                    shutil.copytree(candidate, report / "project", ignore=shutil.ignore_patterns("target", "__pycache__"))
                    candidate = report / "project"
            except OSError as archive_error:
                secondary.append(f"Could not archive candidate: {archive_error}; cache retained at {candidate}")
            detail = f"FAILED: {type(error).__name__}: {error}\nCandidate: {candidate}\n" + "\n".join(secondary)
            try:
                (report / "summary.txt").write_text(detail + "\n")
            except OSError as report_error:
                detail += f"\nCould not write summary: {report_error}"
            if isinstance(error, (KeyboardInterrupt, SystemExit)):
                print(f"{detail}\nSync interrupted. Failure report: {report}", file=sys.stderr)
                raise
            raise SyncError(f"{detail}\nFailure report: {report}") from error

        summary = (f"READY FOR REVIEW\nMakepad: {comparison.manifest['revision']} -> {target}\n"
                   f"Starting branch: {starting_branch or '(detached)'}\nStarting commit: {head}\n"
                   f"Review branch: {branch}\nChecks passed; changes are unstaged and uncommitted.\n"
                   f"Report: {report}\n\nNext: git status --short; git diff --stat; git diff\n"
                   "Review the comparison and smoke frames, then commit/merge when satisfied.\n")
        (report / "summary.txt").write_text(summary)
        print(summary, end="")
        return report


def format_comparison(comparison, show_diff=False):
    lines = [f"Makepad baseline: {comparison.manifest['revision']}", f"Compare to:       {comparison.revision}"]
    counts = {}
    for change in comparison.changes:
        counts[change.status] = counts.get(change.status, 0) + 1
        if change.status != "unchanged":
            lines.append(f"{change.status:14} {change.destination}" + (f" ({change.detail})" if change.detail else ""))
        if show_diff:
            for label, left, right in (("local adaptations", change.base, change.local), ("upstream changes", change.base, change.upstream)):
                if left == right:
                    continue
                lines.append(f"\n{change.destination}: {label}")
                if any(b"\0" in data for data in (left or b"", right or b"")):
                    lines.append("Binary content differs")
                else:
                    lines.extend(line.rstrip("\n") for line in difflib.unified_diff(
                        (left or b"").decode(errors="replace").splitlines(True),
                        (right or b"").decode(errors="replace").splitlines(True),
                        fromfile=f"old Makepad/{change.source}", tofile=f"{label}/{change.destination}"))
    lines.append("\n" + ", ".join(f"{count} {status}" for status, count in sorted(counts.items())))
    if comparison.dependency_changes:
        lines.append("\nChanges outside the WM tree (review framework/API impact):")
        lines.extend(comparison.dependency_changes)
    if comparison.problems:
        lines.append("\nProvenance/dependency problems:")
        lines.extend(comparison.problems)
    return "\n".join(lines) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["sync", "status", "diff", "update"])
    parser.add_argument("--source", type=Path, help="existing Makepad Git clone (default: provenance default_source or ../makepad; never written)")
    parser.add_argument("--to", help="target commit/ref; sync defaults to source HEAD, status/diff to baseline; required for update")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1], help="MakeOS repository root")
    args = parser.parse_args(argv)
    if args.command == "update" and not args.to:
        parser.error("update requires --to")
    try:
        source = source_checkout(args.root, args.source)
        if args.command == "sync":
            sync(args.root, source, args.to)
            return 0
        comparison = update(args.root, source, args.to) if args.command == "update" else compare(args.root, source, args.to)
        print(format_comparison(comparison, show_diff=args.command == "diff"), end="")
        if args.command == "update":
            print("Verified update applied. Review git diff, run host/client GUI smoke tests, then commit the files and baseline together.")
        return 1 if comparison.problems or comparison.conflicts else 0
    except KeyboardInterrupt:
        print("upstream: interrupted", file=sys.stderr)
        return 130
    except (SyncError, OSError, ValueError, KeyError) as error:
        print(f"upstream: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
