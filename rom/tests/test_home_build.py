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
        self.assertEqual(steps[-1][0], ROOT / "home")
        self.assertIn("--sdk-path=/sdk with spaces", commands[-1])
        self.assertIn("--no-sign", commands[-1])
        self.assertTrue(all("--offline" in c for c in commands[1:]))
        self.assertFalse(any("adb" in c or "fastboot" in c for c in commands))
        self.assertFalse(any("OctoSense-mobile" in arg for c in commands for arg in c))

    def test_existing_packager_skips_tool_compilation(self):
        args = self.args("--variant", "standalone", "--development", "--packager", "/tools/cargo-makepad")
        plan = build.build_plan(args)
        self.assertEqual(len(plan), 3)
        self.assertEqual(plan[-1][1][0], "/tools/cargo-makepad")


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
