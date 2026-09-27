"""Finite native caption choices, packed-alpha preservation and editor authority."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class CaptionCustomSettingsTest(unittest.TestCase):
    def test_native_packing_finite_writes_and_scoped_lifecycle(self):
        jdk = os.environ.get("JAVA_HOME")
        javac = str(Path(jdk) / "bin/javac") if jdk else shutil.which("javac")
        java = str(Path(jdk) / "bin/java") if jdk else shutil.which("java")
        if not javac or not java:
            self.skipTest("JDK required")
        with tempfile.TemporaryDirectory() as output:
            sources = [ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/controls/CaptionCustomSettings.java", ROOT / "tests/java/CaptionCustomSettingsTest.java"]
            for command in ([javac, "-d", output, *map(str, sources)], [java, "-cp", output, "CaptionCustomSettingsTest"]):
                result = subprocess.run(command, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
