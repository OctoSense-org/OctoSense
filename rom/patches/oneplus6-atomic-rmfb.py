#!/usr/bin/env python3
"""Apply the OnePlus 6 atomic framebuffer cleanup backport, idempotently."""
from pathlib import Path
import subprocess
import sys


def main():
    kernel = Path(sys.argv[1]) / "kernel/oneplus/sdm845"
    patch = Path(__file__).resolve().parent / "kernel/oneplus6-atomic-rmfb.patch"
    command = ["git", "-C", str(kernel), "apply"]
    check = subprocess.run(command + ["--check", str(patch)], capture_output=True)
    if check.returncode == 0:
        subprocess.run(command + [str(patch)], check=True)
        print("OnePlus 6 atomic framebuffer cleanup: patched")
    elif subprocess.run(command + ["--reverse", "--check", str(patch)],
                        capture_output=True).returncode == 0:
        print("OnePlus 6 atomic framebuffer cleanup: already patched")
    else:
        sys.exit("OnePlus kernel differs from the supported atomic RMFB patch; "
                 "review the source before building")


if __name__ == "__main__":
    main()
