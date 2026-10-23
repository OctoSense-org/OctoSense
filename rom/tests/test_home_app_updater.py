"""Executable JVM policy tests for Home's own APK updater (no device mutation)."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
JAVA = ROOT / "phone/resources/android/java/dev/makepad/octosense"
ANDROID = "{http://schemas.android.com/apk/res/android}"


class HomeUpdaterPolicyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        jdk = os.environ.get("JAVA_HOME")
        cls.javac = str(Path(jdk) / "bin/javac") if jdk else shutil.which("javac")
        cls.java = str(Path(jdk) / "bin/java") if jdk else shutil.which("java")
        if not cls.javac or not cls.java:
            raise unittest.SkipTest("JDK required for Home updater policy")
        cls.classes = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.classes.cleanup)
        run = subprocess.run([cls.javac, "-d", cls.classes.name,
            str(JAVA / "HomeUpdatePolicy.java"), str(ROOT / "rom/tests/java/HomeUpdatePolicyTest.java")],
            capture_output=True, text=True)
        if run.returncode:
            raise AssertionError(run.stdout + run.stderr)

    def run_policy(self, case):
        result = subprocess.run([self.java, "-cp", self.classes.name,
            "dev.makepad.octosense.HomeUpdatePolicyTest", case], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_newer_same_identity(self): self.run_policy("newer")
    def test_standalone_vs_rom_and_isolated_identity(self): self.run_policy("identity")
    def test_other_package_or_unreadable_archive(self): self.run_policy("package")
    def test_downgrade_and_reinstall_rejected(self): self.run_policy("version")
    def test_incompatible_android_version(self): self.run_policy("sdk")
    def test_signer_mismatch_unsigned_and_extra_signer(self): self.run_policy("signature")
    def test_nested_private_cache_file(self): self.run_policy("nested")
    def test_traversal_relative_and_sibling_paths(self): self.run_policy("escape")
    def test_symlink_file_directory_and_root(self): self.run_policy("symlink")
    def test_empty_and_oversized_apk(self): self.run_policy("size")
    def test_exact_staged_bytes_and_mutated_source(self): self.run_policy("hash")
    def test_window_closure_cancels_between_chunks(self): self.run_policy("cancelled_during_copy")
    def test_interrupted_copy_is_cancelled(self): self.run_policy("interrupted")


class HomeUpdaterBoundaryTests(unittest.TestCase):
    def test_manifest_callback_is_private_and_install_is_not_privileged(self):
        manifest = ET.parse(ROOT / "phone/resources/android/AndroidManifest.xml.template")
        callback = next(node for node in manifest.findall("application/activity")
            if node.get(ANDROID + "name") == "dev.makepad.octosense.HomeUpdateInstallActivity")
        self.assertEqual(callback.get(ANDROID + "exported"), "false")
        self.assertFalse(callback.findall("intent-filter"))
        permissions = {node.get(ANDROID + "name") for node in manifest.findall("uses-permission")}
        self.assertIn("android.permission.REQUEST_INSTALL_PACKAGES", permissions)
        self.assertNotIn("android.permission.INSTALL_PACKAGES", permissions)
        self.assertNotIn("android.permission.UPDATE_PACKAGES_WITHOUT_USER_ACTION", permissions)

    def test_private_extension_owns_channel_and_closes_worker(self):
        source = (JAVA / "MakepadAppExtension.java").read_text()
        self.assertIn('"home_updater.command".equals(channel)', source)
        self.assertIn('homeUpdater.command(payload)', source)
        self.assertIn('homeUpdater.close()', source)
        client = (JAVA / "HomeUpdaterClient.java").read_text()
        self.assertIn('setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_REQUIRED)', client)
        self.assertIn('HomeUpdatePolicy.copyVerified', client)
        self.assertNotIn('installReviewedUpdate', client)
        self.assertNotIn('Runtime.getRuntime', client)


class HomeUpdaterSessionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        jdk = os.environ.get("JAVA_HOME")
        javac = str(Path(jdk) / "bin/javac") if jdk else shutil.which("javac")
        cls.java = str(Path(jdk) / "bin/java") if jdk else shutil.which("java")
        if not javac or not cls.java:
            raise unittest.SkipTest("JDK required for Home updater session callback")
        cls.classes = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.classes.cleanup)
        sources = sorted((ROOT / "rom/tests/java/home-updater-stubs").rglob("*.java")) + [
            JAVA / "HomeUpdateInstallActivity.java", ROOT / "rom/tests/java/HomeUpdateInstallTest.java"]
        result = subprocess.run([javac, "-d", cls.classes.name, *map(str, sources)], capture_output=True, text=True)
        if result.returncode:
            raise AssertionError(result.stdout + result.stderr)

    def run_session(self, case):
        result = subprocess.run([self.java, "-cp", self.classes.name,
            "dev.makepad.octosense.HomeUpdateInstallTest", case], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_callback_token_is_required(self): self.run_session("token")
    def test_commit_once_and_explicit_android35_callback(self): self.run_session("commit")
    def test_wrong_session_cannot_open_confirmation(self): self.run_session("wrong_session")
    def test_activity_return_does_not_override_installer(self): self.run_session("confirm")
    def test_system_cancellation(self): self.run_session("cancel")
    def test_system_success_and_late_callback(self): self.run_session("success")
    def test_system_failure(self): self.run_session("failure")
    def test_process_restart_after_self_replacement(self): self.run_session("recovery")
    def test_missing_session_recovery(self): self.run_session("missing_session")
    def test_expired_preparation_is_cleaned(self): self.run_session("expired")
    def test_missing_confirmation_fails_closed(self): self.run_session("missing_confirmation")

    def test_native_closure_cancels_preparation(self): self.run_session("cancel_preparation")
    def test_native_closure_preserves_committed_session(self): self.run_session("preserve_committed")
    def test_stale_closure_cannot_cancel_other_session(self): self.run_session("cancel_other_session")
    def test_stale_callback_cleanup_cannot_overwrite_newer_session(self): self.run_session("stale_cleanup")
