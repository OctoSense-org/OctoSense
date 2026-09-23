"""Offline Git fixtures for the consumer's explicit framework override."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest


SPEC = importlib.util.spec_from_file_location("setup_native", Path(__file__).with_name("setup-native.py"))
setup = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(setup)


def git(path, *args):
    return subprocess.check_output(["git", "-C", str(path), *args], stderr=subprocess.PIPE, text=True).strip()


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def commit(path):
    git(path, "add", "--all")
    git(path, "commit", "-qm", "fixture")
    return git(path, "rev-parse", "HEAD")


class OverrideTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.consumer = self.root / "consumer"
        self.consumer.mkdir()
        self.repos = {}
        for name in ("makepad", "octoscript", "octoscript-makepad"):
            path = self.root / name
            path.mkdir()
            git(path, "init", "-q")
            git(path, "config", "user.email", "fixture@example.invalid")
            git(path, "config", "user.name", "Fixture")
            write(path / "README", name)
            self.repos[name] = path
        self.base = commit(self.repos["makepad"])
        write(self.repos["makepad"] / "policy.rs", "containment")
        self.override = commit(self.repos["makepad"])
        git(self.repos["makepad"], "checkout", "-q", "--detach", self.base)
        self.octo = commit(self.repos["octoscript"])
        self.sources = {
            "makepad": {"url": "https://github.com/OctoSense-org/makepad.git", "revision": self.base},
            "octoscript": {"url": "https://github.com/OctoSense-org/Octoscript.git", "revision": self.octo},
        }
        runtime = self.repos["octoscript-makepad"]
        write(runtime / "runtime.json", json.dumps({"schema_version": 1, "repositories": self.sources}))
        write(runtime / "Cargo.toml", '[workspace.dependencies]\n' + '\n'.join(
            '%s = { git = "%s", rev = "%s" }' % (package, self.sources[source]["url"], self.sources[source]["revision"])
            for package, source in (("makepad-widgets", "makepad"), ("makepad-script", "makepad"),
                                    ("octoscript-ui-l0", "octoscript"))))
        self.wrapper = commit(runtime)
        self.lock = {
            "schema_version": 1, "url": "https://github.com/OctoSense-org/Octoscript-Makepad.git",
            "revision": self.wrapper,
            "makepad_override": {"url": self.sources["makepad"]["url"], "revision": self.override,
                                 "reason": "Required App Hub containment APIs"},
        }
        self.save_lock()
        self.save_cargo(self.override)

    def save_lock(self):
        write(self.consumer / "native-runtime.lock.json", json.dumps(self.lock))

    def save_cargo(self, revision):
        write(self.consumer / "Cargo.toml", '[dependencies]\nmakepad-widgets = { git = "%s", rev = "%s" }\n' %
              (self.sources["makepad"]["url"], revision))

    def run_setup(self, **kwargs):
        return setup.setup(self.root, self.consumer, **kwargs)

    def test_override_prepares_exact_commit_without_modifying_wrapper(self):
        result = self.run_setup(update=True)
        self.assertEqual(git(self.repos["makepad"], "rev-parse", "HEAD"), self.override)
        self.assertEqual(git(self.repos["octoscript-makepad"], "rev-parse", "HEAD"), self.wrapper)
        self.assertEqual(git(self.repos["octoscript-makepad"], "status", "--porcelain"), "")
        self.assertEqual(result["repositories"]["makepad"]["revision"], self.override)
        self.assertEqual(result["makepad_override"], self.lock["makepad_override"])
        self.run_setup(check=True)

    def test_missing_update_preserves_existing_pin(self):
        with self.assertRaisesRegex(RuntimeError, "--update"):
            self.run_setup()
        self.assertEqual(git(self.repos["makepad"], "rev-parse", "HEAD"), self.base)

    def test_check_rejects_base_even_when_base_matches_wrapper_manifest(self):
        with self.assertRaisesRegex(RuntimeError, "makepad"):
            self.run_setup(check=True)
        self.assertEqual(git(self.repos["makepad"], "rev-parse", "HEAD"), self.base)

    def test_dirty_source_is_preserved_before_any_checkout_changes(self):
        write(self.repos["octoscript"] / "README", "uncommitted work")
        with self.assertRaisesRegex(RuntimeError, "Preserving local changes"):
            self.run_setup(update=True)
        self.assertEqual(git(self.repos["makepad"], "rev-parse", "HEAD"), self.base)
        self.assertEqual((self.repos["octoscript"] / "README").read_text(), "uncommitted work")

    def test_dirty_wrapper_is_preserved(self):
        write(self.repos["octoscript-makepad"] / "scratch", "keep me")
        with self.assertRaisesRegex(RuntimeError, "Preserving local changes"):
            self.run_setup(update=True)
        self.assertEqual(git(self.repos["makepad"], "rev-parse", "HEAD"), self.base)

    def test_invalid_override_is_rejected_before_checkout_changes(self):
        for changes in ({"revision": "main"}, {"url": "https://example.org/fork.git"},
                        {"reason": ""}, {"revision": 12}, {"unexpected": True}):
            with self.subTest(changes=changes):
                original = dict(self.lock["makepad_override"])
                self.lock["makepad_override"].update(changes)
                self.save_lock()
                with self.assertRaisesRegex(RuntimeError, "override"):
                    self.run_setup(update=True)
                self.assertEqual(git(self.repos["makepad"], "rev-parse", "HEAD"), self.base)
                self.lock["makepad_override"] = original

    def test_consumer_pin_must_match_effective_override(self):
        self.save_cargo(self.base)
        with self.assertRaisesRegex(RuntimeError, "Cargo.toml"):
            self.run_setup(update=True)
        self.assertEqual(git(self.repos["makepad"], "rev-parse", "HEAD"), self.base)

    def test_multiline_consumer_pin_is_checked(self):
        write(self.consumer / "Cargo.toml", '[dependencies]\nmakepad-widgets = {\n git = "%s",\n rev = "%s"\n}\n' %
              (self.sources["makepad"]["url"], self.base))
        with self.assertRaisesRegex(RuntimeError, "Cargo.toml"):
            self.run_setup(update=True)

    def test_named_dependency_table_pin_is_checked(self):
        write(self.consumer / "Cargo.toml", '[dependencies.makepad-widgets]\ngit = "%s"\nrev = "%s"\n' %
              (self.sources["makepad"]["url"], self.base))
        with self.assertRaisesRegex(RuntimeError, "Cargo.toml"):
            self.run_setup(update=True)

    def test_missing_consumer_revision_is_rejected(self):
        write(self.consumer / "Cargo.toml", '[dependencies]\nmakepad-widgets = { git = "%s" }\n' %
              self.sources["makepad"]["url"])
        with self.assertRaisesRegex(RuntimeError, "Cargo.toml"):
            self.run_setup(update=True)

    def test_inconsistent_original_wrapper_manifest_is_rejected(self):
        runtime = self.repos["octoscript-makepad"]
        path = runtime / "Cargo.toml"
        path.write_text(path.read_text().replace(self.base, self.override))
        self.lock["revision"] = commit(runtime)
        self.save_lock()
        with self.assertRaisesRegex(RuntimeError, "disagree"):
            self.run_setup(update=True)
        self.assertEqual(git(self.repos["makepad"], "rev-parse", "HEAD"), self.base)

    def test_check_does_not_write_a_runtime_receipt(self):
        git(self.repos["makepad"], "checkout", "-q", "--detach", self.override)
        self.run_setup(check=True)
        self.assertFalse((self.root / ".octoscript-runtime.json").exists())

    def test_missing_override_preserves_original_release_behavior(self):
        del self.lock["makepad_override"]
        self.save_lock()
        self.save_cargo(self.base)
        result = self.run_setup(check=True)
        self.assertEqual(result["repositories"]["makepad"]["revision"], self.base)
        self.assertNotIn("makepad_override", result)

    def test_non_checkout_directory_is_preserved(self):
        with self.assertRaisesRegex(RuntimeError, "checkout"):
            setup.checkout_status(self.consumer)

    def test_cargo_graph_rejects_duplicate_or_foreign_framework_sources(self):
        good = {"name": "makepad-widgets", "manifest_path": str(self.repos["makepad"] / "widgets/Cargo.toml")}
        self.assertIn("makepad-widgets", setup.validate_cargo_sources(self.root, {"packages": [good]}))
        for packages in ([good, good], [dict(good, manifest_path=str(self.root / "foreign/Cargo.toml"))], []):
            with self.subTest(packages=packages), self.assertRaises(RuntimeError):
                setup.validate_cargo_sources(self.root, {"packages": packages})


if __name__ == "__main__":
    unittest.main()
