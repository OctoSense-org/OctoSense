"""Artifact handling checks; these do not launch native windows."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import smoke


class ArtifactTests(unittest.TestCase):
    def test_runtime_errors_cannot_pass_with_a_successful_frame(self):
        for message in [
            "draw shader 'DrawShellGlass' failed to compile and will NOT be drawn",
            "[E] platform/src/os/apple/metal.rs:2640:21 - Metal compilation failed",
            "thread 'main' panicked at 'draw list mismatch'",
        ]:
            with self.subTest(message=message), self.assertRaises(AssertionError):
                smoke.assert_no_runtime_errors(message)
        smoke.assert_no_runtime_errors("[I] wm: desktop style makeos applied\n"
                                       "error: package(s) `makeos-package-does-not-exist` not found\n")

    def test_smoke_accepts_explicit_artifact_directory(self):
        result = subprocess.run([sys.executable, smoke.__file__, "--help"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--artifacts-dir", result.stdout)

    def test_artifacts_directory_must_not_overwrite_previous_run(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(sys, "argv", [smoke.__file__, "--artifacts-dir", directory]):
                with patch.object(smoke.subprocess, "Popen") as launch:
                    with self.assertRaises(FileExistsError):
                        smoke.main()
                    launch.assert_not_called()

    def test_single_and_multiwindow_grabs_are_copied_into_report(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifacts = root / "artifacts"
            artifacts.mkdir()
            remote = root / "remote.png"
            remote.write_bytes(b"fixture image bytes")
            smoke.save_grab(artifacts, "frame", {"png": str(remote), "w": 0})
            smoke.save_grab(artifacts, "final-frame", {"png": [str(remote)], "quit": 1})
            remote.unlink()
            single = json.loads((artifacts / "frame.json").read_text())
            multi = json.loads((artifacts / "final-frame.json").read_text())
            for path in [single["png"], *multi["png"]]:
                self.assertEqual(Path(path).parent, artifacts)
                self.assertEqual(Path(path).read_bytes(), b"fixture image bytes")
            self.assertEqual(single["w"], 0)
            self.assertEqual(multi["quit"], 1)


if __name__ == "__main__":
    unittest.main()
