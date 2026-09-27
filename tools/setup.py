#!/usr/bin/env python3
"""Prepare OctoSense's pinned framework sources (Python 3.9+).

The workspace (Cargo.toml at the repository root: desktop/, phone/, crates/,
apps/) resolves Makepad and OctoScript from local checkouts in `.sources/`
at the repository root (git-ignored):

  .sources/octoscript-makepad/  the release native-runtime.lock.json selects
  .sources/makepad/             at the revision that release's runtime.json
                                pins, plus the reviewed patch
                                runtime-patches.lock.json names
  .sources/octoscript/          at the revision runtime.json pins

Everything else external (App Hub, octos, Rinx) is a git dependency pinned
once in the root Cargo.toml [workspace.dependencies]. Local changes are
preserved; --update only moves clean checkouts; --check changes nothing.
`--cargo` also checks the locked Cargo graph: one Makepad (from .sources),
one App Hub, one octos.

  python3 tools/setup.py                 # prepare
  python3 tools/setup.py --check --cargo # verify (CI)
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

# The repository root: the locks and the reviewed patches live here.
PRODUCT = Path(__file__).resolve().parents[1]
CONSUMER = PRODUCT
URL = "https://github.com/OctoSense-org/Octoscript-Makepad.git"
RUNTIME_URLS = {
    "makepad": "https://github.com/OctoSense-org/makepad.git",
    "octoscript": "https://github.com/OctoSense-org/Octoscript.git",
}
# Crates whose sources must be unique in every graph, and where they come from.
MAKEPAD_CRITICAL = {"makepad-script", "makepad-platform", "makepad-draw", "makepad-widgets", "makepad-live-id"}
SINGLE_SOURCE = {"octos-core": "octos", "octos-cli": "octos", "octosense-appstore": "App Hub",
                 "octosense-app-hub-app": "App Hub", "rinx": "Rinx"}


def git(path, *args, check=True):
    result = subprocess.run(["git", "-C", str(path), *args], capture_output=True, text=True)
    if check and result.returncode:
        raise RuntimeError(result.stderr.strip() or f"git {' '.join(args)} failed in {path}")
    return result


def prepare_source(root, name, spec, args, overlay=None):
    """Check out `name` at `spec["revision"]`, plus the reviewed `overlay` patches."""
    path = root / name
    if not re.fullmatch(r"[0-9a-f]{40}", spec.get("revision", "")):
        raise RuntimeError(f"Unpinned source: {name}")
    if not (path / ".git").exists():
        if args.check or path.exists() and any(path.iterdir()):
            raise RuntimeError(f"Expected an empty dependency directory: {path}")
        path.mkdir(parents=True, exist_ok=True)
        git(path, "init", "--quiet")
        git(path, "remote", "add", "origin", spec["url"])
    if Path(git(path, "rev-parse", "--show-toplevel").stdout.strip()).resolve() != path.resolve():
        raise RuntimeError(f"Not a dependency checkout: {path}")
    current = git(path, "rev-parse", "--verify", "HEAD", check=False).stdout.strip()
    if current and (git(path, "diff", "--quiet", check=False).returncode or git(path, "ls-files", "--others", "--exclude-standard").stdout):
        raise RuntimeError(f"Preserving local changes: {path}")
    tree = git(path, "write-tree").stdout.strip()
    base_tree = git(path, "rev-parse", "HEAD^{tree}", check=False).stdout.strip()
    patches = []
    if overlay:
        if overlay["base_revision"] != spec["revision"]:
            raise RuntimeError("Runtime patch does not match its source lock")
        # The patch, then any stacked on it (each a reviewed PR not yet merged
        # into the runtime), in order; `tree` is the tree after the last one.
        for entry in [overlay, *overlay.get("stacked", [])]:
            patch = (PRODUCT / entry["patch"]).resolve()
            if not patch.is_relative_to(PRODUCT) or hashlib.sha256(patch.read_bytes()).hexdigest() != entry["sha256"]:
                raise RuntimeError(f"Runtime patch does not match its source lock: {entry['patch']}")
            patches.append(patch)
    if current == spec["revision"] and overlay and tree == overlay["tree"]:
        return
    if current == spec["revision"] and not overlay and tree == base_tree:
        return
    if current and tree != base_tree:
        raise RuntimeError(f"Preserving staged changes: {path}")
    if current != spec["revision"]:
        if args.check or current and not args.update:
            raise RuntimeError(f"{path} selects another revision; --update only changes clean checkouts")
        if git(path, "cat-file", "-e", spec["revision"] + "^{commit}", check=False).returncode:
            cached = args.cache.resolve() / name if args.cache else None
            fetched = False
            if cached and (cached / ".git").exists() and not git(cached, "cat-file", "-e", spec["revision"] + "^{commit}", check=False).returncode:
                fetched = not git(path, "fetch", "--quiet", "--no-tags", "--depth=1", str(cached), spec["revision"], check=False).returncode
            if not fetched:
                git(path, "fetch", "--quiet", "--no-tags", "--depth=1", "origin", spec["revision"])
        git(path, "checkout", "--quiet", "--detach", spec["revision"])
    if overlay:
        if args.check:
            raise RuntimeError(f"Reviewed runtime patch is not applied in {path}; run setup without --check")
        for patch in patches:
            git(path, "apply", "--check", str(patch))
            git(path, "apply", "--index", str(patch))
        if git(path, "write-tree").stdout.strip() != overlay["tree"]:
            raise RuntimeError(f"Runtime patch produced an unexpected tree: {path}")


def verify_consumer(expected):
    """Reject manifests that pin a runtime source at another revision."""
    skip = {".git", ".sources", "target", "node_modules", "build", ".gradle"}
    for directory, subdirs, files in os.walk(CONSUMER):
        subdirs[:] = [n for n in subdirs if n not in skip]
        if "Cargo.toml" not in files:
            continue
        path = Path(directory) / "Cargo.toml"
        for line in path.read_text().splitlines():
            if line.lstrip().startswith("#"):
                continue
            url = re.search(r'git\s*=\s*"([^"]+)"', line)
            if url and url[1] in expected:
                rev = re.search(r'rev\s*=\s*"([0-9a-f]{40})"', line)
                if not rev or rev[1] != expected[url[1]]:
                    raise RuntimeError(f"Divergent runtime dependency: {path}: {line}")


def verify_cargo(root):
    """One Makepad (the prepared checkout), one App Hub, one octos, one Rinx."""
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--all-features"], cwd=CONSUMER, text=True))
    seen = set()
    sources = {}
    for package in metadata["packages"]:
        name = package["name"]
        if name in MAKEPAD_CRITICAL:
            if name in seen or not Path(package["manifest_path"]).resolve().is_relative_to(root / "makepad"):
                raise RuntimeError(f"Duplicate or foreign runtime crate: {name} ({package['manifest_path']})")
            seen.add(name)
        if name in SINGLE_SOURCE:
            sources.setdefault(SINGLE_SOURCE[name], set()).add((package["source"] or "").split("#")[0])
    if seen != MAKEPAD_CRITICAL:
        raise RuntimeError(f"Incomplete Makepad dependency graph: missing {sorted(MAKEPAD_CRITICAL - seen)}")
    for what, found in sources.items():
        if len(found) > 1:
            raise RuntimeError(f"More than one {what} source: {sorted(found)}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--root", type=Path, default=PRODUCT / ".sources",
                        help="Where the framework checkouts live (default and what Cargo.toml's [patch] names: .sources)")
    parser.add_argument("--update", action="store_true", help="Move clean checkouts to the locked revisions")
    parser.add_argument("--check", action="store_true", help="Verify without changing anything")
    parser.add_argument("--cache", type=Path, help="Optional directory of Git object caches (<cache>/makepad, ...)")
    parser.add_argument("--cargo", action="store_true", help="Also check the locked Cargo graph")
    args = parser.parse_args()
    root = args.root.resolve()
    lock = json.loads((PRODUCT / "native-runtime.lock.json").read_text())
    if lock.get("schema_version") != 1 or lock.get("url") != URL:
        parser.error("Unsupported runtime lock")
    patches = json.loads((PRODUCT / "runtime-patches.lock.json").read_text())
    if patches.get("schema_version") != 1 or not set(patches) <= {"schema_version", "makepad"}:
        parser.error("Unsupported runtime patch lock")
    prepare_source(root, "octoscript-makepad", lock, args)
    manifest = json.loads((root / "octoscript-makepad/runtime.json").read_text())
    if manifest.get("schema_version") != 1 or set(manifest.get("repositories", {})) != set(RUNTIME_URLS):
        parser.error("Unsupported runtime source set")
    expected = {URL: lock["revision"]}
    for name, spec in manifest["repositories"].items():
        if spec.get("url") != RUNTIME_URLS[name]:
            parser.error(f"Unexpected runtime source: {name}")
        prepare_source(root, name, spec, args, patches.get(name))
        expected[spec["url"]] = spec["revision"]
    verify_consumer(expected)
    if args.cargo:
        verify_cargo(root)
    print(json.dumps({"runtime": lock, "repositories": manifest["repositories"], "patches": patches}, indent=2))


if __name__ == "__main__":
    main()
