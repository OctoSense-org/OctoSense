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
# The layer's presence decides: octosense.mk is inherited after this file is parsed,
# so a variable set there would come too late for make's ifneq.
stale2 = """else ifeq ($(wildcard vendor/octosense/octosense.mk),)
PRODUCT_PACKAGES += \\
    TrebuchetQuickStep

PRODUCT_DEXPREOPT_SPEED_APPS += \\
    TrebuchetQuickStep
endif"""
stale = """else ifneq ($(OCTOSENSE_NO_TREBUCHET),true)
PRODUCT_PACKAGES += \\
    TrebuchetQuickStep

PRODUCT_DEXPREOPT_SPEED_APPS += \\
    TrebuchetQuickStep
endif"""
# Unconditional: this tree is the OctoSense ROM's tree, and a wildcard test here
# proved unreliable (the package was still installed).
new = """else
# OctoSense ROM: TrebuchetQuickStep is not installed (patches/lineage-no-trebuchet.py).
endif"""
if new in text:
    print("trebuchet: already patched")
elif stale in text or stale2 in text:
    path.write_text(text.replace(stale, new).replace(stale2, new))
    print("trebuchet: repatched")
elif text.count(old) == 1:
    path.write_text(text.replace(old, new))
    print("trebuchet: patched")
else:
    sys.exit("common_mobile.mk differs from the pinned LineageOS source")
