"""Tests for tools/kernel-artifact.py (no network, no build)."""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("kernel_artifact", ROOT / "tools/kernel-artifact.py")
kernel = importlib.util.module_from_spec(spec)
spec.loader.exec_module(kernel)

REV = "ae230ce04d57f3c29cf6c2518e5956a86c07d788"


def lock_with(*revs):
    text = "".join(f'[[package]]\nname = "octos-cli"\nversion = "2.0.3"\n'
                   f'source = "git+https://github.com/octos-org/octos.git?rev={rev}#{rev}"\n\n' for rev in revs)
    return text + '[[package]]\nname = "nix"\nversion = "0.29.0"\nsource = "git+https://github.com/octos-org/octos.git?rev=18fcd3f16e527d2b601d7ae244f056fb711bb6b8#18fcd3f1"\n'


class RevisionTests(unittest.TestCase):
    def test_the_revision_is_the_one_cargo_lock_pins(self):
        with tempfile.TemporaryDirectory() as temp:
            lock = Path(temp) / "Cargo.lock"
            lock.write_text(lock_with(REV))
            self.assertEqual(kernel.octos_revision(lock), REV, "the nix patch from the octos repo is not the kernel")
            lock.write_text('[[package]]\nname = "serde"\n')
            with self.assertRaises(RuntimeError):
                kernel.octos_revision(lock)
            lock.write_text(lock_with(REV, "b" * 40))
            with self.assertRaisesRegex(RuntimeError, "one octos per graph"):
                kernel.octos_revision(lock)

    @unittest.skipUnless((ROOT / "Cargo.lock").is_file(), "no workspace Cargo.lock here")
    def test_the_workspace_pins_one_kernel(self):
        self.assertRegex(kernel.octos_revision(), r"^[0-9a-f]{40}$")


class PlanTests(unittest.TestCase):
    def test_the_desktop_plan_builds_the_locked_revision_unpatched(self):
        steps, binary, source = kernel.kernel_plan(host=True, work=Path("/w"))
        self.assertEqual(source, f"{kernel.OCTOS_URL}@{kernel.octos_revision()}")
        self.assertEqual(binary, Path("/w/target/release/octos"))
        build = steps[-1][1]
        self.assertIn("--locked", build)
        self.assertNotIn("--target", build)
        self.assertNotIn("--offline", build)
        # octos ships `serve --host-managed` itself: no overlay step, no lock.
        self.assertFalse(any("--apply-host-patch" in argv for _, argv in steps))
        self.assertFalse((ROOT / "octos-runtime-patches.lock.json").exists())
        self.assertFalse((ROOT / "tools/runtime-patches/octos-host-managed.patch").exists())

    def test_an_offline_desktop_plan_stays_offline(self):
        steps, _, _ = kernel.kernel_plan(host=True, work=Path("/w"), offline=True)
        self.assertFalse(any(argv[:2] == ["git", "fetch"] for _, argv in steps))
        self.assertIn("--offline", steps[-1][1])

    def test_the_plan_checks_out_and_cross_builds_the_kernel(self):
        work = Path("/w")
        steps, binary = kernel.plan("a" * 40, work, Path("/sdk with spaces"), required=False)
        commands = [c for _, c in steps]
        self.assertEqual(commands[0], ["git", "init", "--quiet", "/w/src"])
        self.assertIn(["git", "fetch", "--quiet", "--no-tags", "--depth=1", kernel.OCTOS_URL, "a" * 40], commands)
        self.assertIn(["git", "checkout", "--quiet", "--detach", "a" * 40], commands)
        cargo = commands[-1]
        self.assertEqual(cargo[0], "env")
        self.assertTrue(any(a.startswith("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=/sdk with spaces/ndk/") for a in cargo))
        self.assertIn("CARGO_TARGET_DIR=/w/target", cargo)
        self.assertEqual(cargo[cargo.index("cargo"):],
                         ["cargo", "build", "--locked", "--release", "--target", "aarch64-linux-android", *kernel.KERNEL_BUILD])
        self.assertEqual(binary, work / "target/aarch64-linux-android/release/octos")
        offline, _ = kernel.plan("a" * 40, work, Path("/sdk"), offline=True, required=False)
        self.assertFalse(any(c[:2] == ["git", "fetch"] for _, c in offline), "offline builds fetch nothing")
        self.assertEqual(offline[-1][1][-1], "--offline")

    def test_the_ndk_clang_is_found_in_the_sdk(self):
        with tempfile.TemporaryDirectory() as temp:
            sdk = Path(temp)
            with self.assertRaises(RuntimeError):
                kernel.ndk_bin(sdk)
            self.assertIn("<version>", str(kernel.ndk_bin(sdk, required=False)))
            old = sdk / "ndk/25.1.0/toolchains/llvm/prebuilt/darwin-x86_64/bin"
            bin_dir = sdk / "ndk/28.2.1/toolchains/llvm/prebuilt/darwin-x86_64/bin"
            old.mkdir(parents=True)
            bin_dir.mkdir(parents=True)
            self.assertEqual(kernel.ndk_bin(sdk), bin_dir, "the newest NDK")
            env = kernel.build_env(bin_dir)
            self.assertEqual(env["CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"], str(bin_dir / "aarch64-linux-android33-clang"))

    def test_a_prebuilt_kernel_or_none(self):
        self.assertEqual(kernel.kernel_plan(kernel="/k/octos"), ([], Path("/k/octos"), "prebuilt"))
        self.assertEqual(kernel.kernel_plan(no_kernel=True), ([], None, None))
        self.assertIsNone(kernel.extra_libs(None))
        self.assertEqual(kernel.extra_libs(Path("/k/octos")), "liboctos.so=/k/octos")
        with self.assertRaises(RuntimeError):
            kernel.kernel_plan(lock=Path("/nonexistent"))

    def test_the_default_build_bundles_the_locked_revision(self):
        with tempfile.TemporaryDirectory() as temp:
            lock = Path(temp) / "Cargo.lock"
            lock.write_text(lock_with(REV))
            steps, binary, source = kernel.kernel_plan(lock=lock, work=Path(temp) / "work", sdk=Path("/sdk"), required=False)
            self.assertEqual(source, f"{kernel.OCTOS_URL}@{REV}")
            self.assertIn(["git", "checkout", "--quiet", "--detach", REV], [c for _, c in steps])
            self.assertEqual(binary, Path(temp) / "work/target/aarch64-linux-android/release/octos")

    def test_the_receipt_records_the_kernels_hash(self):
        with tempfile.TemporaryDirectory() as temp:
            binary = Path(temp) / "octos"
            binary.write_bytes(b"\x7fELF")
            self.assertEqual(kernel.receipt(binary, "prebuilt"),
                             {"source": "prebuilt", "sha256": hashlib.sha256(b"\x7fELF").hexdigest()})
            self.assertIsNone(kernel.receipt(None, None))

    def test_the_cli_prints_a_plan_and_runs_nothing(self):
        with tempfile.TemporaryDirectory() as temp:
            lock = Path(temp) / "Cargo.lock"
            lock.write_text(lock_with(REV))
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                kernel.main(["--lock", str(lock), "--sdk", "/sdk", "--work", str(Path(temp) / "w"), "--plan", "--", "cargo", "makepad"])
            printed = json.loads(out.getvalue())
            self.assertEqual(printed["source"], f"{kernel.OCTOS_URL}@{REV}")
            self.assertTrue(printed["android_env"]["MAKEPAD_ANDROID_EXTRA_LIBS"].startswith("liboctos.so="))
            self.assertEqual(printed["then"], ["cargo", "makepad"])
            self.assertFalse((Path(temp) / "w").exists(), "a plan creates nothing")
            with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                kernel.main(["--kernel", "/k/octos", "--no-kernel"])


@unittest.skipIf(os.name == "nt", "the stand-in kernel is a POSIX shell script")
class StageTests(unittest.TestCase):
    """--stage: the desktop's packaged kernel and the receipt
    crates/kernel/src/launch.rs checks before running it."""

    def stand_in(self, directory, version):
        program = Path(directory) / "octos"
        program.write_text(f"#!/bin/sh\nprintf '%s\\n' '{version}'\n")
        program.chmod(0o755)
        return program

    def test_the_version_must_name_the_locked_revision(self):
        self.assertTrue(kernel.version_matches(f"octos 2.0.3-rc.13 ({REV[:7]} 2026-10-01)", REV))
        self.assertTrue(kernel.version_matches(f"octos 2.0.3 ({REV[:9]})", REV))
        self.assertFalse(kernel.version_matches("octos 2.0.3-rc.13 (0000000 2026-10-01)", REV))
        self.assertFalse(kernel.version_matches("octos 2.0.3", REV))
        self.assertFalse(kernel.version_matches(f"something else ({REV[:7]})", REV))

    def test_a_staged_kernel_carries_its_revision_and_hash(self):
        with tempfile.TemporaryDirectory() as temp:
            source = self.stand_in(temp, f"octos 2.0.3-rc.13 ({REV[:7]} 2026-10-01)")
            out = Path(temp) / "target dir/release"
            record = kernel.stage(source, out, REV)
            staged = out / kernel.STAGED_NAME
            self.assertTrue(os.access(staged, os.X_OK))
            on_disk = json.loads((out / kernel.RECEIPT_NAME).read_text())
            self.assertEqual(on_disk, record)
            self.assertEqual(on_disk["revision"], REV)
            self.assertEqual(on_disk["source"], f"{kernel.OCTOS_URL}@{REV}")
            self.assertEqual(on_disk["sha256"], hashlib.sha256(staged.read_bytes()).hexdigest())
            self.assertEqual(sorted(p.name for p in out.iterdir()), sorted([kernel.STAGED_NAME, kernel.RECEIPT_NAME]))

    def test_a_kernel_of_another_revision_replaces_nothing(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / "release"
            out.mkdir()
            (out / kernel.STAGED_NAME).write_bytes(b"previous")
            (out / kernel.RECEIPT_NAME).write_text("previous receipt")
            wrong = self.stand_in(temp, "octos 2.0.3-rc.13 (0000000 2026-09-01)")
            with self.assertRaisesRegex(RuntimeError, "not the locked octos revision"):
                kernel.stage(wrong, out, REV)
            self.assertEqual((out / kernel.STAGED_NAME).read_bytes(), b"previous")
            self.assertEqual((out / kernel.RECEIPT_NAME).read_text(), "previous receipt")
            self.assertEqual(len(list(out.iterdir())), 2, "no temporary file is left behind")
            garbage = Path(temp) / "garbage"
            garbage.write_bytes(b"\x00\x01 not a program")
            with self.assertRaisesRegex(RuntimeError, "does not run here"):
                kernel.stage(garbage, out, REV)

    def test_the_cli_stages_only_a_desktop_kernel(self):
        with tempfile.TemporaryDirectory() as temp:
            lock = Path(temp) / "Cargo.lock"
            lock.write_text(lock_with(REV))
            source = self.stand_in(temp, f"octos 2.0.3 ({REV[:7]} 2026-10-01)")
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                kernel.main(["--lock", str(lock), "--kernel", str(source), "--stage", str(Path(temp) / "rel")])
            self.assertIn("staged:", out.getvalue())
            self.assertEqual(json.loads((Path(temp) / "rel" / kernel.RECEIPT_NAME).read_text())["source"], "prebuilt")
            plan = io.StringIO()
            with contextlib.redirect_stdout(plan):
                kernel.main(["--lock", str(lock), "--host", "--work", str(Path(temp) / "w"), "--stage", str(Path(temp) / "x"), "--plan"])
            printed = json.loads(plan.getvalue())
            self.assertEqual(Path(printed["stage"]).name, kernel.STAGED_NAME)
            self.assertIn("--target-dir", printed["steps"][-1]["argv"], "no `env` wrapper: Windows runs it too")
            self.assertFalse((Path(temp) / "x").exists(), "a plan stages nothing")
            for argv in (["--sdk", "/sdk", "--stage", "x"], ["--no-kernel", "--stage", "x"]):
                with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                    kernel.main(["--lock", str(lock), *argv])

    def test_the_names_are_the_ones_the_desktop_looks_for(self):
        launch = (ROOT / "crates/kernel/src/launch.rs").read_text()
        self.assertIn('"octos-kernel"', launch)
        self.assertIn(f'"{kernel.RECEIPT_NAME}"', launch)


if __name__ == "__main__":
    unittest.main()
