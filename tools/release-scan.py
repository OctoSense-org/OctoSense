#!/usr/bin/env python3
"""Refuse to publish an artifact that carries a private path or name (Python 3.9+).

    python3 tools/release-scan.py target/octosense-package/dist/*
    python3 tools/release-scan.py --extra 'some-private-host' OctoSense.app

Every file is read as bytes, including the files inside the packages a
release ships: `.app` bundles and directories, `.dmg` images (mounted with
hdiutil on macOS), `.deb` (ar + tar), `.AppImage` (`--appimage-extract` on
Linux), `.zip`, `.tar.*`, and NSIS installers (7-Zip, when installed; the
installed files themselves are scanned before packaging on Windows, since
the release workflow scans the staged payload too). An unreadable container
fails the scan rather than being skipped.

It fails on:

- a macOS user directory (`/Users/<anyone>`), a Windows one other than a
  CI runner's (`C:\\Users\\runneradmin`, which the prebuilt NSIS plugin
  cargo-packager bundles carries; also UTF-16), and a Linux home other than
  a CI runner's (`/home/runner`);
- a `<name>.local` host name (mDNS: a build machine on a private network;
  one label, as mDNS names are, so words glued together in a binary's
  string data are not read as a dotted name);
- a private IPv4 address (10/8, 172.16/12, 192.168/16);
- the name of the account and host running the scan (skipped for generic CI
  accounts), and every regular expression in `--extra` or the
  `RELEASE_SCAN_EXTRA` environment variable (comma-separated; the release
  workflow feeds it from a secret, so the patterns themselves stay private).

Exit status 1 with one line per finding (the match is shown masked), 0 when
clean. Findings are about the build, not the code: fix them with neutral
build paths (see desktop/scripts/package.py). The only exceptions are the
product's own `.local` constants (PRODUCT_LOCAL_NAMES), exact names.
"""
import argparse
import getpass
import io
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import sys
import tarfile
import tempfile
import zipfile

GENERIC_ACCOUNTS = {"runner", "runneradmin", "root", "admin", "administrator", "user", "builder", "build",
                    "vagrant", "ubuntu", "ec2-user", "github", "ci", "localhost"}

# `.local` names that are constants in the code, not machines: the Mail
# service's SMTP EHLO name, and octos' own placeholder addresses (its fleet
# worker's git identity, the solo profile, a test account).
PRODUCT_LOCAL_NAMES = ("octosense.local", "octos.local", "solo.local", "test.local")

BASE_PATTERNS = [
    ("macOS user directory", rb"/Users/[^/\s\x00\"']+"),
    ("Windows user directory", rb"[A-Za-z]:[\\/]{1,2}Users[\\/]{1,2}(?!runneradmin[\\/])[^\\/\s\x00\"']+"),
    ("Windows user directory (UTF-16)", rb"(?:[A-Za-z]\x00):\x00(?:[\\/]\x00){1,2}U\x00s\x00e\x00r\x00s\x00"),
    ("Linux home directory", rb"/home/(?!runner/)[a-z_][a-z0-9_.-]*/"),
    ("mDNS .local host name", rb"(?<![A-Za-z0-9_.-])(?!(?:" + b"|".join(re.escape(n.encode()) for n in PRODUCT_LOCAL_NAMES)
     + rb")(?![A-Za-z0-9_-]))[A-Za-z0-9][A-Za-z0-9-]*\.local(?![A-Za-z0-9_-])"
     # ~/.local/bin and friends glued to a neighbouring string are paths.
     # (Rust packs literals back to back, so the next literal may follow.)
     rb"(?!/(?:bin|share|lib|state|include))"),
    ("private IPv4 address", rb"(?<![0-9.])(?:10\.\d{1,3}|172\.(?:1[6-9]|2\d|3[01])|192\.168)\.\d{1,3}\.\d{1,3}(?![0-9.])"),
]


def identity_patterns(user=None, host=None):
    """The scanning machine's own account and host names, unless generic."""
    found = []
    user = (user if user is not None else _safe(getpass.getuser) or "").strip()
    host = (host if host is not None else _safe(socket.gethostname) or "").split(".")[0].strip()
    if len(user) >= 3 and user.lower() not in GENERIC_ACCOUNTS:
        found.append(("the scanning account's name", rb"(?<![A-Za-z0-9])" + re.escape(user.encode()) + rb"(?![A-Za-z0-9])"))
    if len(host) >= 4 and host.lower() not in GENERIC_ACCOUNTS and not host.lower().startswith(("fv-az", "runner", "mac-", "ip-")):
        found.append(("the scanning host's name", rb"(?<![A-Za-z0-9])" + re.escape(host.encode()) + rb"(?![A-Za-z0-9])"))
    return found


def _safe(fn):
    try:
        return fn()
    except Exception:
        return None


def compile_patterns(extra=(), identity=True):
    patterns = list(BASE_PATTERNS)
    if identity:
        patterns += identity_patterns()
    for i, pattern in enumerate(p for p in extra if p.strip()):
        patterns.append((f"--extra pattern #{i + 1}", pattern.strip().encode()))
    return [(label, re.compile(p, re.IGNORECASE if label.startswith("the scanning") else 0)) for label, p in patterns]


def mask(match):
    text = match.decode("utf-8", "replace").replace("\x00", "")
    return text if len(text) <= 4 else text[:3] + "*" * min(len(text) - 3, 12) + f" ({len(text)} chars)"


def scan_bytes(data, where, patterns, findings):
    for label, regex in patterns:
        seen = set()
        for m in regex.finditer(data):
            if m.group(0) in seen:
                continue
            seen.add(m.group(0))
            findings.append(f"{where}: {label}: {mask(m.group(0))}")
            if len(seen) >= 5:
                findings.append(f"{where}: {label}: (more matches not shown)")
                break


def scan_tree(root, label, patterns, findings):
    root = Path(root)
    count = 0
    for path in sorted(root.rglob("*")):
        if path.is_file() and not path.is_symlink():
            count += 1
            scan_bytes(path.read_bytes(), f"{label}/{path.relative_to(root).as_posix()}", patterns, findings)
    return count


def scan_tar(data, label, patterns, findings):
    with tarfile.open(fileobj=io.BytesIO(data)) as tar:
        for member in tar.getmembers():
            if member.isfile():
                scan_bytes(tar.extractfile(member).read(), f"{label}!{member.name}", patterns, findings)


def ar_members(data):
    """The members of a Unix ar archive (a .deb)."""
    if not data.startswith(b"!<arch>\n"):
        raise ValueError("not an ar archive")
    pos = 8
    while pos + 60 <= len(data):
        header = data[pos:pos + 60]
        name = header[:16].decode().strip().rstrip("/")
        size = int(header[48:58].decode().strip())
        yield name, data[pos + 60:pos + 60 + size]
        pos += 60 + size + (size % 2)


def scan_artifact(path, patterns, findings, dmg_bytes_only=False):
    """Scan one artifact and what it contains; returns how many files.
    `dmg_bytes_only`: a .dmg's own bytes only, where it cannot be mounted
    (the release job on Linux; its contents were scanned on macOS)."""
    path = Path(path)
    name = path.name
    if path.is_dir():
        return scan_tree(path, name, patterns, findings)
    data = path.read_bytes()
    scan_bytes(data, name, patterns, findings)
    lower = name.lower()
    if lower.endswith(".dmg"):
        if dmg_bytes_only:
            return 1
        if sys.platform != "darwin":
            raise RuntimeError(f"{name}: a .dmg can only be opened on macOS")
        with tempfile.TemporaryDirectory() as mount:
            # An image with a license agreement waits for "Y" on stdin.
            subprocess.run(["hdiutil", "attach", "-readonly", "-nobrowse", "-noautoopen", "-mountpoint", mount, str(path)],
                           check=True, capture_output=True, input=b"Y\n", env={**os.environ, "PAGER": "cat"})
            try:
                return 1 + scan_tree(mount, name, patterns, findings)
            finally:
                subprocess.run(["hdiutil", "detach", mount, "-force"], capture_output=True)
    if lower.endswith(".deb"):
        count = 1
        for member, blob in ar_members(data):
            if member.startswith(("data.tar", "control.tar")):
                if member.endswith(".zst"):
                    raise RuntimeError(f"{name}: {member} is zstd; install dpkg-deb or repackage")
                scan_tar(blob, f"{name}!{member}", patterns, findings)
                count += 1
        return count
    if lower.endswith(".appimage"):
        if not sys.platform.startswith("linux"):
            raise RuntimeError(f"{name}: an AppImage can only be extracted on Linux")
        with tempfile.TemporaryDirectory() as temp:
            copy = Path(temp) / name
            shutil.copy2(path, copy)
            copy.chmod(0o755)
            subprocess.run([str(copy), "--appimage-extract"], cwd=temp, check=True, capture_output=True)
            return 1 + scan_tree(Path(temp) / "squashfs-root", name, patterns, findings)
    if lower.endswith(".zip"):
        with zipfile.ZipFile(path) as z:
            for info in z.infolist():
                if not info.is_dir():
                    scan_bytes(z.read(info), f"{name}!{info.filename}", patterns, findings)
        return 1
    if re.search(r"\.tar(\.(gz|xz|bz2))?$|\.tgz$", lower):
        scan_tar(data, name, patterns, findings)
        return 1
    if lower.endswith(".exe") and data[:2] == b"MZ" and b"Nullsoft" in data:
        seven = shutil.which("7z") or shutil.which("7z.exe")
        if not seven:
            print(f"release-scan: {name}: no 7-Zip; scanned the installer's bytes only", file=sys.stderr)
            return 1
        with tempfile.TemporaryDirectory() as temp:
            subprocess.run([seven, "x", "-y", f"-o{temp}", str(path)], check=True, capture_output=True)
            return 1 + scan_tree(temp, name, patterns, findings)
    return 1


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("artifacts", nargs="+", type=Path)
    p.add_argument("--extra", action="append", default=[], help="Another regular expression that must not appear")
    p.add_argument("--no-identity", action="store_true", help="Do not look for this machine's account and host names")
    p.add_argument("--dmg-bytes-only", action="store_true",
                   help="Scan a .dmg's bytes without mounting it (off macOS, after its contents were scanned on macOS)")
    args = p.parse_args(argv)
    extra = args.extra + os.environ.get("RELEASE_SCAN_EXTRA", "").split(",")
    patterns = compile_patterns(extra, identity=not args.no_identity)
    findings, files = [], 0
    for artifact in args.artifacts:
        if not artifact.exists():
            p.error(f"no such artifact: {artifact}")
        try:
            files += scan_artifact(artifact, patterns, findings, args.dmg_bytes_only)
        except (RuntimeError, ValueError, OSError, subprocess.CalledProcessError, tarfile.TarError, zipfile.BadZipFile) as e:
            findings.append(f"{artifact.name}: could not be opened for scanning: {e}")
    for line in findings:
        print(f"release-scan: {line}")
    if findings:
        print(f"release-scan: FAILED: {len(findings)} finding(s) in {len(args.artifacts)} artifact(s)", file=sys.stderr)
        return 1
    print(f"release-scan: clean: {len(args.artifacts)} artifact(s), {files} file(s), {len(patterns)} pattern(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
