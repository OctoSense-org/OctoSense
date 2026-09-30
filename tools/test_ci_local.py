"""Tests for tools/ci_local.py and tools/ci_local_merge.py (no cargo, no network)."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


ci = load("ci_local", "tools/ci_local.py")
merge = load("ci_local_merge", "tools/ci_local_merge.py")


class WorkflowReader(unittest.TestCase):
    def test_reads_what_workflows_use(self):
        text = (
            "name: X\n"
            "on:\n"
            "  push:\n"
            "    branches: [main, 'rel']\n"
            "jobs:\n"
            "  a:\n"
            "    runs-on: macos-14  # a comment\n"
            "    steps:\n"
            "      - uses: actions/checkout@v6\n"
            "      - name: 'Quoted: name'\n"
            "        working-directory: phone\n"
            "        timeout-minutes: 30\n"
            "        env:\n"
            "          K: ${{ runner.temp }}/x\n"
            "        run: |\n"
            "          # a shell comment stays\n"
            "          echo \"a # b\"\n"
            "\n"
            "          echo two\n"
            "      - run: echo one-line # yaml comment\n"
        )
        data = ci.load_yaml(text)
        self.assertEqual(data["on"]["push"]["branches"], ["main", "rel"])
        steps = data["jobs"]["a"]["steps"]
        self.assertEqual(steps[0], {"uses": "actions/checkout@v6"})
        self.assertEqual(steps[1]["name"], "Quoted: name")
        self.assertEqual(steps[1]["timeout-minutes"], 30)
        self.assertEqual(steps[1]["env"], {"K": "${{ runner.temp }}/x"})
        self.assertEqual(steps[1]["run"], "# a shell comment stays\necho \"a # b\"\n\necho two\n")
        self.assertEqual(steps[2]["run"], "echo one-line")

    def test_agrees_with_pyyaml_on_every_workflow(self):
        try:
            import yaml
        except ImportError:
            self.skipTest("PyYAML is not installed")
        for path in sorted((ROOT / ".github/workflows").glob("*.yml")):
            reference = yaml.safe_load(path.read_text())
            if True in reference:  # YAML 1.1 reads the key `on` as true
                reference["on"] = reference.pop(True)
            self.assertEqual(ci.load_yaml(path.read_text()), reference, path.name)


class Drift(unittest.TestCase):
    def test_the_mapping_fits_the_workflows(self):
        self.assertEqual(ci.check_drift(), [])

    def test_a_changed_workflow_is_drift(self):
        with tempfile.TemporaryDirectory() as temp:
            workflows = Path(temp)
            for path in (ROOT / ".github/workflows").glob("*.yml"):
                shutil.copy(path, workflows / path.name)
            phone = workflows / "phone.yml"
            phone.write_text(phone.read_text().replace("uses: actions/checkout@v6", "uses: someone/new-action@v1", 1))
            rom = workflows / "rom.yml"
            rom.write_text(rom.read_text().replace("Check generated Agent Binder client", "Renamed step"))
            (workflows / "extra.yml").write_text("on: push\njobs:\n  x:\n    runs-on: windows-latest\n    steps:\n      - run: echo ${{ secrets.X }}\n")
            with patch.object(ci, "WORKFLOWS", workflows):
                problems = "\n".join(ci.check_drift())
        self.assertIn("someone/new-action", problems)
        self.assertIn("rom.yml:product:Check generated Agent Binder client", problems)
        self.assertIn("extra.yml: a workflow ci-local does not run", problems)
        self.assertIn("extra.yml:x: a job ci-local does not know", problems)
        self.assertIn("secrets.X", problems)


class PathFilters(unittest.TestCase):
    def test_globs(self):
        self.assertTrue(ci.glob_match("crates/shell/src/lib.rs", "crates/**"))
        self.assertTrue(ci.glob_match("Cargo.toml", "Cargo.toml"))
        self.assertFalse(ci.glob_match("desktop/Cargo.toml", "Cargo.toml"))
        self.assertFalse(ci.glob_match("a/b", "a/*/c"))

    def test_triggered_workflows_follow_pull_request_paths(self):
        self.assertEqual(ci.triggered_workflows(["docs/architecture.md"]), [])
        self.assertEqual(ci.triggered_workflows(["crates/shell/src/lib.rs"]), ["desktop.yml", "phone.yml", "apps.yml"])
        self.assertEqual(ci.triggered_workflows(["rom/README.md"]), ["rom.yml"])
        self.assertIn("rom.yml", ci.triggered_workflows(["tools/kernel-artifact.py"]))


class MachineSharing(unittest.TestCase):
    def test_slots_are_exclusive_and_stale_owners_are_taken_over(self):
        with tempfile.TemporaryDirectory() as temp:
            first, second = ci.DirLock(Path(temp) / "slot-0"), ci.DirLock(Path(temp) / "slot-0")
            self.assertTrue(first.try_acquire())
            self.assertFalse(second.try_acquire())
            first.release()
            self.assertTrue(second.try_acquire())
            owner = json.loads((Path(temp) / "slot-0/owner.json").read_text())
            owner["pid"] = 2 ** 22 + 7  # no such process
            (Path(temp) / "slot-0/owner.json").write_text(json.dumps(owner))
            third = ci.DirLock(Path(temp) / "slot-0")
            self.assertTrue(third.try_acquire(), "a dead owner's slot is taken over")
            third.release()

    def test_acquire_slot_without_waiting(self):
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, {"OCTOSENSE_CI_LOCAL_LOCKS": temp}):
            held = [ci.acquire_slot(2, False, print, {}) for _ in range(2)]
            self.assertTrue(all(held))
            self.assertIsNone(ci.acquire_slot(2, False, print, {}))
            for lock in held:
                lock.release()

    def test_the_kernel_cache_is_keyed_by_revision_and_written_atomically(self):
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, {"OCTOSENSE_CI_LOCAL_CACHE": temp}):
            rev = "a" * 40
            cached = ci.kernel_cache_file(rev)
            self.assertIn(rev, str(cached))
            built = Path(temp) / "built"
            built.write_bytes(b"octos")
            ci.publish_to_cache(built, cached)
            self.assertEqual(cached.read_bytes(), b"octos")
            self.assertTrue(os.access(cached, os.X_OK))
            self.assertEqual([p.name for p in cached.parent.iterdir()], ["octos"], "no temp file left behind")
            runner_temp = Path(temp) / "runner"
            ci.link_kernel(runner_temp, cached)
            self.assertEqual((runner_temp / ci.KERNEL_BINARY).resolve(), cached.resolve())


def result(**overrides):
    base = {"sha": "b" * 40, "dirty": False, "passed": True, "only": "all", "workflows": ci.GROUPS["all"],
            "seconds": 90, "host": {"system": "Darwin", "machine": "arm64"}, "notes": [],
            "steps": [{"workflow": "phone.yml", "job": "home", "name": "Test", "status": "PASS",
                       "seconds": 3, "reason": "", "expected_skip": None}]}
    base.update(overrides)
    return base


class MergeEvidence(unittest.TestCase):
    def test_a_pass_on_the_head_is_evidence(self):
        problems, required = merge.evidence_problems(result(), "b" * 40, ["crates/shell/src/lib.rs"])
        self.assertEqual(problems, [])
        self.assertIn("phone.yml", required)

    def test_refuses_stale_dirty_and_uncovered_runs(self):
        problems, _ = merge.evidence_problems(result(dirty=True, workflows=["desktop.yml"]), "c" * 40, ["phone/src/main.rs"])
        text = "\n".join(problems)
        self.assertIn("stale", text)
        self.assertIn("uncommitted", text)
        self.assertIn("did not cover phone.yml", text)

    def test_refuses_failures_and_unexpected_skips_that_matter(self):
        skip = {"workflow": "rom.yml", "job": "product", "name": "AIDL", "status": "SKIPPED",
                "seconds": 0, "reason": "no SDK", "expected_skip": False}
        fail = {"workflow": "apps.yml", "job": "apps", "name": "Clippy", "status": "FAIL",
                "seconds": 1, "reason": "exit 1", "expected_skip": None}
        problems, _ = merge.evidence_problems(result(steps=[skip]), "b" * 40, ["crates/kernel/src/lib.rs"])
        self.assertEqual(problems, [], "a skip in a workflow this PR does not trigger does not block")
        problems, _ = merge.evidence_problems(result(steps=[skip]), "b" * 40, ["rom/tests/test_x.py"])
        self.assertTrue(any("skipped" in p for p in problems))
        problems, _ = merge.evidence_problems(result(steps=[fail], passed=False), "b" * 40, ["docs/x.md"])
        self.assertEqual(problems, [], "a failure in a workflow the PR does not trigger does not block")
        problems, _ = merge.evidence_problems(result(steps=[fail], passed=False), "b" * 40, ["apps/mail/x.rs"])
        self.assertTrue(any("failed: apps.yml" in p for p in problems))
        drift = dict(fail, workflow="ci-local", job="drift", name="Drift")
        problems, _ = merge.evidence_problems(result(steps=[drift], passed=False), "b" * 40, ["docs/x.md"])
        self.assertTrue(problems, "drift always blocks")

    def test_the_comment_names_the_commit(self):
        body = merge.comment_body(result(), ["phone.yml"])
        self.assertTrue(body.startswith("Local CI passed on " + "b" * 40))
        self.assertIn("| phone.yml / home | Test | PASS |", body)


if __name__ == "__main__":
    unittest.main()
