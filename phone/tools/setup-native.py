#!/usr/bin/env python3
"""Prepare Home's pinned runtime and its reviewed product patches (Python 3.9+)."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

CONSUMER = Path(__file__).resolve().parents[1]
PRODUCT = CONSUMER.parent
URL = "https://github.com/OctoSense-org/Octoscript-Makepad.git"


def git(path, *args, check=True):
    result = subprocess.run(["git", "-C", str(path), *args], capture_output=True, text=True)
    if check and result.returncode:
        raise RuntimeError(result.stderr.strip())
    return result


def prepare_source(root, name, spec, args, overlay=None):
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
    for directory, subdirs, files in os.walk(CONSUMER):
        subdirs[:] = [n for n in subdirs if n not in {".git", "target", "node_modules", "build", ".gradle"}]
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=PRODUCT / ".sources")
    parser.add_argument("--update", action="store_true")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--cache", type=Path)
    parser.add_argument("--cargo-manifest", type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    if root == CONSUMER or root.is_relative_to(CONSUMER):
        parser.error("Keep framework checkouts outside home/")
    lock = json.loads((CONSUMER / "native-runtime.lock.json").read_text())
    if lock.get("schema_version") != 1 or lock.get("url") != URL:
        parser.error("Unsupported runtime lock")
    patches = json.loads((CONSUMER / "runtime-patches.lock.json").read_text())
    if patches.get("schema_version") != 1 or not set(patches) <= {"schema_version", "makepad"}:
        parser.error("Unsupported runtime patch lock")
    prepare_source(root, "octoscript-makepad", lock, args)
    manifest = json.loads((root / "octoscript-makepad/runtime.json").read_text())
    urls = {"makepad": "https://github.com/OctoSense-org/makepad.git", "octoscript": "https://github.com/OctoSense-org/Octoscript.git"}
    if manifest.get("schema_version") != 1 or set(manifest.get("repositories", {})) != set(urls):
        parser.error("Unsupported runtime source set")
    expected = {URL: lock["revision"]}
    for name, spec in manifest["repositories"].items():
        if spec.get("url") != urls[name]:
            parser.error(f"Unexpected runtime source: {name}")
        prepare_source(root, name, spec, args, patches.get(name))
        expected[spec["url"]] = spec["revision"]
    verify_consumer(expected)
    if args.cargo_manifest:
        cargo_manifest = args.cargo_manifest.resolve()
        metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1", "--features", "mobile-apps", "--manifest-path", str(cargo_manifest)], cwd=cargo_manifest.parent, text=True))
        seen = set()
        critical = {"makepad-script", "makepad-platform", "makepad-draw", "makepad-widgets", "makepad-live-id"}
        for package in metadata["packages"]:
            if package["name"] in critical:
                if package["name"] in seen or not Path(package["manifest_path"]).resolve().is_relative_to(root / "makepad"):
                    raise RuntimeError(f"Duplicate or foreign runtime crate: {package['name']}")
                seen.add(package["name"])
        if seen != critical:
            raise RuntimeError("Incomplete Makepad dependency graph")
    print(json.dumps({"runtime": lock, "repositories": manifest["repositories"], "patches": patches}, indent=2))


if __name__ == "__main__":
    main()
