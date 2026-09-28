import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


build = load("build_home", "scripts/build-home.py")
stage = load("stage_home", "scripts/stage-home.py")


class BuildTests(unittest.TestCase):
    def args(self, *extra):
        return build.arguments(["--sdk", "/sdk with spaces", "--android-sdk", "/android", *extra])

    def test_standalone_requires_an_explicit_signer_choice(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            self.args("--variant", "standalone")

    def test_rom_rejects_development_signing(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            self.args("--variant", "rom", "--development")

    def test_rom_requires_both_signing_inputs(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            self.args("--variant", "rom", "--sign-key", "/keys/platform.pk8")

    def test_outputs_are_separate_and_builds_do_not_deploy(self):
        ordinary = self.args("--variant", "standalone", "--development", "--offline")
        rom = self.args("--variant", "rom", "--sign-key", "/keys/platform.pk8", "--sign-cert", "/keys/platform.x509.pem")
        self.assertNotEqual(ordinary.output, rom.output)
        steps = build.build_plan(ordinary)
        commands = [command for _, command in steps]
        self.assertEqual(steps[-1][0], ROOT.parent / "phone")
        self.assertIn("--sdk-path=/sdk with spaces", commands[-1])
        self.assertIn("--no-sign", commands[-1])
        self.assertTrue(all("--offline" in c for c in commands[1:] if c[0] != "git"))
        self.assertFalse(any(c[:2] == ["git", "fetch"] for c in commands), "offline builds fetch nothing")
        self.assertFalse(any("adb" in c or "fastboot" in c for c in commands))
        self.assertFalse(any("OctoSense-mobile" in arg for c in commands for arg in c))

    def test_existing_packager_skips_tool_compilation(self):
        args = self.args("--variant", "standalone", "--development", "--packager", "/tools/cargo-makepad", "--no-octos-kernel")
        plan = build.build_plan(args)
        self.assertEqual(len(plan), 3)
        self.assertEqual(plan[-1][1][0], "/tools/cargo-makepad")

    def test_the_default_phone_build_bundles_the_pinned_octos_kernel(self):
        args = self.args("--variant", "standalone", "--development")
        steps, kernel = build.kernel_plan(args)
        revision = build.octos_revision()
        commands = [command for _, command in steps]
        self.assertIn(["git", "fetch", "--quiet", "--no-tags", "--depth=1", build.OCTOS_URL, revision], commands)
        self.assertIn(["git", "checkout", "--quiet", "--detach", revision], commands)
        cargo = commands[-1]
        self.assertEqual(cargo[0], "env")
        self.assertTrue(any(arg.startswith("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=/sdk with spaces/ndk/") for arg in cargo))
        self.assertEqual(cargo[cargo.index("cargo"):], ["cargo", "build", "--locked", "--release", "--target",
                                                        "aarch64-linux-android", *build.OCTOS_KERNEL_BUILD])
        self.assertEqual(kernel, build.REPO / ".sources/octos-kernel/target/aarch64-linux-android/release/octos")
        self.assertIn(f"CARGO_TARGET_DIR={build.REPO / '.sources/octos-kernel/target'}", cargo)
        self.assertEqual(build.extra_libs(kernel), f"liboctos.so={kernel}")
        # The kernel is built before the APK that bundles it.
        plan = [command for _, command in build.build_plan(args)]
        self.assertLess(plan.index(cargo), len(plan) - 1)

    def test_a_prebuilt_kernel_or_none(self):
        steps, kernel = build.kernel_plan(self.args("--variant", "standalone", "--development", "--octos-kernel", "/k/octos"))
        self.assertEqual((steps, kernel), ([], Path("/k/octos")))
        steps, kernel = build.kernel_plan(self.args("--variant", "standalone", "--development", "--no-octos-kernel"))
        self.assertEqual((steps, kernel), ([], None))
        self.assertIsNone(build.extra_libs(None))
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            self.args("--variant", "standalone", "--development", "--octos-kernel", "/k/octos", "--no-octos-kernel")

    def test_the_octos_revision_is_the_one_cargo_lock_pins(self):
        with tempfile.TemporaryDirectory() as temp:
            lock = Path(temp) / "Cargo.lock"
            rev = "6ad76e5c1e659bdf10ec05ae869428b48edccf7f"
            lock.write_text(f'[[package]]\nname = "octos-cli"\nversion = "2.0.3"\nsource = "git+https://github.com/octos-org/octos.git?rev={rev}#{rev}"\n')
            self.assertEqual(build.octos_revision(lock), rev)
            lock.write_text("[[package]]\nname = \"serde\"\n")
            with self.assertRaises(RuntimeError):
                build.octos_revision(lock)


    def test_rustflags_remap_the_checkout_and_cargo_home(self):
        flags = build.rustflags({"RUSTFLAGS": "-C debuginfo=0", "CARGO_HOME": "/opt/cargo"})
        self.assertTrue(flags.startswith("-C debuginfo=0 "))
        self.assertIn("--remap-path-prefix=/opt/cargo=/cargo", flags)
        # rustc applies the last matching remap: the checkout wins over home.
        self.assertTrue(flags.endswith(f"--remap-path-prefix={build.REPO}=/octosense"))

    def test_personal_paths_scan_only_native_libraries(self):
        import zipfile
        with tempfile.TemporaryDirectory() as temp:
            apk = Path(temp) / "home.apk"
            with zipfile.ZipFile(apk, "w") as archive:
                archive.writestr("lib/arm64-v8a/libclean.so", b"/cargo/registry/src/x.rs\0/octosense/phone")
                archive.writestr("lib/arm64-v8a/libleak.so", b"\0/Users/Shared/build/cargo/git/x.rs\0")
                archive.writestr("assets/readme.txt", b"/Users/someone")
            self.assertEqual(build.personal_paths(apk, home="/nonexistent-home"),
                             {"lib/arm64-v8a/libleak.so": ["/Users/"]})

class StagingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.receipt = {"schema_version": 1, "variant": "rom", "development": False, "artifacts": {}}
        for name in ("OctoSenseHome.apk", "OctoSenseBridge.apk"):
            data = name.encode()
            (self.directory / name).write_bytes(data)
            self.receipt["artifacts"][name] = {"sha256": hashlib.sha256(data).hexdigest(), "certificate_sha256": "abc"}

    def write_receipt(self):
        (self.directory / "build.json").write_text(json.dumps(self.receipt))

    def test_accepts_the_recorded_pair(self):
        self.write_receipt()
        self.assertEqual(stage.verify(self.directory), self.receipt)

    def test_rejects_standalone_on_the_rom_channel(self):
        self.receipt["variant"] = "standalone"
        self.write_receipt()
        with self.assertRaises(ValueError):
            stage.verify(self.directory)

    def test_rejects_an_apk_replaced_after_signing(self):
        self.write_receipt()
        (self.directory / "OctoSenseHome.apk").write_bytes(b"another APK")
        with self.assertRaises(ValueError):
            stage.verify(self.directory)

    def test_rejects_different_home_bridge_signers(self):
        self.receipt["artifacts"]["OctoSenseBridge.apk"]["certificate_sha256"] = "def"
        self.write_receipt()
        with self.assertRaises(ValueError):
            stage.verify(self.directory)


if __name__ == "__main__":
    unittest.main()
