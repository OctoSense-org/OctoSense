#!/usr/bin/env python3
"""LineageOS installs TrebuchetQuickStep unconditionally (vendor/lineage/config/
common_mobile.mk). The OctoSense ROM ships its own Home and its Quickstep fork
for Recents, so the upstream launcher package is left out; the Trebuchet source
tree stays (OctoSenseQuickstep is built from it). Idempotent. Usage: <tree>"""
import sys
from pathlib import Path
path = Path(sys.argv[1]) / "vendor/lineage/config/common_mobile.mk"
text = path.read_text()
old = """else
PRODUCT_PACKAGES += \\
    TrebuchetQuickStep

PRODUCT_DEXPREOPT_SPEED_APPS += \\
    TrebuchetQuickStep
endif"""
new = """else ifneq ($(OCTOSENSE_NO_TREBUCHET),true)
PRODUCT_PACKAGES += \\
    TrebuchetQuickStep

PRODUCT_DEXPREOPT_SPEED_APPS += \\
    TrebuchetQuickStep
endif"""
if new in text:
    print("trebuchet: already patched")
elif text.count(old) == 1:
    path.write_text(text.replace(old, new))
    print("trebuchet: patched")
else:
    sys.exit("common_mobile.mk differs from the pinned LineageOS source")
