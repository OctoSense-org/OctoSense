#!/usr/bin/env python3
"""Validate built-in Settings through public Android accessibility APIs on an emulator."""
import argparse
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
import zipfile

PACKAGE = "dev.makepad.octosense.settingsa11yfixture"
PUBLISHER = "dev.makepad.octosense.notificationfixture"
LEGACY_PUBLISHER = "dev.makepad.octosense.notificationlegacyfixture"
PUBLISHER_COMPONENT = "dev.makepad.octosense.notificationfixture.NotificationFixture"
ROLE_BROWSERS = ("dev.makepad.octosense.rolebrowserfixture", "dev.makepad.octosense.rolebrowsersecondfixture")
ROLE_ASSISTANT = "dev.makepad.octosense.roleassistantfixture"
ROLE_APK = "/data/local/tmp/octosense-role-browser-b.apk"
PERMISSION_PACKAGES = ("dev.makepad.octosense.permissionfixture", "dev.makepad.octosense.permissionlegacyfixture")
PERMISSION_APK = "/data/local/tmp/octosense-permission-legacy.apk"
ROOT = Path(__file__).resolve().parents[3]


def build_fixture(source, output, run, java_home, sdk, extra_sources=()):
    """Build a disposable public-SDK APK with a fresh independent identity."""
    platform = sdk / "platforms/android-35/android.jar"
    build = sdk / "build-tools/35.0.0"
    (output / "classes").mkdir(parents=True)
    (output / "dex").mkdir()
    resources = source / "res"
    generated = ()
    if resources.is_dir():
        (output / "generated").mkdir()
        run(build / "aapt2", "compile", "--dir", resources, "-o", output / "resources.zip")
        run(build / "aapt2", "link", "--manifest", source / "AndroidManifest.xml", "-I", platform,
            output / "resources.zip", "--java", output / "generated", "-o", output / "unsigned.apk")
        generated = tuple(sorted((output / "generated").rglob("*.java")))
    run(java_home / "bin/javac", "-cp", platform, "-d", output / "classes",
        *sorted(source.glob("*.java")), *extra_sources, *generated)
    run(java_home / "bin/jar", "cf", output / "classes.jar", "-C", output / "classes", ".")
    run(build / "d8", "--lib", platform, "--min-api", "30", "--output", output / "dex", output / "classes.jar")
    if not resources.is_dir():
        run(build / "aapt2", "link", "--manifest", source / "AndroidManifest.xml", "-I", platform, "-o", output / "unsigned.apk")
    with zipfile.ZipFile(output / "unsigned.apk", "a") as archive:
        archive.write(output / "dex/classes.dex", "classes.dex")
    run(build / "zipalign", "-f", "4", output / "unsigned.apk", output / "aligned.apk")
    run(java_home / "bin/keytool", "-genkeypair", "-keystore", output / "fixture.p12", "-storepass", "fixture-only",
        "-alias", "fixture", "-keyalg", "RSA", "-validity", "2", "-dname", "CN=Settings validation fixture", "-noprompt")
    run(build / "apksigner", "sign", "--ks", output / "fixture.p12", "--ks-pass", "pass:fixture-only",
        "--out", output / "fixture.apk", output / "aligned.apk")
    return output / "fixture.apk"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--adb", type=Path, required=True)
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--java-home", type=Path, required=True)
    parser.add_argument("--scenario", choices=("inspect", "smoke", "entries", "cold_entries", "rom_defaults", "preferred_fallback", "density", "display_controls", "night_unavailable", "sound_feedback", "notification_channels", "notification_unavailable", "notification_entries", "roles_unavailable", "default_roles", "roles_entries", "runtime_permissions", "permissions_legacy", "permissions_unavailable", "dnd_unavailable", "dnd_entries", "app_network", "app_network_unavailable", "app_battery", "app_battery_unavailable", "app_storage", "app_storage_unavailable", "vision_controls", "vision_unavailable", "app_language", "app_language_inspect", "app_language_unavailable", "hearing_controls", "hearing_unavailable", "caption_language", "caption_language_unavailable", "caption_custom", "caption_custom_unavailable", "text_interaction", "text_interaction_unavailable"), default="inspect",
                        help="smoke edits only a Search draft; entries restarts Home; density changes/restores an emulator display override; display_controls needs the documented Night Light fixture and restores display/Location settings")
    args = parser.parse_args()
    env = dict(os.environ, JAVA_HOME=str(args.java_home))

    def run(*command):
        return subprocess.run([str(part) for part in command], check=True, capture_output=True, text=True, env=env).stdout

    def adb(*command):
        return run(args.adb, "-s", args.serial, *command)

    def shell(*command):
        return adb("shell", shlex.join(str(part) for part in command))

    if shell("getprop", "ro.kernel.qemu").strip() != "1":
        raise SystemExit("This probe only runs on an Android emulator.")
    if shell("pm", "list", "packages", PACKAGE).strip():
        raise SystemExit("Fixture already installed; refusing to replace an existing package.")
    interaction = args.scenario == "text_interaction"
    interaction_package = "dev.makepad.octosense.interactionfixture"
    interaction_keys = {"secure": ("high_text_contrast_enabled", "font_weight_adjustment", "long_press_timeout",
        "accessibility_interactive_ui_timeout_ms", "accessibility_non_interactive_ui_timeout_ms",
        "accessibility_autoclick_enabled", "accessibility_autoclick_delay", "accessibility_large_pointer_icon"),
        "global": ("window_animation_scale", "transition_animation_scale", "animator_duration_scale")}
    if interaction:
        if args.serial != "emulator-5560" or "OctoSense_ROM_Roles_API35" not in adb("emu", "avd", "name"):
            raise SystemExit("Text/interaction changes require disposable emulator5560")
        if shell("pm", "list", "packages", interaction_package).strip():
            raise SystemExit("Existing interaction consumer; refusing replacement")
    if args.scenario == "text_interaction_unavailable" and args.serial != "emulator-5556":
        raise SystemExit("Ordinary interaction acceptance belongs to emulator5556")
    def interaction_rows():
        result = {}
        for table, keys in interaction_keys.items():
            rows = dict(line.split("=", 1) for line in shell("settings", "list", table).splitlines() if "=" in line)
            result[table] = {key: rows.get(key) for key in keys}
        return result
    interaction_before = interaction_rows() if interaction else None
    hearing = args.scenario in ("hearing_controls", "caption_language", "caption_custom")
    hearing_package = "dev.makepad.octosense.hearingfixture"
    hearing_keys = {"system": ("master_mono", "master_balance"), "secure": (
        "accessibility_captioning_enabled", "accessibility_captioning_font_scale", "accessibility_captioning_preset",
        "accessibility_captioning_locale", "accessibility_captioning_foreground_color", "accessibility_captioning_background_color",
        "accessibility_captioning_window_color", "accessibility_captioning_edge_type", "accessibility_captioning_edge_color", "accessibility_captioning_typeface")}
    def hearing_rows():
        result = {}
        for table, keys in hearing_keys.items():
            rows = dict(line.split("=", 1) for line in shell("settings", "list", table).splitlines() if "=" in line)
            result[table] = {key: rows.get(key) for key in keys}
        return result
    hearing_before = hearing_rows() if hearing else None
    def unrelated_audio_rows():
        system = dict(line.split("=", 1) for line in shell("settings", "list", "system").splitlines() if "=" in line)
        return {"volumes": {key: value for key, value in system.items() if key.startswith("volume_")},
                "dnd": shell("settings", "get", "global", "zen_mode")}
    hearing_audio_before = unrelated_audio_rows() if hearing else None
    vision = args.scenario == "vision_controls"
    if vision and (args.serial != "emulator-5560" or "OctoSense_ROM_Roles_API35" not in adb("emu", "avd", "name")):
        raise SystemExit("Color mutations require disposable emulator-5560")
    if args.scenario == "vision_unavailable" and args.serial != "emulator-5556":
        raise SystemExit("Ordinary color acceptance belongs to emulator-5556")
    vision_keys = ("accessibility_display_inversion_enabled", "accessibility_display_daltonizer_enabled", "accessibility_display_daltonizer")
    def vision_rows():
        rows = dict(line.split("=", 1) for line in shell("settings", "list", "secure").splitlines() if "=" in line)
        return {key: rows.get(key) for key in vision_keys}
    vision_before = vision_rows() if vision else None
    app_network = args.scenario == "app_network"
    language = args.scenario.startswith("app_language")
    language_package = "dev.makepad.octosense.languagefixture"
    language_apk_path = "/data/local/tmp/octosense-language-fixture.apk"
    storage = args.scenario == "app_storage"
    storage_packages = ("dev.makepad.octosense.storagereaderfixture", "dev.makepad.octosense.storagefixture", "dev.makepad.octosense.storagespacefixture")
    storage_apk_path = "/data/local/tmp/octosense-storage-fixture.apk"
    app_battery = args.scenario == "app_battery"
    network_package = "dev.makepad.octosense.networkfixture"
    battery_packages = (network_package, "dev.makepad.octosense.batterylegacyfixture",
                        "dev.makepad.octosense.batterysharedfixture", "dev.makepad.octosense.batterysharedpeer")
    def battery_baseline():
        result = {"allowlist": sorted(shell("cmd", "deviceidle", "whitelist").splitlines())}
        for op in ("RUN_ANY_IN_BACKGROUND", "RUN_IN_BACKGROUND"):
            for mode in ("allow", "ignore", "deny", "default", "foreground"):
                response = shell("cmd", "appops", "query-op", "--user", "0", op, mode).strip()
                lines = [line.strip() for line in response.splitlines() if line.strip() and line.strip() != "No operations."]
                if any(not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_.]*", line) for line in lines):
                    raise RuntimeError("Unexpected AppOps inventory response for " + op + "/" + mode)
                result[op + "/" + mode] = sorted(lines)
        return result
    battery_before = None
    if app_battery:
        if args.serial != "emulator-5560" or "OctoSense_ROM_Roles_API35" not in adb("emu", "avd", "name"):
            raise SystemExit("App battery mutations require disposable emulator-5560")
        for package in battery_packages:
            if shell("pm", "list", "packages", package).strip():
                raise SystemExit("Existing battery fixture; refusing replacement: " + package)
        battery_before = battery_baseline()
    def network_baseline():
        dump=shell("dumpsys","netpolicy")
        section=dump.split("Policy for UIDs:\n",1)[1].split("Power save whitelist",1)[0]
        return {"uids":dict(re.findall(r"UID=(\d+) policy=(\d+)",section)),
                "data_saver":shell("cmd","netpolicy","get","restrict-background").strip()}
    network_before=None
    if app_network:
        if args.serial != "emulator-5560" or "OctoSense_ROM_Roles_API35" not in adb("emu","avd","name"):
            raise SystemExit("App network mutations require disposable emulator-5560")
        if shell("pm","list","packages",network_package).strip():
            raise SystemExit("Existing network fixture; refusing replacement")
        network_before=network_baseline()
    notifications = args.scenario == "notification_channels"
    roles = args.scenario == "default_roles"
    permissions = args.scenario in ("runtime_permissions", "permissions_legacy")
    roles_before = None
    assistant_before = None
    grants_before = None
    def holders():
        return {name.strip(): sorted(re.findall(r"holders=([^\n]+)", body))
                for name, body in re.findall(r"\{\s*name=([^\n]+)\n(.*?)\n\s*\}", shell("dumpsys", "role"), re.S)}
    def permission_inventory():
        packages, package = {}, None
        for line in shell("dumpsys", "package", "packages").splitlines():
            if line.startswith("Hidden system packages:"):
                break  # Old system bases must not replace their active data updates.
            match = re.match(r"^\s*Package \[([^]]+)\]", line)
            if match:
                package = match[1]
                packages[package] = {}
            match = re.match(r"^\s*([A-Za-z0-9_.]+): granted=(true|false)(?:, flags=\[([^]]*)\])?", line)
            if match and package:
                packages[package][match[1]] = (match[2], match[3])
        if len(packages) < 50 or not packages.get("dev.makepad.octosense"):
            raise RuntimeError("Incomplete permission baseline; refusing mutation acceptance")
        return packages
    if roles:
        if "OctoSense_ROM_Roles_API35" not in adb("emu", "avd", "name"):
            raise SystemExit("Default-role mutations require the disposable role-validation clone.")
        for package in (*ROLE_BROWSERS, ROLE_ASSISTANT):
            if shell("pm", "list", "packages", package).strip():
                raise SystemExit("Role candidate already installed; refusing to replace it: " + package)
        shell("sh", "-c", "test ! -e " + ROLE_APK)
        roles_before = holders()
        if roles_before.get("android.app.role.BROWSER") != ["org.chromium.webview_shell"]:
            raise SystemExit("Unexpected browser baseline; no roles were changed.")
        if roles_before.get("android.app.role.ASSISTANT") != []:
            raise SystemExit("Assistant None acceptance requires the disposable clone's empty holder baseline.")
        assistant_before = {key: shell("settings", "get", "secure", key).strip()
                            for key in ("assistant", "voice_interaction_service", "voice_recognition_service")}
    if notifications:
        for package in (PUBLISHER, LEGACY_PUBLISHER):
            if shell("pm", "list", "packages", package).strip():
                raise SystemExit("Notification publisher already installed; refusing to replace an existing package: " + package)
    if permissions:
        if args.serial != "emulator-5560" or "OctoSense_ROM_Roles_API35" not in adb("emu", "avd", "name"):
            raise SystemExit("Runtime-permission mutations require the disposable emulator-5560 clone.")
        for package in PERMISSION_PACKAGES:
            if shell("pm", "list", "packages", package).strip():
                raise SystemExit("Permission fixture already installed; refusing to replace it: " + package)
        shell("sh", "-c", "test ! -e " + PERMISSION_APK)
        roles_before = holders()
        grants_before = permission_inventory()
    if storage:
        if args.serial != "emulator-5560" or "OctoSense_ROM_Roles_API35" not in adb("emu", "avd", "name"):
            raise SystemExit("Storage clearing requires only the disposable emulator-5560 clone")
        for package in storage_packages:
            if shell("pm", "list", "packages", package).strip():
                raise SystemExit("Existing storage fixture; refusing replacement: " + package)
        shell("sh", "-c", "test ! -e " + storage_apk_path)
        roles_before = holders()
        grants_before = permission_inventory()
    if language:
        required = "emulator-5556" if args.scenario == "app_language_unavailable" else "emulator-5560"
        if args.serial != required:
            raise SystemExit("Language scenario has a dedicated emulator: " + required)
        if shell("pm", "list", "packages", language_package).strip():
            raise SystemExit("Existing language fixture; refusing replacement")
        shell("sh", "-c", "test ! -e " + language_apk_path)
        language_system_before = shell("settings", "get", "system", "system_locales")
        roles_before = holders()
        grants_before = permission_inventory()
    if hearing:
        if args.serial != "emulator-5560" or "OctoSense_ROM_Roles_API35" not in adb("emu", "avd", "name"):
            raise SystemExit("Hearing changes require disposable emulator5560")
        if shell("pm", "list", "packages", hearing_package).strip():
            raise SystemExit("Existing hearing fixture; refusing replacement")
    if args.scenario in ("hearing_unavailable", "caption_language_unavailable", "caption_custom_unavailable") and args.serial != "emulator-5556":
        raise SystemExit("Ordinary hearing acceptance belongs to emulator5556")
    fixture = ROOT / "home/android/validation-fixtures/settings-accessibility"
    with tempfile.TemporaryDirectory(prefix="octosense-settings-a11y-") as temporary:
        output = Path(temporary)
        contract = ROOT / "home/android/contracts/src/main/java/dev/makepad/octosense/contracts"
        apk = build_fixture(fixture, output, run, args.java_home, args.sdk,
                            (contract / "SystemSettings.java", contract / "Protocol.java",
                             fixture.parent / "dnd-observer/DndNativeSnapshot.java"))
        publishers = []
        if notifications:
            publishers.append((PUBLISHER, build_fixture(fixture.parent / "notification-channels", output / "publisher",
                                                       run, args.java_home, args.sdk)))
            publishers.append((LEGACY_PUBLISHER, build_fixture(fixture.parent / "notification-legacy", output / "legacy",
                                                              run, args.java_home, args.sdk,
                                                              (fixture.parent / "notification-channels/NotificationFixture.java",))))
        installed = []
        pushed_role_apk = False
        pushed_permission_apk = False
        pushed_storage_apk = False
        pushed_language_apk = False
        try:
            if interaction:
                interaction_apk = build_fixture(fixture.parent / "text-interaction", output / "interaction", run, args.java_home, args.sdk)
                adb("install", "--no-incremental", interaction_apk)
                installed.append(interaction_package)
            if hearing:
                hearing_apk = build_fixture(fixture.parent / "hearing-captions", output / "hearing", run, args.java_home, args.sdk)
                adb("install", "--no-incremental", hearing_apk)
                installed.append(hearing_package)
            if language:
                language_apk = build_fixture(fixture.parent / "app-language", output / "language", run, args.java_home, args.sdk)
                adb("install", "--no-incremental", language_apk)
                installed.append(language_package)
                adb("push", language_apk, language_apk_path)
                pushed_language_apk = True
            if storage:
                for index, directory in enumerate(("app-storage-reader", "app-storage", "app-storage-space")):
                    extra = tuple(sorted((fixture.parent / "app-storage").glob("*.java"))) if index == 2 else ()
                    storage_apk = build_fixture(fixture.parent / directory, output / directory, run, args.java_home, args.sdk, extra)
                    adb("install", "--no-incremental", storage_apk)
                    installed.append(storage_packages[index])
                    if index == 1:
                        adb("push", storage_apk, storage_apk_path)
                        pushed_storage_apk = True
            if app_network or app_battery:
                network_apk=build_fixture(fixture.parent / "app-network",output / "network",run,args.java_home,args.sdk)
                adb("install","--no-incremental",network_apk)
                installed.append(network_package)
            if app_battery:
                shared_key = None
                for index, directory in enumerate(("app-battery-legacy", "app-battery-shared", "app-battery-shared-peer")):
                    built = output / directory
                    battery_apk = build_fixture(fixture.parent / directory, built, run, args.java_home, args.sdk)
                    if index == 1:
                        shared_key = built / "fixture.p12"
                    if index == 2:
                        # A real shared UID needs exactly the same disposable signer.
                        run(args.sdk / "build-tools/35.0.0/apksigner", "sign", "--ks", shared_key,
                            "--ks-pass", "pass:fixture-only", "--out", built / "shared.apk", built / "aligned.apk")
                        battery_apk = built / "shared.apk"
                    adb("install", "--no-incremental", battery_apk)
                    installed.append(battery_packages[index + 1])
                first = re.search(r"uid:(\d+)", shell("pm", "list", "packages", "-U", battery_packages[2]))
                peer = re.search(r"uid:(\d+)", shell("pm", "list", "packages", "-U", battery_packages[3]))
                if not first or not peer or first.group(1) != peer.group(1):
                    raise RuntimeError("Disposable shared-UID fixture identities differ")
            if permissions:
                for index, package in enumerate(PERMISSION_PACKAGES):
                    source = fixture.parent / ("runtime-permissions" if index == 0 else "runtime-permissions-legacy")
                    extra = () if index == 0 else tuple(sorted((fixture.parent / "runtime-permissions").glob("*.java")))
                    permission_apk = build_fixture(source, output / ("permissions" + str(index)), run, args.java_home, args.sdk, extra)
                    install_options = () if index == 0 else ("--bypass-low-target-sdk-block",)
                    adb("install", "--no-incremental", *install_options, permission_apk)
                    installed.append(package)
                    if index == 1:
                        adb("push", permission_apk, PERMISSION_APK)
                        pushed_permission_apk = True
            if roles:
                for index, package in enumerate(ROLE_BROWSERS):
                    source = fixture.parent / ("default-role-browser" if index == 0 else "default-role-browser-secondary")
                    extra = () if index == 0 else tuple(sorted((fixture.parent / "default-role-browser").glob("*.java")))
                    role_apk = build_fixture(source, output / ("browser" + str(index)), run, args.java_home, args.sdk, extra)
                    adb("install", "--no-incremental", role_apk)
                    installed.append(package)
                    if index == 1:
                        adb("push", role_apk, ROLE_APK)
                        pushed_role_apk = True
                assistant_apk = build_fixture(fixture.parent / "default-role-assistant", output / "assistant", run, args.java_home, args.sdk,
                                              tuple(sorted((fixture.parent / "default-role-browser").glob("*.java"))))
                adb("install", "--no-incremental", assistant_apk)
                installed.append(ROLE_ASSISTANT)
            for publisher, publisher_apk in publishers:
                adb("install", "--no-incremental", str(publisher_apk))
                installed.append(publisher)
                shell("pm", "grant", publisher, "android.permission.POST_NOTIFICATIONS")
                setup = shell("am", "instrument", "-w", "-e", "operation", "setup", publisher + "/" + PUBLISHER_COMPONENT)
                if "INSTRUMENTATION_RESULT: passed=true" not in setup:
                    raise SystemExit("Synthetic notification publisher setup failed: " + setup)
            adb("install", "--no-incremental", str(apk))
            installed.append(PACKAGE)
            report = shell("am", "instrument", "-w", "-e", "scenario", args.scenario, PACKAGE + "/.SettingsAccessibilityProbe")
            print(report.strip())
            if "INSTRUMENTATION_RESULT: passed=true" not in report:
                raise SystemExit("Settings accessibility acceptance failed.")
            if (roles or permissions or storage or language) and holders() != roles_before:
                raise RuntimeError("Default-role acceptance did not restore every role holder.")
        finally:
            failures = []
            if interaction:
                try:
                    current = interaction_rows()
                    for table, values in interaction_before.items():
                        for key, value in values.items():
                            if current[table][key] != value:
                                if value is None:
                                    shell("settings", "delete", table, key)
                                else:
                                    shell("settings", "put", table, key, value)
                    if interaction_rows() != interaction_before:
                        failures.append(("text/interaction", "exact raw restoration failed"))
                    else:
                        print("Restored all eleven exact text and interaction rows, including absent/empty values")
                except Exception as failure:
                    failures.append(("text/interaction restoration", failure))
            if hearing:
                try:
                    current = hearing_rows()
                    for table, values in hearing_before.items():
                        for key, value in values.items():
                            if current[table][key] != value:
                                if value is None:
                                    shell("settings", "delete", table, key)
                                else:
                                    shell("settings", "put", table, key, value)
                    if hearing_rows() != hearing_before:
                        failures.append(("hearing preferences", "exact raw restoration failed"))
                    if unrelated_audio_rows() != hearing_audio_before:
                        failures.append(("stream volume or DND", "unrelated setting changed"))
                    if not failures:
                        print("Restored all exact hearing/caption rows; volume and DND settings preserved")
                except Exception as failure:
                    failures.append(("hearing restoration", failure))
            if vision:
                # Also restore if Android kills instrumentation before Java's finally.
                try:
                    current = vision_rows()
                    for key, value in vision_before.items():
                        if current[key] != value:
                            if value is None:
                                shell("settings", "delete", "secure", key)
                            else:
                                shell("settings", "put", "secure", key, value)
                    if vision_rows() != vision_before:
                        failures.append(("color preferences", "exact raw restoration failed"))
                    else:
                        print("Restored all three exact color preference rows, including absent/empty values")
                except Exception as failure:
                    failures.append(("color preferences", failure))

            if (roles or permissions or storage or language) and holders() != roles_before:
                failures.append(("role restoration", "one or more holders differ before fixture cleanup"))
            for package in reversed(installed):
                try:
                    adb("uninstall", package)
                except Exception as failure:
                    failures.append((package, failure))
            if pushed_role_apk:
                try:
                    shell("rm", ROLE_APK)
                except Exception as failure:
                    failures.append((ROLE_APK, failure))
            if language and shell("settings", "get", "system", "system_locales") != language_system_before:
                failures.append(("system language", "unrelated device language changed"))
            if pushed_language_apk:
                try:
                    shell("rm", language_apk_path)
                except Exception as failure:
                    failures.append((language_apk_path, failure))
            if pushed_storage_apk:
                try:
                    shell("rm", storage_apk_path)
                except Exception as failure:
                    failures.append((storage_apk_path, failure))
            if pushed_permission_apk:
                try:
                    shell("rm", PERMISSION_APK)
                except Exception as failure:
                    failures.append((PERMISSION_APK, failure))
            if (roles or permissions or storage or language) and holders() != roles_before:
                failures.append(("role restoration", "one or more holders differ after fixture cleanup"))
            if permissions or storage or language:
                current = permission_inventory()
                changed = [package for package, grants in grants_before.items() if current.get(package) != grants]
                if changed:
                    failures.append(("unrelated permissions", "grant/flag records changed in " + ", ".join(changed)))
                else:
                    print("Preserved existing permission grant/flag records:", sum(map(len, grants_before.values())),
                          "across", len(grants_before), "packages")
            if roles and holders() == roles_before:
                # Native Assistant selection also owns legacy secure rows.
                # Restore exact raw presence only after the UI restored roles.
                for key, expected in assistant_before.items():
                    if shell("settings", "get", "secure", key).strip() != expected:
                        if expected == "null":
                            shell("settings", "delete", "secure", key)
                        else:
                            shell("settings", "put", "secure", key, expected)
                    if shell("settings", "get", "secure", key).strip() != expected:
                        failures.append((key, "legacy assistant preference restoration failed"))
            if app_battery:
                if battery_baseline() != battery_before:
                    failures.append(("battery policy", "original power allowlist or background AppOps changed"))
                else:
                    print("Preserved original power allowlist and both background AppOps inventories; all battery fixtures removed")
            if app_network:
                if network_baseline()!=network_before:
                    failures.append(("network policy","unrelated UID policy or global Data Saver changed"))
                else:
                    print("Preserved all original UID policies and global Data Saver; synthetic network fixture removed")
            if failures:
                raise RuntimeError("Fixture cleanup failed: " + "; ".join(package + ": " + str(error) for package, error in failures))


if __name__ == "__main__":
    main()
