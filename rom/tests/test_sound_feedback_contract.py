import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class SoundFeedbackContractTest(unittest.TestCase):
    def test_device_levels_and_non_atomic_compatibility_writes(self):
        java_home = os.environ.get("JAVA_HOME")
        javac = str(Path(java_home) / "bin/javac") if java_home else shutil.which("javac")
        java = str(Path(java_home) / "bin/java") if java_home else shutil.which("java")
        if not javac or not java:
            self.skipTest("JDK required")
        with tempfile.TemporaryDirectory() as output:
            files = [ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/controls/SoundFeedbackContract.java",
                     ROOT / "tests/java/SoundFeedbackContractTest.java"]
            result = subprocess.run([javac, "-d", output, *map(str, files)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            result = subprocess.run([java, "-cp", output, "SoundFeedbackContractTest"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
