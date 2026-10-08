"""The native fixture's atomic command publication tolerates only bounded sharing errors."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch


spec = importlib.util.spec_from_file_location("browser_smoke", Path(__file__).with_name("browser-smoke.py"))
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)


class Clock:
    def __init__(self):
        self.now = 0.0
        self.sleeps = []

    def monotonic(self):
        return self.now

    def sleep(self, delay):
        self.sleeps.append(delay)
        self.now += delay


def windows_error(code):
    error = PermissionError(13, "synthetic command-file collision")
    error.winerror = code
    return error


class CommandPublicationTests(unittest.TestCase):
    def test_transient_windows_errors_publish_the_same_complete_command(self):
        for code in (5, 32, 33):
            with self.subTest(winerror=code), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                staged, destination = root / "command.next", root / "command.json"
                payload = json.dumps({"id": 11, "op": "close"}).encode()
                previous = json.dumps({"id": 10, "op": "inspect"}).encode()
                staged.write_bytes(payload)
                destination.write_bytes(previous)
                attempts = []
                clock = Clock()
                real_replace = Path.replace

                def replace(path, target):
                    attempts.append((path, target, path.read_bytes()))
                    self.assertEqual(destination.read_bytes(), previous)
                    if len(attempts) <= 2:
                        raise windows_error(code)
                    return real_replace(path, target)

                with patch.object(smoke.os, "name", "nt"), \
                     patch.object(smoke.time, "monotonic", clock.monotonic), \
                     patch.object(smoke.time, "sleep", clock.sleep), \
                     patch.object(Path, "replace", replace):
                    smoke.replace_command(staged, destination, Mock(poll=Mock(return_value=None)))
                self.assertEqual(attempts, [(staged, destination, payload)] * 3)
                self.assertEqual(destination.read_bytes(), payload)
                self.assertFalse(staged.exists())
                self.assertEqual(clock.sleeps, [.025, .025])

    def test_persistent_collision_is_bounded_and_preserves_terminal_error(self):
        clock = Clock()
        error = windows_error(5)
        staged = Mock()
        staged.replace.side_effect = error
        with patch.object(smoke.os, "name", "nt"), \
             patch.object(smoke.time, "monotonic", clock.monotonic), \
             patch.object(smoke.time, "sleep", clock.sleep):
            with self.assertRaises(PermissionError) as raised:
                smoke.replace_command(staged, Mock(), Mock(poll=Mock(return_value=None)))
        self.assertIs(raised.exception, error)
        self.assertAlmostEqual(clock.now, 2.0)
        self.assertLessEqual(staged.replace.call_count, 82)

    def test_other_errors_are_not_retried(self):
        for platform, error in (("nt", windows_error(123)), ("nt", OSError(2, "synthetic missing file")),
                                ("posix", windows_error(5))):
            with self.subTest(platform=platform, winerror=getattr(error, "winerror", None)):
                staged = Mock()
                staged.replace.side_effect = error
                with patch.object(smoke.os, "name", platform), patch.object(smoke.time, "sleep") as sleep:
                    with self.assertRaises(OSError) as raised:
                        smoke.replace_command(staged, Mock(), None)
                self.assertIs(raised.exception, error)
                staged.replace.assert_called_once()
                sleep.assert_not_called()

    def test_child_exit_stops_retry_before_another_publication(self):
        error = windows_error(32)
        staged = Mock()
        staged.replace.side_effect = error
        process = Mock(poll=Mock(side_effect=[None, 7]))
        with patch.object(smoke.os, "name", "nt"), patch.object(smoke.time, "sleep") as sleep:
            with self.assertRaisesRegex(AssertionError, "status 7") as raised:
                smoke.replace_command(staged, Mock(), process)
        self.assertIs(raised.exception.__cause__, error)
        staged.replace.assert_called_once()
        sleep.assert_called_once()

    def test_exited_child_receives_no_command(self):
        staged = Mock()
        with self.assertRaisesRegex(AssertionError, "status 0"):
            smoke.replace_command(staged, Mock(), Mock(poll=Mock(return_value=0)))
        staged.replace.assert_not_called()


if __name__ == "__main__":
    unittest.main()
