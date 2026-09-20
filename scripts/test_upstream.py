"""Local-only Git fixtures for the Makepad import maintenance tool."""
import hashlib
import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import signal
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import upstream


REPOSITORY = "https://github.com/makepad/makepad.git"
ORIGINAL = "one\ntwo\nthree\nfour\nfive\nsix\nseven\n"


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], stderr=subprocess.PIPE).decode().strip()


def write(root, name, text):
    path = root / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def commit(root):
    git(root, "add", "--all")
    git(root, "commit", "-qm", "fixture")
    return git(root, "rev-parse", "HEAD")


class RepoFixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve() / "octosense"
        self.source = Path(self.temp.name).resolve() / "makepad"
        for root in (self.root, self.source):
            root.mkdir()
            git(root, "init", "-q")
            git(root, "config", "user.email", "fixture@example.invalid")
            git(root, "config", "user.name", "Fixture")
        write(self.source, "apps/wm/src/main.rs", ORIGINAL)
        write(self.source, "LICENSE", "upstream license\n")
        self.base = commit(self.source)
        write(self.root, "src/main.rs", ORIGINAL)
        write(self.root, "LICENSES/Makepad-MIT.txt", "upstream license\n")
        self.manifest = {
            "schema_version": 1, "repository": REPOSITORY,
            "revision": self.base, "dependency_revision": self.base,
            "source_prefix": "apps/wm/", "adaptations": [], "omissions": [],
            "files": [
                {"source": source, "destination": dest,
                 "sha256": hashlib.sha256((self.source / source).read_bytes()).hexdigest()}
                for source, dest in [("apps/wm/src/main.rs", "src/main.rs"),
                                     ("LICENSE", "LICENSES/Makepad-MIT.txt")]
            ],
        }
        self.save_manifest()
        cargo = '[package]\nname = "fixture"\nversion = "0.1.0"\n[dependencies]\n' + (
            'makepad-widgets = { git = "%s", rev = "%s" }\n' % (REPOSITORY, self.base)
        )
        write(self.root, "Cargo.toml", cargo)
        write(self.root, "apps/reference/Cargo.toml", cargo.replace('name = "fixture"', 'name = "reference"'))
        self.write_lock(self.root, self.base)
        commit(self.root)

    def save_manifest(self):
        write(self.root, "upstream/makepad.json", json.dumps(self.manifest, indent=2) + "\n")

    def write_lock(self, root, revision):
        write(root, "Cargo.lock", 'version = 3\n[[package]]\nname = "makepad-widgets"\nversion = "0.1.0"\nsource = "git+%s?rev=%s#%s"\n' % (REPOSITORY, revision, revision))

    def target(self, text=None):
        if text is not None:
            write(self.source, "apps/wm/src/main.rs", text)
        else:
            write(self.source, "framework.rs", "a framework update\n")
        return commit(self.source)

    def compare(self, target=None):
        return upstream.compare(self.root, self.source, target or self.base)

    def change(self, comparison, destination="src/main.rs"):
        return next(item for item in comparison.changes if item.destination == destination)

    def fake_verify(self, stage):
        self.assertNotEqual(stage, self.root)
        self.assertEqual((self.root / "upstream/makepad.json").read_text(), self.live_baseline)
        # Simulate Cargo's resolver: both workspace manifests must already use the new pin.
        for path in ["Cargo.toml", "apps/reference/Cargo.toml"]:
            self.assertIn(self.next_revision, (stage / path).read_text())
        self.write_lock(stage, self.next_revision)

    def update(self, revision, verify=None):
        self.next_revision = revision
        self.live_baseline = (self.root / "upstream/makepad.json").read_text()
        return upstream.update(self.root, self.source, revision, verify=verify or self.fake_verify)

    def snapshot(self):
        return {str(path.relative_to(self.root)): path.read_bytes()
                for path in self.root.rglob("*") if path.is_file() and ".git" not in path.parts}


class SyncTests(RepoFixture):
    def test_nested_worktrees_do_not_participate_in_dependency_pins(self):
        stale = (self.root / "Cargo.toml").read_text().replace(self.base, "0" * 40)
        write(self.root, ".worktrees/old-feature/Cargo.toml", stale)
        self.assertFalse(upstream.pin_problems(self.root, REPOSITORY, self.base))
        upstream.rewrite_pins(self.root, REPOSITORY, self.base, "1" * 40)
        self.assertEqual((self.root / ".worktrees/old-feature/Cargo.toml").read_text(), stale)

    def test_recorded_default_source_used_by_sync_and_cli(self):
        renamed = self.source.with_name("guofoo-makepad")
        self.source.rename(renamed)
        self.source = renamed
        self.manifest["default_source"] = "../guofoo-makepad"
        self.save_manifest()
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertIsNone(upstream.sync(self.root))
            self.assertEqual(upstream.main(["status", "--root", str(self.root)]), 0)
            self.assertEqual(upstream.main(["sync", "--root", str(self.root)]), 0)

    def test_explicit_source_overrides_recorded_default(self):
        self.manifest["default_source"] = "../missing-checkout"
        self.save_manifest()
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertIsNone(upstream.sync(self.root, self.source))
            self.assertEqual(upstream.main(["status", "--root", str(self.root),
                                            "--source", str(self.source)]), 0)

    def test_unchanged_and_local_only(self):
        self.assertFalse(self.compare().problems)
        self.assertEqual(self.change(self.compare()).status, "unchanged")
        write(self.root, "src/main.rs", ORIGINAL.replace("one", "local"))
        before = self.snapshot()
        self.assertEqual(self.change(self.compare()).status, "local-only")
        self.assertEqual(self.snapshot(), before)

    def test_upstream_only_update_changes_baseline_and_both_pins(self):
        new_text = ORIGINAL.replace("one", "upstream")
        target = self.target(new_text)
        self.assertEqual(self.change(self.compare(target)).status, "upstream-only")
        self.update(target)
        self.assertEqual((self.root / "src/main.rs").read_text(), new_text)
        manifest = json.loads((self.root / "upstream/makepad.json").read_text())
        self.assertEqual(manifest["revision"], target)
        self.assertEqual(manifest["dependency_revision"], target)
        entry = next(item for item in manifest["files"] if item["destination"] == "src/main.rs")
        self.assertEqual(entry["sha256"], hashlib.sha256(new_text.encode()).hexdigest())
        self.assertFalse(self.compare(target).problems)
        self.assertEqual(git(self.source, "status", "--porcelain"), "")

    def test_nonoverlapping_edits_keep_both(self):
        write(self.root, "src/main.rs", ORIGINAL.replace("one", "local"))
        commit(self.root)
        target = self.target(ORIGINAL.replace("seven", "upstream"))
        self.assertEqual(self.change(self.compare(target)).status, "merged")
        self.update(target)
        self.assertEqual((self.root / "src/main.rs").read_text(), ORIGINAL.replace("one", "local").replace("seven", "upstream"))

    def test_conflict_never_overwrites_live_files_or_baseline(self):
        write(self.root, "src/main.rs", ORIGINAL.replace("one", "local"))
        commit(self.root)
        target = self.target(ORIGINAL.replace("one", "upstream"))
        self.assertEqual(self.change(self.compare(target)).status, "conflict")
        before = self.snapshot()
        with self.assertRaisesRegex(upstream.SyncError, "conflict"):
            self.update(target)
        self.assertEqual(self.snapshot(), before)

    def test_addition_and_deletion_are_tracked(self):
        write(self.source, "apps/wm/src/new.rs", "new\n")
        (self.source / "apps/wm/src/main.rs").unlink()
        target = commit(self.source)
        comparison = self.compare(target)
        self.assertEqual(self.change(comparison, "src/new.rs").status, "added")
        self.assertEqual(self.change(comparison).status, "deleted")
        self.update(target)
        self.assertFalse((self.root / "src/main.rs").exists())
        self.assertEqual((self.root / "src/new.rs").read_text(), "new\n")
        entries = json.loads((self.root / "upstream/makepad.json").read_text())["files"]
        self.assertEqual({item["source"] for item in entries}, {"LICENSE", "apps/wm/src/new.rs"})

    def test_upstream_deletion_of_locally_modified_file_conflicts(self):
        write(self.root, "src/main.rs", "local modification\n")
        commit(self.root)
        (self.source / "apps/wm/src/main.rs").unlink()
        target = commit(self.source)
        before = self.snapshot()
        with self.assertRaisesRegex(upstream.SyncError, "conflict"):
            self.update(target)
        self.assertEqual(self.snapshot(), before)

    def test_local_deletion_and_upstream_edit_conflicts(self):
        (self.root / "src/main.rs").unlink()
        commit(self.root)
        target = self.target("upstream\n")
        self.assertEqual(self.change(self.compare(target)).status, "conflict")

    def test_new_upstream_path_cannot_overwrite_local_file_even_if_ignored(self):
        write(self.root, ".gitignore", "src/new.rs\n")
        commit(self.root)
        write(self.root, "src/new.rs", "precious ignored file\n")
        write(self.source, "apps/wm/src/new.rs", "new upstream file\n")
        target = commit(self.source)
        before = self.snapshot()
        self.assertEqual(self.change(self.compare(target), "src/new.rs").status, "conflict")
        with self.assertRaisesRegex(upstream.SyncError, "conflict"):
            self.update(target)
        self.assertEqual(self.snapshot(), before)

    def test_dirty_repository_refused(self):
        write(self.root, "new.txt", "untracked\n")
        with self.assertRaisesRegex(upstream.SyncError, "clean"):
            self.update(self.target())

    def test_mismatched_provenance_and_pins_reported(self):
        self.manifest["dependency_revision"] = "0" * 40
        self.save_manifest()
        self.assertTrue(any("revision" in problem for problem in self.compare().problems))
        self.manifest["dependency_revision"] = self.base
        self.save_manifest()
        path = self.root / "apps/reference/Cargo.toml"
        path.write_text(path.read_text().replace(self.base, "0" * 40))
        self.assertTrue(any("apps/reference/Cargo.toml" in problem for problem in self.compare().problems))

    def test_mismatched_hash_and_unrecorded_baseline_are_reported(self):
        self.manifest["files"][0]["sha256"] = "0" * 64
        self.save_manifest()
        self.assertTrue(any("hash" in problem for problem in self.compare().problems))
        self.manifest["files"].pop(0)
        self.save_manifest()
        self.assertTrue(any("unrecorded" in problem for problem in self.compare().problems))

    def test_verification_failure_is_transactional(self):
        before = self.snapshot()
        def fail(stage):
            self.fake_verify(stage)
            raise RuntimeError("fixture compilation failure")
        with self.assertRaisesRegex(upstream.SyncError, "verification"):
            self.update(self.target("upstream edit\n"), fail)
        self.assertEqual(self.snapshot(), before)

    def test_source_worktree_changes_are_never_read_or_modified(self):
        write(self.source, "apps/wm/src/main.rs", "uncommitted upstream edits\n")
        before = git(self.source, "diff")
        self.assertEqual(self.change(self.compare()).status, "unchanged")
        self.assertEqual(git(self.source, "diff"), before)

    def test_committed_local_only_edits_survive_dependency_only_upgrade(self):
        write(self.root, "src/main.rs", "local-only content\n")
        commit(self.root)
        target = self.target()
        self.update(target)
        self.assertEqual((self.root / "src/main.rs").read_text(), "local-only content\n")
        self.assertEqual(self.change(self.compare(target)).status, "local-only")

    def test_lockfile_revision_mismatch_blocks_update(self):
        self.write_lock(self.root, "0" * 40)
        commit(self.root)
        self.assertTrue(any("Cargo.lock" in problem for problem in self.compare().problems))
        with self.assertRaisesRegex(upstream.SyncError, "Cargo.lock"):
            self.update(self.target())

    def test_verifier_must_produce_matching_lockfile(self):
        before = self.snapshot()
        with self.assertRaisesRegex(upstream.SyncError, "Cargo.lock"):
            self.update(self.target(), lambda stage: None)
        self.assertEqual(self.snapshot(), before)

    def test_concurrent_changes_during_verification_are_preserved(self):
        target = self.target()
        def concurrent_edit(stage):
            self.fake_verify(stage)
            write(self.root, "src/main.rs", "written during verification\n")
        with self.assertRaisesRegex(upstream.SyncError, "clean|changed"):
            self.update(target, concurrent_edit)
        self.assertEqual((self.root / "src/main.rs").read_text(), "written during verification\n")
        self.assertEqual((self.root / "upstream/makepad.json").read_text(), self.live_baseline)

    def test_apply_failure_rolls_back_previously_written_files(self):
        before = self.snapshot()
        real_put = upstream.put
        failed = False
        def failing_put(root, name, data, mode=0o644):
            nonlocal failed
            if root == self.root and name == "src/main.rs" and not failed:
                failed = True
                raise OSError("fixture disk error")
            return real_put(root, name, data, mode)
        with patch.object(upstream, "put", failing_put):
            with self.assertRaisesRegex(upstream.SyncError, "fixture disk error"):
                self.update(self.target("upstream edit\n"))
        self.assertTrue(failed)
        self.assertEqual(self.snapshot(), before)

    def test_failed_rollback_reports_recovery_instead_of_claiming_unchanged(self):
        real_put = upstream.put
        failed = False
        def failing_put(root, name, data, mode=0o644):
            nonlocal failed
            if root == self.root and name == "src/main.rs":
                failed = True
                raise OSError("fixture disk failure")
            if root == self.root and name == "Cargo.toml" and failed:
                raise OSError("fixture rollback failure")
            return real_put(root, name, data, mode)
        with patch.object(upstream, "put", failing_put):
            with self.assertRaisesRegex(upstream.SyncError, "recovery") as caught:
                self.update(self.target("upstream edit\n"))
        self.assertNotIn("were not advanced", str(caught.exception))

    def test_ordinary_dependency_tables_are_supported(self):
        write(self.root, "apps/reference/Cargo.toml", '[package]\nname = "reference"\nversion = "0.1.0"\n[dependencies.makepad-widgets]\ngit = "%s"\nrev = "%s"\n' % (REPOSITORY.removesuffix('.git'), self.base))
        commit(self.root)
        self.update(self.target())
        self.assertIn(self.next_revision, (self.root / "apps/reference/Cargo.toml").read_text())

    def test_omissions_are_explicit_and_retained(self):
        self.manifest["files"].pop(0)
        self.manifest["omissions"] = [{"source": "apps/wm/src/main.rs", "reason": "fixture omission"}]
        self.save_manifest()
        commit(self.root)
        target = self.target("upstream change to omitted file\n")
        self.assertFalse(self.compare(target).problems)
        self.update(target)
        self.assertEqual((self.root / "src/main.rs").read_text(), ORIGINAL)
        self.assertEqual(json.loads((self.root / "upstream/makepad.json").read_text())["omissions"], self.manifest["omissions"])

    def test_symlinks_and_escape_paths_are_refused(self):
        (self.root / "src/main.rs").unlink()
        (self.root / "src/main.rs").symlink_to(self.source / "LICENSE")
        with self.assertRaisesRegex(upstream.SyncError, "symlink"):
            self.compare()
        (self.root / "src/main.rs").unlink()
        self.manifest["files"][0]["destination"] = "../outside.rs"
        self.save_manifest()
        with self.assertRaisesRegex(upstream.SyncError, "unsafe"):
            self.compare()

    def test_ignored_recorded_destination_must_be_tracked_before_update(self):
        git(self.root, "rm", "--cached", "src/main.rs")
        write(self.root, ".gitignore", "src/main.rs\n")
        commit(self.root)
        before = self.snapshot()
        with self.assertRaisesRegex(upstream.SyncError, "tracked"):
            self.update(self.target("upstream edit\n"))
        self.assertEqual(self.snapshot(), before)

    def test_default_verification_resolves_before_locked_check(self):
        # A fake Cargo executable records subprocess arguments. Fixtures never
        # contact a registry or build Makepad.
        stage = Path(self.temp.name) / "stage"
        stage.mkdir()
        bin_dir = Path(self.temp.name) / "bin"
        bin_dir.mkdir()
        cargo = bin_dir / "cargo"
        cargo.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> commands.log\n')
        cargo.chmod(0o755)
        with patch.dict("os.environ", {"PATH": str(bin_dir)}):
            upstream.verify_stage(stage)
        self.assertEqual((stage / "commands.log").read_text().splitlines(), [
            "metadata --format-version 1", "check --locked --workspace",
            "test --locked --workspace --quiet",
        ])


class DailySyncTests(RepoFixture):
    def setUp(self):
        super().setUp()
        write(self.root, ".gitignore", "/target/\n")
        commit(self.root)

    def snapshot(self):
        return {name: data for name, data in super().snapshot().items()
                if not name.startswith("target/")}

    def sync(self, target=None, verify=None):
        self.next_revision = target or git(self.source, "rev-parse", "HEAD")
        self.live_baseline = (self.root / upstream.BASELINE).read_text()
        return upstream.sync(self.root, to=target, verify=verify or self.fake_verify)

    def test_cli_sync_defaults_to_sibling_head_and_noops(self):
        result = subprocess.run([sys.executable, upstream.__file__, "sync", "--root", str(self.root)],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Already", result.stdout)
        self.assertIn(self.base, result.stdout)
        self.assertFalse((self.root / "target").exists())

    def test_noop_keeps_uncommitted_work_and_does_not_verify(self):
        write(self.root, "src/main.rs", "work in progress\n")
        before = self.snapshot()
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            self.assertIsNone(self.sync(verify=lambda _: self.fail("no-op must not build")))
        self.assertIn("uncommitted", output.getvalue())
        self.assertEqual(self.snapshot(), before)

    def test_success_leaves_unstaged_upgrade_on_new_branch_with_report(self):
        target = self.target(ORIGINAL.replace("one", "upstream"))
        starting_head = git(self.root, "rev-parse", "HEAD")
        source_before = git(self.source, "status", "--porcelain")
        report = self.sync()
        self.assertEqual(git(self.root, "branch", "--show-current"), f"sync/makepad-{target[:12]}")
        self.assertEqual(git(self.root, "rev-parse", "HEAD"), starting_head, "sync must not commit")
        self.assertEqual(git(self.root, "diff", "--cached"), "")
        self.assertEqual(json.loads((self.root / upstream.BASELINE).read_text())["revision"], target)
        self.assertIn("upstream changes", (report / "comparison.txt").read_text())
        self.assertIn("READY", (report / "summary.txt").read_text())
        self.assertEqual(git(self.source, "status", "--porcelain"), source_before)

    def test_new_revision_requires_clean_octosense(self):
        self.target()
        write(self.root, "src/main.rs", "unfinished local edit\n")
        before = self.snapshot()
        with self.assertRaisesRegex(upstream.SyncError, "clean"):
            self.sync()
        self.assertEqual(self.snapshot(), before)

    def test_failed_runtime_verification_keeps_original_branch_and_candidate(self):
        self.target()
        before = self.snapshot()
        branch = git(self.root, "branch", "--show-current")
        def fail_smoke(stage):
            self.fake_verify(stage)
            raise upstream.SyncError("GUI smoke failed")
        with self.assertRaisesRegex(upstream.SyncError, "GUI smoke failed"):
            self.sync(verify=fail_smoke)
        self.assertEqual(self.snapshot(), before)
        self.assertEqual(git(self.root, "branch", "--show-current"), branch)
        reports = list((self.root / "target/makepad-sync/reports").iterdir())
        self.assertEqual(len(reports), 1)
        self.assertIn("FAILED", (reports[0] / "summary.txt").read_text())
        self.assertTrue((reports[0] / "project/Cargo.toml").is_file())

    def test_conflict_is_reported_without_creating_branch_or_running_checks(self):
        write(self.root, "src/main.rs", ORIGINAL.replace("one", "local"))
        commit(self.root)
        self.target(ORIGINAL.replace("one", "upstream"))
        branches = git(self.root, "branch")
        before = self.snapshot()
        with self.assertRaisesRegex(upstream.SyncError, "conflict"):
            self.sync(verify=lambda _: self.fail("conflicts must stop before checks"))
        self.assertEqual(git(self.root, "branch"), branches)
        self.assertEqual(self.snapshot(), before)
        candidates = list((self.root / "target/makepad-sync/reports").glob("*/project/src/main.rs"))
        self.assertIn("<<<<<<< OctoSense", candidates[0].read_text())

    def test_cache_reused_but_stale_candidate_source_removed(self):
        self.target("first update\n")
        def first(stage):
            self.fake_verify(stage)
            write(stage, "target/cache-marker", "cached build\n")
            write(stage, "stale-candidate-file", "must disappear\n")
        first_report = self.sync(verify=first)
        commit(self.root)
        self.target("second update\n")
        def second(stage):
            self.assertEqual((stage / "target/cache-marker").read_text(), "cached build\n")
            self.assertFalse((stage / "stale-candidate-file").exists())
            self.fake_verify(stage)
        second_report = self.sync(verify=second)
        self.assertNotEqual(first_report, second_report)
        self.assertTrue((first_report / "comparison.txt").exists())

    def test_existing_branch_is_not_reused_or_overwritten(self):
        target = self.target()
        branch = f"sync/makepad-{target[:12]}"
        git(self.root, "branch", branch)
        original = git(self.root, "rev-parse", branch)
        self.sync()
        self.assertEqual(git(self.root, "branch", "--show-current"), branch + "-2")
        self.assertEqual(git(self.root, "rev-parse", branch), original)

    def test_concurrent_branch_switch_stops_apply_even_if_files_match(self):
        self.target()
        before = self.snapshot()
        def switched(stage):
            self.fake_verify(stage)
            git(self.root, "switch", "-c", "human-branch")
        with self.assertRaisesRegex(upstream.SyncError, "HEAD|branch|changed"):
            self.sync(verify=switched)
        self.assertEqual(git(self.root, "branch", "--show-current"), "human-branch")
        self.assertEqual(self.snapshot(), before)

    def test_sync_lock_rejects_overlap_and_releases_on_exit(self):
        self.target()
        with upstream.sync_lock(self.root / "target/makepad-sync"):
            with self.assertRaisesRegex(upstream.SyncError, "running"):
                self.sync()
        self.assertIsNotNone(self.sync())

    def test_archiving_failure_preserves_original_error_and_restores_branch(self):
        self.target()
        branch = git(self.root, "branch", "--show-current")
        before = self.snapshot()
        with patch.object(upstream, "apply", side_effect=upstream.SyncError("fixture apply failed")):
            with patch.object(upstream.shutil, "copytree", side_effect=OSError("snapshot unavailable")):
                with self.assertRaises(upstream.SyncError) as caught:
                    self.sync()
        self.assertIn("fixture apply failed", str(caught.exception))
        self.assertIn("snapshot unavailable", str(caught.exception))
        self.assertEqual(git(self.root, "branch", "--show-current"), branch)
        self.assertEqual(self.snapshot(), before)

    @unittest.skipUnless(os.name == "posix", "native sync uses POSIX process groups")
    def test_interrupted_verifier_waits_for_smoke_cleanup(self):
        stage = Path(self.temp.name) / "interrupt-stage"
        stage.mkdir()
        bin_dir = Path(self.temp.name) / "interrupt-bin"
        bin_dir.mkdir()
        cargo = bin_dir / "cargo"
        cargo.write_text("#!/bin/sh\nexit 0\n")
        cargo.chmod(0o755)
        write(stage, "scripts/smoke.py", 'import subprocess, sys, time\nfrom pathlib import Path\n'
              'child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"], start_new_session=True)\n'
              'Path("sleeper.pid").write_text(str(child.pid))\n'
              'try:\n    time.sleep(60)\n'
              'finally:\n    time.sleep(1)\n    child.terminate()\n    child.wait(timeout=3)\n    Path("cleanup-done").touch()\n')
        driver = ('import sys\nfrom pathlib import Path\n'
                  f'sys.path.insert(0, {str(Path(upstream.__file__).parent)!r})\n'
                  f'import upstream\nupstream.verify_stage(Path({str(stage)!r}), runtime=True)\n')
        env = dict(os.environ, PATH=str(bin_dir))
        with (stage / "driver.log").open("wb") as output:
            process = subprocess.Popen([sys.executable, "-c", driver], env=env,
                                       stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
        child_pid = None
        try:
            deadline = time.monotonic() + 10
            while not (stage / "sleeper.pid").exists() and time.monotonic() < deadline:
                time.sleep(0.02)
            self.assertTrue((stage / "sleeper.pid").exists(), (stage / "driver.log").read_text())
            child_pid = int((stage / "sleeper.pid").read_text())
            os.killpg(process.pid, signal.SIGINT)
            process.wait(timeout=10)
            self.assertTrue((stage / "cleanup-done").exists(), "supervisor returned before smoke cleanup completed")
            with self.assertRaises(ProcessLookupError):
                os.kill(child_pid, 0)
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=3)
            if child_pid:
                try:
                    os.killpg(child_pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass

    def test_runtime_verification_builds_both_profiles_before_both_smokes(self):
        stage = Path(self.temp.name) / "stage"
        stage.mkdir()
        bin_dir = Path(self.temp.name) / "bin"
        bin_dir.mkdir()
        cargo = bin_dir / "cargo"
        cargo.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> commands.log\n')
        cargo.chmod(0o755)
        write(stage, "scripts/smoke.py", 'import json, os, sys\nfrom pathlib import Path\n'
              'with Path("smokes.jsonl").open("a") as out: out.write(json.dumps([sys.argv[1:], os.environ["CARGO_TARGET_DIR"]]) + "\\n")\n')
        report = Path(self.temp.name) / "report"
        report.mkdir()
        with patch.dict("os.environ", {"PATH": str(bin_dir), "CARGO_TARGET_DIR": "/unrelated/build"}):
            upstream.verify_stage(stage, runtime=True, report=report)
        self.assertEqual((stage / "commands.log").read_text().splitlines(), [
            "metadata --format-version 1", "check --locked --workspace", "test --locked --workspace --quiet",
            "build --release --locked --workspace", "build --locked --workspace",
        ])
        smokes = [json.loads(line) for line in (stage / "smokes.jsonl").read_text().splitlines()]
        self.assertEqual(smokes[0], [["--styles", "--artifacts-dir", str(report / "smoke-release")], str(stage / "target")])
        self.assertEqual(smokes[1], [["--cargo-run", "--default-catalog", "--artifacts-dir", str(report / "smoke-default")], str(stage / "target")])


if __name__ == "__main__":
    unittest.main()


UPSTREAM_REGISTRY = """
fn curated() -> Vec<AppDef> {
    use LaunchPolicy::*;
    vec![
        AppDef::app("browser", "Browser", "makepad-browser", "apps/browser", "browser", OrFocus),
        {
            // The demo VFS, never the real disk.
            let mut files =
                AppDef::app("files", "Files", "makepad-files", "apps/files", "files", OrFocus);
            if std::env::var("MAKEPAD_WM_FILES_REAL").is_err() {
                files.args.push("--demo".to_string());
            }
            files
        },
        AppDef::app(
            "score",
            "Score",
            "makepad-app-score",
            "apps/score",
            "makepad-app-score",
            OrFocus,
        ),
        AppDef::app("terminal", "Terminal", "makepad-terminal", "apps/terminal", "terminal", AlwaysNew),
    ]
}

pub fn find_app(id: &str) -> Option<AppDef> {
    match id {
        "image" => Some(AppDef::app("image", "Image Viewer", "makepad-image", "apps/image", "image", AlwaysNew)),
        _ => None,
    }
}
"""


class CuratedApps(unittest.TestCase):
    def test_rows_come_from_the_registry_and_not_the_hidden_entries(self):
        """`find_app` registers viewers that are deliberately not menu rows;
        reading them here would put them in the launcher by accident."""
        self.assertEqual(
            upstream.curated_apps(UPSTREAM_REGISTRY),
            [
                {"id": "browser", "label": "Browser", "source": "makepad",
                 "package": "makepad-browser", "bin": "browser", "policy": "focus"},
                {"id": "files", "label": "Files", "source": "makepad",
                 "package": "makepad-files", "bin": "files", "policy": "focus",
                 "args": ["--demo"]},
                {"id": "score", "label": "Score", "source": "makepad",
                 "package": "makepad-app-score", "bin": "makepad-app-score",
                 "policy": "focus"},
                {"id": "terminal", "label": "Terminal", "source": "makepad",
                 "package": "makepad-terminal", "bin": "terminal", "policy": "new"},
            ],
        )

    def test_arguments_computed_at_run_time_are_left_out_entirely(self):
        """Fab opens a model whose path upstream computes at launch. Taking
        the flag without its value would emit a switch with nothing after
        it, so the row travels with no arguments at all."""
        source = """
fn curated() -> Vec<AppDef> {
    vec![
        {
            let mut fab = AppDef::app("fab", "Fab", "makepad-fab", "apps/fab", "makepad-fab", OrFocus);
            let house = "local/fab/models/woodside.glb";
            if exists {
                fab.args.push("--open".to_string());
                fab.args.push(house.to_string());
            }
            fab
        },
    ]
}
"""
        self.assertEqual(
            upstream.curated_apps(source),
            [{"id": "fab", "label": "Fab", "source": "makepad",
              "package": "makepad-fab", "bin": "makepad-fab", "policy": "focus"}],
        )

OVERLAY = {
    "overrides": {"studio": {"label": "Director", "package": "makepad-director", "bin": "director"}},
    "drop": ["scope"],
    "rows": [
        {"id": "reference", "label": "Reference", "manifest": "../apps/reference/Cargo.toml",
         "package": "octosense-reference", "bin": "octosense-reference", "policy": "new"}
    ],
}
class MergeCatalog(unittest.TestCase):
    def test_this_project_renames_drops_and_adds_without_losing_upstream_order(self):
        upstream_rows = [
            {"id": "browser", "label": "Browser", "source": "makepad",
             "package": "makepad-browser", "bin": "browser", "policy": "focus"},
            {"id": "studio", "label": "Studio", "source": "makepad",
             "package": "makepad-studio", "bin": "studio", "policy": "focus"},
            {"id": "scope", "label": "Scope", "source": "makepad",
             "package": "makepad-scope", "bin": "scope", "policy": "focus"},
        ]
        merged = upstream.merge_catalog(upstream_rows, OVERLAY)
        self.assertEqual([row["id"] for row in merged], ["reference", "browser", "studio"])
        studio = next(row for row in merged if row["id"] == "studio")
        # The id is a launch reference people already use; only what it runs
        # changes when upstream replaces the app behind it.
        self.assertEqual(studio["package"], "makepad-director")
        self.assertEqual(studio["bin"], "director")
        self.assertEqual(studio["label"], "Director")
        self.assertEqual(studio["source"], "makepad")

    def test_helper_rows_follow_the_curated_menu(self):
        """Viewers and the assistant are reachable but belong after the
        apps upstream curates, not ahead of them."""
        overlay = dict(OVERLAY, append=[
            {"id": "pdf", "label": "PDF Viewer", "source": "makepad",
             "package": "makepad-pdf", "bin": "pdf", "policy": "new"}
        ])
        merged = upstream.merge_catalog(
            [{"id": "browser", "label": "Browser", "source": "makepad",
              "package": "makepad-browser", "bin": "browser", "policy": "focus"}],
            overlay,
        )
        self.assertEqual([row["id"] for row in merged], ["reference", "browser", "pdf"])

class CatalogProblems(unittest.TestCase):
    def test_rows_naming_a_crate_the_revision_does_not_build_are_reported(self):
        """Upstream's registry can name a binary its own revision no longer
        produces; a row like that is a menu entry that cannot start."""
        packages = {"makepad-browser": ["browser"], "makepad-app-route": ["route"]}
        rows = [
            {"id": "browser", "source": "makepad", "package": "makepad-browser", "bin": "browser"},
            {"id": "route", "source": "makepad", "package": "makepad-app-route", "bin": "makepad-app-route"},
            {"id": "studio", "source": "makepad", "package": "makepad-studio", "bin": "studio"},
            {"id": "reference", "manifest": "../apps/reference/Cargo.toml",
             "package": "octosense-reference", "bin": "octosense-reference"},
        ]
        problems = upstream.catalog_problems(rows, packages)
        self.assertEqual([problem["id"] for problem in problems], ["route", "studio"])
        self.assertIn("route", problems[0]["detail"])
        self.assertIn("makepad-studio", problems[1]["detail"])


class PinnedCheckout(unittest.TestCase):
    def test_sibling_repositories_are_not_mistaken_for_makepad(self):
        self.assertEqual(
            upstream.git_repo_name(
                "git+https://github.com/OctoSense-org/makepad.git"
                "?rev=ad8f3729d#ad8f3729d"),
            "makepad",
        )
        for other in (
            "git+https://github.com/Project-Robius-China/makepad-diagram-kit.git?rev=0536492#0536492",
            "git+https://github.com/OctoSense-org/Octoscript-Makepad.git?rev=abc123#abc123",
        ):
            self.assertNotEqual(upstream.git_repo_name(other), "makepad")
        self.assertIsNone(upstream.git_repo_name("registry+https://github.com/rust-lang/crates.io-index"))


class PathOverrideCheckout(unittest.TestCase):
    def test_shared_runtime_path_override_is_found(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve() / "app"
            framework = Path(directory).resolve() / "shared-framework"
            write(root, "Cargo.toml", '''[package]
name = "catalog-fixture"
version = "0.1.0"
edition = "2021"
[dependencies]
makepad-widgets = { path = "../shared-framework/widgets" }
''')
            write(root, "src/lib.rs", "")
            write(framework, "Cargo.toml", '[workspace]\nmembers = ["widgets"]\n')
            write(framework, "widgets/Cargo.toml", '''[package]
name = "makepad-widgets"
version = "0.1.0"
edition = "2021"
''')
            write(framework, "widgets/src/lib.rs", "")
            write(framework, "apps/wm/Cargo.toml", "[package]\n")
            upstream.run(["cargo", "generate-lockfile", "--offline"], root)
            self.assertEqual(upstream.pinned_checkout(root), framework)


class ShippedCatalog(unittest.TestCase):
    def test_the_shipped_catalog_matches_the_pinned_revision(self):
        """The generated rows are the ones in the tree. Drift here means the
        catalog and the revision disagree about what the launcher can start,
        which is the duplication this generation exists to remove."""
        root = Path(upstream.__file__).resolve().parents[1]
        try:
            generated, problems = upstream.generated_catalog(root)
        except upstream.SyncError as error:
            self.skipTest(f"pinned checkout unavailable: {error}")
        self.assertEqual(problems, [])
        self.assertEqual(json.loads((root / "config/apps.json").read_text()), generated)
