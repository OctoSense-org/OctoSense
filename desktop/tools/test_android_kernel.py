"""Tests for tools/android-kernel.py (no network, no build)."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("android_kernel", ROOT / "tools/android-kernel.py")
kernel = importlib.util.module_from_spec(spec)
spec.loader.exec_module(kernel)


class AndroidKernelTests(unittest.TestCase):
    def test_the_revision_is_the_one_cargo_lock_pins(self):
        rev = kernel.octos_revision()
        self.assertRegex(rev, r"^[0-9a-f]{40}$")
        with tempfile.TemporaryDirectory() as temp:
            lock = Path(temp) / "Cargo.lock"
            lock.write_text('[[package]]\nname = "serde"\n')
            with self.assertRaises(RuntimeError):
                kernel.octos_revision(lock)

    def test_the_plan_checks_out_and_cross_builds_the_kernel(self):
        work = Path("/w")
        steps, binary = kernel.plan("a" * 40, work)
        commands = [c for _, c in steps]
        self.assertIn(["git", "fetch", "--quiet", "--no-tags", "--depth=1", kernel.OCTOS_URL, "a" * 40], commands)
        self.assertEqual(commands[-1], ["cargo", "build", "--locked", "--release", "--target", "aarch64-linux-android", *kernel.KERNEL_BUILD])
        self.assertEqual(binary, work / "target/aarch64-linux-android/release/octos")
        offline, _ = kernel.plan("a" * 40, work, offline=True)
        self.assertFalse(any(c[:2] == ["git", "fetch"] for _, c in offline))

    def test_the_ndk_clang_is_found_in_the_sdk(self):
        with tempfile.TemporaryDirectory() as temp:
            sdk = Path(temp)
            with self.assertRaises(RuntimeError):
                kernel.ndk_bin(sdk)
            bin_dir = sdk / "ndk/28.2.1/toolchains/llvm/prebuilt/darwin-x86_64/bin"
            bin_dir.mkdir(parents=True)
            self.assertEqual(kernel.ndk_bin(sdk), bin_dir)
            env = kernel.build_env(bin_dir)
            self.assertEqual(env["CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"], str(bin_dir / "aarch64-linux-android33-clang"))


if __name__ == "__main__":
    unittest.main()
