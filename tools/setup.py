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

With a hub configured, each missing checkout is a `git worktree` of an
existing local clone (the hub) instead of a new clone, so a machine keeps one
object store per repository. The hub for <name> is, first match wins:

  OCTOSENSE_<NAME>_HUB=<clone>       e.g. OCTOSENSE_OCTOSCRIPT_MAKEPAD_HUB
  --hub <dir>                        holds the clones as <dir>/<name>
  OCTOSENSE_SOURCES_HUB=<dir>        the same, from the environment
  ~/.config/octosense/sources.json   {"repositories": {"<name>": "<clone>"},
                                      "hub": "<dir>"}  ($XDG_CONFIG_HOME,
                                      or the file OCTOSENSE_SOURCES_CONFIG names)

With none, the checkouts are clones as before (CI, fresh machines).
--no-hub ignores every hub setting.

  python3 tools/setup.py                      # prepare
  python3 tools/setup.py --check --cargo      # verify (CI)
  python3 tools/setup.py --convert            # replace clean clones with hub worktrees
  python3 tools/setup.py --remove-worktrees   # before deleting this checkout
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

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
                 "octosense-app-hub-app": "App Hub", "octosense-app-policy": "App Hub",
                 "octosense-app-hub": "App Hub", "rinx": "Rinx"}


def git(path, *args, check=True):
    result = subprocess.run(["git", "-C", str(path), *args], capture_output=True, text=True)
    if check and result.returncode:
        raise RuntimeError(result.stderr.strip() or f"git {' '.join(args)} failed in {path}")
    return result


def note(message):
    print(f"setup: {message}", file=sys.stderr)


def same_url(a, b):
    def norm(url):
        url = url.strip().rstrip("/")
        return (url[:-4] if url.endswith(".git") else url).lower()
    return norm(a) == norm(b)


def load_config():
    """The per-user hub settings; they live outside every repository."""
    path = os.environ.get("OCTOSENSE_SOURCES_CONFIG")
    if path is None:
        base = os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config"
        path = Path(base) / "octosense/sources.json"
    path = Path(path).expanduser()
    if not path.is_file():
        return {}
    config = json.loads(path.read_text())
    if not isinstance(config, dict) or not set(config) <= {"hub", "repositories"}:
        raise RuntimeError(f"Unsupported hub config: {path}")
    return config


def configured_hub(name, args):
    """The local clone `name` should be a worktree of, or None to clone."""
    if getattr(args, "no_hub", False):
        return None
    env = os.environ.get(f"OCTOSENSE_{name.upper().replace('-', '_')}_HUB")
    if env:
        return Path(env).expanduser()
    directory = getattr(args, "hub", None) or os.environ.get("OCTOSENSE_SOURCES_HUB")
    if directory:
        return Path(directory).expanduser() / name
    config = load_config()
    if config.get("repositories", {}).get(name):
        return Path(config["repositories"][name]).expanduser()
    if config.get("hub"):
        return Path(config["hub"]).expanduser() / name
    return None


def common_dir(path):
    """The object store a checkout (clone or worktree) uses."""
    found = git(path, "rev-parse", "--path-format=absolute", "--git-common-dir").stdout.strip()
    return Path(found).resolve()


def validate_hub(hub, name, spec):
    if not (hub / ".git").exists():
        raise RuntimeError(f"No hub clone for {name} at {hub}: clone {spec['url']} there once, "
                           f"point OCTOSENSE_{name.upper().replace('-', '_')}_HUB at the clone, or pass --no-hub")
    remote = git(hub, "remote", "get-url", "origin", check=False).stdout.strip()
    if not same_url(remote, spec["url"]):
        raise RuntimeError(f"Hub {hub} is not a clone of {spec['url']} (origin: {remote or 'none'})")
    return hub.resolve()


def is_shallow(path):
    return git(path, "rev-parse", "--is-shallow-repository").stdout.strip() == "true"


def fetch_revision(path, name, revision, args, shallow):
    """Fetch `revision` into the object store behind `path`.

    `shallow` fetches only that commit. A full hub must never be fetched with
    --depth: that would turn it into a shallow clone."""
    if not git(path, "cat-file", "-e", revision + "^{commit}", check=False).returncode:
        return
    depth = ["--depth=1"] if shallow else []
    cached = args.cache.resolve() / name if args.cache else None
    if cached and (cached / ".git").exists() and not git(cached, "cat-file", "-e", revision + "^{commit}", check=False).returncode:
        if not git(path, "fetch", "--quiet", "--no-tags", *depth, str(cached), revision, check=False).returncode:
            return
    git(path, "fetch", "--quiet", "--no-tags", *depth, "origin", revision)


def add_worktree(hub, path, name, revision, args):
    """`git worktree add --detach` from the hub: no new clone, one object store."""
    fetch_revision(hub, name, revision, args, is_shallow(hub))
    # A checkout deleted without `git worktree remove` leaves a stale entry
    # that blocks adding the same path again.
    listed = git(hub, "worktree", "list", "--porcelain").stdout.splitlines()
    if f"worktree {path.resolve()}" in listed or f"worktree {path}" in listed:
        git(hub, "worktree", "prune")
    path.parent.mkdir(parents=True, exist_ok=True)
    git(hub, "worktree", "add", "--quiet", "--detach", str(path), revision)
    note(f"{path} is a worktree of {hub} at {revision[:12]}")


def owned_state(path, overlay):
    """Why the checkout holds work setup does not own, or None if it holds none.

    Setup owns the checked-out revision and, for a patched source, the
    reviewed patch staged on it."""
    if git(path, "diff", "--quiet", check=False).returncode:
        return "unstaged changes"
    untracked = git(path, "ls-files", "--others", "--exclude-standard").stdout.split()
    if untracked:
        return f"untracked files ({', '.join(untracked[:5])})"
    tree = git(path, "write-tree").stdout.strip()
    allowed = {git(path, "rev-parse", "HEAD^{tree}").stdout.strip()}
    if overlay:
        allowed.add(overlay["tree"])
    if tree not in allowed:
        return "staged changes"
    return None


def convert_clone(hub, path, name, overlay, args):
    """Replace a clean standalone clone with a worktree of the hub at the same commit."""
    reason = owned_state(path, overlay)
    ignored = git(path, "ls-files", "--others", "--ignored", "--exclude-standard", "--directory").stdout.split()
    if ignored:
        reason = reason or f"ignored files ({', '.join(ignored[:5])})"
    if not git(path, "rev-parse", "--verify", "--quiet", "refs/stash", check=False).returncode:
        reason = reason or "stashed changes"
    head = git(path, "rev-parse", "HEAD").stdout.strip()
    fetch_revision(hub, name, head, args, is_shallow(hub))
    for branch in git(path, "for-each-ref", "--format=%(refname:short) %(objectname)", "refs/heads").stdout.splitlines():
        ref, commit = branch.split()
        if git(hub, "cat-file", "-e", commit + "^{commit}", check=False).returncode:
            reason = reason or f"branch {ref} has commits the hub lacks"
    if reason:
        raise RuntimeError(f"Not converting {path}: it has {reason}; move that work into {hub} first")
    aside = path.with_name(f".{name}.converting")
    if aside.exists():
        raise RuntimeError(f"Leftover from an interrupted conversion: {aside}")
    path.rename(aside)
    try:
        add_worktree(hub, path, name, head, args)
    except Exception:
        if path.exists():
            shutil.rmtree(path)
        git(hub, "worktree", "prune")
        aside.rename(path)
        raise
    shutil.rmtree(aside)
    note(f"converted the clone at {path} into a worktree of {hub}")


def check_hub(root, name, spec, args, overlay):
    """With a hub configured, an existing checkout should be its worktree."""
    path = root / name
    try:
        hub = configured_hub(name, args)
        if hub is None:
            return
        hub = validate_hub(hub, name, spec)
    except (RuntimeError, ValueError) as error:
        if getattr(args, "convert", False):
            raise
        note(str(error))
        return
    if common_dir(path) == common_dir(hub):
        return
    if (path / ".git").is_dir() and getattr(args, "convert", False) and not args.check:
        convert_clone(hub, path, name, overlay, args)
    elif (path / ".git").is_dir():
        note(f"{path} is a standalone clone, not a worktree of {hub}; "
             "run `python3 tools/setup.py --convert` to replace it (it must hold no local work)")
    else:
        note(f"{path} is a worktree of {common_dir(path)}, not of the hub {hub}; leaving it")


def prepare_source(root, name, spec, args, overlay=None):
    """Check out `name` at `spec["revision"]`, plus the reviewed `overlay` patches."""
    path = root / name
    if not re.fullmatch(r"[0-9a-f]{40}", spec.get("revision", "")):
        raise RuntimeError(f"Unpinned source: {name}")
    if not (path / ".git").exists():
        if args.check or path.exists() and any(path.iterdir()):
            raise RuntimeError(f"Expected an empty dependency directory: {path}")
        hub = configured_hub(name, args)
        if hub is not None:
            add_worktree(validate_hub(hub, name, spec), path, name, spec["revision"], args)
        else:
            path.mkdir(parents=True, exist_ok=True)
            git(path, "init", "--quiet")
            git(path, "remote", "add", "origin", spec["url"])
    else:
        check_hub(root, name, spec, args, overlay)
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
        # A standalone checkout fetches only the pinned commit; a worktree
        # shares its hub's store and fetches shallowly only if the hub is shallow.
        linked = common_dir(path) != (path / ".git").resolve()
        fetch_revision(path, name, spec["revision"], args, not linked or is_shallow(path))
        git(path, "checkout", "--quiet", "--detach", spec["revision"])
    if overlay:
        if args.check:
            raise RuntimeError(f"Reviewed runtime patch is not applied in {path}; run setup without --check")
        for patch in patches:
            git(path, "apply", "--check", str(patch))
            git(path, "apply", "--index", str(patch))
        if git(path, "write-tree").stdout.strip() != overlay["tree"]:
            raise RuntimeError(f"Runtime patch produced an unexpected tree: {path}")


def remove_worktrees(root, patches):
    """Unregister and delete the checkouts that are hub worktrees, then prune.

    Deleting a worktree with `rm -rf` (or deleting this repository's checkout,
    which holds .sources/) leaves a stale entry in the hub; this removes each
    one properly. Work setup does not own stops it; ignored files (build
    output) go with the checkout. Standalone clones are left in place."""
    if not root.is_dir():
        return
    for path in sorted(p for p in root.iterdir() if (p / ".git").exists()):
        if (path / ".git").is_dir():
            note(f"{path} is a standalone clone; left in place")
            continue
        store = common_dir(path)
        reason = owned_state(path, patches.get(path.name))
        if reason:
            raise RuntimeError(f"Not removing {path}: it has {reason}")
        # The patch is staged, so git needs --force; owned_state has checked it.
        git(store, "worktree", "remove", "--force", str(path))
        git(store, "worktree", "prune")
        note(f"removed the worktree {path} from {store}")


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
    parser.add_argument("--hub", help="Directory holding local clones as <dir>/<name>: missing checkouts become their worktrees")
    parser.add_argument("--no-hub", action="store_true", help="Ignore every hub setting and clone")
    parser.add_argument("--convert", action="store_true",
                        help="Replace standalone clones that hold no local work with worktrees of the hub")
    parser.add_argument("--remove-worktrees", action="store_true",
                        help="Remove the checkouts that are hub worktrees (before deleting this repository checkout)")
    args = parser.parse_args()
    root = args.root.resolve()
    lock = json.loads((PRODUCT / "native-runtime.lock.json").read_text())
    if lock.get("schema_version") != 1 or lock.get("url") != URL:
        parser.error("Unsupported runtime lock")
    patches = json.loads((PRODUCT / "runtime-patches.lock.json").read_text())
    if patches.get("schema_version") != 1 or not set(patches) <= {"schema_version", "makepad"}:
        parser.error("Unsupported runtime patch lock")
    if args.remove_worktrees:
        remove_worktrees(root, patches)
        return
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
