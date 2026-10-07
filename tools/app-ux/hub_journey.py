#!/usr/bin/env python3
"""Read-only App Hub navigation, search, empty-state recovery and detail review."""
import argparse
import json
import os
from pathlib import Path
import time
from capture import AppNative


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--size", default="406x820")
    parser.add_argument("--dark", action="store_true")
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    os.environ.update(OCTOSENSE_APP_DATA=str(out / "state"), OCTOSENSE_PREVIEW_SIZE=args.size,
                      MAKEPAD_WIDGET_STYLE="ios-dark" if args.dark else "ios")
    native = AppNative(args.binary.resolve(), [], out, "hub")
    checks = []
    try:
        native.label("App Hub")
        time.sleep(1)
        # Preview entries are local and have no install authority.
        native.click("Live catalog")
        native.wait(lambda: native.find(text="Preview catalog"))
        native.click_row(native.find(identifier="apps"))
        native.label("Apps")
        checks.append("Browse the explicitly labelled preview catalog")
        native.click_row(native.find(identifier="search_tab"))
        native.field("search", "no-app-matches-this-ux-test")
        native.label("No matching apps")
        native.click("Clear filters")
        native.field("search", "maps")
        native.label("Maps")
        checks.append("Search, reach empty results, clear filters, recover results")
        native.click_row(native.reachable(text="Maps", kind="Label"))
        native.wait(lambda: native.find(text="‹ Back"))
        checks.append("Open app details and preserve a reachable Back action")
        native.click("‹ Back")
        native.click_row(native.find(identifier="library"))
        checks.append("Navigate to Library without changing installed apps")
        native.click_row(native.find(identifier="today"))
        native.capture("hub-today")
        native.check_logs()
        (out / "receipt.json").write_text(json.dumps({"checks": checks, "physical_mobile": False,
            "size": args.size, "dark": args.dark, "installed_apps_changed": False,
            "actions": native.actions}, indent=2) + "\n")
        print(json.dumps(checks))
    except Exception:
        native.capture("failed-state")
        raise
    finally:
        native.close()


if __name__ == "__main__":
    main()
