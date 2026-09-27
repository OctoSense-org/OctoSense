#!/usr/bin/env python3
"""build/make/target/product/handheld_system_ext.mk installs the Launcher3QuickStep
phony target, which Trebuchet's Android.bp makes `required: TrebuchetQuickStep`.
The OctoSense ROM ships its own Home, so the phony is dropped. Idempotent. Usage: <tree>"""
import sys
from pathlib import Path
path = Path(sys.argv[1]) / "build/make/target/product/handheld_system_ext.mk"
text = path.read_text()
old = "    Launcher3QuickStep \\\n"
new = "    $(if $(wildcard vendor/octosense/octosense.mk),,Launcher3QuickStep) \\\n"
if new in text:
    print("launcher3 phony: already patched")
elif text.count(old) == 1:
    path.write_text(text.replace(old, new))
    print("launcher3 phony: patched")
else:
    sys.exit("handheld_system_ext.mk differs from the pinned AOSP source")
