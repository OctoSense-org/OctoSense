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
   out and built for this machine by `tools/kernel-artifact.py --host` (with
   the same remapped paths), then staged by its `stage` (which refuses a
   binary whose `--version` is not the pinned revision), or `--kernel
   <path>`. It ships beside the executable as `octos-kernel[.exe]` (in a
   `.app`, `Contents/MacOS`), and its receipt `octos-kernel.json` with the
   resources; the kernel service runs it only when that receipt names the
   pinned revision and the kernel's SHA-256 (`crates/kernel/src/launch.rs`).
   `--no-kernel` ships none: the app then runs without an assistant (AI
   providers are still saved) and says why.
4. **Package** with cargo-packager from `desktop/packaging/release.json`:
   macOS `.app` and `.dmg`, Windows NSIS `.exe`, Linux `.deb` and `.AppImage`.

Where the resources land, and the `MAKEPAD_PACKAGE_DIR` that finds them:

| Format | Resources | `MAKEPAD_PACKAGE_DIR` |
| --- | --- | --- |
| `.app`/`.dmg` | `OctoSense.app/Contents/Resources/` | `.` (+ `apple_bundle`: NSBundle's resource path) |
| NSIS `.exe` | the install directory, beside `octosense.exe` | `.` (next to the executable) |
| `.deb`, `.AppImage` | `usr/lib/octosense/` | `../lib/octosense` (from `usr/bin/octosense`) |

The packages are always **unsigned**, and this script needs no secret: it
removes cargo-packager's signing variables (`APPLE_SIGNING_IDENTITY`,
`APPLE_CERTIFICATE`, ...) from the build's environment, so a stray one never
signs, and a build never sees a signing key. Signing and notarization are a
separate step on the finished packages (the release workflow's `sign-*`
jobs, `.github/workflows/release-desktop.yml`), which never runs Cargo.

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
RECEIPT_NAME = "octos-kernel.json"
# cargo-packager signs when these are set; the build never does (see above).
SIGNING_ENV = ("APPLE_SIGNING_IDENTITY", "APPLE_CERTIFICATE", "APPLE_CERTIFICATE_PASSWORD", "APPLE_API_KEY",
               "APPLE_API_ISSUER", "APPLE_API_KEY_PATH", "APPLE_API_KEY_P8", "APPLE_ID", "APPLE_PASSWORD", "APPLE_TEAM_ID",
               "WINDOWS_CERTIFICATE", "WINDOWS_CERTIFICATE_PASSWORD", "WINDOWS_CERTIFICATE_THUMBPRINT")
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


def kernel_tool():
    """tools/kernel-artifact.py: the one place that knows the pinned kernel."""
    spec = importlib.util.spec_from_file_location("kernel_artifact", ROOT / "tools/kernel-artifact.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def appimage_tool():
    spec = importlib.util.spec_from_file_location("appimage_receipt", DESKTOP / "scripts/appimage_receipt.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def build_kernel(env, work, offline=False):
    """The octos kernel at the workspace's revision, built for this machine
    (`kernel-artifact.py --host`'s steps) with the release environment: paths
    remapped, no debug info, no Makepad packaging variables."""
    tool = kernel_tool()
    steps, binary, source = tool.kernel_plan(host=True, work=work, offline=offline)
    env = {k: v for k, v in env.items() if not k.startswith("MAKEPAD") and k != "CARGO_BUILD_TARGET"}
    for cwd, command in steps:
        Path(cwd).mkdir(parents=True, exist_ok=True)
        subprocess.run(command, cwd=cwd, env=env, check=True)
    return binary, source


def stage_kernel(kernel, source, out, resources):
    """Stage (and check) the kernel with `kernel-artifact.py`'s `stage`, put
    its receipt with the resources (a `.app`'s `Contents/Resources`, the
    Windows install dir beside the kernel, Linux `usr/lib/octosense`: where
    the kernel service looks), and name a copy the way cargo-packager's
    `externalBinaries` expects (`<name>-<triple>[.exe]`)."""
    tool = kernel_tool()
    receipt = tool.stage(kernel, out / "kernel", tool.octos_revision(), source)
    shutil.copy2(out / "kernel" / tool.RECEIPT_NAME, resources / RECEIPT_NAME)
    suffix = ".exe" if host_os() == "windows" else ""
    sidecar = out / "bin" / KERNEL_NAME
    sidecar.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(out / "kernel" / tool.STAGED_NAME, sidecar.with_name(f"{KERNEL_NAME}-{host_triple()}{suffix}"))
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


def packager_config(base, *, version, binaries_dir, out_dir, resources, kernel):
    """release.json with this build's version, paths and kernel (never
    signing: see the module docstring)."""
    config = json.loads(json.dumps(base))
    config["version"] = version
    config["binariesDir"] = str(binaries_dir)
    config["outDir"] = str(out_dir)
    config["resources"] = [{"src": str(resources), "target": "."}]
    if kernel:
        # cargo-packager copies `<path>-<target triple>[.exe]` beside the
        # main binary as `<name>[.exe]`.
        config["externalBinaries"] = [str(kernel)]
    return config


def build_env(added, base_env):
    """The build's whole environment: this script's additions, RUSTFLAGS
    folded into CARGO_ENCODED_RUSTFLAGS, and no signing variable."""
    env = {**base_env, **added}
    env.pop("RUSTFLAGS", None)
    for name in SIGNING_ENV:
        env.pop(name, None)
    return env


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
    env = build_env(added, os.environ)
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
            binary, source = args.kernel.resolve(), "prebuilt"
        else:
            print("==> building the octos kernel for this machine (tools/kernel-artifact.py --host)", flush=True)
            binary, source = build_kernel(env, out / "kernel-build", args.offline)
        sidecar, kernel_receipt = stage_kernel(binary, source, out, resources)
        print(f"    {KERNEL_NAME}: {kernel_receipt['version']} sha256 {kernel_receipt['sha256'][:16]}")
    else:
        print("==> no kernel: this package runs without an assistant", flush=True)

    base = json.loads((PACKAGING / "release.json").read_text())
    binaries_dir = ROOT / "target" / "release"
    config = packager_config(base, version=version, binaries_dir=binaries_dir, out_dir=out / "dist",
                             resources=resources, kernel=sidecar)
    if os_name == "linux":
        depends = debian_depends([binaries_dir / PACKAGE, *([sidecar.with_name(f"{KERNEL_NAME}-{triple}")] if sidecar else [])])
        if depends:
            # GTK/WebKit are loaded dynamically and do not appear in shlibdeps.
            # Preserve explicit runtime dependencies from release.json.
            deb = config.setdefault("deb", {})
            deb["depends"] = list(dict.fromkeys([*deb.get("depends", []), *depends]))
    # Relative paths in the config resolve from its directory, so the
    # generated copy sits beside release.json (git-ignored).
    generated = PACKAGING / ".release.generated.json"
    generated.write_text(json.dumps(config, indent=2) + "\n")
    receipt_path = out / "receipt.json"
    receipt_path.unlink(missing_ok=True)  # A failed restage must not leave an old success receipt.
    receipt = {
        "version": version, "target": triple, "formats": formats,
        "resources": [name for name, _ in crates], "kernel": kernel_receipt,
        "makepad_package_dir": added["MAKEPAD_PACKAGE_DIR"],
    }
    print(f"==> cargo packager --formats {','.join(formats)}", flush=True)
    try:
        subprocess.run(["cargo", "packager", "--release", "--config", str(generated), "--formats", ",".join(formats)],
                       cwd=PACKAGING, env=env, check=True)
    finally:
        generated.unlink(missing_ok=True)
    if os_name == "linux" and kernel_receipt:
        # linuxdeploy rewrites the AppImage sidecar's RUNPATH after resources
        # are copied. Finalize only that format; the DEB retains staged bytes.
        receipt["kernel_scope"] = "staged_before_packaging"
        receipt["packages"] = appimage_tool().finalize_linux(
            out / "dist", formats, kernel_receipt,
            (out / "kernel" / KERNEL_NAME).read_bytes())
    receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"==> packages in {out / 'dist'}; receipt {out / 'receipt.json'}")


if __name__ == "__main__":
    try:
        main()
    except (subprocess.CalledProcessError, RuntimeError, ValueError) as e:
        sys.exit(f"package.py: {e}")
