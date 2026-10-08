"""Failure receipts preserve geometry, never fixture text or raw log content."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from types import SimpleNamespace

TOOLS = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS / 'connected-e2e'))
spec = importlib.util.spec_from_file_location('backend_login_diagnostics',
                                            TOOLS / 'connected-e2e/backend_login.py')
backend = importlib.util.module_from_spec(spec)
spec.loader.exec_module(backend)


class FailureDiagnosticsTests(unittest.TestCase):
    def report(self, rows, windows):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'native.log'
            log.write_text('ScriptError: synthetic-sensitive-content https://example.test/?code=fictional')
            native = SimpleNamespace(log_path=log, child=SimpleNamespace(poll=lambda: None),
                call=lambda route: {'s': rows} if route == 'snap' else {'w': windows})
            return backend.failure_geometry(native)

    def test_reports_clipping_without_text_or_logs(self):
        report = self.report([
            {'ty': 'Label', 'i': 'status', 'r': [24, 801, 382, 0],
             't': 'synthetic-sensitive-content'},
            {'ty': 'TextInput', 'i': 'synthetic-sensitive-content', 'r': [24, 40, 120, 40],
             't': 'synthetic-sensitive-content'},
        ], [{'sz': [430, 720], 'title': 'synthetic-sensitive-content'}])
        self.assertEqual(report['window_sizes'], [[430, 720]])
        self.assertEqual(report['rows'][0]['rect'], [24, 801, 382, 0])
        self.assertEqual(report['log_markers']['ScriptError'], 1)
        output = json.dumps(report)
        for secret in ('synthetic-sensitive-content', 'https:', 'code='):
            self.assertNotIn(secret, output)

    def test_non_numeric_or_unbounded_geometry_cannot_be_published(self):
        report = self.report([{'ty': 'https://example.test', 'i': 'private',
                               'r': ['private', 1, 2, 3]}] * 100,
                             [{'sz': [float('nan'), 500]}] * 10)
        self.assertEqual(len(report['rows']), 80)
        self.assertEqual(report['rows'][0], {'kind': 'other', 'id': None, 'rect': None})
        self.assertEqual(report['window_sizes'], [None] * 4)


if __name__ == '__main__':
    unittest.main()
