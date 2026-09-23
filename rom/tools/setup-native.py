#!/usr/bin/env python3
"""Bootstrap an application's pinned Octoscript-Makepad runtime (Python 3.9+)."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

CONSUMER = Path(__file__).resolve().parents[1]
URL = "https://github.com/OctoSense-org/Octoscript-Makepad.git"
MAKEPAD_URL = "https://github.com/OctoSense-org/makepad.git"
OCTOSCRIPT_URL = "https://github.com/OctoSense-org/Octoscript.git"
SOURCE_URLS = {"makepad": MAKEPAD_URL, "octoscript": OCTOSCRIPT_URL}


def git(path, *args, check=True):
    return subprocess.run(["git", "-C", str(path), *args], check=check,
                          capture_output=True, text=True)


def pinned(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{40}", value) is not None


def load_lock(consumer):
    lock = json.loads((consumer / "native-runtime.lock.json").read_text())
    if not isinstance(lock, dict) or set(lock) - {"schema_version", "url", "revision", "makepad_override"} or \
            lock.get("schema_version") != 1 or lock.get("url") != URL or not pinned(lock.get("revision")):
        raise RuntimeError("Expected a pinned OctoSense-org/Octoscript-Makepad release")
    if "makepad_override" in lock:
        override = lock["makepad_override"]
        if not isinstance(override, dict) or set(override) != {"url", "revision", "reason"} or \
                override.get("url") != MAKEPAD_URL or not pinned(override.get("revision")) or \
                not isinstance(override.get("reason"), str) or not override["reason"].strip():
            raise RuntimeError("Expected a pinned makepad_override with its reason and canonical URL")
    return lock


def checkout_status(path):
    if not path.exists():
        return None
    if not path.is_dir() or not (path / ".git").exists():
        if path.is_dir() and not any(path.iterdir()):
            return None
        raise RuntimeError(f"Preserving a directory that is not a Git checkout: {path}")
    top = Path(git(path, "rev-parse", "--show-toplevel").stdout.strip()).resolve()
    if top != path.resolve():
        raise RuntimeError(f"{path} belongs to another Git checkout: {top}")
    return {"revision": git(path, "rev-parse", "--verify", "HEAD", check=False).stdout.strip(),
            "dirty": bool(git(path, "status", "--porcelain", "--untracked-files=normal").stdout)}


def preflight(path, state, revision, *, update, check):
    if state and state["dirty"]:
        raise RuntimeError(f"Preserving local changes: {path}")
    if not state or state["revision"] != revision:
        if check:
            raise RuntimeError(f"{path} differs from the selected runtime revision {revision}")
        if state and state["revision"] and not update:
            raise RuntimeError(f"{path} has another revision; use --update with clean sources")


def prepare_checkout(path, spec, state):
    if state and state["revision"] == spec["revision"]:
        return
    if state is None:
        path.mkdir(parents=True, exist_ok=True)
        git(path, "init", "--quiet")
        git(path, "remote", "add", "origin", spec["url"])
    if git(path, "cat-file", "-e", spec["revision"] + "^{commit}", check=False).returncode:
        # Fetch the declared source, even if the checkout's origin was changed.
        git(path, "fetch", "--quiet", "--no-tags", "--depth=1", spec["url"], spec["revision"])
    git(path, "checkout", "--quiet", "--detach", spec["revision"])


def runtime_manifest(runtime):
    """Validate the unmodified wrapper release before applying a consumer override."""
    value = json.loads((runtime / "runtime.json").read_text())
    if not isinstance(value, dict) or value.get("schema_version") != 1 or \
            set(value.get("repositories", {})) != set(SOURCE_URLS):
        raise RuntimeError("Unsupported Octoscript-Makepad runtime manifest")
    for name, url in SOURCE_URLS.items():
        spec = value["repositories"][name]
        if not isinstance(spec, dict) or spec.get("url") != url or not pinned(spec.get("revision")):
            raise RuntimeError(f"Invalid runtime source: {name}")
    cargo = (runtime / "Cargo.toml").read_text()
    for package, source in (("makepad-widgets", "makepad"), ("makepad-script", "makepad"),
                            ("octoscript-ui-l0", "octoscript")):
        declaration = re.search(r"^" + re.escape(package) + r"\s*=\s*\{([^}]+)\}", cargo, re.M)
        revision = re.search(r'rev\s*=\s*"([0-9a-f]{40})"', declaration[1]) if declaration else None
        if not revision or revision[1] != value["repositories"][source]["revision"]:
            raise RuntimeError(f"Cargo.toml and runtime.json disagree about {package}")
    return value


def verify_consumer(consumer, lock, sources):
    expected = {spec["url"]: spec["revision"] for spec in sources.values()}
    expected[URL] = lock["revision"]
    checked = []
    for directory, subdirs, filenames in os.walk(consumer):
        subdirs[:] = [name for name in subdirs if name not in {
            ".git", "target", "vendor", "octos", "node_modules", ".venv", "private", "runtime", "pipeline-output"
        }]
        if "Cargo.toml" not in filenames:
            continue
        path = Path(directory) / "Cargo.toml"
        cargo = "\n".join(line for line in path.read_text().splitlines() if not line.lstrip().startswith("#"))
        # Inline dependency tables may span lines. Named dependency tables are
        # handled too, so formatting does not hide a stale or missing pin.
        declarations = re.findall(r"\{([^}]+)\}", cargo, re.S)
        declarations += re.findall(r"(?m)^\[[^\]\n]+\]\s*\n([^\[]*)", cargo)
        for declaration in declarations:
            for url in re.finditer(r'git\s*=\s*"([^"]+)"', declaration):
                if url[1] not in expected:
                    continue
                # A section holding inline tables was checked individually above.
                if "{" in declaration:
                    continue
                revision = re.search(r'rev\s*=\s*"([0-9a-f]{40})"', declaration)
                if not revision or revision[1] != expected[url[1]]:
                    raise RuntimeError(f"{path}: dependency differs from the selected runtime: {url[1]}")
        checked.append(str(path.relative_to(consumer)))
    return checked


def validate_cargo_sources(root, metadata):
    critical = {"makepad-script", "makepad-script-std", "makepad-platform", "makepad-draw",
                "makepad-widgets", "makepad-live-id", "makepad-app-module", "makepad-network"}
    seen = {}
    for package in metadata["packages"]:
        name = package["name"]
        if name not in critical:
            continue
        source = Path(package["manifest_path"]).resolve()
        if not source.is_relative_to((root / "makepad").resolve()) or name in seen:
            raise RuntimeError(f"Multiple or foreign Makepad sources in Cargo graph: {name} ({source})")
        seen[name] = str(source)
    if not seen:
        raise RuntimeError("Cargo graph has no Makepad runtime")
    return seen


def setup(root, consumer, *, update=False, check=False, cargo_manifest=None):
    root, consumer = Path(root).resolve(), Path(consumer).resolve()
    if root == consumer or root.is_relative_to(consumer):
        raise RuntimeError("Keep native framework repositories outside the application")
    lock = load_lock(consumer)
    names = ("octoscript-makepad", "makepad", "octoscript")
    states = {name: checkout_status(root / name) for name in names}
    # Preserve local work everywhere before changing even the wrapper checkout.
    for name, state in states.items():
        if state and state["dirty"]:
            path = root / name
            raise RuntimeError(f"Preserving local changes: {path}")
    runtime = root / "octoscript-makepad"
    preflight(runtime, states["octoscript-makepad"], lock["revision"], update=update, check=check)
    if not check:
        prepare_checkout(runtime, lock, states["octoscript-makepad"])
    manifest = runtime_manifest(runtime)
    sources = dict(manifest["repositories"])
    if "makepad_override" in lock:
        sources["makepad"] = {key: lock["makepad_override"][key] for key in ("url", "revision")}
    consumer_manifests = verify_consumer(consumer, lock, sources)
    for name, spec in sources.items():
        preflight(root / name, states[name], spec["revision"], update=update, check=check)
    if not check:
        for name, spec in sources.items():
            prepare_checkout(root / name, spec, states[name])
    selected = {"octoscript-makepad": {"url": URL, "revision": lock["revision"]}, **sources}
    for name, spec in selected.items():
        preflight(root / name, checkout_status(root / name), spec["revision"], update=False, check=True)
    receipt = {"schema_version": 1, "runtime": selected["octoscript-makepad"], "repositories": sources,
               "manifest_sha256": hashlib.sha256((runtime / "runtime.json").read_bytes()).hexdigest(),
               "consumer_manifests": consumer_manifests}
    if "makepad_override" in lock:
        receipt["makepad_override"] = lock["makepad_override"]
    if cargo_manifest:
        cargo_manifest = Path(cargo_manifest).resolve()
        metadata = json.loads(subprocess.check_output([
            "cargo", "metadata", "--locked", "--format-version", "1", "--manifest-path", str(cargo_manifest)
        ], cwd=cargo_manifest.parent, text=True))
        receipt["cargo_sources"] = validate_cargo_sources(root, metadata)
    if not check:
        (root / ".octoscript-runtime.json").write_text(json.dumps(receipt, indent=2) + "\n")
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=CONSUMER.parent)
    parser.add_argument("--update", action="store_true", help="Update clean checkouts to the selected revisions")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--cargo-manifest", type=Path)
    args = parser.parse_args()
    print(json.dumps(setup(args.root, CONSUMER, update=args.update, check=args.check,
                           cargo_manifest=args.cargo_manifest), indent=2))


if __name__ == "__main__":
    main()
