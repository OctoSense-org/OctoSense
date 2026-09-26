import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class DeviceSettingsContractTest(unittest.TestCase):
    def test_external_entries_are_navigation_only_and_do_not_override_android_actions(self):
        self.run_contract("SettingsEntryContract", "SettingsEntryContractTest",
                          main_class="dev.makepad.octosense.SettingsEntryContractTest")

    def test_accessibility_roles_identity_and_scroll_graph_are_bounded(self):
        self.run_contract("SettingsAccessibilityContract", "SettingsAccessibilityContractTest",
                          main_class="dev.makepad.octosense.SettingsAccessibilityContractTest")

    def test_history_pages_and_unicode_are_bounded(self):
        self.run_contract("NotificationHistoryContract", "NotificationHistoryContractTest",
                          ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/notifications/NotificationHistoryContract.java")

    def test_civil_time_dst_and_zone_choices_are_strict(self):
        self.run_contract("DateTimeContract", "DateTimeContractTest",
                          ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/datetime/DateTimeContract.java")

    def test_bluetooth_actions_names_and_identity_are_strict(self):
        self.run_contract("BluetoothSettingsContract", "BluetoothSettingsContractTest",
                          ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/bluetooth/BluetoothSettingsContract.java")

    def test_control_pages_and_choices_are_finite(self):
        self.run_contract("SettingsControlsContract", "SettingsControlsContractTest",
                          ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/controls/SettingsControlsContract.java")

    def test_wifi_actions_and_observed_identity_are_strict(self):
        self.run_contract("WifiSettingsContract", "WifiSettingsContractTest",
                          ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/wifi/WifiSettingsContract.java")

    def test_apps_requests_are_finite_and_strict(self):
        self.run_contract("AppsSettingsContract", "AppsSettingsContractTest")

    def test_untrusted_values_cannot_select_arbitrary_settings(self):
        self.run_contract("DeviceSetting", "DeviceSettingContractTest")

    def run_contract(self, source, harness, source_path=None, main_class=None):
        java_home = os.environ.get("JAVA_HOME")
        javac = str(Path(java_home) / "bin/javac") if java_home else shutil.which("javac")
        java = str(Path(java_home) / "bin/java") if java_home else shutil.which("java")
        if not javac or not java:
            self.skipTest("JDK required for the device settings contract")
        with tempfile.TemporaryDirectory() as classes:
            subprocess.run([
                javac, "-d", classes,
                *([str(ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/controls/DndMode.java")]
                  if source == "SettingsControlsContract" else []),
                *([str(ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/controls/ColorAccessibility.java"), str(ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/controls/HearingSettings.java"), str(ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/controls/AccessibilityTextMotorSettings.java")] if source == "SettingsControlsContract" else []),
                str(source_path or ROOT / f"home/resources/android/java/dev/makepad/octosense/{source}.java"),
                str(ROOT / f"tests/java/{harness}.java"),
            ], check=True, capture_output=True, text=True)
            subprocess.run([java, "-cp", classes, main_class or harness],
                           check=True, capture_output=True, text=True)


if __name__ == "__main__":
    unittest.main()
