import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]

class NetworkSettingsContractTest(unittest.TestCase):
    def test_hostname_and_finite_network_values(self):
        java_home = os.environ.get("JAVA_HOME")
        javac = str(Path(java_home) / "bin/javac") if java_home else shutil.which("javac")
        java = str(Path(java_home) / "bin/java") if java_home else shutil.which("java")
        if not javac or not java:
            self.skipTest("JDK required for network Settings contract")
        sources = [ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/network/NetworkSettingsContract.java",
            ROOT / "tests/java/NetworkSettingsContractTest.java"]
        with tempfile.TemporaryDirectory() as classes:
            result = subprocess.run([javac, "-d", classes, *map(str, sources)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            result = subprocess.run([java, "-cp", classes, "NetworkSettingsContractTest"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
