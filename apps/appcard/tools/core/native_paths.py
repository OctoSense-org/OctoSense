"""Shared native repositories live outside AppCards, in the organization workspace.

AppCard lives at apps/appcard/ in OctoSense-System-Apps, so the workspace that
holds the sibling checkouts (makepad, octoscript, octoscript-makepad) is the
parent of the repository root: two levels above APPCARDS.
"""
from pathlib import Path
import os

APPCARDS = Path(__file__).resolve().parents[2]  # apps/appcard
REPOSITORY = APPCARDS.parents[1]  # OctoSense-System-Apps
WORKSPACE = Path(os.environ.get(
    "OCTOSENSE_WORKSPACE",
    REPOSITORY.parent,
)).resolve()
# The octos kernel is not carried in this repository: app/Cargo.toml takes every
# octos crate from git at one rev. Tools that build the kernel binary (Android,
# OHOS, macOS runners) use a checkout of that rev, by default beside the
# repository (OCTOS_SOURCE selects another).
OCTOS = Path(os.environ.get("OCTOS_SOURCE", WORKSPACE / "octos")).resolve()


def octos_revision():
    """The one octos rev app/Cargo.toml pins (octos-core and octos-cli share it)."""
    import re
    manifest = (APPCARDS / "app/Cargo.toml").read_text()
    revs = set(re.findall(r'git = "https://github\.com/octos-org/octos\.git", rev = "([0-9a-f]{40})"', manifest))
    if len(revs) != 1:
        raise RuntimeError(f"app/Cargo.toml must pin exactly one octos rev, found {sorted(revs)}")
    return revs.pop()


DIRECTORIES = {"makepad": "makepad", "splash": "octoscript", "splash-makepad": "octoscript-makepad"}


def repository(name, workspace=None):
    return Path(workspace or os.environ.get("OCTOS_APPCARD_NATIVE_ROOT", WORKSPACE)) / DIRECTORIES.get(name, name)


def adapt_cargo_paths(repo):
    """Adapt the pinned upstream sibling directory name; leave Rust crate names intact."""
    import subprocess
    names = subprocess.check_output(["git", "-C", str(repo), "ls-files", "--", "*Cargo.toml"], text=True).splitlines()
    for name in names:
        path = Path(repo) / name
        source = path.read_text()
        updated = cargo_paths(source)
        if updated != source:
            path.write_text(updated)


def cargo_paths(source):
    return source.replace("../../../splash/crates/", "../../../octoscript/crates/")
