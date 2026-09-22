import argparse
import hashlib
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("setup_native", ROOT / "home/tools/setup-native.py")
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


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


if __name__ == "__main__":
    unittest.main()
