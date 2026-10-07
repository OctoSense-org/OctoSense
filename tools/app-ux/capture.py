#!/usr/bin/env python3
"""Capture untouched native system-app screens with isolated, disposable state.

Phone mode is a desktop host at phone dimensions, not a physical-device result.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import subprocess
import time
from urllib.error import HTTPError

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/connected-e2e"))
from native import Native


class AppNative(Native):
    def call(self, route, **query):
        if route == "m" and query.get("k") == "scroll":
            # Deterministic trackpad deltas, not a smoothed wheel animation
            # whose first subsequent press is consumed catching its motion.
            query.setdefault("precise", 1)
        try:
            return super().call(route, **query)
        except HTTPError as error:
            # The native instrument uses 404 for explicit failures too (for
            # example an input/frame timeout). Preserve that reason in the
            # evidence instead of reducing every failure to "Not Found".
            detail = error.read().decode('utf-8', errors='replace')
            if detail:
                error.msg = detail
            raise

    def rows(self):
        try:
            return super().rows()
        except HTTPError as error:
            if error.code == 404 and self.child.poll() is None:
                return []  # Instrument ready, first native window not ready yet.
            raise

    def capture(self, name):
        # A grab may arrive between window creation and its first present.
        # Retry that explicit 404, never a script error or failed assertion.
        for attempt in range(15):
            try:
                return super().capture(name)
            except HTTPError as error:
                if error.code != 404 or attempt == 14:
                    raise
                time.sleep(.2)

    def close(self):
        try:
            super().close()
        except subprocess.TimeoutExpired:
            self.child.kill()
            self.child.wait(timeout=5)
            self.log.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/octosense")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--apps", nargs="+", default=["calendar", "mail", "news", "photos", "ai-providers", "apphub", "youtube"])
    parser.add_argument("--mode", choices=["desktop", "phone"], default="phone")
    parser.add_argument("--dark", action="store_true")
    parser.add_argument("--settle-seconds", type=float, default=2,
                        help="Additional resource-loading time before each capture")
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    # A camera bundle is phone-only even in the desktop phone preview.
    desktop_apps = json.loads((ROOT / 'desktop/system-apps.json').read_text())['apps']
    unsupported = set(args.apps) - set(desktop_apps) - {'apphub'}
    if unsupported:
        parser.error('Not packaged in this desktop test binary: ' + ', '.join(sorted(unsupported)))
    results = []
    for app in args.apps:
        name = app + "-" + args.mode + ("-dark" if args.dark else "-light")
        state = out / (name + "-state")
        os.environ.update(OCTOSENSE_HOME=str(state / "home"), OCTOS_APP_CORE_DIR=str(state / "core"),
                          OCTOSENSE_APP_DATA=str(state / "apps"), OCTOSENSE_LLM_VAULT="file",
                          MAKEPAD_APP_CONFIG=json.dumps({"mail_demo": True}))
        actions = ["--test-action", "phone:ios"] if args.mode == "phone" else []
        actions += ["--test-action", "launch-" + app]
        instance = AppNative(args.binary.resolve(), actions, out, name)
        try:
            instance.wait(lambda: len(instance.rows()) > 4, timeout=45)
            time.sleep(args.settle_seconds)
            if args.dark:
                # Appearance controls in the phone preview and the desktop
                # shell bar. Desktop submits once, then captures a new frame.
                if args.mode == "phone":
                    instance.call("click", x=332, y=16, wait=1)
                else:
                    instance.call("click", x=252, y=16)
                    instance.capture(name + "-appearance-transition")
                time.sleep(args.settle_seconds)  # Map tile styles rebake asynchronously after restyle.
            instance.capture(name)
            instance.check_logs()
            results.append({"screen": name, "status": "captured", "native": True,
                            "physical_mobile": False, "sha256": hashlib.sha256((out / (name + ".png")).read_bytes()).hexdigest()})
        except Exception as error:
            results.append({"screen": name, "status": "failed", "error": str(error)})
        finally:
            try:
                instance.close()
            except subprocess.TimeoutExpired:
                instance.child.kill()
                instance.child.wait(timeout=5)
                instance.log.close()
        print(json.dumps(results[-1]), flush=True)
    receipt = out / (args.mode + ("-dark" if args.dark else "-light") + ".json")
    receipt.write_text(json.dumps({"binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(), "screens": results}, indent=2) + "\n")
    if any(r["status"] == "failed" for r in results):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
