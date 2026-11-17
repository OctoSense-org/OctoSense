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


def paste_web_views(grab, shots, window_width):
    """A GPU grab can't hold a native web view: paste its own picture in place, as much
    of it as a person sees. What is in front of a web view is in the grab; `mask` says the
    page leaves room for it. Pasted whole, a page would hide that, or fake it."""
    from PIL import Image, ImageChops  # Only when a web view is showing.
    picture = Image.open(grab).convert('RGB')
    scale = picture.width / window_width
    for shot in shots:
        x, y, width, height = shot['r']
        if not Path(shot['png']).exists() or shot['size'][0] <= 0 or shot['size'][1] <= 0:
            continue
        web = Image.open(shot['png']).convert('RGBA')
        sx, sy = web.width / shot['size'][0], web.height / shot['size'][1]
        left, top = shot['from']
        web = web.crop((round(left * sx), round(top * sy), round((left + width) * sx), round((top + height) * sy)))
        web = web.resize((round(width * scale), round(height * scale)), Image.LANCZOS)
        if shot.get('mask'):
            seen = Image.open(shot['mask']).convert('L').resize(web.size, Image.LANCZOS)
            web.putalpha(ImageChops.multiply(web.getchannel('A'), seen))
        picture.paste(web, (round(x * scale), round(y * scale)), web)
    picture.save(grab)


class AppNative(Native):
    def web_shots(self):
        """The native web views showing now, after their pictures have arrived."""
        try:
            folder = Path(super().call('web')['dir'])
        except (HTTPError, RuntimeError, KeyError):
            return []  # A binary without the route.
        deadline = time.monotonic() + 6
        shots = None
        while time.monotonic() < deadline:
            if shots is None and (folder / 'shots.json').exists():
                shots = json.loads((folder / 'shots.json').read_text())
            if shots is not None and all(Path(shot['png']).exists() for shot in shots):
                break
            time.sleep(.05)
        return shots or []

    def call(self, route, **query):
        if route == 'g':
            grabbed = self.grab(**query)
            shots = self.web_shots()
            if shots and grabbed.get('png'):
                paste_web_views(grabbed['png'], shots, super().call('s')['w'][0]['sz'][0])
            return grabbed
        return self.call_retrying(route, **query)

    def grab(self, **query):
        return self.call_retrying('g', **query)

    def call_retrying(self, route, **query):
        if route == "m" and query.get("k") == "scroll":
            # Deterministic trackpad deltas, not a smoothed wheel animation
            # whose first subsequent press is consumed catching its motion.
            query.setdefault("precise", 1)
        for attempt in range(15):
            try:
                return super().call(route, **query)
            except HTTPError as error:
                # The native instrument uses 404 for explicit failures too (for
                # example an input/frame timeout). Preserve that reason in the
                # evidence instead of reducing every failure to "Not Found".
                detail = error.read().decode('utf-8', errors='replace')
                # The input was applied; only the wait for its frame failed on a
                # loaded machine. A retry would press twice, so carry on.
                if 'requested input frame could not be submitted' in detail:
                    time.sleep(.3)
                    return {}
                # A grab has no effect, and the instrument asks for this retry.
                if route == 'g' and 'retry' in detail and attempt < 14:
                    time.sleep(.2)
                    continue
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
        # The desktop's own appearance by name: the bar's control moved when a
        # Mac's menu bar arrived. Each desktop starts in its own style.
        desktop_dark = "macos-dark" if sys.platform == "darwin" else "octosense-dark"
        actions = ["--test-action", "phone:ios"] if args.mode == "phone" else []
        if args.mode == "desktop" and args.dark:
            actions += ["--test-action", "style:" + desktop_dark]
        actions += ["--test-action", "launch-" + app]
        instance = AppNative(args.binary.resolve(), actions, out, name)
        try:
            instance.wait(lambda: len(instance.rows()) > 4, timeout=45)
            time.sleep(args.settle_seconds)
            if args.dark:
                # The phone preview's appearance control; a desktop starts dark.
                if args.mode == "phone":
                    instance.call("click", x=332, y=16, wait=1)
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
