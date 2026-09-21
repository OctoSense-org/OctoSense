"""Artifact handling checks; these do not launch native windows."""
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from urllib.error import HTTPError

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
        smoke.assert_no_runtime_errors("[I] wm: desktop style octosense applied\n"
                                       "error: package(s) `octosense-package-does-not-exist` not found\n")

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


def refusal(body='requested input frame could not be submitted; retry'):
    return HTTPError('http://127.0.0.1/click', 404, 'Not Found', {}, io.BytesIO(body.encode()))


class RemoteInputTests(unittest.TestCase):
    @patch('smoke.time.sleep')
    @patch('smoke.urlopen')
    def test_input_presentation_failure_waits_without_replaying_input(self, request, sleep):
        for route in ('click', 'k'):
            with self.subTest(route=route):
                request.reset_mock()
                request.side_effect = [refusal(), io.StringIO('{"png":"frame.png"}')]
                self.assertEqual(smoke.get(1, route, w=2, wait=1), {'ok': 1})
                self.assertEqual(request.call_count, 2)
                self.assertEqual(request.call_args_list[0].args[0], f'http://127.0.0.1:1/{route}?w=2&wait=1')
                self.assertEqual(request.call_args_list[1].args[0], 'http://127.0.0.1:1/g?w=2')

    @patch('smoke.time.sleep')
    @patch('smoke.urlopen')
    def test_persistent_refusal_is_bounded(self, request, sleep):
        request.side_effect = [refusal('grab frame could not be submitted at arming; retry') for _ in range(3)]
        with self.assertRaisesRegex(ValueError, 'HTTP 404'):
            smoke.get(1, 'g')
        self.assertEqual(request.call_count, 3)

    @patch('smoke.time.sleep')
    @patch('smoke.urlopen')
    def test_failed_capture_never_replays_applied_input(self, request, sleep):
        request.side_effect = [refusal()] + [refusal('grab frame could not be submitted at arming; retry') for _ in range(3)]
        with self.assertRaisesRegex(ValueError, 'HTTP 404'):
            smoke.get(1, 'click', x=1, y=2, wait=1)
        urls = [call.args[0] for call in request.call_args_list]
        self.assertEqual(len(urls), 4)
        self.assertEqual(sum('/click?' in url for url in urls), 1)

    @patch('smoke.urlopen')
    def test_other_errors_are_not_replayed(self, request):
        request.side_effect = refusal('widget not found')
        with self.assertRaisesRegex(ValueError, 'widget not found'):
            smoke.get(1, 'click')
        self.assertEqual(request.call_count, 1)

    @patch('smoke.time.sleep')
    @patch('smoke.urlopen')
    def test_retries_grab_refused_before_arming(self, request, sleep):
        request.side_effect = [refusal('grab frame could not be submitted at arming; retry'), io.StringIO('{"png":"frame.png"}')]
        self.assertEqual(smoke.get(1, 'g'), {'png': 'frame.png'})
        self.assertEqual(request.call_count, 2)

if __name__ == "__main__":
    unittest.main()
