"""Fault-inject the actual kernel backport's transaction and ownership logic."""
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class AtomicRmfbTest(unittest.TestCase):
    def test_drm_transaction_contracts(self):
        patch = (ROOT / 'patches/kernel/oneplus6-atomic-rmfb.patch').read_text()
        added = '\n'.join(line[1:] for line in patch.splitlines()
                          if line.startswith('+') and not line.startswith('+++'))
        start = added.index('int drm_atomic_remove_fb(')
        end = added.index('\n}\n', start) + 3
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            (out / 'atomic_remove_fb_under_test.c').write_text(added[start:end])
            subprocess.run(['cc', '-std=c11', '-Wall', '-Wextra',
                            '-Wno-unused-but-set-variable',
                            '-fsanitize=address,undefined', '-g', '-I', str(out),
                            str(ROOT / 'tests/kernel/atomic_rmfb_harness.c'),
                            '-o', str(out / 'test')], check=True)
            subprocess.run([str(out / 'test')], check=True)


if __name__ == '__main__':
    unittest.main()
