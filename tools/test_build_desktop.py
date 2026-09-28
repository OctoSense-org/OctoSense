"""Desktop packaging contracts; no downloads, real credentials or LLM calls."""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("build_desktop", Path(__file__).with_name("build-desktop.py"))
desktop = importlib.util.module_from_spec(spec)
spec.loader.exec_module(desktop)
REV = "a6ea8505170735f191a12bb47629a6728415d417"


class DesktopRuntime(unittest.TestCase):
    def test_plan_uses_locked_kernel_and_stages_next_to_shell(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "not-created"
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                desktop.main(["--plan", "--profile", "dev", "--target-dir", str(root), "--offline"])
            plan = json.loads(output.getvalue())
            self.assertEqual(plan["revision"], desktop.kernel_tool.octos_revision())
            self.assertEqual(Path(plan["shell"]).parent, Path(plan["packaged_kernel"]).parent)
            self.assertIn("--locked", plan["kernel_build"])
            self.assertIn("--offline", plan["kernel_build"])
            self.assertFalse(root.exists())

    def test_version_must_identify_the_locked_revision(self):
        self.assertTrue(desktop.version_matches(f"octos 2.0.3-rc.13 ({REV[:7]} 2026-09-27)", REV))
        self.assertFalse(desktop.version_matches("octos 2.0.3-rc.13 (0000000 2026-09-27)", REV))
        self.assertFalse(desktop.version_matches("octos 2.0.3", REV))

    def test_wrong_runtime_preserves_existing_binary_and_receipt(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "source"
            source.write_bytes(b"kernel")
            out = root / "packaged"
            out.mkdir()
            dest = out / ("octos-kernel" + desktop.SUFFIX)
            dest.write_bytes(b"previous")
            receipt = out / "octos-kernel.json"
            receipt.write_text("previous receipt")
            with patch.object(desktop.subprocess, "check_output", return_value="octos wrong"):
                with self.assertRaisesRegex(RuntimeError, "differs"):
                    desktop.stage(source, out, REV)
            self.assertEqual(dest.read_bytes(), b"previous")
            self.assertEqual(receipt.read_text(), "previous receipt")
            self.assertEqual(sorted(p.name for p in out.iterdir()), sorted([dest.name, receipt.name]))

    @unittest.skipIf(os.name == "nt", "executable script fixture is POSIX")
    def test_staged_executable_is_verified_and_receipted(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "source"
            source.write_text(f"#!/bin/sh\nprintf '%s\\n' 'octos 2.0.3-rc.13 ({REV[:7]} 2026-09-27)'\n")
            dest = desktop.stage(source, root / "with spaces", REV)
            receipt = json.loads(dest.with_name("octos-kernel.json").read_text())
            self.assertEqual(receipt["revision"], REV)
            self.assertEqual(receipt["sha256"], hashlib.sha256(dest.read_bytes()).hexdigest())
            self.assertTrue(os.access(dest, os.X_OK))

    def test_modified_private_source_is_preserved(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            source = work / "src"
            source.mkdir()
            (source / "keep").write_text("user work")
            with self.assertRaisesRegex(RuntimeError, "Preserving"):
                desktop.source_at(REV, work, offline=True)
            self.assertEqual((source / "keep").read_text(), "user work")


if __name__ == "__main__":
    unittest.main()
