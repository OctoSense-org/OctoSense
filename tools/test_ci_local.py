"""Tests for tools/ci_local.py and tools/ci_local_merge.py (no cargo, no network)."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
from types import SimpleNamespace
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

    def test_native_host_acceptance_is_not_claimed_on_linux(self):
        job = ci.job_definition("desktop.yml:native-host-api")
        runner = SimpleNamespace(which=lambda tool: tool)
        with patch.object(ci.sys, "platform", "linux"):
            reason = ci.Run.job_problem(runner, "desktop.yml", "native-host-api", job)
        self.assertIn("requires a graphical macOS session", reason)
        with patch.object(ci.sys, "platform", "darwin"):
            self.assertIsNone(ci.Run.job_problem(runner, "desktop.yml", "native-host-api", job))

    def test_the_release_workflow_is_left_out_on_purpose(self):
        self.assertIn("release-desktop.yml", ci.NOT_LOCAL)
        self.assertNotIn("release-desktop.yml", ci.GROUPS["all"])
        self.assertTrue(ci.NOT_LOCAL["release-desktop.yml"].strip())
        # It is left out, not invisible: checked on its own it is drift.
        self.assertTrue(ci.check_drift(["release-desktop.yml"]))
        with tempfile.TemporaryDirectory() as temp:
            workflows = Path(temp)
            for path in (ROOT / ".github/workflows").glob("*.yml"):
                if path.name != "release-desktop.yml":
                    shutil.copy(path, workflows / path.name)
            with patch.object(ci, "WORKFLOWS", workflows):
                problems = "\n".join(ci.check_drift())
        self.assertIn("release-desktop.yml: listed in NOT_LOCAL but no such workflow", problems)

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


class MainRuns(unittest.TestCase):
    def test_main_runs_share_one_cancelling_group_and_merges_do_not_skip_ci(self):
        for name in ("desktop.yml", "phone.yml", "apps.yml"):
            workflow = name[:-len(".yml")]
            concurrency = ci.load_workflow(name)["concurrency"]
            self.assertIn(f"'{workflow}-main'", concurrency["group"], name)
            self.assertIn("github.ref == 'refs/heads/main'", str(concurrency["cancel-in-progress"]), name)
        self.assertNotIn("skip ci", (ROOT / "tools/ci_local_merge.py").read_text())


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


class LinuxHostPlan(unittest.TestCase):
    def where(self, workflows, linux_host):
        return {(w, j): where for w, j, _, where in ci.plan_jobs(workflows, linux_host)}

    def test_without_a_host_everything_runs_here_and_nothing_is_added(self):
        plan = self.where(ci.GROUPS["all"], False)
        self.assertEqual(set(plan.values()), {"local"})
        self.assertNotIn(ci.LINUX_HOST, {w for w, _ in plan})

    def test_ubuntu_jobs_go_to_the_host_and_macos_jobs_stay(self):
        plan = self.where(["apps.yml"], True)
        self.assertEqual(plan[("apps.yml", "services")], "linux")
        self.assertEqual(plan[("apps.yml", "kernel-security")], "linux")
        self.assertEqual(plan[("apps.yml", "apps")], "local")
        self.assertEqual(plan[(ci.LINUX_HOST, "sandbox")], "linux", "the sandbox tests come with apps")
        for workflow in ci.GROUPS["all"]:
            for job_id, job in ci.jobs_of(workflow):
                expected = "linux" if "ubuntu" in job["runs-on"] else "local"
                self.assertEqual(self.where([workflow], True)[(workflow, job_id)], expected, (workflow, job_id))

    def test_the_sandbox_tests_follow_the_workflows_that_cover_the_shell(self):
        self.assertIn((ci.LINUX_HOST, "sandbox"), self.where(["phone.yml"], True))
        self.assertNotIn((ci.LINUX_HOST, "sandbox"), self.where(["rom.yml"], True),
                         "rom.yml does not run for a change to the shell")
        job = ci.job_definition(f"{ci.LINUX_HOST}:sandbox")
        self.assertIn("octosense-shell", job["steps"][-1]["run"])
        self.assertEqual(ci.job_definition("apps.yml:services")["runs-on"], "ubuntu-latest")

    def test_offload_splits_the_macos_jobs_between_here_and_the_host(self):
        plan = [(w, j, [ci.step_label(s, i) for i, s in enumerate(job["steps"]) if "run" in s], where)
                for w, j, job, where in ci.plan_jobs(ci.GROUPS["all"], True, offload=True)]
        where = {(w, j): place for w, j, _, place in plan}
        self.assertEqual(where[("apps.yml", "apps@linux")], "linux")
        self.assertNotIn(("apps.yml", "apps"), where, "apps has no macOS steps: it all moves")
        self.assertEqual(where[("desktop.yml", "native-host-api")], "local", "native acceptance stays on macOS")
        self.assertEqual((where[("phone.yml", "home")], where[("phone.yml", "home@linux")]), ("local", "linux"))
        steps = {(w, j): labels for w, j, labels, _ in plan}
        here, there = steps[("phone.yml", "home")], steps[("phone.yml", "home@linux")]
        mac = ci.JOBS["phone.yml:home"]["mac_steps"]
        self.assertEqual([s for s in here if s in mac], mac)
        self.assertFalse(set(mac) & set(there), "a macOS step never runs on the host")
        self.assertIn("Test Home, App Hub admission and runtime policy", there)
        setup = "Prepare pinned sources and the reviewed runtime patch"
        self.assertTrue(setup in here and setup in there, "both parts prepare the sources")
        whole = [ci.step_label(s, i) for i, s in enumerate(ci.job_definition("phone.yml:home")["steps"]) if "run" in s]
        self.assertEqual(sorted(set(here) | set(there)), sorted(whole), "every step runs on one side")
        home = dict(ci.jobs_of("phone.yml"))["home"]
        self.assertEqual(ci.job_definition("phone.yml:home@linux"), ci.split_offloaded("phone.yml", "home", home)[1],
                         "the host rebuilds the same Linux part from the key")
        with self.assertRaises(KeyError):
            ci.job_definition("rom.yml:product@linux")
        self.assertEqual(self.where(ci.GROUPS["all"], True), {(w, j): p for w, j, _, p in
                                                              ci.plan_jobs(ci.GROUPS["all"], True)},
                         "without --offload the plan is unchanged")

    def test_a_mac_step_that_no_longer_exists_is_drift(self):
        jobs = dict(ci.JOBS)
        jobs["phone.yml:home"] = {"mac_steps": ["A step that was renamed"]}
        with patch.object(ci, "JOBS", jobs):
            self.assertTrue(any("A step that was renamed" in p for p in ci.check_drift()))

    def test_a_linux_host_job_whose_source_moved_is_drift(self):
        moved = {"sandbox": dict(ci.LINUX_HOST_JOBS["sandbox"], source="crates/shell/src/gone.rs")}
        with patch.object(ci, "LINUX_HOST_JOBS", moved):
            self.assertTrue(any("gone.rs" in p for p in ci.check_drift()))

    def test_the_host_comes_from_the_flag_the_environment_or_build_env(self):
        with tempfile.TemporaryDirectory() as temp:
            env_file = Path(temp) / "build.env"
            env_file.write_text("OCTOSENSE_BUILD_HOST=someone@build.invalid\nexport OCTOSENSE_BUILD_KEY=\"$HOME/k/id\"\n")
            clean = {k: v for k, v in os.environ.items() if not k.startswith("OCTOSENSE_BUILD_")}
            with patch.object(ci, "BUILD_ENV", env_file), patch.dict(os.environ, clean, clear=True):
                target, key = ci.linux_host_settings("")
                self.assertEqual(target, "someone@build.invalid")
                self.assertEqual(key, os.path.expanduser("~/k/id"))
                self.assertEqual(ci.linux_host_settings("other@x.invalid")[0], "other@x.invalid")
            with patch.object(ci, "BUILD_ENV", Path(temp) / "none"), patch.dict(os.environ, clean, clear=True):
                self.assertEqual(ci.linux_host_settings(""), (None, None))


def remote_run(temp, target="someone@build.invalid"):
    args = SimpleNamespace(jobs=1, verbose=False, keep_going=False, linux_settings=(target, "/keys/id_build"))
    run = ci.Run(args, out_dir=Path(temp))
    return run


def host_result(sha, steps_by_key):
    return {"sha": sha, "run": f"{sha}-x", "slot": 1, "slots": 4, "system": "Linux", "machine": "x86_64",
            "cpus": 72, "versions": {}, "seconds": 10, "prepare_seconds": 1,
            "jobs": {key: {"steps": steps, "actions": [], "notes": []} for key, steps in steps_by_key.items()}}


class LinuxHostResults(unittest.TestCase):
    sha = "d" * 40

    def step(self, job="services", status="PASS", workflow="apps.yml"):
        return {"workflow": workflow, "job": job, "name": "Kernel service", "status": status, "seconds": 5,
                "reason": "", "expected_skip": None, "command": "cargo test", "host": "local"}

    def test_remote_steps_join_the_run_marked_linux_and_bound_to_the_commit(self):
        with tempfile.TemporaryDirectory() as temp:
            run = remote_run(temp)
            self.addCleanup(run.close)
            keys = ["apps.yml:services", "linux-host:sandbox"]
            ci.merge_remote_result(run, host_result(self.sha, {
                keys[0]: [self.step()], keys[1]: [self.step("sandbox", "FAIL", ci.LINUX_HOST)]}), self.sha, keys)
            self.assertEqual([(s["job"], s["host"], s["sha"], s["status"]) for s in run.steps],
                             [("services", "linux", self.sha, "PASS"), ("sandbox", "linux", self.sha, "FAIL")])
            self.assertEqual(run.linux_info["machine"], "x86_64")
            table = ci.format_table({"steps": run.steps, "notes": []})
            self.assertIn("apps.yml / services (linux)", table)

    def test_a_result_for_another_commit_is_refused(self):
        with tempfile.TemporaryDirectory() as temp:
            run = remote_run(temp)
            self.addCleanup(run.close)
            ci.merge_remote_result(run, host_result("e" * 40, {"apps.yml:services": [self.step()]}),
                                   self.sha, ["apps.yml:services"])
            self.assertEqual([s["status"] for s in run.steps], ["FAIL"])
            self.assertIn("stale", run.steps[0]["reason"])

    def test_a_missing_job_or_a_busy_host_never_passes(self):
        with tempfile.TemporaryDirectory() as temp:
            run = remote_run(temp)
            self.addCleanup(run.close)
            ci.merge_remote_result(run, host_result(self.sha, {}), self.sha, ["apps.yml:services"])
            ci.merge_remote_result(run, {"sha": self.sha, "busy": True}, self.sha, ["apps.yml:kernel-security"])
            ci.merge_remote_result(run, {"sha": self.sha, "error": "setup failed"}, self.sha, ["linux-host:sandbox"])
            self.assertEqual([(s["status"], s["expected_skip"]) for s in run.steps],
                             [("FAIL", None), ("SKIPPED", False), ("FAIL", None)])

    def test_an_offloaded_part_reports_under_its_job(self):
        with tempfile.TemporaryDirectory() as temp:
            run = remote_run(temp)
            self.addCleanup(run.close)
            ci.merge_remote_result(run, {"sha": self.sha, "busy": True}, self.sha, ["phone.yml:home@linux"])
            ci.merge_remote_result(run, {"sha": self.sha, "error": "x"}, self.sha, ["apps.yml:apps@linux"])
            self.assertEqual([(s["job"], s["host"]) for s in run.steps], [("home", "linux"), ("apps", "linux")])

    def test_the_host_address_and_key_never_reach_the_log(self):
        with tempfile.TemporaryDirectory() as temp:
            run = remote_run(temp)
            self.addCleanup(run.close)
            run.log("ssh: connect to host build.invalid port 22 as someone@build.invalid with /keys/id_build", echo=False)
            run.remote_fail(["apps.yml:services"], "ssh: Could not resolve hostname build.invalid")
            run.log_file.flush()
            text = run.log_path.read_text() + json.dumps(run.steps)
            self.assertNotIn("build.invalid", text)
            self.assertNotIn("id_build", text)

    def test_the_remote_environment_is_an_allow_list(self):
        self.assertTrue(ci.REMOTE_ENV.startswith('HOME="$HOME"'))
        for name in ("TOKEN", "KEY", "SECRET", "GH_", "GITHUB", "SSH_AUTH"):
            self.assertNotIn(name, ci.REMOTE_ENV)


class MergeLinuxHost(unittest.TestCase):
    head = "b" * 40

    def remote(self, sha, workflow="apps.yml", job="services", status="PASS"):
        return {"workflow": workflow, "job": job, "name": "Kernel service", "status": status, "seconds": 3,
                "reason": "", "expected_skip": None, "host": "linux", "sha": sha}

    def test_a_remote_pass_on_the_head_is_evidence(self):
        last = result(steps=[self.remote(self.head), self.remote(self.head, ci.LINUX_HOST, "sandbox")],
                      linux_host={"system": "Linux", "machine": "x86_64"})
        problems, _ = merge.evidence_problems(last, self.head, ["crates/kernel/src/lib.rs"])
        self.assertEqual(problems, [])
        self.assertIn("--linux-host", merge.comment_body(last, ["apps.yml"]))
        self.assertIn("| apps.yml / services (linux) | Kernel service | PASS |", merge.comment_body(last, ["apps.yml"]))

    def test_an_offloaded_run_is_evidence_and_says_so(self):
        steps = [dict(self.remote(self.head, "phone.yml", "home"), host="local", sha=None, name="Compile Home"),
                 self.remote(self.head, "phone.yml", "home")]
        last = result(steps=steps, linux_host={"system": "Linux", "machine": "x86_64"}, offload=True)
        problems, _ = merge.evidence_problems(last, self.head, ["crates/shell/src/lib.rs"])
        self.assertEqual(problems, [])
        body = merge.comment_body(last, ["phone.yml"])
        self.assertIn("--linux-host --offload", body)
        self.assertIn("the macOS jobs' portable steps", body)
        self.assertIn("| phone.yml / home (linux) | Kernel service | PASS |", body)

    def test_a_stale_remote_result_is_refused_even_when_the_run_is_on_the_head(self):
        problems, _ = merge.evidence_problems(result(steps=[self.remote("c" * 40)]), self.head, ["crates/kernel/src/lib.rs"])
        self.assertTrue(any(p.startswith("stale: 1 step(s) on the Linux host ran on cccccccccccc") for p in problems), problems)
        problems, _ = merge.evidence_problems(result(steps=[self.remote(None)]), self.head, ["crates/kernel/src/lib.rs"])
        self.assertTrue(any("stale" in p for p in problems), "a remote step bound to no commit is stale")

    def test_the_linux_only_checks_block_the_prs_that_trigger_what_they_cover(self):
        sandbox = self.remote(self.head, ci.LINUX_HOST, "sandbox", "FAIL")
        problems, _ = merge.evidence_problems(result(steps=[sandbox], passed=False), self.head, ["crates/shell/src/lib.rs"])
        self.assertTrue(any("failed: linux-host / sandbox" in p for p in problems), problems)
        problems, _ = merge.evidence_problems(result(steps=[sandbox], passed=False), self.head, ["rom/README.md"])
        self.assertEqual(problems, [], "a rom-only PR does not run the shell's checks")


if __name__ == "__main__":
    unittest.main()
