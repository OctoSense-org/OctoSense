#!/usr/bin/env python3
"""Prepare OctoSense's pinned sibling sources (Python 3.9+).

Beside this repository, in one workspace directory:

  octoscript-makepad/     the release native-runtime.lock.json selects
  makepad/, octoscript/   at the revisions that release's runtime.json pins;
                          makepad carries the reviewed product patch
                          runtime-patches.lock.json names (contained apps)
  OctoSense-System-Apps/  at the revision native-apps.lock.json pins: the
                          system app bundles system-apps.json selects, the
                          Mail host service and the AppCard assistant

Local changes are preserved; --update only moves clean checkouts.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

CONSUMER = Path(__file__).resolve().parents[1]
URL = "https://github.com/OctoSense-org/Octoscript-Makepad.git"
RUNTIME_URLS = {
    "makepad": "https://github.com/OctoSense-org/makepad.git",
    "octoscript": "https://github.com/OctoSense-org/Octoscript.git",
}
APP_SOURCES = {
    # lock key: (sibling directory, URL)
    "system-apps": ("OctoSense-System-Apps", "https://github.com/OctoSense-org/OctoSense-System-Apps.git"),
}


def git(path, *args, check=True):
    result = subprocess.run(["git", "-C", str(path), *args], capture_output=True, text=True)
    if check and result.returncode:
        raise RuntimeError(result.stderr.strip() or f"git {' '.join(args)} failed in {path}")
    return result


def fetch(path, name, revision, args):
    if not git(path, "cat-file", "-e", revision + "^{commit}", check=False).returncode:
        return
    cached = args.cache.resolve() / name if args.cache else None
    if cached and (cached / ".git").exists() and not git(cached, "cat-file", "-e", revision + "^{commit}", check=False).returncode:
        if not git(path, "fetch", "--quiet", "--no-tags", "--depth=1", str(cached), revision, check=False).returncode:
            return
    git(path, "fetch", "--quiet", "--no-tags", "--depth=1", "origin", revision)


def prepare_source(root, name, url, revision, args, overlay=None):
    """Check out `name` at `revision` (plus the reviewed `overlay` patch)."""
    path = root / name
    if not re.fullmatch(r"[0-9a-f]{40}", revision or ""):
        raise RuntimeError(f"Unpinned source: {name}")
    if not (path / ".git").exists():
        if args.check or path.exists() and any(path.iterdir()):
            raise RuntimeError(f"Expected an empty dependency directory: {path}")
        path.mkdir(parents=True, exist_ok=True)
        git(path, "init", "--quiet")
        git(path, "remote", "add", "origin", url)
    if Path(git(path, "rev-parse", "--show-toplevel").stdout.strip()).resolve() != path.resolve():
        raise RuntimeError(f"Not a dependency checkout: {path}")
    current = git(path, "rev-parse", "--verify", "HEAD", check=False).stdout.strip()
    if current and (git(path, "diff", "--quiet", check=False).returncode
                    or git(path, "ls-files", "--others", "--exclude-standard").stdout):
        raise RuntimeError(f"Preserving local changes: {path}")
    tree = git(path, "write-tree").stdout.strip() if current else ""
    head_tree = git(path, "rev-parse", "HEAD^{tree}", check=False).stdout.strip()
    patch = None
    if overlay:
        patch = (CONSUMER / overlay["patch"]).resolve()
        if (not patch.is_relative_to(CONSUMER)
                or hashlib.sha256(patch.read_bytes()).hexdigest() != overlay["sha256"]
                or overlay["base_revision"] != revision):
            raise RuntimeError("Runtime patch does not match its source lock")
        # Prepared: the base with the patch staged, or a clean checkout of the
        # commit the patch was cut from (the same tree).
        if tree == overlay["tree"] and (current == revision or head_tree == overlay["tree"]):
            return current
    elif current == revision and tree == head_tree:
        return current
    if current and tree != head_tree:
        raise RuntimeError(f"Preserving staged changes: {path}")
    if current != revision:
        if args.check or current and not args.update:
            raise RuntimeError(f"{path} selects another revision; --update only changes clean checkouts")
        fetch(path, name, revision, args)
        git(path, "checkout", "--quiet", "--detach", revision)
    if patch:
        if args.check:
            raise RuntimeError(f"Reviewed runtime patch is not applied in {path}; run setup without --check")
        git(path, "apply", "--check", str(patch))
        git(path, "apply", "--index", str(patch))
        if git(path, "write-tree").stdout.strip() != overlay["tree"]:
            raise RuntimeError(f"Runtime patch produced an unexpected tree: {path}")
    return revision


def verify_consumer(expected):
    """Reject manifests that pin a runtime source at another revision."""
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


def verify_cargo(root, cargo_manifest):
    """One Makepad in the graph, from the prepared sibling checkout."""
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--features", "mobile-apps",
         "--manifest-path", str(cargo_manifest)], cwd=cargo_manifest.parent, text=True))
    seen = set()
    critical = {"makepad-script", "makepad-platform", "makepad-draw", "makepad-widgets", "makepad-live-id"}
    for package in metadata["packages"]:
        if package["name"] in critical:
            if package["name"] in seen or not Path(package["manifest_path"]).resolve().is_relative_to(root / "makepad"):
                raise RuntimeError(f"Duplicate or foreign runtime crate: {package['name']}")
            seen.add(package["name"])
    if seen != critical:
        raise RuntimeError("Incomplete Makepad dependency graph")
    octos = {package["source"] for package in metadata["packages"] if package["name"] == "octos-core"}
    if len(octos) > 1:
        raise RuntimeError(f"More than one octos source: {sorted(octos)}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--root", type=Path, default=CONSUMER.parent)
    parser.add_argument("--update", action="store_true", help="Move clean checkouts to the locked revisions")
    parser.add_argument("--check", action="store_true", help="Verify without changing anything")
    parser.add_argument("--cache", type=Path, help="Optional directory of Git object caches")
    parser.add_argument("--cargo-manifest", type=Path, help="Also check the locked Cargo graph")
    args = parser.parse_args()
    root = args.root.resolve()
    if root == CONSUMER or root.is_relative_to(CONSUMER):
        parser.error("Keep the sibling repositories outside the application")
    lock = json.loads((CONSUMER / "native-runtime.lock.json").read_text())
    if lock.get("schema_version") != 1 or lock.get("url") != URL:
        parser.error("Expected a pinned OctoSense-org/Octoscript-Makepad release")
    patches = json.loads((CONSUMER / "runtime-patches.lock.json").read_text())
    if patches.get("schema_version") != 1 or not set(patches) <= {"schema_version", "makepad"}:
        parser.error("Unsupported runtime patch lock")
    apps = json.loads((CONSUMER / "native-apps.lock.json").read_text())
    if apps.get("schema_version") != 1 or set(apps.get("repositories", {})) != set(APP_SOURCES):
        parser.error("Unsupported application source lock")

    prepare_source(root, "octoscript-makepad", URL, lock["revision"], args)
    manifest = json.loads((root / "octoscript-makepad/runtime.json").read_text())
    if manifest.get("schema_version") != 1 or set(manifest.get("repositories", {})) != set(RUNTIME_URLS):
        parser.error("Unsupported runtime source set")
    expected = {URL: lock["revision"]}
    for name, spec in manifest["repositories"].items():
        if spec.get("url") != RUNTIME_URLS[name]:
            parser.error(f"Unexpected runtime source: {name}")
        prepare_source(root, name, spec["url"], spec["revision"], args, patches.get(name))
        expected[spec["url"]] = spec["revision"]
    for key, spec in apps["repositories"].items():
        directory, url = APP_SOURCES[key]
        if spec.get("url") != url:
            parser.error(f"Unexpected application source: {key}")
        prepare_source(root, directory, url, spec["revision"], args)
    verify_consumer(expected)
    if args.cargo_manifest:
        verify_cargo(root, args.cargo_manifest.resolve())
    print(json.dumps({"runtime": lock, "repositories": manifest["repositories"],
                      "patches": patches, "apps": apps["repositories"]}, indent=2))


if __name__ == "__main__":
    main()
