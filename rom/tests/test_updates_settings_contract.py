import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]

class UpdatesSettingsContractTest(unittest.TestCase):
    def test_async_settings_state_with_fake_platform_boundary(self):
        java_home = os.environ.get("JAVA_HOME")
        javac = str(Path(java_home) / "bin/javac") if java_home else shutil.which("javac")
        java = str(Path(java_home) / "bin/java") if java_home else shutil.which("java")
        if not javac or not java:
            self.skipTest("JDK required for update Settings state tests")
        sources = list((ROOT / "tests/java/updates-stubs").rglob("*.java")) + [
            ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/updates/UpdatesSettingsContract.java",
            ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/updates/UpdatesSettingsSnapshot.java",
            ROOT / "vendor/octosense/agent/src/dev/makepad/octosense/agent/UpdateSettings.java",
            ROOT / "tests/java/UpdatesSettingsStateTest.java",
        ]
        with tempfile.TemporaryDirectory() as classes:
            result = subprocess.run([javac, "-d", classes, *map(str, sources)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            result = subprocess.run([java, "-cp", classes, "dev.makepad.octosense.agent.UpdatesSettingsStateTest"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_review_identity_expiry_and_concurrent_consumption(self):
        java_home = os.environ.get("JAVA_HOME")
        javac = str(Path(java_home) / "bin/javac") if java_home else shutil.which("javac")
        java = str(Path(java_home) / "bin/java") if java_home else shutil.which("java")
        if not javac or not java:
            self.skipTest("JDK required for update Settings contract")
        with tempfile.TemporaryDirectory() as classes:
            subprocess.run([javac, "-d", classes,
                str(ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/updates/UpdatesSettingsContract.java"),
                str(ROOT / "tests/java/UpdatesSettingsContractTest.java")], check=True, capture_output=True, text=True)
            subprocess.run([java, "-cp", classes, "UpdatesSettingsContractTest"], check=True, capture_output=True, text=True)
