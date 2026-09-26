from pathlib import Path
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
A = "{http://schemas.android.com/apk/res/android}"


class SettingsBrokerBoundaryTest(unittest.TestCase):
    def test_only_finite_signed_settings_binders_are_exported_from_system_uid(self):
        manifest = ET.parse(ROOT / "vendor/octosense/settings-broker/AndroidManifest.xml").getroot()
        self.assertEqual(manifest.get(A + "sharedUserId"), "android.uid.system")
        application = manifest.find("application")
        self.assertNotEqual(application.get(A + "persistent"), "true")
        self.assertEqual(application.get(A + "allowBackup"), "false")
        self.assertEqual(application.findall("receiver"), [])
        self.assertEqual(application.findall("provider"), [])
        self.assertEqual(application.findall(".//intent-filter"), [])
        components = list(application)
        exported = [item for item in components if item.get(A + "exported") == "true"]
        self.assertEqual({item.get(A + "name") for item in exported},
                         {".OctoSenseAccountSettingsService", ".OctoSenseZenSettingsService", ".OctoSenseAppNotificationsService",
                          ".OctoSenseAppNetworkService", ".OctoSenseAppBatteryService", ".OctoSenseAppStorageService", ".OctoSenseAppLanguageService", ".OctoSenseSystemLanguageService", ".OctoSenseKeyboardService"})
        permission = "dev.makepad.octosense.permission.BIND_AGENT_PLATFORM"
        for service in exported:
            self.assertEqual(service.tag, "service")
            self.assertEqual(service.get(A + "permission"), permission)
        agent = ET.parse(ROOT / "vendor/octosense/agent/AndroidManifest.xml").getroot()
        self.assertIsNone(agent.get(A + "sharedUserId"), "general Agent must not inherit system UID")
        declaration = next(item for item in agent.findall("permission") if item.get(A + "name") == permission)
        self.assertEqual(declaration.get(A + "protectionLevel"), "signature")
        self.assertTrue(any(item.get(A + "name") == permission for item in agent.findall("uses-permission")))
        confirmation = next(item for item in application.findall("activity") if item.get(A + "name") == ".OctoSenseRemoveAccountActivity")
        self.assertEqual(confirmation.get(A + "name"), ".OctoSenseRemoveAccountActivity")
        self.assertEqual(confirmation.get(A + "exported"), "false")
        keyboard = next(item for item in application.findall("activity") if item.get(A + "name") == ".OctoSenseKeyboardActivity")
        self.assertEqual(keyboard.get(A + "exported"), "false")
        avatar = next(item for item in application.findall("activity") if item.get(A + "name") == "com.android.settingslib.avatarpicker.AvatarPickerActivity")
        self.assertEqual(avatar.get("{http://schemas.android.com/tools}node"), "remove")
        self.assertNotIn("android.permission.INTERNET", [item.get(A + "name") for item in manifest.findall("uses-permission")])

    def test_helper_and_broker_share_exact_account_wire_contract(self):
        path = "src/dev/makepad/octosense/settingsbroker/IAccountSettings.aidl"
        broker = ROOT / "vendor/octosense/settings-broker" / path
        helper = ROOT / "vendor/octosense/agent" / path
        self.assertEqual(broker.read_bytes(), helper.read_bytes())
        obsolete = ROOT / "home/android/platform-build/systemui/files/src/com/android/systemui/octosense"
        self.assertFalse((obsolete / "IAccountSettings.aidl").exists())
        self.assertFalse((obsolete / "OctoSenseAccountSettingsService.java").exists())
        self.assertFalse((obsolete / "OctoSenseRemoveAccountActivity.java").exists())

    def test_helper_and_broker_share_exact_dnd_wire_contract(self):
        path = "src/dev/makepad/octosense/settingsbroker/IZenSettings.aidl"
        self.assertEqual((ROOT / "vendor/octosense/settings-broker" / path).read_bytes(),
                         (ROOT / "vendor/octosense/agent" / path).read_bytes())

    def test_helper_and_broker_share_exact_app_notifications_wire_contract(self):
        path = "src/dev/makepad/octosense/settingsbroker/IAppNotifications.aidl"
        self.assertEqual((ROOT / "vendor/octosense/settings-broker" / path).read_bytes(),
                         (ROOT / "vendor/octosense/agent" / path).read_bytes())

    def test_helper_and_broker_share_finite_app_policy_contracts(self):
        for name in ("IAppNetworkSettings", "IAppBatterySettings", "IAppStorageSettings", "IAppLanguageSettings", "ISystemLanguageSettings", "IKeyboardSettings"):
            path = "src/dev/makepad/octosense/settingsbroker/" + name + ".aidl"
            broker = (ROOT / "vendor/octosense/settings-broker" / path).read_bytes()
            self.assertEqual(broker, (ROOT / "vendor/octosense/agent" / path).read_bytes())
            self.assertNotIn(b"int uid", broker)
            self.assertNotIn(b"int policy", broker)
