import argparse
import hashlib
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("setup_native", ROOT / "tools/setup.py")
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def isolated_environment(config):
    """No hub settings from the machine running the tests."""
    env = {k: v for k, v in os.environ.items() if not k.startswith("OCTOSENSE_")}
    env["OCTOSENSE_SOURCES_CONFIG"] = str(config)
    return patch.dict(os.environ, env, clear=True)


class RuntimeSources(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.source = self.root / "makepad"
        self.source.mkdir()
        self.git("init", "--quiet")
        self.git("config", "user.name", "Fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.file = self.source / "policy.txt"
        self.file.write_text("before\n")
        self.git("add", "policy.txt")
        self.git("commit", "--quiet", "-m", "Fixture base")
        self.revision = self.git("rev-parse", "HEAD").strip()
        self.file.write_text("after\n")
        self.patch = self.root / "policy.patch"
        self.patch.write_text(self.git("diff", "--binary"))
        self.git("add", "policy.txt")
        tree = self.git("write-tree").strip()
        self.git("restore", "--staged", "--worktree", "policy.txt")
        self.overlay = {"base_revision": self.revision, "patch": "policy.patch",
                        "sha256": hashlib.sha256(self.patch.read_bytes()).hexdigest(), "tree": tree}
        self.spec = {"revision": self.revision, "url": "https://example.invalid/unused"}
        self.args = argparse.Namespace(check=False, update=False, cache=None)
        self.product = patch.object(native, "PRODUCT", self.root)
        self.product.start()
        self.addCleanup(self.product.stop)
        environment = isolated_environment(self.root / "no-config.json")
        environment.start()
        self.addCleanup(environment.stop)

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.source), *args], text=True)

    def prepare(self):
        native.prepare_source(self.root, "makepad", self.spec, self.args, self.overlay)

    def test_applies_the_recorded_patch_and_check_is_read_only(self):
        self.prepare()
        self.assertEqual(self.file.read_text(), "after\n")
        self.args.check = True
        before = self.git("status", "--porcelain")
        self.prepare()
        self.assertEqual(self.git("status", "--porcelain"), before)

    def test_rejects_modified_patch_bytes(self):
        self.patch.write_text(self.patch.read_text() + "\n")
        with self.assertRaisesRegex(RuntimeError, "patch does not match"):
            self.prepare()
        self.assertEqual(self.file.read_text(), "before\n")

    def test_preserves_unstaged_changes(self):
        self.prepare()
        self.file.write_text("local work\n")
        with self.assertRaisesRegex(RuntimeError, "Preserving local changes"):
            self.prepare()
        self.assertEqual(self.file.read_text(), "local work\n")

    def test_rejects_additional_staged_changes(self):
        self.prepare()
        self.file.write_text("local work\n")
        self.git("add", "policy.txt")
        with self.assertRaisesRegex(RuntimeError, "Preserving staged changes"):
            self.prepare()

    def test_check_requires_the_patch_without_applying_it(self):
        self.args.check = True
        with self.assertRaisesRegex(RuntimeError, "not applied"):
            self.prepare()
        self.assertEqual(self.file.read_text(), "before\n")


def run(*args, cwd=None):
    return subprocess.check_output(["git", *args], cwd=cwd, text=True, stderr=subprocess.STDOUT).strip()


class HubWorktrees(unittest.TestCase):
    """A local clone (the hub) serves .sources/<name> as a worktree."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        base = Path(self.temp.name).resolve()
        self.product_dir = base / "product"
        self.product_dir.mkdir()
        # The remote the pins name, a full local clone of it (the hub) and the
        # product's .sources/.
        self.upstream = base / "upstream.git"
        run("init", "--quiet", "--bare", str(self.upstream))
        run("-C", str(self.upstream), "config", "uploadpack.allowAnySHA1InWant", "true")
        work = base / "work"
        run("clone", "--quiet", str(self.upstream), str(work))
        run("-C", str(work), "config", "user.name", "Fixture")
        run("-C", str(work), "config", "user.email", "fixture@example.invalid")
        self.file = "policy.txt"
        self.commits = []
        for text in ("one\n", "two\n"):
            (work / self.file).write_text(text)
            run("-C", str(work), "add", self.file)
            run("-C", str(work), "commit", "--quiet", "-m", text.strip())
            self.commits.append(run("-C", str(work), "rev-parse", "HEAD"))
        run("-C", str(work), "push", "--quiet", "origin", "HEAD:refs/heads/main")
        self.work = work
        self.hubs = base / "hubs"
        self.hub = self.hubs / "makepad"
        run("clone", "--quiet", str(self.upstream), str(self.hub))
        self.root = self.product_dir / ".sources"
        self.path = self.root / "makepad"
        self.spec = {"revision": self.commits[0], "url": str(self.upstream)}
        self.args = argparse.Namespace(check=False, update=False, cache=None, hub=None, no_hub=False, convert=False)
        self.config = base / "sources.json"
        environment = isolated_environment(self.config)
        environment.start()
        self.addCleanup(environment.stop)
        product = patch.object(native, "PRODUCT", self.product_dir)
        product.start()
        self.addCleanup(product.stop)

    def push_new_commit(self, text="three\n"):
        (self.work / self.file).write_text(text)
        run("-C", str(self.work), "commit", "--quiet", "-am", text.strip())
        run("-C", str(self.work), "push", "--quiet", "origin", "HEAD:refs/heads/main")
        return run("-C", str(self.work), "rev-parse", "HEAD")

    def prepare(self, overlay=None):
        with contextlib.redirect_stderr(io.StringIO()) as err:
            native.prepare_source(self.root, "makepad", self.spec, self.args, overlay)
        return err.getvalue()

    def assert_worktree_of_hub(self, revision):
        self.assertTrue((self.path / ".git").is_file(), "expected a worktree, found a clone")
        self.assertEqual(native.common_dir(self.path), native.common_dir(self.hub))
        self.assertEqual(run("-C", str(self.path), "rev-parse", "HEAD"), revision)
        self.assertEqual(run("-C", str(self.hub), "rev-parse", "--is-shallow-repository"), "false")

    def hub_worktrees(self):
        listed = run("-C", str(self.hub), "worktree", "list", "--porcelain").splitlines()
        return [Path(line[len("worktree "):]) for line in listed if line.startswith("worktree ")]

    def test_without_a_hub_it_still_clones(self):
        self.prepare()
        self.assertTrue((self.path / ".git").is_dir())
        self.assertEqual(run("-C", str(self.path), "rev-parse", "HEAD"), self.commits[0])
        self.assertEqual(self.hub_worktrees(), [self.hub])

    def test_hub_directory_from_the_environment(self):
        os.environ["OCTOSENSE_SOURCES_HUB"] = str(self.hubs)
        self.prepare()
        self.assert_worktree_of_hub(self.commits[0])
        self.assertIn(self.path, self.hub_worktrees())

    def test_per_source_environment_and_config_file(self):
        os.environ["OCTOSENSE_MAKEPAD_HUB"] = str(self.hub)
        self.assertEqual(native.configured_hub("makepad", self.args), self.hub)
        del os.environ["OCTOSENSE_MAKEPAD_HUB"]
        self.config.write_text(json.dumps({"repositories": {"makepad": str(self.hub)}}))
        self.assertEqual(native.configured_hub("makepad", self.args), self.hub)
        self.assertIsNone(native.configured_hub("octoscript", self.args))
        self.config.write_text(json.dumps({"hub": str(self.hubs)}))
        self.args.no_hub = True
        self.assertIsNone(native.configured_hub("makepad", self.args))
        self.args.no_hub = False
        self.prepare()
        self.assert_worktree_of_hub(self.commits[0])

    def test_fetches_a_new_pin_into_the_hub_without_making_it_shallow(self):
        self.args.hub = str(self.hubs)
        self.spec["revision"] = self.push_new_commit()
        self.prepare()
        self.assert_worktree_of_hub(self.spec["revision"])

    def test_check_creates_nothing(self):
        self.args.hub = str(self.hubs)
        self.args.check = True
        with self.assertRaisesRegex(RuntimeError, "Expected an empty dependency directory"):
            self.prepare()
        self.assertFalse(self.path.exists())
        self.assertEqual(self.hub_worktrees(), [self.hub])

    def test_update_moves_the_worktree(self):
        self.args.hub = str(self.hubs)
        self.prepare()
        self.spec["revision"] = self.push_new_commit()
        with self.assertRaisesRegex(RuntimeError, "selects another revision"):
            self.prepare()
        self.args.update = True
        self.prepare()
        self.assert_worktree_of_hub(self.spec["revision"])
        self.args.update, self.args.check = False, True
        self.prepare()

    def test_applies_the_reviewed_patch_in_a_worktree(self):
        self.args.hub = str(self.hubs)
        patch_file = self.product_dir / "policy.patch"
        patch_file.write_text(run("-C", str(self.hub), "diff", "--binary", self.commits[0], self.commits[1]) + "\n")
        overlay = {"base_revision": self.commits[0], "patch": "policy.patch",
                   "sha256": hashlib.sha256(patch_file.read_bytes()).hexdigest(),
                   "tree": run("-C", str(self.hub), "rev-parse", self.commits[1] + "^{tree}")}
        self.prepare(overlay)
        self.assertEqual((self.path / self.file).read_text(), "two\n")
        self.args.check = True
        self.prepare(overlay)
        # The staged patch is setup's own state, so removal accepts it.
        with contextlib.redirect_stderr(io.StringIO()):
            native.remove_worktrees(self.root, {"makepad": overlay})
        self.assertFalse(self.path.exists())
        self.assertEqual(self.hub_worktrees(), [self.hub])

    def test_a_stale_registration_does_not_block_adding_again(self):
        self.args.hub = str(self.hubs)
        self.prepare()
        subprocess.check_call(["rm", "-rf", str(self.path)])
        self.prepare()
        self.assert_worktree_of_hub(self.commits[0])

    def test_rejects_a_hub_of_another_repository(self):
        run("-C", str(self.hub), "remote", "set-url", "origin", "https://example.invalid/other.git")
        self.args.hub = str(self.hubs)
        with self.assertRaisesRegex(RuntimeError, "is not a clone of"):
            self.prepare()
        self.assertFalse(self.path.exists())

    def test_a_missing_hub_is_an_error_not_a_clone(self):
        self.args.hub = str(self.product_dir / "nowhere")
        with self.assertRaisesRegex(RuntimeError, "No hub clone for makepad"):
            self.prepare()
        self.assertFalse(self.path.exists())

    def test_reports_an_existing_clone_and_converts_it_on_request(self):
        self.prepare()  # no hub yet: a standalone clone
        self.args.hub = str(self.hubs)
        self.assertIn("standalone clone", self.prepare())
        self.assertTrue((self.path / ".git").is_dir())
        self.args.convert = True
        self.prepare()
        self.assert_worktree_of_hub(self.commits[0])
        self.assertEqual(list(self.root.iterdir()), [self.path])

    def test_conversion_preserves_local_work(self):
        self.prepare()
        (self.path / "notes.txt").write_text("mine\n")
        self.args.hub, self.args.convert = str(self.hubs), True
        with self.assertRaisesRegex(RuntimeError, "Not converting .*untracked"):
            self.prepare()
        self.assertTrue((self.path / ".git").is_dir())
        self.assertEqual((self.path / "notes.txt").read_text(), "mine\n")

    def test_remove_worktrees_keeps_local_work_and_clones(self):
        self.args.hub = str(self.hubs)
        self.prepare()
        (self.path / "notes.txt").write_text("mine\n")
        with self.assertRaisesRegex(RuntimeError, "Not removing"):
            native.remove_worktrees(self.root, {})
        self.assertTrue((self.path / "notes.txt").exists())
        (self.path / "notes.txt").unlink()
        clone = self.root / "octoscript"
        run("clone", "--quiet", str(self.upstream), str(clone))
        with contextlib.redirect_stderr(io.StringIO()):
            native.remove_worktrees(self.root, {})
        self.assertFalse(self.path.exists())
        self.assertTrue((clone / ".git").is_dir())
        self.assertEqual(self.hub_worktrees(), [self.hub])


if __name__ == "__main__":
    unittest.main()
