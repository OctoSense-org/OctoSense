#!/usr/bin/env python3
"""LineageOS leaves KERNEL_BUILD_OUT_PREFIX empty for a relative OUT_DIR other
than plain `out`, so `headers_install` writes the kernel UAPI headers into the
kernel source dir and the qcom display HAL fails on media/msm_media_info.h.
Any relative OUT_DIR gets the prefix. Idempotent. Usage: <tree>"""
import sys
from pathlib import Path
path = Path(sys.argv[1]) / "vendor/lineage/config/BoardConfigKernel.mk"
text = path.read_text()
old = """OUT_DIR_PREFIX := $(shell echo $(OUT_DIR) | sed -e 's|/target/.*$$||g')
KERNEL_BUILD_OUT_PREFIX :=
ifeq ($(OUT_DIR_PREFIX),out)
    KERNEL_BUILD_OUT_PREFIX := $(BUILD_TOP)/
endif"""
new = """# OctoSense: any relative OUT_DIR (out/octosense-rom included) needs the prefix.
KERNEL_BUILD_OUT_PREFIX :=
ifeq ($(filter /%,$(OUT_DIR)),)
    KERNEL_BUILD_OUT_PREFIX := $(BUILD_TOP)/
endif"""
if new in text:
    print("kernel out prefix: already patched")
elif text.count(old) == 1:
    path.write_text(text.replace(old, new))
    print("kernel out prefix: patched")
else:
    sys.exit("BoardConfigKernel.mk differs from the pinned LineageOS source")
