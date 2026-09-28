#!/usr/bin/env python3
"""Build the OctoSense desktop as an installable package (Python 3.9+).

    python3 desktop/scripts/package.py                 # this OS's formats
    python3 desktop/scripts/package.py --formats app   # just the macOS .app
    python3 desktop/scripts/package.py --version-from-tag desktop-v0.2.0

One command does the whole release build, from the repository root after
`python3 tools/setup.py`:

1. **Build** `octosense` in release with Makepad's packaged-resource mode:
   `MAKEPAD_PACKAGE_DIR` (and `MAKEPAD=apple_bundle` on macOS), so every
   `crate_resource(...)` is read from the package, never from `.sources/` or
   the checkout. Paths in the binary are remapped (`--remap-path-prefix` for
   the home directory, `CARGO_HOME` and the checkout) and debug info stripped (symbol names stay, so backtraces and `[ui-hang]` reports remain readable).
2. **Stage resources**: the `resources/` directory of every git or path crate
   the app links (Makepad's widgets and fonts, the shell's icons, App Hub,
   Rinx, ...) into `target/octosense-package/resources/<crate_name>/`, the
   layout Makepad looks up (`<crate_name>/resources/<file>`). System apps
   need nothing here: App Hub packs their bundles into the binary.
3. **Kernel**: the octos kernel at the revision the workspace pins, checked
   out, built for this machine and staged by `tools/build-desktop.py`'s
   `source_at` and `stage` (which refuses a binary whose `--version` is not
   the locked revision), or `--kernel <path>`. It ships beside the executable
   as `octos-kernel[.exe]`, where the kernel service finds it without
   `OCTOS_APP_CORE_BIN` (`crates/kernel/src/launch.rs`). `--no-kernel` ships
   none: the app then runs without an assistant (AI providers are still
   saved) and its log says why.
4. **Package** with cargo-packager from `desktop/packaging/release.json`:
   macOS `.app` and `.dmg`, Windows NSIS `.exe`, Linux `.deb` and `.AppImage`.

Where the resources land, and the `MAKEPAD_PACKAGE_DIR` that finds them:

| Format | Resources | `MAKEPAD_PACKAGE_DIR` |
| --- | --- | --- |
| `.app`/`.dmg` | `OctoSense.app/Contents/Resources/` | `.` (+ `apple_bundle`: NSBundle's resource path) |
| NSIS `.exe` | the install directory, beside `octosense.exe` | `.` (next to the executable) |
| `.deb`, `.AppImage` | `usr/lib/octosense/` | `../lib/octosense` (from `usr/bin/octosense`) |

Signing (all optional; unsigned without them): `APPLE_SIGNING_IDENTITY` sets
the Developer ID identity cargo-packager signs the .app, its kernel and the
.dmg with (it imports `APPLE_CERTIFICATE`/`APPLE_CERTIFICATE_PASSWORD` into a
temporary keychain, and notarizes the .app with `APPLE_API_KEY`,
`APPLE_API_ISSUER`, `APPLE_API_KEY_PATH` or `APPLE_ID`, `APPLE_PASSWORD`,
`APPLE_TEAM_ID`); `WINDOWS_CERTIFICATE_THUMBPRINT` signs the Windows binaries
and installer with signtool from the certificate store. The release workflow
(`.github/workflows/release-desktop.yml`) sets these from secrets.

Run `python3 tools/release-scan.py <artifacts>` on the output before
publishing anything.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
DESKTOP = ROOT / "desktop"
PACKAGING = DESKTOP / "packaging"
PACKAGE = "octosense"
KERNEL_NAME = "octos-kernel"
DEFAULT_FORMATS = {"macos": ["app", "dmg"], "windows": ["nsis"], "linux": ["deb", "appimage"]}
# Where Makepad reads packaged resources from, per OS (see the table above).
PACKAGE_DIR = {"macos": ".", "windows": ".", "linux": "../lib/octosense"}
ICON_SLOTS = {"32": "icon_32.png", "64": "icon_64.png", "128": "icon_128.png", "256": "icon_256.png",
              "512": "icon_512.png", "1024": "icon_512@2x.png", "ICO": "icon.ico"}
MACOS_MINIMUM = "11.0"


def host_os():
    return {"darwin": "macos", "win32": "windows"}.get(sys.platform, "linux")


def version_from_tag(tag):
    """`desktop-v1.2.3` (or a ref ending in it) -> `1.2.3`. Installers need a
    numeric version, so anything else is refused."""
    name = tag.rsplit("/", 1)[-1]
    match = re.fullmatch(r"desktop-v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.]+)?)", name)
    if not match:
        raise ValueError(f"{tag!r} is not a desktop release tag (desktop-v<major>.<minor>.<patch>[-<pre>])")
    return match.group(1)


def remap_flags(root, home=None, cargo_home=None):
    """`--remap-path-prefix` pairs for rustc, general first: when several
    match, rustc applies the last, so the checkout (`.`) beats the home dir
    (`~`) it may sit in."""
    pairs = []
    if home:
        pairs.append((str(home), "~"))
    if cargo_home:
        pairs.append((str(cargo_home), "/cargo"))
    pairs.append((str(root), "."))
    flags = []
    for src, dst in pairs:
        flags += ["--remap-path-prefix", f"{src}={dst}"]
    return flags


def package_env(os_name, root, base_env):
    """The environment the release build adds: packaged-resource mode, path
    remapping, no debug info, the window icon, the macOS minimum."""
    env = {"MAKEPAD_PACKAGE_DIR": PACKAGE_DIR[os_name], "CARGO_PROFILE_RELEASE_STRIP": "debuginfo"}
    if os_name == "macos":
        env["MAKEPAD"] = "apple_bundle"
        env["MACOSX_DEPLOYMENT_TARGET"] = MACOS_MINIMUM
    # Windows: the native profile path, not an MSYS `HOME` like /c/Users/...
    home = base_env.get("USERPROFILE") if os_name == "windows" else base_env.get("HOME")
    cargo_home = base_env.get("CARGO_HOME") or (str(Path(home) / ".cargo") if home else None)
    existing = base_env.get("CARGO_ENCODED_RUSTFLAGS")
    flags = existing.split("\x1f") if existing else base_env.get("RUSTFLAGS", "").split()
    # CARGO_ENCODED_RUSTFLAGS carries paths with spaces intact (RUSTFLAGS
    # splits on them) and takes precedence over RUSTFLAGS.
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join([f for f in flags if f] + remap_flags(root, home, cargo_home))
    for slot, name in ICON_SLOTS.items():
        env[f"MAKEPAD_APP_ICON_{slot}"] = str(PACKAGING / "icons" / name)
    return env


def cargo_json(args, env):
    out = subprocess.run(["cargo", *args], cwd=ROOT, env=env, check=True, capture_output=True, text=True).stdout
    return out


def feature_args(features):
    """`--features` for octosense, qualified (the workspace root is virtual)."""
    if not features:
        return []
    return ["--features", ",".join(f if "/" in f else f"{PACKAGE}/{f}" for f in features.split(","))]


def linked_packages(features, env):
    """(name, version) of every package the app links: `cargo tree` resolves
    features for octosense alone, as its build does (the workspace-wide
    `cargo metadata` resolve would add what other members enable)."""
    out = cargo_json(["tree", "--locked", "-p", PACKAGE, "-e", "normal", "--prefix", "none", "--target", host_triple(),
                      "--format", "{p}", *feature_args(features)], env)
    linked = set()
    for line in out.splitlines():
        match = re.match(r"(\S+) v(\S+)", line.strip())
        if match:
            linked.add((match.group(1), match.group(2)))
    return linked


def host_triple():
    out = subprocess.run(["rustc", "-vV"], check=True, capture_output=True, text=True).stdout
    return re.search(r"^host: (\S+)$", out, re.M).group(1)


def resource_crates(metadata, linked):
    """(crate_name, resources dir) of every linked git or path package that
    has a `resources/` directory. Registry crates are left out: Makepad
    resources are named by the crates that declare `crate_resource`s, and
    none of those come from crates.io."""
    found = []
    for package in sorted(metadata["packages"], key=lambda p: p["name"]):
        if (package["name"], package["version"]) not in linked:
            continue
        source = package.get("source") or ""
        if source.startswith(("registry+", "sparse+")):
            continue
        resources = Path(package["manifest_path"]).parent / "resources"
        if resources.is_dir():
            found.append((package["name"].replace("-", "_"), resources))
    return found


def stage_resources(crates, dest):
    """Copy each crate's resources to `<dest>/<crate_name>/resources/`."""
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)
    for name, resources in crates:
        shutil.copytree(resources, dest / name / "resources",
                        ignore=shutil.ignore_patterns(".DS_Store", "*.md"))
    return dest


def build_desktop():
    """tools/build-desktop.py: the locked kernel's checkout and staging."""
    spec = importlib.util.spec_from_file_location("build_desktop", ROOT / "tools/build-desktop.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def build_kernel(env, work, offline=False):
    """The octos kernel at the workspace's revision, built for this machine
    with the release environment (paths remapped, no debug info)."""
    tool = build_desktop()
    revision = tool.kernel_tool.octos_revision()
    source = tool.source_at(revision, work, offline)
    env = {k: v for k, v in env.items() if not k.startswith("MAKEPAD")}
    command = ["cargo", "build", "--locked", "--release", "--target-dir", str(work / "target"), *tool.kernel_tool.KERNEL_BUILD]
    subprocess.run(command + (["--offline"] if offline else []), cwd=source, env=env, check=True)
    return work / "target" / "release" / ("octos" + tool.SUFFIX)


def stage_kernel(kernel, out):
    """Stage (and check) the kernel as `octos-kernel`, then name a copy the
    way cargo-packager's `externalBinaries` expects (`<name>-<triple>`)."""
    tool = build_desktop()
    staged = tool.stage(kernel, out / "kernel", tool.kernel_tool.octos_revision())
    sidecar = out / "bin" / KERNEL_NAME
    sidecar.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(staged, sidecar.with_name(f"{KERNEL_NAME}-{host_triple()}{tool.SUFFIX}"))
    receipt = json.loads((out / "kernel" / "octos-kernel.json").read_text())
    return sidecar, receipt


def debian_depends(binaries):
    """The shared-library packages the binaries link, from dpkg-shlibdeps."""
    if not shutil.which("dpkg-shlibdeps"):
        return None
    with tempfile.TemporaryDirectory() as temp:
        (Path(temp) / "debian").mkdir()
        (Path(temp) / "debian/control").write_text("Source: octosense\n\nPackage: octosense\nArchitecture: any\n")
        out = subprocess.run(["dpkg-shlibdeps", "-O", "--ignore-missing-info", *(f"-e{b}" for b in binaries)],
                             cwd=temp, check=True, capture_output=True, text=True).stdout
    line = next((l for l in out.splitlines() if l.startswith("shlibs:Depends=")), "")
    return [d.strip() for d in line.split("=", 1)[-1].split(",") if d.strip()]


def packager_config(base, *, version, binaries_dir, out_dir, resources, kernel, env):
    """release.json with this build's version, paths, kernel and signing."""
    config = json.loads(json.dumps(base))
    config["version"] = version
    config["binariesDir"] = str(binaries_dir)
    config["outDir"] = str(out_dir)
    config["resources"] = [{"src": str(resources), "target": "."}]
    if kernel:
        # cargo-packager copies `<path>-<target triple>[.exe]` beside the
        # main binary as `<name>[.exe]`.
        config["externalBinaries"] = [str(kernel)]
    identity = env.get("APPLE_SIGNING_IDENTITY")
    if identity:
        config.setdefault("macos", {})["signingIdentity"] = identity
    thumbprint = env.get("WINDOWS_CERTIFICATE_THUMBPRINT")
    if thumbprint:
        config.setdefault("windows", {}).update({
            "certificateThumbprint": thumbprint, "digestAlgorithm": "sha256", "tsp": True,
            "timestampUrl": env.get("WINDOWS_TIMESTAMP_URL", "http://timestamp.digicert.com"),
        })
    return config


def cargo_version():
    text = (DESKTOP / "Cargo.toml").read_text()
    return re.search(r'^version = "([^"]+)"', text, re.M).group(1)


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--formats", help="Comma-separated cargo-packager formats (default: this OS's: "
                   + "; ".join(f"{k} {','.join(v)}" for k, v in DEFAULT_FORMATS.items()) + ")")
    p.add_argument("--version", help="The package version (default: desktop/Cargo.toml's)")
    p.add_argument("--version-from-tag", metavar="TAG", help="Take the version from a desktop-v<version> tag")
    p.add_argument("--features", help="Cargo features for octosense (default: its defaults)")
    kernel = p.add_mutually_exclusive_group()
    kernel.add_argument("--kernel", type=Path, help="Ship this prebuilt octos kernel instead of building it")
    kernel.add_argument("--no-kernel", action="store_true", help="Ship no kernel: the app runs without an assistant")
    p.add_argument("--out", type=Path, default=ROOT / "target/octosense-package",
                   help="Work and output directory (default: target/octosense-package)")
    p.add_argument("--skip-build", action="store_true", help="Reuse the last release build (resources and packaging only)")
    p.add_argument("--offline", action="store_true")
    p.add_argument("--print-env", action="store_true", help="Print the build environment as JSON and exit")
    args = p.parse_args(argv)

    os_name = host_os()
    formats = args.formats.split(",") if args.formats else DEFAULT_FORMATS[os_name]
    version = version_from_tag(args.version_from_tag) if args.version_from_tag else (args.version or cargo_version())
    added = package_env(os_name, ROOT, os.environ)
    if args.print_env:
        print(json.dumps(added, indent=2))
        return
    env = {**os.environ, **added}
    env.pop("RUSTFLAGS", None)  # folded into CARGO_ENCODED_RUSTFLAGS
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    offline = ["--offline"] if args.offline else []
    features = feature_args(args.features)

    if not args.skip_build:
        print(f"==> cargo build --release -p {PACKAGE} (MAKEPAD_PACKAGE_DIR={added['MAKEPAD_PACKAGE_DIR']})", flush=True)
        subprocess.run(["cargo", "build", "--locked", "--release", "-p", PACKAGE, *features, *offline],
                       cwd=ROOT, env=env, check=True)

    print("==> staging resources", flush=True)
    metadata = json.loads(cargo_json(["metadata", "--locked", "--format-version", "1"], env))
    crates = resource_crates(metadata, linked_packages(args.features, env))
    resources = stage_resources(crates, out / "resources")
    for name, src in crates:
        size = sum(f.stat().st_size for f in (resources / name).rglob("*") if f.is_file())
        print(f"    {name:<28} {size / 1e6:7.1f} MB  <- {src.relative_to(ROOT) if ROOT in src.parents else name}")

    triple = host_triple()
    kernel_receipt = None
    sidecar = None
    if not args.no_kernel:
        if args.kernel:
            binary = args.kernel.resolve()
        else:
            print("==> building the octos kernel for this machine (tools/build-desktop.py source_at)", flush=True)
            binary = build_kernel(env, out / "kernel-build", args.offline)
        sidecar, kernel_receipt = stage_kernel(binary, out)
        print(f"    {KERNEL_NAME}: {kernel_receipt['version']} sha256 {kernel_receipt['sha256'][:16]}")
    else:
        print("==> no kernel: this package runs without an assistant", flush=True)

    base = json.loads((PACKAGING / "release.json").read_text())
    binaries_dir = ROOT / "target" / "release"
    config = packager_config(base, version=version, binaries_dir=binaries_dir, out_dir=out / "dist",
                             resources=resources, kernel=sidecar, env=os.environ)
    if os_name == "linux":
        depends = debian_depends([binaries_dir / PACKAGE, *([sidecar.with_name(f"{KERNEL_NAME}-{triple}")] if sidecar else [])])
        if depends:
            config.setdefault("deb", {})["depends"] = depends
    # Relative paths in the config resolve from its directory, so the
    # generated copy sits beside release.json (git-ignored).
    generated = PACKAGING / ".release.generated.json"
    generated.write_text(json.dumps(config, indent=2) + "\n")
    (out / "receipt.json").write_text(json.dumps({
        "version": version, "target": triple, "formats": formats,
        "resources": [name for name, _ in crates], "kernel": kernel_receipt,
        "makepad_package_dir": added["MAKEPAD_PACKAGE_DIR"],
    }, indent=2) + "\n")
    print(f"==> cargo packager --formats {','.join(formats)}", flush=True)
    try:
        subprocess.run(["cargo", "packager", "--release", "--config", str(generated), "--formats", ",".join(formats)],
                       cwd=PACKAGING, env=env, check=True)
    finally:
        generated.unlink(missing_ok=True)
    print(f"==> packages in {out / 'dist'}; receipt {out / 'receipt.json'}")


if __name__ == "__main__":
    try:
        main()
    except (subprocess.CalledProcessError, RuntimeError, ValueError) as e:
        sys.exit(f"package.py: {e}")
