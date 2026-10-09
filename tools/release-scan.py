#!/usr/bin/env python3
"""Refuse to publish an artifact that carries a private path or name (Python 3.9+).

    python3 tools/release-scan.py target/octosense-package/dist/*
    python3 tools/release-scan.py --extra 'some-private-host' OctoSense.app

Every file is read as bytes, including the files inside the packages a
release ships: `.app` bundles and directories, `.dmg` images (mounted with
hdiutil on macOS), `.deb` (ar + tar), `.AppImage` (trusted `unsquashfs`,
from squashfs-tools), `.zip`, `.tar.*`, and NSIS installers (7-Zip, when installed; the
installed files themselves are scanned before packaging on Windows, since
the release workflow scans the staged payload too). In a directory, a
symbolic link's target is read too. An unreadable container fails the scan
rather than being skipped.

An AppImage is never executed, including to find its payload: its type-2
ELF section table determines the offset. Its own bytes are read except for
its squashfs filesystem, which is read through its extracted files and links instead. That filesystem is
zstd-compressed, and zstd keeps a block's literal bytes back to back, so the
raw bytes hold text that no file has (in octosense 0.1.0's AppImage, a
`/home/` path from a literal run followed by the start of the next run).

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
build paths (see desktop/scripts/package.py). The only exceptions are known
`.local` constants, exact names: the product's own (PRODUCT_LOCAL_NAMES) and
its dependencies' (DEPENDENCY_LOCAL_NAMES), plus the independently verified
public-source seams below and individual synthetic design-example spans
inside complete assets whose SHA-256 matches release-scan-public-assets.json.
"""
import argparse
import getpass
import hashlib
import io
import json
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
from functools import lru_cache

GENERIC_ACCOUNTS = {"runner", "runneradmin", "root", "admin", "administrator", "user", "builder", "build",
                    "vagrant", "ubuntu", "ec2-user", "github", "ci", "localhost"}

# `.local` names that are constants in the code, not machines: the Mail
# service's SMTP EHLO name, and octos' own placeholder addresses (its fleet
# worker's git identity, the solo profile, a test account).
PRODUCT_LOCAL_NAMES = ("octosense.local", "octos.local", "solo.local", "test.local")

# The same in a dependency, one entry per constant, exact names. Each says
# whose constant it is and why it shows up as a `.local` name.
DEPENDENCY_LOCAL_NAMES = (
    # matrix-sdk (Rinx's Matrix client) names media still in its send queue
    # `mxc://send-queue.localhost/<txn>` (LOCAL_MXC_SERVER_NAME,
    # crates/matrix-sdk/src/media.rs). x86-64 code compares a server name
    # with it 16 bytes at a time, so its first 16 bytes are a constant of
    # their own; when the next constant starts with a non-name byte, they
    # read as this name (seen in the Windows build).
    "send-queue.local",
)

# Rinx 4b89097's two public source filenames are adjacent Rust literals in
# the Linux executable. The slash starting the second remapped path makes
# the first filename look like a Linux account: /home/main_desktop_ui.rs/.
# Admit only this proven pair, in the same neutral Cargo checkout/revision;
# a standalone /home/main_desktop_ui.rs/private path must still fail.
RINX_SOURCE_SEAM = re.compile(
    rb"(?P<root>/cargo/git/checkouts/rinx-[0-9a-f]{16}/[0-9a-f]{7,40}/src)"
    rb"/home/main_desktop_ui\.rs(?P<next>(?P=root))"
    rb"/home/tombstone_footer\.rs"
)

# Rinx's pinned public source filename and complete MIME literal are pooled
# in the optimized Linux executable, making the filename look like a home
# directory. Require this exact revision, filename and MIME sequence; only
# the captured apparent home span is exempt, never a neighboring occurrence.
# Public source proof (both at 4b89097d8791a7190d01de1c576979c93df0013d):
# https://github.com/hagency-org/Rinx/blob/4b89097d8791a7190d01de1c576979c93df0013d/src/home/room_screen.rs
# https://github.com/hagency-org/Rinx/blob/4b89097d8791a7190d01de1c576979c93df0013d/src/utils.rs#L295
# There is no trailing MIME boundary: another Rust literal follows directly.
RINX_MIME_SOURCE_SEAM = re.compile(
    rb"/cargo/git/checkouts/rinx-[0-9a-f]{16}/"
    rb"(?:4b89097|4b89097d8791a7190d01de1c576979c93df0013d)/src"
    rb"(?P<home>/home/room_screen\.rsapplication/)octet-stream"
)

# The Windows linker pools these four public Mail literals without NULs:
# `Mail service is not registered`, `attempts`, `send`, `octosense.local`
# (apps/mail/host-service/src/drafts.rs: configured() and add_attempt()).
# The scanner otherwise reads the last word + three literals as one host.
# Require the entire known sentence and exact sequence; the same apparent
# hostname standing alone, or another hostname after the sentence, fails.
MAIL_LITERAL_SEAM = re.compile(
    rb"Mail service is not (?P<host>registeredattemptssendoctosense\.local)"
)

# Public OctosCode 5d0c2a0 UI examples pooled by the macOS linker. chrome.rs
# seed_board2() supplies the two demo workspace paths (2936/2939), next to
# its theme names (2568-2577). screens/browser.rs supplies the path field
# placeholder (733/780), button text and ID (736-738). Only the /home/user/
# match spans inside these complete proven sequences qualify, never those
# paths standing alone or a different field's private value.
OCTOSCODE_PLACEHOLDER_SEAMS = (
    re.compile(rb"(?P<first>/home/user/)src/octoscode-app"
               rb"(?P<second>/home/user/)src/octosSystemSolarizedSlateClaudeCodexLight"),
    re.compile(rb"(?P<first>/home/user/)codeUse this folderb1_br_use"),
    # The same browser.rs:733/737 literals can pool without the translated
    # button label. Require both surrounding widget IDs, not the path alone.
    re.compile(rb"b1_br_path(?P<first>/home/user/)codeb1_br_use"),
)

BASE_PATTERNS = [
    ("macOS user directory", rb"/Users/[^/\s\x00\"']+"),
    ("Windows user directory", rb"[A-Za-z]:[\\/]{1,2}Users[\\/]{1,2}(?!runneradmin[\\/])[^\\/\s\x00\"']+"),
    ("Windows user directory (UTF-16)", rb"(?:[A-Za-z]\x00):\x00(?:[\\/]\x00){1,2}U\x00s\x00e\x00r\x00s\x00"),
    ("Linux home directory", rb"/home/(?!runner/)[a-z_][a-z0-9_.-]*/"),
    ("mDNS .local host name", rb"(?<![A-Za-z0-9_.-])(?!(?:"
     + b"|".join(re.escape(n.encode()) for n in PRODUCT_LOCAL_NAMES + DEPENDENCY_LOCAL_NAMES)
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


@lru_cache(maxsize=1)
def public_asset_matches():
    """Reviewed public examples, indexed by rule and matched bytes' digest.

    This is deliberately not a path/address allowlist. The whole containing
    asset must match, and only the recorded offsets for a base rule qualify.
    Identity and --extra rules never consult this table. Missing or malformed
    metadata is an error, not permission to skip scanning.
    """
    manifest = json.loads(Path(__file__).with_name("release-scan-public-assets.json").read_text())
    index = {}
    for asset in manifest["assets"]:
        size, digest = asset["size"], asset["sha256"]
        if not isinstance(size, int) or not 0 < size <= 1_000_000 or not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise ValueError("invalid reviewed public asset")
        for offset, label, match_digest in asset["matches"]:
            if (not isinstance(offset, int) or not 0 <= offset < size
                    or label not in ("Linux home directory", "private IPv4 address")
                    or not re.fullmatch(r"[0-9a-f]{64}", match_digest)):
                raise ValueError("invalid reviewed public asset match")
            index.setdefault((label, match_digest), []).append((offset, size, digest))
    return index


def is_public_asset_match(data, label, match, verified):
    if label not in ("Linux home directory", "private IPv4 address"):
        return False
    key = (label, hashlib.sha256(match.group(0)).hexdigest())
    for offset, size, digest in public_asset_matches().get(key, ()):
        start = match.start() - offset
        end = start + size
        if start < 0 or end > len(data) or match.end() > end:
            continue
        candidate = (start, end, digest)
        if candidate not in verified:
            verified[candidate] = hashlib.sha256(memoryview(data)[start:end]).hexdigest() == digest
        if verified[candidate]:
            return True
    return False


def scan_bytes(data, where, patterns, findings):
    verified_assets = {}
    for label, regex in patterns:
        source_seams = set()
        if label == "Linux home directory":
            source_seams = {
                (seam.start("next") - len(b"/home/main_desktop_ui.rs"), seam.start("next") + 1)
                for seam in RINX_SOURCE_SEAM.finditer(data)
            }
            source_seams.update(seam.span("home") for seam in RINX_MIME_SOURCE_SEAM.finditer(data))
            source_seams.update(
                seam.span(group)
                for pattern in OCTOSCODE_PLACEHOLDER_SEAMS
                for seam in pattern.finditer(data)
                for group in seam.groupdict()
            )
        elif label == "mDNS .local host name":
            source_seams = {seam.span("host") for seam in MAIL_LITERAL_SEAM.finditer(data)}
        seen = set()
        for m in regex.finditer(data):
            if (m.start(), m.end()) in source_seams:
                continue
            if is_public_asset_match(data, label, m, verified_assets):
                continue
            if m.group(0) in seen:
                continue
            seen.add(m.group(0))
            findings.append(f"{where}: {label}: {mask(m.group(0))}")
            if len(seen) >= 5:
                findings.append(f"{where}: {label}: (more matches not shown)")
                break


def scan_tree(root, label, patterns, findings):
    """Every file's bytes and every symbolic link's target (links are not followed)."""
    root = Path(root)
    count = 0
    for path in sorted(root.rglob("*")):
        where = f"{label}/{path.relative_to(root).as_posix()}"
        if path.is_symlink():
            scan_bytes(os.fsencode(os.readlink(path)), f"{where} (link target)", patterns, findings)
        elif path.is_file():
            count += 1
            scan_bytes(path.read_bytes(), where, patterns, findings)
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


def appimage_filesystem(data):
    """Return the bounded type-2 SquashFS span without running the artifact.

    libappimage's ElfFile::getSize uses the later of the section-table end
    and the last section's end. Read both ELF classes and byte orders, and
    refuse unsupported/invalid layouts instead of guessing an offset.
    https://github.com/AppImageCommunity/libappimage/blob/master/src/libappimage/utils/ElfFile.cpp
    """
    if len(data) < 16 or data[:4] != b"\x7fELF" or data[8:11] != b"AI\x02":
        raise ValueError("not a type-2 ELF AppImage")
    if data[4] not in (1, 2) or data[5] not in (1, 2) or data[6] != 1:
        raise ValueError("unsupported AppImage ELF class, byte order or version")
    order = "little" if data[5] == 1 else "big"
    header, shoff_pos, word, ehsize_pos, entsize_pos, count_pos, section_size, section_pos = (
        (52, 32, 4, 40, 46, 48, 40, 16) if data[4] == 1 else
        (64, 40, 8, 52, 58, 60, 64, 24))

    def integer(pos, size):
        if pos < 0 or pos + size > len(data):
            raise ValueError("truncated AppImage ELF header")
        return int.from_bytes(data[pos:pos + size], order)

    if len(data) < header or integer(ehsize_pos, 2) != header:
        raise ValueError("invalid AppImage ELF header size")
    shoff, entsize, count = integer(shoff_pos, word), integer(entsize_pos, 2), integer(count_pos, 2)
    if shoff < header or entsize != section_size or not count:
        raise ValueError("unsupported AppImage ELF section table")
    table_end = shoff + entsize * count
    if table_end > len(data):
        raise ValueError("truncated AppImage ELF section table")
    last = table_end - entsize
    section_end = integer(last + section_pos, word) + integer(last + section_pos + word, word)
    offset = max(table_end, section_end)
    superblock = data[offset:offset + 96]
    if len(superblock) != 96 or superblock[:4] != b"hsqs":
        raise ValueError("no squashfs filesystem at the AppImage ELF boundary")
    if superblock[28:32] != b"\x04\x00\x00\x00":
        raise ValueError("unsupported AppImage squashfs version")
    used = int.from_bytes(superblock[40:48], "little")
    end = offset + used
    if used < 96 or end > len(data):
        raise ValueError("truncated or invalid AppImage squashfs size")
    return offset, end


def scan_appimage(path, data, patterns, findings):
    """Scan the runtime, trusted-tool-extracted filesystem and trailing bytes.
    Neither the input runtime nor any extracted program is executed."""
    name = path.name
    offset, end = appimage_filesystem(data)
    extractor = shutil.which("unsquashfs")
    if not extractor:
        raise RuntimeError(f"{name}: install squashfs-tools to scan AppImages (unsquashfs missing)")
    scan_bytes(data[:offset], f"{name} (runtime)", patterns, findings)
    scan_bytes(data[end:], f"{name} (after its filesystem)", patterns, findings)
    with tempfile.TemporaryDirectory() as temp:
        # Extract precisely the bytes checked above, even if the caller's
        # original path is replaced meanwhile. The copy is never executable.
        copy = Path(temp) / "input.AppImage"
        copy.write_bytes(data)
        destination = Path(temp) / "squashfs-root"
        subprocess.run([extractor, "-strict-errors", "-no-progress", "-no-xattrs",
                        "-processors", "2", "-offset", str(offset), "-dest", str(destination), str(copy)],
                       check=True, capture_output=True, env={"PATH": os.defpath, "LC_ALL": "C"})
        if not destination.is_dir() or destination.is_symlink():
            raise RuntimeError(f"{name}: unsquashfs did not produce a filesystem directory")
        return 1 + scan_tree(destination, name, patterns, findings)


def scan_artifact(path, patterns, findings, dmg_bytes_only=False):
    """Scan one artifact and what it contains; returns how many files.
    `dmg_bytes_only`: a .dmg's own bytes only, where it cannot be mounted
    (the release job on Linux; its contents were scanned on macOS)."""
    path = Path(path)
    name = path.name
    if path.is_dir():
        return scan_tree(path, name, patterns, findings)
    data = path.read_bytes()
    lower = name.lower()
    if lower.endswith(".appimage"):
        return scan_appimage(path, data, patterns, findings)
    scan_bytes(data, name, patterns, findings)
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
