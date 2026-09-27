#!/usr/bin/env python3
"""Prepare Home's pinned framework sources: tools/setup.py at the repository root.

Kept so the ROM scripts and habits keep working; `--cargo` checks the
workspace's locked graph (one Makepad, App Hub, octos and Rinx).
"""
from pathlib import Path
import subprocess
import sys

REPO = Path(__file__).resolve().parents[2]

if __name__ == "__main__":
    sys.exit(subprocess.run([sys.executable, str(REPO / "tools/setup.py"), *sys.argv[1:]]).returncode)
