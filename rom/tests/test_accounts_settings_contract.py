import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]

class AccountsSettingsContractTest(unittest.TestCase):
    def test_actions_identity_and_observation_expiry(self):
        java_home = os.environ.get("JAVA_HOME")
        javac = str(Path(java_home) / "bin/javac") if java_home else shutil.which("javac")
        java = str(Path(java_home) / "bin/java") if java_home else shutil.which("java")
        if not javac or not java:
            self.skipTest("JDK required for Accounts settings contract")
        with tempfile.TemporaryDirectory() as classes:
            subprocess.run([javac, "-d", classes,
                str(ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/accounts/AccountsSettingsContract.java"),
                str(ROOT / "tests/java/AccountsSettingsContractTest.java")], check=True, capture_output=True, text=True)
            subprocess.run([java, "-cp", classes, "AccountsSettingsContractTest"], check=True, capture_output=True, text=True)
