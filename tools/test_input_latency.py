import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    "input_latency", Path(__file__).parent / "perf/android/input_latency.py"
)
latency = importlib.util.module_from_spec(spec)
spec.loader.exec_module(latency)


class InputLatencyTests(unittest.TestCase):
    def scenario(self, timestamps=(1000000000, 1008000000, 1016000000)):
        return {
            "name": "linear-drag",
            "before": {"search_scroll": 0},
            "frames": [
                {"ns": 1040000000, "search_scroll": 300},
                {"ns": 1050000000, "search_scroll": 400},
            ],
            "injection": "\n".join(
                f"{ns},{0 if i == 0 else 2},530,{1750 - i * 200}"
                for i, ns in enumerate(timestamps)
            ),
        }

    def test_constant_speed_content_age(self):
        result = latency.summarize(self.scenario(), [{
            "id": 42, "submitted_ns": 1045000000, "present_ns": 1060000000,
        }], 1, 12)
        self.assertTrue(result["input_cadence_valid"])
        self.assertEqual(result["observed_content_age_ms"]["max"], 48)
        self.assertEqual(result["matched_frames"], 1)

    def test_injector_stall_invalidates_attribution_without_hiding_tail(self):
        result = latency.summarize(self.scenario((1000000000, 1008000000, 1032000000)), [{
            "id": 42, "submitted_ns": 1045000000, "present_ns": 1068000000,
        }], 1, 12)
        self.assertFalse(result["input_cadence_valid"])
        self.assertEqual(result["input_gap_ms"]["max"], 24)
        self.assertEqual(result["observed_content_age_ms"]["max"], 48)
        self.assertTrue(result["worst_samples"][0]["source_gap_exceeded"])

    def test_stall_outside_measured_sample_still_invalidates_whole_run(self):
        result = latency.summarize(self.scenario((1000000000, 1024000000, 1032000000)), [{
            "id": 42, "submitted_ns": 1045000000, "present_ns": 1060000000,
        }], 1, 12)
        self.assertFalse(result["input_cadence_valid"])
        self.assertFalse(result["worst_samples"][0]["source_gap_exceeded"])
        self.assertEqual(result["matched_frames"], 1)

    def test_unmatched_capture_is_not_a_pass(self):
        with self.assertRaisesRegex(ValueError, "No scene/input/EGL matches"):
            latency.summarize(self.scenario(), [], 1, 12)

    def test_nonmonotonic_input_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "increasing actual input timestamps"):
            latency.summarize(self.scenario((1000000000, 999000000, 1016000000)), [], 1, 12)

    def test_duplicate_buffer_scene_match_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "More than one submitted buffer"):
            latency.summarize(self.scenario(), [
                {"id": 42, "submitted_ns": 1045000000, "present_ns": 1060000000},
                {"id": 43, "submitted_ns": 1046000000, "present_ns": 1076000000},
            ], 1, 12)

    def test_cli_fails_cadence_gate_and_keeps_report(self):
        with tempfile.TemporaryDirectory() as directory:
            capture = Path(directory) / "capture.json"
            capture.write_text(json.dumps([
                self.scenario((1000000000, 1008000000, 1032000000))
            ]))
            capture.with_suffix(".log").write_text(
                "[android.present] id=42 submitted_ns=1045000000 present_ns=1068000000\n"
            )
            result = subprocess.run(
                [sys.executable, str(Path(latency.__file__)), "--dpi", "1", str(capture)],
                capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 2, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(len(report), 1)
            self.assertEqual(report[0]["observed_content_age_ms"]["max"], 48)


if __name__ == "__main__":
    unittest.main()
