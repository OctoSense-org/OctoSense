import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ColorAccessibilityTest(unittest.TestCase):
    def test_actual_finite_backend_preserves_rows_and_rechecks_authority(self):
        java_home = os.environ.get("JAVA_HOME")
        javac = str(Path(java_home) / "bin/javac") if java_home else shutil.which("javac")
        java = str(Path(java_home) / "bin/java") if java_home else shutil.which("java")
        if not javac or not java:
            self.skipTest("JDK required")
        controls = ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/controls"
        sources = [controls / name for name in ("ColorAccessibility.java", "HearingSettings.java", "AccessibilityTextMotorSettings.java", "SettingsControlsContract.java", "DndMode.java")]
        sources.append(ROOT / "tests/java/ColorAccessibilityTest.java")
        with tempfile.TemporaryDirectory() as output:
            compile_result = subprocess.run([javac, "-d", output, *map(str, sources)], capture_output=True, text=True)
            self.assertEqual(compile_result.returncode, 0, compile_result.stdout + compile_result.stderr)
            result = subprocess.run([java, "-cp", output, "ColorAccessibilityTest"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
