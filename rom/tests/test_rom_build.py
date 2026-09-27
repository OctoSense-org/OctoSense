"""A failed or stale ROM build must never look like a fresh exported image."""
from pathlib import Path
import os
import shlex
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class RomBuildTests(unittest.TestCase):
    def run_fixture(self, mode, target="bacon"):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        build, exports = root / "build", root / "exports"
        exports.mkdir()
        (build / "build/soong").mkdir(parents=True)
        product = build / "out/octosense-rom/target/product/enchilada"
        product.mkdir(parents=True)
        old_zip = product / "lineage-old-enchilada.zip"
        old_zip.write_bytes(b"old ROM")
        os.utime(old_zip, (1, 1))
        (exports / old_zip.name).write_bytes(b"previous successful export")
        (exports / "finished.txt").write_text("previous build finished\n")
        (build / "build/soong/soong_ui.bash").write_text("#!/bin/bash\nprintf 'fixture configuration\\n'\n")
        (build / "build/soong/soong_ui.bash").chmod(0o755)
        (build / "build/envsetup.sh").write_text('''
lunch() { :; }
get_build_var() { echo '22.2-fixture-UNOFFICIAL-enchilada'; }
sha256sum() { echo 'fixture checksum'; }
m() {
    case "$FIXTURE_BUILD_MODE" in
        fail) echo 'simulated compiler failure'; return 37 ;;
        stale) return 0 ;;
        aliases)
            local product=out/octosense-rom/target/product/enchilada
            # Model elapsed build time without depending on timestamp resolution.
            touch -t 200001010000 "$FIXTURE_START_MARKER"
            printf 'new ROM' > "$product/lineage_enchilada-ota.zip"
            printf 'new boot' > "$product/boot.img"
            ln "$product/lineage_enchilada-ota.zip" "$product/lineage-22.2-fixture-UNOFFICIAL-enchilada.zip"
            ln -f "$product/lineage_enchilada-ota.zip" "$product/lineage-old-enchilada.zip"
            ;;
    esac
}
''')
        script = (ROOT / "scripts/build-rom.sh").read_text()
        script = script.replace("cd /build\n", f"cd {shlex.quote(str(build))}\n")
        script = script.replace("/exports/rom-build", str(exports))
        result = subprocess.run(["bash", "-c", script, "build-rom.sh", target],
                                env={**os.environ, "FIXTURE_BUILD_MODE": mode,
                                     "FIXTURE_START_MARKER": str(exports / "started.txt")},
                                capture_output=True, text=True)
        return result, exports, old_zip.name

    def test_compiler_failure_survives_log_pipeline(self):
        result, exports, old_name = self.run_fixture("fail")
        self.assertEqual(result.returncode, 37, result.stderr)
        self.assertFalse((exports / "finished.txt").exists())
        self.assertIn("simulated compiler failure", (exports / "bacon.log").read_text())
        self.assertEqual((exports / old_name).read_bytes(), b"previous successful export")

    def test_success_without_new_zip_cannot_export_old_build(self):
        result, exports, old_name = self.run_fixture("stale")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("refusing stale artifacts", result.stderr)
        self.assertFalse((exports / "finished.txt").exists())
        self.assertEqual((exports / old_name).read_bytes(), b"previous successful export")

    def test_module_failure_is_not_reported_as_success(self):
        result, exports, _ = self.run_fixture("fail", "module")
        self.assertEqual(result.returncode, 37, result.stderr)
        self.assertIn("simulated compiler failure", (exports / "module.log").read_text())

    def test_old_release_aliases_do_not_change_the_exported_build_name(self):
        result, exports, old_name = self.run_fixture("aliases")
        self.assertEqual(result.returncode, 0, result.stderr)
        new_zip = exports / "lineage-22.2-fixture-UNOFFICIAL-enchilada.zip"
        self.assertEqual(new_zip.read_bytes(), b"new ROM")
        self.assertFalse((exports / old_name).exists())
        self.assertTrue((exports / "finished.txt").exists())


if __name__ == "__main__":
    unittest.main()
