import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]

# The real SystemSettings class is compiled with Android's public constants.
# Only package observations and Activity launch are replaced for fault injection.
STUBS = {
    "android/content/Context.java": """package android.content;
public abstract class Context { public abstract android.content.pm.PackageManager getPackageManager();
public abstract String getPackageName(); public abstract void startActivity(Intent value); }""",
    "android/content/ComponentName.java": """package android.content;
public final class ComponentName { final String pkg,name; public ComponentName(String p,String n){pkg=p;name=n;}
public String getPackageName(){return pkg;} public String getClassName(){return name;}
public String flattenToString(){return pkg+\"/\"+name;} }""",
    "android/content/Intent.java": """package android.content;
public final class Intent { public static final int FLAG_ACTIVITY_NEW_TASK=0x10000000,FLAG_ACTIVITY_CLEAR_TOP=0x04000000,FLAG_ACTIVITY_MULTIPLE_TASK=0x08000000,FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS=0x00800000;
public int flags;
String action,pkg; ComponentName component; public final java.util.Map<String,String> extras=new java.util.HashMap<>();
public Intent(){} public Intent(String a){action=a;} public Intent(String a,android.net.Uri uri){action=a;}
public Intent setComponent(ComponentName c){component=c;return this;} public ComponentName getComponent(){return component;}
public Intent setPackage(String p){pkg=p;return this;} public String getPackage(){return pkg;}
public Intent addFlags(int f){flags|=f;return this;} public Intent putExtra(String k,String v){extras.put(k,v);return this;}
public String getStringExtra(String k){return extras.get(k);} public String getAction(){return action;} }""",
    "android/content/ActivityNotFoundException.java": """package android.content;
public final class ActivityNotFoundException extends RuntimeException {}""",
    "android/content/pm/ActivityInfo.java": """package android.content.pm;
public final class ActivityInfo { public String name,packageName,targetActivity; public boolean enabled,exported;
public ApplicationInfo applicationInfo; public android.os.Bundle metaData; }""",
    "android/content/pm/ApplicationInfo.java": """package android.content.pm;
public final class ApplicationInfo { public static final int FLAG_SYSTEM=1,FLAG_UPDATED_SYSTEM_APP=128;
public boolean enabled; public int flags; }""",
    "android/content/pm/PackageManager.java": """package android.content.pm;
public abstract class PackageManager { public static final int GET_META_DATA=128,SIGNATURE_MATCH=0,COMPONENT_ENABLED_STATE_DEFAULT=0,COMPONENT_ENABLED_STATE_ENABLED=1;
public static class NameNotFoundException extends Exception {}
public abstract ActivityInfo getActivityInfo(android.content.ComponentName n,int flags) throws NameNotFoundException;
public abstract int checkSignatures(String a,String b); public abstract int getApplicationEnabledSetting(String n);
public abstract int getComponentEnabledSetting(android.content.ComponentName n); }""",
    "android/net/Uri.java": """package android.net;
public final class Uri { public static Uri parse(String value){return new Uri();} }""",
    "android/os/Build.java": """package android.os;
public final class Build { public static class VERSION { public static int SDK_INT=33; } }""",
    "android/os/Bundle.java": """package android.os;
public final class Bundle { private final java.util.Map<String,Object> values=new java.util.HashMap<>();
public void putBoolean(String key,boolean value){values.put(key,value);}
public void putInt(String key,int value){values.put(key,value);} public void putString(String key,String value){values.put(key,value);}
public boolean getBoolean(String key,boolean fallback){Object value=values.get(key);return value instanceof Boolean?(Boolean)value:fallback;} }""",
}


class SystemSettingsRoutingTest(unittest.TestCase):
    def test_android_defaults_live_on_renderer_for_updated_system_package_priority(self):
        # Android matches an updated alias against its target Activity before
        # inspecting the exact alias. Putting these filters only on the alias
        # therefore loses their priority after a Home APK update over the ROM.
        android = "{http://schemas.android.com/apk/res/android}"
        manifest = ET.parse(ROOT / "home/resources/android/AndroidManifest.xml.template").getroot()
        application = manifest.find("application")
        entry = next(node for node in application.findall("activity-alias")
                     if node.get(android + "name") == "dev.makepad.octosense.SettingsEntry")
        renderer = next(node for node in application.findall("activity")
                        if node.get(android + "name") == entry.get(android + "targetActivity"))
        expected = {"SETTINGS", "DISPLAY_SETTINGS", "NIGHT_DISPLAY_SETTINGS", "SOUND_SETTINGS", "WIFI_SETTINGS", "BLUETOOTH_SETTINGS",
                    "MANAGE_APPLICATIONS_SETTINGS", "APPLICATION_SETTINGS", "SYNC_SETTINGS", "LOCATION_SOURCE_SETTINGS",
                    "BATTERY_SAVER_SETTINGS", "INTERNAL_STORAGE_SETTINGS", "DATE_SETTINGS", "DEVICE_INFO_SETTINGS",
                    "SYSTEM_UPDATE_SETTINGS", "AIRPLANE_MODE_SETTINGS", "DATA_SAVER_SETTINGS", "PRIVACY_SETTINGS",
                    "NOTIFICATION_SETTINGS", "ZEN_MODE_SETTINGS", "APP_NOTIFICATION_SETTINGS", "MANAGE_DEFAULT_APPS_SETTINGS"}
        actual = set()
        for intent in renderer.findall("intent-filter"):
            actions = {action.get(android + "name") for action in intent.findall("action")}
            standards = {action.removeprefix("android.settings.") for action in actions if action.startswith("android.settings.")}
            if standards:
                if "MANAGE_DEFAULT_APPS_SETTINGS" in standards:
                    self.assertEqual(standards, {"MANAGE_DEFAULT_APPS_SETTINGS"}, "new Defaults action cannot cap older ROM filters")
                    self.assertEqual(intent.get(android + "priority"), "3", "native PermissionController has priority 2")
                else:
                    self.assertEqual(intent.get(android + "priority"), "2")
                self.assertIn("android.intent.category.DEFAULT", {node.get(android + "name") for node in intent.findall("category")})
                if "NIGHT_DISPLAY_SETTINGS" in standards:
                    self.assertEqual(standards, {"NIGHT_DISPLAY_SETTINGS"}, "new actions cannot cap existing ROM filters")
                if "APP_NOTIFICATION_SETTINGS" in standards:
                    self.assertEqual(standards, {"APP_NOTIFICATION_SETTINGS"}, "targeted entries must not cap older ROM filters")
                actual.update(standards)
        self.assertEqual(actual, expected)
        alias_actions = {action.get(android + "name") for intent in entry.findall("intent-filter") for action in intent.findall("action")}
        self.assertEqual(alias_actions, {"dev.makepad.octosense.action.SETTINGS"})
        self.assertTrue(all(int(intent.get(android + "priority", "0")) == 0 for intent in entry.findall("intent-filter")))
        capability = next(node for node in entry.findall("meta-data")
                          if node.get(android + "name") == "dev.makepad.octosense.SETTINGS_DEFAULT_APPS")
        self.assertEqual(capability.get(android + "value"), "true")
        dnd_capability=next(node for node in entry.findall("meta-data") if node.get(android+"name")=="dev.makepad.octosense.SETTINGS_DND")
        self.assertEqual(dnd_capability.get(android+"value"),"true")

    def test_preferred_routing_requires_enabled_platform_system_alias_and_preserves_native_consent(self):
        java_home = os.environ.get("JAVA_HOME")
        javac = str(Path(java_home) / "bin/javac") if java_home else shutil.which("javac")
        java = str(Path(java_home) / "bin/java") if java_home else shutil.which("java")
        sdk = os.environ.get("ANDROID_SDK_ROOT") or os.environ.get("ANDROID_HOME")
        jars = sorted((Path(sdk) / "platforms").glob("*/android.jar")) if sdk else []
        if not jars:
            jars = list((Path.home() / ".local/share/octosense/android-tools/makepad-android/platforms").glob("*/android.jar"))
        if not javac or not java or not jars:
            self.skipTest("JDK and Android SDK required for system Settings routing tests")
        sources = [ROOT / "home/android/contracts/src/main/java/dev/makepad/octosense/contracts/SystemSettings.java",
                   ROOT / "home/android/contracts/src/main/java/dev/makepad/octosense/contracts/Protocol.java",
                   ROOT / "tests/java/SystemSettingsRoutingTest.java"]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, body in STUBS.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(body)
                sources.append(path)
            classes = root / "classes"
            result = subprocess.run([javac, "-cp", str(jars[0]), "-d", str(classes), *map(str, sources)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            result = subprocess.run([java, "-cp", str(classes), "SystemSettingsRoutingTest"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
