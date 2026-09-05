"""Local-only Git fixtures for the Makepad import maintenance tool."""
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
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


class SyncTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve() / "makeos"
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


if __name__ == "__main__":
    unittest.main()
