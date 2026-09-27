#!/usr/bin/env python3
"""Exercise the built-in ordered language UI and independent native configuration consumer."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
PACKAGE = "dev.makepad.octosense.systemlanguagesfixture"
REMOTE = "/data/local/tmp/octosense-system-language-observer.apk"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--adb", type=Path, required=True)
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--java-home", type=Path, required=True)
    parser.add_argument("--scenario", choices=("native", "unavailable"), required=True)
    parser.add_argument("--receipt", type=Path, required=True)
    parser.add_argument("--build-only", action="store_true")
    args = parser.parse_args()
    env = dict(os.environ, JAVA_HOME=str(args.java_home))

    def run(*command, timeout=600):
        return subprocess.run([str(part) for part in command], env=env, check=True,
                              capture_output=True, text=True, timeout=timeout).stdout

    def adb(*command):
        return run(args.adb, "-s", args.serial, *command)

    def shell(*command):
        return adb("shell", shlex.join(str(part) for part in command))

    builder_path = ROOT / "home/android/scripts/run-settings-accessibility-probe.py"
    spec = importlib.util.spec_from_file_location("settings_fixture_builder", builder_path)
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    receipt = {"serial": args.serial, "scenario": args.scenario, "passed": False}
    with tempfile.TemporaryDirectory(prefix="octosense-system-languages-") as temp:
        apk = builder.build_fixture(ROOT / "home/android/validation-fixtures/system-languages",
                                    Path(temp), run, args.java_home, args.sdk)
        if args.build_only:
            receipt.update(build_passed=True)
            args.receipt.parent.mkdir(parents=True, exist_ok=True)
            args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
            print("System language consumer and probe APK compile passed")
            return
        expected = "emulator-5560" if args.scenario == "native" else "emulator-5556"
        if args.serial != expected or shell("getprop", "ro.kernel.qemu").strip() != "1":
            raise SystemExit("This scenario is restricted to the assigned validation emulator")
        if args.scenario == "native" and "OctoSense_ROM_Roles_API35" not in adb("emu", "avd", "name"):
            raise SystemExit("Native changes require the disposable roles-validation clone")
        if shell("pm", "list", "packages", PACKAGE).strip():
            raise SystemExit("Existing fixture package; refusing replacement")
        shell("test", "!", "-e", REMOTE)

        def rows():
            values = dict(line.split("=", 1) for line in shell("settings", "list", "system").splitlines() if "=" in line)
            return {key: values.get(key) for key in ("system_locales", "locale_preferences")}

        def configuration():
            output = shell("env", "CLASSPATH=" + REMOTE, "app_process", "/system/bin",
                           PACKAGE + ".ConfigurationObservation")
            return json.loads(output[output.index("{"):])

        installed = pushed = False
        failure = None
        try:
            adb("push", apk, REMOTE)
            pushed = True
            receipt["before_configuration"] = configuration()
            receipt["before_raw_rows"] = rows()
            receipt["before_crash"] = shell("logcat", "-b", "crash", "-d")
            adb("install", "--user", "0", apk)
            installed = True
            result = shell("am", "instrument", "-w", "-r", "-e", "scenario", args.scenario,
                           PACKAGE + "/.SystemLanguagesProbe")
            receipt["instrumentation"] = result
            print(result, end="", flush=True)
            if "INSTRUMENTATION_RESULT: passed=true" not in result or "INSTRUMENTATION_CODE: 0" not in result:
                raise RuntimeError("System language UI probe failed; see its exact phase/result")
        except BaseException as error:
            failure = error
            receipt["failure"] = str(error)
        finally:
            try:
                if "before_configuration" in receipt:
                    after = configuration()
                    receipt["after_ui_configuration"] = after
                    receipt["after_ui_raw_rows"] = rows()
                    same_locales = after["locales"] == receipt["before_configuration"]["locales"]
                    receipt["ordered_configuration_restored"] = same_locales
                    if not same_locales:
                        raise RuntimeError("UI cleanup did not restore authoritative locale order; no invented restoration write")
                    # Native LocalePicker always writes a locale row. Restore the exact absent/empty
                    # provider baseline only after native ordered configuration is already restored.
                    if args.scenario == "native":
                        for key, value in receipt["before_raw_rows"].items():
                            if rows()[key] != value:
                                if value is None:
                                    shell("settings", "delete", "system", key)
                                else:
                                    shell("settings", "put", "system", key, value)
                    receipt["after_configuration"] = configuration()
                    receipt["after_raw_rows"] = rows()
                    if receipt["after_configuration"]["locales"] != receipt["before_configuration"]["locales"]:
                        raise RuntimeError("Provider cleanup altered authoritative configuration")
                    if receipt["after_raw_rows"] != receipt["before_raw_rows"]:
                        raise RuntimeError("Exact locale provider baseline was not restored")
                    receipt["user_set_locale_preserved"] = receipt["after_configuration"]["user_set_locale"] == receipt["before_configuration"]["user_set_locale"]
                    receipt["metadata_note"] = "Native LocalePicker sets userSetLocale=true; this probe never resets it through a hidden setter. Ordered locales and raw provider restoration are reported separately."
            except BaseException as cleanup:
                receipt["cleanup_failure"] = str(cleanup)
                if failure is None:
                    failure = cleanup
            finally:
                if installed:
                    receipt["fixture_uninstall"] = adb("uninstall", PACKAGE).strip()
                if pushed:
                    shell("rm", "-f", REMOTE)
                receipt["fixture_removed"] = not shell("pm", "list", "packages", PACKAGE).strip()
                receipt["after_crash"] = shell("logcat", "-b", "crash", "-d")
                receipt["passed"] = failure is None
                args.receipt.parent.mkdir(parents=True, exist_ok=True)
                args.receipt.write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + "\n")
        if failure is not None:
            raise failure


if __name__ == "__main__":
    main()
